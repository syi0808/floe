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
            if saved.scope.is_some() {
                return Err(AgentFailure::PolicyDenied);
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
            let card = registry.expert_card(
                views.grant().person_id,
                request.assignment_id,
                revision,
                views.grant().handle,
            )?;
            if !request
                .policy
                .data_classes
                .contains(&views.grant().data_class())
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
                propose_focus: request.propose_focus,
                card,
                deadline,
                cancellation: request.cancellation,
                parent: parent.clone(),
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
                runtime
                    .continue_turn_with_agents(
                        request.command.person_id,
                        request.command.session_id,
                        request.command.expected_revision,
                        request.context,
                        &router,
                        turn.cancellation.clone(),
                        emit,
                    )
                    .await?
            } else {
                runtime
                    .run_turn_with_agents(
                        request.command,
                        request.context,
                        &router,
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
    propose_focus: bool,
    card: AgentCard,
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
        ) || matches!(
            appended,
            Some(AgentMessage::Delegation { task, .. })
                if task.state == A2ATaskState::Completed
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
            .await;
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
    Access: CalendarReadAccess,
    Clock: Fn() -> DateTime<Utc> + Sync,
    Model: ModelRunner + Sync,
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
    Access: CalendarReadAccess,
    Clock: Fn() -> DateTime<Utc> + Sync,
    Model: ModelRunner + Sync,
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
        let result = ExpertHost {
            registry: &self.registry,
            views: &self.views,
        }
        .invoke_with_model(
            ExpertInvocation {
                usage: request.usage.clone(),
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
            self.model,
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
