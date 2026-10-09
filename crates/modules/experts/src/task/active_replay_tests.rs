use std::{
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use floe_agent_contract::{
    A2A_PROTOCOL_VERSION, AGENT_SCHEMA_VERSION, AgentCard, AgentContext, AgentDefinition,
    AgentEndpoint, AgentFailure, Cancellation, DelegationExecutionContext, DelegationPort,
    DelegationRequest, DependencyCoverage, EndpointInvocation, ExecutionJournal, ExecutionScope,
    ExpertExecutionOutcome, InvocationKey, JournalAck, JournalEntry, JournalEvent,
    MAX_OUTPUT_BYTES, ModelConversation, ModelConversationEntry, PackageKind, PackageRef,
    TaskExecutionKey, TaskExecutionReceipt, TaskExecutionReceiptRef, TaskId, TaskSnapshot,
    TaskState, TraceContext,
};
use floe_conversation_contract::{ConversationBranchId, ConversationId, MessageId};
use floe_execution::budget::{BudgetConfig, BudgetLedger, ModelUsage};
use floe_kernel::{PersonId, RunId};
use tokio::sync::{Notify, oneshot};
use uuid::Uuid;

use super::*;
use crate::{
    Directory, DirectoryEntry, ExpertAdmissionIdentity, ExpertExecutionSelection, ExpertHistoryPin,
    ExpertTaskAdmissionReference, ExpertTaskConversation, ExpertTaskConversationDraft,
    ExpertTaskConversationInput, TaskActivation, TaskAdmission, TaskExecutionCommit, TaskRecord,
};

#[derive(Default)]
struct StoredTask {
    record: Option<TaskRecord>,
    admission: Option<ExpertTaskAdmissionReference>,
    conversation: Option<ExpertTaskConversation>,
}

struct AdmissionBarrierRepository {
    stored: Mutex<StoredTask>,
    admission_started: Notify,
    release_admission: Notify,
    pause_admission: bool,
    fail_admission: bool,
    admission_calls: AtomicUsize,
    reference_reads: AtomicUsize,
    early_reference_reads: AtomicUsize,
}

impl AdmissionBarrierRepository {
    fn new(pause_admission: bool, fail_admission: bool) -> Self {
        Self {
            stored: Mutex::new(StoredTask::default()),
            admission_started: Notify::new(),
            release_admission: Notify::new(),
            pause_admission,
            fail_admission,
            admission_calls: AtomicUsize::new(0),
            reference_reads: AtomicUsize::new(0),
            early_reference_reads: AtomicUsize::new(0),
        }
    }

    fn stored(&self) -> std::sync::MutexGuard<'_, StoredTask> {
        self.stored
            .lock()
            .expect("Task fixture mutex remains usable")
    }
}

struct NoopJournal;

impl ExecutionJournal for NoopJournal {
    fn record_intent<'a>(
        &'a self,
        _event: JournalEvent,
    ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::StorageUnavailable) })
    }

    fn record_result<'a>(
        &'a self,
        _event: JournalEvent,
    ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::StorageUnavailable) })
    }

    fn record_output<'a>(
        &'a self,
        _event: JournalEvent,
    ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::StorageUnavailable) })
    }

    fn checkpoint<'a>(
        &'a self,
        _event: JournalEvent,
    ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::StorageUnavailable) })
    }
}

impl TaskRepository for AdmissionBarrierRepository {
    fn activate<'a>(
        &'a self,
    ) -> floe_agent_contract::BoxFuture<'a, Result<TaskActivation, AgentFailure>> {
        Box::pin(async {
            Ok(TaskActivation {
                executor_generation: 1,
                interrupted: vec![],
            })
        })
    }

    fn admit<'a>(
        &'a self,
        proposed: TaskRecord,
        draft: ExpertTaskConversationDraft,
    ) -> floe_agent_contract::BoxFuture<'a, Result<TaskAdmission, AgentFailure>> {
        Box::pin(async move {
            self.admission_calls.fetch_add(1, Ordering::SeqCst);
            self.admission_started.notify_one();
            if self.pause_admission {
                self.release_admission.notified().await;
            }
            if self.fail_admission {
                return Err(AgentFailure::StorageUnavailable);
            }

            let identity = draft.key.core_identity()?;
            let input = ExpertTaskConversationInput {
                execution: proposed.execution(),
                request_digest: proposed.request_digest,
                key: draft.key,
                identity,
                conversation_id: ConversationId::new(),
                branch_id: ConversationBranchId::new(),
                run_id: RunId::new(),
                input_message_id: MessageId::new(),
                input_command_id: CommandId::new(),
                delegated_message: draft.delegated_message.clone(),
                input_coverage: draft.input_coverage,
                input_reference: None,
                history_pin: ExpertHistoryPin {
                    head_revision: 0,
                    head_reference: None,
                    prefix_digest: [0; 32],
                },
            };
            let admission = input.issue_admission_reference()?;
            let conversation = ExpertTaskConversation {
                conversation: ModelConversation {
                    history: vec![],
                    current_turn: vec![ModelConversationEntry::User {
                        message_id: input.input_message_id.as_uuid(),
                        text: input.delegated_message,
                    }],
                },
                history_coverage: vec![],
            };
            conversation.validate()?;
            let mut stored = self.stored();
            if stored.record.is_some() {
                return Err(AgentFailure::Conflict);
            }
            stored.record = Some(proposed.clone());
            stored.admission = Some(admission);
            stored.conversation = Some(conversation);
            Ok(TaskAdmission::Created {
                record: proposed,
                expert_input: admission,
            })
        })
    }

    fn load_conversation<'a>(
        &'a self,
        execution: TaskExecutionKey,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ExpertTaskConversation, AgentFailure>> {
        Box::pin(async move {
            let stored = self.stored();
            let record = stored.record.as_ref().ok_or(AgentFailure::NotFound)?;
            if record.execution() != execution {
                return Err(AgentFailure::Conflict);
            }
            stored
                .conversation
                .clone()
                .ok_or(AgentFailure::StorageUnavailable)
        })
    }

    fn expert_admission_reference<'a>(
        &'a self,
        execution: TaskExecutionKey,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ExpertTaskAdmissionReference, AgentFailure>>
    {
        Box::pin(async move {
            self.reference_reads.fetch_add(1, Ordering::SeqCst);
            let stored = self.stored();
            let Some(record) = stored.record.as_ref() else {
                self.early_reference_reads.fetch_add(1, Ordering::SeqCst);
                return Err(AgentFailure::NotFound);
            };
            if record.execution() != execution {
                return Err(AgentFailure::Conflict);
            }
            stored.admission.ok_or(AgentFailure::StorageUnavailable)
        })
    }

    fn compare_and_swap<'a>(
        &'a self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
        expert_input: ExpertTaskAdmissionReference,
    ) -> floe_agent_contract::BoxFuture<'a, Result<TaskRecord, AgentFailure>> {
        Box::pin(async move {
            let mut stored = self.stored();
            if stored.admission != Some(expert_input) {
                return Err(AgentFailure::StorageUnavailable);
            }
            let record = stored.record.as_ref().ok_or(AgentFailure::NotFound)?;
            if record.snapshot.task_id != task_id {
                return Err(AgentFailure::NotFound);
            }
            let next = record.transition(
                expected_aggregate_revision,
                executor_generation,
                snapshot,
                MAX_OUTPUT_BYTES,
            )?;
            stored.record = Some(next.clone());
            Ok(next)
        })
    }

    fn journal(
        &self,
        _execution: TaskExecutionKey,
    ) -> Result<Arc<dyn ExecutionJournal>, AgentFailure> {
        Ok(Arc::new(NoopJournal))
    }

    fn load_journal<'a>(
        &'a self,
        execution: TaskExecutionKey,
    ) -> floe_agent_contract::BoxFuture<'a, Result<Vec<JournalEntry>, AgentFailure>> {
        Box::pin(async move {
            let stored = self.stored();
            if stored
                .record
                .as_ref()
                .is_none_or(|record| record.execution() != execution)
            {
                return Err(AgentFailure::NotFound);
            }
            Ok(vec![])
        })
    }

    fn read_execution_receipt<'a>(
        &'a self,
        reference: TaskExecutionReceiptRef,
    ) -> floe_agent_contract::BoxFuture<'a, Result<TaskExecutionReceipt, AgentFailure>> {
        Box::pin(async move {
            let stored = self.stored();
            let record = stored.record.as_ref().ok_or(AgentFailure::NotFound)?;
            if record.execution() != reference.execution {
                return Err(AgentFailure::Conflict);
            }
            let receipt = record.receipt.clone().ok_or(AgentFailure::Conflict)?;
            if receipt.reference != reference {
                return Err(AgentFailure::Conflict);
            }
            Ok(receipt)
        })
    }

    fn validate_settlement(
        &self,
        _settlement: &floe_agent_contract::EndpointSettlement,
    ) -> Result<(), AgentFailure> {
        Ok(())
    }

    fn settle_execution<'a>(
        &'a self,
        commit: TaskExecutionCommit,
    ) -> floe_agent_contract::BoxFuture<'a, Result<TaskExecutionReceipt, AgentFailure>> {
        Box::pin(async move {
            let mut stored = self.stored();
            let record = stored.record.as_ref().ok_or(AgentFailure::NotFound)?;
            let expected =
                crate::task_record::settle_task_execution(record, &commit, &[], MAX_OUTPUT_BYTES)?;
            let receipt = expected
                .receipt
                .clone()
                .ok_or(AgentFailure::StorageUnavailable)?;
            stored.record = Some(expected);
            Ok(receipt)
        })
    }

    fn get<'a>(
        &'a self,
        task_id: TaskId,
    ) -> floe_agent_contract::BoxFuture<'a, Result<Option<TaskRecord>, AgentFailure>> {
        Box::pin(async move {
            Ok(self
                .stored()
                .record
                .as_ref()
                .filter(|record| record.snapshot.task_id == task_id)
                .cloned())
        })
    }
}

struct RejectingEndpoint {
    dispatches: Arc<AtomicUsize>,
}

impl AgentEndpoint for RejectingEndpoint {
    fn execute<'a>(
        &'a self,
        _invocation: EndpointInvocation,
        _scope: &'a ExecutionScope,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ExpertExecutionOutcome, AgentFailure>> {
        self.dispatches.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(AgentFailure::PolicyDenied) })
    }
}

fn setup() -> (
    Directory,
    Arc<AdmissionBarrierRepository>,
    Arc<AtomicUsize>,
    PersonId,
    TaskId,
    RunId,
    DelegationRequest,
) {
    let person_id = PersonId::new();
    let task_id = TaskId::new();
    let parent_run_id = RunId::new();
    let dispatches = Arc::new(AtomicUsize::new(0));
    let directory = Directory::default();
    let admission = ExpertAdmissionIdentity {
        registry_instance_id: Uuid::new_v4(),
        assignment_id: Uuid::new_v4(),
        installation_id: Uuid::new_v4(),
        package: PackageRef {
            kind: PackageKind::Expert,
            id: "fixture.expert".into(),
            version: "1.0.0".into(),
        },
        definition_revision: 1,
    };
    let selection =
        ExpertExecutionSelection::without_requirements(1).expect("valid fixture selection");
    let endpoint = Arc::new(RejectingEndpoint {
        dispatches: Arc::clone(&dispatches),
    });
    directory
        .register(
            DirectoryEntry {
                definition: AgentDefinition {
                    card: AgentCard {
                        schema_version: AGENT_SCHEMA_VERSION,
                        protocol_version: A2A_PROTOCOL_VERSION.into(),
                        id: "fixture.expert".into(),
                        version: "1.0.0".into(),
                        name: "Fixture Expert".into(),
                        description: "A deterministic Task admission fixture".into(),
                        domain_tags: vec![],
                        skills: vec![],
                    },
                    definition_revision: 1,
                },
                admission,
                selection,
                reviewed: true,
                enabled: true,
                admitted_principals: vec![],
                purposes: vec!["everyday_assistance".into()],
            },
            endpoint,
        )
        .expect("register fixture Expert");
    let request = DelegationRequest {
        task_id,
        parent_run_id: Some(parent_run_id.as_uuid()),
        principal: person_id.to_string(),
        invocation_key: InvocationKey::new(),
        selected_agent_id: "fixture.expert".into(),
        selected_definition_revision: 1,
        message: "test delegated request".into(),
        context_refs: vec![],
        execution_context: DelegationExecutionContext {
            session_id: Uuid::new_v4(),
            device_id: "device-fixture".into(),
            agent_context: AgentContext {
                projection_version: 1,
                persona: None,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            },
            max_output_bytes: MAX_OUTPUT_BYTES,
            projection_coverage: DependencyCoverage::Independent,
        },
    };
    (
        directory,
        Arc::new(AdmissionBarrierRepository::new(true, false)),
        dispatches,
        person_id,
        task_id,
        parent_run_id,
        request,
    )
}

fn scope_for(task_id: TaskId, parent_run_id: RunId) -> ExecutionScope {
    let budget = BudgetLedger::new(BudgetConfig::new(64, 64), ModelUsage::default());
    ExecutionScope::root(
        Cancellation::new(),
        tokio::time::Instant::now() + Duration::from_secs(30),
        budget.root_lease(),
        TraceContext::new(Uuid::new_v4())
            .with_run_id(parent_run_id)
            .with_task_id(task_id),
    )
}

fn spawn_delegate(
    environment: Arc<RunExpertEnvironment<AdmissionBarrierRepository>>,
    request: DelegationRequest,
    scope: ExecutionScope,
) -> tokio::task::JoinHandle<Result<floe_agent_contract::TaskReceipt, AgentFailure>> {
    tokio::spawn(async move { environment.delegate(request, &scope).await })
}

fn spawn_delegate_and_signal_after_first_poll(
    environment: Arc<RunExpertEnvironment<AdmissionBarrierRepository>>,
    request: DelegationRequest,
    scope: ExecutionScope,
) -> (
    tokio::task::JoinHandle<Result<floe_agent_contract::TaskReceipt, AgentFailure>>,
    oneshot::Receiver<()>,
) {
    let (sender, receiver) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut future = Box::pin(environment.delegate(request, &scope));
        let mut sender = Some(sender);
        std::future::poll_fn(|context| {
            let polled = Future::poll(future.as_mut(), context);
            if let Some(sender) = sender.take() {
                let _ = sender.send(());
            }
            polled
        })
        .await
    });
    (task, receiver)
}

#[tokio::test]
async fn duplicate_joins_registered_driver_before_admission_reference_exists() {
    let (directory, repository, dispatches, person_id, task_id, parent_run_id, request) = setup();
    let (coordinator, interrupted) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday_assistance",
        MAX_OUTPUT_BYTES,
    )
    .await
    .expect("activate fixture Task owner");
    assert!(interrupted.is_empty());
    let coordinator = Arc::new(coordinator);
    let environment = Arc::new(
        coordinator
            .environment(&person_id.to_string())
            .expect("build admitted fixture environment"),
    );
    let scope = scope_for(task_id, parent_run_id);
    let first = spawn_delegate(Arc::clone(&environment), request.clone(), scope.clone());
    repository.admission_started.notified().await;

    let (duplicate, polled) =
        spawn_delegate_and_signal_after_first_poll(Arc::clone(&environment), request, scope);
    polled.await.expect("duplicate reached its first poll");
    repository.release_admission.notify_one();

    let first = first.await.expect("first delegate task did not panic");
    let duplicate = duplicate
        .await
        .expect("duplicate delegate task did not panic");
    assert_eq!(duplicate, first, "both callers join the same Task receipt");
    let receipt = first.expect("the single admitted Task returns its terminal receipt");
    assert_eq!(receipt.snapshot.state, TaskState::Rejected);
    assert_eq!(receipt.snapshot.issue, Some(AgentFailure::PolicyDenied));
    assert_eq!(repository.admission_calls.load(Ordering::SeqCst), 1);
    assert_eq!(repository.early_reference_reads.load(Ordering::SeqCst), 0);
    assert_eq!(repository.reference_reads.load(Ordering::SeqCst), 1);
    assert_eq!(dispatches.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn duplicate_joining_failed_admission_does_not_invent_receipt_or_reference() {
    let (directory, mut repository, dispatches, person_id, task_id, parent_run_id, request) =
        setup();
    let repository_mut = Arc::get_mut(&mut repository).expect("fixture repository is unique");
    repository_mut.fail_admission = true;
    let (coordinator, interrupted) = TaskCoordinator::activate(
        directory,
        Arc::clone(&repository),
        "everyday_assistance",
        MAX_OUTPUT_BYTES,
    )
    .await
    .expect("activate fixture Task owner");
    assert!(interrupted.is_empty());
    let coordinator = Arc::new(coordinator);
    let environment = Arc::new(
        coordinator
            .environment(&person_id.to_string())
            .expect("build admitted fixture environment"),
    );
    let scope = scope_for(task_id, parent_run_id);
    let first = spawn_delegate(Arc::clone(&environment), request.clone(), scope.clone());
    repository.admission_started.notified().await;

    let (duplicate, polled) =
        spawn_delegate_and_signal_after_first_poll(Arc::clone(&environment), request, scope);
    polled.await.expect("duplicate reached its first poll");
    repository.release_admission.notify_one();

    let first = first.await.expect("first delegate task did not panic");
    let duplicate = duplicate
        .await
        .expect("duplicate delegate task did not panic");
    assert_eq!(first, Err(AgentFailure::StorageUnavailable));
    assert_eq!(duplicate, first, "duplicate joins the retained failure");
    assert_eq!(repository.admission_calls.load(Ordering::SeqCst), 1);
    assert_eq!(repository.reference_reads.load(Ordering::SeqCst), 0);
    assert_eq!(repository.early_reference_reads.load(Ordering::SeqCst), 0);
    assert_eq!(dispatches.load(Ordering::SeqCst), 0);
    assert!(repository.stored().record.is_none());
}
