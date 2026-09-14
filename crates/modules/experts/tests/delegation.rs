use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use floe_agent_contract::{
    A2A_PROTOCOL_VERSION, AGENT_SCHEMA_VERSION, AgentCard, AgentDefinition, AgentEndpoint,
    AgentFailure, AllowedCatalog, BoundedContext, BoxFuture, DelegationRequest, DependencyCoverage,
    EndpointInvocation, EngineRequest, ExecutionJournal, ExpertReport, JournalAck, JournalEvent,
    ModelPort, ModelRequest, ModelResponse, ModelStep, ModelUsage, RoleSpec, TaskId, TaskSnapshot,
    TaskState, ToolCall, ToolPort, ToolResult,
};
use floe_agent_runtime::Engine;
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger},
};
use floe_experts::{
    Directory, DirectoryEntry, DirectoryQuery, TaskActivation, TaskAdmission, TaskCoordinator,
    TaskRecord, TaskRepository,
};
use floe_kernel::{RunId, TraceContext};
use tokio::time::Instant;
use uuid::Uuid;

#[derive(Default)]
struct MemoryTasks(Mutex<MemoryTaskState>);

#[derive(Default)]
struct MemoryTaskState {
    executor_generation: u64,
    records: HashMap<TaskId, TaskRecord>,
}

impl TaskRepository for MemoryTasks {
    fn activate<'a>(&'a self) -> BoxFuture<'a, Result<TaskActivation, AgentFailure>> {
        Box::pin(async move {
            let mut state = self
                .0
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            state.executor_generation = state
                .executor_generation
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            let executor_generation = state.executor_generation;
            let mut interrupted = Vec::new();
            for current in state.records.values_mut() {
                if let Some(recovered) = current
                    .interrupt_orphan(executor_generation, floe_agent_contract::MAX_OUTPUT_BYTES)?
                {
                    *current = recovered;
                    interrupted.push(current.clone());
                }
            }
            Ok(TaskActivation {
                executor_generation,
                interrupted,
            })
        })
    }

    fn admit<'a>(
        &'a self,
        proposed: TaskRecord,
    ) -> BoxFuture<'a, Result<TaskAdmission, AgentFailure>> {
        Box::pin(async move {
            proposed.validate_initial(floe_agent_contract::MAX_OUTPUT_BYTES)?;
            let mut records = self
                .0
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            if let Some(record) = records.records.get(&proposed.snapshot.task_id) {
                if record.invocation_key != proposed.invocation_key
                    || record.request_digest != proposed.request_digest
                    || record.snapshot.task_id != proposed.snapshot.task_id
                    || record.snapshot.parent_run_id != proposed.snapshot.parent_run_id
                    || record.snapshot.principal != proposed.snapshot.principal
                    || record.snapshot.agent_id != proposed.snapshot.agent_id
                    || record.snapshot.definition_revision != proposed.snapshot.definition_revision
                {
                    return Err(AgentFailure::Conflict);
                }
                return Ok(TaskAdmission::Existing(record.clone()));
            }
            if proposed.executor_generation != records.executor_generation {
                return Err(AgentFailure::Conflict);
            }
            records
                .records
                .insert(proposed.snapshot.task_id, proposed.clone());
            Ok(TaskAdmission::Created(proposed))
        })
    }

    fn compare_and_swap<'a>(
        &'a self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
    ) -> BoxFuture<'a, Result<TaskRecord, AgentFailure>> {
        Box::pin(async move {
            let mut records = self
                .0
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            let current = records
                .records
                .get_mut(&task_id)
                .ok_or(AgentFailure::NotFound)?;
            *current = current.transition(
                expected_aggregate_revision,
                executor_generation,
                snapshot,
                floe_agent_contract::MAX_OUTPUT_BYTES,
            )?;
            Ok(current.clone())
        })
    }

    fn get<'a>(
        &'a self,
        task_id: TaskId,
    ) -> BoxFuture<'a, Result<Option<TaskRecord>, AgentFailure>> {
        Box::pin(async move {
            Ok(self
                .0
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .records
                .get(&task_id)
                .cloned())
        })
    }
}

struct Endpoint {
    result: Result<&'static str, AgentFailure>,
    calls: Arc<AtomicUsize>,
}

impl AgentEndpoint for Endpoint {
    fn execute<'a>(
        &'a self,
        invocation: EndpointInvocation,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let result = self.result;
        Box::pin(async move {
            Ok(ExpertReport {
                task_id: invocation.request.task_id,
                principal: invocation.request.principal,
                agent_id: invocation.request.selected_agent_id,
                definition_revision: invocation.request.selected_definition_revision,
                result: result?.into(),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
            })
        })
    }
}

fn definition(id: &str, revision: u64) -> AgentDefinition {
    AgentDefinition {
        card: AgentCard {
            schema_version: AGENT_SCHEMA_VERSION,
            protocol_version: A2A_PROTOCOL_VERSION.into(),
            id: id.into(),
            version: "1.0.0".into(),
            name: id.into(),
            description: format!("{id} expert"),
            domain_tags: vec![],
            skills: vec!["analyze an admitted request".into()],
        },
        definition_revision: revision,
    }
}

fn register(directory: &Directory, id: &str, endpoint: Endpoint) -> Result<(), AgentFailure> {
    directory.register(
        DirectoryEntry {
            definition: definition(id, 1),
            reviewed: true,
            enabled: true,
            admitted_principals: vec!["person-a".into()],
            purposes: vec!["everyday-assistance".into()],
        },
        Arc::new(endpoint),
    )?;
    Ok(())
}

fn scope(run_id: RunId, task_id: Option<TaskId>) -> ExecutionScope {
    let budget = BudgetLedger::new(BudgetConfig::new(50_000, 100_000), Default::default());
    let root = ExecutionScope::root(
        Cancellation::new(),
        Instant::now() + Duration::from_secs(5),
        budget.work_lease(),
        TraceContext::new(Uuid::new_v4()).with_run_id(run_id),
    );
    task_id.map_or(root.clone(), |task_id| {
        root.child_scope(root.deadline(), 20_000, 50_000, Some(task_id))
    })
}

fn delegation(run_id: RunId, task_id: TaskId, agent_id: &str) -> DelegationRequest {
    DelegationRequest {
        task_id,
        parent_run_id: Some(run_id.as_uuid()),
        principal: "person-a".into(),
        invocation_key: floe_agent_contract::InvocationKey::new(),
        selected_agent_id: agent_id.into(),
        selected_definition_revision: 1,
        message: "Summarize the relevant context".into(),
        context_refs: vec![],
    }
}

#[tokio::test]
async fn registered_ninth_endpoint_executes_without_dispatch_changes_and_replays_task() {
    let directory = Directory::default();
    let calls = Arc::new(AtomicUsize::new(0));
    register(
        &directory,
        "floe.test.ninth",
        Endpoint {
            result: Ok("ninth result"),
            calls: Arc::clone(&calls),
        },
    )
    .unwrap();
    let repository = Arc::new(MemoryTasks::default());
    let coordinator = TaskCoordinator::activate(
        directory.clone(),
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let coordinator = coordinator.0;
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let request = delegation(run_id, task_id, "floe.test.ninth");
    let task_scope = scope(run_id, Some(task_id));

    let first =
        floe_agent_contract::DelegationPort::delegate(&coordinator, request.clone(), &task_scope)
            .await
            .unwrap();
    let engine_run_id = RunId::new();
    let engine_scope = scope(engine_run_id, None);
    let catalog = directory
        .list_cards(DirectoryQuery {
            principal: "person-a",
            purpose: "everyday-assistance",
        })
        .unwrap();
    let engine_report = Engine::default()
        .drive_with_default_validator(
            manager_request(catalog, engine_scope),
            &ManagerModel {
                selected_agent_id: "floe.test.ninth".into(),
                observed_coverage: Arc::new(Mutex::new(vec![])),
            },
            &NoTools,
            &coordinator,
            &Journal,
        )
        .await
        .unwrap();
    directory.set_enabled("floe.test.ninth", 1, false).unwrap();
    let second = floe_agent_contract::DelegationPort::delegate(&coordinator, request, &task_scope)
        .await
        .unwrap();

    assert_eq!(first.snapshot.state, TaskState::Completed);
    assert_eq!(first.snapshot.result.as_deref(), Some("ninth result"));
    assert_eq!(engine_report.output.as_deref(), Some("manager continued"));
    assert_eq!(second.snapshot, first.snapshot);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(matches!(
        coordinator
            .get_task(
                "person-b",
                Some(run_id.as_uuid()),
                task_id,
                &scope(run_id, None),
            )
            .await,
        Err(AgentFailure::CapabilityDenied)
    ));
    assert_eq!(
        coordinator
            .get_task(
                "person-a",
                Some(run_id.as_uuid()),
                task_id,
                &scope(run_id, None),
            )
            .await
            .unwrap()
            .unwrap()
            .snapshot,
        first.snapshot
    );
}

struct BlockingEndpoint {
    started: Arc<tokio::sync::Notify>,
}

impl AgentEndpoint for BlockingEndpoint {
    fn execute<'a>(
        &'a self,
        _: EndpointInvocation,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>> {
        let started = Arc::clone(&self.started);
        Box::pin(async move {
            started.notify_one();
            std::future::pending().await
        })
    }
}

#[tokio::test]
async fn explicit_task_cancel_is_authorized_persisted_and_does_not_cancel_parent() {
    let directory = Directory::default();
    let started = Arc::new(tokio::sync::Notify::new());
    directory
        .register(
            DirectoryEntry {
                definition: definition("floe.test.blocking", 1),
                reviewed: true,
                enabled: true,
                admitted_principals: vec!["person-a".into()],
                purposes: vec!["everyday-assistance".into()],
            },
            Arc::new(BlockingEndpoint {
                started: Arc::clone(&started),
            }),
        )
        .unwrap();
    let (coordinator, recovered) = TaskCoordinator::activate(
        directory,
        Arc::new(MemoryTasks::default()),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    assert!(recovered.is_empty());
    let coordinator = Arc::new(coordinator);
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let request = delegation(run_id, task_id, "floe.test.blocking");
    let task_scope = scope(run_id, Some(task_id));
    let parent_scope = scope(run_id, None);
    let task_coordinator = Arc::clone(&coordinator);
    let delegated = tokio::spawn(async move {
        floe_agent_contract::DelegationPort::delegate(
            task_coordinator.as_ref(),
            request,
            &task_scope,
        )
        .await
    });
    started.notified().await;

    let cancelled = coordinator
        .cancel_task("person-a", Some(run_id.as_uuid()), task_id, &parent_scope)
        .await
        .unwrap();
    let settled = delegated.await.unwrap().unwrap();

    assert_eq!(cancelled.snapshot.state, TaskState::Cancelled);
    assert_eq!(settled.snapshot.state, TaskState::Cancelled);
    assert!(!parent_scope.cancellation().is_cancelled());
}

#[tokio::test]
async fn restart_recovery_interrupts_only_an_orphaned_nonterminal_task() {
    let repository = Arc::new(MemoryTasks::default());
    repository.0.lock().unwrap().executor_generation = 7;
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let request = delegation(run_id, task_id, "floe.test.recovery");
    let request_digest =
        floe_agent_contract::input_digest(&serde_json::to_string(&request).unwrap());
    let submitted = TaskRecord {
        snapshot: TaskSnapshot {
            task_id,
            parent_run_id: request.parent_run_id,
            principal: request.principal.clone(),
            agent_id: request.selected_agent_id.clone(),
            definition_revision: request.selected_definition_revision,
            state: TaskState::Submitted,
            result: None,
            artifacts: vec![],
            coverage: DependencyCoverage::Unknown,
            issue: None,
        },
        invocation_key: request.invocation_key,
        request_digest,
        aggregate_revision: 1,
        executor_generation: 7,
    };
    assert!(matches!(
        repository.admit(submitted.clone()).await.unwrap(),
        TaskAdmission::Created(_)
    ));
    let working = repository
        .compare_and_swap(
            task_id,
            1,
            7,
            TaskSnapshot {
                state: TaskState::Working,
                ..submitted.snapshot
            },
        )
        .await
        .unwrap();
    assert_eq!(working.snapshot.state, TaskState::Working);
    let (_coordinator, recovered) = TaskCoordinator::activate(
        Directory::default(),
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let persisted = repository.get(task_id).await.unwrap().unwrap();

    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].snapshot.state, TaskState::Interrupted);
    assert_eq!(persisted.aggregate_revision, 3);
    assert_eq!(persisted.executor_generation, 8);
    let (_, recovered_again) = TaskCoordinator::activate(
        Directory::default(),
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    assert!(recovered_again.is_empty());
}

#[tokio::test]
async fn disabled_selection_is_persisted_as_rejected_without_endpoint_execution() {
    let directory = Directory::default();
    let calls = Arc::new(AtomicUsize::new(0));
    register(
        &directory,
        "floe.test.disabled",
        Endpoint {
            result: Ok("must not run"),
            calls: Arc::clone(&calls),
        },
    )
    .unwrap();
    directory
        .set_enabled("floe.test.disabled", 1, false)
        .unwrap();
    let coordinator = TaskCoordinator::activate(
        directory,
        Arc::new(MemoryTasks::default()),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let coordinator = coordinator.0;
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let receipt = floe_agent_contract::DelegationPort::delegate(
        &coordinator,
        delegation(run_id, task_id, "floe.test.disabled"),
        &scope(run_id, Some(task_id)),
    )
    .await
    .unwrap();

    assert_eq!(receipt.snapshot.state, TaskState::Rejected);
    assert_eq!(receipt.snapshot.issue, Some(AgentFailure::CapabilityDenied));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

struct ManagerModel {
    selected_agent_id: String,
    observed_coverage: Arc<Mutex<Vec<DependencyCoverage>>>,
}

impl ModelPort for ManagerModel {
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        let observed_coverage = Arc::clone(&self.observed_coverage);
        let selected_agent_id = self.selected_agent_id.clone();
        Box::pin(async move {
            observed_coverage.lock().unwrap().extend(
                request
                    .messages
                    .iter()
                    .filter(|message| message.role == floe_agent_contract::MessageRole::Tool)
                    .map(|message| message.coverage.clone()),
            );
            let step = if request
                .messages
                .iter()
                .any(|message| message.role == floe_agent_contract::MessageRole::Tool)
            {
                ModelStep::Answer {
                    text: "manager continued".into(),
                    artifacts: vec![],
                }
            } else {
                ModelStep::Delegate {
                    agent_id: selected_agent_id,
                    definition_revision: 1,
                    message: "Check communication".into(),
                    context_refs: vec![],
                }
            };
            Ok(ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![step],
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
        Box::pin(async { Err(AgentFailure::CapabilityDenied) })
    }
}

struct Journal;

impl ExecutionJournal for Journal {
    fn record_intent<'a>(
        &'a self,
        _: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
    }

    fn record_result<'a>(
        &'a self,
        _: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
    }

    fn record_output<'a>(
        &'a self,
        _: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
    }

    fn checkpoint<'a>(
        &'a self,
        _: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
    }
}

async fn run_manager(
    communication_result: Result<&'static str, AgentFailure>,
) -> (
    floe_agent_runtime::EngineReport,
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
    Vec<DependencyCoverage>,
) {
    let directory = Directory::default();
    let schedule_calls = Arc::new(AtomicUsize::new(0));
    let communication_calls = Arc::new(AtomicUsize::new(0));
    register(
        &directory,
        "floe.builtin.schedule",
        Endpoint {
            result: Ok("schedule"),
            calls: Arc::clone(&schedule_calls),
        },
    )
    .unwrap();
    register(
        &directory,
        "floe.builtin.communication",
        Endpoint {
            result: communication_result,
            calls: Arc::clone(&communication_calls),
        },
    )
    .unwrap();
    let catalog = directory
        .list_cards(DirectoryQuery {
            principal: "person-a",
            purpose: "everyday-assistance",
        })
        .unwrap();
    assert_eq!(catalog.cards.len(), 2);
    let coordinator = TaskCoordinator::activate(
        directory,
        Arc::new(MemoryTasks::default()),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let coordinator = coordinator.0;
    let run_id = RunId::new();
    let root_scope = scope(run_id, None);
    let observed_coverage = Arc::new(Mutex::new(vec![]));
    let report = Engine::default()
        .drive_with_default_validator(
            manager_request(catalog, root_scope),
            &ManagerModel {
                selected_agent_id: "floe.builtin.communication".into(),
                observed_coverage: Arc::clone(&observed_coverage),
            },
            &NoTools,
            &coordinator,
            &Journal,
        )
        .await
        .unwrap();

    let coverage = observed_coverage.lock().unwrap().clone();
    (report, schedule_calls, communication_calls, coverage)
}

fn manager_request(catalog: AllowedCatalog, scope: ExecutionScope) -> EngineRequest {
    EngineRequest {
        principal: "person-a".into(),
        role_spec: RoleSpec {
            role_id: "manager".into(),
            prompt: "delegate when useful".into(),
            output_contract: "plain text".into(),
        },
        prompt: "What needs attention?".into(),
        scope,
        bounded_context: BoundedContext {
            text: "".into(),
            coverage: DependencyCoverage::Independent,
        },
        messages: vec![],
        allowed_catalog: catalog,
        max_iterations: 3,
        max_output_bytes: 16 * 1024,
        replay: vec![],
    }
}

#[tokio::test]
async fn manager_selection_is_not_redirected_by_eligible_schedule_and_keeps_task_coverage() {
    let (report, schedule_calls, communication_calls, coverage) =
        run_manager(Ok("communication result")).await;

    assert_eq!(report.output.as_deref(), Some("manager continued"));
    assert_eq!(schedule_calls.load(Ordering::SeqCst), 0);
    assert_eq!(communication_calls.load(Ordering::SeqCst), 1);
    assert_eq!(coverage, vec![DependencyCoverage::Independent]);
    assert!(report.steps.iter().any(|step| matches!(
        step,
        floe_agent_contract::EngineStep::Delegation(receipt)
            if receipt.snapshot.state == TaskState::Completed
                && receipt.snapshot.coverage == DependencyCoverage::Independent
    )));
}

#[tokio::test]
async fn failed_task_is_scoped_and_manager_root_continues() {
    let (report, schedule_calls, communication_calls, coverage) =
        run_manager(Err(AgentFailure::CapabilityUnavailable)).await;

    assert_eq!(report.output.as_deref(), Some("manager continued"));
    assert_eq!(schedule_calls.load(Ordering::SeqCst), 0);
    assert_eq!(communication_calls.load(Ordering::SeqCst), 1);
    assert_eq!(coverage, vec![DependencyCoverage::Unknown]);
    assert!(report.steps.iter().any(|step| matches!(
        step,
        floe_agent_contract::EngineStep::Delegation(receipt)
            if receipt.snapshot.state == TaskState::Failed
                && receipt.snapshot.issue == Some(AgentFailure::CapabilityUnavailable)
    )));
}

#[test]
fn disabled_or_stale_endpoint_is_not_dispatchable() {
    let directory = Directory::default();
    register(
        &directory,
        "floe.builtin.schedule",
        Endpoint {
            result: Ok("schedule"),
            calls: Arc::new(AtomicUsize::new(0)),
        },
    )
    .unwrap();
    assert!(matches!(
        directory.resolve(
            "floe.builtin.schedule",
            2,
            DirectoryQuery {
                principal: "person-a",
                purpose: "everyday-assistance"
            },
        ),
        Err(AgentFailure::Conflict)
    ));
    directory
        .set_enabled("floe.builtin.schedule", 1, false)
        .unwrap();
    assert!(matches!(
        directory.resolve(
            "floe.builtin.schedule",
            1,
            DirectoryQuery {
                principal: "person-a",
                purpose: "everyday-assistance"
            },
        ),
        Err(AgentFailure::CapabilityDenied)
    ));
}
