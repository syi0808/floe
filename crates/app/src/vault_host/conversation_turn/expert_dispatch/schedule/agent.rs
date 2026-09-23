use std::sync::Mutex;

use chrono::{DateTime, Utc};
use floe_access::{
    CalendarReadAccessAdmission, CalendarReadAccessRequest, CalendarReadAccessStamp,
    CalendarReadAdmission,
};
use floe_agent_contract::{AgentFailure, ModelPlacement};
use floe_context::{
    AgentContext, CalendarMirrorReader, CalendarObservation, CalendarObserveRequest,
    CalendarSource, CalendarTimelineViews, InferencePolicyDecision, ProjectedCalendarObservation,
};
use floe_context_contract::{
    CalendarProvider, ContextDependency, GrantConsumer, GrantOperation, GrantPurpose,
    ProcessingRestriction,
};
use floe_conversation::{AgentBudget, SessionStore};
use floe_day::CalendarTimelineGrant;
use floe_execution::Cancellation;
use floe_experts::{AgentRegistry, ExpertBudget, ExpertInput, ExpertInvocation, ExpertResult};
use floe_experts_builtin::{
    BuiltinExpertKind,
    schedule::{ExpertHost, ScheduleExecutionIntent},
};
use floe_kernel::{AGENT_VERSION, PersonId};
use floe_vault::{CalendarGrantAdmission, EncryptedAgentVault, VaultKeyProvider};
use tokio::time::Instant;
use uuid::Uuid;

use super::super::super::expert_host::ExpertModelHost;
use crate::FloeCore;

pub struct CalendarExpertEndpointRequest {
    pub person_id: PersonId,
    pub intent: ScheduleExecutionIntent,
    pub context: AgentContext,
    pub policy: InferencePolicyDecision,
    pub grant: CalendarTimelineGrant,
    pub assignment_id: Uuid,
    pub invocation_id: Uuid,
    pub assignment: String,
    pub propose_focus: bool,
    pub max_output_bytes: usize,
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

pub struct CalendarExpertEndpointResult {
    pub report: ExpertResult,
    pub dependencies: Vec<ContextDependency>,
    pub settlement: CalendarExpertSettlement,
}

/// The Schedule Expert settles through the common Task settlement.
pub type CalendarExpertSettlement = floe_experts::ExpertSettlement;

impl FloeCore {
    pub async fn run_calendar_expert_endpoint<
        Keys: VaultKeyProvider,
        Access: CalendarSource + CalendarReadAdmission,
        Clock: Fn() -> DateTime<Utc> + Sync + Copy,
    >(
        &self,
        vault: &EncryptedAgentVault<Keys>,
        access: &Access,
        executor: &dyn floe_inference::InferenceExecutor,
        scope: &floe_execution::ExecutionScope,
        request: CalendarExpertEndpointRequest,
        clock: Clock,
    ) -> Result<CalendarExpertEndpointResult, AgentFailure> {
        check_running(request.deadline, &request.cancellation)?;
        if request.person_id != vault.person_id()
            || request.person_id != request.grant.person_id
            || request.assignment.trim().is_empty()
            || request.assignment.len() > 2048
            || request.max_output_bytes == 0
            || request.max_output_bytes > AgentBudget::default().max_output_bytes
        {
            return Err(AgentFailure::PolicyDenied);
        }
        // Inference selects the profile per the run's intent and fences the
        // dispatch; the endpoint only states the class. The legacy models all
        // reported device-local placement — the candidate server route was
        // always server-local — so the endpoint authorizes device-local
        // context and reads sources without assuming remote processing,
        // exactly as before. Any external dispatch is fenced by Access at
        // dispatch time under exact-recipient consent.
        let guarded_access = GrantBoundCalendarAccess {
            core: self,
            vault,
            access,
            grant: request.grant.clone(),
            grant_pin: Mutex::new(None),
            remote_processing: false,
        };
        let views = CalendarTimelineViews::new(
            &self.lease_registry,
            &self.store,
            &guarded_access,
            request.grant,
            clock,
        )?;
        vault.check_access()?;
        let snapshot = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(request.deadline) => return Err(AgentFailure::DeadlineExceeded),
            result = vault.expert_registry() => result?.ok_or(AgentFailure::CapabilityDenied)?,
        };
        let registry = AgentRegistry::restore(snapshot, vault.registry_instance_id())?;
        let admitted = registry.admit_registered_expert_invocation(calendar_invocation(
            views.grant(),
            request.assignment_id,
            Some(request.invocation_id),
            Some(&schedule_expert()?),
        ))?;
        let revision = admitted.revision;
        if !request
            .policy
            .data_classes
            .contains(&views.grant().data_class())
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut expert_context = request.context;
        floe_context::prepare_expert_context(
            &mut expert_context,
            &self.store,
            floe_context::ExpertContextRequest {
                person_id: views.grant().person_id,
                policy: &request.policy,
                placement: ModelPlacement::DeviceLocal,
                protection: vault.protection(),
                now: clock(),
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            },
        )
        .await?;
        let registry = Mutex::new(registry);
        let invocation = ExpertInvocation {
            capabilities: std::sync::Arc::new(NoCapabilityJournal),
            context: expert_context,
            schema_version: AGENT_VERSION,
            invocation_id: request.invocation_id,
            instance_id: vault.registry_instance_id(),
            person_id: request.person_id,
            assignment_id: request.assignment_id,
            expected_registry_revision: revision,
            granted_view_handles: vec![views.grant().handle],
            allowed_data_classes: vec![views.grant().data_class()],
            current_time_unix_ms: u64::try_from(views.current_time().timestamp_millis())
                .map_err(|_| AgentFailure::InvalidInput)?,
            timezone_offset_seconds: views.grant().day.timezone_offset_seconds,
            suggested_range_start_unix_ms: Some(
                u64::try_from(views.grant().starts_at.timestamp_millis())
                    .map_err(|_| AgentFailure::InvalidInput)?,
            ),
            suggested_range_end_unix_ms: Some(
                u64::try_from(views.grant().ends_at.timestamp_millis())
                    .map_err(|_| AgentFailure::InvalidInput)?,
            ),
            input: if request.propose_focus {
                ExpertInput::ProposeFocus { focus_minutes: 60 }
            } else {
                ExpertInput::Analyze {
                    request: request.assignment,
                    focus_minutes: None,
                }
            },
            budget: ExpertBudget {
                max_output_bytes: request.max_output_bytes,
                ..ExpertBudget::default()
            },
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        let assignments = floe_experts::RegistryAssignments::new(&registry);
        // This run's model attempts settle against a bounded child of the
        // Task scope; its source reads are captured for the exact dispatch
        // coverage. Both bindings last exactly as long as this run.
        let captured = Mutex::new(Vec::new());
        let reasoner = ExpertModelHost {
            executor,
            scope,
            captured: &captured,
        };
        let report = ExpertHost {
            assignments: &assignments,
            views: &ExpertTimelineViews(&views),
        }
        .invoke_with_model(invocation, &reasoner, &request.policy, request.intent)
        .await?;
        check_running(request.deadline, &request.cancellation)?;
        views
            .revalidate(request.deadline, request.cancellation.clone())
            .await?;
        vault.check_access()?;
        let dependencies = views.consumed_context_dependencies()?;
        check_running(request.deadline, &request.cancellation)?;
        for dependency in &dependencies {
            views.validate_dependency_liveness(dependency)?;
        }
        let task_result =
            serde_json::to_string(&report).map_err(|_| AgentFailure::InvalidModelOutput)?;
        let settlement = registry
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .settle_registered_expert_invocation(
                floe_experts_builtin::BuiltinExpertKind::Schedule.package_id(),
                revision,
                request.assignment_id,
                request.invocation_id,
                dependencies.clone(),
                task_result,
            );
        Ok(CalendarExpertEndpointResult {
            report,
            dependencies,
            settlement,
        })
    }
}

/// The invocation a calendar-scoped Expert run asks the registry to admit.
fn calendar_invocation<'a>(
    grant: &'a CalendarTimelineGrant,
    assignment_id: Uuid,
    invocation_id: Option<Uuid>,
    required_builtin: Option<&'a floe_experts::AgentId>,
) -> floe_experts::RegisteredExpertInvocation<'a> {
    floe_experts::RegisteredExpertInvocation {
        view: floe_experts::CalendarViewClaim {
            person_id: grant.person_id,
            handle: grant.handle,
            provider: grant.provider,
            device_id: &grant.device_id,
            calendar_ids: &grant.calendar_ids,
        },
        assignment_id,
        invocation_id,
        required_builtin,
    }
}

/// The only Expert a Schedule endpoint invocation may be answered by.
fn schedule_expert() -> Result<floe_experts::AgentId, AgentFailure> {
    floe_experts::AgentId::try_new(BuiltinExpertKind::Schedule.package_id())
        .ok_or(AgentFailure::CapabilityDenied)
}

struct GrantBoundCalendarAccess<'host, Keys, Access> {
    core: &'host FloeCore,
    vault: &'host EncryptedAgentVault<Keys>,
    access: &'host Access,
    grant: CalendarTimelineGrant,
    grant_pin: Mutex<
        Option<(
            floe_context_contract::GrantAuthority,
            floe_context_contract::ConsumerPolicyAuthority,
        )>,
    >,
    remote_processing: bool,
}

impl<Keys: VaultKeyProvider, Access: CalendarSource + CalendarReadAdmission>
    GrantBoundCalendarAccess<'_, Keys, Access>
{
    async fn authorize_remote(
        &self,
        request: &CalendarReadAccessRequest,
        stamp: &CalendarReadAccessStamp,
    ) -> Result<Option<CalendarReadAccessAdmission>, AgentFailure> {
        if matches!(
            self.grant.provider,
            CalendarProvider::EventKit | CalendarProvider::Android
        ) {
            return Ok(None);
        }
        if self.grant.provider == CalendarProvider::Fixture {
            return Ok(None);
        }
        let admission = self
            .access
            .admission_after_check(request, stamp)
            .await?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        floe_access::admits_calendar_read(
            &admission,
            self.grant.person_id,
            floe_access::hosted_calendar_connector(self.grant.provider)
                .ok_or(AgentFailure::CapabilityDenied)?,
            &self.grant.calendar_ids,
        )?;
        floe_access::admits_processing(admission.processing(), self.remote_processing)?;
        Ok(Some(admission))
    }

    async fn authorize_native(
        &self,
        request: &CalendarReadAccessRequest,
        native_subject_fingerprint: Option<&str>,
    ) -> Result<Option<CalendarGrantAdmission>, AgentFailure> {
        if !matches!(
            self.grant.provider,
            CalendarProvider::EventKit | CalendarProvider::Android
        ) {
            return Ok(None);
        }
        // A calendar this device answers for itself is never read into a model
        // somewhere else.
        if self.remote_processing {
            return Err(AgentFailure::CapabilityDenied);
        }
        floe_access::admits_calendar_read_request(
            request,
            self.grant.person_id,
            self.grant.provider,
            &self.grant.device_id,
            &self.grant.calendar_ids,
        )?;
        let snapshot = self
            .vault
            .expert_registry()
            .await?
            .ok_or(AgentFailure::CapabilityDenied)?;
        let binding = AgentRegistry::restore(snapshot, self.vault.registry_instance_id())?
            .calendar_source_binding(self.grant.person_id, self.grant.handle)?;
        let connection = self
            .core
            .calendar_connection(self.grant.person_id)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        let calendar_ids: Vec<_> = connection
            .calendars
            .iter()
            .map(|calendar| calendar.calendar_id.clone())
            .collect();
        let source_authority = floe_access::native_calendar_source_current(
            floe_access::NativeCalendarConnection {
                connection_id: &connection.connection_id,
                device_id: &connection.device_id,
                disconnected: connection.disconnected,
                provider: connection.provider,
                scope: connection.scope,
                source_authority: connection.source_authority,
                calendar_ids: &calendar_ids,
            },
            floe_access::NativeCalendarReview {
                device_id: &self.grant.device_id,
                provider: self.grant.provider,
                scope: binding.connection_scope,
                calendar_ids: &self.grant.calendar_ids,
                source_authority: binding.source_authority,
                native_subject_fingerprint: None,
                connection_id: None,
            },
        )?;
        let admission = self
            .vault
            .authorize_calendar_grant(
                binding.setup_id,
                binding.view_handle,
                &connection.connection_id,
                self.grant.provider,
                &self.grant.device_id,
                &self.grant.calendar_ids,
                source_authority,
                GrantOperation::Read,
                GrantPurpose::Assistant,
                GrantConsumer::builtin(floe_access::CALENDAR_EXPERT_CONSUMER)
                    .map_err(|_| AgentFailure::CapabilityDenied)?,
                ProcessingRestriction::LocalOnly,
                native_subject_fingerprint,
            )
            .await?;
        let grant_pin = self
            .grant_pin
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        if grant_pin.is_some_and(|(authority, policy)| {
            authority != admission.authority || policy != admission.consumer_policy
        }) {
            return Err(AgentFailure::StaleContext);
        }
        Ok(Some(admission))
    }

    fn pin_native(&self, candidate: Option<&CalendarGrantAdmission>) -> Result<(), AgentFailure> {
        let Some(candidate) = candidate else {
            return Ok(());
        };
        let mut grant_pin = self
            .grant_pin
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        if grant_pin.is_some_and(|(authority, policy)| {
            authority != candidate.authority || policy != candidate.consumer_policy
        }) {
            return Err(AgentFailure::StaleContext);
        }
        *grant_pin = Some((candidate.authority, candidate.consumer_policy));
        Ok(())
    }
}

impl<Keys: VaultKeyProvider, Access: CalendarSource + CalendarReadAdmission> CalendarSource
    for GrantBoundCalendarAccess<'_, Keys, Access>
{
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        let stamp = self.access.check(request.clone()).await?;
        if matches!(
            self.grant.provider,
            CalendarProvider::EventKit | CalendarProvider::Android
        ) {
            self.authorize_native(&request, Some(&stamp.native_subject_fingerprint))
                .await?;
        } else {
            self.authorize_remote(&request, &stamp).await?;
        }
        Ok(stamp)
    }

    async fn observe(
        &self,
        mut request: CalendarObserveRequest,
    ) -> Result<Option<CalendarObservation>, AgentFailure> {
        request.expected_native_subject_fingerprint = None;
        let access_request = CalendarReadAccessRequest {
            person_id: request.person_id,
            device_id: request.device_id.clone(),
            provider: request.provider,
            calendar_ids: request.calendar_ids.clone(),
            expected_native_subject_fingerprint: None,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        let stamp = self.access.check(access_request.clone()).await?;
        let candidate = self
            .authorize_native(&access_request, Some(&stamp.native_subject_fingerprint))
            .await?;
        request.expected_native_subject_fingerprint = Some(stamp.native_subject_fingerprint);
        let observed = self.access.observe(request).await?;
        if observed.is_some() {
            self.pin_native(candidate.as_ref())?;
        }
        Ok(observed)
    }

    async fn observe_projected(
        &self,
        mut request: CalendarObserveRequest,
    ) -> Result<Option<ProjectedCalendarObservation>, AgentFailure> {
        request.expected_native_subject_fingerprint = None;
        let access_request = CalendarReadAccessRequest {
            person_id: request.person_id,
            device_id: request.device_id.clone(),
            provider: request.provider,
            calendar_ids: request.calendar_ids.clone(),
            expected_native_subject_fingerprint: None,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        let stamp = self.access.check(access_request.clone()).await?;
        let candidate = self
            .authorize_native(&access_request, Some(&stamp.native_subject_fingerprint))
            .await?;
        request.expected_native_subject_fingerprint = Some(stamp.native_subject_fingerprint);
        let observed = self.access.observe_projected(request).await?;
        if observed.is_some() {
            self.pin_native(candidate.as_ref())?;
        }
        Ok(observed)
    }
}

impl<Keys: VaultKeyProvider, Access: CalendarSource + CalendarReadAdmission> CalendarReadAdmission
    for GrantBoundCalendarAccess<'_, Keys, Access>
{
    async fn admission(
        &self,
        request: &CalendarReadAccessRequest,
    ) -> Result<Option<CalendarReadAccessAdmission>, AgentFailure> {
        let stamp = self.access.check(request.clone()).await?;
        self.admission_after_check(request, &stamp).await
    }

    async fn admission_after_check(
        &self,
        request: &CalendarReadAccessRequest,
        stamp: &CalendarReadAccessStamp,
    ) -> Result<Option<CalendarReadAccessAdmission>, AgentFailure> {
        if matches!(
            self.grant.provider,
            CalendarProvider::EventKit | CalendarProvider::Android
        ) {
            let consumer = GrantConsumer::builtin("calendar.expert")
                .map_err(|_| AgentFailure::CapabilityDenied)?;
            return Ok(self
                .authorize_native(request, Some(&stamp.native_subject_fingerprint))
                .await?
                .map(|admission| {
                    CalendarReadAccessAdmission::device_local(
                        admission.source.person_id(),
                        admission.grant_id,
                        admission.authority,
                        admission.source,
                        admission.scope,
                        admission.consumer_policy,
                        consumer,
                    )
                }));
        } else {
            return self.authorize_remote(request, stamp).await;
        }
    }
}

fn check_running(deadline: Instant, cancellation: &Cancellation) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

/// A delegated Schedule turn records its own capability calls through the Task
/// it is settling, not through the caller's Session.
struct NoCapabilityJournal;

impl floe_agent_contract::CapabilityJournal for NoCapabilityJournal {
    fn record<'a>(
        &'a self,
        _record: floe_agent_contract::CapabilityExecution,
    ) -> floe_agent_contract::BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { Ok(()) })
    }
}

/// The Expert's view port, satisfied by the acquisition Context performs.
///
/// The Expert asks for a bounded timeline; Context decides what it is entitled
/// to and returns it. This only names the boundary between the two.
pub(crate) struct ExpertTimelineViews<'a, Access, Mirror, Clock>(
    pub(crate) &'a CalendarTimelineViews<'a, Access, Mirror, Clock>,
);

impl<
    Access: CalendarSource + CalendarReadAdmission,
    Mirror: CalendarMirrorReader,
    Clock: Fn() -> DateTime<Utc> + Sync,
> floe_experts_builtin::schedule::ExpertViews for ExpertTimelineViews<'_, Access, Mirror, Clock>
{
    async fn timeline(
        &self,
        request: floe_agent_contract::TimelineViewRead,
    ) -> Result<floe_agent_contract::ExpertTimelineView, AgentFailure> {
        self.0.timeline(request).await
    }
}
