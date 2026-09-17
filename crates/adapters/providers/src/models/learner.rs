//! What the bundled on-device model reports about itself.
//!
//! The Learner role, its prompt and the answer it accepts belong to Knowledge.
//! This adapter only says which profile the Foundation model is offered as and
//! whether it is available right now; the generation itself is the shared
//! model transport.

use floe_agent_contract::AgentFailure;
use floe_agent_contract::ModelPlacement;
use floe_inference::{ModelTransport, ModelTransportRequest, ModelTransportResponse};

use crate::models::foundation::{FoundationModelRunner, LocalModelAvailability};

/// The on-device model, as a role-neutral transport.
pub struct FoundationLearnerTransport {
    model: FoundationModelRunner,
    profile_id: String,
}

impl FoundationLearnerTransport {
    pub fn new(profile_id: impl Into<String>) -> Self {
        Self {
            model: FoundationModelRunner::encrypted(),
            profile_id: profile_id.into(),
        }
    }

    pub fn availability(&self) -> Result<LocalModelAvailability, AgentFailure> {
        self.model.availability()
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
}

impl ModelTransport for FoundationLearnerTransport {
    fn placement(&self) -> ModelPlacement {
        self.model.placement()
    }

    async fn generate(
        &self,
        request: ModelTransportRequest,
    ) -> Result<ModelTransportResponse, AgentFailure> {
        self.model.generate(request).await
    }
}
