use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use floe_agent::*;
use floe_domain::PersonId;
use tokio::time::Instant;
use uuid::Uuid;

use crate::*;

pub struct CalendarAgentTurnRequest {
    pub command: AgentCommand,
    pub context: AgentContext,
    pub policy: InferencePolicyDecision,
    pub budget: AgentBudget,
    pub grant: CalendarTimelineGrant,
    pub assignment_id: Uuid,
    pub destination: Option<ExpertCalendarDestination>,
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

impl FloeCore {
    pub async fn run_calendar_agent_turn<
        Keys: VaultKeyProvider,
        Access: CalendarReadAccess,
        Model: ModelRunner + Sync,
        Clock: Fn() -> DateTime<Utc> + Sync + Copy,
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
        let child = Cancellation::default();
        let _cancel = CancelTurn(child.clone());
        let mut request = request;
        request.cancellation = child.clone();
        let operation = async {
            validate_budget(request.budget)?;
            if request.command.person_id != request.grant.person_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            let saved = vault
                .load(request.command.person_id, request.command.session_id)
                .await?;
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
            if let Some(scope) = saved.scope {
                let setup = vault.calendar_session_setup(&saved).await?;
                if setup.expert_assignment_id != request.assignment_id
                    || setup.view_handle != request.grant.handle
                    || scope.data_class() != request.grant.data_class()
                {
                    return Err(AgentFailure::CapabilityDenied);
                }
            }
            let views = CalendarTimelineViews::new(self, access, request.grant, clock)?;
            vault.check_access()?;
            let snapshot = tokio::select! {
                biased;
                _ = request.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
                _ = tokio::time::sleep_until(deadline) => return Err(AgentFailure::DeadlineExceeded),
                result = vault.expert_registry() => result?.ok_or(AgentFailure::CapabilityDenied)?,
            };
            let registry = AgentRegistry::restore(snapshot, vault.registry_instance_id())?;
            let revision = registry.revision();
            let binding = registry.calendar_view(views.grant().person_id, views.grant().handle)?;
            let mut calendars = views.grant().calendar_ids.clone();
            calendars.sort();
            if binding.provider != views.grant().provider || binding.calendar_ids != calendars {
                return Err(AgentFailure::CapabilityDenied);
            }
            let descriptor = registry.expert_descriptor(
                views.grant().person_id,
                request.assignment_id,
                revision,
                views.grant().handle,
            )?;
            if descriptor.output_data_class != views.grant().data_class()
                || !request
                    .policy
                    .data_classes
                    .contains(&descriptor.output_data_class)
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let turn = CalendarTurn {
                vault,
                views,
                model,
                policy: request.policy,
                registry: Mutex::new(registry),
                revision: AtomicU64::new(revision),
                assignment_id: request.assignment_id,
                descriptor,
                deadline,
                cancellation: request.cancellation,
                parent: parent.clone(),
            };
            let guarded_model = CalendarModel { turn: &turn, model };
            let runtime = AgentRuntime {
                store: &turn,
                capabilities: &turn,
                model: &guarded_model,
                policy: &turn.policy,
                budget: request.budget,
            };
            let session = if request.continuation {
                runtime
                    .continue_turn(
                        request.command.person_id,
                        request.command.session_id,
                        request.command.expected_revision,
                        request.context,
                        turn.cancellation.clone(),
                        emit,
                    )
                    .await?
            } else {
                runtime
                    .run_turn(
                        request.command,
                        request.context,
                        turn.cancellation.clone(),
                        emit,
                    )
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
                    let AgentMessage::Capability {
                        turn_id,
                        call_id,
                        result: Ok(output),
                        ..
                    } = message
                    else {
                        continue;
                    };
                    if Some(*turn_id) != current_turn {
                        continue;
                    }
                    let evidence: ExpertResult =
                        serde_json::from_str(output).map_err(|_| AgentFailure::InvalidInput)?;
                    if evidence.action_proposals.is_empty() {
                        continue;
                    }
                    let reference = ExpertProposalReference {
                        person_id: session.person_id,
                        session_id: session.id,
                        invocation_id: *call_id,
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
        };
        tokio::pin!(operation);
        tokio::select! {
            biased;
            _ = parent.cancelled() => {
                child.cancel();
                operation.await
            }
            result = &mut operation => result,
        }
    }
}

struct CancelTurn(Cancellation);

impl Drop for CancelTurn {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

struct CalendarTurn<'host, Keys, Access, Clock, Model> {
    vault: &'host EncryptedAgentVault<Keys>,
    views: CalendarTimelineViews<'host, Access, Clock>,
    model: &'host Model,
    policy: InferencePolicyDecision,
    registry: Mutex<AgentRegistry>,
    revision: AtomicU64,
    assignment_id: Uuid,
    descriptor: CapabilityDescriptor,
    deadline: Instant,
    cancellation: Cancellation,
    parent: Cancellation,
}

impl<
    Keys: VaultKeyProvider,
    Access: CalendarReadAccess,
    Clock: Fn() -> DateTime<Utc> + Sync,
    Model: Sync,
> CalendarTurn<'_, Keys, Access, Clock, Model>
{
    fn check_running(&self) -> Result<(), AgentFailure> {
        if self.parent.is_cancelled() {
            self.cancellation.cancel();
        }
        check_running(self.deadline, &self.cancellation)
    }

    async fn validate(&self) -> Result<(), AgentFailure> {
        self.check_running()?;
        let validation = async {
            let registry = self
                .vault
                .expert_registry()
                .await?
                .ok_or(AgentFailure::CapabilityDenied)?;
            if registry.revision != self.revision.load(Ordering::Acquire) {
                return Err(AgentFailure::Conflict);
            }
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
    Access: CalendarReadAccess,
    Clock: Fn() -> DateTime<Utc> + Sync,
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
        let dependent = matches!(
            appended,
            Some(AgentMessage::Assistant { .. } | AgentMessage::Capability { result: Ok(_), .. })
        );
        if appended.is_some() {
            self.check_running()?;
        }
        let staged = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .snapshot();
        let committed = self
            .vault
            .commit_expert_session_with_hook(
                session,
                previous_revision,
                self.revision.load(Ordering::Acquire),
                &staged,
                async {
                    if dependent {
                        self.views
                            .revalidate(self.deadline, self.cancellation.clone())
                            .await?;
                        self.check_running()?;
                    }
                    Ok(())
                },
            )
            .await?;
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
    Access: CalendarReadAccess,
    Clock: Fn() -> DateTime<Utc> + Sync,
    Model: ModelRunner + Sync,
> CapabilityHost for CalendarTurn<'_, Keys, Access, Clock, Model>
{
    fn descriptors(&self, person_id: PersonId) -> Vec<CapabilityDescriptor> {
        if person_id == self.views.grant().person_id {
            vec![self.descriptor.clone()]
        } else {
            vec![]
        }
    }

    async fn invoke(&self, invocation: CapabilityInvocation) -> Result<String, AgentFailure> {
        if invocation.person_id != self.views.grant().person_id
            || invocation.capability_id != self.descriptor.id
            || invocation.input.len() > 1024
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let input: ExpertInput =
            serde_json::from_str(&invocation.input).map_err(|_| AgentFailure::InvalidInput)?;
        self.validate().await?;
        let result = ExpertHost {
            registry: &self.registry,
            views: &self.views,
        }
        .invoke_with_model(
            ExpertInvocation {
                schema_version: invocation.schema_version,
                invocation_id: invocation.call_id,
                instance_id: self.vault.registry_instance_id(),
                person_id: invocation.person_id,
                assignment_id: self.assignment_id,
                expected_registry_revision: self.revision.load(Ordering::Acquire),
                granted_view_handles: vec![self.views.grant().handle],
                allowed_data_classes: vec![self.descriptor.output_data_class],
                input,
                budget: ExpertBudget {
                    max_output_bytes: invocation.max_output_bytes,
                    max_model_calls: 2,
                    max_tool_calls: 1,
                    ..ExpertBudget::default()
                },
                deadline: invocation.deadline.min(self.deadline),
                cancellation: invocation.cancellation,
            },
            self.model,
            &self.policy,
        )
        .await?;
        serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)
    }
}

struct CalendarModel<'model, 'host, Keys, Access, Clock, Model> {
    turn: &'model CalendarTurn<'host, Keys, Access, Clock, Model>,
    model: &'model Model,
}

impl<
    Keys: VaultKeyProvider,
    Access: CalendarReadAccess,
    Clock: Fn() -> DateTime<Utc> + Sync,
    Model: ModelRunner + Sync,
> ModelRunner for CalendarModel<'_, '_, Keys, Access, Clock, Model>
{
    fn placement(&self) -> ModelPlacement {
        self.model.placement()
    }

    async fn generate(&self, mut request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        self.turn.validate().await?;
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
        let response = tokio::select! {
            biased;
            _ = self.turn.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(self.turn.deadline) => return Err(AgentFailure::DeadlineExceeded),
            result = self.model.generate(request) => result?,
        };
        self.turn.validate().await?;
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
