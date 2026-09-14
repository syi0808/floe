use std::future::Future;

use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;

use crate::ContextMemory;

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
