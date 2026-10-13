use super::*;
use floe_agent_contract::{
    BoxFuture, ModelBindingDigest, ModelBudgetProfile, ModelCapabilities, ModelPlanRequest,
    ModelPort, ModelRequest, ModelResponse, ModelSelectionCommitment, ModelStep, ModelUsage,
    PreparedModelCall, PreparedModelPlan, ProcessingBoundary,
};
use floe_context::{DependencyAuthorization, DependencyResolver};
use floe_conversation::{CompactionDisposition, CompactionDriver, CompactionRepository};
use floe_execution::{Cancellation, ExecutionScope};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

struct NoDependencies;
impl DependencyResolver for NoDependencies {
    fn authorize<'a>(
        &'a self,
        _: &'a floe_agent_contract::ContextDependency,
        _: &'a DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { panic!("synthetic history has no source dependencies") })
    }
    fn authorize_memories<'a>(
        &'a self,
        _: PersonId,
        _: &'a [floe_context_contract::MemoryRevisionReference],
        _: &'a DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { panic!("compaction must not read saved memories") })
    }
}
#[derive(Clone, Copy)]
enum Behavior {
    Answer,
    PreflightReject,
    Unknown,
    InvalidAnswer,
}
struct Model {
    behavior: Behavior,
    calls: Arc<AtomicUsize>,
    prepares: Arc<AtomicUsize>,
}
struct Prepared {
    plan: PreparedModelPlan,
    behavior: Behavior,
    calls: Arc<AtomicUsize>,
}
impl ModelPort for Model {
    fn prepare<'a>(
        &'a self,
        request: ModelPlanRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedModelCall>, AgentFailure>> {
        Box::pin(async move {
            self.prepares.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Prepared {
                plan: PreparedModelPlan {
                    operation_id: Uuid::new_v4(),
                    principal: request.principal,
                    device_id: request.device_id,
                    purpose: request.purpose,
                    consumer: request.consumer,
                    capabilities: ModelCapabilities::chat(),
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([2; 32]),
                    selection_commitment: Some(ModelSelectionCommitment([1; 32])),
                    budget_profile: Some(ModelBudgetProfile::unknown()),
                },
                behavior: self.behavior,
                calls: self.calls.clone(),
            }) as Box<dyn PreparedModelCall>)
        })
    }
}
impl PreparedModelCall for Prepared {
    fn plan(&self) -> &PreparedModelPlan {
        &self.plan
    }
    fn validate_request(&self, _: &ModelRequest, _: &ExecutionScope) -> Result<(), AgentFailure> {
        if matches!(self.behavior, Behavior::PreflightReject) {
            Err(AgentFailure::BudgetExceeded)
        } else {
            Ok(())
        }
    }
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut tokens = request.reservation_ceiling.tokens;
            let mut cost = request.reservation_ceiling.cost_micros;
            let mut attempt =
                scope
                    .budget()
                    .begin_model_attempt(request.attempt_id, &mut tokens, &mut cost)?;
            attempt.mark_dispatched();
            if matches!(self.behavior, Behavior::Unknown) {
                return Err(AgentFailure::BudgetExceeded);
            }
            let receipt = attempt.settle_observed(Some(10), Some(10))?;
            Ok(ModelResponse {
                attempt_id: request.attempt_id,
                steps: vec![ModelStep::Answer {
                    text: if matches!(self.behavior, Behavior::InvalidAnswer) {
                        " invalid "
                    } else {
                        "Earlier test inputs."
                    }
                    .into(),
                    artifacts: vec![],
                }],
                usage: ModelUsage {
                    tokens: receipt.charged_tokens,
                    cost_micros: receipt.charged_cost_micros,
                },
                accounting: receipt.accounting,
            })
        })
    }
}

#[tokio::test]
async fn compaction_driver_executes_once_and_replays_committed_receipt() {
    let (mut scenario, mut actor, request) = compaction_custody_fixture().await;
    actor.runtime_epoch = 77; // Host lifetime is distinct from durable executor generation.
    let vault = Arc::new(scenario.vault.take().unwrap());
    let repository = Arc::new(crate::repositories::VaultConversationRepository::new(
        vault.clone(),
        scenario.identity.clone(),
    ));
    let job = repository
        .admit_compaction(&actor, request)
        .await
        .unwrap()
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let prepares = Arc::new(AtomicUsize::new(0));
    let mut driver = CompactionDriver::new(
        actor.clone(),
        repository.clone(),
        Arc::new(crate::repositories::ContextEvidenceReader::new(
            vault.clone(),
        )),
        Arc::new(NoDependencies),
        Arc::new(Model {
            behavior: Behavior::Answer,
            calls: calls.clone(),
            prepares: prepares.clone(),
        }),
        Duration::from_secs(30),
    )
    .unwrap();
    let receipt = driver.drive(job.claim, Cancellation::new()).await.unwrap();
    assert_eq!(receipt.summary.text, "Earlier test inputs.");
    assert_eq!(receipt.session_revision, job.source.session_revision);
    vault.activate_conversation_executor().await.unwrap();
    assert_eq!(
        driver.drive(job.claim, Cancellation::new()).await.unwrap(),
        receipt
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(prepares.load(Ordering::SeqCst), 1);
    let recorded = repository.load_compaction(&actor, job.claim).await.unwrap();
    assert!(matches!(
        recorded.disposition,
        CompactionDisposition::Committed { .. }
    ));
    assert_eq!(
        recorded
            .entries
            .iter()
            .filter(|e| matches!(
                e.event,
                floe_agent_contract::JournalEvent::ModelIntent { .. }
            ))
            .count(),
        1
    );
}

#[tokio::test]
async fn compaction_driver_distinguishes_pure_rejection_from_uncertain_dispatch() {
    for (behavior, expected, expected_calls) in [
        (Behavior::PreflightReject, AgentFailure::BudgetExceeded, 0),
        (Behavior::Unknown, AgentFailure::Interrupted, 1),
        (Behavior::InvalidAnswer, AgentFailure::Interrupted, 2),
    ] {
        let (mut scenario, actor, request) = compaction_custody_fixture().await;
        let vault = Arc::new(scenario.vault.take().unwrap());
        let repository = Arc::new(crate::repositories::VaultConversationRepository::new(
            vault.clone(),
            scenario.identity.clone(),
        ));
        let job = repository
            .admit_compaction(&actor, request)
            .await
            .unwrap()
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let prepares = Arc::new(AtomicUsize::new(0));
        let mut driver = CompactionDriver::new(
            actor.clone(),
            repository.clone(),
            Arc::new(crate::repositories::ContextEvidenceReader::new(vault)),
            Arc::new(NoDependencies),
            Arc::new(Model {
                behavior,
                calls: calls.clone(),
                prepares: prepares.clone(),
            }),
            Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(
            driver.drive(job.claim, Cancellation::new()).await.err(),
            Some(expected)
        );
        assert_eq!(
            driver.drive(job.claim, Cancellation::new()).await.err(),
            Some(expected)
        );
        assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
        assert_eq!(prepares.load(Ordering::SeqCst), 1);
        let recorded = repository.load_compaction(&actor, job.claim).await.unwrap();
        assert_eq!(
            recorded.disposition,
            CompactionDisposition::Retired { failure: expected }
        );
        if expected_calls == 0 {
            assert!(recorded.entries.is_empty());
        }
    }
}

#[tokio::test]
async fn compaction_driver_recovers_output_without_even_preparing_a_model() {
    let (mut scenario, actor, request) = compaction_custody_fixture().await;
    let job = scenario
        .vault()
        .admit_manager_compaction(&actor, request)
        .await
        .unwrap()
        .unwrap();
    record_compaction_answer(&scenario, &actor, &job, "Recorded answer.").await;
    let vault = Arc::new(scenario.vault.take().unwrap());
    let repository = Arc::new(crate::repositories::VaultConversationRepository::new(
        vault.clone(),
        scenario.identity.clone(),
    ));
    let calls = Arc::new(AtomicUsize::new(0));
    let prepares = Arc::new(AtomicUsize::new(0));
    let mut driver = CompactionDriver::new(
        actor,
        repository,
        Arc::new(crate::repositories::ContextEvidenceReader::new(vault)),
        Arc::new(NoDependencies),
        Arc::new(Model {
            behavior: Behavior::Unknown,
            calls: calls.clone(),
            prepares: prepares.clone(),
        }),
        Duration::from_secs(30),
    )
    .unwrap();
    let receipt = driver.drive(job.claim, Cancellation::new()).await.unwrap();
    assert_eq!(receipt.summary.text, "Recorded answer.");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(prepares.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn compaction_driver_retires_recorded_output_when_source_has_moved() {
    let (mut scenario, actor, request) = compaction_custody_fixture().await;
    let job = scenario
        .vault()
        .admit_manager_compaction(&actor, request)
        .await
        .unwrap()
        .unwrap();
    record_compaction_answer(&scenario, &actor, &job, "Recorded answer.").await;
    let (run, _) = scenario
        .admit_run_and_append_input("New foreground input")
        .await;
    scenario.cancel_owner_run(&run).await;
    let vault = Arc::new(scenario.vault.take().unwrap());
    let repository = Arc::new(crate::repositories::VaultConversationRepository::new(
        vault.clone(),
        scenario.identity.clone(),
    ));
    let prepares = Arc::new(AtomicUsize::new(0));
    let mut driver = CompactionDriver::new(
        actor.clone(),
        repository.clone(),
        Arc::new(crate::repositories::ContextEvidenceReader::new(vault)),
        Arc::new(NoDependencies),
        Arc::new(Model {
            behavior: Behavior::Unknown,
            calls: Arc::new(AtomicUsize::new(0)),
            prepares: prepares.clone(),
        }),
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(
        driver.drive(job.claim, Cancellation::new()).await.err(),
        Some(AgentFailure::Conflict)
    );
    assert_eq!(
        repository
            .load_compaction(&actor, job.claim)
            .await
            .unwrap()
            .disposition,
        CompactionDisposition::Retired {
            failure: AgentFailure::Conflict
        }
    );
    assert_eq!(prepares.load(Ordering::SeqCst), 0);
}
