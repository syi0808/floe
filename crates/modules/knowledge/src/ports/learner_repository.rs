use chrono::{DateTime, Utc};
use floe_agent_contract::{
    AgentFailure, BoxFuture, ExecutionJournal, ExecutionScope, OwnerActor, PersonId,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum LearningTranscriptMessage {
    User {
        message_id: Uuid,
        turn_id: Uuid,
        text: String,
    },
    Assistant {
        turn_id: Uuid,
        original_user_message_id: Uuid,
        text: String,
    },
    Other,
}

#[derive(Clone, Debug)]
pub struct LearningSessionSnapshot {
    pub evidence: crate::LearningEvidenceSnapshot,
    pub messages: Vec<LearningTranscriptMessage>,
}

pub trait LearnerJobRepository: Send + Sync {
    fn discovery_sessions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        limit: usize,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<LearningSessionSnapshot>, AgentFailure>>;
    fn enqueue<'a>(
        &'a self,
        actor: &'a OwnerActor,
        input: crate::LearnerReviewInput,
        available_at: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::LearnerReviewJob, AgentFailure>>;
    fn claim_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        budget: crate::LearnerBudget,
        now: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<crate::LearnerReviewJob>, AgentFailure>>;
    fn settle_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        job_id: Uuid,
        expected_attempt: u8,
        settlement: crate::LearnerJobSettlement,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;
}

/// Claim-owned journal access; durable settlement remains available after execution cancellation.
pub trait LearnerJournalFactory: Send + Sync {
    fn journal(
        &self,
        person_id: PersonId,
        claim: crate::LearnerClaimRef,
    ) -> Result<std::sync::Arc<dyn ExecutionJournal>, AgentFailure>;
    fn load_journal<'a>(
        &'a self,
        person_id: PersonId,
        claim: crate::LearnerClaimRef,
    ) -> BoxFuture<'a, Result<crate::LearnerClaimJournal, AgentFailure>>;
}
