use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use floe_agent_contract::prompts::{
    PromptAssembly, PromptComponent, PromptComponentKind, PromptRole,
};
use floe_agent_contract::{
    A2A_PROTOCOL_VERSION, AGENT_SCHEMA_VERSION, AgentCard, AgentDefinition, AgentEndpoint,
    AgentFailure, AllowedCatalog, AuthorizedModelProjection, BoxFuture, ContextEnvelope,
    ContextManifest, ContextualData, DataClass, DelegationPort, DelegationRequest,
    DependencyCoverage, EndpointInvocation, EngineRequest, ExecutionJournal, ExpertReport,
    JournalAck, JournalEvent, ModelCallOutcome, ModelConversation, ModelConversationEntry,
    ModelPlacement, ModelPort, ModelProjectionPort, ModelProjectionRequest, ModelRequest,
    ModelResponse, ModelStep, ModelUsage, ProjectionRef, RoleSpec, RuntimeContext,
    ScopedInstructions, TaskId, TaskSnapshot, TaskState, ToolCall, ToolPort, ToolResult,
};
use floe_agent_contract::{RunId, TraceContext};
use floe_agent_runtime::Engine;
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger},
};
use floe_experts::{
    Directory, DirectoryEntry, DirectoryQuery, ExpertAdmissionIdentity, TaskActivation,
    TaskAdmission, TaskCoordinator, TaskRecord, TaskRepository,
};
use tokio::time::Instant;
use uuid::Uuid;

#[derive(Default)]
struct MemoryTasks(
    Mutex<MemoryTaskState>,
    Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
);

#[derive(Default)]
struct MemoryTaskState {
    executor_generation: u64,
    records: HashMap<TaskId, TaskRecord>,
    settlements: usize,
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
                    || record.admission != proposed.admission
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
            let became_working = snapshot.state == TaskState::Working;
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
            let updated = current.clone();
            drop(records);
            if became_working {
                if let Some(on_working) = self
                    .1
                    .lock()
                    .map_err(|_| AgentFailure::StorageUnavailable)?
                    .as_ref()
                {
                    on_working();
                }
            }
            Ok(updated)
        })
    }

    fn validate_settlement(
        &self,
        settlement: &floe_agent_contract::EndpointSettlement,
    ) -> Result<(), AgentFailure> {
        (settlement.owner() == "memory" && settlement.payload() == "{}")
            .then_some(())
            .ok_or(AgentFailure::CapabilityUnavailable)
    }

    fn settle<'a>(
        &'a self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
        settlement: Option<floe_agent_contract::EndpointSettlement>,
    ) -> BoxFuture<'a, Result<TaskRecord, AgentFailure>> {
        Box::pin(async move {
            if let Some(settlement) = settlement {
                self.validate_settlement(&settlement)?;
                self.0
                    .lock()
                    .map_err(|_| AgentFailure::StorageUnavailable)?
                    .settlements += 1;
            }
            self.compare_and_swap(
                task_id,
                expected_aggregate_revision,
                executor_generation,
                snapshot,
            )
            .await
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

struct SettlementEndpoint(&'static str);

impl AgentEndpoint for SettlementEndpoint {
    fn execute<'a>(
        &'a self,
        invocation: EndpointInvocation,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>> {
        let owner = self.0;
        Box::pin(async move {
            Ok(ExpertReport {
                task_id: invocation.request.task_id,
                principal: invocation.request.principal,
                agent_id: invocation.request.selected_agent_id,
                definition_revision: invocation.request.selected_definition_revision,
                result: "settled result".into(),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                settlement: Some(
                    floe_agent_contract::EndpointSettlement::try_new(owner, "{}").unwrap(),
                ),
            })
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
                settlement: None,
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
            supported_placements: vec![ModelPlacement::DeviceLocal, ModelPlacement::Remote],
        },
        definition_revision: revision,
    }
}

fn admission(id: &str, definition_revision: u64) -> ExpertAdmissionIdentity {
    ExpertAdmissionIdentity {
        registry_instance_id: Uuid::new_v4(),
        assignment_id: Uuid::new_v4(),
        installation_id: Uuid::new_v4(),
        package: floe_agent_contract::PackageRef {
            kind: floe_agent_contract::PackageKind::Expert,
            id: id.into(),
            version: "1.0.0".into(),
        },
        definition_revision,
    }
}

fn register(directory: &Directory, id: &str, endpoint: Endpoint) -> Result<(), AgentFailure> {
    directory.register(
        DirectoryEntry {
            definition: definition(id, 1),
            admission: admission(id, 1),
            reviewed: true,
            enabled: true,
            admitted_principals: vec!["person-a".into()],
            purposes: vec!["everyday-assistance".into()],
        },
        Arc::new(endpoint),
    )?;
    Ok(())
}

fn publication_entry(id: &str) -> (DirectoryEntry, Arc<dyn AgentEndpoint>) {
    (
        DirectoryEntry {
            definition: definition(id, 1),
            admission: admission(id, 1),
            reviewed: true,
            enabled: true,
            admitted_principals: vec!["person-a".into()],
            purposes: vec!["everyday-assistance".into()],
        },
        Arc::new(Endpoint {
            result: Ok("published result"),
            calls: Arc::new(AtomicUsize::new(0)),
        }),
    )
}

#[test]
fn owner_publication_replaces_a_complete_set_without_losing_other_owners() {
    let directory = Directory::default();
    let first = publication_entry("example.test.first");
    let second = publication_entry("example.test.second");
    let unrelated = publication_entry("example.test.unrelated");
    let first_revision = directory.publish("bundle-a", vec![first]).unwrap();
    directory.publish("bundle-b", vec![unrelated]).unwrap();
    let before_conflict = directory
        .list_cards(DirectoryQuery {
            principal: "person-a",
            purpose: "everyday-assistance",
        })
        .unwrap();
    assert_eq!(
        directory.publish(
            "bundle-a",
            vec![publication_entry("example.test.unrelated")]
        ),
        Err(AgentFailure::Conflict),
    );
    assert_eq!(
        directory
            .list_cards(DirectoryQuery {
                principal: "person-a",
                purpose: "everyday-assistance",
            })
            .unwrap(),
        before_conflict
    );
    let revision = directory.publish("bundle-a", vec![second]).unwrap();
    assert_eq!(revision, first_revision + 2);
    let after = directory
        .list_cards(DirectoryQuery {
            principal: "person-a",
            purpose: "everyday-assistance",
        })
        .unwrap();
    let ids = after
        .cards
        .iter()
        .map(|definition| definition.card.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["example.test.second", "example.test.unrelated"]);
    assert_eq!(after.revision, revision);
}

#[test]
fn owner_publication_rejects_duplicate_candidates_and_rejoins_exact_set() {
    let directory = Directory::default();
    let candidate = publication_entry("example.test.stable");
    let revision = directory
        .publish("test-bundle", vec![candidate.clone()])
        .unwrap();
    assert_eq!(
        directory.publish("test-bundle", vec![candidate.clone()]),
        Ok(revision),
    );
    assert_eq!(
        directory.publish("test-bundle", vec![candidate.clone(), candidate]),
        Err(AgentFailure::Conflict),
    );
    assert_eq!(
        directory
            .list_cards(DirectoryQuery {
                principal: "person-a",
                purpose: "everyday-assistance",
            })
            .unwrap()
            .revision,
        revision,
    );
}

#[test]
fn owner_publication_rejects_duplicate_assignment_identity() {
    let directory = Directory::default();
    let first = publication_entry("example.test.first");
    let mut second = publication_entry("example.test.second");
    second.0.admission.registry_instance_id = first.0.admission.registry_instance_id;
    second.0.admission.assignment_id = first.0.admission.assignment_id;
    assert_eq!(
        directory.publish("bundle-a", vec![first.clone(), second.clone()]),
        Err(AgentFailure::Conflict),
    );
    directory.publish("bundle-a", vec![first]).unwrap();
    assert_eq!(
        directory.publish("bundle-b", vec![second]),
        Err(AgentFailure::Conflict),
    );
    assert_eq!(
        directory
            .list_cards(DirectoryQuery {
                principal: "person-a",
                purpose: "everyday-assistance",
            })
            .unwrap()
            .cards
            .len(),
        1,
    );
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
        execution_context: delegation_context(),
    }
}

#[tokio::test]
async fn admitted_task_keeps_endpoint_across_publication_refresh() {
    let directory = Directory::default();
    let old_calls = Arc::new(AtomicUsize::new(0));
    let new_calls = Arc::new(AtomicUsize::new(0));
    let (entry, _) = publication_entry("example.test.pinned");
    let admitted_identity = entry.admission.clone();
    directory
        .publish(
            "test-bundle",
            vec![(
                entry,
                Arc::new(Endpoint {
                    result: Ok("old endpoint"),
                    calls: Arc::clone(&old_calls),
                }),
            )],
        )
        .unwrap();
    let repository = Arc::new(MemoryTasks::default());
    let refresh_directory = directory.clone();
    let refresh_calls = Arc::clone(&new_calls);
    *repository.1.lock().unwrap() = Some(Box::new(move || {
        let (entry, _) = publication_entry("example.test.pinned");
        refresh_directory
            .publish(
                "test-bundle",
                vec![(
                    entry,
                    Arc::new(Endpoint {
                        result: Ok("new endpoint"),
                        calls: Arc::clone(&refresh_calls),
                    }),
                )],
            )
            .unwrap();
    }));
    let (coordinator, _) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let first = coordinator
        .delegate(
            delegation(run_id, task_id, "example.test.pinned"),
            &scope(run_id, Some(task_id)),
        )
        .await
        .unwrap();
    assert_eq!(first.snapshot.result.as_deref(), Some("old endpoint"));
    assert_eq!(repository.get(task_id).await.unwrap().unwrap().admission, admitted_identity);
    assert_eq!(old_calls.load(Ordering::SeqCst), 1);
    assert_eq!(new_calls.load(Ordering::SeqCst), 0);
    *repository.1.lock().unwrap() = None;
    let new_task_id = TaskId::new();
    let second = coordinator
        .delegate(
            delegation(run_id, new_task_id, "example.test.pinned"),
            &scope(run_id, Some(new_task_id)),
        )
        .await
        .unwrap();
    assert_eq!(second.snapshot.result.as_deref(), Some("new endpoint"));
    assert_ne!(
        repository.get(new_task_id).await.unwrap().unwrap().admission,
        admitted_identity,
    );
    assert_eq!(new_calls.load(Ordering::SeqCst), 1);
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
    let engine_report = match Engine::default()
        .drive_with_default_validator(
            manager_request(catalog, engine_scope),
            &PROJECTOR,
            &ManagerModel {
                selected_agent_id: "floe.test.ninth".into(),
                observed_coverage: Arc::new(Mutex::new(vec![])),
            },
            &NoTools,
            &coordinator,
            &Journal,
        )
        .await
        .unwrap()
    {
        floe_agent_runtime::EngineOutcome::Completed(report) => report,
        floe_agent_runtime::EngineOutcome::Blocked(blocked) => {
            panic!(
                "manager drive blocked unexpectedly: {:?}",
                blocked.requirement
            )
        }
    };
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

#[tokio::test]
async fn trusted_endpoint_settlement_reaches_the_repository_once() {
    let directory = Directory::default();
    directory
        .register(
            DirectoryEntry {
                definition: definition("floe.test.settlement", 1),
                admission: admission("floe.test.settlement", 1),
                reviewed: true,
                enabled: true,
                admitted_principals: vec!["person-a".into()],
                purposes: vec!["everyday-assistance".into()],
            },
            Arc::new(SettlementEndpoint("memory")),
        )
        .unwrap();
    let repository = Arc::new(MemoryTasks::default());
    let (coordinator, _) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday-assistance",
        floe_agent_contract::MAX_OUTPUT_BYTES,
    )
    .await
    .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let request = delegation(run_id, task_id, "floe.test.settlement");
    let receipt = coordinator
        .delegate(request.clone(), &scope(run_id, Some(task_id)))
        .await
        .unwrap();

    assert_eq!(receipt.snapshot.state, TaskState::Completed);
    assert_eq!(receipt.snapshot.result.as_deref(), Some("settled result"));
    assert_eq!(repository.0.lock().unwrap().settlements, 1);
    assert_eq!(
        coordinator
            .delegate(request, &scope(run_id, Some(task_id)))
            .await
            .unwrap()
            .snapshot,
        receipt.snapshot
    );
    assert_eq!(repository.0.lock().unwrap().settlements, 1);
}

#[tokio::test]
async fn unsupported_endpoint_settlement_becomes_a_failed_task_before_commit() {
    let directory = Directory::default();
    directory
        .register(
            DirectoryEntry {
                definition: definition("floe.test.unsupported-settlement", 1),
                admission: admission("floe.test.unsupported-settlement", 1),
                reviewed: true,
                enabled: true,
                admitted_principals: vec!["person-a".into()],
                purposes: vec!["everyday-assistance".into()],
            },
            Arc::new(SettlementEndpoint("unknown")),
        )
        .unwrap();
    let repository = Arc::new(MemoryTasks::default());
    let (coordinator, _) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday-assistance",
        floe_agent_contract::MAX_OUTPUT_BYTES,
    )
    .await
    .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let receipt = coordinator
        .delegate(
            delegation(run_id, task_id, "floe.test.unsupported-settlement"),
            &scope(run_id, Some(task_id)),
        )
        .await
        .unwrap();

    assert_eq!(receipt.snapshot.state, TaskState::Failed);
    assert_eq!(
        receipt.snapshot.issue,
        Some(AgentFailure::CapabilityUnavailable)
    );
    assert_eq!(repository.0.lock().unwrap().settlements, 0);
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
                admission: admission("floe.test.blocking", 1),
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
    let request_digest = floe_agent_contract::delegation_request_digest(&request);
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
        admission: admission("floe.test.recovery", 1),
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
async fn disabled_selection_is_denied_before_task_admission() {
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
    let repository = Arc::new(MemoryTasks::default());
    let coordinator = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let coordinator = coordinator.0;
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let failure = floe_agent_contract::DelegationPort::delegate(
        &coordinator,
        delegation(run_id, task_id, "floe.test.disabled"),
        &scope(run_id, Some(task_id)),
    )
    .await
    .unwrap_err();

    assert_eq!(failure, AgentFailure::CapabilityDenied);
    assert!(repository.get(task_id).await.unwrap().is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

struct TestProjector;

static PROJECTOR: TestProjector = TestProjector;

impl ModelProjectionPort for TestProjector {
    fn project<'a>(
        &'a self,
        request: ModelProjectionRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<AuthorizedModelProjection, AgentFailure>> {
        request.validate().unwrap();
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
            conversation: request.conversation.clone(),
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
        Box::pin(async move {
            Ok(AuthorizedModelProjection {
                projection_ref: ProjectionRef::new(),
                projection_revision: 1,
                envelope,
                coverage: DependencyCoverage::Independent,
                input_data_classes: vec![DataClass::Synthetic],
            })
        })
    }
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
    ) -> BoxFuture<'a, Result<ModelCallOutcome, AgentFailure>> {
        let observed_coverage = Arc::clone(&self.observed_coverage);
        let selected_agent_id = self.selected_agent_id.clone();
        Box::pin(async move {
            let conversation = &request.projection.envelope.conversation;
            let settled: Vec<DependencyCoverage> = conversation
                .history
                .iter()
                .chain(&conversation.current_turn)
                .filter_map(|entry| match entry {
                    ModelConversationEntry::DelegationExchange { receipt, .. } => {
                        Some(receipt.snapshot.coverage.clone())
                    }
                    _ => None,
                })
                .collect();
            observed_coverage
                .lock()
                .unwrap()
                .extend(settled.iter().cloned());
            let step = if settled.is_empty() {
                ModelStep::Delegate {
                    agent_id: selected_agent_id,
                    definition_revision: 1,
                    message: "Check communication".into(),
                    context_refs: vec![],
                }
            } else {
                ModelStep::Answer {
                    text: "manager continued".into(),
                    artifacts: vec![],
                }
            };
            Ok(ModelCallOutcome::Ready(ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![step],
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
    let report = match Engine::default()
        .drive_with_default_validator(
            manager_request(catalog, root_scope),
            &PROJECTOR,
            &ManagerModel {
                selected_agent_id: "floe.builtin.communication".into(),
                observed_coverage: Arc::clone(&observed_coverage),
            },
            &NoTools,
            &coordinator,
            &Journal,
        )
        .await
        .unwrap()
    {
        floe_agent_runtime::EngineOutcome::Completed(report) => report,
        floe_agent_runtime::EngineOutcome::Blocked(blocked) => {
            panic!(
                "manager drive blocked unexpectedly: {:?}",
                blocked.requirement
            )
        }
    };

    let coverage = observed_coverage.lock().unwrap().clone();
    (report, schedule_calls, communication_calls, coverage)
}

fn manager_request(catalog: AllowedCatalog, scope: ExecutionScope) -> EngineRequest {
    EngineRequest {
        principal: "person-a".into(),
        role_spec: RoleSpec {
            role_id: "manager".into(),
            instructions: "delegate when useful".into(),
            output_contract: "plain text".into(),
        },
        scope,
        conversation: ModelConversation {
            history: vec![],
            current_turn: vec![ModelConversationEntry::User {
                message_id: Uuid::new_v4(),
                text: "What needs attention?".into(),
            }],
        },
        allowed_catalog: catalog,
        purpose: "test-purpose".into(),
        consumer: "test-consumer".into(),
        preferred_profile_id: None,
        max_iterations: 3,
        max_output_bytes: 16 * 1024,
        replay: vec![],
        resume: None,
        delegation_context: Some(delegation_context()),
        lineage: None,
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

#[tokio::test]
async fn coordinator_catalog_lists_directory_admitted_cards_without_model_placement_filter() {
    let directory = Directory::default();
    let mut remote_only = definition("floe.test.remote-only", 3);
    remote_only.card.supported_placements = vec![ModelPlacement::Remote];
    directory
        .register(
            DirectoryEntry {
                definition: remote_only,
                admission: admission("floe.test.remote-only", 3),
                reviewed: true,
                enabled: true,
                admitted_principals: vec!["person-a".into()],
                purposes: vec!["everyday-assistance".into()],
            },
            Arc::new(Endpoint {
                result: Ok("remote-only result"),
                calls: Arc::new(AtomicUsize::new(0)),
            }),
        )
        .unwrap();
    register(
        &directory,
        "floe.test.local",
        Endpoint {
            result: Ok("local result"),
            calls: Arc::new(AtomicUsize::new(0)),
        },
    )
    .unwrap();
    directory
        .register(
            DirectoryEntry {
                definition: definition("floe.test.disabled", 1),
                admission: admission("floe.test.disabled", 1),
                reviewed: true,
                enabled: false,
                admitted_principals: vec!["person-a".into()],
                purposes: vec!["everyday-assistance".into()],
            },
            Arc::new(Endpoint {
                result: Ok("disabled result"),
                calls: Arc::new(AtomicUsize::new(0)),
            }),
        )
        .unwrap();
    directory
        .register(
            DirectoryEntry {
                definition: definition("floe.test.unreviewed", 1),
                admission: admission("floe.test.unreviewed", 1),
                reviewed: false,
                enabled: true,
                admitted_principals: vec!["person-a".into()],
                purposes: vec!["everyday-assistance".into()],
            },
            Arc::new(Endpoint {
                result: Ok("unreviewed result"),
                calls: Arc::new(AtomicUsize::new(0)),
            }),
        )
        .unwrap();
    directory
        .register(
            DirectoryEntry {
                definition: definition("floe.test.other-principal", 1),
                admission: admission("floe.test.other-principal", 1),
                reviewed: true,
                enabled: true,
                admitted_principals: vec!["person-b".into()],
                purposes: vec!["everyday-assistance".into()],
            },
            Arc::new(Endpoint {
                result: Ok("other-principal result"),
                calls: Arc::new(AtomicUsize::new(0)),
            }),
        )
        .unwrap();
    let repository = Arc::new(MemoryTasks::default());
    let (coordinator, _) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();

    let catalog = coordinator.catalog("person-a").unwrap();
    let listed: Vec<(&str, u64)> = catalog
        .cards
        .iter()
        .map(|entry| (entry.card.id.as_str(), entry.definition_revision))
        .collect();
    // Both admitted entries appear with their registered revisions, including
    // the Remote-only card: model placement never filters the root catalog.
    assert_eq!(
        listed,
        vec![("floe.test.local", 1), ("floe.test.remote-only", 3)]
    );
    // Disabled, unreviewed and wrong-principal entries never appear.
    assert_eq!(coordinator.catalog("person-b").unwrap().cards.len(), 1);
    assert!(
        coordinator
            .catalog("person-b")
            .unwrap()
            .cards
            .iter()
            .all(|entry| entry.card.id == "floe.test.other-principal")
    );
}

#[tokio::test]
async fn stale_definition_revision_is_rejected_by_task_coordinator() {
    let directory = Directory::default();
    register(
        &directory,
        "floe.test.versioned",
        Endpoint {
            result: Ok("versioned result"),
            calls: Arc::new(AtomicUsize::new(0)),
        },
    )
    .unwrap();
    let repository = Arc::new(MemoryTasks::default());
    let (coordinator, _) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let mut request = delegation(run_id, task_id, "floe.test.versioned");
    request.selected_definition_revision = 99;
    let failure = coordinator
        .delegate(request, &scope(run_id, Some(task_id)))
        .await
        .unwrap_err();
    assert_eq!(failure, AgentFailure::Conflict);
    assert!(repository.get(task_id).await.unwrap().is_none());
}

#[tokio::test]
async fn exact_duplicate_request_replays_without_endpoint_redispatch() {
    // 2-C C3: same TaskId + same exact request returns the existing Task;
    // the endpoint runs once.
    let directory = Directory::default();
    let calls = Arc::new(AtomicUsize::new(0));
    register(
        &directory,
        "floe.test.replay",
        Endpoint {
            result: Ok("replay result"),
            calls: Arc::clone(&calls),
        },
    )
    .unwrap();
    let repository = Arc::new(MemoryTasks::default());
    let (coordinator, _) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let request = delegation(run_id, task_id, "floe.test.replay");
    let task_scope = scope(run_id, Some(task_id));
    let first = coordinator
        .delegate(request.clone(), &task_scope)
        .await
        .unwrap();
    assert_eq!(first.snapshot.state, TaskState::Completed);
    let second = coordinator.delegate(request, &task_scope).await.unwrap();
    assert_eq!(second.snapshot, first.snapshot);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn same_task_with_changed_execution_context_conflicts() {
    // 2-C C3: the canonical request digest covers the execution context, so
    // a changed device/session with the same TaskId conflicts instead of
    // reusing the prior Task result.
    let directory = Directory::default();
    let calls = Arc::new(AtomicUsize::new(0));
    register(
        &directory,
        "floe.test.context",
        Endpoint {
            result: Ok("context result"),
            calls: Arc::clone(&calls),
        },
    )
    .unwrap();
    let repository = Arc::new(MemoryTasks::default());
    let (coordinator, _) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let request = delegation(run_id, task_id, "floe.test.context");
    let task_scope = scope(run_id, Some(task_id));
    let first = coordinator
        .delegate(request.clone(), &task_scope)
        .await
        .unwrap();
    assert_eq!(first.snapshot.state, TaskState::Completed);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut changed = request.clone();
    changed.execution_context.device_id = "changed-device".into();
    changed.execution_context.session_id = Uuid::new_v4();
    assert_eq!(
        coordinator.delegate(changed, &task_scope).await.err(),
        Some(AgentFailure::Conflict)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // The exact original still replays.
    let replayed = coordinator.delegate(request, &task_scope).await.unwrap();
    assert_eq!(replayed.snapshot, first.snapshot);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn denied_endpoint_produces_rejected_terminal_task() {
    // 2-C C3: denied endpoints settle as typed Rejected Tasks, not thrown
    // Manager failures.
    for failure in [
        AgentFailure::CapabilityDenied,
        AgentFailure::PolicyDenied,
        AgentFailure::ConsentRequired,
    ] {
        let directory = Directory::default();
        register(
            &directory,
            "floe.test.denied",
            Endpoint {
                result: Err(failure),
                calls: Arc::new(AtomicUsize::new(0)),
            },
        )
        .unwrap();
        let repository = Arc::new(MemoryTasks::default());
        let (coordinator, _) = TaskCoordinator::activate(
            directory,
            Arc::clone(&repository),
            "everyday-assistance",
            16 * 1024,
        )
        .await
        .unwrap();
        let run_id = RunId::new();
        let task_id = TaskId::new();
        let receipt = coordinator
            .delegate(
                delegation(run_id, task_id, "floe.test.denied"),
                &scope(run_id, Some(task_id)),
            )
            .await
            .unwrap();
        assert_eq!(receipt.snapshot.state, TaskState::Rejected, "{failure:?}");
        assert_eq!(receipt.snapshot.issue, Some(failure), "{failure:?}");
        assert_eq!(receipt.snapshot.result, None);
    }
}

struct DeadlineEndpoint;

impl AgentEndpoint for DeadlineEndpoint {
    fn execute<'a>(
        &'a self,
        invocation: EndpointInvocation,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ExpertReport, AgentFailure>> {
        Box::pin(async move {
            scope
                .cancellation()
                .cancel_with_reason(floe_agent_contract::CancelReason::Deadline);
            Ok(ExpertReport {
                task_id: invocation.request.task_id,
                principal: invocation.request.principal,
                agent_id: invocation.request.selected_agent_id,
                definition_revision: invocation.request.selected_definition_revision,
                result: "too late".into(),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                settlement: None,
            })
        })
    }
}

#[tokio::test]
async fn deadline_during_execution_produces_timed_out_terminal_task() {
    // 2-C C3: a deadline observed while the endpoint runs settles a typed
    // TimedOut Task.
    let directory = Directory::default();
    directory
        .register(
            DirectoryEntry {
                definition: definition("floe.test.deadline", 1),
                admission: admission("floe.test.deadline", 1),
                reviewed: true,
                enabled: true,
                admitted_principals: vec!["person-a".into()],
                purposes: vec!["everyday-assistance".into()],
            },
            Arc::new(DeadlineEndpoint),
        )
        .unwrap();
    let repository = Arc::new(MemoryTasks::default());
    let (coordinator, _) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday-assistance",
        16 * 1024,
    )
    .await
    .unwrap();
    let run_id = RunId::new();
    let task_id = TaskId::new();
    let receipt = coordinator
        .delegate(
            delegation(run_id, task_id, "floe.test.deadline"),
            &scope(run_id, Some(task_id)),
        )
        .await
        .unwrap();
    assert_eq!(receipt.snapshot.state, TaskState::TimedOut);
    assert_eq!(receipt.snapshot.issue, Some(AgentFailure::DeadlineExceeded));
    assert_eq!(receipt.snapshot.result, None);
}
