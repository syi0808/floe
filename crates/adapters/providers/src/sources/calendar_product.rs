//! Product Calendar acquisition over the existing native broker and private
//! Gateway mirror protocol. Product policy remains in Access.
use std::{collections::HashSet, sync::Arc, time::Duration};

use chrono::{DateTime, NaiveDate, Utc};
use floe_access::{
    ProductCalendarDispatchFence, ProductCalendarPermission, ProductCalendarReadPermit,
    ProductCalendarResultBinding, ProductSourceObservation, SourceExpectation,
};
use floe_connections::SourceConnection;
use floe_context::{CalendarProductReadResult, CalendarProductTransport};
use floe_context_contract::{CalendarProvider, GrantSourceBinding, ResourceHandle};
use floe_day::{
    AllDaySchedule, CalendarExternalRevision, CalendarFailure, CalendarRecord,
    CalendarResourceOutcome, EventSchedule, TimedSchedule,
};
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};
use floe_native::{
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult, CalendarBroker,
    NativeCalendarBatch, NativeCalendarFailure,
};
use sha2::{Digest, Sha256};
use tokio::time::Instant;
use uuid::Uuid;

use crate::gateway::{ProductGatewayLeaseRegistry, calendar_mirror::GatewayCalendarMirrorClient};

const MAX_NATIVE_CALL: Duration = Duration::from_secs(30);
const MAX_CALENDAR_RESOURCES: usize = 256;
const MAX_CALENDAR_RECORDS: usize = 10_000;
const MAX_TITLE_BYTES: usize = 4096;

/// Provider-side adapter injected into Context and the Access source-fact
/// port. Context retains its independent inventory and commit-fence checks.
pub struct CalendarProductAdapter {
    broker: Arc<CalendarBroker>,
    gateway: Arc<ProductGatewayLeaseRegistry>,
}

impl CalendarProductAdapter {
    pub fn new(broker: Arc<CalendarBroker>, gateway: Arc<ProductGatewayLeaseRegistry>) -> Self {
        Self { broker, gateway }
    }

    async fn observe_local_calendar(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        provider: CalendarProvider,
        scope: &ExecutionScope,
    ) -> Result<ProductSourceObservation, AgentFailure> {
        let source_binding = validate_source_for_actor(actor, source)?;
        let (resource_handles, calendar_ids) = configured_resources(source)?;
        let expected_subject = source
            .native_subject_fingerprint()
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let host_epoch = calendar_host_epoch(&self.broker, actor, provider)?;
        let read = NativeReadFence {
            actor,
            host_epoch: &host_epoch,
            connection_id: source.connection_id().as_str(),
            connection_revision: source.revision(),
            provider,
            calendar_ids: &calendar_ids,
            range_start_unix_ms: 0,
            range_end_unix_ms: 1,
            expected_native_subject: expected_subject,
        };
        let request_id = Uuid::new_v4();
        let response = native_call(
            &self.broker,
            &read,
            request_id,
            CalendarAcquisitionMode::InspectSubject,
            None,
            scope,
        )
        .await?;
        validate_native_observation(&response, &read, request_id)?;
        Ok(ProductSourceObservation {
            expectation: SourceExpectation {
                source: source_binding,
                revision: Some(source.revision()),
                provider_revision: None,
                authority: source.source_authority(),
                physical_resources: resource_handles,
                subject_fingerprint: response.native_subject_fingerprint_after,
                gateway: None,
            },
            provider,
            permission: ProductCalendarPermission::NativeRead,
            observed_at: Utc::now(),
        })
    }

    async fn observe_gateway(
        &self,
        actor: &OwnerActor,
        source: &SourceConnection,
        scope: &ExecutionScope,
    ) -> Result<ProductSourceObservation, AgentFailure> {
        validate_source_for_actor(actor, source)?;
        let lease = self
            .gateway
            .acquire_optional(actor, scope)
            .await?
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        GatewayCalendarMirrorClient::new(lease)
            .observe_source(actor, source, scope)
            .await
    }

    async fn acquire_native(
        &self,
        permit: &ProductCalendarReadPermit,
        scope: &ExecutionScope,
    ) -> Result<CalendarProductReadResult, AgentFailure> {
        permit.revalidate(scope).await?;
        let request = permit.request();
        let observed = permit.observation();
        let source = &observed.expectation;
        let source_revision = source.revision.ok_or(AgentFailure::StaleContext)?;
        let calendar_ids = source
            .physical_resources
            .iter()
            .map(|resource| resource.as_str().to_owned())
            .collect::<Vec<_>>();
        if calendar_ids.is_empty()
            || calendar_ids.len() > MAX_CALENDAR_RESOURCES
            || calendar_ids.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        if !matches!(observed.provider, CalendarProvider::EventKit | CalendarProvider::Fixture)
            || source.gateway.is_some()
            || source.source != request.source
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let connection_id_value = request.source.connection_id();
        let connection_id = connection_id_value.as_str();
        let host_epoch = calendar_host_epoch(&self.broker, &request.actor, observed.provider)?;
        let fence = NativeReadFence {
            actor: &request.actor,
            host_epoch: &host_epoch,
            connection_id,
            connection_revision: source_revision,
            provider: observed.provider,
            calendar_ids: &calendar_ids,
            range_start_unix_ms: request.range_start.timestamp_millis(),
            range_end_unix_ms: request.range_end.timestamp_millis(),
            expected_native_subject: &source.subject_fingerprint,
        };

        // Inspect the admitted subject immediately before and after EventKit.
        // Every call retains the same host epoch, actor/runtime and source
        // binding; the read request ID is the Access read operation ID.
        let before_id = Uuid::new_v4();
        let before = native_call(
            &self.broker,
            &fence,
            before_id,
            CalendarAcquisitionMode::InspectSubject,
            Some(permit.expires_at()),
            scope,
        )
        .await?;
        permit.revalidate(scope).await?;
        validate_native_observation(&before, &fence, before_id)?;

        let read = native_call(
            &self.broker,
            &fence,
            request.read_operation_id,
            CalendarAcquisitionMode::ReadEvents,
            Some(permit.expires_at()),
            scope,
        )
        .await?;
        permit.revalidate(scope).await?;
        validate_native_read(&read, &fence, request.read_operation_id)?;

        let after_id = Uuid::new_v4();
        let after = native_call(
            &self.broker,
            &fence,
            after_id,
            CalendarAcquisitionMode::InspectSubject,
            Some(permit.expires_at()),
            scope,
        )
        .await?;
        permit.revalidate(scope).await?;
        validate_native_observation(&after, &fence, after_id)?;
        if before.native_subject_fingerprint_after != after.native_subject_fingerprint_after {
            return Err(AgentFailure::StaleContext);
        }

        let observed_at = Utc::now();
        let native_payload_bytes = serde_json::to_vec(&read.batches)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len();
        let batches = normalize_native_batches(read.batches, &calendar_ids, observed_at)?;
        let bytes = serde_json::to_vec(&batches).map_err(|_| AgentFailure::InvalidInput)?;
        let record_count = batches.iter().fold(0usize, |count, batch| match batch {
            CalendarResourceOutcome::Complete { records, .. } => {
                count.saturating_add(records.len())
            }
            CalendarResourceOutcome::Failed { .. } => count,
        });
        let record_count = u32::try_from(record_count).map_err(|_| AgentFailure::BudgetExceeded)?;
        let byte_count = u32::try_from(bytes.len()).map_err(|_| AgentFailure::BudgetExceeded)?;
        if record_count > request.limits.max_records
            || byte_count > request.limits.max_bytes
            || native_payload_bytes > request.limits.max_bytes as usize
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let expires_at = permit.expires_at();
        if observed_at >= expires_at {
            return Err(AgentFailure::DeadlineExceeded);
        }
        permit.revalidate(scope).await?;
        Ok(CalendarProductReadResult {
            consumed_records: record_count,
            consumed_bytes: u32::try_from(native_payload_bytes.max(bytes.len()))
                .map_err(|_| AgentFailure::BudgetExceeded)?,
            batches,
            binding: ProductCalendarResultBinding {
                read_operation_id: request.read_operation_id,
                source: source.clone(),
                payload_digest: Sha256::digest(&bytes).into(),
                record_count,
                byte_count,
                observed_at,
                expires_at,
            },
        })
    }
}

impl CalendarProductTransport for CalendarProductAdapter {
    fn observe<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a SourceConnection,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ProductSourceObservation, AgentFailure>> {
        Box::pin(async move {
            match provider_for(source)? {
                CalendarProvider::EventKit | CalendarProvider::Fixture => {
                    self.observe_local_calendar(actor, source, provider_for(source)?, scope)
                        .await
                }
                CalendarProvider::Google | CalendarProvider::Microsoft => {
                    self.observe_gateway(actor, source, scope).await
                }
                _ => Err(AgentFailure::CapabilityUnavailable),
            }
        })
    }

    fn acquire<'a>(
        &'a self,
        fence: ProductCalendarDispatchFence<'a>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CalendarProductReadResult, AgentFailure>> {
        Box::pin(async move {
            let permit = fence.permit();
            match permit.observation().provider {
                CalendarProvider::EventKit | CalendarProvider::Fixture => {
                    self.acquire_native(permit, scope).await
                }
                CalendarProvider::Google | CalendarProvider::Microsoft => {
                    let generation = permit
                        .gateway_runtime_generation()
                        .ok_or(AgentFailure::PolicyDenied)?;
                    let lease = self
                        .gateway
                        .acquire(&permit.request().actor, generation, scope)
                        .await?;
                    GatewayCalendarMirrorClient::new(lease)
                        .acquire(permit, scope)
                        .await
                }
                _ => Err(AgentFailure::CapabilityUnavailable),
            }
        })
    }
}

struct NativeReadFence<'a> {
    actor: &'a OwnerActor,
    host_epoch: &'a str,
    connection_id: &'a str,
    connection_revision: u64,
    provider: CalendarProvider,
    calendar_ids: &'a [String],
    range_start_unix_ms: i64,
    range_end_unix_ms: i64,
    expected_native_subject: &'a str,
}

fn validate_source_for_actor(
    actor: &OwnerActor,
    source: &SourceConnection,
) -> Result<GrantSourceBinding, AgentFailure> {
    actor.validate()?;
    source.validate().map_err(|_| AgentFailure::InvalidInput)?;
    if source.person_id() != actor.person_id || !source.is_serving() {
        return Err(AgentFailure::StaleContext);
    }
    GrantSourceBinding::try_new(
        actor.person_id,
        source.connection_id().clone(),
        source.connector_id().clone(),
        source.execution_owner_id().clone(),
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn provider_for(source: &SourceConnection) -> Result<CalendarProvider, AgentFailure> {
    match source.connector_id().as_str() {
        "calendar.google" => Ok(CalendarProvider::Google),
        "calendar.microsoft" => Ok(CalendarProvider::Microsoft),
        connector => match floe_access::local_calendar_provider(connector) {
            Some(CalendarProvider::EventKit) => Ok(CalendarProvider::EventKit),
            #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
            Some(CalendarProvider::Fixture) => Ok(CalendarProvider::Fixture),
            _ => Err(AgentFailure::CapabilityUnavailable),
        },
    }
}

fn calendar_host_epoch(
    broker: &CalendarBroker,
    actor: &OwnerActor,
    provider: CalendarProvider,
) -> Result<String, AgentFailure> {
    match provider {
        CalendarProvider::EventKit => broker.host_epoch(actor.person_id),
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        CalendarProvider::Fixture => Ok(super::fixture_calendar::host_epoch(&actor.device_id)),
        _ => Err(AgentFailure::CapabilityUnavailable),
    }
}

fn configured_resources(
    source: &SourceConnection,
) -> Result<(Vec<ResourceHandle>, Vec<String>), AgentFailure> {
    let resources = source.resources();
    if resources.is_empty() || resources.len() > MAX_CALENDAR_RESOURCES {
        return Err(AgentFailure::BudgetExceeded);
    }
    let handles = resources
        .iter()
        .map(|resource| resource.handle().clone())
        .collect::<Vec<_>>();
    let ids = handles
        .iter()
        .map(|resource| resource.as_str().to_owned())
        .collect::<Vec<_>>();
    if ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(AgentFailure::InvalidInput);
    }
    Ok((handles, ids))
}

fn native_deadline(
    scope: &ExecutionScope,
    expiry: Option<DateTime<Utc>>,
) -> Result<(Instant, i64), AgentFailure> {
    if scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    let mono_now = Instant::now();
    let mut remaining = scope
        .deadline()
        .saturating_duration_since(mono_now)
        .min(MAX_NATIVE_CALL);
    if let Some(expiry) = expiry {
        let wall_remaining = expiry
            .signed_duration_since(Utc::now())
            .to_std()
            .map_err(|_| AgentFailure::DeadlineExceeded)?;
        remaining = remaining.min(wall_remaining);
    }
    if remaining.is_zero() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    let wall_deadline = Utc::now()
        .checked_add_signed(
            chrono::Duration::from_std(remaining).map_err(|_| AgentFailure::InvalidInput)?,
        )
        .ok_or(AgentFailure::InvalidInput)?;
    Ok((mono_now + remaining, wall_deadline.timestamp_millis()))
}

#[allow(clippy::too_many_arguments)]
async fn native_call(
    broker: &CalendarBroker,
    fence: &NativeReadFence<'_>,
    request_id: Uuid,
    mode: CalendarAcquisitionMode,
    expiry: Option<DateTime<Utc>>,
    scope: &ExecutionScope,
) -> Result<CalendarAcquisitionResult, AgentFailure> {
    if request_id.is_nil()
        || fence.host_epoch.is_empty()
        || fence.connection_id.is_empty()
        || fence.connection_revision == 0
        || fence.calendar_ids.is_empty()
        || fence.calendar_ids.len() > MAX_CALENDAR_RESOURCES
        || fence.range_start_unix_ms < 0
        || fence.range_end_unix_ms <= fence.range_start_unix_ms
        || fence.expected_native_subject.len() != 64
    {
        return Err(AgentFailure::InvalidInput);
    }
    fence.actor.validate()?;
    let (deadline, deadline_unix_ms) = native_deadline(scope, expiry)?;
    let request = CalendarAcquisitionRequest {
        request_id,
        host_epoch: fence.host_epoch.to_owned(),
        person_id: fence.actor.person_id,
        device_id: fence.actor.device_id.clone(),
        connection_id: fence.connection_id.to_owned(),
        connection_revision: fence.connection_revision,
        provider: fence.provider,
        mode,
        calendar_ids: fence.calendar_ids.to_vec(),
        range_start_unix_ms: fence.range_start_unix_ms,
        range_end_unix_ms: fence.range_end_unix_ms,
        deadline_unix_ms,
        expected_native_subject_fingerprint: (mode == CalendarAcquisitionMode::ReadEvents)
            .then(|| fence.expected_native_subject.to_owned()),
    };
    let response = if fence.provider == CalendarProvider::Fixture {
        #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
        {
            super::fixture_calendar::respond(&request)?
        }
        #[cfg(not(all(feature = "qa-fixtures", target_os = "linux")))]
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
    } else {
        broker
            .submit(
                request,
                Utc::now().timestamp_millis(),
                scope.cancellation().clone(),
            )
            .await?
    };
    if scope.cancellation().is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    if calendar_host_epoch(broker, fence.actor, fence.provider)?.as_str() != fence.host_epoch {
        return Err(AgentFailure::StaleContext);
    }
    Ok(response)
}

fn validate_native_observation(
    response: &CalendarAcquisitionResult,
    fence: &NativeReadFence<'_>,
    request_id: Uuid,
) -> Result<(), AgentFailure> {
    if !native_identity_matches(
        response,
        fence,
        request_id,
        CalendarAcquisitionMode::InspectSubject,
    ) || response.native_subject_fingerprint_before != fence.expected_native_subject
        || response.native_subject_fingerprint_after != fence.expected_native_subject
        || !response.batches.is_empty()
        || !valid_calendar_inventory(response)
        || fence
            .calendar_ids
            .iter()
            .any(|resource| !response.available_calendar_ids.contains(resource))
    {
        return Err(AgentFailure::StaleContext);
    }
    if !matches!(response.permission_class.as_str(), "full" | "authorized") {
        return Err(AgentFailure::CapabilityDenied);
    }
    Ok(())
}

fn validate_native_read(
    response: &CalendarAcquisitionResult,
    fence: &NativeReadFence<'_>,
    request_id: Uuid,
) -> Result<(), AgentFailure> {
    if !native_identity_matches(
        response,
        fence,
        request_id,
        CalendarAcquisitionMode::ReadEvents,
    ) || response.native_subject_fingerprint_before != fence.expected_native_subject
        || response.native_subject_fingerprint_after != fence.expected_native_subject
        || !valid_calendar_inventory(response)
        || fence
            .calendar_ids
            .iter()
            .any(|resource| !response.available_calendar_ids.contains(resource))
        || response.batches.len() != fence.calendar_ids.len()
        || response.batches.iter().any(|batch| {
            !fence.calendar_ids.contains(&batch.calendar_id)
                || batch
                    .records
                    .iter()
                    .any(|record| record.calendar_id != batch.calendar_id)
        })
    {
        return Err(AgentFailure::StaleContext);
    }
    if !matches!(response.permission_class.as_str(), "full" | "authorized") {
        return Err(AgentFailure::CapabilityDenied);
    }
    Ok(())
}

fn native_identity_matches(
    response: &CalendarAcquisitionResult,
    fence: &NativeReadFence<'_>,
    request_id: Uuid,
    mode: CalendarAcquisitionMode,
) -> bool {
    response.request_id == request_id
        && response.host_epoch == fence.host_epoch
        && response.person_id == fence.actor.person_id
        && response.device_id == fence.actor.device_id
        && response.connection_id == fence.connection_id
        && response.connection_revision == fence.connection_revision
        && response.provider == fence.provider
        && response.mode == mode
        && response.calendar_ids == fence.calendar_ids
        && response.range_start_unix_ms == fence.range_start_unix_ms
        && response.range_end_unix_ms == fence.range_end_unix_ms
        && valid_fingerprint(&response.native_subject_fingerprint_before)
        && valid_fingerprint(&response.native_subject_fingerprint_after)
}

fn valid_calendar_inventory(response: &CalendarAcquisitionResult) -> bool {
    response.available_calendar_ids.len() <= MAX_CALENDAR_RESOURCES
        && response.available_calendar_ids.len() == response.available_calendars.len()
        && !response
            .available_calendar_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        && response
            .available_calendar_ids
            .iter()
            .all(|resource| resource_is_valid(resource))
        && response
            .available_calendars
            .iter()
            .zip(&response.available_calendar_ids)
            .all(|(resource, id)| {
                resource.handle == *id
                    && !resource.label.is_empty()
                    && resource.label.len() <= 256
                    && !resource.label.chars().any(char::is_control)
            })
}

pub(super) fn normalize_native_batches(
    batches: Vec<NativeCalendarBatch>,
    expected_ids: &[String],
    observed_at: DateTime<Utc>,
) -> Result<Vec<CalendarResourceOutcome>, AgentFailure> {
    if batches.len() != expected_ids.len() {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let mut by_id = std::collections::BTreeMap::new();
    for batch in batches {
        if by_id.insert(batch.calendar_id.clone(), batch).is_some() {
            return Err(AgentFailure::InvalidInput);
        }
    }
    if by_id
        .keys()
        .map(String::as_str)
        .ne(expected_ids.iter().map(String::as_str))
    {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let mut output = Vec::with_capacity(expected_ids.len());
    let mut total_records = 0usize;
    for calendar_id in expected_ids {
        let batch = by_id
            .remove(calendar_id)
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        if let Some(failure) = batch.failure {
            if !batch.records.is_empty() {
                return Err(AgentFailure::InvalidInput);
            }
            output.push(CalendarResourceOutcome::Failed {
                calendar_id: calendar_id.clone(),
                reason: native_calendar_failure(failure),
                observed_at,
            });
            continue;
        }
        if batch.records.len() > MAX_CALENDAR_RECORDS {
            return Err(AgentFailure::BudgetExceeded);
        }
        total_records = total_records
            .checked_add(batch.records.len())
            .ok_or(AgentFailure::BudgetExceeded)?;
        if total_records > MAX_CALENDAR_RECORDS {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut ids = HashSet::new();
        let records = batch
            .records
            .into_iter()
            .map(|record| {
                if record.calendar_id != *calendar_id
                    || record.external_id.trim().is_empty()
                    || record.external_id.len() > 512
                    || record.external_id.chars().any(char::is_control)
                    || record.title.len() > MAX_TITLE_BYTES
                    || !ids.insert(record.external_id.clone())
                {
                    return Err(AgentFailure::InvalidInput);
                }
                let external_revision = CalendarExternalRevision::from_observation_fingerprint_hex(
                    &record.external_revision,
                )
                .ok_or(AgentFailure::InvalidInput)?;
                let schedule = native_schedule(record.schedule)?;
                Ok(CalendarRecord {
                    can_modify: record.can_modify,
                    calendar_id: record.calendar_id,
                    external_id: record.external_id,
                    external_revision,
                    title: record.title,
                    schedule,
                })
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        output.push(CalendarResourceOutcome::Complete {
            calendar_id: calendar_id.clone(),
            records,
            observed_at,
        });
    }
    Ok(output)
}

fn native_calendar_failure(failure: NativeCalendarFailure) -> CalendarFailure {
    match failure {
        NativeCalendarFailure::PermissionDenied => CalendarFailure::PermissionDenied,
        NativeCalendarFailure::CalendarUnavailable => CalendarFailure::CalendarUnavailable,
        NativeCalendarFailure::ProviderUnavailable => CalendarFailure::ProviderUnavailable,
    }
}

fn native_schedule(
    schedule: floe_native::NativeEventSchedule,
) -> Result<EventSchedule, AgentFailure> {
    match schedule {
        floe_native::NativeEventSchedule::Timed {
            starts_at,
            ends_at,
            timezone,
        } => {
            let starts_at = DateTime::parse_from_rfc3339(&starts_at)
                .map_err(|_| AgentFailure::InvalidInput)?
                .with_timezone(&Utc);
            let ends_at = DateTime::parse_from_rfc3339(&ends_at)
                .map_err(|_| AgentFailure::InvalidInput)?
                .with_timezone(&Utc);
            TimedSchedule::new(starts_at, ends_at, timezone)
                .map(EventSchedule::Timed)
                .map_err(|_| AgentFailure::InvalidInput)
        }
        floe_native::NativeEventSchedule::AllDay {
            start_date,
            end_date_exclusive,
        } => {
            let start_date = NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
                .map_err(|_| AgentFailure::InvalidInput)?;
            let end_date_exclusive = NaiveDate::parse_from_str(&end_date_exclusive, "%Y-%m-%d")
                .map_err(|_| AgentFailure::InvalidInput)?;
            AllDaySchedule::new(start_date, end_date_exclusive)
                .map(EventSchedule::AllDay)
                .map_err(|_| AgentFailure::InvalidInput)
        }
    }
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && value.bytes().any(|byte| byte != b'0')
}

fn resource_is_valid(value: &str) -> bool {
    ResourceHandle::try_new(value.to_owned()).is_ok()
}
