use std::{collections::BTreeMap, future::Future, time::SystemTime};

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
        mut emit: impl FnMut(AgentEvent),
    ) -> Result<AgentSession, AgentFailure> {
        if command.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if command.text.trim().is_empty()
            || command.text.len() > self.budget.max_output_bytes
            || self.budget.max_session_bytes < 4096
            || self.budget.deadline_ms == 0
            || self.budget.deadline_ms > 300_000
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
        session.active_turn = Some(turn_id);
        session.last_outcome = None;
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
        let outcome = match self
            .drive(
                &mut session,
                &context,
                turn_id,
                deadline,
                &cancellation,
                &mut emit,
            )
            .await
        {
            Ok(()) => AgentOutcome::Completed,
            Err(reason) => AgentOutcome::Halted { reason },
        };
        if outcome != AgentOutcome::Completed {
            session.active_turn = None;
            session.last_outcome = Some(outcome);
            self.commit(&mut session).await?;
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
            session.active_turn = None;
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

    async fn drive(
        &self,
        session: &mut AgentSession,
        context: &AgentContext,
        turn_id: Uuid,
        deadline: Instant,
        cancellation: &Cancellation,
        emit: &mut impl FnMut(AgentEvent),
    ) -> Result<(), AgentFailure> {
        let mut used_tokens = 0_u64;
        let mut cost_micros = 0_u64;
        let mut capability_calls = 0_u32;
        let mut repeated = BTreeMap::new();
        for iteration in 0..self.budget.max_iterations {
            check_running(deadline, cancellation)?;
            self.authorize(context)?;
            if encoded_len(context)?.saturating_add(encoded_len(&session.messages)?)
                > self.budget.max_context_bytes
                || used_tokens >= self.budget.max_tokens
                || cost_micros > self.budget.max_cost_micros
            {
                return Err(AgentFailure::BudgetExceeded);
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
            emit_event(
                session,
                turn_id,
                AgentEventKind::ModelStarted {
                    iteration,
                    placement: self.model.placement(),
                },
                emit,
            );
            let request = ModelRequest {
                schema_version: AGENT_VERSION,
                system_instructions: AGENT_SYSTEM_INSTRUCTIONS,
                person_id: session.person_id,
                session_id: session.id,
                turn_id,
                policy: self.policy.clone(),
                context: context.clone(),
                messages: session.messages.clone(),
                capabilities: descriptors.clone(),
                remaining_tokens: self.budget.max_tokens - used_tokens,
                remaining_cost_micros: self.budget.max_cost_micros - cost_micros,
                max_output_bytes: self.budget.max_output_bytes,
                deadline,
                cancellation: cancellation.clone(),
            };
            if encoded_len(&request.capabilities)?
                .saturating_add(encoded_len(context)?)
                .saturating_add(encoded_len(&session.messages)?)
                .saturating_add(encoded_len(self.policy)?)
                > self.budget.max_context_bytes
            {
                return Err(AgentFailure::BudgetExceeded);
            }
            let response = bounded(self.model.generate(request), deadline, cancellation).await?;
            check_running(deadline, cancellation)?;
            self.authorize(context)?;
            if response.schema_version != AGENT_VERSION {
                return Err(AgentFailure::InvalidModelOutput);
            }
            used_tokens = used_tokens
                .checked_add(response.used_tokens)
                .ok_or(AgentFailure::BudgetExceeded)?;
            cost_micros = cost_micros
                .checked_add(response.cost_micros)
                .ok_or(AgentFailure::BudgetExceeded)?;
            if used_tokens > self.budget.max_tokens
                || cost_micros > self.budget.max_cost_micros
                || encoded_len(&response.step)? > self.budget.max_output_bytes
            {
                return Err(AgentFailure::BudgetExceeded);
            }
            let message = match response.step {
                ModelStep::Answer { text } => {
                    if text.trim().is_empty() {
                        return Err(AgentFailure::InvalidModelOutput);
                    }
                    AgentMessage::Assistant { turn_id, text }
                }
                ModelStep::Call {
                    capability_id,
                    input,
                } => {
                    if capability_calls >= self.budget.max_capability_calls {
                        return Err(AgentFailure::BudgetExceeded);
                    }
                    let Some(descriptor) = descriptors
                        .iter()
                        .find(|descriptor| descriptor.id == capability_id)
                    else {
                        return Err(AgentFailure::CapabilityDenied);
                    };
                    if !self
                        .capabilities
                        .descriptors(session.person_id)
                        .contains(descriptor)
                    {
                        return Err(AgentFailure::CapabilityUnavailable);
                    }
                    let repetitions = repeated
                        .entry((capability_id.clone(), input.clone()))
                        .or_insert(0_u32);
                    if *repetitions >= self.budget.max_repeated_calls {
                        return Err(AgentFailure::Stalled);
                    }
                    *repetitions += 1;
                    capability_calls += 1;
                    let call_id = Uuid::new_v4();
                    emit_event(
                        session,
                        turn_id,
                        AgentEventKind::CapabilityStarted {
                            call_id,
                            capability_id: capability_id.clone(),
                        },
                        emit,
                    );
                    let result = bounded(
                        self.capabilities.invoke(CapabilityInvocation {
                            schema_version: AGENT_VERSION,
                            call_id,
                            person_id: session.person_id,
                            session_id: session.id,
                            turn_id,
                            capability_id: capability_id.clone(),
                            input: input.clone(),
                            max_output_bytes: self.budget.max_output_bytes,
                            deadline,
                            cancellation: cancellation.clone(),
                        }),
                        deadline,
                        cancellation,
                    )
                    .await;
                    if let Err(AgentFailure::Cancelled | AgentFailure::DeadlineExceeded) = &result {
                        return Err(result.unwrap_err());
                    }
                    check_running(deadline, cancellation)?;
                    if result
                        .as_ref()
                        .is_ok_and(|text| text.len() > self.budget.max_output_bytes)
                    {
                        return Err(AgentFailure::BudgetExceeded);
                    }
                    AgentMessage::Capability {
                        turn_id,
                        call_id,
                        capability_id,
                        input,
                        result,
                    }
                }
            };
            let completed = matches!(message, AgentMessage::Assistant { .. });
            session.messages.push(message.clone());
            if encoded_len(session)? > self.budget.max_session_bytes.saturating_sub(4096) {
                session.messages.pop();
                return Err(AgentFailure::BudgetExceeded);
            }
            if completed {
                session.active_turn = None;
                session.last_outcome = Some(AgentOutcome::Completed);
            }
            if let Err(failure) = self.commit(session).await {
                session.messages.pop();
                return Err(failure);
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
            if completed {
                return Ok(());
            }
        }
        Err(AgentFailure::BudgetExceeded)
    }
}

fn encoded_len(value: &impl serde::Serialize) -> Result<usize, AgentFailure> {
    serde_json::to_vec(value)
        .map(|encoded| encoded.len())
        .map_err(|_| AgentFailure::InvalidInput)
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
