use chrono::Utc;
use floe_execution::Cancellation;
use floe_kernel::AgentFailure;

use crate::{
    LearnerBudget, LearnerJobRepository, LearnerModel, LearnerReviewJob, LearnerRuntime,
    MemoryCandidateSink, settlement_for_learner_result,
};

const DISCOVERY_LIMIT: usize = 8;

pub struct LearnerService<'service, Model, Repository> {
    pub model: &'service Model,
    pub repository: &'service Repository,
}

impl<Model, Repository> LearnerService<'_, Model, Repository>
where
    Model: LearnerModel + Sync,
    Repository: LearnerJobRepository + MemoryCandidateSink,
{
    pub async fn run_next(&self, cancellation: Cancellation) -> Result<bool, AgentFailure> {
        if cancellation.is_cancelled() {
            return Ok(false);
        }
        self.repository
            .discover_reviews(Utc::now(), DISCOVERY_LIMIT)
            .await?;
        if cancellation.is_cancelled() {
            return Ok(false);
        }
        let Some(job) = self.repository.claim_review(Utc::now()).await? else {
            return Ok(false);
        };
        self.review_claimed(&job, cancellation).await
    }

    pub async fn review_claimed(
        &self,
        job: &LearnerReviewJob,
        cancellation: Cancellation,
    ) -> Result<bool, AgentFailure> {
        let result = LearnerRuntime {
            model: self.model,
            candidates: self.repository,
            budget: LearnerBudget::default(),
            extractor_version: crate::prompts::LEARNER_EXTRACTOR_VERSION,
            prompt_version: crate::prompts::LEARNER_PROMPT_VERSION,
        }
        .review(job.input.clone(), cancellation)
        .await;
        let settled_at = Utc::now();
        let settlement = settlement_for_learner_result(
            result.map(|candidate| candidate.map(|candidate| candidate.id)),
            settled_at,
        )?;
        self.repository
            .settle_review(job.id, job.attempts, settlement, settled_at)
            .await?;
        Ok(true)
    }
}
