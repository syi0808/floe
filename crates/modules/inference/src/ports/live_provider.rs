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
