use chrono::Utc;
use floe_agent::{AgentFailure, Cancellation};
use floe_core::{EncryptedAgentVault, VaultKeyProvider};
use floe_infra::learner_model::FoundationLearnerTransport;
use floe_knowledge::{
    InferenceLearnerModel, LearnerBudget, LearnerModel, LearnerReviewJob, LearnerRuntime,
    settlement_for_learner_result,
};

const DISCOVERY_LIMIT: usize = 8;

pub(super) async fn run<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    cancellation: Cancellation,
) -> Result<bool, AgentFailure> {
    if cancellation.is_cancelled() {
        return Ok(false);
    }
    vault
        .discover_explicit_learner_reviews(Utc::now(), DISCOVERY_LIMIT)
        .await?;
    if cancellation.is_cancelled() {
        return Ok(false);
    }
    let Some(job) = vault.claim_learner_review(Utc::now()).await? else {
        return Ok(false);
    };
    let model = InferenceLearnerModel::new(FoundationLearnerTransport);
    review_claimed(vault, &job, &model, cancellation).await
}

pub(super) async fn review_claimed<Keys: VaultKeyProvider, Model: LearnerModel + Sync>(
    vault: &EncryptedAgentVault<Keys>,
    job: &LearnerReviewJob,
    model: &Model,
    cancellation: Cancellation,
) -> Result<bool, AgentFailure> {
    let runtime = LearnerRuntime {
        model,
        candidates: vault,
        budget: LearnerBudget::default(),
        extractor_version: floe_knowledge::prompts::LEARNER_EXTRACTOR_VERSION,
        prompt_version: floe_knowledge::prompts::LEARNER_PROMPT_VERSION,
    };
    let result = runtime.review(job.input.clone(), cancellation).await;
    let settled_at = Utc::now();
    let settlement = settlement_for_learner_result(
        result.map(|candidate| candidate.map(|candidate| candidate.id)),
        settled_at,
    )?;
    vault
        .settle_learner_review(job.id, job.attempts, settlement, settled_at)
        .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent::retryable_learner_failure;

    #[test]
    fn only_transient_background_failures_are_retried() {
        for failure in [
            AgentFailure::Cancelled,
            AgentFailure::DeadlineExceeded,
            AgentFailure::ModelUnavailable,
            AgentFailure::LocalModelUnavailable,
            AgentFailure::QuotaExceeded,
            AgentFailure::Interrupted,
        ] {
            assert!(retryable_learner_failure(failure));
        }
        for failure in [
            AgentFailure::InvalidModelOutput,
            AgentFailure::PolicyDenied,
            AgentFailure::BudgetExceeded,
            AgentFailure::StaleContext,
        ] {
            assert!(!retryable_learner_failure(failure));
        }
    }
}
