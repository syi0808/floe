//! Live preparation is distinct from generation, with the same route policy.
use crate::{LocalObservation, ModelObservationError, PrimaryObservation};
use floe_agent_contract::{
    AgentFailure, BoxFuture, ModelBindingDigest, ModelSelectionCommitment, OwnerActor,
    ProcessingBoundary,
};
use floe_execution::ExecutionScope;
use floe_model_contract::{LiveModelCapabilities, LiveTransportLimits};

pub const LIVE_PRESENTATION_PURPOSE: &str = "voice_presentation";
pub const LIVE_PRESENTATION_CONSUMER: &str = "floe.voice";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveModelPlanRequest {
    pub actor: OwnerActor,
    pub required_capabilities: LiveModelCapabilities,
}
impl LiveModelPlanRequest {
    /// Build the standard Voice presentation request using Inference's
    /// canonical capability contract. Composition roots select the route but
    /// do not need to depend on the neutral model wire types.
    pub fn conversational(actor: OwnerActor) -> Self {
        Self {
            actor,
            required_capabilities: LiveModelCapabilities::conversational(),
        }
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.actor.validate()?;
        self.required_capabilities
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedLiveModelCapability {
    pub purpose: String,
    pub consumer: String,
    pub capabilities: LiveModelCapabilities,
    pub limits: LiveTransportLimits,
    pub boundary: ProcessingBoundary,
    pub binding_digest: ModelBindingDigest,
    pub selection_commitment: ModelSelectionCommitment,
}

pub struct PreparedLiveModelProfile<T> {
    pub capability: ObservedLiveModelCapability,
    pub target: floe_access::ModelDispatchTarget,
    pub transport: T,
}

/// Observation prepares an owned transport capability; it does not open a call.
/// The future dispatch owner must consume an exact durable startup intent first.
pub trait LiveModelProvider: Send + Sync {
    type Transport: Send;
    fn observe_primary_live<'a>(
        &'a self,
        request: &'a LiveModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<
            PrimaryObservation<PreparedLiveModelProfile<Self::Transport>>,
            ModelObservationError,
        >,
    >;
    fn observe_local_live<'a>(
        &'a self,
        request: &'a LiveModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<LocalObservation<PreparedLiveModelProfile<Self::Transport>>, ModelObservationError>,
    >;
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent_contract::PersonId;

    #[test]
    fn conversational_request_owns_the_standard_voice_capability_set() {
        let request = LiveModelPlanRequest::conversational(OwnerActor {
            person_id: PersonId::new(),
            device_id: "fixture-device".into(),
            runtime_epoch: 1,
        });

        assert_eq!(
            request.required_capabilities,
            LiveModelCapabilities::conversational()
        );
        assert!(request.validate().is_ok());
    }
}
