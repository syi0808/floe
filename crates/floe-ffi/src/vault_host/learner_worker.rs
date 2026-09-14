use floe_agent::{AgentFailure, Cancellation};
use floe_core::{EncryptedAgentVault, VaultKeyProvider};
use floe_infra::learner_model::FoundationLearnerTransport;
use floe_knowledge::{InferenceLearnerModel, LearnerService};

pub(super) async fn run<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    cancellation: Cancellation,
) -> Result<bool, AgentFailure> {
    let model = InferenceLearnerModel::new(FoundationLearnerTransport);
    LearnerService {
        model: &model,
        repository: vault,
    }
    .run_next(cancellation)
    .await
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
