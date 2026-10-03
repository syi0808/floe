//! Day's inverse acquisition port: complete source inventory and Access policy.
use std::sync::Arc;
use floe_access::{AccessClock, CalendarReadLimits, ProductCalendarReadAuthority, ProductCalendarReadRequest, ProductSourceAuthority, ProductSourceObservation};
use floe_connections::{ConnectionsRepository, SourceConnection, SourceState};
use floe_context_contract::{CalendarProvider, ConnectorId, GrantSourceBinding};
use floe_day::{CalendarAcquisition, CalendarAcquisitionPort, CalendarFailure, CalendarRefreshError, CalendarRefreshRequest, CalendarResourceOutcome, CalendarSelection, CalendarSourceOutcome, CalendarSourceVersion, DayRefreshFailure, MAX_REFRESH_BYTES, MAX_REFRESH_CALENDARS, MAX_REFRESH_RECORDS, MAX_REFRESH_SOURCES};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor, Revision};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use crate::CalendarProductTransport;

/// Immutable product source handles. Construct before an encrypted generation;
/// only the transport's explicit Gateway lease depends on Vault readiness.
pub struct ContextCore {
    sources: Arc<dyn ConnectionsRepository>,
    calendar_transport: Arc<dyn CalendarProductTransport>,
    product_access: Arc<ProductCalendarReadAuthority>,
    clock: Arc<dyn AccessClock>,
}
impl ContextCore {
    pub fn new(sources: Arc<dyn ConnectionsRepository>, calendar_transport: Arc<dyn CalendarProductTransport>, clock: Arc<dyn AccessClock>) -> Self {
        let source_authority = Arc::new(ConfiguredCalendarAuthority { sources: sources.clone(), transport: calendar_transport.clone() });
        let product_access = Arc::new(ProductCalendarReadAuthority::new(source_authority, clock.clone()));
        Self { sources, calendar_transport, product_access, clock }
    }
    async fn inventory(&self, actor: &OwnerActor, scope: &ExecutionScope) -> Result<Vec<SourceConnection>, DayRefreshFailure> {
        actor.validate().map_err(|_| DayRefreshFailure::InvalidAcquisition)?; check(scope)?;
        let mut sources = Vec::new();
        for connector in ["calendar.event_kit", "calendar.google", "calendar.microsoft"] {
            let connector = ConnectorId::try_new(connector).map_err(|_| DayRefreshFailure::InvalidAcquisition)?;
            let current = self.sources.list_current(actor.person_id, &connector).await.map_err(|_| DayRefreshFailure::StorageUnavailable)?;
            for source in current {
                source.validate().map_err(|_| DayRefreshFailure::InvalidAcquisition)?;
                if source.person_id() != actor.person_id || source.connector_id() != &connector || source.state() == SourceState::Disconnected { return Err(DayRefreshFailure::InvalidAcquisition); }
                sources.push(source);
                if sources.len() > MAX_REFRESH_SOURCES { return Err(DayRefreshFailure::BudgetExceeded); }
            }
        }
        sources.sort_by(|left, right| left.connection_id().cmp(right.connection_id()));
        if sources.windows(2).any(|pair| pair[0].connection_id() == pair[1].connection_id()) || sources.iter().map(|source| source.resources().len()).sum::<usize>() > MAX_REFRESH_CALENDARS { return Err(DayRefreshFailure::BudgetExceeded); }
        check(scope)?; Ok(sources)
    }
}

pub struct ContextCalendarAcquisition { core: Arc<ContextCore> }
impl ContextCalendarAcquisition { pub fn new(core: Arc<ContextCore>) -> Self { Self { core } } }
impl CalendarAcquisitionPort for ContextCalendarAcquisition {
    fn acquire<'a>(&'a self, request: CalendarRefreshRequest, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<CalendarAcquisition, CalendarRefreshError>> {
        Box::pin(async move {
            request.actor.validate().map_err(|_| DayRefreshFailure::InvalidAcquisition)?;
            if request.refresh_operation_id.is_nil() { return Err(DayRefreshFailure::InvalidAcquisition); }
            let range = request.query.range().map_err(|_| DayRefreshFailure::InvalidAcquisition)?;
            let (starts_at, ends_at) = floe_day::range_bounds(&range).map_err(|_| DayRefreshFailure::InvalidAcquisition)?;
            let sources = self.core.inventory(&request.actor, scope).await?;
            let inventory = sources.iter().map(calendar_source_version).collect::<Result<Vec<_>, _>>()?;
            let mut outcomes = Vec::with_capacity(sources.len()); let mut used_records = 0usize; let mut used_bytes = 0usize;
            for (source, version) in sources.iter().zip(&inventory) {
                check(scope)?;
                let unavailable = |reason| CalendarSourceOutcome::Unavailable { source: version.clone(), reason, observed_at: self.core.clock.now() };
                if source.state() != SourceState::Ready || source.resources().is_empty() { outcomes.push(unavailable(CalendarFailure::CalendarUnavailable)); continue; }
                if self.core.sources.source_is_fenced(request.actor.person_id, source.connection_id()).await.map_err(|_| DayRefreshFailure::StorageUnavailable)? { outcomes.push(unavailable(CalendarFailure::SourceFenced)); continue; }
                let remaining_records = MAX_REFRESH_RECORDS.checked_sub(used_records).filter(|remaining| *remaining > 0).ok_or(DayRefreshFailure::BudgetExceeded)?;
                let remaining_bytes = MAX_REFRESH_BYTES.checked_sub(used_bytes).filter(|remaining| *remaining > 0).ok_or(DayRefreshFailure::BudgetExceeded)?;
                let read_request = ProductCalendarReadRequest { actor: request.actor.clone(), refresh_operation_id: request.refresh_operation_id, read_operation_id: Uuid::new_v4(), source: version.source.clone(), range_start: starts_at, range_end: ends_at, limits: CalendarReadLimits { max_records: remaining_records as u32, max_bytes: remaining_bytes as u32, max_page_records: remaining_records.min(128) as u32, max_page_bytes: remaining_bytes.min(1024 * 1024) as u32 } };
                let permit = match self.core.product_access.admit(read_request, scope).await { Ok(value) => value, Err(failure) => { outcomes.push(unavailable(source_failure(failure))); continue; } };
                let fence = match self.core.product_access.revalidate_for_dispatch(&permit, scope).await { Ok(value) => value, Err(failure) => { outcomes.push(unavailable(source_failure(failure))); continue; } };
                let result = self.core.calendar_transport.acquire(fence, scope).await.map_err(refresh_failure)?;
                if result.batches.len() != version.calendars.len() || result.batches.iter().zip(&version.calendars).any(|(batch, calendar)| batch.calendar_id() != calendar.calendar_id) { return Err(DayRefreshFailure::InvalidAcquisition); }
                let encoded = serde_json::to_vec(&result.batches).map_err(|_| DayRefreshFailure::InvalidAcquisition)?;
                let count = result.batches.iter().map(|batch| match batch { CalendarResourceOutcome::Complete { records, .. } => records.len(), CalendarResourceOutcome::Failed { .. } => 0 }).sum::<usize>();
                let digest: [u8; 32] = Sha256::digest(&encoded).into();
                if result.binding.payload_digest != digest || result.binding.byte_count as usize != encoded.len() || result.binding.record_count as usize != count || (result.consumed_records as usize) < count || (result.consumed_bytes as usize) < encoded.len() || result.consumed_records > permit.request().limits.max_records || result.consumed_bytes > permit.request().limits.max_bytes { return Err(DayRefreshFailure::InvalidAcquisition); }
                used_records = used_records.checked_add(result.consumed_records as usize).filter(|count| *count <= MAX_REFRESH_RECORDS).ok_or(DayRefreshFailure::BudgetExceeded)?;
                used_bytes = used_bytes.checked_add(result.consumed_bytes as usize).filter(|bytes| *bytes <= MAX_REFRESH_BYTES).ok_or(DayRefreshFailure::BudgetExceeded)?;
                let released = match self.core.product_access.release(permit, result.binding, scope).await { Ok(value) => value, Err(AgentFailure::StaleContext | AgentFailure::Conflict | AgentFailure::AccessReviewRequired | AgentFailure::PolicyDenied | AgentFailure::CapabilityDenied) => return Err(DayRefreshFailure::SourceChanged), Err(failure) => { outcomes.push(unavailable(source_failure(failure))); continue; } };
                outcomes.push(CalendarSourceOutcome::Acquired { source: version.clone(), read_operation_id: released.request().read_operation_id, observed_at: released.binding().observed_at, expires_at: released.binding().expires_at, batches: result.batches });
            }
            let current = self.core.inventory(&request.actor, scope).await?.iter().map(calendar_source_version).collect::<Result<Vec<_>, _>>()?;
            if current != inventory { return Err(DayRefreshFailure::SourceChanged); }
            for result in &outcomes { if result.has_success() && self.core.sources.source_is_fenced(request.actor.person_id, &result.source().source.connection_id()).await.map_err(|_| DayRefreshFailure::StorageUnavailable)? { return Err(DayRefreshFailure::SourceChanged); } }
            let acquisition = CalendarAcquisition { refresh_operation_id: request.refresh_operation_id, person_id: request.actor.person_id, device_id: request.actor.device_id.clone(), range, inventory, sources: outcomes, completed_at: self.core.clock.now() };
            acquisition.validate(&request, self.core.clock.now()).map_err(|_| DayRefreshFailure::InvalidAcquisition)?; check(scope)?; Ok(acquisition)
        })
    }
}

struct ConfiguredCalendarAuthority { sources: Arc<dyn ConnectionsRepository>, transport: Arc<dyn CalendarProductTransport> }
impl ProductSourceAuthority for ConfiguredCalendarAuthority {
    fn observe_current<'a>(&'a self, actor: &'a OwnerActor, binding: &'a GrantSourceBinding, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<ProductSourceObservation, AgentFailure>> {
        Box::pin(async move {
            actor.validate()?; if binding.person_id() != actor.person_id { return Err(AgentFailure::PolicyDenied); }
            let connection_id = binding.connection_id();
            let source = self.sources.load(actor.person_id, &connection_id).await.map_err(|_| AgentFailure::StorageUnavailable)?.ok_or(AgentFailure::CapabilityUnavailable)?;
            if !source.is_serving() || source.resources().is_empty() || source_binding(&source)? != *binding || self.sources.source_is_fenced(actor.person_id, &connection_id).await.map_err(|_| AgentFailure::StorageUnavailable)? { return Err(AgentFailure::PolicyDenied); }
            let observation = self.transport.observe(actor, &source, scope).await?;
            let mut resources = source.resources().iter().map(|resource| resource.handle().clone()).collect::<Vec<_>>(); resources.sort();
            if observation.expectation.source != *binding || observation.expectation.revision != Some(source.revision()) || observation.expectation.authority != source.source_authority() || observation.expectation.physical_resources != resources || source.native_subject_fingerprint().is_some_and(|subject| observation.expectation.subject_fingerprint != subject) { return Err(AgentFailure::StaleContext); }
            let after = self.sources.load(actor.person_id, &connection_id).await.map_err(|_| AgentFailure::StorageUnavailable)?.ok_or(AgentFailure::StaleContext)?;
            if after != source || self.sources.source_is_fenced(actor.person_id, &connection_id).await.map_err(|_| AgentFailure::StorageUnavailable)? { return Err(AgentFailure::StaleContext); }
            Ok(observation)
        })
    }
}
fn source_binding(source: &SourceConnection) -> Result<GrantSourceBinding, AgentFailure> { GrantSourceBinding::try_new(source.person_id(), source.connection_id().clone(), source.connector_id().clone(), source.execution_owner_id().clone()).map_err(|_| AgentFailure::InvalidInput) }
fn calendar_source_version(source: &SourceConnection) -> Result<CalendarSourceVersion, DayRefreshFailure> {
    let provider = match source.connector_id().as_str() { "calendar.event_kit" => CalendarProvider::EventKit, "calendar.google" => CalendarProvider::Google, "calendar.microsoft" => CalendarProvider::Microsoft, _ => return Err(DayRefreshFailure::InvalidAcquisition) };
    let mut calendars = source.resources().iter().map(|resource| CalendarSelection { calendar_id: resource.handle().as_str().to_owned(), calendar_name: resource.label().to_owned() }).collect::<Vec<_>>(); calendars.sort_by(|left, right| left.calendar_id.cmp(&right.calendar_id));
    let version = CalendarSourceVersion { source: source_binding(source).map_err(|_| DayRefreshFailure::InvalidAcquisition)?, provider, revision: Revision(source.revision()), authority: source.source_authority(), configuration_digest: Sha256::digest(serde_json::to_vec(source).map_err(|_| DayRefreshFailure::InvalidAcquisition)?).into(), calendars };
    version.validate(source.person_id()).map_err(|_| DayRefreshFailure::InvalidAcquisition)?; Ok(version)
}
fn check(scope: &ExecutionScope) -> Result<(), DayRefreshFailure> { if scope.cancellation().is_cancelled() { Err(DayRefreshFailure::Cancelled) } else if tokio::time::Instant::now() >= scope.deadline() { Err(DayRefreshFailure::DeadlineExceeded) } else { Ok(()) } }
fn source_failure(failure: AgentFailure) -> CalendarFailure { match failure { AgentFailure::VaultLocked => CalendarFailure::VaultLocked, AgentFailure::CapabilityDenied | AgentFailure::PolicyDenied => CalendarFailure::PermissionDenied, AgentFailure::StaleContext | AgentFailure::AccessReviewRequired | AgentFailure::Conflict => CalendarFailure::SourceChanged, AgentFailure::BudgetExceeded => CalendarFailure::BudgetExceeded, AgentFailure::DeadlineExceeded => CalendarFailure::DeadlineExceeded, AgentFailure::Cancelled | AgentFailure::Interrupted => CalendarFailure::Cancelled, _ => CalendarFailure::ProviderUnavailable } }

fn refresh_failure(failure: AgentFailure) -> DayRefreshFailure { match failure { AgentFailure::VaultLocked => DayRefreshFailure::VaultLocked, AgentFailure::BudgetExceeded => DayRefreshFailure::BudgetExceeded, AgentFailure::DeadlineExceeded | AgentFailure::ServerModelTimeout => DayRefreshFailure::DeadlineExceeded, AgentFailure::Cancelled => DayRefreshFailure::Cancelled, AgentFailure::Interrupted => DayRefreshFailure::HostInterrupted, AgentFailure::StaleContext | AgentFailure::Conflict | AgentFailure::AccessReviewRequired => DayRefreshFailure::SourceChanged, AgentFailure::PolicyDenied | AgentFailure::CapabilityDenied => DayRefreshFailure::PermissionDenied, AgentFailure::StorageUnavailable | AgentFailure::VaultUnavailable => DayRefreshFailure::StorageUnavailable, _ => DayRefreshFailure::Unavailable } }
