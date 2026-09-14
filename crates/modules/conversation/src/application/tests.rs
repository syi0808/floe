use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use floe_agent_contract::{
    AgentMessage, AllowedCatalog, BoundedContext, BoxFuture, DelegationPort, DelegationRequest,
    DependencyCoverage, ExecutionJournal, JournalAck, JournalEvent, ModelPort, ModelRequest,
    ModelResponse, ModelStep, ModelUsage, RoleSpec, TaskReceipt, ToolCall, ToolPort, ToolResult,
};
use floe_agent_runtime::FinalPayloadValidator;
use floe_execution::{ExecutionScope, budget::BudgetConfig};
use floe_kernel::{AgentFailure, CommandId, RunId};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    AdmittedTurn, ConversationPorts, ConversationRepository, JournalEntry, ManagerConfig,
    RecoveryReceipt, RecoveryRequest, RunReceipt, RunState, RunTerminal, TurnAdmission,
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
            session.transcript.push(request.user_message);
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
