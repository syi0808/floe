use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use floe_agent_contract::prompts::{
    PromptAssembly, PromptComponent, PromptComponentKind, PromptRole,
};
use floe_agent_contract::{
    AgentMessage, AllowedCatalog, AuthorizedModelProjection, BoxFuture, ContextEnvelope,
    ContextManifest, ContextualData, DataClass, DelegationPort, DelegationRequest,
    DependencyCoverage, ExecutionJournal, JournalAck, JournalEvent, ModelCallOutcome,
    ModelConversation, ModelConversationEntry, ModelPort, ModelProjectionPort,
    ModelProjectionRequest, ModelRequest, ModelResponse, ModelStep, ModelUsage, ProjectionRef,
    RoleSpec, RuntimeContext, ScopedInstructions, TaskReceipt, ToolCall, ToolDescriptor, ToolPort,
    ToolResult,
};
use floe_agent_runtime::FinalPayloadValidator;
use floe_execution::{ExecutionScope, budget::BudgetConfig};
use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    AdmittedTurn, CancelCommandRequest, CancelRunAdmission, CancelRunCommand, CancelRunReceipt,
    CancelRunRequest, CancelRunStatus, ConversationInteraction, ConversationPorts,
    ConversationRepository, DecisionAdmission, ExpireInteraction, ExpireOutcome,
    InteractionDecision, InteractionRepository, InteractionResolution, InteractionState,
    JournalEntry, MAX_ACTIVE_INTERACTIONS_PER_RUN, MAX_STORED_INTERACTIONS_PER_RUN, ManagerConfig,
    PublishAdmission, RecoveryReceipt, RecoveryRequest, RunCancellationRegistry, RunReceipt,
    RunState, RunTerminal, SupersedeInteraction, TurnAdmission, TurnAdmissionRequest, TurnRequest,
    next_state_after_decision, state_after_resolution,
};

use super::ConversationService;

struct Session {
    principal: String,
    revision: u64,
    active_run: Option<RunId>,
    transcript: Vec<AgentMessage>,
}

struct StoredRun {
    admitted: AdmittedTurn,
}

#[derive(Default)]
struct State {
    sessions: HashMap<Uuid, Session>,
    commands: HashMap<CommandId, RunId>,
    cancellations: HashMap<CommandId, CancelRunReceipt>,
    runs: HashMap<RunId, StoredRun>,
    interactions: HashMap<Uuid, ConversationInteraction>,
    decisions: HashMap<Uuid, InteractionDecision>,
}

#[derive(Default)]
struct MemoryRepository {
    state: Mutex<State>,
    journal: Arc<Journal>,
}

impl MemoryRepository {
    fn add_session(&self, session_id: Uuid, principal: &str) {
        self.state.lock().unwrap().sessions.insert(
            session_id,
            Session {
                principal: principal.into(),
                revision: 0,
                active_run: None,
                transcript: vec![],
            },
        );
    }
}

impl ConversationRepository for MemoryRepository {
    fn find_command<'a>(
        &'a self,
        query: crate::CommandQuery,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
        Box::pin(async move {
            let state = self.state.lock().unwrap();
            Ok(state
                .commands
                .get(&query.command_id)
                .and_then(|run_id| state.runs.get(run_id))
                .map(|stored| stored.admitted.receipt.clone()))
        })
    }

    fn admit_turn<'a>(
        &'a self,
        request: TurnAdmissionRequest,
    ) -> BoxFuture<'a, Result<TurnAdmission, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            let mut state = self.state.lock().unwrap();
            if let Some(run_id) = state.commands.get(&request.command_id) {
                let receipt = state.runs.get(run_id).unwrap().admitted.receipt.clone();
                return Ok(TurnAdmission::Existing(receipt));
            }
            let (continuation_of, continuation_level) = match &request.mode {
                crate::TurnMode::New => (None, 0),
                crate::TurnMode::Continue(reference) => {
                    let source = state
                        .runs
                        .get(&reference.run_id)
                        .ok_or(AgentFailure::Conflict)?;
                    if source.admitted.receipt.continuation().as_ref() != Some(reference)
                        || source.admitted.receipt.session_id != request.session_id
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    (Some(reference.run_id), reference.level)
                }
            };
            let session = state
                .sessions
                .get_mut(&request.session_id)
                .ok_or(AgentFailure::NotFound)?;
            if session.principal != request.principal
                || session.revision != request.expected_session_revision
                || session.active_run.is_some()
            {
                return Err(AgentFailure::Conflict);
            }
            session.revision += 1;
            session.active_run = Some(request.run_id);
            if matches!(request.mode, crate::TurnMode::New) {
                session.transcript.push(request.user_message);
            }
            let receipt = RunReceipt {
                run_id: request.run_id,
                command_id: request.command_id,
                session_id: request.session_id,
                principal: request.principal,
                request_digest: request.request_digest,
                state: RunState::Working,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: None,
                session_revision: session.revision,
                aggregate_revision: 1,
                executor_generation: 1,
                continuation_of,
                continuation_executor_generation: match &request.mode {
                    crate::TurnMode::New => None,
                    crate::TurnMode::Continue(reference) => Some(reference.executor_generation),
                },
                continuation_level,
                retry_of: request.retry_of,
                profile: request.profile,
                attempt_refs: vec![],
                task_refs: vec![],
            };
            receipt.validate()?;
            let admitted = AdmittedTurn {
                receipt,
                transcript: session.transcript.clone(),
            };
            state.commands.insert(request.command_id, request.run_id);
            state.runs.insert(
                request.run_id,
                StoredRun {
                    admitted: admitted.clone(),
                },
            );
            Ok(TurnAdmission::Created(admitted))
        })
    }

    fn admit_cancel<'a>(
        &'a self,
        request: CancelRunCommand,
    ) -> BoxFuture<'a, Result<CancelRunAdmission, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            let mut state = self.state.lock().unwrap();
            if state.commands.contains_key(&request.command_id) {
                return Err(AgentFailure::Conflict);
            }
            if let Some(receipt) = state.cancellations.get(&request.command_id) {
                return if receipt.run_id == request.run_id && receipt.principal == request.principal
                {
                    Ok(CancelRunAdmission::Existing(receipt.clone()))
                } else {
                    Err(AgentFailure::Conflict)
                };
            }
            let run = state
                .runs
                .get(&request.run_id)
                .ok_or(AgentFailure::NotFound)?;
            if run.admitted.receipt.principal != request.principal {
                return Err(AgentFailure::CapabilityDenied);
            }
            let receipt = CancelRunReceipt {
                command_id: request.command_id,
                run_id: request.run_id,
                principal: request.principal,
            };
            state
                .cancellations
                .insert(receipt.command_id, receipt.clone());
            Ok(CancelRunAdmission::Created(receipt))
        })
    }

    fn journal(&self, run_id: RunId) -> Result<Arc<dyn ExecutionJournal>, AgentFailure> {
        self.state
            .lock()
            .unwrap()
            .runs
            .contains_key(&run_id)
            .then(|| self.journal.clone() as Arc<dyn ExecutionJournal>)
            .ok_or(AgentFailure::NotFound)
    }

    fn finish_run<'a>(
        &'a self,
        run_id: RunId,
        expected_aggregate_revision: u64,
        terminal: RunTerminal,
    ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>> {
        Box::pin(async move {
            terminal.validate()?;
            let mut state = self.state.lock().unwrap();
            let (session_id, current_revision) = {
                let stored = state.runs.get(&run_id).ok_or(AgentFailure::NotFound)?;
                (
                    stored.admitted.receipt.session_id,
                    stored.admitted.receipt.aggregate_revision,
                )
            };
            if current_revision != expected_aggregate_revision {
                return Err(AgentFailure::Conflict);
            }
            let session = state.sessions.get_mut(&session_id).unwrap();
            if session.active_run != Some(run_id) {
                return Err(AgentFailure::Conflict);
            }
            session.active_run = None;
            session.revision += 1;
            if let Some(output) = terminal.output.as_ref() {
                session.transcript.push(AgentMessage {
                    message_id: Uuid::new_v4(),
                    role: floe_agent_contract::MessageRole::Assistant,
                    text: output.clone(),
                    call_id: None,
                    coverage: terminal.coverage.clone(),
                });
            }
            let session_revision = session.revision;
            let transcript = session.transcript.clone();
            let stored = state.runs.get_mut(&run_id).unwrap();
            stored.admitted.receipt.state = terminal.state;
            stored.admitted.receipt.output = terminal.output;
            stored.admitted.receipt.coverage = terminal.coverage;
            stored.admitted.receipt.issue = terminal.issue;
            stored.admitted.receipt.session_revision = session_revision;
            stored.admitted.receipt.aggregate_revision += 1;
            stored.admitted.transcript = transcript;
            stored.admitted.receipt.validate()?;
            Ok(stored.admitted.receipt.clone())
        })
    }

    fn load_run<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Option<AdmittedTurn>, AgentFailure>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .unwrap()
                .runs
                .get(&run_id)
                .map(|stored| stored.admitted.clone()))
        })
    }

    fn load_receipt<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .unwrap()
                .runs
                .get(&run_id)
                .map(|stored| stored.admitted.receipt.clone()))
        })
    }

    fn recover_session<'a>(
        &'a self,
        request: RecoveryRequest,
    ) -> BoxFuture<'a, Result<RecoveryReceipt, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            let state = self.state.lock().unwrap();
            let session = state
                .sessions
                .get(&request.session_id)
                .ok_or(AgentFailure::NotFound)?;
            if session.principal != request.principal {
                return Err(AgentFailure::CapabilityDenied);
            }
            if session.revision != request.expected_session_revision {
                return Err(AgentFailure::Conflict);
            }
            if session.active_run.is_some() {
                return Err(AgentFailure::Conflict);
            }
            Ok(RecoveryReceipt {
                session_id: request.session_id,
                session_revision: session.revision,
            })
        })
    }

    fn load_journal<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Vec<JournalEntry>, AgentFailure>> {
        Box::pin(async move {
            if !self.state.lock().unwrap().runs.contains_key(&run_id) {
                return Err(AgentFailure::NotFound);
            }
            Ok(self
                .journal
                .events
                .lock()
                .unwrap()
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, event)| JournalEntry {
                    revision: index as u64 + 1,
                    event,
                })
                .collect())
        })
    }
}

impl InteractionRepository for MemoryRepository {
    fn publish_interaction<'a>(
        &'a self,
        record: ConversationInteraction,
    ) -> BoxFuture<'a, Result<PublishAdmission, AgentFailure>> {
        Box::pin(async move {
            record.validate()?;
            let mut state = self.state.lock().unwrap();
            if let Some(existing) = state.interactions.get(&record.id) {
                if existing.requirement_digest != record.requirement_digest
                    || existing.target_digest != record.target_digest
                    || existing.origin_run_id != record.origin_run_id
                    || existing.origin != record.origin
                {
                    return Err(AgentFailure::VaultUnavailable);
                }
                return Ok(PublishAdmission::Existing(existing.clone()));
            }
            let origin = state
                .runs
                .get(&record.origin_run_id)
                .ok_or(AgentFailure::NotFound)?;
            if origin.admitted.receipt.session_id != record.session_id {
                return Err(AgentFailure::Conflict);
            }
            if origin.admitted.receipt.state == RunState::Cancelled {
                return Err(AgentFailure::Conflict);
            }
            let stored = state
                .interactions
                .values()
                .filter(|entry| entry.origin_run_id == record.origin_run_id)
                .count();
            if stored >= MAX_STORED_INTERACTIONS_PER_RUN {
                return Err(AgentFailure::BudgetExceeded);
            }
            let active = state
                .interactions
                .values()
                .filter(|entry| {
                    entry.origin_run_id == record.origin_run_id && !entry.state.is_terminal()
                })
                .count();
            if active >= MAX_ACTIVE_INTERACTIONS_PER_RUN {
                return Err(AgentFailure::BudgetExceeded);
            }
            state.interactions.insert(record.id, record.clone());
            Ok(PublishAdmission::Created(record))
        })
    }

    fn get_interaction<'a>(
        &'a self,
        person_id: PersonId,
        interaction_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<ConversationInteraction>, AgentFailure>> {
        Box::pin(async move {
            if interaction_id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            let state = self.state.lock().unwrap();
            Ok(state
                .interactions
                .get(&interaction_id)
                .filter(|record| record.person_id == person_id)
                .cloned())
        })
    }

    fn list_run_interactions<'a>(
        &'a self,
        person_id: PersonId,
        origin_run_id: RunId,
    ) -> BoxFuture<'a, Result<Vec<ConversationInteraction>, AgentFailure>> {
        Box::pin(async move {
            if !origin_run_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            let state = self.state.lock().unwrap();
            let mut records: Vec<ConversationInteraction> = state
                .interactions
                .values()
                .filter(|record| {
                    record.person_id == person_id && record.origin_run_id == origin_run_id
                })
                .cloned()
                .collect();
            if records.len() > MAX_STORED_INTERACTIONS_PER_RUN {
                return Err(AgentFailure::StorageUnavailable);
            }
            records.sort_by(|left, right| {
                (left.created_at_unix_ms, left.id).cmp(&(right.created_at_unix_ms, right.id))
            });
            Ok(records)
        })
    }

    fn record_decision<'a>(
        &'a self,
        decision: InteractionDecision,
    ) -> BoxFuture<'a, Result<DecisionAdmission, AgentFailure>> {
        Box::pin(async move {
            decision.validate()?;
            let mut state = self.state.lock().unwrap();
            if let Some(recorded) = state.decisions.get(&decision.command_id) {
                if !decision.matches_recorded(recorded) {
                    return Err(AgentFailure::Conflict);
                }
                let current = state
                    .interactions
                    .get(&decision.interaction_id)
                    .cloned()
                    .ok_or(AgentFailure::VaultUnavailable)?;
                return Ok(DecisionAdmission::Rejoined(current));
            }
            let current = state
                .interactions
                .get(&decision.interaction_id)
                .cloned()
                .ok_or(AgentFailure::NotFound)?;
            if decision.principal != current.person_id.to_string() {
                return Err(AgentFailure::CapabilityDenied);
            }
            if current.revision != decision.interaction_revision
                || current.target_digest != decision.target_digest
                || decision.decided_at_unix_ms < current.created_at_unix_ms
                || decision.decided_at_unix_ms >= current.expires_at_unix_ms
            {
                return Err(AgentFailure::Conflict);
            }
            let mut updated = current;
            updated.state = next_state_after_decision(&updated.state, &decision)?;
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            state.decisions.insert(decision.command_id, decision);
            state.interactions.insert(updated.id, updated.clone());
            Ok(DecisionAdmission::Applied(updated))
        })
    }

    fn record_resolution<'a>(
        &'a self,
        resolution: InteractionResolution,
    ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>> {
        Box::pin(async move {
            resolution.validate()?;
            let mut state = self.state.lock().unwrap();
            let recorded = state
                .decisions
                .get(&resolution.decision_id)
                .cloned()
                .ok_or(AgentFailure::Conflict)?;
            if recorded.interaction_id != resolution.interaction_id
                || resolution.resolved_at_unix_ms < recorded.decided_at_unix_ms
            {
                return Err(AgentFailure::Conflict);
            }
            let current = state
                .interactions
                .get(&resolution.interaction_id)
                .cloned()
                .ok_or(AgentFailure::NotFound)?;
            if resolution.person_id != current.person_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            if current.revision != resolution.expected_revision {
                return Err(AgentFailure::Conflict);
            }
            let mut updated = current;
            updated.state = state_after_resolution(
                &updated.state,
                resolution.decision_id,
                resolution.owner_operation_id,
                resolution.resolved_at_unix_ms,
            )?;
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            state.interactions.insert(updated.id, updated.clone());
            Ok(updated)
        })
    }

    fn mark_superseded<'a>(
        &'a self,
        supersede: SupersedeInteraction,
    ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>> {
        Box::pin(async move {
            supersede.validate()?;
            let mut state = self.state.lock().unwrap();
            let current = state
                .interactions
                .get(&supersede.interaction_id)
                .cloned()
                .ok_or(AgentFailure::NotFound)?;
            if supersede.person_id != current.person_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            if current.revision != supersede.expected_revision || current.state.is_terminal() {
                return Err(AgentFailure::Conflict);
            }
            let mut updated = current;
            updated.state = InteractionState::Superseded {
                superseded_by: supersede.superseded_by,
            };
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            state.interactions.insert(updated.id, updated.clone());
            Ok(updated)
        })
    }

    fn mark_expired<'a>(
        &'a self,
        expire: ExpireInteraction,
    ) -> BoxFuture<'a, Result<ExpireOutcome, AgentFailure>> {
        Box::pin(async move {
            expire.validate()?;
            let mut state = self.state.lock().unwrap();
            let current = state
                .interactions
                .get(&expire.interaction_id)
                .cloned()
                .ok_or(AgentFailure::NotFound)?;
            if expire.person_id != current.person_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            if current.state.is_terminal() {
                return Ok(ExpireOutcome::AlreadyTerminal(current));
            }
            if expire.now_unix_ms < current.expires_at_unix_ms {
                return Ok(ExpireOutcome::NotExpired(current));
            }
            let mut updated = current;
            updated.state = InteractionState::Expired;
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            state.interactions.insert(updated.id, updated.clone());
            Ok(ExpireOutcome::Expired(updated))
        })
    }
}

#[derive(Default)]
struct Journal {
    revision: std::sync::atomic::AtomicU64,
    events: Mutex<Vec<JournalEvent>>,
}

impl Journal {
    fn accepted(&self, event: JournalEvent) -> JournalAck {
        self.events.lock().unwrap().push(event);
        JournalAck::Accepted {
            revision: self
                .revision
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                + 1,
        }
    }
}

impl ExecutionJournal for Journal {
    fn record_intent<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move { Ok(self.accepted(event)) })
    }

    fn record_result<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move { Ok(self.accepted(event)) })
    }

    fn record_output<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move { Ok(self.accepted(event)) })
    }

    fn checkpoint<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move { Ok(self.accepted(event)) })
    }
}

struct TestProjector;

static PROJECTOR: TestProjector = TestProjector;

fn authorized_test_projection(
    request: &ModelProjectionRequest,
    conversation: ModelConversation,
    coverage: DependencyCoverage,
) -> AuthorizedModelProjection {
    let envelope = ContextEnvelope {
        schema_version: floe_agent_contract::AGENT_VERSION,
        stable_instructions: PromptAssembly {
            schema_version: floe_agent_contract::AGENT_VERSION,
            role: PromptRole::Manager,
            components: vec![
                PromptComponent {
                    kind: PromptComponentKind::BehaviorKernel,
                    source: "test-kernel".into(),
                    revision: 1,
                    content: "kernel".into(),
                },
                PromptComponent {
                    kind: PromptComponentKind::Role,
                    source: "test-role".into(),
                    revision: 1,
                    content: "role".into(),
                },
                PromptComponent {
                    kind: PromptComponentKind::CapabilityProtocol,
                    source: "test-protocol".into(),
                    revision: 1,
                    content: "protocol".into(),
                },
            ],
        },
        scoped_instructions: ScopedInstructions {
            purpose: "test-purpose".into(),
            response_contract: request.role.output_contract.clone(),
            available_capabilities: vec![],
            active_experts: vec![],
            correction: request.correction.clone(),
        },
        contextual_data: ContextualData {
            projection_version: 1,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        },
        conversation,
        runtime: RuntimeContext {
            max_output_bytes: request.max_output_bytes,
        },
        manifest: ContextManifest {
            prompt_components: vec![],
            evidence: vec![],
            memories: vec![],
            agent_cards: vec![],
        },
    };
    AuthorizedModelProjection {
        projection_ref: ProjectionRef::new(),
        projection_revision: 1,
        envelope,
        coverage,
        input_data_classes: vec![DataClass::Synthetic],
    }
}

impl ModelProjectionPort for TestProjector {
    fn project<'a>(
        &'a self,
        request: ModelProjectionRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<AuthorizedModelProjection, AgentFailure>> {
        request.validate().unwrap();
        let conversation = request.conversation.clone();
        Box::pin(async move {
            Ok(authorized_test_projection(
                &request,
                conversation,
                DependencyCoverage::Independent,
            ))
        })
    }
}

fn current_user_text(request: &ModelRequest) -> String {
    request
        .projection
        .envelope
        .conversation
        .current_turn
        .iter()
        .find_map(|entry| match entry {
            ModelConversationEntry::User { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap()
}

#[derive(Default)]
struct AnswerModel {
    calls: std::sync::atomic::AtomicUsize,
}

impl ModelPort for AnswerModel {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelCallOutcome, AgentFailure>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            Ok(ModelCallOutcome::Ready(ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![ModelStep::Answer {
                    text: format!("answered: {}", current_user_text(&request)),
                    artifacts: vec![],
                }],
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            }))
        })
    }
}

#[derive(Default)]
struct FinalizationModel {
    calls: std::sync::atomic::AtomicUsize,
    finalization_scope: Mutex<Option<ExecutionScope>>,
}

impl ModelPort for FinalizationModel {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelCallOutcome, AgentFailure>> {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            if call == 0 {
                assert_eq!(request.catalog.tools.len(), 1);
                assert_eq!(scope.budget().max_tokens(), 4_096);
                Ok(ModelCallOutcome::Ready(ModelResponse {
                    attempt_id: request.attempt_id,
                    steps: vec![ModelStep::CallTool {
                        tool_id: "lookup".into(),
                        definition_revision: 1,
                        input: "{}".into(),
                    }],
                    usage: ModelUsage {
                        tokens: 11,
                        cost_micros: 2,
                    },
                }))
            } else {
                assert_eq!(call, 1);
                *self.finalization_scope.lock().unwrap() = Some(scope.clone());
                assert!(request.catalog.tools.is_empty());
                assert!(request.catalog.cards.is_empty());
                assert_eq!(
                    request
                        .projection
                        .envelope
                        .scoped_instructions
                        .response_contract,
                    crate::FINALIZATION_OUTPUT_CONTRACT
                );
                assert_eq!(request.purpose, "test-purpose");
                assert_eq!(request.consumer, crate::CONVERSATION_MODEL_CONSUMER);
                assert_eq!(request.preferred_profile_id, None);
                assert_eq!(request.replay.len(), 1);
                assert_eq!(scope.budget().max_tokens(), 1_024);
                Ok(ModelCallOutcome::Ready(ModelResponse {
                    attempt_id: request.attempt_id,
                    steps: vec![ModelStep::Answer {
                        text: "The lookup succeeded, but the full request did not complete.".into(),
                        artifacts: vec![],
                    }],
                    usage: ModelUsage {
                        tokens: 7,
                        cost_micros: 1,
                    },
                }))
            }
        })
    }
}

struct CountingTool {
    calls: std::sync::atomic::AtomicUsize,
    failure: Option<AgentFailure>,
}

#[derive(Default)]
struct CountingDelegation {
    calls: std::sync::atomic::AtomicUsize,
}

impl DelegationPort for CountingDelegation {
    fn delegate<'a>(
        &'a self,
        _: DelegationRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
    }
}

impl ToolPort for CountingTool {
    fn invoke<'a>(
        &'a self,
        call: ToolCall,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolResult, AgentFailure>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let failure = self.failure;
        Box::pin(async move {
            if let Some(failure) = failure {
                return Err(failure);
            }
            Ok(ToolResult {
                call_id: call.call_id,
                text: "lookup result".into(),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                issue: None,
            })
        })
    }
}

struct BlockingModel {
    calls: std::sync::atomic::AtomicUsize,
    entered: Semaphore,
    release: Semaphore,
}

impl ModelPort for BlockingModel {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelCallOutcome, AgentFailure>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            self.entered.add_permits(1);
            self.release.acquire().await.unwrap().forget();
            Ok(ModelCallOutcome::Ready(ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![ModelStep::Answer {
                    text: "done".into(),
                    artifacts: vec![],
                }],
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            }))
        })
    }
}

struct NoTools;
impl ToolPort for NoTools {
    fn invoke<'a>(
        &'a self,
        _: ToolCall,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolResult, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
    }
}

struct NoDelegation;
impl DelegationPort for NoDelegation {
    fn delegate<'a>(
        &'a self,
        _: DelegationRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
    }
}

struct Validator;
impl FinalPayloadValidator for Validator {
    fn validate(
        &self,
        role: &str,
        text: &str,
        _: &[floe_agent_contract::Artifact],
    ) -> Result<(), AgentFailure> {
        if (role == "manager" || role == crate::FINALIZATION_ROLE_ID) && !text.trim().is_empty() {
            Ok(())
        } else {
            Err(AgentFailure::InvalidModelOutput)
        }
    }
}

fn service(repository: Arc<MemoryRepository>) -> ConversationService<MemoryRepository> {
    ConversationService::new(
        repository,
        ManagerConfig {
            role_spec: RoleSpec {
                role_id: "manager".into(),
                instructions: "Answer or delegate.".into(),
                output_contract: "User-facing text.".into(),
            },
            purpose: "test-purpose".into(),
            max_iterations: 4,
            max_output_bytes: 16 * 1024,
            max_run_duration: std::time::Duration::from_secs(10),
            budget: BudgetConfig::new(16_384, 1_000_000),
        },
    )
    .unwrap()
}

fn finalization_service(
    repository: Arc<MemoryRepository>,
    max_iterations: u32,
) -> ConversationService<MemoryRepository> {
    ConversationService::new(
        repository,
        ManagerConfig {
            role_spec: RoleSpec {
                role_id: "manager".into(),
                instructions: "Answer or delegate.".into(),
                output_contract: "User-facing text.".into(),
            },
            purpose: "test-purpose".into(),
            max_iterations,
            max_output_bytes: 16 * 1024,
            max_run_duration: std::time::Duration::from_secs(10),
            budget: BudgetConfig::new(8_192, 100).with_finalization_reserve(1_024, 10),
        },
    )
    .unwrap()
}

fn delegation_context() -> floe_agent_contract::DelegationExecutionContext {
    floe_agent_contract::DelegationExecutionContext {
        session_id: Uuid::new_v4(),
        device_id: "test-device".into(),
        agent_context: floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            optional_context_issues: vec![],
            evidence: vec![],
        },
        max_output_bytes: 16 * 1024,
    }
}

fn request(
    command_id: CommandId,
    session_id: Uuid,
    expected_session_revision: u64,
    prompt: &str,
) -> TurnRequest {
    TurnRequest {
        command_id,
        session_id,
        expected_session_revision,
        principal: "person-a".into(),
        device_id: "device-a".into(),
        now_unix_ms: 1_700_000_000_000,
        prompt: prompt.into(),
        mode: crate::TurnMode::New,
        retry_of: None,
        profile: crate::ProfileSelection::Auto,
        allowed_catalog: AllowedCatalog::default(),
        replay: vec![],
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(2),
        cancellation: floe_execution::Cancellation::default(),
        delegation_context: Some(delegation_context()),
    }
}

fn ports(model: &dyn ModelPort) -> ConversationPorts<'_> {
    ConversationPorts {
        projection: &PROJECTOR,
        model,
        tools: &NoTools,
        delegation: &NoDelegation,
        validator: &Validator,
    }
}

#[tokio::test]
async fn exact_command_replay_does_not_dispatch_again_and_release_is_not_required() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = service(Arc::clone(&repository));
    let model = AnswerModel::default();
    let command_id = CommandId::new();
    let first_request = request(command_id, session_id, 0, "\thello\t");
    let first = service
        .run_turn(first_request, ports(&model))
        .await
        .unwrap();
    assert_eq!(first.state, RunState::Completed);
    assert_eq!(first.coverage, DependencyCoverage::Independent);
    assert_eq!(first.session_revision, 2);
    assert_eq!(first.output.as_deref(), Some("answered: hello"));

    {
        let state = repository.state.lock().unwrap();
        let session = state.sessions.get(&session_id).unwrap();
        assert_eq!(session.transcript[0].text, "hello");
    }

    let mut replay_request = request(command_id, session_id, 0, "hello");
    replay_request.deadline = tokio::time::Instant::now() - std::time::Duration::from_secs(1);
    let replay = service
        .run_turn(replay_request, ports(&model))
        .await
        .unwrap();
    assert_eq!(replay, first);
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    let conflicting = service
        .run_turn(
            request(command_id, session_id, 0, "different"),
            ports(&model),
        )
        .await;
    assert_eq!(conflicting, Err(AgentFailure::Conflict));

    let second = service
        .run_turn(
            request(
                CommandId::new(),
                session_id,
                first.session_revision,
                "again",
            ),
            ports(&model),
        )
        .await
        .unwrap();
    assert_eq!(second.state, RunState::Completed);
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn explicit_retry_records_a_terminal_source_and_conflicts_on_changed_lineage() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = service(Arc::clone(&repository));
    let model = AnswerModel::default();
    let source = service
        .run_turn(
            request(CommandId::new(), session_id, 0, "read this"),
            ports(&model),
        )
        .await
        .unwrap();

    let retry_command = CommandId::new();
    let mut retry = request(
        retry_command,
        session_id,
        source.session_revision,
        "read this",
    );
    retry.retry_of = Some(source.run_id);
    let retried = service
        .run_turn(retry.clone(), ports(&model))
        .await
        .unwrap();
    assert_eq!(retried.retry_of, Some(source.run_id));
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 2);

    retry.retry_of = None;
    assert_eq!(
        service.run_turn(retry, ports(&model)).await,
        Err(AgentFailure::Conflict)
    );

    let mut unknown = request(
        CommandId::new(),
        session_id,
        retried.session_revision,
        "read this",
    );
    unknown.retry_of = Some(RunId::new());
    assert_eq!(
        service.run_turn(unknown, ports(&model)).await,
        Err(AgentFailure::NotFound)
    );
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn one_session_rejects_a_second_root_while_first_model_is_waiting() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = Arc::new(service(Arc::clone(&repository)));
    let model = Arc::new(BlockingModel {
        calls: Default::default(),
        entered: Semaphore::new(0),
        release: Semaphore::new(0),
    });
    let running_service = Arc::clone(&service);
    let running_model = Arc::clone(&model);
    let running = tokio::spawn(async move {
        running_service
            .run_turn(
                request(CommandId::new(), session_id, 0, "first"),
                ports(running_model.as_ref()),
            )
            .await
    });
    model.entered.acquire().await.unwrap().forget();

    let second = service
        .run_turn(
            request(CommandId::new(), session_id, 1, "second"),
            ports(model.as_ref()),
        )
        .await;
    assert_eq!(second, Err(AgentFailure::Conflict));
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    model.release.add_permits(1);
    assert_eq!(running.await.unwrap().unwrap().state, RunState::Completed);
}

#[tokio::test]
async fn admission_observer_receives_durable_receipt_before_model_completion() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = Arc::new(service(repository));
    let model = Arc::new(BlockingModel {
        calls: Default::default(),
        entered: Semaphore::new(0),
        release: Semaphore::new(0),
    });
    let command_id = CommandId::new();
    let (admitted, admission) = tokio::sync::oneshot::channel();
    let mut admitted = Some(admitted);
    let running_service = Arc::clone(&service);
    let running_model = Arc::clone(&model);
    let running = tokio::spawn(async move {
        running_service
            .run_turn_observed(
                request(command_id, session_id, 0, "first"),
                ports(running_model.as_ref()),
                move |receipt| {
                    admitted.take().unwrap().send(receipt.clone()).unwrap();
                },
            )
            .await
    });

    let receipt = admission.await.unwrap();
    assert_eq!(receipt.command_id, command_id);
    assert_eq!(receipt.state, RunState::Working);
    model.entered.acquire().await.unwrap().forget();
    assert!(!running.is_finished());
    model.release.add_permits(1);
    assert_eq!(running.await.unwrap().unwrap().state, RunState::Completed);
}

#[tokio::test]
async fn admitted_root_is_cancelled_by_owner_identity_and_then_becomes_inactive() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let run_cancellations = Arc::new(RunCancellationRegistry::default());
    let service = Arc::new(
        ConversationService::with_run_cancellations(
            Arc::clone(&repository),
            ManagerConfig {
                role_spec: RoleSpec {
                    role_id: "manager".into(),
                    instructions: "Answer or delegate.".into(),
                    output_contract: "User-facing text.".into(),
                },
                purpose: "test-purpose".into(),
                max_iterations: 4,
                max_output_bytes: 16 * 1024,
                max_run_duration: std::time::Duration::from_secs(10),
                budget: BudgetConfig::new(16_384, 1_000_000),
            },
            Arc::clone(&run_cancellations),
        )
        .unwrap(),
    );
    let model = Arc::new(BlockingModel {
        calls: Default::default(),
        entered: Semaphore::new(0),
        release: Semaphore::new(0),
    });
    let command_id = CommandId::new();
    assert_eq!(
        run_cancellations
            .cancel_command(CancelCommandRequest {
                command_id,
                principal: "person-a".into(),
            })
            .unwrap(),
        CancelRunStatus::Unknown
    );
    let running_service = Arc::clone(&service);
    let running_model = Arc::clone(&model);
    let running = tokio::spawn(async move {
        running_service
            .run_turn(
                request(command_id, session_id, 0, "cancel after admission"),
                ports(running_model.as_ref()),
            )
            .await
    });
    model.entered.acquire().await.unwrap().forget();
    let run_id = repository
        .state
        .lock()
        .unwrap()
        .commands
        .get(&command_id)
        .copied()
        .unwrap();

    assert_eq!(
        service
            .cancel_run(CancelRunRequest {
                run_id,
                principal: "person-b".into(),
            })
            .await,
        Err(AgentFailure::CapabilityDenied)
    );
    assert_eq!(
        run_cancellations
            .cancel_command(CancelCommandRequest {
                command_id,
                principal: "person-a".into(),
            })
            .unwrap(),
        CancelRunStatus::Cancelled
    );
    let receipt = running.await.unwrap().unwrap();
    assert_eq!(receipt.state, RunState::Cancelled);
    assert_eq!(receipt.issue, Some(AgentFailure::Cancelled));
    assert_eq!(
        service
            .cancel_run(CancelRunRequest {
                run_id,
                principal: "person-a".into(),
            })
            .await
            .unwrap(),
        CancelRunStatus::Inactive
    );
}

#[tokio::test]
async fn cancelled_root_is_terminalized_without_model_dispatch_and_releases_the_claim() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = service(repository);
    let model = AnswerModel::default();
    let cancelled = request(CommandId::new(), session_id, 0, "cancel this");
    cancelled.cancellation.cancel();

    let receipt = service.run_turn(cancelled, ports(&model)).await.unwrap();
    assert_eq!(receipt.state, RunState::Cancelled);
    assert_eq!(receipt.issue, Some(AgentFailure::Cancelled));
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 0);

    let next = service
        .run_turn(
            request(
                CommandId::new(),
                session_id,
                receipt.session_revision,
                "continue",
            ),
            ports(&model),
        )
        .await
        .unwrap();
    assert_eq!(next.state, RunState::Completed);
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn deadline_continuation_is_generation_bound_and_does_not_duplicate_the_user_message() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = service(Arc::clone(&repository));
    let model = AnswerModel::default();
    let mut initial = request(CommandId::new(), session_id, 0, "finish this");
    initial.deadline = tokio::time::Instant::now() - std::time::Duration::from_secs(1);

    let timed_out = service.run_turn(initial, ports(&model)).await.unwrap();
    assert_eq!(timed_out.state, RunState::TimedOut);
    assert_eq!(timed_out.issue, Some(AgentFailure::DeadlineExceeded));
    let reference = timed_out.continuation().unwrap();

    let mut stale = request(
        CommandId::new(),
        session_id,
        timed_out.session_revision,
        "finish this",
    );
    stale.mode = crate::TurnMode::Continue(crate::ContinuationRef {
        executor_generation: reference.executor_generation + 1,
        ..reference.clone()
    });
    assert_eq!(
        service.run_turn(stale, ports(&model)).await,
        Err(AgentFailure::Conflict)
    );

    let mut continuation = request(
        CommandId::new(),
        session_id,
        timed_out.session_revision,
        "finish this",
    );
    continuation.mode = crate::TurnMode::Continue(reference);
    let completed = service.run_turn(continuation, ports(&model)).await.unwrap();
    assert_eq!(completed.state, RunState::Completed);
    assert_eq!(completed.continuation_of, Some(timed_out.run_id));
    assert_eq!(completed.continuation_level, 1);
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    let state = repository.state.lock().unwrap();
    let session = state.sessions.get(&session_id).unwrap();
    assert_eq!(
        session
            .transcript
            .iter()
            .filter(|message| message.role == floe_agent_contract::MessageRole::User)
            .count(),
        1
    );
}

#[tokio::test]
async fn continuation_chain_preserves_ancestor_generation_and_cumulative_budget_state() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = service(Arc::clone(&repository));
    let model = AnswerModel::default();
    let mut first = request(CommandId::new(), session_id, 0, "finish this");
    first.deadline = tokio::time::Instant::now() - std::time::Duration::from_secs(1);
    let first = service.run_turn(first, ports(&model)).await.unwrap();

    let mut second = request(
        CommandId::new(),
        session_id,
        first.session_revision,
        "finish this",
    );
    second.mode = crate::TurnMode::Continue(first.continuation().unwrap());
    second.deadline = tokio::time::Instant::now() - std::time::Duration::from_secs(1);
    let second = service.run_turn(second, ports(&model)).await.unwrap();
    let snapshot = service
        .continuation(second.run_id, "person-a")
        .await
        .unwrap();
    assert_eq!(snapshot.reference.level, 2);
    assert_eq!(snapshot.completed_iterations, 0);
    assert_eq!(snapshot.usage, Default::default());

    repository
        .state
        .lock()
        .unwrap()
        .runs
        .get_mut(&second.run_id)
        .unwrap()
        .admitted
        .receipt
        .continuation_executor_generation = Some(first.executor_generation + 1);
    assert!(matches!(
        service.continuation(second.run_id, "person-a").await,
        Err(AgentFailure::StorageUnavailable)
    ));
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn recovery_is_revision_bound_and_never_interrupts_a_live_root() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = Arc::new(service(Arc::clone(&repository)));
    assert_eq!(
        service
            .recover_session(RecoveryRequest {
                session_id,
                expected_session_revision: 0,
                principal: "person-a".into(),
            })
            .await
            .unwrap(),
        RecoveryReceipt {
            session_id,
            session_revision: 0,
        }
    );

    let model = Arc::new(BlockingModel {
        calls: Default::default(),
        entered: Semaphore::new(0),
        release: Semaphore::new(0),
    });
    let running_service = Arc::clone(&service);
    let running_model = Arc::clone(&model);
    let running = tokio::spawn(async move {
        running_service
            .run_turn(
                request(CommandId::new(), session_id, 0, "first"),
                ports(running_model.as_ref()),
            )
            .await
    });
    model.entered.acquire().await.unwrap().forget();
    assert_eq!(
        service
            .recover_session(RecoveryRequest {
                session_id,
                expected_session_revision: 1,
                principal: "person-a".into(),
            })
            .await,
        Err(AgentFailure::Conflict)
    );
    model.release.add_permits(1);
    let completed = running.await.unwrap().unwrap();
    assert_eq!(completed.state, RunState::Completed);
    assert_eq!(
        service
            .recover_session(RecoveryRequest {
                session_id,
                expected_session_revision: completed.session_revision,
                principal: "person-a".into(),
            })
            .await
            .unwrap()
            .session_revision,
        completed.session_revision
    );
}

#[tokio::test]
async fn t29_finalization_is_bounded_and_accounted() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = finalization_service(Arc::clone(&repository), 1);
    let model = FinalizationModel::default();
    let tools = CountingTool {
        calls: Default::default(),
        failure: None,
    };
    let delegation = CountingDelegation::default();
    let mut turn = request(CommandId::new(), session_id, 0, "look this up");
    turn.allowed_catalog = AllowedCatalog {
        cards: vec![],
        tools: vec![ToolDescriptor {
            id: "lookup".into(),
            definition_revision: 1,
            description: "Read a stable value.".into(),
            input_schema: "{\"type\":\"object\"}".into(),
            output_data_class: "public".into(),
        }],
        revision: 1,
    };

    let receipt = service
        .run_turn(
            turn,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &model,
                tools: &tools,
                delegation: &delegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::Stalled));
    assert_eq!(
        receipt.output.as_deref(),
        Some("The lookup succeeded, but the full request did not complete.")
    );
    assert_eq!(receipt.coverage, DependencyCoverage::Independent);
    assert!(receipt.continuation().is_none());
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(tools.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        delegation.calls.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    let budget = model
        .finalization_scope
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .budget()
        .snapshot();
    // The Engine never settles model usage into the ledger; InferenceService
    // owns settlement, so this non-settling fake leaves the ledger at zero
    // while the journal below still records every attempt's reported usage.
    assert_eq!(budget.settled.attempts, 0);
    assert_eq!(budget.settled.tokens, 0);
    assert_eq!(budget.settled.cost_micros, 0);
    assert_eq!(budget.unknown_tokens, 0);
    assert_eq!(budget.reserved_tokens, 0);

    let events = repository.journal.events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, JournalEvent::ModelIntent { .. }))
            .count(),
        2
    );
    let usage = events
        .iter()
        .filter_map(|event| match event {
            JournalEvent::ModelResult { usage, .. } => Some(*usage),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(usage.len(), 2);
    assert_eq!(usage.iter().map(|usage| usage.tokens).sum::<u64>(), 18);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, JournalEvent::Output { .. }))
            .count(),
        1
    );
}

struct SettlingFinalizationModel {
    calls: std::sync::atomic::AtomicUsize,
    scopes: Mutex<Vec<ExecutionScope>>,
}

impl ModelPort for SettlingFinalizationModel {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelCallOutcome, AgentFailure>> {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            // Mirror the settling owner's discipline: one budget attempt per
            // model call, dispatched, then settled for its actual usage.
            let mut tokens = scope.budget().max_tokens().min(4_096);
            let mut cost = scope.budget().max_cost_micros().min(1_000_000);
            let mut attempt = scope.budget().begin(&mut tokens, &mut cost).unwrap();
            attempt.mark_dispatched();
            let tool_batch = |tokens: u64, cost: u64| ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![ModelStep::CallTool {
                    tool_id: "lookup".into(),
                    definition_revision: 1,
                    input: "{}".into(),
                }],
                usage: ModelUsage {
                    tokens,
                    cost_micros: cost,
                },
            };
            if call == 0 {
                // First work attempt: full per-attempt allowance.
                assert_eq!(tokens, 4_096);
                attempt.settle(4_096, 10).unwrap();
                Ok(ModelCallOutcome::Ready(tool_batch(4_096, 10)))
            } else if call == 1 {
                // Second work attempt: the allowance clamps to the remaining
                // work partition (8_192 - 1_024 - 4_096), proving work cannot
                // consume the reserve.
                assert_eq!(tokens, 3_072);
                attempt.settle(3_072, 10).unwrap();
                Ok(ModelCallOutcome::Ready(tool_batch(3_072, 10)))
            } else {
                assert_eq!(call, 2);
                assert_eq!(scope.budget().max_tokens(), 1_024);
                assert_eq!(tokens, 1_024);
                self.scopes.lock().unwrap().push(scope.clone());
                attempt.settle(100, 3).unwrap();
                Ok(ModelCallOutcome::Ready(ModelResponse {
                    attempt_id: request.attempt_id,
                    steps: vec![ModelStep::Answer {
                        text: "The lookup succeeded, but the full request did not complete.".into(),
                        artifacts: vec![],
                    }],
                    usage: ModelUsage {
                        tokens: 100,
                        cost_micros: 3,
                    },
                }))
            }
        })
    }
}

#[tokio::test]
async fn finalization_reserve_is_settled_once_not_double_charged() {
    // 2-B.5 D5: with a settling model, the work attempt settles once from
    // the work partition, the finalization attempt settles once from the
    // reserve, and the ledger total equals the journaled total exactly.
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = finalization_service(Arc::clone(&repository), 2);
    let model = SettlingFinalizationModel {
        calls: std::sync::atomic::AtomicUsize::new(0),
        scopes: Mutex::new(Vec::new()),
    };
    let tools = CountingTool {
        calls: Default::default(),
        failure: None,
    };
    let delegation = CountingDelegation::default();
    let mut turn = request(CommandId::new(), session_id, 0, "look this up");
    turn.allowed_catalog = AllowedCatalog {
        cards: vec![],
        tools: vec![ToolDescriptor {
            id: "lookup".into(),
            definition_revision: 1,
            description: "Read a stable value.".into(),
            input_schema: "{\"type\":\"object\"}".into(),
            output_data_class: "public".into(),
        }],
        revision: 1,
    };

    let receipt = service
        .run_turn(
            turn,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &model,
                tools: &tools,
                delegation: &delegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::Stalled));
    assert!(receipt.output.is_some());
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert_eq!(tools.calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    // The journal records each attempt's usage exactly once.
    let events = repository.journal.events.lock().unwrap();
    let usage = events
        .iter()
        .filter_map(|event| match event {
            JournalEvent::ModelResult { usage, .. } => Some(*usage),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(usage.len(), 3);
    let journaled_tokens = usage.iter().map(|usage| usage.tokens).sum::<u64>();
    let journaled_cost = usage.iter().map(|usage| usage.cost_micros).sum::<u64>();
    assert_eq!(journaled_tokens, 4_096 + 3_072 + 100);
    assert_eq!(journaled_cost, 10 + 10 + 3);
    drop(events);
    // The ledger settled the same total exactly once: work (4_096/10 and
    // 3_072/10) plus one finalization attempt (100/3). A second
    // finalization settlement, or work dipping into the reserve, changes
    // these totals.
    let scopes = model.scopes.lock().unwrap();
    assert_eq!(scopes.len(), 1);
    let snapshot = scopes[0].budget().snapshot();
    assert_eq!(snapshot.settled.attempts, 3);
    assert_eq!(snapshot.settled.tokens, journaled_tokens);
    assert_eq!(snapshot.settled.cost_micros, journaled_cost);
    assert_eq!(snapshot.unknown_tokens, 0);
    assert_eq!(snapshot.unknown_cost_micros, 0);
    assert_eq!(snapshot.reserved_tokens, 0);
}

#[tokio::test]
async fn consent_exhaustion_does_not_start_finalization() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = finalization_service(repository, 1);
    let model = FinalizationModel::default();
    let tools = CountingTool {
        calls: Default::default(),
        failure: Some(AgentFailure::ConsentRequired),
    };
    let mut turn = request(CommandId::new(), session_id, 0, "look this up");
    turn.allowed_catalog.tools.push(ToolDescriptor {
        id: "lookup".into(),
        definition_revision: 1,
        description: "Read a stable value.".into(),
        input_schema: "{\"type\":\"object\"}".into(),
        output_data_class: "personal".into(),
    });
    turn.allowed_catalog.revision = 1;

    let receipt = service
        .run_turn(
            turn,
            ConversationPorts {
                projection: &PROJECTOR,
                model: &model,
                tools: &tools,
                delegation: &NoDelegation,
                validator: &Validator,
            },
        )
        .await
        .unwrap();

    assert_eq!(receipt.state, RunState::Failed);
    assert_eq!(receipt.issue, Some(AgentFailure::ConsentRequired));
    assert!(receipt.output.is_none());
    assert_eq!(model.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(tools.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn auto_admission_records_profile_intent_without_route() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = service(Arc::clone(&repository));
    let model = AnswerModel::default();
    let receipt = service
        .run_turn(
            request(CommandId::new(), session_id, 0, "hello"),
            ports(&model),
        )
        .await
        .unwrap();
    assert_eq!(receipt.state, RunState::Completed);
    assert_eq!(receipt.profile, crate::ProfileSelection::Auto);
    let debug = format!("{receipt:?}");
    assert!(!debug.contains("device_local"));
    assert!(!debug.contains("remote"));
    assert!(!debug.contains("endpoint"));
    assert!(!debug.contains("recipient"));
}

#[tokio::test]
async fn explicit_profile_persisted_and_same_command_different_profile_conflicts() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = service(Arc::clone(&repository));
    let model = AnswerModel::default();
    let command_id = CommandId::new();
    let mut first = request(command_id, session_id, 0, "hello");
    first.profile = crate::ProfileSelection::Explicit("local-fast".into());
    let receipt = service.run_turn(first, ports(&model)).await.unwrap();
    assert_eq!(
        receipt.profile,
        crate::ProfileSelection::Explicit("local-fast".into())
    );

    let mut conflicting = request(command_id, session_id, 0, "hello");
    conflicting.profile = crate::ProfileSelection::Auto;
    assert_eq!(
        service.run_turn(conflicting, ports(&model)).await,
        Err(AgentFailure::Conflict)
    );

    let mut same = request(command_id, session_id, 0, "hello");
    same.profile = crate::ProfileSelection::Explicit("local-fast".into());
    let replayed = service.run_turn(same, ports(&model)).await.unwrap();
    assert_eq!(replayed, receipt);
}

#[tokio::test]
async fn continuation_profile_mismatch_fails_closed() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = service(Arc::clone(&repository));
    let model = AnswerModel::default();
    let mut initial = request(CommandId::new(), session_id, 0, "finish this");
    initial.deadline = tokio::time::Instant::now() - std::time::Duration::from_secs(1);
    let timed_out = service.run_turn(initial, ports(&model)).await.unwrap();
    assert_eq!(timed_out.state, RunState::TimedOut);
    assert_eq!(timed_out.profile, crate::ProfileSelection::Auto);
    let reference = timed_out.continuation().unwrap();

    let mut mismatched = request(
        CommandId::new(),
        session_id,
        timed_out.session_revision,
        "finish this",
    );
    mismatched.profile = crate::ProfileSelection::Explicit("local-fast".into());
    mismatched.mode = crate::TurnMode::Continue(reference);
    assert_eq!(
        service.run_turn(mismatched, ports(&model)).await,
        Err(AgentFailure::StorageUnavailable)
    );
}

fn history_dependency() -> floe_agent_contract::ContextDependency {
    use floe_context_contract::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
        GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    let person = floe_kernel::PersonId::new();
    let source = GrantSourceBinding::try_new(
        person,
        ConnectionId::try_new("connection").unwrap(),
        ConnectorId::try_new("connector").unwrap(),
        ExecutionOwnerId::try_new("owner").unwrap(),
        SourceAuthority::new(),
    )
    .unwrap();
    let now = chrono::Utc::now();
    floe_agent_contract::ContextDependency::try_new(
        person,
        GrantId::new(),
        GrantAuthority::new(),
        source,
        vec![ResourceHandle::try_new("resource").unwrap()],
        vec![GrantDataCategory::Metadata],
        GrantOperation::Read,
        GrantPurpose::Assistant,
        GrantConsumer::builtin("assistant").unwrap(),
        ProcessingRestriction::LocalOnly,
        ConsumerPolicyAuthority::new(),
        Uuid::new_v4(),
        b"fingerprint".to_vec(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        now - chrono::Duration::minutes(1),
        now + chrono::Duration::minutes(5),
    )
    .unwrap()
}

/// Committed history coverage plus a revocation switch, shared with the
/// filtering projector below.
#[derive(Clone)]
struct HistoryEvidence {
    coverage: Arc<Mutex<HashMap<Uuid, DependencyCoverage>>>,
    revoked: Arc<std::sync::atomic::AtomicBool>,
    shown: Arc<Mutex<Vec<Vec<String>>>>,
}

impl floe_context::EvidenceReader for HistoryEvidence {
    fn read_turn_coverage(
        &self,
        _session_id: Uuid,
        turn_id: Uuid,
    ) -> impl std::future::Future<Output = Result<DependencyCoverage, AgentFailure>> + Send {
        let coverage = self
            .coverage
            .lock()
            .unwrap()
            .get(&turn_id)
            .cloned()
            .unwrap_or(DependencyCoverage::Unknown);
        async move { Ok(coverage) }
    }
}

impl floe_context::DependencyResolver for HistoryEvidence {
    fn authorize<'a>(
        &'a self,
        _dependency: &'a floe_agent_contract::ContextDependency,
        _request: &'a floe_context::DependencyAuthorization,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>>
    {
        let revoked = self.revoked.load(std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            if revoked {
                Err(AgentFailure::PolicyDenied)
            } else {
                Ok(())
            }
        })
    }
}

/// A projector that filters typed history through Context decisions and folds
/// the reauthorized history dependencies into its projection coverage: the
/// shape the canonical projector takes in 2-B.3.
struct FilteringProjector {
    evidence: HistoryEvidence,
    session_id: Uuid,
}

impl ModelProjectionPort for FilteringProjector {
    fn project<'a>(
        &'a self,
        request: ModelProjectionRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<AuthorizedModelProjection, AgentFailure>> {
        request.validate().unwrap();
        let evidence = self.evidence.clone();
        let session_id = self.session_id;
        let authorization = floe_context::DependencyAuthorization {
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
        };
        Box::pin(async move {
            let projected = crate::project_model_conversation_history(
                &evidence,
                session_id,
                &request.conversation,
                Some(&evidence),
                &authorization,
            )
            .await?;
            let mut coverage = DependencyCoverage::Independent;
            for dependency in &projected.authorized_history_dependencies {
                coverage = coverage
                    .merge(
                        &DependencyCoverage::dependent(dependency.clone())
                            .map_err(|_| AgentFailure::InvalidModelOutput)?,
                    )
                    .map_err(|_| AgentFailure::InvalidModelOutput)?;
            }
            let shown = projected
                .conversation
                .history
                .iter()
                .map(|entry| match entry {
                    ModelConversationEntry::User { text, .. } => format!("user:{text}"),
                    ModelConversationEntry::Preamble { text, .. } => {
                        format!("preamble:{text}")
                    }
                    ModelConversationEntry::Assistant { text, .. } => {
                        format!("assistant:{text}")
                    }
                    ModelConversationEntry::ToolExchange { result, .. } => {
                        format!("tool:{}", result.text)
                    }
                    ModelConversationEntry::DelegationExchange { receipt, .. } => {
                        format!("delegation:{}", receipt.task_id.as_uuid())
                    }
                })
                .collect::<Vec<_>>();
            evidence.shown.lock().unwrap().push(shown);
            Ok(authorized_test_projection(
                &request,
                projected.conversation,
                coverage,
            ))
        })
    }
}

struct DependentTool {
    coverage: DependencyCoverage,
}

impl ToolPort for DependentTool {
    fn invoke<'a>(
        &'a self,
        call: ToolCall,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolResult, AgentFailure>> {
        let coverage = self.coverage.clone();
        Box::pin(async move {
            Ok(ToolResult {
                call_id: call.call_id,
                text: "source observation".into(),
                artifacts: vec![],
                coverage,
                issue: None,
            })
        })
    }
}

struct ToolThenAnswer;

impl ModelPort for ToolThenAnswer {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelCallOutcome, AgentFailure>> {
        Box::pin(async move {
            let settled = request
                .projection
                .envelope
                .conversation
                .current_turn
                .iter()
                .any(|entry| matches!(entry, ModelConversationEntry::ToolExchange { .. }));
            let steps = if settled {
                vec![ModelStep::Answer {
                    text: "answered from the source".into(),
                    artifacts: vec![],
                }]
            } else {
                vec![ModelStep::CallTool {
                    tool_id: "lookup".into(),
                    definition_revision: 1,
                    input: "{}".into(),
                }]
            };
            Ok(ModelCallOutcome::Ready(ModelResponse {
                attempt_id: request.attempt_id,
                steps,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            }))
        })
    }
}

fn lookup_catalog() -> AllowedCatalog {
    AllowedCatalog {
        cards: vec![],
        tools: vec![ToolDescriptor {
            id: "lookup".into(),
            definition_revision: 1,
            description: "lookup".into(),
            input_schema: "{}".into(),
            output_data_class: "derived".into(),
        }],
        revision: 1,
    }
}

fn coverage_has(
    coverage: &DependencyCoverage,
    dependency: &floe_agent_contract::ContextDependency,
) -> bool {
    match coverage {
        DependencyCoverage::Dependent { dependencies } => dependencies.contains(dependency),
        DependencyCoverage::Independent | DependencyCoverage::Unknown => false,
    }
}

#[tokio::test]
async fn retained_history_answer_commits_its_dependency_until_revocation() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = service(Arc::clone(&repository));
    let held = history_dependency();
    let evidence = HistoryEvidence {
        coverage: Arc::new(Mutex::new(HashMap::new())),
        revoked: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        shown: Arc::new(Mutex::new(Vec::new())),
    };
    let projector = FilteringProjector {
        evidence: evidence.clone(),
        session_id,
    };
    let validator = Validator;
    let tool_model = ToolThenAnswer;
    let answer_model = AnswerModel::default();
    let tools = DependentTool {
        coverage: DependencyCoverage::dependent(held.clone()).unwrap(),
    };
    let no_delegation = NoDelegation;

    // Turn 1 reads the source and answers: the terminal coverage commits it.
    let mut first = request(CommandId::new(), session_id, 0, "what does the source say?");
    first.allowed_catalog = lookup_catalog();
    let first_receipt = service
        .run_turn(
            first,
            ConversationPorts {
                projection: &projector,
                model: &tool_model,
                tools: &tools,
                delegation: &no_delegation,
                validator: &validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(first_receipt.state, RunState::Completed);
    let stored = repository
        .load_run(first_receipt.run_id)
        .await
        .unwrap()
        .unwrap();
    let committed = stored.receipt.coverage.clone();
    assert!(coverage_has(&committed, &held));
    // The committed transcript coverage becomes the evidence the next turn
    // reauthorizes against.
    for message in &stored.transcript {
        evidence
            .coverage
            .lock()
            .unwrap()
            .insert(message.message_id, message.coverage.clone());
    }

    // Turn 2 runs no new tool call: the answer still commits the retained
    // history dependency, and the model saw the derived history text.
    evidence.shown.lock().unwrap().clear();
    let second = request(
        CommandId::new(),
        session_id,
        first_receipt.session_revision,
        "and then?",
    );
    let second_receipt = service
        .run_turn(
            second,
            ConversationPorts {
                projection: &projector,
                model: &answer_model,
                tools: &tools,
                delegation: &no_delegation,
                validator: &validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(second_receipt.state, RunState::Completed);
    let stored = repository
        .load_run(second_receipt.run_id)
        .await
        .unwrap()
        .unwrap();
    assert!(coverage_has(&stored.receipt.coverage, &held));
    let shown = evidence.shown.lock().unwrap();
    let last = shown.last().unwrap();
    assert!(last.iter().any(|entry| entry.starts_with("user:")));
    assert!(
        last.iter()
            .any(|entry| entry == "assistant:answered from the source"),
        "retained derived history must reach the model: {last:?}"
    );
    drop(shown);

    // Revocation drops the derived history while the Person's own words stay,
    // and the next answer no longer commits the revoked dependency.
    evidence
        .revoked
        .store(true, std::sync::atomic::Ordering::SeqCst);
    evidence.shown.lock().unwrap().clear();
    for message in &stored.transcript {
        evidence
            .coverage
            .lock()
            .unwrap()
            .insert(message.message_id, message.coverage.clone());
    }
    let third = request(
        CommandId::new(),
        session_id,
        second_receipt.session_revision,
        "once more?",
    );
    let third_receipt = service
        .run_turn(
            third,
            ConversationPorts {
                projection: &projector,
                model: &answer_model,
                tools: &tools,
                delegation: &no_delegation,
                validator: &validator,
            },
        )
        .await
        .unwrap();
    assert_eq!(third_receipt.state, RunState::Completed);
    let stored = repository
        .load_run(third_receipt.run_id)
        .await
        .unwrap()
        .unwrap();
    assert!(!coverage_has(&stored.receipt.coverage, &held));
    assert_eq!(stored.receipt.coverage, DependencyCoverage::Independent);
    let shown = evidence.shown.lock().unwrap();
    let last = shown.last().unwrap();
    assert!(
        last.iter().all(|entry| entry.starts_with("user:")),
        "revoked derived history must not reach the model: {last:?}"
    );
    assert!(!last.is_empty());
}
