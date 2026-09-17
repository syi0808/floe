use floe_agent_contract::AgentFailure;
use floe_execution::Cancellation;
use floe_knowledge::{
    FOUNDATION_LEARNER_PROFILE, InferenceLearnerModel, LearnerModelAvailability, LearnerService,
    TransportLearnerModel,
};
use floe_provider_adapters::models::learner::FoundationLearnerTransport;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};

pub(super) async fn run<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    cancellation: Cancellation,
) -> Result<bool, AgentFailure> {
    let model = InferenceLearnerModel::new(TransportLearnerModel::new(BundledLearnerModel(
        FoundationLearnerTransport::new(FOUNDATION_LEARNER_PROFILE),
    )));
    LearnerService {
        model: &model,
        repository: vault,
    }
    .run_next(cancellation)
    .await
}

/// The bundled on-device model, bound to the Learner role.
///
/// Knowledge owns the role and the prompt; the adapter owns the model. Tying
/// the two together is composition, so it happens here.
struct BundledLearnerModel(FoundationLearnerTransport);

impl floe_inference::ModelTransport for BundledLearnerModel {
    fn placement(&self) -> floe_agent_contract::ModelPlacement {
        self.0.placement()
    }

    async fn generate(
        &self,
        request: floe_inference::ModelTransportRequest,
    ) -> Result<floe_inference::ModelTransportResponse, AgentFailure> {
        self.0.generate(request).await
    }
}

impl LearnerModelAvailability for BundledLearnerModel {
    fn profile_id(&self) -> &str {
        self.0.profile_id()
    }

    fn is_available(&self) -> Result<bool, AgentFailure> {
        Ok(self.0.availability()?
            == floe_provider_adapters::models::LocalModelAvailability::Available)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_knowledge::retryable_learner_failure;

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
