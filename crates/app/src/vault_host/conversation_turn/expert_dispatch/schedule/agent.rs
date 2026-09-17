use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};

use floe_access::{
    CalendarReadAccessAdmission, CalendarReadAccessRequest, CalendarReadAccessStamp,
    CalendarReadAdmission,
};
use floe_context::{
    CalendarMirrorReader, CalendarObservation, CalendarObserveRequest, CalendarSource,
    CalendarTimelineViews, GovernedDependencyResolver, ProjectedCalendarObservation,
};
use floe_actions::{
    CalendarAction, ExpertCalendarDestination, ExpertCalendarRequest, ExpertProposalReference,
};
use floe_agent_contract::{AgentFailure, CancelReason, ModelPlacement, SessionProtection};
// What the regressions below stand a task and a registry up with.
#[cfg(test)]
use floe_agent_contract::{TaskId, TaskSnapshot};
#[cfg(test)]
use floe_experts::RegistrySnapshot;
use floe_context::{
    AgentContext, FeasibilityView, InferencePolicyDecision, WellbeingView,
};
use floe_conversation::{
    AgentBudget, AgentCommand, AgentEvent, AgentMessage, AgentOutcome, AgentRuntime, AgentSession,
    CapabilityDescriptor, CapabilityHost, CapabilityInvocation, ModelRequest, ModelResponse,
    ModelRunner, SessionStore, UsageLedger,
};
use floe_day::CalendarTimelineGrant;
use floe_execution::Cancellation;
use floe_experts::{
    A2AArtifact, A2AMessageRole, A2APart, A2ARouter, A2ASendMessageRequest, A2ATaskState,
    EXPERT_RESULT_MEDIA_TYPE, A2ATask, AgentCard, AgentRegistry, ExpertBudget, ExpertInput,
    ExpertInvocation, ExpertResult, InProcessA2ATransport, InProcessAgent,
};
use floe_experts_builtin::BuiltinExpertKind;
use floe_experts_builtin::schedule::{CalendarHistoryBoundary, ExpertHost};
use floe_kernel::AGENT_VERSION;
use floe_vault::{
    CalendarGrantAdmission, ContextEvidenceReader, EncryptedAgentVault, VaultKeyProvider,
};

use crate::FloeCore;
use floe_context_contract::{ContextDependency, DependencyCoverage, GrantConsumer, GrantOperation, GrantPurpose, ProcessingRestriction};
use floe_context_contract::CalendarProvider;
use floe_kernel::PersonId;
use tokio::time::Instant;
use uuid::Uuid;


pub struct CalendarAgentTurnRequest {
    pub command: AgentCommand,
    pub context: AgentContext,
    pub policy: InferencePolicyDecision,
    pub budget: AgentBudget,
    pub grant: CalendarTimelineGrant,
    pub assignment_id: Uuid,
    pub feasibility: Option<FeasibilityView>,
    pub wellbeing: Option<WellbeingView>,
    pub destination: Option<ExpertCalendarDestination>,
    pub propose_focus: bool,
    pub cancellation: Cancellation,
    pub continuation: bool,
}

pub struct CalendarAgentProposal {
    pub reference: ExpertProposalReference,
    pub result: Result<CalendarAction, AgentFailure>,
}

pub struct CalendarAgentTurnResult {
    pub session: AgentSession,
    pub proposals: Vec<CalendarAgentProposal>,
}

pub struct CalendarExpertEndpointRequest {
    pub person_id: PersonId,
    pub usage: UsageLedger,
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

pub const CALENDAR_EXPERT_SETTLEMENT_OWNER: &str = "floe.builtin.schedule/v1";

/// The Schedule Expert settles through the common Task settlement.
pub type CalendarExpertSettlement = floe_experts::ExpertSettlement;


impl FloeCore {
    pub async fn run_calendar_expert_endpoint<
        Keys: VaultKeyProvider,
        Access: CalendarSource + CalendarReadAdmission,
        Model: floe_inference::ModelTransport + Sync,
        Clock: Fn() -> DateTime<Utc> + Sync + Copy,
    >(
        &self,
        vault: &EncryptedAgentVault<Keys>,
        access: &Access,
        model: &Model,
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
            || !request
                .policy
                .allowed_placements
                .contains(&model.placement())
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let guarded_access = GrantBoundCalendarAccess {
            core: self,
            vault,
            access,
            grant: request.grant.clone(),
            grant_pin: Mutex::new(None),
            remote_processing: model.placement() == ModelPlacement::Remote,
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
                placement: model.placement(),
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
        // This run's model attempts are charged to the ledger it carries.
        let reasoner = crate::vault_host::conversation_turn::ExpertModelHost {
            model,
            usage: request.usage,
        };
        let report = ExpertHost {
            assignments: &assignments,
            views: &ExpertTimelineViews(&views),
        }
        .invoke_with_model(invocation, &reasoner, &request.policy)
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
                CALENDAR_EXPERT_SETTLEMENT_OWNER,
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

    pub async fn run_calendar_agent_turn<
        Keys: VaultKeyProvider,
        Access: CalendarSource + CalendarReadAdmission + Send,
        Model: floe_inference::ModelTransport + Sync,
        Clock: Fn() -> DateTime<Utc> + Sync + Copy + Send,
    >(
        &self,
        vault: &EncryptedAgentVault<Keys>,
        access: &Access,
        model: &Model,
        request: CalendarAgentTurnRequest,
        clock: Clock,
        emit: impl FnMut(AgentEvent) + Send,
    ) -> Result<CalendarAgentTurnResult, AgentFailure> {
        let parent = request.cancellation.clone();
        let child = parent.child_scope();
        let _cancel = CancelTurn(child.clone());
        let mut request = request;
        request.cancellation = child.clone();
        let operation = Box::pin(async {
            validate_budget(request.budget)?;
            if request.command.person_id != request.grant.person_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            let saved = vault
                .load(request.command.person_id, request.command.session_id)
                .await?;
            if request.continuation && floe_conversation::carries_source_history(&saved.messages, &CalendarHistoryBoundary) {
                return Err(AgentFailure::StaleContext);
            }
            let effective_budget = if request.continuation {
                let level = saved
                    .continuation
                    .ok_or(AgentFailure::InvalidInput)?
                    .level
                    .checked_add(1)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                request
                    .budget
                    .expanded(level)
                    .ok_or(AgentFailure::BudgetExceeded)?
            } else {
                request.budget
            };
            let deadline = Instant::now() + Duration::from_millis(effective_budget.deadline_ms);
            check_running(deadline, &request.cancellation)?;
            floe_conversation::admit_unscoped_session(&saved)?;
            let guarded_access = GrantBoundCalendarAccess {
                core: self,
                vault,
                access,
                grant: request.grant.clone(),
                grant_pin: Mutex::new(None),
                remote_processing: model.placement() == ModelPlacement::Remote,
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
                _ = tokio::time::sleep_until(deadline) => return Err(AgentFailure::DeadlineExceeded),
                result = vault.expert_registry() => result?.ok_or(AgentFailure::CapabilityDenied)?,
            };
            let registry = AgentRegistry::restore(snapshot, vault.registry_instance_id())?;
            let admitted = registry.admit_registered_expert_invocation(calendar_invocation(
                views.grant(),
                request.assignment_id,
                None,
                None,
            ))?;
            let revision = admitted.revision;
            let card = admitted.card;
            if !request
                .policy
                .data_classes
                .contains(&views.grant().data_class())
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut expert_context = request.context.clone();
            floe_context::prepare_expert_context(
                &mut expert_context,
                &self.store,
                floe_context::ExpertContextRequest {
                    person_id: request.command.person_id,
                    policy: &request.policy,
                    placement: model.placement(),
                    protection: vault.protection(),
                    now: clock(),
                    deadline,
                    cancellation: request.cancellation.clone(),
                },
            )
            .await?;
            let turn = CalendarTurn {
                vault,
                views,
                model,
                policy: request.policy,
                registry: Mutex::new(registry),
                revision: AtomicU64::new(revision),
                assignment_id: request.assignment_id,
                propose_focus: request.propose_focus,
                card,
                expert_context,
                historical_dependencies: Mutex::new(Vec::new()),
                deadline,
                cancellation: request.cancellation,
            };
            let guarded_model = CalendarModel { turn: &turn, model };
            let transport = InProcessA2ATransport::new(&turn);
            let router = A2ARouter::new(&transport);
            let runtime = AgentRuntime {
                store: &turn,
                capabilities: &turn,
                model: &guarded_model,
                policy: &turn.policy,
                budget: request.budget,
            };
            let session = if request.continuation {
                Box::pin(runtime.continue_turn_with_agents(
                    request.command.person_id,
                    request.command.session_id,
                    request.command.expected_revision,
                    request.context,
                    &router,
                    turn.cancellation.clone(),
                    emit,
                ))
                .await?
            } else {
                Box::pin(runtime.run_turn_with_agents(
                    request.command,
                    request.context,
                    &router,
                    turn.cancellation.clone(),
                    emit,
                ))
                .await?
            };
            let mut proposals = vec![];
            if session.last_outcome == Some(AgentOutcome::Completed)
                && let Some(destination) = request.destination
            {
                let current_turn =
                    session
                        .messages
                        .iter()
                        .rev()
                        .find_map(|message| match message {
                            AgentMessage::User { turn_id, .. } => Some(*turn_id),
                            _ => None,
                        });
                for message in &session.messages {
                    let AgentMessage::Delegation { turn_id, task } = message else {
                        continue;
                    };
                    if Some(*turn_id) != current_turn {
                        continue;
                    }
                    let Some(output) = task.data_part(EXPERT_RESULT_MEDIA_TYPE) else {
                        continue;
                    };
                    let evidence: ExpertResult =
                        serde_json::from_str(output).map_err(|_| AgentFailure::InvalidInput)?;
                    if evidence.action_proposals.is_empty() {
                        continue;
                    }
                    let reference = ExpertProposalReference {
                        person_id: session.person_id,
                        session_id: session.id,
                        invocation_id: task.id,
                    };
                    let result = async {
                        turn.validate().await?;
                        let action = self
                            .prepare_expert_calendar_action(
                                vault,
                                ExpertCalendarRequest {
                                    reference: reference.clone(),
                                    destination: ExpertCalendarDestination {
                                        provider: destination.provider,
                                        calendar_id: destination.calendar_id.clone(),
                                        connection_revision: destination.connection_revision,
                                        timezone: destination.timezone.clone(),
                                    },
                                    cancellation: turn.cancellation.clone(),
                                    deadline,
                                },
                                clock,
                            )
                            .await?;
                        turn.check_running()?;
                        Ok(action)
                    }
                    .await;
                    proposals.push(CalendarAgentProposal { reference, result });
                }
            }
            Ok(CalendarAgentTurnResult { session, proposals })
        });
        tokio::pin!(operation);
        tokio::select! {
            biased;
            _ = parent.cancelled() => operation.await,
            result = &mut operation => result,
        }
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

#[cfg(test)]
fn append_schedule_context(
    context: &mut AgentContext,
    feasibility: Option<&FeasibilityView>,
    wellbeing: Option<&WellbeingView>,
    now: DateTime<Utc>,
    calendar_expires_at: DateTime<Utc>,
) -> Result<(), AgentFailure> {
    let now_unix_ms = now.timestamp_millis();
    let status_expires_at = u64::try_from(calendar_expires_at.timestamp_millis())
        .map_err(|_| AgentFailure::InvalidInput)?;
    match feasibility {
        Some(view) if validate_feasibility_view(view, now_unix_ms).is_ok() => {
            context.evidence.push(personal_context_evidence(view)?);
        }
        Some(view) => context.evidence.push(context_status_evidence(
            FEASIBILITY_VIEW_ID,
            if view.expires_at_unix_ms <= now_unix_ms {
                "stale"
            } else {
                "unavailable"
            },
            status_expires_at,
        )?),
        None => context.evidence.push(context_status_evidence(
            FEASIBILITY_VIEW_ID,
            "unavailable",
            status_expires_at,
        )?),
    }
    match wellbeing {
        Some(view) if validate_wellbeing_view(view, now_unix_ms).is_ok() => {
            context.evidence.push(personal_context_evidence(view)?);
        }
        Some(view) => context.evidence.push(context_status_evidence(
            WELLBEING_VIEW_ID,
            if view.expires_at_unix_ms <= now_unix_ms {
                "stale"
            } else {
                "unavailable"
            },
            status_expires_at,
        )?),
        None => context.evidence.push(context_status_evidence(
            WELLBEING_VIEW_ID,
            "unavailable",
            status_expires_at,
        )?),
    }
    Ok(())
}

#[cfg(test)]
fn context_status_evidence(
    view_id: &str,
    state: &str,
    expires_at_unix_ms: u64,
) -> Result<ContextEvidence, AgentFailure> {
    Ok(ContextEvidence {
        source_handle: format!("floe.context-status:{view_id}"),
        data_class: DataClass::Personal,
        untrusted_text: serde_json::to_string(&serde_json::json!({
            "view_id": view_id,
            "state": state,
        }))
        .map_err(|_| AgentFailure::InvalidInput)?,
        expires_at_unix_ms,
    })
}

struct CancelTurn(Cancellation);

impl Drop for CancelTurn {
    fn drop(&mut self) {
        self.0.cancel_with_reason(CancelReason::OwnerDropped);
    }
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
        let expected_connector = floe_access::hosted_calendar_connector(self.grant.provider)
            .ok_or(AgentFailure::CapabilityDenied)?;
        if admission.person_id() != self.grant.person_id
            || admission.source().connector().as_str() != expected_connector
            || admission.scope().resources().iter().any(|resource| {
                !self
                    .grant
                    .calendar_ids
                    .iter()
                    .any(|calendar_id| calendar_id == resource.as_str())
            })
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        match admission.processing() {
            ProcessingRestriction::LocalOnly if self.remote_processing => {
                return Err(AgentFailure::PolicyDenied);
            }
            ProcessingRestriction::ApprovedRecipient { .. } if !self.remote_processing => {
                return Err(AgentFailure::PolicyDenied);
            }
            ProcessingRestriction::LocalOnly | ProcessingRestriction::ApprovedRecipient { .. } => {}
        }
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
        if self.remote_processing {
            return Err(AgentFailure::CapabilityDenied);
        }
        if request.person_id != self.grant.person_id
            || request.provider != self.grant.provider
            || request.device_id != self.grant.device_id
            || request
                .calendar_ids
                .iter()
                .any(|calendar_id| !self.grant.calendar_ids.contains(calendar_id))
        {
            return Err(AgentFailure::CapabilityDenied);
        }
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

struct CalendarTurn<'host, Keys, Access, Clock, Model> {
    vault: &'host EncryptedAgentVault<Keys>,
    views: CalendarTimelineViews<'host, Access, floe_vault::TursoStore, Clock>,
    model: &'host Model,
    policy: InferencePolicyDecision,
    registry: Mutex<AgentRegistry>,
    revision: AtomicU64,
    assignment_id: Uuid,
    propose_focus: bool,
    card: AgentCard,
    expert_context: AgentContext,
    historical_dependencies: Mutex<Vec<ContextDependency>>,
    deadline: Instant,
    cancellation: Cancellation,
}

impl<
    Keys: VaultKeyProvider,
    Access: CalendarSource + CalendarReadAdmission + Send,
    Clock: Fn() -> DateTime<Utc> + Sync + Send,
    Model: Sync,
> CalendarTurn<'_, Keys, Access, Clock, Model>
{
    fn check_running(&self) -> Result<(), AgentFailure> {
        check_running(self.deadline, &self.cancellation)
    }

    fn coverage_for_message(
        &self,
        message: &AgentMessage,
    ) -> Result<(Uuid, DependencyCoverage), AgentFailure> {
        let turn_id = message.turn_id();
        if turn_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut dependencies = self.views.consumed_context_dependencies()?;
        dependencies.extend(
            self.historical_dependencies
                .lock()
                .map_err(|_| AgentFailure::CapabilityUnavailable)?
                .iter()
                .cloned(),
        );
        let coverage =
            floe_context::message_coverage(dependencies, self.views.source_was_observed())?;
        Ok((turn_id, coverage))
    }

    fn retain_historical_dependencies(
        &self,
        dependencies: impl IntoIterator<Item = ContextDependency>,
    ) -> Result<(), AgentFailure> {
        let mut retained = self
            .historical_dependencies
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        for dependency in dependencies {
            if !retained.contains(&dependency) {
                retained.push(dependency);
            }
        }
        Ok(())
    }

    async fn revalidate_historical_dependencies(&self) -> Result<(), AgentFailure> {
        let dependencies = self
            .historical_dependencies
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .clone();
        let resolver = GovernedDependencyResolver::new(&self.views);
        for dependency in dependencies {
            resolver
                .resolve(&dependency, self.deadline, self.cancellation.clone())
                .await?;
        }
        Ok(())
    }

    fn validate_historical_dependency_liveness(&self) -> Result<(), AgentFailure> {
        let dependencies = self
            .historical_dependencies
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .clone();
        for dependency in dependencies {
            self.views.validate_dependency_liveness(&dependency)?;
        }
        Ok(())
    }

    /// The transcript this attempt may still see.
    ///
    /// Context decides what each recorded turn authorizes and Conversation
    /// applies that to the transcript; what is Schedule's own is the boundary —
    /// a turn carrying a calendar result it can no longer re-admit is not
    /// independent, whatever its recorded coverage claims.
    async fn project_history(&self, request: &mut ModelRequest) -> Result<(), AgentFailure> {
        let current_turn = request.turn_id;
        let resolver = GovernedDependencyResolver::new(&self.views);
        let reader = ContextEvidenceReader::new(self.vault, request.session_id);
        let mut decisions = floe_context::project_history(
            &reader,
            request.session_id,
            request
                .messages
                .iter()
                .map(AgentMessage::turn_id)
                .filter(|turn_id| *turn_id != current_turn),
            Some(&resolver),
            &floe_access::DependencyAuthorization {
                allowed_placements: request.policy.allowed_placements.clone(),
                deadline: self.deadline,
                cancellation: self.cancellation.clone(),
            },
        )
        .await?;
        floe_conversation::narrow_by_source_boundary(
            &mut decisions,
            &request.messages,
            &floe_experts_builtin::schedule::CalendarHistoryBoundary,
        );
        floe_conversation::project_history_into(
            request,
            floe_conversation::HistoryProjection {
                decisions: &decisions,
                current_turn: Some(current_turn),
            },
            |_, dependencies| self.retain_historical_dependencies(dependencies.to_vec()),
        )?;
        Ok(())
    }

    async fn validate(&self) -> Result<(), AgentFailure> {
        self.check_running()?;
        let validation = async {
            let snapshot = self
                .vault
                .expert_registry()
                .await?
                .ok_or(AgentFailure::CapabilityDenied)?;
            AgentRegistry::restore(snapshot, self.vault.registry_instance_id())?
                .still_admits_registered_expert_invocation(calendar_invocation(
                    self.views.grant(),
                    self.assignment_id,
                    None,
                    None,
                ))?;
            self.views
                .revalidate(self.deadline, self.cancellation.clone())
                .await?;
            self.vault.check_access()?;
            self.check_running()
        };
        tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(self.deadline) => Err(AgentFailure::DeadlineExceeded),
            result = validation => result,
        }
    }
}

impl<
    Keys: VaultKeyProvider,
    Access: CalendarSource + CalendarReadAdmission + Send,
    Clock: Fn() -> DateTime<Utc> + Sync + Send,
    Model: Sync,
> SessionStore for CalendarTurn<'_, Keys, Access, Clock, Model>
{
    fn protection(&self) -> SessionProtection {
        self.vault.protection()
    }

    async fn load(
        &self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> Result<AgentSession, AgentFailure> {
        self.vault.load(person_id, session_id).await
    }

    async fn compare_and_swap(
        &self,
        session: &AgentSession,
        previous_revision: u64,
    ) -> Result<(), AgentFailure> {
        let previous = self.vault.load(session.person_id, session.id).await?;
        let appended = session.messages.get(previous.messages.len());
        let dependent = appended.is_some_and(AgentMessage::may_derive_from_source);
        if appended.is_some() {
            self.check_running()?;
            if self.views.source_denial_requires_halt() {
                return Err(AgentFailure::CapabilityDenied);
            }
        }
        let coverage = appended
            .map(|message| self.coverage_for_message(message))
            .transpose()?;
        let staged = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .snapshot();
        let committed = if let Some((coverage_turn, coverage)) = coverage {
            self.vault
                .commit_expert_session_scoped_with_coverage_hook(
                    session,
                    previous_revision,
                    self.revision.load(Ordering::Acquire),
                    &staged,
                    self.assignment_id,
                    self.views.grant().handle,
                    coverage_turn,
                    coverage,
                    async {
                        if dependent {
                            self.validate_historical_dependency_liveness()?;
                            self.views
                                .revalidate(self.deadline, self.cancellation.clone())
                                .await?;
                            self.check_running()?;
                        }
                        Ok(())
                    },
                )
                .await
        } else {
            self.vault
                .commit_expert_session_scoped_with_hook(
                    session,
                    previous_revision,
                    self.revision.load(Ordering::Acquire),
                    &staged,
                    self.assignment_id,
                    self.views.grant().handle,
                    async {
                        if dependent {
                            self.validate_historical_dependency_liveness()?;
                            self.views
                                .revalidate(self.deadline, self.cancellation.clone())
                                .await?;
                            self.check_running()?;
                        }
                        Ok(())
                    },
                )
                .await
        };
        let committed = match committed {
            Ok(committed) => committed,
            Err(failure) => {
                if let Some(snapshot) = self.vault.expert_registry().await? {
                    let revision = snapshot.revision;
                    *self
                        .registry
                        .lock()
                        .map_err(|_| AgentFailure::StorageUnavailable)? =
                        AgentRegistry::restore(snapshot, self.vault.registry_instance_id())?;
                    self.revision.store(revision, Ordering::Release);
                }
                return Err(failure);
            }
        };
        let revision = committed.revision;
        *self
            .registry
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)? =
            AgentRegistry::restore(committed, self.vault.registry_instance_id())?;
        self.revision.store(revision, Ordering::Release);
        Ok(())
    }
}

impl<
    Keys: VaultKeyProvider,
    Access: CalendarSource + CalendarReadAdmission + Send,
    Clock: Fn() -> DateTime<Utc> + Sync + Send,
    Model: floe_inference::ModelTransport + Sync,
> CapabilityHost for CalendarTurn<'_, Keys, Access, Clock, Model>
{
    fn descriptors(&self, _: PersonId) -> Vec<CapabilityDescriptor> {
        vec![]
    }

    async fn invoke(&self, _: CapabilityInvocation) -> Result<String, AgentFailure> {
        Err(AgentFailure::CapabilityDenied)
    }
}

impl<
    Keys: VaultKeyProvider,
    Access: CalendarSource + CalendarReadAdmission + Send,
    Clock: Fn() -> DateTime<Utc> + Sync + Send,
    Model: floe_inference::ModelTransport + Sync,
> InProcessAgent for CalendarTurn<'_, Keys, Access, Clock, Model>
{
    fn agent_cards(&self, person_id: PersonId) -> Vec<AgentCard> {
        if person_id == self.views.grant().person_id {
            vec![self.card.clone()]
        } else {
            vec![]
        }
    }

    async fn handle_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> Result<A2ATask, AgentFailure> {
        if request.schema_version != AGENT_VERSION
            || request.person_id != self.views.grant().person_id
            || request.agent_id != self.card.id
            || request.message.role != A2AMessageRole::User
            || request.message.task_id.is_none()
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let task_id = request.message.task_id.ok_or(AgentFailure::InvalidInput)?;
        let assignment = request.message.text()?.to_owned();
        self.validate().await?;
        let assignments = floe_experts::RegistryAssignments::new(&self.registry);
        // This message's model attempts are charged to the ledger it carries.
        let model = super::super::super::ExpertModelHost {
            model: self.model,
            usage: request.usage.clone(),
        };
        let result = ExpertHost {
            assignments: &assignments,
            views: &ExpertTimelineViews(&self.views),
        }
        .invoke_with_model(
            ExpertInvocation {
                capabilities: std::sync::Arc::new(NoCapabilityJournal),
                context: self.expert_context.clone(),
                schema_version: request.schema_version,
                invocation_id: task_id,
                instance_id: self.vault.registry_instance_id(),
                person_id: request.person_id,
                assignment_id: self.assignment_id,
                expected_registry_revision: self.revision.load(Ordering::Acquire),
                granted_view_handles: vec![self.views.grant().handle],
                allowed_data_classes: vec![self.views.grant().data_class()],
                current_time_unix_ms: u64::try_from(self.views.current_time().timestamp_millis())
                    .map_err(|_| AgentFailure::InvalidInput)?,
                timezone_offset_seconds: self.views.grant().day.timezone_offset_seconds,
                suggested_range_start_unix_ms: Some(
                    u64::try_from(self.views.grant().starts_at.timestamp_millis())
                        .map_err(|_| AgentFailure::InvalidInput)?,
                ),
                suggested_range_end_unix_ms: Some(
                    u64::try_from(self.views.grant().ends_at.timestamp_millis())
                        .map_err(|_| AgentFailure::InvalidInput)?,
                ),
                input: if self.propose_focus {
                    ExpertInput::ProposeFocus { focus_minutes: 60 }
                } else {
                    ExpertInput::Analyze {
                        request: assignment,
                        focus_minutes: None,
                    }
                },
                budget: ExpertBudget {
                    max_output_bytes: request.max_output_bytes,
                    ..ExpertBudget::default()
                },
                deadline: request.deadline.min(self.deadline),
                cancellation: request.cancellation,
            },
            &model,
            &self.policy,
        )
        .await?;
        let summary = result
            .summary
            .clone()
            .ok_or(AgentFailure::InvalidModelOutput)?;
        let data = serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?;
        Ok(A2ATask {
            id: task_id,
            context_id: request.message.context_id,
            agent_id: request.agent_id,
            state: A2ATaskState::Completed,
            history: vec![request.message],
            artifacts: vec![A2AArtifact {
                artifact_id: Uuid::new_v4(),
                name: "Schedule expert result".into(),
                parts: vec![
                    A2APart::Text { text: summary },
                    A2APart::Data {
                        media_type: EXPERT_RESULT_MEDIA_TYPE.into(),
                        data,
                    },
                ],
            }],
            failure: None,
        })
    }
}

struct CalendarModel<'model, 'host, Keys, Access, Clock, Model> {
    turn: &'model CalendarTurn<'host, Keys, Access, Clock, Model>,
    model: &'model Model,
}

impl<
    Keys: VaultKeyProvider,
    Access: CalendarSource + CalendarReadAdmission + Send,
    Clock: Fn() -> DateTime<Utc> + Sync + Send,
    Model: floe_inference::ModelTransport + Sync,
> ModelRunner for CalendarModel<'_, '_, Keys, Access, Clock, Model>
{
    fn history_start(
        &self,
        messages: &[AgentMessage],
        current_turn: Uuid,
        max_bytes: usize,
    ) -> Result<usize, AgentFailure> {
        floe_conversation::bounded_source_history_start(
            messages,
            current_turn,
            max_bytes,
            &CalendarHistoryBoundary,
        )
    }

    fn placement(&self) -> ModelPlacement {
        self.model.placement()
    }

    async fn generate(&self, mut request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        self.turn.validate().await?;
        self.turn.project_history(&mut request).await?;
        for message in &mut request.messages {
            if let AgentMessage::Capability {
                turn_id, result, ..
            } = message
                && *turn_id != request.turn_id
                && result.is_ok()
            {
                *result = Err(AgentFailure::StaleContext);
            }
        }
        request.deadline = request.deadline.min(self.turn.deadline);
        let runner = floe_conversation::TransportModelRunner::new(self.model);
        let response = tokio::select! {
            biased;
            _ = self.turn.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(self.turn.deadline) => return Err(AgentFailure::DeadlineExceeded),
            result = runner.generate(request) => result?,
        };
        if self.turn.views.source_denial_requires_halt() {
            return Err(AgentFailure::CapabilityDenied);
        }
        self.turn.validate().await?;
        self.turn.revalidate_historical_dependencies().await?;
        Ok(response)
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

fn validate_budget(budget: AgentBudget) -> Result<(), AgentFailure> {
    let maximum = AgentBudget::default();
    if budget.deadline_ms == 0
        || budget.deadline_ms > maximum.deadline_ms
        || budget.max_iterations > maximum.max_iterations
        || budget.max_capability_calls > maximum.max_capability_calls
        || budget.max_tokens > maximum.max_tokens
        || budget.max_cost_micros > maximum.max_cost_micros
        || budget.max_output_bytes > maximum.max_output_bytes
        || budget.max_context_bytes > maximum.max_context_bytes
        || budget.max_session_bytes > maximum.max_session_bytes
    {
        return Err(AgentFailure::BudgetExceeded);
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
