use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use floe_agent_contract::{
    AgentMessage, AllowedCatalog, BoundedContext, BoxFuture, DelegationPort, DelegationRequest,
    DependencyCoverage, ExecutionJournal, JournalAck, JournalEvent, ModelPort, ModelRequest,
    ModelResponse, ModelStep, ModelUsage, RoleSpec, TaskReceipt, ToolCall, ToolDescriptor,
    ToolPort, ToolResult,
};
use floe_agent_runtime::FinalPayloadValidator;
use floe_execution::{ExecutionScope, budget::BudgetConfig};
use floe_kernel::{AgentFailure, CommandId, RunId};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    AdmittedTurn, CancelCommandRequest, CancelRunRequest, CancelRunStatus, ConversationPorts,
    ConversationRepository, JournalEntry, ManagerConfig, RecoveryReceipt, RecoveryRequest,
    RunCancellationRegistry, RunReceipt, RunState, RunTerminal, TurnAdmission,
    TurnAdmissionRequest, TurnRequest,
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
    runs: HashMap<RunId, StoredRun>,
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
        command_id: CommandId,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
        Box::pin(async move {
            let state = self.state.lock().unwrap();
            Ok(state
                .commands
                .get(&command_id)
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
                        || source.admitted.receipt.execution_profile != request.execution_profile
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
                execution_profile: request.execution_profile,
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

#[derive(Default)]
struct AnswerModel {
    calls: std::sync::atomic::AtomicUsize,
}

impl ModelPort for AnswerModel {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            Ok(ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![ModelStep::Answer {
                    text: format!("answered: {}", request.prompt),
                    artifacts: vec![],
                }],
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            })
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
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        let call = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            if call == 0 {
                assert_eq!(request.catalog.tools.len(), 1);
                assert_eq!(scope.budget().max_tokens(), 4_096);
                Ok(ModelResponse {
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
                })
            } else {
                assert_eq!(call, 1);
                *self.finalization_scope.lock().unwrap() = Some(scope.clone());
                assert!(request.catalog.tools.is_empty());
                assert!(request.catalog.cards.is_empty());
                assert_eq!(request.role.prompt, crate::FINALIZATION_ROLE_PROMPT);
                assert_eq!(
                    request.role.output_contract,
                    crate::FINALIZATION_OUTPUT_CONTRACT
                );
                assert_eq!(request.bounded_context.text, "");
                assert_eq!(
                    request.bounded_context.coverage,
                    DependencyCoverage::Independent
                );
                assert_eq!(request.replay.len(), 1);
                assert_eq!(scope.budget().max_tokens(), 1_024);
                Ok(ModelResponse {
                    attempt_id: request.attempt_id,
                    steps: vec![ModelStep::Answer {
                        text: "The lookup succeeded, but the full request did not complete.".into(),
                        artifacts: vec![],
                    }],
                    usage: ModelUsage {
                        tokens: 7,
                        cost_micros: 1,
                    },
                })
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
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            self.entered.add_permits(1);
            self.release.acquire().await.unwrap().forget();
            Ok(ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![ModelStep::Answer {
                    text: "done".into(),
                    artifacts: vec![],
                }],
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            })
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
        if role == "manager" && !text.trim().is_empty() {
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
                prompt: "Answer or delegate.".into(),
                output_contract: "User-facing text.".into(),
            },
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
) -> ConversationService<MemoryRepository> {
    ConversationService::new(
        repository,
        ManagerConfig {
            role_spec: RoleSpec {
                role_id: "manager".into(),
                prompt: "Answer or delegate.".into(),
                output_contract: "User-facing text.".into(),
            },
            max_iterations: 1,
            max_output_bytes: 16 * 1024,
            max_run_duration: std::time::Duration::from_secs(10),
            budget: BudgetConfig::new(8_192, 100).with_finalization_reserve(1_024, 10),
        },
    )
    .unwrap()
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
        prompt: prompt.into(),
        request_context_digest: [1; 32],
        mode: crate::TurnMode::New,
        execution_profile: "test-local".into(),
        bounded_context: BoundedContext {
            text: String::new(),
            coverage: DependencyCoverage::Independent,
        },
        allowed_catalog: AllowedCatalog::default(),
        replay: vec![],
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(2),
        cancellation: floe_execution::Cancellation::default(),
    }
}

fn ports(model: &dyn ModelPort) -> ConversationPorts<'_> {
    ConversationPorts {
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
    let first_request = request(command_id, session_id, 0, "hello");
    let first = service
        .run_turn(first_request.clone(), ports(&model))
        .await
        .unwrap();
    assert_eq!(first.state, RunState::Completed);
    assert_eq!(first.coverage, DependencyCoverage::Independent);
    assert_eq!(first.session_revision, 2);

    let mut replay_request = first_request;
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
                    prompt: "Answer or delegate.".into(),
                    output_contract: "User-facing text.".into(),
                },
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
    let service = finalization_service(Arc::clone(&repository));
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
    assert_eq!(budget.settled.attempts, 2);
    assert_eq!(budget.settled.tokens, 18);
    assert_eq!(budget.settled.cost_micros, 3);
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

#[tokio::test]
async fn consent_exhaustion_does_not_start_finalization() {
    let repository = Arc::new(MemoryRepository::default());
    let session_id = Uuid::new_v4();
    repository.add_session(session_id, "person-a");
    let service = finalization_service(repository);
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
