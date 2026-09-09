use std::{future::Future, time::SystemTime};

use tokio::{
    sync::watch,
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::*;

#[derive(Clone)]
pub struct Cancellation(watch::Sender<bool>);

impl Default for Cancellation {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}

impl Cancellation {
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    pub async fn cancelled(&self) {
        let mut receiver = self.0.subscribe();
        while !*receiver.borrow_and_update() {
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
}

pub struct AgentRuntime<'runtime, Store, Model, Host> {
    pub store: &'runtime Store,
    pub model: &'runtime Model,
    pub capabilities: &'runtime Host,
    pub policy: &'runtime InferencePolicyDecision,
    pub budget: AgentBudget,
}

impl<Store: SessionStore, Model: ModelRunner, Host: CapabilityHost>
    AgentRuntime<'_, Store, Model, Host>
{
    pub async fn run_turn(
        &self,
        command: AgentCommand,
        context: AgentContext,
        cancellation: Cancellation,
        emit: impl FnMut(AgentEvent),
    ) -> Result<AgentSession, AgentFailure> {
        self.run_turn_with_agents(command, context, &NoA2AHost, cancellation, emit)
            .await
    }

    pub async fn run_turn_with_agents<Agents: A2AHost>(
        &self,
        command: AgentCommand,
        context: AgentContext,
        agents: &Agents,
        cancellation: Cancellation,
        mut emit: impl FnMut(AgentEvent),
    ) -> Result<AgentSession, AgentFailure> {
        if command.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if command.text.trim().is_empty()
            || command.text.len() > self.budget.max_output_bytes
            || self.budget.max_session_bytes < 4096
            || self.budget.deadline_ms == 0
            || self.budget.deadline_ms > AgentBudget::default().expanded(3).unwrap().deadline_ms
        {
            return Err(AgentFailure::InvalidInput);
        }
        let deadline = Instant::now() + Duration::from_millis(self.budget.deadline_ms);
        self.authorize(&context)?;
        let mut session = self
            .store
            .load(command.person_id, command.session_id)
            .await?;
        self.validate_session(&session, &command)?;
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let turn_id = Uuid::new_v4();
        let user = AgentMessage::User {
            turn_id,
            text: command.text,
        };
        session.messages.push(user.clone());
        session.usage = AgentUsage::default();
        session.active_turn = Some(turn_id);
        session.last_outcome = None;
        session.continuation = None;
        for class in &self.policy.data_classes {
            if !session.data_classes.contains(class) {
                session.data_classes.push(*class);
            }
        }
        if encoded_len(&session)? > self.budget.max_session_bytes.saturating_sub(4096) {
            return Err(AgentFailure::BudgetExceeded);
        }
        self.commit(&mut session).await?;
        emit_event(&session, turn_id, AgentEventKind::Started, &mut emit);
        emit_event(
            &session,
            turn_id,
            AgentEventKind::MessageCommitted {
                message: user,
                revision: session.revision,
            },
            &mut emit,
        );
        let mut usage = AgentUsage::default();
        let ledger = UsageLedger::new(self.budget.max_tokens, self.budget.max_cost_micros, usage);
        let (outcome, resumable) = match self
            .drive(
                &mut session,
                &context,
                turn_id,
                deadline,
                &cancellation,
                self.budget,
                &mut usage,
                &ledger,
                agents,
                &mut emit,
            )
            .await
        {
            Ok(()) => (AgentOutcome::Completed, false),
            Err(stop) => (
                AgentOutcome::Halted {
                    reason: stop.reason,
                },
                stop.resumable,
            ),
        };
        ledger.sync(&mut usage);
        session.usage = usage;
        if outcome != AgentOutcome::Completed {
            let (interrupted, attempts) = interrupt_executions(
                &mut session,
                match outcome {
                    AgentOutcome::Halted { reason } => reason,
                    _ => AgentFailure::Interrupted,
                },
            );
            session.active_turn = None;
            session.last_outcome = Some(outcome);
            session.continuation = soft_continuation(
                resumable && !interrupted,
                turn_id,
                0,
                usage,
                self.model.placement(),
            );
            self.commit(&mut session).await?;
            for record in attempts {
                emit_event(
                    &session,
                    turn_id,
                    AgentEventKind::ModelAttempt { record },
                    &mut emit,
                );
            }
        }
        emit_event(
            &session,
            turn_id,
            AgentEventKind::Finished {
                outcome,
                revision: session.revision,
            },
            &mut emit,
        );
        Ok(session)
    }

    pub async fn continue_turn(
        &self,
        person_id: floe_domain::PersonId,
        session_id: Uuid,
        expected_revision: u64,
        context: AgentContext,
        cancellation: Cancellation,
        emit: impl FnMut(AgentEvent),
    ) -> Result<AgentSession, AgentFailure> {
        self.continue_turn_with_agents(
            person_id,
            session_id,
            expected_revision,
            context,
            &NoA2AHost,
            cancellation,
            emit,
        )
        .await
    }

    pub async fn continue_turn_with_agents<Agents: A2AHost>(
        &self,
        person_id: floe_domain::PersonId,
        session_id: Uuid,
        expected_revision: u64,
        context: AgentContext,
        agents: &Agents,
        cancellation: Cancellation,
        mut emit: impl FnMut(AgentEvent),
    ) -> Result<AgentSession, AgentFailure> {
        self.authorize(&context)?;
        let mut session = self.store.load(person_id, session_id).await?;
        if session.person_id != person_id
            || session.id != session_id
            || session.revision != expected_revision
            || session.active_turn.is_some()
        {
            return Err(AgentFailure::Conflict);
        }
        if session.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if session.data_classes.is_empty()
            || session
                .data_classes
                .iter()
                .any(|class| !self.policy.data_classes.contains(class))
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let continuation = session.continuation.ok_or(AgentFailure::InvalidInput)?;
        if continuation.placement != self.model.placement()
            || !matches!(
                session.last_outcome,
                Some(AgentOutcome::Halted {
                    reason: AgentFailure::BudgetExceeded | AgentFailure::DeadlineExceeded
                })
            )
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let level = continuation
            .level
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let budget = self
            .budget
            .expanded(level)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let deadline = Instant::now() + Duration::from_millis(budget.deadline_ms);
        session.active_turn = Some(continuation.turn_id);
        session.last_outcome = None;
        session.continuation = None;
        self.commit(&mut session).await?;
        emit_event(
            &session,
            continuation.turn_id,
            AgentEventKind::Started,
            &mut emit,
        );
        let mut usage = continuation.usage;
        let ledger = UsageLedger::new(budget.max_tokens, budget.max_cost_micros, usage);
        let (outcome, resumable) = match self
            .drive(
                &mut session,
                &context,
                continuation.turn_id,
                deadline,
                &cancellation,
                budget,
                &mut usage,
                &ledger,
                agents,
                &mut emit,
            )
            .await
        {
            Ok(()) => (AgentOutcome::Completed, false),
            Err(stop) => (
                AgentOutcome::Halted {
                    reason: stop.reason,
                },
                stop.resumable,
            ),
        };
        ledger.sync(&mut usage);
        session.usage = usage;
        if outcome != AgentOutcome::Completed {
            let (interrupted, attempts) = interrupt_executions(
                &mut session,
                match outcome {
                    AgentOutcome::Halted { reason } => reason,
                    _ => AgentFailure::Interrupted,
                },
            );
            session.active_turn = None;
            session.last_outcome = Some(outcome);
            session.continuation = soft_continuation(
                resumable && !interrupted,
                continuation.turn_id,
                level,
                usage,
                self.model.placement(),
            );
            self.commit(&mut session).await?;
            for record in attempts {
                emit_event(
                    &session,
                    continuation.turn_id,
                    AgentEventKind::ModelAttempt { record },
                    &mut emit,
                );
            }
        }
        emit_event(
            &session,
            continuation.turn_id,
            AgentEventKind::Finished {
                outcome,
                revision: session.revision,
            },
            &mut emit,
        );
        Ok(session)
    }

    pub async fn recover_interrupted(
        &self,
        person_id: floe_domain::PersonId,
        session_id: Uuid,
        expected_revision: u64,
    ) -> Result<AgentSession, AgentFailure> {
        if self.store.protection() == SessionProtection::KeyUnavailable {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut session = self.store.load(person_id, session_id).await?;
        if session.person_id != person_id || session.id != session_id {
            return Err(AgentFailure::NotFound);
        }
        if session.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if self.store.protection() == SessionProtection::SyntheticOnly
            && session
                .data_classes
                .iter()
                .any(|class| *class != DataClass::Synthetic)
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        if session.revision != expected_revision {
            return Err(AgentFailure::Conflict);
        }
        if session.active_turn.is_some() {
            interrupt_executions(&mut session, AgentFailure::Interrupted);
            session.active_turn = None;
            session.continuation = None;
            session.last_outcome = Some(AgentOutcome::Halted {
                reason: AgentFailure::Interrupted,
            });
            self.commit(&mut session).await?;
        }
        Ok(session)
    }

    fn validate_session(
        &self,
        session: &AgentSession,
        command: &AgentCommand,
    ) -> Result<(), AgentFailure> {
        if session.person_id != command.person_id || session.id != command.session_id {
            return Err(AgentFailure::NotFound);
        }
        if session.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if session.revision != command.expected_revision || session.active_turn.is_some() {
            return Err(AgentFailure::Conflict);
        }
        if session.data_classes.is_empty()
            || session
                .data_classes
                .iter()
                .any(|class| !self.policy.data_classes.contains(class))
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    fn authorize(&self, context: &AgentContext) -> Result<(), AgentFailure> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?
            .as_millis();
        self.policy.authorize(
            self.model.placement(),
            self.store.protection(),
            context,
            u64::try_from(now).map_err(|_| AgentFailure::StaleContext)?,
        )
    }

    async fn commit(&self, session: &mut AgentSession) -> Result<(), AgentFailure> {
        let previous_revision = session.revision;
        session.revision = previous_revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        if encoded_len(session)? > self.budget.max_session_bytes {
            session.revision = previous_revision;
            return Err(AgentFailure::BudgetExceeded);
        }
        let result = self
            .store
            .compare_and_swap(session, previous_revision)
            .await;
        if result.is_err() {
            session.revision = previous_revision;
        }
        result
    }

    async fn recorded<T>(
        &self,
        future: impl Future<Output = Result<T, AgentFailure>>,
        session: &mut AgentSession,
        usage: &mut AgentUsage,
        ledger: &UsageLedger,
        journal: &mut tokio::sync::mpsc::UnboundedReceiver<crate::model_journal::JournalUpdate>,
        turn_id: Uuid,
        emit: &mut impl FnMut(AgentEvent),
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<T, AgentFailure> {
        let mut future = Box::pin(bounded(future, deadline, cancellation));
        loop {
            tokio::select! {
                biased;
                update = journal.recv() => {
                    let update = update.ok_or(AgentFailure::Interrupted)?;
                    match update.record {
                        crate::model_journal::JournalRecord::Model(record) => {
                            let previous = session.model_attempts.clone();
                            if record.state == ModelAttemptState::Started {
                                session.model_attempts.push(record.clone());
                            } else {
                                let saved = session.model_attempts.iter_mut().find(|saved| saved.id == record.id)
                                    .ok_or(AgentFailure::InvalidInput)?;
                                *saved = record.clone();
                            }
                            ledger.sync(usage);
                            session.usage = *usage;
                            if let Err(failure) = self.commit(session).await {
                                session.model_attempts = previous;
                                if record.state == ModelAttemptState::Started {
                                    ledger.undispatched(record.usage.tokens);
                                } else if let Some(saved) = session.model_attempts.iter_mut().find(|saved| saved.id == record.id) {
                                    saved.usage = record.usage;
                                }
                                let _ = update.acknowledged.send(Err(failure));
                                return Err(failure);
                            }
                            emit_event(session, turn_id, AgentEventKind::ModelAttempt { record }, emit);
                        }
                        crate::model_journal::JournalRecord::Capability(record) => {
                            let previous = session.capability_executions.clone();
                            if record.state == CapabilityExecutionState::Started {
                                if session.capability_executions.iter().any(|saved| saved.call_id == record.call_id) {
                                    return Err(AgentFailure::InvalidInput);
                                }
                                session.capability_executions.push(record.clone());
                            } else {
                                let saved = session.capability_executions.iter_mut().find(|saved| {
                                    saved.call_id == record.call_id && saved.scope_id == record.scope_id
                                        && saved.state == CapabilityExecutionState::Started
                                }).ok_or(AgentFailure::InvalidInput)?;
                                *saved = record.clone();
                            }
                            let message = if record.scope_id == session.id
                                && record.state == CapabilityExecutionState::Settled {
                                Some(AgentMessage::Capability {
                                    turn_id,
                                    call_id: record.call_id,
                                    capability_id: record.capability_id.clone(),
                                    input: record.input.clone(),
                                    result: record.result.clone().ok_or(AgentFailure::InvalidInput)?,
                                })
                            } else {
                                None
                            };
                            if let Some(message) = &message {
                                session.messages.push(message.clone());
                            }
                            ledger.sync(usage);
                            session.usage = *usage;
                            let commit = if encoded_len(session)? > self.budget.max_session_bytes.saturating_sub(4096) {
                                Err(AgentFailure::BudgetExceeded)
                            } else {
                                self.commit(session).await
                            };
                            if let Err(failure) = commit {
                                session.capability_executions = previous;
                                if message.is_some() {
                                    session.messages.pop();
                                }
                                let _ = update.acknowledged.send(Err(failure));
                                return Err(failure);
                            }
                            if record.scope_id == session.id && record.state == CapabilityExecutionState::Started {
                                emit_event(session, turn_id, AgentEventKind::CapabilityStarted {
                                    call_id: record.call_id, capability_id: record.capability_id,
                                }, emit);
                            }
                            if let Some(message) = message {
                                emit_event(session, turn_id, AgentEventKind::MessageCommitted {
                                    message, revision: session.revision,
                                }, emit);
                            }
                        }
                    }
                    let _ = update.acknowledged.send(Ok(()));
                }
                result = &mut future => return result,
            }
        }
    }

    async fn drive<Agents: A2AHost>(
        &self,
        session: &mut AgentSession,
        context: &AgentContext,
        turn_id: Uuid,
        deadline: Instant,
        cancellation: &Cancellation,
        budget: AgentBudget,
        usage: &mut AgentUsage,
        ledger: &UsageLedger,
        agents: &Agents,
        emit: &mut impl FnMut(AgentEvent),
    ) -> Result<(), DriveStop> {
        let (sender, mut journal) = tokio::sync::mpsc::unbounded_channel();
        let ledger = ledger.clone().with_journal(sender);
        for iteration in usage.iterations..budget.max_iterations {
            ledger.sync(usage);
            check_drive_running(deadline, cancellation)?;
            self.authorize(context)?;
            if encoded_len(context)?.saturating_add(encoded_len(&session.messages)?)
                > budget.max_context_bytes
            {
                return Err(AgentFailure::BudgetExceeded.into());
            }
            if usage.tokens >= budget.max_tokens || usage.cost_micros > budget.max_cost_micros {
                return Err(DriveStop::soft(AgentFailure::BudgetExceeded));
            }
            let descriptors: Vec<_> = self
                .capabilities
                .descriptors(session.person_id)
                .into_iter()
                .filter(|descriptor| {
                    descriptor.schema_version == AGENT_VERSION
                        && descriptor.read_only
                        && !descriptor.id.trim().is_empty()
                        && !descriptor.version.trim().is_empty()
                        && self
                            .policy
                            .data_classes
                            .contains(&descriptor.output_data_class)
                })
                .collect();
            let active_agents: Vec<_> = agents
                .agent_cards(session.person_id)
                .into_iter()
                .map(|card| {
                    card.validate()?;
                    Ok(card)
                })
                .collect::<Result<_, AgentFailure>>()?;
            if active_agents.len() > 16 {
                return Err(AgentFailure::BudgetExceeded.into());
            }
            emit_event(
                session,
                turn_id,
                AgentEventKind::ModelStarted {
                    iteration,
                    placement: self.model.placement(),
                },
                emit,
            );
            let replay = session
                .messages
                .iter()
                .filter_map(|message| match message {
                    AgentMessage::Capability {
                        turn_id: message_turn,
                        call_id,
                        ..
                    } if *message_turn == turn_id => session
                        .capability_executions
                        .iter()
                        .find(|execution| {
                            execution.call_id == *call_id
                                && execution.scope_id == session.id
                                && execution.state == CapabilityExecutionState::Settled
                        })
                        .and_then(|execution| execution.replay.clone())
                        .map(|replay| ModelReplay {
                            call_id: *call_id,
                            replay,
                        }),
                    AgentMessage::Delegation {
                        turn_id: message_turn,
                        task,
                    } if *message_turn == turn_id => session
                        .delegation_executions
                        .iter()
                        .find(|execution| {
                            execution.task_id == task.id
                                && execution.state == DelegationExecutionState::Settled
                        })
                        .and_then(|execution| execution.replay.clone())
                        .map(|replay| ModelReplay {
                            call_id: task.id,
                            replay,
                        }),
                    _ => None,
                })
                .collect();
            let request = ModelRequest {
                usage: ledger.clone(),
                replay,
                schema_version: AGENT_VERSION,
                prompt: manager_prompt(context.persona.as_ref()).map_err(DriveStop::from)?,
                person_id: session.person_id,
                session_id: session.id,
                turn_id,
                policy: self.policy.clone(),
                context: context.clone(),
                messages: session.messages.clone(),
                capabilities: descriptors.clone(),
                active_agents: active_agents.clone(),
                remaining_tokens: budget.max_tokens - usage.tokens,
                remaining_cost_micros: budget.max_cost_micros - usage.cost_micros,
                max_output_bytes: budget.max_output_bytes,
                deadline,
                cancellation: cancellation.clone(),
            };
            if encoded_len(&request.capabilities)?
                .saturating_add(encoded_len(&request.active_agents)?)
                .saturating_add(encoded_len(context)?)
                .saturating_add(encoded_len(&session.messages)?)
                .saturating_add(encoded_len(self.policy)?)
                > budget.max_context_bytes
            {
                return Err(AgentFailure::BudgetExceeded.into());
            }
            usage.iterations = iteration + 1;
            let response = self
                .recorded(
                    crate::generate_with_recovery(self.model, request),
                    session,
                    usage,
                    &ledger,
                    &mut journal,
                    turn_id,
                    emit,
                    deadline,
                    cancellation,
                )
                .await
                .map_err(DriveStop::from_call)?;
            check_drive_running(deadline, cancellation)?;
            self.authorize(context)?;
            if response.schema_version != AGENT_VERSION {
                return Err(AgentFailure::InvalidModelOutput.into());
            }
            ledger.sync(usage);
            session.usage = *usage;
            if usage.tokens > budget.max_tokens || usage.cost_micros > budget.max_cost_micros {
                return Err(DriveStop::soft(AgentFailure::BudgetExceeded));
            }
            if encoded_len(&response.output)? > budget.max_output_bytes {
                return Err(AgentFailure::BudgetExceeded.into());
            }
            if response.call_count()
                > budget
                    .max_capability_calls
                    .saturating_sub(usage.capability_calls) as usize
            {
                return Err(DriveStop::soft(AgentFailure::BudgetExceeded));
            }
            let grouped = response.output.len() > 1;
            if grouped {
                session.pending_output = Some(response.output.clone());
                let commit =
                    if encoded_len(session)? > budget.max_session_bytes.saturating_sub(4096) {
                        Err(AgentFailure::BudgetExceeded)
                    } else {
                        self.commit(session).await
                    };
                if let Err(failure) = commit {
                    session.pending_output = None;
                    return Err(failure.into());
                }
            }
            let mut call_index = 0;
            for step in response.output.clone() {
                check_drive_running(deadline, cancellation)?;
                self.authorize(context)?;
                let message = match step {
                    ModelStep::Preamble { text } => {
                        let message = AgentMessage::Preamble { turn_id, text };
                        session.messages.push(message.clone());
                        let commit = if encoded_len(session)?
                            > budget.max_session_bytes.saturating_sub(4096)
                        {
                            Err(AgentFailure::BudgetExceeded)
                        } else {
                            self.commit(session).await
                        };
                        if let Err(failure) = commit {
                            session.messages.pop();
                            return Err(failure.into());
                        }
                        emit_event(
                            session,
                            turn_id,
                            AgentEventKind::MessageCommitted {
                                message,
                                revision: session.revision,
                            },
                            emit,
                        );
                        continue;
                    }
                    ModelStep::Answer { text } => {
                        if text.trim().is_empty() {
                            return Err(AgentFailure::InvalidModelOutput.into());
                        }
                        AgentMessage::Assistant { turn_id, text }
                    }
                    ModelStep::Call {
                        capability_id,
                        input,
                    } => {
                        if usage.capability_calls >= budget.max_capability_calls {
                            return Err(DriveStop::soft(AgentFailure::BudgetExceeded));
                        }
                        let Some(descriptor) = descriptors
                            .iter()
                            .find(|descriptor| descriptor.id == capability_id)
                        else {
                            return Err(AgentFailure::CapabilityDenied.into());
                        };
                        if !self
                            .capabilities
                            .descriptors(session.person_id)
                            .contains(descriptor)
                        {
                            return Err(AgentFailure::CapabilityUnavailable.into());
                        }
                        usage.capability_calls += 1;
                        let call_id = Uuid::new_v4();
                        session.usage = *usage;
                        let execution = CapabilityExecution {
                            scope_id: session.id,
                            result: None,
                            turn_id,
                            call_id,
                            capability_id: capability_id.clone(),
                            input: input.clone(),
                            state: CapabilityExecutionState::Started,
                            replay: response.replay_for(call_index)?,
                        };
                        call_index += 1;
                        let invocation = CapabilityInvocation {
                            usage: ledger.clone(),
                            schema_version: AGENT_VERSION,
                            call_id,
                            person_id: session.person_id,
                            session_id: session.id,
                            turn_id,
                            capability_id: capability_id.clone(),
                            input: input.clone(),
                            max_output_bytes: budget.max_output_bytes,
                            deadline,
                            cancellation: cancellation.clone(),
                        };
                        let result = self
                            .recorded(
                                crate::capability_execution::execute_recorded(
                                    &ledger,
                                    execution,
                                    deadline,
                                    cancellation,
                                    budget.max_output_bytes,
                                    Box::pin(async {
                                        self.authorize(context)?;
                                        if !self
                                            .capabilities
                                            .descriptors(invocation.person_id)
                                            .contains(descriptor)
                                        {
                                            return Err(AgentFailure::CapabilityUnavailable);
                                        }
                                        self.capabilities.invoke(invocation).await
                                    }),
                                ),
                                session,
                                usage,
                                &ledger,
                                &mut journal,
                                turn_id,
                                emit,
                                deadline,
                                cancellation,
                            )
                            .await
                            .map_err(DriveStop::from_call)?;
                        if let Err(AgentFailure::Cancelled | AgentFailure::DeadlineExceeded) =
                            &result
                        {
                            return Err(DriveStop::from_call(result.unwrap_err()));
                        }
                        check_drive_running(deadline, cancellation)?;
                        continue;
                    }
                    ModelStep::Delegate { agent_id, message } => {
                        if usage.capability_calls >= budget.max_capability_calls {
                            return Err(DriveStop::soft(AgentFailure::BudgetExceeded));
                        }
                        let Some(card) = active_agents.iter().find(|card| card.id == agent_id)
                        else {
                            return Err(AgentFailure::CapabilityDenied.into());
                        };
                        if !agents
                            .agent_cards(session.person_id)
                            .iter()
                            .any(|current| current == card)
                        {
                            return Err(AgentFailure::CapabilityUnavailable.into());
                        }
                        usage.capability_calls += 1;
                        let task_id = Uuid::new_v4();
                        let context_id = Uuid::new_v4();
                        let request_message = A2AMessage {
                            message_id: Uuid::new_v4(),
                            context_id,
                            task_id: Some(task_id),
                            role: A2AMessageRole::User,
                            parts: vec![A2APart::Text {
                                text: message.clone(),
                            }],
                        };
                        let execution = DelegationExecution {
                            turn_id,
                            task_id,
                            agent_id: agent_id.clone(),
                            message: message.clone(),
                            state: DelegationExecutionState::Started,
                            task: None,
                            replay: response.replay_for(call_index)?,
                        };
                        call_index += 1;
                        session.delegation_executions.push(execution);
                        session.usage = *usage;
                        if let Err(failure) = self.commit(session).await {
                            session.delegation_executions.pop();
                            return Err(DriveStop::from_call(failure));
                        }
                        emit_event(
                            session,
                            turn_id,
                            AgentEventKind::DelegationStarted {
                                task_id,
                                agent_id: agent_id.clone(),
                            },
                            emit,
                        );
                        let request = A2ASendMessageRequest {
                            usage: ledger.clone(),
                            schema_version: AGENT_VERSION,
                            person_id: session.person_id,
                            session_id: session.id,
                            parent_turn_id: turn_id,
                            agent_id: agent_id.clone(),
                            message: request_message.clone(),
                            max_output_bytes: budget.max_output_bytes,
                            deadline,
                            cancellation: cancellation.clone(),
                        };
                        let result = self
                            .recorded(
                                agents.send_message(request),
                                session,
                                usage,
                                &ledger,
                                &mut journal,
                                turn_id,
                                emit,
                                deadline,
                                cancellation,
                            )
                            .await;
                        if let Err(AgentFailure::Cancelled | AgentFailure::DeadlineExceeded) =
                            result
                        {
                            return Err(DriveStop::from_call(result.unwrap_err()));
                        }
                        let task = match result {
                            Ok(task)
                                if task.id == task_id
                                    && task.context_id == context_id
                                    && task.agent_id == agent_id
                                    && task.state == A2ATaskState::Completed
                                    && task.failure.is_none()
                                    && task.result_text().is_ok() =>
                            {
                                task
                            }
                            Ok(_) => return Err(AgentFailure::InvalidModelOutput.into()),
                            Err(failure) => A2ATask {
                                id: task_id,
                                context_id,
                                agent_id: agent_id.clone(),
                                state: A2ATaskState::Failed,
                                history: vec![request_message],
                                artifacts: vec![],
                                failure: Some(failure),
                            },
                        };
                        if encoded_len(&task)? > budget.max_output_bytes {
                            return Err(AgentFailure::BudgetExceeded.into());
                        }
                        let saved = session
                            .delegation_executions
                            .iter_mut()
                            .find(|execution| execution.task_id == task_id)
                            .ok_or(AgentFailure::InvalidInput)?;
                        let previous = saved.clone();
                        saved.state = DelegationExecutionState::Settled;
                        saved.task = Some(task.clone());
                        let message = AgentMessage::Delegation { turn_id, task };
                        session.messages.push(message.clone());
                        if let Err(failure) = self.commit(session).await {
                            session.messages.pop();
                            *session
                                .delegation_executions
                                .iter_mut()
                                .find(|execution| execution.task_id == task_id)
                                .ok_or(AgentFailure::InvalidInput)? = previous;
                            return Err(DriveStop::from_call(failure));
                        }
                        emit_event(
                            session,
                            turn_id,
                            AgentEventKind::MessageCommitted {
                                message,
                                revision: session.revision,
                            },
                            emit,
                        );
                        continue;
                    }
                };
                ledger.sync(usage);
                session.usage = *usage;
                session.messages.push(message.clone());
                if encoded_len(session)? > budget.max_session_bytes.saturating_sub(4096) {
                    session.messages.pop();
                    return Err(AgentFailure::BudgetExceeded.into());
                }
                session.pending_output = None;
                session.active_turn = None;
                session.last_outcome = Some(AgentOutcome::Completed);
                for execution in &mut session.capability_executions {
                    execution.replay = None;
                }
                for execution in &mut session.delegation_executions {
                    execution.replay = None;
                }
                if let Err(failure) = self.commit(session).await {
                    session.messages.pop();
                    return Err(failure.into());
                }
                emit_event(
                    session,
                    turn_id,
                    AgentEventKind::MessageCommitted {
                        message,
                        revision: session.revision,
                    },
                    emit,
                );
                return Ok(());
            }
            if grouped {
                session.pending_output = None;
                self.commit(session).await.map_err(DriveStop::from_call)?;
            }
        }
        Err(DriveStop::soft(AgentFailure::BudgetExceeded))
    }
}

fn interrupt_executions(
    session: &mut AgentSession,
    failure: AgentFailure,
) -> (bool, Vec<ModelAttemptRecord>) {
    let mut attempts = vec![];
    for record in &mut session.model_attempts {
        if record.state == ModelAttemptState::Started {
            record.state = ModelAttemptState::Interrupted;
            record.failure = Some(failure);
            attempts.push(record.clone());
        }
    }
    let mut interrupted = session.pending_output.take().is_some();
    for execution in &mut session.capability_executions {
        if execution.state == CapabilityExecutionState::Started {
            execution.state = CapabilityExecutionState::Interrupted;
            interrupted = true;
        }
    }
    for execution in &mut session.delegation_executions {
        if execution.state == DelegationExecutionState::Started {
            execution.state = DelegationExecutionState::Interrupted;
            interrupted = true;
        }
    }
    (interrupted, attempts)
}

#[derive(Debug)]
struct DriveStop {
    reason: AgentFailure,
    resumable: bool,
}

impl DriveStop {
    fn soft(reason: AgentFailure) -> Self {
        Self {
            reason,
            resumable: true,
        }
    }

    fn from_call(reason: AgentFailure) -> Self {
        if reason == AgentFailure::DeadlineExceeded {
            Self::soft(reason)
        } else {
            Self::from(reason)
        }
    }
}

impl From<AgentFailure> for DriveStop {
    fn from(reason: AgentFailure) -> Self {
        Self {
            reason,
            resumable: false,
        }
    }
}

fn encoded_len(value: &impl serde::Serialize) -> Result<usize, AgentFailure> {
    serde_json::to_vec(value)
        .map(|encoded| encoded.len())
        .map_err(|_| AgentFailure::InvalidInput)
}

fn soft_continuation(
    resumable: bool,
    turn_id: Uuid,
    level: u8,
    usage: AgentUsage,
    placement: ModelPlacement,
) -> Option<AgentContinuation> {
    (resumable && level < 3).then_some(AgentContinuation {
        turn_id,
        level,
        usage,
        placement,
    })
}

fn check_drive_running(deadline: Instant, cancellation: &Cancellation) -> Result<(), DriveStop> {
    check_running(deadline, cancellation).map_err(DriveStop::from_call)
}

fn emit_event(
    session: &AgentSession,
    turn_id: Uuid,
    event: AgentEventKind,
    emit: &mut impl FnMut(AgentEvent),
) {
    emit(AgentEvent {
        schema_version: AGENT_VERSION,
        session_id: session.id,
        turn_id,
        event,
    });
}

fn check_running(deadline: Instant, cancellation: &Cancellation) -> Result<(), AgentFailure> {
    if cancellation.is_cancelled() {
        Err(AgentFailure::Cancelled)
    } else if Instant::now() >= deadline {
        Err(AgentFailure::DeadlineExceeded)
    } else {
        Ok(())
    }
}

async fn bounded<ResultValue>(
    future: impl Future<Output = Result<ResultValue, AgentFailure>>,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<ResultValue, AgentFailure> {
    check_running(deadline, cancellation)?;
    let mut guard = CallCancellationGuard(Some(cancellation.clone()));
    let result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(AgentFailure::Cancelled),
        result = tokio::time::timeout_at(deadline, future) => result.map_err(|_| AgentFailure::DeadlineExceeded)?,
    };
    if !matches!(
        result,
        Err(AgentFailure::Cancelled | AgentFailure::DeadlineExceeded)
    ) {
        guard.0 = None;
    }
    result
}

struct CallCancellationGuard(Option<Cancellation>);

impl Drop for CallCancellationGuard {
    fn drop(&mut self) {
        if let Some(cancellation) = &self.0 {
            cancellation.cancel();
        }
    }
}
