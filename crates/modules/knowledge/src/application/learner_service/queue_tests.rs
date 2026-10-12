use super::*;
use crate::*;
use chrono::{DateTime, Utc};
use floe_agent_contract::{CommandId, ExecutionJournal, PersonId};
use floe_context_contract::{ContextIssueReason, DependencyCoverage};
use floe_kernel::CommandFailure;
use std::sync::Mutex;
use uuid::Uuid;

struct Fixture {
    person: PersonId,
    partial: bool,
    claim_fails: bool,
    oversized_first: bool,
    calls: Mutex<Vec<&'static str>>,
}
impl Fixture {
    fn call(&self, name: &'static str) {
        self.calls.lock().unwrap().push(name);
    }
    fn session(&self) -> LearningSessionSnapshot {
        let turn = Uuid::new_v4();
        let message = Uuid::new_v4();
        LearningSessionSnapshot {
            evidence: LearningEvidenceSnapshot {
                person_id: self.person,
                session_id: Uuid::new_v4(),
                revision: 1,
                outcome: Some(LearningOutcome::Completed),
                personal: true,
                active_turn: false,
                pending_output: false,
                turn_ids: vec![turn],
                coverage: DependencyCoverage::Independent,
                purpose: EvidenceProjectionPurpose::Learning,
            },
            messages: vec![
                LearningTranscriptMessage::User {
                    message_id: message,
                    turn_id: turn,
                    text: "Please remember that I prefer tea.".into(),
                },
                LearningTranscriptMessage::Assistant {
                    turn_id: turn,
                    original_user_message_id: message,
                    text: "You prefer tea.".into(),
                },
            ],
        }
    }
}

#[allow(unused_variables)]
impl KnowledgeRepository for Fixture {
    fn read_context<'a>(
        &'a self,
        actor: &'a OwnerActor,
        now: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::MemoryContextSnapshot, AgentFailure>> {
        self.call("context");
        Box::pin(async {
            Ok(MemoryContextSnapshot {
                memories: vec![],
                issue: self.partial.then_some(ContextIssueReason::BudgetExceeded),
            })
        })
    }
    fn overview<'a>(
        &'a self,
        actor: &'a OwnerActor,
        limit: usize,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::MemoryOverviewSnapshot, AgentFailure>> {
        panic!("unexpected overview")
    }
    fn review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::MemoryReviewSnapshot, AgentFailure>> {
        panic!("unexpected review")
    }
    fn decide<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: MemoryDecisionRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::KnowledgeDecisionResult, CommandFailure<AgentFailure>>> {
        panic!("unexpected decide")
    }
    fn forget<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: crate::MemoryForgetRequest,
        now: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::MemoryForgetAcknowledgement, CommandFailure<AgentFailure>>>
    {
        panic!("unexpected forget")
    }
    fn stage<'a>(
        &'a self,
        request: MemoryStageRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::KnowledgeCandidate, AgentFailure>> {
        panic!("unexpected stage")
    }
}

#[allow(unused_variables)]
impl LearnerJobRepository for Fixture {
    fn discovery_sessions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        limit: usize,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<LearningSessionSnapshot>, AgentFailure>> {
        self.call("discovery");
        Box::pin(async {
            if self.claim_fails {
                return Err(AgentFailure::StorageUnavailable);
            }
            let mut sessions = vec![];
            if self.oversized_first {
                let mut large = self.session();
                if let LearningTranscriptMessage::User { text, .. } = &mut large.messages[0] {
                    *text = format!("Please remember: {}", "x".repeat(4096));
                }
                sessions.push(large);
            }
            sessions.push(self.session());
            Ok(sessions)
        })
    }
    fn enqueue<'a>(
        &'a self,
        actor: &'a OwnerActor,
        input: crate::LearnerReviewInput,
        available_at: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::LearnerReviewJob, AgentFailure>> {
        self.call("enqueue");
        Box::pin(async { Err(AgentFailure::Conflict) })
    }
    fn claim_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        budget: crate::LearnerBudget,
        now: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<crate::LearnerReviewJob>, AgentFailure>> {
        self.call("claim");
        Box::pin(async {
            if self.claim_fails {
                Err(AgentFailure::Conflict)
            } else {
                Ok(None)
            }
        })
    }
    fn settle_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        job_id: Uuid,
        expected_attempt: u8,
        settlement: crate::LearnerJobSettlement,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        panic!("unexpected settle_review")
    }
}

#[allow(unused_variables)]
impl LearnerJournalFactory for Fixture {
    fn journal(
        &self,
        person_id: PersonId,
        claim: crate::LearnerClaimRef,
    ) -> Result<std::sync::Arc<dyn ExecutionJournal>, AgentFailure> {
        panic!("unexpected journal")
    }
    fn load_journal<'a>(
        &'a self,
        person_id: PersonId,
        claim: crate::LearnerClaimRef,
    ) -> BoxFuture<'a, Result<crate::LearnerClaimJournal, AgentFailure>> {
        panic!("unexpected load_journal")
    }
}

#[allow(unused_variables)]
impl LearnerProjectionPort for Fixture {
    fn project<'a>(
        &'a self,
        request: LearnerProjectionRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelProjectionOutcome, AgentFailure>> {
        panic!("unexpected project")
    }
}

impl ModelPort for Fixture {
    fn prepare<'a>(
        &'a self,
        _: floe_agent_contract::ModelPlanRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Box<dyn floe_agent_contract::PreparedModelCall>, AgentFailure>> {
        panic!("queue tests must not invoke a model")
    }
}
fn service(
    partial: bool,
    claim_fails: bool,
    oversized_first: bool,
) -> (LearnerService, Arc<Fixture>) {
    let fixture = Arc::new(Fixture {
        person: PersonId::new(),
        partial,
        claim_fails,
        oversized_first,
        calls: Mutex::new(vec![]),
    });
    let learner = LearnerService::new(
        OwnerActor {
            person_id: fixture.person,
            device_id: "queue-fixture".into(),
            runtime_epoch: 1,
        },
        fixture.clone(),
        fixture.clone(),
        fixture.clone(),
        fixture.clone(),
        fixture.clone(),
        Arc::new(SystemKnowledgeClock),
        LearnerBudget::default(),
    )
    .unwrap();
    (learner, fixture)
}

#[tokio::test]
async fn existing_queue_is_consulted_before_fallible_discovery() {
    let (learner, fixture) = service(false, true, false);
    assert_eq!(
        learner.run_next(Cancellation::new()).await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(*fixture.calls.lock().unwrap(), vec!["claim"]);
}

#[tokio::test]
async fn partial_comparison_context_does_not_enqueue_new_memory_candidates() {
    let (learner, fixture) = service(true, false, false);
    assert_eq!(learner.run_next(Cancellation::new()).await, Ok(false));
    assert_eq!(
        *fixture.calls.lock().unwrap(),
        vec!["claim", "discovery", "context", "claim"]
    );
}

#[tokio::test]
async fn complete_comparison_context_discovers_and_then_checks_new_jobs() {
    let (learner, fixture) = service(false, false, false);
    assert_eq!(learner.run_next(Cancellation::new()).await, Ok(false));
    assert_eq!(
        *fixture.calls.lock().unwrap(),
        vec!["claim", "discovery", "context", "enqueue", "claim"]
    );
}

#[tokio::test]
async fn oversized_evidence_reports_deferral_but_allows_later_bounded_discovery() {
    let (learner, fixture) = service(false, false, true);
    assert_eq!(
        learner.run_next(Cancellation::new()).await,
        Err(AgentFailure::BudgetExceeded)
    );
    assert_eq!(
        *fixture.calls.lock().unwrap(),
        vec!["claim", "discovery", "context", "enqueue"]
    );
}
