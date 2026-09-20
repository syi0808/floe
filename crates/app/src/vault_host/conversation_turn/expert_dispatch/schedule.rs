use std::{future::Future, pin::Pin, sync::Arc};

use floe_agent_contract::AgentFailure;
use floe_experts_builtin::schedule;

use floe_access::{
    CalendarReadAccessAdmission, CalendarReadAccessRequest, CalendarReadAccessStamp,
    CalendarReadAdmission,
};
use floe_agent_contract::{
    AgentEndpoint, BoxFuture, DependencyCoverage, EndpointInvocation, ExpertReport,
};
use floe_context::{
    CalendarObservation, CalendarObserveRequest, CalendarSource, ProjectedCalendarItem,
    ProjectedCalendarObservation,
};
pub(in crate::vault_host) use floe_experts_builtin::schedule::schedule_definition;

use crate::FloeCore;
use agent::CalendarExpertEndpointRequest;
use floe_context_contract::CalendarProvider;
use floe_day::CalendarTimelineGrant;
use floe_day::{CalendarBatch, CalendarConnection, CalendarRecord};
use floe_execution::ExecutionScope;
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, RemoteCalendarGrantBinding, VaultKeyProvider};
// BOUNDARY(stage-3): the Schedule Expert still reaches the provider adapter directly.
// The acquisition must arrive through an owner-defined source port.
use floe_provider_adapters::control::{
    RemoteAuthorizationClient, calendar_query_sha256, parse_calendar_challenge,
};
use floe_provider_adapters::sources::native_acquisition::{
    CalendarAcquisitionMode, CalendarAcquisitionRequest, CalendarAcquisitionResult,
    NativeCalendarBatch, NativeCalendarFailure, NativeEventSchedule,
};
use floe_provider_adapters::sources::native_calendar::NativeCalendarReadAccess;
use uuid::Uuid;

use crate::local_context::LocalContextHost;
use floe_connections::CalendarConnectionRef;
use floe_provider_adapters::sources::ServerSourceClient;

// The legacy calendar turn's own request type is named by the host regressions
// that still cover that path; the module stays inside the vault host.
pub(in crate::vault_host) mod agent;

pub(crate) use agent::CALENDAR_EXPERT_SETTLEMENT_OWNER;

use super::super::expert_compat::{EndpointConnectionStore, Model, external_transfer_consent};

/// The endpoint the delegating Run invokes for the Schedule Expert.
///
/// The invocation is self-sufficient: the Manager delegation message is the
/// assignment, device/AgentContext/output-bound arrive in the explicit
/// execution context, and the saved-connection store is injected at
/// construction. No run-id staging exists, and the root user prompt is never
/// consulted.
pub(crate) struct ScheduleEndpoint<Keys> {
    core: Arc<FloeCore>,
    vault: Arc<EncryptedAgentVault<Keys>>,
    local_context: Arc<LocalContextHost>,
    connections: EndpointConnectionStore,
}

impl<Keys> ScheduleEndpoint<Keys> {
    pub(crate) fn new(
        core: Arc<FloeCore>,
        vault: Arc<EncryptedAgentVault<Keys>>,
        local_context: Arc<LocalContextHost>,
        connections: EndpointConnectionStore,
    ) -> Self {
        Self {
            core,
            vault,
            local_context,
            connections,
        }
    }
}

impl<Keys: VaultKeyProvider + 'static> AgentEndpoint for ScheduleEndpoint<Keys> {
    fn execute<'a>(
        &'a self,
        invocation: EndpointInvocation,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>> {
        Box::pin(async move {
            let context = &invocation.request.execution_context;
            context.validate()?;
            if invocation.request.principal != self.vault.person_id().to_string() {
                return Err(AgentFailure::CapabilityDenied);
            }
            let selected = select_active_setup(&self.vault, &context.device_id).await?;
            if selected.ambiguous {
                return Err(AgentFailure::AccessReviewRequired);
            }
            // Legacy Schedule compatibility, prepared here because an actual
            // delegated endpoint runs: the stored credential is loaded from
            // the injected store and admitted for source and legacy model
            // use. No pre-resolved route exists.
            let person_id = self.vault.person_id();
            let stored = floe_inference::SavedConnectionStore::load(&self.connections)?;
            let source_client = ServerSourceClient::prepare(
                stored.clone(),
                &person_id.to_string(),
                &context.device_id,
            )?;
            // The Manager delegation message is the Expert assignment, never
            // the root user prompt.
            let plan = schedule::plan_run(
                &invocation.request.message,
                selected.binding.provider,
                selected.binding.calendar_ids.len(),
                source_client.is_some(),
                chrono::Local::now(),
                chrono::Utc::now(),
            )?;
            // The Expert decided where it may reason; the host only builds it.
            let model = match plan.reasoning {
                schedule::ScheduleReasoning::OnDevice => {
                    Model::for_stored_connection(None, &person_id.to_string(), &context.device_id)?
                }
                schedule::ScheduleReasoning::ConversationRoute => {
                    Model::for_stored_connection(
                        stored,
                        &person_id.to_string(),
                        &context.device_id,
                    )?
                }
            };
            let remote_backend = match source_client.as_ref() {
                Some(client) if plan.acquire_remotely => Some(VaultRemoteCalendarBackend::new(
                    &self.vault,
                    &self.core,
                    client,
                    person_id,
                    selected.binding.provider,
                    selected.binding.calendar_ids.clone(),
                    selected.binding.connection_revision,
                )?),
                _ => None,
            };
            // Source enumeration happens here, only when this provider needs
            // the catalog cross-check during validation.
            let calendar_catalog = match (&source_client, selected.binding.provider) {
                (Some(client), CalendarProvider::Google | CalendarProvider::Microsoft) => {
                    Some(
                        client
                            .observe_calendar_connections(
                                scope.deadline(),
                                scope.cancellation(),
                            )
                            .await?,
                    )
                }
                _ => None,
            };
            let access = BoundAccess {
                core: &self.core,
                setup: &selected.setup,
                binding: &selected.binding,
                request_device_id: &context.device_id,
                calendar_catalog: calendar_catalog.as_deref(),
                ambiguous: selected.ambiguous,
                access: Access::new(
                    self.vault.person_id(),
                    selected.binding.provider,
                    selected.binding.device_id.clone(),
                    selected.binding.calendar_ids.clone(),
                    selected.setup.setup_id.to_string(),
                    selected.binding.connection_revision,
                    &model,
                    &self.local_context,
                    &self.core,
                    selected.binding.source_authority,
                    remote_backend
                        .as_ref()
                        .map(|backend| backend as &dyn RemoteCalendarBackend),
                ),
            };
            let placement = floe_inference::ModelTransport::placement(&model);
            let endpoint = self
                .core
                .run_calendar_expert_endpoint(
                    &self.vault,
                    &access,
                    &model,
                    CalendarExpertEndpointRequest {
                        person_id: self.vault.person_id(),
                        usage: Default::default(),
                        context: context.agent_context.clone(),
                        policy: schedule::run_policy(
                            placement,
                            selected.binding.data_class(),
                            external_transfer_consent(placement),
                        ),
                        grant: CalendarTimelineGrant {
                            person_id: self.vault.person_id(),
                            handle: selected.setup.view_handle,
                            provider: selected.binding.provider,
                            device_id: selected.binding.device_id.clone(),
                            calendar_ids: selected.binding.calendar_ids.clone(),
                            connection_revision: selected.binding.connection_revision,
                            day: plan.range,
                            starts_at: plan.starts_at,
                            ends_at: plan.ends_at,
                            expires_at: plan.expires_at,
                        },
                        assignment_id: selected.setup.expert_assignment_id,
                        invocation_id: invocation.request.invocation_key.as_uuid(),
                        assignment: invocation.request.message.clone(),
                        propose_focus: plan.propose_focus,
                        max_output_bytes: context.max_output_bytes,
                        deadline: scope.deadline(),
                        cancellation: scope.cancellation().clone(),
                    },
                    chrono::Utc::now,
                )
                .await?;
            let coverage = if endpoint.dependencies.is_empty() {
                DependencyCoverage::Independent
            } else {
                DependencyCoverage::Dependent {
                    dependencies: endpoint.dependencies,
                }
            };
            coverage
                .validate()
                .map_err(|_| AgentFailure::InvalidModelOutput)?;
            Ok(ExpertReport {
                task_id: invocation.request.task_id,
                principal: invocation.request.principal,
                agent_id: invocation.request.selected_agent_id,
                definition_revision: invocation.request.selected_definition_revision,
                result: serde_json::to_string(&endpoint.report)
                    .map_err(|_| AgentFailure::InvalidModelOutput)?,
                artifacts: vec![],
                coverage,
                settlement: Some(endpoint.settlement.into_endpoint_settlement()?),
            })
        })
    }
}

struct SelectedSetup {
    setup: floe_experts::CalendarExpertSetupReceipt,
    binding: floe_experts::CalendarViewBinding,
    ambiguous: bool,
}

/// Read the Person's recorded calendar setups and let the Schedule Expert pick
/// the one this device may use.
///
/// Everything here is record reading and pairing; which setup is eligible, and
/// whether the choice is ambiguous, is the Expert's judgment.
async fn select_active_setup<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    device_id: &str,
) -> Result<SelectedSetup, AgentFailure> {
    let overview = vault.calendar_expert_overview().await?;
    let enabled = |ids: [Uuid; 2], installations: bool| {
        ids.iter().all(|id| {
            if installations {
                overview
                    .registry
                    .installations
                    .iter()
                    .any(|entry| entry.id == *id && entry.enabled)
            } else {
                overview
                    .registry
                    .assignments
                    .iter()
                    .any(|entry| entry.id == *id && entry.enabled)
            }
        })
    };
    let pairs: Vec<_> = overview
        .setups
        .iter()
        .filter_map(|setup| {
            let binding = overview
                .views
                .iter()
                .find(|binding| binding.handle == setup.view_handle)?;
            Some((setup.clone(), binding.clone()))
        })
        .collect();
    let candidates: Vec<_> = pairs
        .iter()
        .map(|(setup, binding)| schedule::ScheduleSetupCandidate {
            device_id: binding.device_id.clone(),
            active: binding.enabled
                && enabled(
                    [setup.tool_installation_id, setup.expert_installation_id],
                    true,
                )
                && enabled(
                    [setup.tool_assignment_id, setup.expert_assignment_id],
                    false,
                ),
        })
        .collect();
    let selection = schedule::select_active_setup(&candidates, device_id)?;
    let (setup, binding) = pairs[selection.index].clone();
    Ok(SelectedSetup {
        setup,
        binding,
        ambiguous: selection.ambiguous,
    })
}

struct BoundAccess<'host> {
    core: &'host crate::FloeCore,
    setup: &'host floe_experts::CalendarExpertSetupReceipt,
    binding: &'host floe_experts::CalendarViewBinding,
    request_device_id: &'host str,
    calendar_catalog: Option<&'host [CalendarConnectionRef]>,
    ambiguous: bool,
    access: Access<'host>,
}

impl BoundAccess<'_> {
    async fn validate(&self, person_id: PersonId) -> Result<(), AgentFailure> {
        if person_id != self.setup.person_id || person_id != self.binding.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        if self.ambiguous {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let connection = self
            .core
            .calendar_connection(person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        validate_active_connection(
            self.setup,
            self.binding,
            &connection,
            self.request_device_id,
            self.calendar_catalog,
        )?;
        Ok(())
    }
}

impl CalendarSource for BoundAccess<'_> {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        self.validate(request.person_id).await?;
        self.access.check(request).await
    }

    async fn observe(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<CalendarObservation>, AgentFailure> {
        self.validate(request.person_id).await?;
        self.access.observe(request).await
    }

    async fn observe_projected(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<ProjectedCalendarObservation>, AgentFailure> {
        self.validate(request.person_id).await?;
        self.access.observe_projected(request).await
    }
}

impl CalendarReadAdmission for BoundAccess<'_> {
    async fn admission(
        &self,
        request: &CalendarReadAccessRequest,
    ) -> Result<Option<CalendarReadAccessAdmission>, AgentFailure> {
        self.validate(request.person_id).await?;
        self.access.admission(request).await
    }

    async fn admission_after_check(
        &self,
        request: &CalendarReadAccessRequest,
        stamp: &CalendarReadAccessStamp,
    ) -> Result<Option<CalendarReadAccessAdmission>, AgentFailure> {
        self.validate(request.person_id).await?;
        self.access.admission_after_check(request, stamp).await
    }
}

fn validate_active_connection(
    setup: &floe_experts::CalendarExpertSetupReceipt,
    binding: &floe_experts::CalendarViewBinding,
    connection: &CalendarConnection,
    request_device_id: &str,
    calendar_catalog: Option<&[CalendarConnectionRef]>,
) -> Result<(), AgentFailure> {
    let calendar_ids: Vec<_> = connection
        .calendars
        .iter()
        .map(|calendar| calendar.calendar_id.clone())
        .collect();
    let native = matches!(
        connection.provider,
        CalendarProvider::EventKit | CalendarProvider::Android
    );
    if native {
        let authority = connection.source_authority;
        if !authority.is_valid() {
            return Err(AgentFailure::AccessReviewRequired);
        }
        if setup.source_authority != Some(authority) || binding.source_authority != Some(authority)
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
    }
    if connection.disconnected
        || connection.revision == 0
        || setup.view_handle != binding.handle
        || setup.connection_scope != connection.scope
        || connection.device_id != binding.device_id
        || binding.device_id != request_device_id
        || connection.provider != binding.provider
        || !binding
            .calendar_ids
            .iter()
            .all(|identifier| calendar_ids.contains(identifier))
        || binding.connection_scope != connection.scope
    {
        return Err(AgentFailure::StaleContext);
    }
    if let Ok(connection_id) = Uuid::parse_str(&connection.connection_id)
        && setup.setup_id != connection_id
    {
        return Err(AgentFailure::StaleContext);
    }
    if !native
        && (setup.connection_revision != connection.revision
            || binding.connection_revision != connection.revision
            || calendar_ids != binding.calendar_ids)
    {
        return Err(AgentFailure::StaleContext);
    }
    let connector_id = match connection.provider {
        CalendarProvider::Google => Some("calendar.google"),
        CalendarProvider::Microsoft => Some("calendar.microsoft"),
        CalendarProvider::Fixture | CalendarProvider::EventKit | CalendarProvider::Android => None,
    };
    match (connector_id, calendar_catalog) {
        (Some(connector_id), Some(catalog)) => {
            let candidates: Vec<_> = catalog
                .iter()
                .filter(|candidate| candidate.connector_id == connector_id)
                .collect();
            if !matches!(candidates.as_slice(), [candidate]
                    if candidate.connection_id == connection.connection_id
                        && candidate.connection_revision == connection.revision)
                || catalog.iter().any(|candidate| {
                    candidate.connector_id != connector_id
                        && candidate.connection_id == connection.connection_id
                })
            {
                return Err(AgentFailure::StaleContext);
            }
        }
        (Some(_), _) => return Err(AgentFailure::StaleContext),
        (None, _) => {}
    }
    Ok(())
}

enum Access<'model> {
    Fixture(FixtureAccess),
    Device(DeviceCalendarAccess<'model>),
    Native(NativeCalendarReadAccess),
    Remote(RemoteCalendarAccess<'model>),
}

impl<'model> Access<'model> {
    fn new(
        person_id: PersonId,
        provider: CalendarProvider,
        device_id: String,
        calendar_ids: Vec<String>,
        connection_id: String,
        connection_revision: u64,
        model: &'model Model,
        local_context: &'model LocalContextHost,
        core: &'model crate::FloeCore,
        source_authority: Option<floe_context_contract::SourceAuthority>,
        remote_backend: Option<&'model dyn RemoteCalendarBackend>,
    ) -> Self {
        match provider {
            CalendarProvider::Fixture => Self::Fixture(FixtureAccess {
                device_id,
                calendar_ids,
            }),
            CalendarProvider::EventKit => {
                #[cfg(target_os = "macos")]
                {
                    Self::Native(NativeCalendarReadAccess::new(
                        person_id,
                        device_id,
                        provider,
                        calendar_ids,
                        connection_id,
                        connection_revision,
                    ))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Self::Device(DeviceCalendarAccess {
                        core,
                        connection_id,
                        source_authority,
                        local_context,
                        provider,
                        device_id,
                        calendar_ids,
                        connection_revision,
                    })
                }
            }
            CalendarProvider::Google | CalendarProvider::Microsoft => match model {
                Model::Server(_) => Self::Remote(RemoteCalendarAccess {
                    backend: remote_backend,
                    provider,
                    device_id,
                    calendar_ids,
                }),
                Model::Foundation(_) => Self::Remote(RemoteCalendarAccess {
                    backend: remote_backend,
                    provider,
                    device_id,
                    calendar_ids,
                }),
            },
            CalendarProvider::Android => Self::Device(DeviceCalendarAccess {
                core,
                connection_id,
                source_authority,
                local_context,
                provider,
                device_id,
                calendar_ids,
                connection_revision,
            }),
        }
    }
}

impl CalendarSource for Access<'_> {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        match self {
            Self::Fixture(access) => access.check(request).await,
            Self::Device(access) => access.check(request).await,
            Self::Native(access) => access.check(request).await,
            Self::Remote(access) => access.check(request).await,
        }
    }

    async fn observe(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<CalendarObservation>, AgentFailure> {
        match self {
            Self::Fixture(_) => Ok(None),
            Self::Device(access) => access.observe(request).await,
            Self::Native(access) => access.observe(request).await,
            Self::Remote(_) => Ok(None),
        }
    }

    async fn observe_projected(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<ProjectedCalendarObservation>, AgentFailure> {
        match self {
            Self::Remote(access) => access.observe_projected(request).await,
            Self::Native(access) => access.observe_projected(request).await,
            Self::Fixture(_) | Self::Device(_) => Ok(None),
        }
    }
}

impl CalendarReadAdmission for Access<'_> {
    async fn admission(
        &self,
        request: &CalendarReadAccessRequest,
    ) -> Result<Option<CalendarReadAccessAdmission>, AgentFailure> {
        match self {
            Self::Remote(access) => access.admission(request).await,
            Self::Fixture(_) | Self::Device(_) | Self::Native(_) => Ok(None),
        }
    }
}

struct DeviceCalendarAccess<'store> {
    core: &'store crate::FloeCore,
    connection_id: String,
    source_authority: Option<floe_context_contract::SourceAuthority>,
    local_context: &'store LocalContextHost,
    provider: CalendarProvider,
    device_id: String,
    calendar_ids: Vec<String>,
    connection_revision: u64,
}

impl DeviceCalendarAccess<'_> {
    async fn connection(&self, person_id: PersonId) -> Result<CalendarConnection, AgentFailure> {
        let connection = self
            .core
            .calendar_connection(person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        if self.source_authority.is_none()
            || Some(connection.source_authority) != self.source_authority
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        if connection.disconnected
            || connection.connection_id != self.connection_id
            || connection.revision != self.connection_revision
            || connection.device_id != self.device_id
            || connection.provider != self.provider
            || !self.calendar_ids.iter().all(|identifier| {
                connection
                    .calendars
                    .iter()
                    .any(|calendar| &calendar.calendar_id == identifier)
            })
        {
            return Err(AgentFailure::StaleContext);
        }
        if self.calendar_ids.iter().any(|identifier| {
            connection
                .source_statuses
                .get(identifier)
                .is_some_and(|status| {
                    status.error == Some(floe_day::CalendarFailure::PermissionDenied)
                })
        }) {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(connection)
    }

    async fn acquire(
        &self,
        person_id: PersonId,
        mode: CalendarAcquisitionMode,
        starts_at: chrono::DateTime<chrono::Utc>,
        ends_at: chrono::DateTime<chrono::Utc>,
        expected_native_subject_fingerprint: Option<String>,
        deadline: tokio::time::Instant,
        cancellation: floe_execution::Cancellation,
    ) -> Result<CalendarAcquisitionResult, AgentFailure> {
        let connection = self.connection(person_id).await?;
        let broker = self.local_context.calendar();
        let result = broker
            .submit(
                CalendarAcquisitionRequest {
                    request_id: Uuid::new_v4(),
                    host_epoch: broker.host_epoch(person_id)?,
                    person_id,
                    device_id: self.device_id.clone(),
                    connection_id: connection.connection_id,
                    connection_revision: connection.revision,
                    provider: self.provider,
                    mode,
                    calendar_ids: self.calendar_ids.clone(),
                    range_start_unix_ms: starts_at.timestamp_millis(),
                    range_end_unix_ms: ends_at.timestamp_millis(),
                    // A device read never waits longer than half a minute.
                    deadline_unix_ms: chrono::Utc::now().timestamp_millis()
                        + deadline
                            .saturating_duration_since(tokio::time::Instant::now())
                            .as_millis()
                            .min(30_000) as i64,
                    expected_native_subject_fingerprint,
                },
                chrono::Utc::now().timestamp_millis(),
                cancellation,
            )
            .await?;
        // The subject must not have moved while the device was reading.
        if result.native_subject_fingerprint_before != result.native_subject_fingerprint_after {
            return Err(AgentFailure::StaleContext);
        }
        Ok(result)
    }
}

impl CalendarSource for DeviceCalendarAccess<'_> {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if request.deadline <= tokio::time::Instant::now() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        if request.device_id != self.device_id
            || request.provider != self.provider
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let expected_fingerprint = request.expected_native_subject_fingerprint.clone();
        match self
            .acquire(
                request.person_id,
                CalendarAcquisitionMode::InspectSubject,
                chrono::Utc::now(),
                chrono::Utc::now() + chrono::Duration::days(1),
                None,
                request.deadline,
                request.cancellation,
            )
            .await
        {
            Ok(result) => {
                if expected_fingerprint
                    .as_deref()
                    .is_some_and(|expected| expected != result.native_subject_fingerprint_before)
                {
                    return Err(AgentFailure::AccessReviewRequired);
                }
                Ok(CalendarReadAccessStamp {
                    schema_version: floe_kernel::AGENT_VERSION,
                    person_id: request.person_id,
                    device_id: request.device_id,
                    provider: request.provider,
                    calendar_ids: request.calendar_ids,
                    native_subject_fingerprint: result.native_subject_fingerprint_before,
                    generation: format!("device-{}", result.connection_revision),
                })
            }
            Err(error) => Err(error),
        }
    }

    async fn observe(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<CalendarObservation>, AgentFailure> {
        if request.device_id != self.device_id
            || request.provider != self.provider
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        match self
            .acquire(
                request.person_id,
                CalendarAcquisitionMode::ReadEvents,
                request.starts_at,
                request.ends_at,
                request.expected_native_subject_fingerprint.clone(),
                request.deadline,
                request.cancellation,
            )
            .await
        {
            Ok(result) => {
                let batches = result
                    .batches
                    .into_iter()
                    .map(calendar_batch_from_native)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Some(CalendarObservation {
                    stamp: CalendarReadAccessStamp {
                        schema_version: floe_kernel::AGENT_VERSION,
                        person_id: request.person_id,
                        device_id: request.device_id,
                        provider: request.provider,
                        calendar_ids: request.calendar_ids,
                        native_subject_fingerprint: result.native_subject_fingerprint_before,
                        generation: format!("device-{}", result.connection_revision),
                    },
                    observed_at: chrono::Utc::now(),
                    batches,
                }))
            }
            Err(error) => Err(error),
        }
    }
}

/// One batch the device read, as Day states it.
fn calendar_batch_from_native(batch: NativeCalendarBatch) -> Result<CalendarBatch, AgentFailure> {
    let records = batch
        .records
        .into_iter()
        .map(|record| {
            Ok(CalendarRecord {
                can_modify: record.can_modify,
                calendar_id: record.calendar_id,
                external_id: record.external_id,
                external_revision: record.external_revision,
                title: record.title,
                schedule: native_schedule(record.schedule)?,
            })
        })
        .collect::<Result<Vec<_>, AgentFailure>>()?;
    Ok(CalendarBatch {
        calendar_id: batch.calendar_id,
        records,
        failure: batch.failure.map(native_failure),
    })
}

fn native_failure(value: NativeCalendarFailure) -> floe_day::CalendarFailure {
    match value {
        NativeCalendarFailure::PermissionDenied => floe_day::CalendarFailure::PermissionDenied,
        NativeCalendarFailure::CalendarUnavailable => {
            floe_day::CalendarFailure::CalendarUnavailable
        }
        NativeCalendarFailure::ProviderUnavailable => {
            floe_day::CalendarFailure::ProviderUnavailable
        }
    }
}

fn native_schedule(value: NativeEventSchedule) -> Result<floe_day::EventSchedule, AgentFailure> {
    match value {
        NativeEventSchedule::Timed {
            starts_at,
            ends_at,
            timezone,
        } => Ok(floe_day::EventSchedule::Timed(
            floe_day::TimedSchedule::new(
                chrono::DateTime::parse_from_rfc3339(&starts_at)
                    .map_err(|_| AgentFailure::InvalidInput)?
                    .with_timezone(&chrono::Utc),
                chrono::DateTime::parse_from_rfc3339(&ends_at)
                    .map_err(|_| AgentFailure::InvalidInput)?
                    .with_timezone(&chrono::Utc),
                timezone,
            )
            .map_err(|_| AgentFailure::InvalidInput)?,
        )),
        NativeEventSchedule::AllDay {
            start_date,
            end_date_exclusive,
        } => Ok(floe_day::EventSchedule::AllDay(
            floe_day::AllDaySchedule::new(
                chrono::NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
                    .map_err(|_| AgentFailure::InvalidInput)?,
                chrono::NaiveDate::parse_from_str(&end_date_exclusive, "%Y-%m-%d")
                    .map_err(|_| AgentFailure::InvalidInput)?,
            )
            .map_err(|_| AgentFailure::InvalidInput)?,
        )),
    }
}

trait RemoteCalendarBackend: Sync {
    fn admission<'future>(
        &'future self,
        request: &'future CalendarReadAccessRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<CalendarReadAccessAdmission, AgentFailure>> + Send + 'future,
        >,
    >;

    fn read<'future>(
        &'future self,
        request: &'future CalendarObserveRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<floe_context::CalendarContextView, AgentFailure>>
                + Send
                + 'future,
        >,
    >;
}

struct VaultRemoteCalendarBackend<'host, Keys> {
    vault: &'host floe_vault::EncryptedAgentVault<Keys>,
    core: &'host crate::FloeCore,
    client: RemoteAuthorizationClient,
    person_id: PersonId,
    provider: CalendarProvider,
    calendar_ids: Vec<String>,
    connection_revision: u64,
    pairing: floe_inference::RoutePairing,
}

impl<'host, Keys: VaultKeyProvider> VaultRemoteCalendarBackend<'host, Keys> {
    fn new(
        vault: &'host floe_vault::EncryptedAgentVault<Keys>,
        core: &'host crate::FloeCore,
        source: &ServerSourceClient,
        person_id: PersonId,
        provider: CalendarProvider,
        calendar_ids: Vec<String>,
        connection_revision: u64,
    ) -> Result<Self, AgentFailure> {
        let prepared = source.source();
        if prepared.person_id() != person_id.to_string() || prepared.device_id().is_empty() {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(Self {
            vault,
            core,
            client: source.authorization_client()?,
            person_id,
            provider,
            calendar_ids,
            connection_revision,
            pairing: floe_inference::RoutePairing {
                client_id: prepared.client_id().to_owned(),
                person_id: prepared.person_id().to_owned(),
                device_id: prepared.device_id().to_owned(),
            },
        })
    }

    async fn binding(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<RemoteCalendarGrantBinding, AgentFailure> {
        let connection = self
            .core
            .calendar_connection(self.person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        if connection.provider != self.provider || connection.disconnected {
            return Err(AgentFailure::StaleContext);
        }
        if self.calendar_ids.is_empty()
            || self.calendar_ids.iter().any(|calendar_id| {
                !connection
                    .calendars
                    .iter()
                    .any(|calendar| calendar.calendar_id == *calendar_id)
            })
        {
            return Err(AgentFailure::StaleContext);
        }
        let connector = match self.provider {
            CalendarProvider::Google => "calendar.google",
            CalendarProvider::Microsoft => "calendar.microsoft",
            _ => return Err(AgentFailure::CapabilityDenied),
        };
        let preview = self
            .client
            .calendar_source_preview(
                connector,
                &connection.connection_id,
                self.calendar_ids
                    .first()
                    .ok_or(AgentFailure::InvalidInput)?,
                deadline,
                cancellation,
            )
            .await?;
        let source = self
            .vault
            .verify_remote_calendar_source_preview(
                &preview.descriptor_b64url,
                &preview.producer_signature,
                &self.pairing.person_id,
                &self.pairing.client_id,
                &self.pairing.device_id,
                connector,
                &connection.connection_id,
                self.calendar_ids
                    .first()
                    .ok_or(AgentFailure::InvalidInput)?,
            )
            .await?;
        if source.person_id != self.person_id.to_string()
            || source.connector_id != connector
            || source.connection_id != connection.connection_id
            || source.client_id != self.pairing.client_id
            || source.device_id != self.pairing.device_id
            || source.execution_owner.is_empty()
        {
            return Err(AgentFailure::StaleContext);
        }
        self.vault
            .remote_calendar_grant_binding(
                connector,
                &connection.connection_id,
                source.source_authority,
                self.calendar_ids
                    .first()
                    .ok_or(AgentFailure::InvalidInput)?,
            )
            .await
    }

    fn expectation(
        &self,
        binding: &RemoteCalendarGrantBinding,
        challenge_id: String,
        admission_id: String,
        query_sha256: String,
        result_sha256: String,
    ) -> floe_vault::RemoteCalendarAuthorizationExpectation {
        floe_vault::RemoteCalendarAuthorizationExpectation {
            operation: if admission_id.is_empty() {
                "admission".into()
            } else {
                "release".into()
            },
            client_id: self.pairing.client_id.clone(),
            device_id: self.pairing.device_id.clone(),
            challenge_id,
            admission_id,
            query_sha256,
            result_sha256,
            grant_id: binding.grant.id().as_uuid().to_string(),
            grant_incarnation: binding.grant.authority().incarnation().to_string(),
            grant_epoch: binding.grant.authority().access_epoch().get(),
            source_connector: binding.grant.source().connector().as_str().into(),
            source_connection: binding.grant.source().connection_id().as_str().into(),
            source_execution_owner: binding.grant.source().execution_owner().as_str().into(),
            source_incarnation: binding
                .grant
                .source()
                .source_authority()
                .incarnation()
                .to_string(),
            source_epoch: binding.grant.source().source_authority().epoch().get(),
            resources: binding
                .grant
                .scope()
                .resources()
                .iter()
                .map(|resource| resource.as_str().to_owned())
                .collect(),
            max_items: floe_context::MAX_CALENDAR_CONTEXT_ITEMS as u32,
            max_bytes: floe_context::MAX_CALENDAR_CONTEXT_BYTES as u32,
        }
    }
}

impl<Keys: VaultKeyProvider> RemoteCalendarBackend for VaultRemoteCalendarBackend<'_, Keys> {
    fn admission<'future>(
        &'future self,
        request: &'future CalendarReadAccessRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<CalendarReadAccessAdmission, AgentFailure>> + Send + 'future,
        >,
    > {
        Box::pin(async move {
            if request.person_id != self.person_id
                || request.provider != self.provider
                || request.device_id != self.pairing.device_id
                || request.calendar_ids != self.calendar_ids
                || request.calendar_ids.len() != 1
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let binding = self
                .binding(request.deadline, &request.cancellation)
                .await?;
            let consumer = floe_context_contract::GrantConsumer::builtin("calendar.expert")
                .map_err(|_| AgentFailure::CapabilityDenied)?;
            Ok(CalendarReadAccessAdmission::remote(
                self.person_id,
                binding.grant.id(),
                binding.grant.authority(),
                binding.grant.source().clone(),
                binding.grant.scope().clone(),
                binding.consumer_policy,
                consumer,
                binding.grant.scope().processing().clone(),
            ))
        })
    }

    fn read<'future>(
        &'future self,
        request: &'future CalendarObserveRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<floe_context::CalendarContextView, AgentFailure>>
                + Send
                + 'future,
        >,
    > {
        Box::pin(async move {
            let binding = self
                .binding(request.deadline, &request.cancellation)
                .await?;
            let connector = match self.provider {
                CalendarProvider::Google => "calendar.google",
                CalendarProvider::Microsoft => "calendar.microsoft",
                _ => return Err(AgentFailure::CapabilityDenied),
            };
            let consumer = floe_context_contract::GrantConsumer::builtin("calendar.expert")
                .map_err(|_| AgentFailure::CapabilityDenied)?;
            let challenge = self
                .client
                .begin_calendar_admission(
                    connector,
                    binding.grant.source().connection_id().as_str(),
                    self.connection_revision,
                    self.calendar_ids
                        .first()
                        .ok_or(AgentFailure::InvalidInput)?,
                    &binding.consumer_policy.incarnation().to_string(),
                    binding.consumer_policy.epoch().get(),
                    &binding.grant.id().as_uuid().to_string(),
                    &binding.grant.authority().incarnation().to_string(),
                    binding.grant.authority().access_epoch().get(),
                    "everyday_assistance",
                    consumer.identifier(),
                    floe_context::MAX_CALENDAR_CONTEXT_ITEMS as u32,
                    floe_context::MAX_CALENDAR_CONTEXT_BYTES as u32,
                    request.starts_at.timestamp_millis(),
                    request.ends_at.timestamp_millis(),
                    "",
                    floe_context::MAX_CALENDAR_CONTEXT_ITEMS,
                    request.deadline,
                    &request.cancellation,
                )
                .await?;
            let admission_parts = parse_calendar_challenge(&challenge.challenge_b64url)?;
            if admission_parts.person_id != self.person_id.to_string()
                || admission_parts.client_id != self.pairing.client_id
                || admission_parts.device_id != self.pairing.device_id
                || admission_parts.operation != "admission"
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let query_sha256 = calendar_query_sha256(
                request.starts_at.timestamp_millis(),
                request.ends_at.timestamp_millis(),
                "",
                floe_context::MAX_CALENDAR_CONTEXT_ITEMS,
            )?;
            let admission_expected = self.expectation(
                &binding,
                admission_parts.challenge_id.clone(),
                String::new(),
                query_sha256.clone(),
                String::new(),
            );
            if admission_parts.query_sha256 != admission_expected.query_sha256
                || admission_parts.resources != admission_expected.resources
                || admission_parts.max_items != admission_expected.max_items
                || admission_parts.max_bytes != admission_expected.max_bytes
                || admission_parts.consumer != consumer.identifier()
                || admission_parts.purpose != "everyday_assistance"
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let release = self
                .client
                .read_calendar_admission(
                    self.vault,
                    &admission_expected,
                    &challenge,
                    request.deadline,
                    &request.cancellation,
                )
                .await?;
            let release_parts = parse_calendar_challenge(&release.challenge_b64url)?;
            if release_parts.person_id != self.person_id.to_string()
                || release_parts.client_id != self.pairing.client_id
                || release_parts.device_id != self.pairing.device_id
                || release_parts.operation != "release"
                || release_parts.admission_id != admission_parts.challenge_id
                || release_parts.query_sha256 != admission_expected.query_sha256
                || release_parts.resources != admission_expected.resources
                || release_parts.max_items != admission_expected.max_items
                || release_parts.max_bytes != admission_expected.max_bytes
                || release_parts.consumer != consumer.identifier()
                || release_parts.purpose != "everyday_assistance"
                || release_parts.result_sha256.len() != 64
                || !release_parts
                    .result_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let release_expected = self.expectation(
                &binding,
                release_parts.challenge_id.clone(),
                admission_parts.challenge_id,
                query_sha256,
                release_parts.result_sha256.clone(),
            );
            let view = self
                .client
                .release_calendar(
                    self.vault,
                    &release_expected,
                    &release,
                    request.deadline,
                    &request.cancellation,
                )
                .await?;
            serde_json::from_value(view).map_err(|_| AgentFailure::CapabilityUnavailable)
        })
    }
}

struct RemoteCalendarAccess<'model> {
    backend: Option<&'model dyn RemoteCalendarBackend>,
    provider: CalendarProvider,
    device_id: String,
    calendar_ids: Vec<String>,
}

impl CalendarSource for RemoteCalendarAccess<'_> {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if request.deadline <= tokio::time::Instant::now() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        if request.device_id != self.device_id
            || request.provider != self.provider
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let backend = self.backend.ok_or(AgentFailure::CapabilityUnavailable)?;
        let admission = backend.admission(&request).await?;
        Ok(self.stamp(request.person_id, &admission))
    }

    async fn observe_projected(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<ProjectedCalendarObservation>, AgentFailure> {
        if request.device_id != self.device_id
            || request.provider != self.provider
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let backend = self.backend.ok_or(AgentFailure::CapabilityUnavailable)?;
        let view = backend.read(&request).await?;
        floe_context::validate_calendar_context_view(&view, chrono::Utc::now().timestamp_millis())?;
        if view.range_start_unix_ms != request.starts_at.timestamp_millis()
            || view.range_end_unix_ms != request.ends_at.timestamp_millis()
        {
            return Err(AgentFailure::StaleContext);
        }
        let observed_at = chrono::DateTime::from_timestamp_millis(view.observed_at_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let expires_at = chrono::DateTime::from_timestamp_millis(view.expires_at_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let range_start = chrono::DateTime::from_timestamp_millis(view.range_start_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let range_end = chrono::DateTime::from_timestamp_millis(view.range_end_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let items = view
            .items
            .into_iter()
            .map(|item| {
                Ok(ProjectedCalendarItem {
                    evidence_handle: item.evidence_handle,
                    untrusted_title: item.untrusted_title,
                    starts_at: chrono::DateTime::from_timestamp_millis(item.starts_at_unix_ms)
                        .ok_or(AgentFailure::InvalidInput)?,
                    ends_at: chrono::DateTime::from_timestamp_millis(item.ends_at_unix_ms)
                        .ok_or(AgentFailure::InvalidInput)?,
                })
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        Ok(Some(ProjectedCalendarObservation {
            stamp: self.stamp(
                request.person_id,
                &backend
                    .admission(&CalendarReadAccessRequest {
                        person_id: request.person_id,
                        device_id: request.device_id.clone(),
                        provider: request.provider,
                        calendar_ids: request.calendar_ids.clone(),
                        expected_native_subject_fingerprint: None,
                        deadline: request.deadline,
                        cancellation: request.cancellation.clone(),
                    })
                    .await?,
            ),
            source_handle: view.source_handle,
            observed_at,
            expires_at,
            range_start,
            range_end,
            coverage_complete: view.coverage_complete,
            next_cursor: view.next_cursor,
            items,
        }))
    }
}

impl CalendarReadAdmission for RemoteCalendarAccess<'_> {
    async fn admission(
        &self,
        request: &CalendarReadAccessRequest,
    ) -> Result<Option<CalendarReadAccessAdmission>, AgentFailure> {
        let backend = self.backend.ok_or(AgentFailure::CapabilityUnavailable)?;
        Ok(Some(backend.admission(request).await?))
    }
}

/// A fixture source stands on no grant at all.
impl CalendarReadAdmission for FixtureAccess {}

/// A device read is admitted by the grant host that wraps it, not here.
impl CalendarReadAdmission for DeviceCalendarAccess<'_> {}

impl RemoteCalendarAccess<'_> {
    fn stamp(
        &self,
        person_id: PersonId,
        admission: &CalendarReadAccessAdmission,
    ) -> CalendarReadAccessStamp {
        CalendarReadAccessStamp {
            schema_version: floe_kernel::AGENT_VERSION,
            person_id,
            device_id: self.device_id.clone(),
            provider: self.provider,
            calendar_ids: self.calendar_ids.clone(),
            native_subject_fingerprint: format!(
                "remote:{}:{}",
                admission.source().source_authority().incarnation(),
                admission.source().source_authority().epoch().get()
            ),
            generation: format!(
                "remote:{}:{}",
                admission.source().source_authority().incarnation(),
                admission.source().source_authority().epoch().get()
            ),
        }
    }
}

struct FixtureAccess {
    device_id: String,
    calendar_ids: Vec<String>,
}

impl CalendarSource for FixtureAccess {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if request.deadline <= tokio::time::Instant::now() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        if request.device_id != self.device_id
            || request.provider != CalendarProvider::Fixture
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(CalendarReadAccessStamp {
            schema_version: floe_kernel::AGENT_VERSION,
            person_id: request.person_id,
            device_id: request.device_id,
            provider: request.provider,
            calendar_ids: request.calendar_ids,
            native_subject_fingerprint: "e".repeat(64),
            generation: "bounded-fixture".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FloeCore;
    use crate::local_context::{CalendarObservationPublication, LocalContextCommand};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use floe_agent_contract::{ModelPlacement, TransferConsent};
    use floe_context_contract::CalendarScope;
    use floe_context_contract::{
        ConnectorId, ExecutionOwnerId, GrantConsumer, GrantDataCategory, GrantOperation,
        GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle,
    };
    use floe_day::{CalendarSelection, CalendarSyncStatus};
    use floe_provider_adapters::sources::native_acquisition::CalendarAcquisitionResult;
    use floe_vault::{EncryptedAgentVault, VaultKey, VaultKeyProvider};
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use sha2::{Digest, Sha256};
    use std::{
        collections::{BTreeMap, HashMap},
        fs,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        os::unix::fs::PermissionsExt,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tempfile::tempdir;

    #[derive(Clone, Default)]
    struct RemoteCircuitKeys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for RemoteCircuitKeys {
        fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    fn read_http_request(stream: &mut TcpStream) -> Result<(String, Vec<u8>), String> {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        let (header_end, content_length) = loop {
            let read = stream
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("producer closed before request".into());
            }
            bytes.extend_from_slice(&buffer[..read]);
            let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                if bytes.len() > 64 * 1024 {
                    return Err("request headers too large".into());
                }
                continue;
            };
            let headers = String::from_utf8(bytes[..header_end].to_vec())
                .map_err(|_| "invalid request headers".to_owned())?;
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            break (header_end, content_length);
        };
        while bytes.len() < header_end + 4 + content_length {
            let read = stream
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("producer closed during request".into());
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
        let headers = String::from_utf8(bytes[..header_end].to_vec())
            .map_err(|_| "invalid request headers".to_owned())?;
        let path = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .ok_or_else(|| "missing request path".to_owned())?
            .to_owned();
        Ok((
            path,
            bytes[header_end + 4..header_end + 4 + content_length].to_vec(),
        ))
    }

    fn write_http_response(stream: &mut TcpStream, body: Vec<u8>) -> Result<(), String> {
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .map_err(|error| error.to_string())?;
        stream.write_all(&body).map_err(|error| error.to_string())
    }

    fn verify_owner_proof(
        body: &[u8],
        challenge: &[u8],
        expected_key_id: &str,
        owner_public_key: &str,
    ) -> Result<(), String> {
        let request: serde_json::Value =
            serde_json::from_slice(body).map_err(|error| error.to_string())?;
        let proof = request
            .get("proof")
            .ok_or_else(|| "missing owner proof".to_owned())?;
        let key_id = proof
            .get("key_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "missing owner key id".to_owned())?;
        if key_id != expected_key_id {
            return Err("empty owner key id".into());
        }
        let signature = proof
            .get("signature")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "missing owner signature".to_owned())?;
        let public_key = URL_SAFE_NO_PAD
            .decode(owner_public_key)
            .map_err(|_| "invalid owner public key".to_owned())?;
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| "invalid owner signature".to_owned())?;
        let mut signed = Vec::from(b"floe.remote.authorization.v1\0".as_slice());
        signed.extend_from_slice(challenge);
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key)
            .verify(&signed, &signature)
            .map_err(|_| "invalid owner proof".to_owned())
    }

    fn accept_with_timeout(
        listener: &TcpListener,
    ) -> Result<(TcpStream, std::net::SocketAddr), String> {
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match listener.accept() {
                Ok(connection) => return Ok(connection),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return Err("producer accept timeout".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    fn serve_signed_calendar_fixture(
        listener: TcpListener,
        producer_pkcs8: Vec<u8>,
        producer_instance: String,
        execution_owner: String,
        owner_key_id: String,
        person_id: PersonId,
        connection_id: String,
        source_authority: floe_context_contract::SourceAuthority,
        provider: CalendarProvider,
        grant: RemoteCalendarGrantBinding,
        owner_public_key: String,
        request_count: Arc<AtomicUsize>,
        request_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), String> {
        let producer = Ed25519KeyPair::from_pkcs8(&producer_pkcs8).map_err(|_| "producer key")?;
        let connector = match provider {
            CalendarProvider::Google => "calendar.google",
            CalendarProvider::Microsoft => "calendar.microsoft",
            _ => return Err("unsupported fixture provider".into()),
        };
        let audience = format!("floe.server:{producer_instance}");
        let key_id = "00000000-0000-4000-8000-000000000299";
        let public_key = URL_SAFE_NO_PAD.encode(producer.public_key().as_ref());
        let fingerprint = Sha256::digest(producer.public_key().as_ref())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut admission_id = String::new();
        let mut query_digest = String::new();
        let mut admission_challenge = Vec::new();
        let mut release_challenge = Vec::new();
        let view = serde_json::json!({
            "schema_version": 1,
            "view_id": "calendar.timeline",
            "source_handle": "calendar.timeline:fixture",
            "observed_at_unix_ms": request_time.timestamp_millis(),
            "expires_at_unix_ms": (request_time + chrono::Duration::minutes(2)).timestamp_millis(),
            "range_start_unix_ms": request_time.timestamp_millis(),
            "range_end_unix_ms": (request_time + chrono::Duration::hours(1)).timestamp_millis(),
            "coverage_complete": true,
            "items": [],
        });
        let view_bytes = serde_json::to_vec(&view).map_err(|error| error.to_string())?;
        let result_digest = Sha256::digest(&view_bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        for request_number in 0..5 {
            let (mut stream, _) = accept_with_timeout(&listener)?;
            stream
                .set_nonblocking(false)
                .map_err(|error| error.to_string())?;
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .map_err(|error| error.to_string())?;
            stream
                .set_write_timeout(Some(std::time::Duration::from_secs(10)))
                .map_err(|error| error.to_string())?;
            let (path, body) = read_http_request(&mut stream)?;
            request_count.fetch_add(1, Ordering::AcqRel);
            let now = chrono::Utc::now().timestamp_millis();
            let response = match (request_number, path.as_str()) {
                (0 | 4, "/v1/authority/calendar/source") => {
                    let descriptor = serde_json::json!({
                        "v": 1,
                        "operation": "calendar_source_preview",
                        "challenge_id": Uuid::new_v4().to_string(),
                        "nonce": URL_SAFE_NO_PAD.encode([8u8; 32]),
                        "person_id": person_id.to_string(),
                        "client_id": "fixture-client",
                        "device_id": "device-a",
                        "audience": audience,
                        "connector_id": connector,
                        "connection_id": connection_id,
                        "execution_owner": execution_owner,
                        "incarnation": source_authority.incarnation().to_string(),
                        "epoch": source_authority.epoch().get(),
                        "resource": "primary",
                        "provider_identity": "fixture:subject-a",
                        "issued_at_unix_ms": now,
                    });
                    let descriptor_bytes =
                        serde_json::to_vec(&descriptor).map_err(|error| error.to_string())?;
                    let mut signed = Vec::from(b"floe.remote.producer.v1\0".as_slice());
                    signed.extend_from_slice(&descriptor_bytes);
                    let signature = producer.sign(&signed);
                    serde_json::to_vec(&serde_json::json!({
                        "schema_version": 1,
                        "instance_id": producer_instance,
                        "execution_owner": execution_owner,
                        "audience": audience,
                        "key_id": key_id,
                        "public_key": public_key,
                        "fingerprint": fingerprint,
                        "descriptor_b64url": URL_SAFE_NO_PAD.encode(descriptor_bytes),
                        "producer_signature": URL_SAFE_NO_PAD.encode(signature.as_ref()),
                        "expires_at_unix_ms": now + 30_000,
                    }))
                    .map_err(|error| error.to_string())?
                }
                (1, "/v1/views/calendar.timeline/admit") => {
                    let request: serde_json::Value =
                        serde_json::from_slice(&body).map_err(|error| error.to_string())?;
                    let query = request
                        .get("query")
                        .ok_or_else(|| "missing query".to_owned())?;
                    let query_bytes =
                        serde_json::to_vec(query).map_err(|error| error.to_string())?;
                    query_digest = Sha256::digest(query_bytes)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>();
                    admission_id = Uuid::new_v4().to_string();
                    let challenge = serde_json::json!({
                        "v": 1,
                        "operation": "admission",
                        "challenge_id": admission_id,
                        "nonce": URL_SAFE_NO_PAD.encode([9u8; 32]),
                        "key_id": owner_key_id,
                        "person_id": person_id.to_string(),
                        "client_id": "fixture-client",
                        "device_id": "device-a",
                        "audience": audience,
                        "purpose": "everyday_assistance",
                        "consumer": "calendar.expert",
                        "policy": {"incarnation": grant.consumer_policy.incarnation().to_string(), "epoch": grant.consumer_policy.epoch().get()},
                        "source": {"connector_id": connector, "connection_id": connection_id, "execution_owner": execution_owner, "incarnation": source_authority.incarnation().to_string(), "epoch": source_authority.epoch().get()},
                        "grant": {"id": grant.grant.id().as_uuid().to_string(), "incarnation": grant.grant.authority().incarnation().to_string(), "epoch": grant.grant.authority().access_epoch().get()},
                        "resources": ["primary"],
                        "query_sha256": query_digest.clone(),
                        "max_items": 128,
                        "max_bytes": 65536,
                        "result_sha256": "",
                        "admission_id": "",
                        "issued_at_unix_ms": now,
                        "expires_at_unix_ms": now + 30_000,
                    });
                    let challenge_bytes =
                        serde_json::to_vec(&challenge).map_err(|error| error.to_string())?;
                    let mut signed = Vec::from(b"floe.remote.producer.v1\0".as_slice());
                    signed.extend_from_slice(&challenge_bytes);
                    let signature = producer.sign(&signed);
                    admission_challenge = challenge_bytes.clone();
                    serde_json::to_vec(&serde_json::json!({"schema_version": 1, "operation": "admission", "challenge_id": admission_id, "challenge_b64url": URL_SAFE_NO_PAD.encode(&challenge_bytes), "producer_signature": URL_SAFE_NO_PAD.encode(signature.as_ref()), "producer": {"schema_version": 1, "instance_id": producer_instance, "execution_owner": execution_owner, "audience": audience, "key_id": key_id, "public_key": public_key, "fingerprint": fingerprint}, "expires": now + 30_000})).map_err(|error| error.to_string())?
                }
                (2, "/v1/views/calendar.timeline/read") => {
                    verify_owner_proof(
                        &body,
                        &admission_challenge,
                        &owner_key_id,
                        &owner_public_key,
                    )?;
                    let challenge = serde_json::json!({
                        "v": 1,
                        "operation": "release",
                        "challenge_id": Uuid::new_v4().to_string(),
                        "nonce": URL_SAFE_NO_PAD.encode([10u8; 32]),
                        "key_id": owner_key_id,
                        "person_id": person_id.to_string(),
                        "client_id": "fixture-client",
                        "device_id": "device-a",
                        "audience": audience,
                        "purpose": "everyday_assistance",
                        "consumer": "calendar.expert",
                        "policy": {"incarnation": grant.consumer_policy.incarnation().to_string(), "epoch": grant.consumer_policy.epoch().get()},
                        "source": {"connector_id": connector, "connection_id": connection_id, "execution_owner": execution_owner, "incarnation": source_authority.incarnation().to_string(), "epoch": source_authority.epoch().get()},
                        "grant": {"id": grant.grant.id().as_uuid().to_string(), "incarnation": grant.grant.authority().incarnation().to_string(), "epoch": grant.grant.authority().access_epoch().get()},
                        "resources": ["primary"],
                        "query_sha256": query_digest,
                        "max_items": 128,
                        "max_bytes": 65536,
                        "result_sha256": result_digest,
                        "admission_id": admission_id,
                        "issued_at_unix_ms": now,
                        "expires_at_unix_ms": now + 30_000,
                    });
                    let challenge_bytes =
                        serde_json::to_vec(&challenge).map_err(|error| error.to_string())?;
                    let mut signed = Vec::from(b"floe.remote.producer.v1\0".as_slice());
                    signed.extend_from_slice(&challenge_bytes);
                    let signature = producer.sign(&signed);
                    release_challenge = challenge_bytes.clone();
                    serde_json::to_vec(&serde_json::json!({"schema_version": 1, "operation": "release", "challenge_id": challenge["challenge_id"], "challenge_b64url": URL_SAFE_NO_PAD.encode(&challenge_bytes), "producer_signature": URL_SAFE_NO_PAD.encode(signature.as_ref()), "producer": {"schema_version": 1, "instance_id": producer_instance, "execution_owner": execution_owner, "audience": audience, "key_id": key_id, "public_key": public_key, "fingerprint": fingerprint}, "expires": now + 30_000})).map_err(|error| error.to_string())?
                }
                (3, "/v1/views/calendar.timeline/release") => {
                    verify_owner_proof(
                        &body,
                        &release_challenge,
                        &owner_key_id,
                        &owner_public_key,
                    )?;
                    serde_json::to_vec(&serde_json::json!({
                        "schema_version": 1,
                        "view": view,
                    }))
                    .map_err(|error| error.to_string())?
                }
                _ => {
                    return Err(format!(
                        "unexpected producer request {request_number} {path}"
                    ));
                }
            };
            write_http_response(&mut stream, response)?;
        }
        Ok(())
    }

    fn active_identity(
        provider: CalendarProvider,
    ) -> (
        floe_experts::CalendarExpertSetupReceipt,
        floe_experts::CalendarViewBinding,
        CalendarConnection,
    ) {
        let setup_id = Uuid::new_v4();
        let view_handle = Uuid::new_v4();
        let source_authority = Some(floe_context_contract::SourceAuthority::new());
        let setup = floe_experts::CalendarExpertSetupReceipt {
            setup_id,
            person_id: PersonId::new(),
            expected_revision: 0,
            connection_scope: CalendarScope::Selected,
            connection_revision: 7,
            source_authority,
            reviewed_native_subject_fingerprint: Some("a".repeat(64)),
            view_handle,
            tool_installation_id: Uuid::new_v4(),
            expert_installation_id: Uuid::new_v4(),
            tool_assignment_id: Uuid::new_v4(),
            expert_assignment_id: Uuid::new_v4(),
        };
        let binding = floe_experts::CalendarViewBinding {
            handle: view_handle,
            person_id: setup.person_id,
            provider,
            device_id: "device-a".into(),
            calendar_ids: vec!["primary".into()],
            connection_scope: CalendarScope::Selected,
            connection_revision: 7,
            source_authority,
            enabled: true,
        };
        let connection = CalendarConnection {
            connection_id: setup_id.to_string(),
            device_id: binding.device_id.clone(),
            disconnected: false,
            scope: CalendarScope::Selected,
            provider,
            calendars: vec![CalendarSelection {
                calendar_id: "primary".into(),
                calendar_name: "Primary".into(),
            }],
            revision: 7,
            source_authority: source_authority.unwrap(),
            last_success_at: None,
            last_range: None,
            error: None,
            error_at: None,
            source_statuses: BTreeMap::<String, CalendarSyncStatus>::new(),
        };
        (setup, binding, connection)
    }

    fn observed_catalog(
        connector_id: &str,
        connection: &CalendarConnection,
    ) -> Vec<CalendarConnectionRef> {
        vec![CalendarConnectionRef {
            connector_id: connector_id.into(),
            connection_id: connection.connection_id.clone(),
            connection_revision: connection.revision,
        }]
    }

    #[tokio::test]
    async fn remote_calendar_backend_runs_signed_foundation_exchange_for_both_providers() {
        for provider in [CalendarProvider::Google, CalendarProvider::Microsoft] {
            let root = tempdir().unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let core = FloeCore::open(root.path().join("core.db")).await.unwrap();
            let person_id = PersonId::new();
            let connection_id = Uuid::new_v4().to_string();
            core.set_calendar_scope(
                person_id,
                connection_id.clone(),
                7,
                "device-a".into(),
                provider,
                vec![CalendarSelection {
                    calendar_id: "primary".into(),
                    calendar_name: "Primary".into(),
                }],
                CalendarScope::Selected,
            )
            .await
            .unwrap();
            let connection = core.calendar_connection(person_id).await.unwrap().unwrap();
            let keys = RemoteCircuitKeys::default();
            let vault = EncryptedAgentVault::create(root.path(), person_id, keys)
                .await
                .unwrap_or_else(|error| panic!("vault create failed: {error:?}"));
            let producer_pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
                .unwrap()
                .as_ref()
                .to_vec();
            let producer = Ed25519KeyPair::from_pkcs8(&producer_pkcs8).unwrap();
            let producer_instance = if provider == CalendarProvider::Google {
                "00000000-0000-4000-8000-000000000201"
            } else {
                "00000000-0000-4000-8000-000000000202"
            };
            let execution_owner = if provider == CalendarProvider::Google {
                "00000000-0000-4000-8000-000000000211"
            } else {
                "00000000-0000-4000-8000-000000000212"
            };
            let producer_identity = floe_vault::RemoteProducerIdentity {
                schema_version: 1,
                instance_id: producer_instance.into(),
                execution_owner: execution_owner.into(),
                audience: format!("floe.server:{producer_instance}"),
                key_id: "00000000-0000-4000-8000-000000000299".into(),
                public_key: URL_SAFE_NO_PAD.encode(producer.public_key().as_ref()),
                fingerprint: Sha256::digest(producer.public_key().as_ref())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            };
            vault.remote_pin_producer(producer_identity).await.unwrap();
            let producer_source_authority = floe_context_contract::SourceAuthority::new();
            assert_ne!(producer_source_authority, connection.source_authority);
            let source = GrantSourceBinding::try_new(
                person_id,
                floe_context_contract::ConnectionId::try_new(connection_id.clone()).unwrap(),
                ConnectorId::try_new(match provider {
                    CalendarProvider::Google => "calendar.google",
                    CalendarProvider::Microsoft => "calendar.microsoft",
                    _ => unreachable!(),
                })
                .unwrap(),
                ExecutionOwnerId::try_new(execution_owner).unwrap(),
                producer_source_authority,
            )
            .unwrap();
            let consumer = GrantConsumer::builtin("calendar.expert").unwrap();
            let scope = GrantScope::try_new(
                vec![ResourceHandle::try_new("primary").unwrap()],
                vec![GrantDataCategory::Content],
                vec![GrantOperation::Read],
                vec![GrantPurpose::Assistant],
                vec![consumer],
                ProcessingRestriction::LocalOnly,
            )
            .unwrap();
            let grant = vault
                .review_and_activate_remote_calendar_grant(
                    floe_context_contract::GrantId::new(),
                    None,
                    source,
                    scope,
                    None,
                )
                .await
                .unwrap();
            let grant_binding = vault
                .remote_calendar_grant_binding(
                    match provider {
                        CalendarProvider::Google => "calendar.google",
                        CalendarProvider::Microsoft => "calendar.microsoft",
                        _ => unreachable!(),
                    },
                    &connection_id,
                    producer_source_authority,
                    "primary",
                )
                .await
                .unwrap();
            let owner_key = vault.remote_owner_public_key().await.unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let request_count = Arc::new(AtomicUsize::new(0));
            let fixture_count = request_count.clone();
            let owner_key_id = owner_key.key_id.clone();
            let owner_public_key = owner_key.public_key.clone();
            let fixture_connection_id = connection_id.clone();
            let request_time = chrono::Utc::now();
            let server = std::thread::spawn(move || {
                serve_signed_calendar_fixture(
                    listener,
                    producer_pkcs8,
                    producer_instance.into(),
                    execution_owner.into(),
                    owner_key_id,
                    person_id,
                    fixture_connection_id,
                    producer_source_authority,
                    provider,
                    grant_binding,
                    owner_public_key,
                    request_count,
                    request_time,
                )
            });
            let source = ServerSourceClient::new(
                floe_provider_adapters::control::PreparedServerSource::from_parts(
                    &format!("http://127.0.0.1:{}/", address.port()),
                    "fixture-token-that-is-long-enough",
                    "fixture-client",
                    &person_id.to_string(),
                    "device-a",
                )
                .unwrap(),
            );
            let backend = VaultRemoteCalendarBackend::new(
                &vault,
                &core,
                &source,
                person_id,
                provider,
                vec!["primary".into()],
                7,
            )
            .unwrap();
            let access = RemoteCalendarAccess {
                backend: Some(&backend),
                provider,
                device_id: "device-a".into(),
                calendar_ids: vec!["primary".into()],
            };
            let projection = access
                .observe_projected(CalendarObserveRequest {
                    person_id,
                    device_id: "device-a".into(),
                    provider,
                    calendar_ids: vec!["primary".into()],
                    expected_native_subject_fingerprint: None,
                    starts_at: request_time,
                    ends_at: request_time + chrono::Duration::hours(1),
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(10),
                    cancellation: floe_execution::Cancellation::default(),
                })
                .await;
            let projection = match projection {
                Ok(projection) => projection.expect("remote projection"),
                Err(error) => panic!(
                    "remote calendar projection failed: {error:?}, requests: {}, producer: {:?}",
                    fixture_count.load(Ordering::Acquire),
                    server.join().unwrap(),
                ),
            };
            assert_eq!(projection.source_handle, "calendar.timeline:fixture");
            assert!(projection.coverage_complete);
            assert_eq!(fixture_count.load(Ordering::Acquire), 5);
            server.join().unwrap().unwrap();
            let _ = grant;
        }
    }

    #[test]
    fn device_schedule_refresh_does_not_change_authority() {
        let (setup, binding, mut connection) = active_identity(CalendarProvider::EventKit);
        connection.revision += 1;
        assert_eq!(
            validate_active_connection(&setup, &binding, &connection, "device-a", None),
            Ok(())
        );
    }

    #[test]
    fn legacy_schedule_model_never_consents_to_external_transfer() {
        // No pre-resolved recipient exists anymore: legacy model calls run
        // server-local only, so external transfer is never consented here.
        assert_eq!(
            external_transfer_consent(ModelPlacement::Remote),
            TransferConsent::NotGranted
        );
        assert_eq!(
            external_transfer_consent(ModelPlacement::DeviceLocal),
            TransferConsent::NotGranted
        );
    }

    #[test]
    fn device_schedule_accepts_only_explicit_current_authority_and_subset() {
        let (setup, binding, mut connection) = active_identity(CalendarProvider::EventKit);
        connection
            .calendars
            .extend((1..11).map(|index| CalendarSelection {
                calendar_id: format!("calendar-{index}"),
                calendar_name: format!("Calendar {index}"),
            }));
        assert_eq!(
            validate_active_connection(&setup, &binding, &connection, "device-a", None),
            Ok(())
        );
        let mut legacy = binding.clone();
        legacy.source_authority = None;
        assert_eq!(
            validate_active_connection(&setup, &legacy, &connection, "device-a", None),
            Err(AgentFailure::AccessReviewRequired)
        );
        connection.source_authority = connection.source_authority.advance().unwrap();
        assert_eq!(
            validate_active_connection(&setup, &binding, &connection, "device-a", None),
            Err(AgentFailure::AccessReviewRequired)
        );
        connection.source_authority = floe_context_contract::SourceAuthority::new();
        assert_eq!(
            validate_active_connection(&setup, &binding, &connection, "device-a", None),
            Err(AgentFailure::AccessReviewRequired)
        );
    }

    #[test]
    fn device_schedule_dispatch_requires_one_exact_active_identity() {
        for provider in [CalendarProvider::EventKit, CalendarProvider::Android] {
            let (setup, binding, connection) = active_identity(provider);
            assert_eq!(
                validate_active_connection(&setup, &binding, &connection, "device-a", None),
                Ok(())
            );

            let mut stale = connection.clone();
            stale.device_id = "device-b".into();
            assert_eq!(
                validate_active_connection(&setup, &binding, &stale, "device-a", None),
                Err(AgentFailure::StaleContext)
            );
            let mut stale = connection.clone();
            stale.calendars[0].calendar_id = "relabelled".into();
            assert_eq!(
                validate_active_connection(&setup, &binding, &stale, "device-a", None),
                Err(AgentFailure::StaleContext)
            );
            let mut stale = connection.clone();
            stale.scope = CalendarScope::All;
            assert_eq!(
                validate_active_connection(&setup, &binding, &stale, "device-a", None),
                Err(AgentFailure::StaleContext)
            );
            let mut all_setup = setup.clone();
            let mut all_binding = binding.clone();
            let mut all_connection = connection.clone();
            all_setup.connection_scope = CalendarScope::All;
            all_binding.connection_scope = CalendarScope::All;
            all_connection.scope = CalendarScope::All;
            assert_eq!(
                validate_active_connection(
                    &all_setup,
                    &all_binding,
                    &all_connection,
                    "device-a",
                    None,
                ),
                Ok(())
            );
            let mut stale = connection.clone();
            stale.revision = 0;
            assert_eq!(
                validate_active_connection(&setup, &binding, &stale, "device-a", None),
                Err(AgentFailure::StaleContext)
            );
            let mut stale_setup = setup.clone();
            stale_setup.setup_id = Uuid::new_v4();
            assert_eq!(
                validate_active_connection(&stale_setup, &binding, &connection, "device-a", None),
                Err(AgentFailure::StaleContext)
            );
            let mut stale = connection.clone();
            stale.provider = match provider {
                CalendarProvider::EventKit => CalendarProvider::Android,
                CalendarProvider::Android => CalendarProvider::EventKit,
                _ => unreachable!(),
            };
            assert_eq!(
                validate_active_connection(&setup, &binding, &stale, "device-a", None),
                Err(AgentFailure::StaleContext)
            );
            assert_eq!(
                validate_active_connection(&setup, &binding, &connection, "device-b", None),
                Err(AgentFailure::StaleContext)
            );
        }
    }

    #[tokio::test]
    async fn device_active_schedule_ignores_connected_server_calendar_routes() {
        for provider in [CalendarProvider::Android] {
            let (mut setup, mut binding, connection) = active_identity(provider);
            let directory = tempfile::tempdir().unwrap();
            let core = FloeCore::open(directory.path().join("calendar.db"))
                .await
                .unwrap();
            core.set_calendar_scope(
                setup.person_id,
                connection.connection_id.clone(),
                7,
                connection.device_id.clone(),
                provider,
                connection.calendars.clone(),
                connection.scope,
            )
            .await
            .unwrap();
            let connection = core
                .calendar_connection(setup.person_id)
                .await
                .unwrap()
                .unwrap();
            setup.source_authority = Some(connection.source_authority);
            binding.source_authority = Some(connection.source_authority);
            let mut catalog = observed_catalog("calendar.google", &connection);
            catalog[0].connection_id = Uuid::new_v4().to_string();
            catalog.push(floe_connections::CalendarConnectionRef {
                connector_id: "calendar.microsoft".into(),
                connection_id: Uuid::new_v4().to_string(),
                connection_revision: 12,
            });
            assert_eq!(
                validate_active_connection(
                    &setup,
                    &binding,
                    &connection,
                    "device-a",
                    Some(catalog.as_slice()),
                ),
                Ok(())
            );

            let store = LocalContextHost::default();
            let now = chrono::Utc::now().timestamp_millis();
            store
                .execute(
                    setup.person_id,
                    LocalContextCommand::PublishCalendarObservation {
                        device_id: "device-a".into(),
                        observation: Box::new(CalendarObservationPublication {
                            connection_id: connection.connection_id.clone(),
                            connection_revision: connection.revision,
                            provider,
                            calendar_ids: vec!["primary".into()],
                            observed_at_unix_ms: now,
                            expires_at_unix_ms: now + 240_000,
                            range_start_unix_ms: now - 60_000,
                            range_end_unix_ms: now + 60_000,
                            batches: vec![floe_day::CalendarBatch {
                                calendar_id: "primary".into(),
                                records: vec![],
                                failure: None,
                            }],
                        }),
                    },
                    Some(&connection),
                )
                .unwrap();
            let model = Model::for_stored_connection(
                None,
                &setup.person_id.to_string(),
                "device-a",
            )
            .unwrap();
            let access = Access::new(
                setup.person_id,
                provider,
                binding.device_id.clone(),
                binding.calendar_ids.clone(),
                connection.connection_id.clone(),
                connection.revision,
                &model,
                &store,
                &core,
                Some(connection.source_authority),
                None,
            );
            let result = access
                .check(CalendarReadAccessRequest {
                    person_id: setup.person_id,
                    device_id: "device-a".into(),
                    provider,
                    calendar_ids: vec!["primary".into()],
                    expected_native_subject_fingerprint: None,
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                    cancellation: floe_execution::Cancellation::default(),
                })
                .await;
            assert_eq!(result, Err(AgentFailure::CapabilityUnavailable));
        }
    }

    #[test]
    fn remote_schedule_dispatch_rejects_relabelled_or_stale_connection() {
        for (provider, connector_id) in [
            (CalendarProvider::Google, "calendar.google"),
            (CalendarProvider::Microsoft, "calendar.microsoft"),
        ] {
            let (setup, binding, connection) = active_identity(provider);
            let catalog = observed_catalog(connector_id, &connection);
            assert_eq!(
                validate_active_connection(
                    &setup,
                    &binding,
                    &connection,
                    "device-a",
                    Some(catalog.as_slice()),
                ),
                Ok(())
            );
            let mut multi_provider_catalog = catalog.clone();
            multi_provider_catalog.push(
                floe_connections::CalendarConnectionRef {
                    connector_id: match provider {
                        CalendarProvider::Google => "calendar.microsoft".into(),
                        CalendarProvider::Microsoft => "calendar.google".into(),
                        _ => unreachable!(),
                    },
                    connection_id: Uuid::new_v4().to_string(),
                    connection_revision: 3,
                },
            );
            assert_eq!(
                validate_active_connection(
                    &setup,
                    &binding,
                    &connection,
                    "device-a",
                    Some(multi_provider_catalog.as_slice()),
                ),
                Ok(())
            );

            let mut stale_catalog = catalog.clone();
            stale_catalog[0].connection_id = Uuid::new_v4().to_string();
            assert_eq!(
                validate_active_connection(
                    &setup,
                    &binding,
                    &connection,
                    "device-a",
                    Some(stale_catalog.as_slice()),
                ),
                Err(AgentFailure::StaleContext)
            );
            let mut stale_catalog = catalog.clone();
            stale_catalog[0].connection_revision += 1;
            assert_eq!(
                validate_active_connection(
                    &setup,
                    &binding,
                    &connection,
                    "device-a",
                    Some(stale_catalog.as_slice()),
                ),
                Err(AgentFailure::StaleContext)
            );
            let mut relabelled_catalog = catalog.clone();
            relabelled_catalog[0].connector_id = match provider {
                CalendarProvider::Google => "calendar.microsoft".into(),
                CalendarProvider::Microsoft => "calendar.google".into(),
                _ => unreachable!(),
            };
            assert_eq!(
                validate_active_connection(
                    &setup,
                    &binding,
                    &connection,
                    "device-a",
                    Some(relabelled_catalog.as_slice()),
                ),
                Err(AgentFailure::StaleContext)
            );
            let mut duplicate_catalog = catalog.clone();
            duplicate_catalog.push(catalog[0].clone());
            assert_eq!(
                validate_active_connection(
                    &setup,
                    &binding,
                    &connection,
                    "device-a",
                    Some(duplicate_catalog.as_slice()),
                ),
                Err(AgentFailure::StaleContext)
            );
        }
    }

    fn publish_device_calendar(
        store: &LocalContextHost,
        person_id: PersonId,
        connection: &CalendarConnection,
        observed_at_unix_ms: i64,
    ) {
        store
            .execute(
                person_id,
                LocalContextCommand::PublishCalendarObservation {
                    device_id: connection.device_id.clone(),
                    observation: Box::new(CalendarObservationPublication {
                        connection_id: connection.connection_id.clone(),
                        connection_revision: connection.revision,
                        provider: CalendarProvider::EventKit,
                        calendar_ids: vec!["primary".into()],
                        observed_at_unix_ms,
                        expires_at_unix_ms: observed_at_unix_ms + 240_000,
                        range_start_unix_ms: observed_at_unix_ms - 60_000,
                        range_end_unix_ms: observed_at_unix_ms + 60_000,
                        batches: vec![floe_day::CalendarBatch {
                            calendar_id: "primary".into(),
                            records: vec![],
                            failure: None,
                        }],
                    }),
                },
                Some(connection),
            )
            .unwrap();
    }

    #[tokio::test]
    async fn device_calendar_access_never_consumes_another_devices_observation() {
        let store = LocalContextHost::default();
        let person_id = PersonId::new();
        let now = chrono::Utc::now().timestamp_millis();
        let directory = tempfile::tempdir().unwrap();
        let core = FloeCore::open(directory.path().join("calendar.db"))
            .await
            .unwrap();
        core.set_calendar_scope(
            person_id,
            Uuid::new_v4().to_string(),
            7,
            "iphone".into(),
            CalendarProvider::EventKit,
            vec![CalendarSelection {
                calendar_id: "primary".into(),
                calendar_name: "Primary".into(),
            }],
            CalendarScope::Selected,
        )
        .await
        .unwrap();
        let connection = core.calendar_connection(person_id).await.unwrap().unwrap();
        publish_device_calendar(&store, person_id, &connection, now - 2);
        let mut other_device = connection.clone();
        other_device.device_id = "ipad".into();
        publish_device_calendar(&store, person_id, &other_device, now - 1);
        let access = DeviceCalendarAccess {
            core: &core,
            connection_id: connection.connection_id.clone(),
            source_authority: Some(connection.source_authority),
            local_context: &store,
            provider: CalendarProvider::EventKit,
            device_id: "iphone".into(),
            calendar_ids: vec!["primary".into()],
            connection_revision: connection.revision,
        };
        let request = |device_id: &str| CalendarReadAccessRequest {
            person_id,
            device_id: device_id.into(),
            provider: CalendarProvider::EventKit,
            calendar_ids: vec!["primary".into()],
            expected_native_subject_fingerprint: None,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            cancellation: floe_execution::Cancellation::default(),
        };

        assert_eq!(
            access.check(request("iphone")).await,
            Err(AgentFailure::CapabilityUnavailable)
        );
        assert_eq!(
            access.check(request("ipad")).await,
            Err(AgentFailure::CapabilityDenied)
        );
    }

    #[tokio::test]
    async fn device_calendar_access_rejects_a_stale_connection_revision() {
        let store = LocalContextHost::default();
        let person_id = PersonId::new();
        let directory = tempfile::tempdir().unwrap();
        let core = FloeCore::open(directory.path().join("calendar.db"))
            .await
            .unwrap();
        core.set_calendar_scope(
            person_id,
            Uuid::new_v4().to_string(),
            7,
            "android".into(),
            CalendarProvider::Android,
            vec![CalendarSelection {
                calendar_id: "primary".into(),
                calendar_name: "Primary".into(),
            }],
            CalendarScope::Selected,
        )
        .await
        .unwrap();
        let connection = core.calendar_connection(person_id).await.unwrap().unwrap();
        let access = DeviceCalendarAccess {
            core: &core,
            connection_id: connection.connection_id.clone(),
            source_authority: Some(connection.source_authority),
            local_context: &store,
            provider: CalendarProvider::Android,
            device_id: "android".into(),
            calendar_ids: vec!["primary".into()],
            connection_revision: connection.revision + 1,
        };

        assert_eq!(
            access
                .check(CalendarReadAccessRequest {
                    person_id,
                    device_id: "android".into(),
                    provider: CalendarProvider::Android,
                    calendar_ids: vec!["primary".into()],
                    expected_native_subject_fingerprint: None,
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                    cancellation: floe_execution::Cancellation::default(),
                })
                .await,
            Err(AgentFailure::StaleContext)
        );
    }

    #[tokio::test]
    async fn device_calendar_access_reads_through_the_bound_acquisition_host() {
        let store = LocalContextHost::default();
        let person_id = PersonId::new();
        let directory = tempfile::tempdir().unwrap();
        let core = FloeCore::open(directory.path().join("calendar.db"))
            .await
            .unwrap();
        core.set_calendar_scope(
            person_id,
            Uuid::new_v4().to_string(),
            7,
            "android".into(),
            CalendarProvider::Android,
            vec![CalendarSelection {
                calendar_id: "primary".into(),
                calendar_name: "Primary".into(),
            }],
            CalendarScope::Selected,
        )
        .await
        .unwrap();
        let connection = core.calendar_connection(person_id).await.unwrap().unwrap();
        store
            .execute(
                person_id,
                LocalContextCommand::RegisterAcquisitionHost {
                    host_epoch: "host-a".into(),
                },
                None,
            )
            .unwrap();
        let access = DeviceCalendarAccess {
            core: &core,
            connection_id: connection.connection_id.clone(),
            source_authority: Some(connection.source_authority),
            local_context: &store,
            provider: CalendarProvider::Android,
            device_id: "android".into(),
            calendar_ids: vec!["primary".into()],
            connection_revision: connection.revision,
        };
        let request = CalendarReadAccessRequest {
            person_id,
            device_id: "android".into(),
            provider: CalendarProvider::Android,
            calendar_ids: vec!["primary".into()],
            expected_native_subject_fingerprint: None,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(2),
            cancellation: floe_execution::Cancellation::default(),
        };
        let mut check = Box::pin(access.check(request));
        let result = loop {
            tokio::select! {
                result = &mut check => break result,
                _ = tokio::time::sleep(std::time::Duration::from_millis(1)) => {
                    let polled = store
                        .execute(
                            person_id,
                            LocalContextCommand::PollAcquisitions {
                                host_epoch: "host-a".into(),
                            },
                            None,
                        )
                        .unwrap();
                    if let Some(acquisition) = polled.acquisitions.into_iter().next() {
                        store
                            .execute(
                                person_id,
                                LocalContextCommand::CompleteAcquisition {
                                    host_epoch: "host-a".into(),
                                    result: Box::new(CalendarAcquisitionResult {
                                        request_id: acquisition.request_id,
                                        host_epoch: acquisition.host_epoch,
                                        person_id: acquisition.person_id,
                                        device_id: acquisition.device_id,
                                        connection_id: acquisition.connection_id,
                                        connection_revision: acquisition.connection_revision,
                                        provider: acquisition.provider,
                                        mode: acquisition.mode,
                                        calendar_ids: acquisition.calendar_ids,
                                        range_start_unix_ms: acquisition.range_start_unix_ms,
                                        range_end_unix_ms: acquisition.range_end_unix_ms,
                                        native_subject_fingerprint_before: "a".repeat(64),
                                        native_subject_fingerprint_after: "a".repeat(64),
                                        available_calendar_ids: vec!["primary".into()],
                                        permission_class: "full".into(),
                                        batches: vec![],
                                    }),
                                },
                                None,
                            )
                            .unwrap();
                    }
                }
            }
        };
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn server_calendar_access_without_owner_backend_fails_closed() {
        let access = RemoteCalendarAccess {
            backend: None,
            provider: CalendarProvider::Google,
            device_id: "test-device".into(),
            calendar_ids: vec!["primary".into()],
        };
        let now = chrono::Utc::now();
        let request = CalendarObserveRequest {
            person_id: PersonId::new(),
            device_id: "test-device".into(),
            provider: CalendarProvider::Google,
            calendar_ids: vec!["primary".into()],
            expected_native_subject_fingerprint: None,
            starts_at: now - chrono::Duration::minutes(1),
            ends_at: now + chrono::Duration::minutes(1),
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(2),
            cancellation: floe_execution::Cancellation::default(),
        };
        assert_eq!(
            access.observe_projected(request).await,
            Err(AgentFailure::CapabilityUnavailable)
        );
    }

    #[tokio::test]
    async fn device_only_and_wrong_provider_access_keep_distinct_failures() {
        let access = RemoteCalendarAccess {
            backend: None,
            provider: CalendarProvider::Android,
            device_id: "test-device".into(),
            calendar_ids: vec!["primary".into()],
        };
        let person_id = PersonId::new();
        let unavailable = access
            .check(CalendarReadAccessRequest {
                person_id,
                device_id: "test-device".into(),
                provider: CalendarProvider::Android,
                calendar_ids: vec!["primary".into()],
                expected_native_subject_fingerprint: None,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_execution::Cancellation::default(),
            })
            .await;
        assert_eq!(unavailable, Err(AgentFailure::CapabilityUnavailable));
        let denied = access
            .check(CalendarReadAccessRequest {
                person_id,
                device_id: "test-device".into(),
                provider: CalendarProvider::Google,
                calendar_ids: vec!["primary".into()],
                expected_native_subject_fingerprint: None,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_execution::Cancellation::default(),
            })
            .await;
        assert_eq!(denied, Err(AgentFailure::CapabilityDenied));
    }
}
