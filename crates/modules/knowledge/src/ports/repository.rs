use std::future::Future;

use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;

use crate::{
    ContextMemory, KnowledgeActor, KnowledgeDecisionKind, KnowledgeDecisionResult,
    MemoryReviewSnapshot,
};

pub trait LearnerJobRepository: Send + Sync {
    fn discover_reviews(
        &self,
        now: DateTime<Utc>,
        limit: usize,
    ) -> impl Future<Output = Result<(), AgentFailure>> + Send;

    fn claim_review(
        &self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Option<crate::LearnerReviewJob>, AgentFailure>> + Send;

    fn settle_review(
        &self,
        job_id: uuid::Uuid,
        expected_attempt: u8,
        settlement: crate::LearnerJobSettlement,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<(), AgentFailure>> + Send;
}

pub trait MemoryContextReader: Send + Sync {
    fn read_memory_context(
        &self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<ContextMemory>, AgentFailure>> + Send;
}

/// The Person's memory candidates and the decisions they record on them.
///
/// Knowledge decides which candidate may be decided and by whom; the store
/// reads the candidates and commits one decision atomically.
pub trait MemoryReviewRepository: Send + Sync {
    fn memory_review_snapshot(
        &self,
    ) -> impl Future<Output = Result<MemoryReviewSnapshot, AgentFailure>> + Send;

    fn decide_memory_candidate(
        &self,
        candidate_id: uuid::Uuid,
        kind: KnowledgeDecisionKind,
        actor: KnowledgeActor,
        decided_at: DateTime<Utc>,
    ) -> impl Future<Output = Result<KnowledgeDecisionResult, AgentFailure>> + Send;
}
