//! Generation-owned Conversation service. Durable admission, background drives,
//! linked resume and observation all remain with the Conversation owner.
use super::coordinator::{RunAdmission, RunCoordinator};
use crate::*;
use floe_agent_contract::{AgentContext, BoxFuture, ModelPort};
use floe_context::{DependencyResolver, EvidenceReader};
use floe_execution::{Cancellation, ExecutionScope, budget::BudgetLedger};
use floe_kernel::{AgentFailure, CommandFailure, CommandId, OwnerActor, RunId, TraceContext};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Mutex, Notify, RwLock};
use tokio::task::JoinSet;
use uuid::Uuid;

mod recovery_driver;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandReceipt {
    pub command_id: CommandId,
    pub run_id: RunId,
    pub session_revision: u64,
}
impl From<&RunReceipt> for CommandReceipt {
    fn from(run: &RunReceipt) -> Self {
        Self {
            command_id: run.command_id,
            run_id: run.run_id,
            session_revision: run.session_revision,
        }
    }
}
#[derive(Clone, Debug)]
pub struct ResolveInteraction {
    pub command_id: Uuid,
    pub interaction_id: Uuid,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub decision: InteractionDecisionKind,
    pub target_digest: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct RefreshInteraction {
    pub command_id: Uuid,
    pub interaction_id: Uuid,
    pub session_id: Uuid,
    pub expected_revision: u64,
}
#[derive(Clone, Debug)]
pub struct InteractionResult {
    pub interaction: InteractionSnapshot,
    pub linked: Option<CommandReceipt>,
}
#[derive(Clone, Debug)]
pub struct CalendarProposalRequest {
    pub session_id: Uuid,
    pub origin_run_id: RunId,
    pub receipt: floe_agent_contract::TaskExecutionReceiptRef,
    pub artifact_id: Uuid,
    pub destination_ref: Uuid,
}
#[derive(Clone, Debug)]
pub struct CalendarProposalResult {
    pub operation: floe_calendar_operations::ActionSnapshot,
    pub interaction: Option<InteractionSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageSnapshotRole {
    User,
    Assistant,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MessageSnapshot {
    pub message_id: Uuid,
    pub role: MessageSnapshotRole,
    pub text: String,
}

/// An object-safe owner handle exposed by a ready Vault generation.
pub trait ConversationOwner: Send + Sync {
    /// Immediate lifetime fence for Drop/panic paths; shutdown also drains.
    fn close_admission(&self);
    #[cfg(feature = "qa-fixtures")]
    fn qa_lose_next_operation_approval_admission_ack(&self);
    #[cfg(feature = "qa-fixtures")]
    fn qa_pause_recovery<'a>(&'a self) -> BoxFuture<'a, Result<(), AgentFailure>>;
    #[cfg(feature = "qa-fixtures")]
    fn qa_read_interaction_record<'a>(
        &'a self,
        actor: &'a OwnerActor,
        interaction_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<ConversationInteraction>, AgentFailure>>;
    fn start_session<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::SessionStartAdmission, crate::SessionStartFailure>>;
    fn calendar_operation_policy<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_access::OperationAuthorizationPolicy, AgentFailure>>;
    fn set_calendar_operation_policy<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        mode: floe_access::OperationPolicyMode,
        expected_revision: u64,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<floe_access::OperationAuthorizationPolicy, CommandFailure<AgentFailure>>,
    >;
    fn resume_session<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<SessionSnapshot>, AgentFailure>>;
    fn get_session<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
        before_message_id: Option<Uuid>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SessionSnapshot, AgentFailure>>;
    fn start_turn<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: StartTurn,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CommandReceipt, CommandFailure<AgentFailure>>>;
    fn submit_calendar_proposal<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        request: CalendarProposalRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CalendarProposalResult, CommandFailure<AgentFailure>>>;
    fn cancel_run<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        run_id: RunId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CancelRunReceipt, CommandFailure<AgentFailure>>>;
    fn read_command<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>>;
    fn read_run<'a>(
        &'a self,
        actor: &'a OwnerActor,
        run_id: RunId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>>;
    fn read_message<'a>(
        &'a self,
        actor: &'a OwnerActor,
        message_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<MessageSnapshot>, AgentFailure>>;
    fn read_interaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        interaction_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<InteractionSnapshot>, AgentFailure>>;
    fn list_interactions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<InteractionSnapshot>, AgentFailure>>;
    fn resolve_interaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: ResolveInteraction,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<InteractionResult, CommandFailure<AgentFailure>>>;
    fn refresh_interaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: RefreshInteraction,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<InteractionResult, CommandFailure<AgentFailure>>>;
    fn read_events<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: ReadConversationEvents,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<EventRead, AgentFailure>>;
    fn activate<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
    fn shutdown<'a>(&'a self, scope: &'a ExecutionScope)
    -> BoxFuture<'a, Result<(), AgentFailure>>;
}

pub struct ConversationDependencies<R, S, T> {
    pub repository: Arc<R>,
    pub sessions: Arc<S>,
    pub experts: Arc<floe_experts::TaskCoordinator<T>>,
    pub model: Arc<dyn ModelPort>,
    pub evidence: Arc<dyn EvidenceReader>,
    pub resolver: Arc<dyn DependencyResolver>,
    pub connections: Arc<floe_connections::ConnectionsService>,
    pub calendar_operations: Arc<floe_calendar_operations::CalendarOperationsService>,
    pub experts_owner: Arc<dyn floe_experts::ExpertsOwner>,
    pub knowledge: Arc<dyn floe_knowledge::KnowledgeOwner>,
    pub runtime_epoch: u64,
}

pub struct ConversationService<R, S, T> {
    inner: Arc<ServiceState<R, S, T>>,
}
impl<R, S, T> Drop for ConversationService<R, S, T> {
    fn drop(&mut self) {
        self.inner.closing.store(true, Ordering::Release);
        self.inner
            .shutdown
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
    }
}
struct ServiceState<R, S, T> {
    dependencies: ConversationDependencies<R, S, T>,
    coordinator: RunCoordinator<R>,
    config: ManagerConfig,
    cancellations: Arc<RunCancellationRegistry>,
    events: ConversationEventBuffer,
    shutdown: Cancellation,
    closing: AtomicBool,
    admission: RwLock<()>,
    command_locks: Mutex<std::collections::HashMap<Uuid, Weak<Mutex<()>>>>,
    tasks: Mutex<JoinSet<()>>,
    recovery_started: AtomicBool,
    recovery_wake: Notify,
    #[cfg(feature = "qa-fixtures")]
    lose_next_operation_admission_ack: AtomicBool,
    #[cfg(feature = "qa-fixtures")]
    recovery_pause_requested: AtomicBool,
    #[cfg(feature = "qa-fixtures")]
    recovery_pause_acknowledged: AtomicBool,
    #[cfg(feature = "qa-fixtures")]
    recovery_pause_ack: Notify,
}

impl<R, S, T> ConversationService<R, S, T>
where
    R: ConversationRepository + InteractionRepository + SessionRepository + 'static,
    S: SessionStore + Send + Sync + 'static,
    T: floe_experts::TaskRepository + 'static,
{
    pub fn new(
        dependencies: ConversationDependencies<R, S, T>,
        config: ManagerConfig,
    ) -> Result<Self, AgentFailure> {
        config.validate()?;
        let cancellations = Arc::new(RunCancellationRegistry::default());
        let coordinator = RunCoordinator::new(
            dependencies.repository.clone(),
            config.clone(),
            dependencies.connections.clone(),
            dependencies.calendar_operations.clone(),
            dependencies.experts_owner.clone(),
        )?;
        let events = ConversationEventBuffer::new(dependencies.runtime_epoch)?;
        Ok(Self {
            inner: Arc::new(ServiceState {
                dependencies,
                coordinator,
                config,
                cancellations,
                events,
                shutdown: Cancellation::new(),
                closing: AtomicBool::new(false),
                admission: RwLock::new(()),
                command_locks: Mutex::new(std::collections::HashMap::new()),
                tasks: Mutex::new(JoinSet::new()),
                recovery_started: AtomicBool::new(false),
                recovery_wake: Notify::new(),
                #[cfg(feature = "qa-fixtures")]
                lose_next_operation_admission_ack: AtomicBool::new(false),
                #[cfg(feature = "qa-fixtures")]
                recovery_pause_requested: AtomicBool::new(false),
                #[cfg(feature = "qa-fixtures")]
                recovery_pause_acknowledged: AtomicBool::new(false),
                #[cfg(feature = "qa-fixtures")]
                recovery_pause_ack: Notify::new(),
            }),
        })
    }
}

impl<R, S, T> ServiceState<R, S, T>
where
    R: ConversationRepository + InteractionRepository + SessionRepository + 'static,
    S: SessionStore + Send + Sync + 'static,
    T: floe_experts::TaskRepository + 'static,
{
    fn check(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        actor.validate()?;
        if actor.runtime_epoch != self.dependencies.runtime_epoch
            || self.closing.load(Ordering::Acquire)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    async fn lock_command(
        &self,
        command_id: CommandId,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, AgentFailure> {
        if !command_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let lock = {
            let mut locks = self.command_locks.lock().await;
            if locks.len() >= 64 {
                locks.retain(|_, lock| lock.strong_count() > 0);
            }
            if let Some(lock) = locks.get(&command_id.as_uuid()).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(command_id.as_uuid(), Arc::downgrade(&lock));
                lock
            }
        };
        Ok(lock.lock_owned().await)
    }

    async fn submit(
        self: &Arc<Self>,
        actor: &OwnerActor,
        command_id: CommandId,
        intent: CanonicalTurnIntent,
        session: AgentSession,
        scope: &ExecutionScope,
    ) -> Result<CommandReceipt, CommandFailure<AgentFailure>> {
        let _admission = self.admission.read().await;
        self.check(actor).map_err(CommandFailure::NotAdmitted)?;
        let environment = Arc::new(
            self.dependencies
                .experts
                .environment(&actor.person_id.to_string())
                .map_err(CommandFailure::NotApplied)?,
        );
        let request = TurnRequest {
            command_id,
            session_id: intent.session_id,
            expected_session_revision: intent.expected_revision,
            principal: actor.person_id.to_string(),
            prompt: intent.text,
            mode: intent.mode,
            retry_of: intent.retry_of,
            allowed_catalog: environment.catalog(),
            expert_environment: environment.identity(),
            replay: vec![],
            deadline: tokio::time::Instant::now() + self.config.max_run_duration,
            cancellation: self.shutdown.child_scope(),
            delegation_context: None,
            device_id: actor.device_id.clone(),
            now_unix_ms: chrono::Utc::now().timestamp_millis(),
        };
        let prepared = match scope
            .run(async { Ok(self.coordinator.prepare_run(actor, &request, scope).await) })
            .await
            .map_err(CommandFailure::Indeterminate)?
            .map_err(|failure| failure)?
        {
            RunAdmission::Existing(receipt) => return Ok(CommandReceipt::from(&receipt)),
            RunAdmission::Created(prepared) => prepared,
        };
        let receipt = prepared.admitted.receipt.clone();
        let guard = match self.cancellations.register(
            receipt.run_id,
            command_id,
            &receipt.principal,
            request.cancellation.clone(),
        ) {
            Ok(guard) => guard,
            Err(failure) => {
                let _ = self
                    .dependencies
                    .repository
                    .finish_run(
                        receipt.run_id,
                        receipt.aggregate_revision,
                        RunTerminal::from_failure(failure),
                    )
                    .await;
                return Err(CommandFailure::Admitted(failure));
            }
        };
        if let Err(failure) = self.events.publish_command(&receipt) {
            let _ = self
                .dependencies
                .repository
                .finish_run(
                    receipt.run_id,
                    receipt.aggregate_revision,
                    RunTerminal::from_failure(failure),
                )
                .await;
            return Err(CommandFailure::Admitted(failure));
        }
        let state = Arc::clone(self);
        let actor = actor.clone();
        let run_id = receipt.run_id;
        let mut tasks = self.tasks.lock().await;
        while tasks.try_join_next().is_some() {}
        tasks.spawn(async move {
            let _wake = recovery_driver::WakeOnDrop(&state.recovery_wake);
            // Keep the retained-driver proof through outer error settlement.
            // The wake is declared first so this guard drops before notification.
            let driver_guard = guard;
            let mut request = request;
            let outcome = async {
                let _foreground = state.dependencies.knowledge.foreground_lease()?;
                let read_scope = ExecutionScope::root(
                    request.cancellation.child_scope(),
                    request.deadline,
                    BudgetLedger::new(
                        floe_execution::budget::BudgetConfig::new(0, 0),
                        Default::default(),
                    )
                    .root_lease(),
                    TraceContext::new(request.command_id.as_uuid()).with_run_id(run_id),
                );
                let memory_snapshot = state
                    .dependencies
                    .knowledge
                    .read_context(&actor, &read_scope)
                    .await?;
                let context = AgentContext {
                    projection_version: 1,
                    persona: None,
                    memories: memory_snapshot.memories,
                    optional_context_issues: memory_snapshot
                        .issue
                        .map(|reason| floe_agent_contract::ContextIssue {
                            source: floe_agent_contract::ContextSource::Memory,
                            reason,
                        })
                        .into_iter()
                        .collect(),
                    evidence: vec![],
                };
                context.validate()?;
                let projection = ConversationModelProjection::new(
                    state.dependencies.evidence.clone(),
                    state.dependencies.resolver.clone(),
                    request.session_id,
                    context.clone(),
                    session.data_classes,
                    environment.identity(),
                )?;
                request.delegation_context = Some(floe_agent_contract::DelegationContextInput {
                    session_id: request.session_id,
                    device_id: actor.device_id.clone(),
                    agent_context: context,
                    max_output_bytes: state.config.max_output_bytes,
                });
                state
                    .coordinator
                    .drive_run(
                        &actor,
                        request,
                        ConversationPorts {
                            projection: &projection,
                            coverage_resolver: state.dependencies.resolver.as_ref(),
                            model: state.dependencies.model.as_ref(),
                            tools: &super::manager_policy::NoManagerTools,
                            delegation: environment.as_ref(),
                            validator: &super::manager_policy::ManagerPayloadValidator,
                        },
                        prepared,
                        &driver_guard,
                    )
                    .await
            }
            .await;
            let settled = match outcome {
                Ok(receipt) => Ok(receipt),
                Err(failure) => match state.dependencies.repository.load_receipt(run_id).await {
                    Ok(Some(receipt)) if receipt.state == RunState::Working => {
                        let settled = state
                            .dependencies
                            .repository
                            .finish_run(
                                run_id,
                                receipt.aggregate_revision,
                                RunTerminal::from_failure(failure),
                            )
                            .await;
                        settled
                    }
                    Ok(Some(receipt)) => Ok(receipt),
                    Ok(None) => Err(AgentFailure::StorageUnavailable),
                    Err(failure) => Err(failure),
                },
            };
            if let Ok(receipt) = settled {
                let _ = state.events.publish_run(&receipt);
            }
        });
        Ok(CommandReceipt::from(&receipt))
    }

    async fn interaction(
        &self,
        actor: &OwnerActor,
        interaction_id: Uuid,
        session_id: Option<Uuid>,
        scope: &ExecutionScope,
    ) -> Result<Option<ConversationInteraction>, AgentFailure> {
        self.check(actor)?;
        if interaction_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let record = scope
            .run(
                self.dependencies
                    .repository
                    .get_interaction(actor.person_id, interaction_id),
            )
            .await?;
        if let Some(record) = &record {
            record.validate()?;
            let run = scope
                .run(
                    self.dependencies
                        .repository
                        .load_receipt(record.origin_run_id),
                )
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if record.person_id != actor.person_id
                || run.principal != actor.person_id.to_string()
                || run.device_id != actor.device_id
                || session_id.is_some_and(|id| id != record.session_id)
            {
                return Err(AgentFailure::PolicyDenied);
            }
        }
        Ok(record)
    }
}

impl<R, S, T> ConversationOwner for ConversationService<R, S, T>
where
    R: ConversationRepository + InteractionRepository + SessionRepository + 'static,
    S: SessionStore + Send + Sync + 'static,
    T: floe_experts::TaskRepository + 'static,
{
    fn close_admission(&self) {
        self.inner.closing.store(true, Ordering::Release);
        self.inner
            .shutdown
            .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
    }
    #[cfg(feature = "qa-fixtures")]
    fn qa_lose_next_operation_approval_admission_ack(&self) {
        self.inner
            .lose_next_operation_admission_ack
            .store(true, Ordering::Release);
    }
    #[cfg(feature = "qa-fixtures")]
    fn qa_pause_recovery<'a>(&'a self) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.inner
                .recovery_pause_requested
                .store(true, Ordering::Release);
            self.inner.recovery_wake.notify_one();
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let notified = self.inner.recovery_pause_ack.notified();
                    if self
                        .inner
                        .recovery_pause_acknowledged
                        .load(Ordering::Acquire)
                    {
                        return;
                    }
                    notified.await;
                }
            })
            .await
            .map_err(|_| AgentFailure::DeadlineExceeded)
        })
    }
    #[cfg(feature = "qa-fixtures")]
    fn qa_read_interaction_record<'a>(
        &'a self,
        actor: &'a OwnerActor,
        interaction_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<ConversationInteraction>, AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            scope
                .run(
                    self.inner
                        .dependencies
                        .repository
                        .get_interaction(actor.person_id, interaction_id),
                )
                .await
        })
    }
    fn start_session<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::SessionStartAdmission, crate::SessionStartFailure>> {
        Box::pin(async move {
            let _command = self
                .inner
                .lock_command(command_id)
                .await
                .map_err(crate::SessionStartFailure::NotAdmitted)?;
            let _admission = self.inner.admission.read().await;
            self.inner
                .check(actor)
                .map_err(crate::SessionStartFailure::NotAdmitted)?;
            scope
                .run(async {
                    Ok(crate::start_session(
                        self.inner.dependencies.repository.as_ref(),
                        StartSessionRequest {
                            principal: actor.person_id.to_string(),
                            command_id,
                        },
                    )
                    .await)
                })
                .await
                .map_err(crate::SessionStartFailure::Indeterminate)?
        })
    }
    fn calendar_operation_policy<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_access::OperationAuthorizationPolicy, AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            self.inner
                .dependencies
                .calendar_operations
                .inspect_authority(actor, scope)
                .await
        })
    }
    fn set_calendar_operation_policy<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        mode: floe_access::OperationPolicyMode,
        expected_revision: u64,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<floe_access::OperationAuthorizationPolicy, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async move {
            self.inner
                .check(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            self.inner
                .dependencies
                .calendar_operations
                .set_calendar_create_authority(
                    actor,
                    command_id.as_uuid(),
                    mode,
                    expected_revision,
                    scope,
                )
                .await
        })
    }
    fn resume_session<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<SessionSnapshot>, AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            let receipt = scope
                .run(crate::resume_session(
                    self.inner.dependencies.repository.as_ref(),
                    SessionRequest {
                        principal: actor.person_id.to_string(),
                    },
                ))
                .await?;
            self.inner.check(actor)?;
            match receipt {
                Some(receipt) => self
                    .get_session(actor, receipt.session_id, None, scope)
                    .await
                    .map(Some),
                None => Ok(None),
            }
        })
    }
    fn get_session<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
        before_message_id: Option<Uuid>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<SessionSnapshot, AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            read_session_snapshot(
                self.inner.dependencies.repository.as_ref(),
                self.inner.dependencies.sessions.as_ref(),
                actor,
                session_id,
                before_message_id,
                scope,
            )
            .await
        })
    }
    fn start_turn<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: StartTurn,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CommandReceipt, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            let _command = self
                .inner
                .lock_command(request.command_id)
                .await
                .map_err(CommandFailure::NotAdmitted)?;
            self.inner
                .check(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            let prepared = prepare_start_turn(
                self.inner.dependencies.repository.as_ref(),
                self.inner.dependencies.sessions.as_ref(),
                actor,
                &request,
                scope,
            )
            .await?;
            if let Some(existing) = prepared.existing {
                return Ok(CommandReceipt::from(&existing));
            }
            let session = prepared.session.ok_or(CommandFailure::Indeterminate(
                AgentFailure::StorageUnavailable,
            ))?;
            self.inner
                .submit(actor, request.command_id, prepared.intent, session, scope)
                .await
        })
    }
    fn submit_calendar_proposal<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        request: CalendarProposalRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CalendarProposalResult, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            let _command = self
                .inner
                .lock_command(command_id)
                .await
                .map_err(CommandFailure::NotAdmitted)?;
            self.inner
                .check(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            let admission = self
                .inner
                .dependencies
                .calendar_operations
                .prepare_expert_proposal(
                    actor,
                    command_id.as_uuid(),
                    request.receipt.clone(),
                    request.artifact_id,
                    request.destination_ref,
                    request.session_id,
                    request.origin_run_id.as_uuid(),
                    scope,
                )
                .await?;
            let admitted = scope
                .run(async {
                    Ok(self
                        .inner
                        .dependencies
                        .repository
                        .admit_operation_approval(
                            admission,
                            OperationApprovalPublication {
                                run_id: request.origin_run_id,
                                session_id: request.session_id,
                                execution: request.receipt,
                                artifact_id: request.artifact_id,
                            },
                        )
                        .await)
                })
                .await
                .map_err(CommandFailure::Indeterminate)??;
            #[cfg(feature = "qa-fixtures")]
            if self
                .inner
                .lose_next_operation_admission_ack
                .swap(false, Ordering::AcqRel)
            {
                return Err(CommandFailure::Indeterminate(
                    AgentFailure::StorageUnavailable,
                ));
            }
            let operation = self
                .inner
                .dependencies
                .calendar_operations
                .complete_admission(actor, admitted.operation.clone(), scope)
                .await?;
            let interaction = if let Some(record) = admitted.interaction.as_ref() {
                Some(
                    super::interaction_projection::project_interaction(
                        self.inner.dependencies.connections.as_ref(),
                        self.inner.dependencies.experts_owner.as_ref(),
                        self.inner.dependencies.calendar_operations.as_ref(),
                        actor,
                        record,
                        chrono::Utc::now().timestamp_millis(),
                        scope,
                    )
                    .await
                    .map_err(CommandFailure::Admitted)?,
                )
            } else {
                None
            };
            Ok(CalendarProposalResult {
                operation,
                interaction,
            })
        })
    }
    fn cancel_run<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        run_id: RunId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CancelRunReceipt, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            let _command = self
                .inner
                .lock_command(command_id)
                .await
                .map_err(CommandFailure::NotAdmitted)?;
            let _admission = self.inner.admission.read().await;
            self.inner
                .check(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            scope
                .run(async {
                    Ok(cancel_run_command(
                        self.inner.dependencies.repository.as_ref(),
                        &self.inner.cancellations,
                        CancelRunCommand {
                            command_id,
                            run_id,
                            principal: actor.person_id.to_string(),
                        },
                    )
                    .await)
                })
                .await
                .map_err(CommandFailure::Indeterminate)??;
            Ok(CancelRunReceipt {
                command_id,
                run_id,
                principal: actor.person_id.to_string(),
            })
        })
    }
    fn read_command<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            scope
                .run(crate::get_command(
                    self.inner.dependencies.repository.as_ref(),
                    CommandQuery {
                        principal: actor.person_id.to_string(),
                        command_id,
                    },
                ))
                .await
        })
    }
    fn read_run<'a>(
        &'a self,
        actor: &'a OwnerActor,
        run_id: RunId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            scope
                .run(crate::get_run(
                    self.inner.dependencies.repository.as_ref(),
                    RunQuery {
                        principal: actor.person_id.to_string(),
                        run_id,
                    },
                ))
                .await
        })
    }
    fn read_message<'a>(
        &'a self,
        actor: &'a OwnerActor,
        message_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<MessageSnapshot>, AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            let run_id = RunId::from_uuid(message_id).ok_or(AgentFailure::InvalidInput)?;
            if let Some(receipt) = self.read_run(actor, run_id, scope).await? {
                if let Some(text) = receipt.output {
                    return Ok(Some(MessageSnapshot {
                        message_id,
                        role: MessageSnapshotRole::Assistant,
                        text,
                    }));
                }
            }
            let command_id = CommandId::from_uuid(message_id).ok_or(AgentFailure::InvalidInput)?;
            let Some(receipt) = self.read_command(actor, command_id, scope).await? else {
                return Ok(None);
            };
            if receipt.user_message_id != message_id {
                return Ok(None);
            }
            let admitted = scope
                .run(self.inner.dependencies.repository.load_run(receipt.run_id))
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            Ok(admitted
                .transcript
                .into_iter()
                .find(|message| {
                    message.message_id == message_id
                        && message.role == floe_agent_contract::MessageRole::User
                })
                .map(|message| MessageSnapshot {
                    message_id,
                    role: MessageSnapshotRole::User,
                    text: message.text,
                }))
        })
    }
    fn read_interaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        interaction_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<InteractionSnapshot>, AgentFailure>> {
        Box::pin(async move {
            let Some(record) = self
                .inner
                .interaction(actor, interaction_id, None, scope)
                .await?
            else {
                return Ok(None);
            };
            super::interaction_projection::project_interaction(
                self.inner.dependencies.connections.as_ref(),
                self.inner.dependencies.experts_owner.as_ref(),
                self.inner.dependencies.calendar_operations.as_ref(),
                actor,
                &record,
                chrono::Utc::now().timestamp_millis(),
                scope,
            )
            .await
            .map(Some)
        })
    }
    fn list_interactions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        session_id: Uuid,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<InteractionSnapshot>, AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            // Validate the exact product Session through a single normalized
            // entry or its pristine header. Never hydrate AgentSession's
            // growing messages vector to enumerate interaction owners.
            scope
                .run(
                    self.inner
                        .dependencies
                        .repository
                        .read_session_history_page(session_id, None, 1, crate::MAX_SESSION_BYTES),
                )
                .await?;
            let mut records = scope
                .run(
                    self.inner
                        .dependencies
                        .repository
                        .list_session_interactions(actor.person_id, session_id, &actor.device_id),
                )
                .await?;
            records.sort_by_key(|record| (record.created_at_unix_ms, record.id));
            let mut snapshots = Vec::new();
            for record in records {
                snapshots.push(
                    super::interaction_projection::project_interaction(
                        self.inner.dependencies.connections.as_ref(),
                        self.inner.dependencies.experts_owner.as_ref(),
                        self.inner.dependencies.calendar_operations.as_ref(),
                        actor,
                        &record,
                        chrono::Utc::now().timestamp_millis(),
                        scope,
                    )
                    .await?,
                );
            }
            Ok(snapshots)
        })
    }
    fn resolve_interaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: ResolveInteraction,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<InteractionResult, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            let command_id = CommandId::from_uuid(request.command_id)
                .ok_or(CommandFailure::NotApplied(AgentFailure::InvalidInput))?;
            let _command = self
                .inner
                .lock_command(command_id)
                .await
                .map_err(CommandFailure::NotAdmitted)?;
            self.inner
                .check(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            let _wake = recovery_driver::WakeOnDrop(&self.inner.recovery_wake);
            let admission = self.inner.admission.read().await;
            let interaction = scope
                .run(async {
                    Ok(apply_source_interaction(
                        self.inner.dependencies.repository.as_ref(),
                        self.inner.dependencies.connections.as_ref(),
                        self.inner.dependencies.calendar_operations.as_ref(),
                        actor,
                        DecideInteractionCommand {
                            command_id: request.command_id,
                            interaction_id: request.interaction_id,
                            principal: actor.person_id.to_string(),
                            expected_revision: request.expected_revision,
                            kind: request.decision,
                            target_digest: request.target_digest,
                        },
                        chrono::Utc::now().timestamp_millis(),
                        scope,
                    )
                    .await)
                })
                .await
                .map_err(CommandFailure::Indeterminate)??;
            drop(admission);
            self.inner.recovery_wake.notify_one();
            let linked = self
                .read_command(
                    actor,
                    resume_command_id(interaction.origin_run_id)
                        .map_err(CommandFailure::Admitted)?,
                    scope,
                )
                .await
                .map_err(CommandFailure::Admitted)?
                .as_ref()
                .map(CommandReceipt::from);
            let interaction = super::interaction_projection::project_interaction(
                self.inner.dependencies.connections.as_ref(),
                self.inner.dependencies.experts_owner.as_ref(),
                self.inner.dependencies.calendar_operations.as_ref(),
                actor,
                &interaction,
                chrono::Utc::now().timestamp_millis(),
                scope,
            )
            .await
            .map_err(CommandFailure::Admitted)?;
            Ok(InteractionResult {
                interaction,
                linked,
            })
        })
    }
    fn refresh_interaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: RefreshInteraction,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<InteractionResult, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            let command_id = CommandId::from_uuid(request.command_id)
                .ok_or(CommandFailure::NotApplied(AgentFailure::InvalidInput))?;
            let _command = self
                .inner
                .lock_command(command_id)
                .await
                .map_err(CommandFailure::NotAdmitted)?;
            self.inner
                .check(actor)
                .map_err(CommandFailure::NotAdmitted)?;
            let _wake = recovery_driver::WakeOnDrop(&self.inner.recovery_wake);
            let admission = self.inner.admission.read().await;
            let stored = scope
                .run(async {
                    Ok(self
                        .inner
                        .dependencies
                        .repository
                        .admit_refresh(InteractionRefresh {
                            command_id: request.command_id,
                            person_id: actor.person_id,
                            session_id: request.session_id,
                            interaction_id: request.interaction_id,
                            expected_revision: request.expected_revision,
                        })
                        .await)
                })
                .await
                .map_err(CommandFailure::Indeterminate)??;
            let result = async {
                let interaction = if matches!(stored.target, ReviewedTarget::ExpertBinding(_)) {
                    super::interaction_resolution::recover_binding_interaction(
                        self.inner.dependencies.repository.as_ref(),
                        self.inner.dependencies.experts_owner.as_ref(),
                        actor,
                        &request,
                        chrono::Utc::now().timestamp_millis(),
                        scope,
                    )
                    .await?
                } else {
                    recover_source_interaction(
                        self.inner.dependencies.repository.as_ref(),
                        self.inner.dependencies.connections.as_ref(),
                        self.inner.dependencies.calendar_operations.as_ref(),
                        actor,
                        request.interaction_id,
                        chrono::Utc::now().timestamp_millis(),
                        scope,
                    )
                    .await?
                };
                drop(admission);
                self.inner.recovery_wake.notify_one();
                let linked = self
                    .read_command(actor, resume_command_id(interaction.origin_run_id)?, scope)
                    .await?
                    .as_ref()
                    .map(CommandReceipt::from);
                let interaction = super::interaction_projection::project_interaction(
                    self.inner.dependencies.connections.as_ref(),
                    self.inner.dependencies.experts_owner.as_ref(),
                    self.inner.dependencies.calendar_operations.as_ref(),
                    actor,
                    &interaction,
                    chrono::Utc::now().timestamp_millis(),
                    scope,
                )
                .await?;
                Ok::<_, AgentFailure>(InteractionResult {
                    interaction,
                    linked,
                })
            }
            .await
            .map_err(CommandFailure::Admitted)?;
            Ok(result)
        })
    }
    fn read_events<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: ReadConversationEvents,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<EventRead, AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            self.inner.events.read(actor, &request)
        })
    }
    fn activate<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.inner.check(actor)?;
            self.inner.start_recovery(actor, scope).await?;
            Ok(())
        })
    }
    fn shutdown<'a>(
        &'a self,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.close_admission();
            {
                let _barrier = self.inner.admission.write().await;
            }
            let mut tasks = self.inner.tasks.lock().await;
            while !tasks.is_empty() {
                match tokio::time::timeout_at(scope.deadline(), tasks.join_next()).await {
                    Ok(Some(Ok(()))) => {}
                    Ok(Some(Err(_))) => return Err(AgentFailure::Interrupted),
                    Ok(None) => break,
                    Err(_) => return Err(AgentFailure::DeadlineExceeded),
                }
            }
            Ok(())
        })
    }
}
