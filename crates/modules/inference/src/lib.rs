//! One purpose-bound Gateway-primary planner and source-admitted model dispatcher.

mod api;
mod application;
mod ports;

pub use api::{ModelConsumer, ModelPurpose};
pub use application::{InferenceAvailability, InferenceService};
pub use floe_agent_contract::{
    ModelBindingDigest, ModelCapabilities, ModelCapability, ModelPlanRequest, PreparedModelPlan,
    ProcessingBoundary,
};
pub use ports::model_provider::{
    AdmittedDispatchTarget, CanonicalModelRequest, CanonicalModelResponse, ModelProvider,
    ObservedModelCapability, PreparedModelProfile, PreparedModelTransport,
    ProviderUsageObservation,
};

pub use ports::selection::{
    LocalAvailabilityReason, LocalObservation, ModelObservationError, PrimaryAbsence,
    PrimaryObservation,
};

pub use application::PreparedLiveSelection;
pub use floe_model_contract::LiveModelPlan;
pub use ports::live_provider::{
    LIVE_PRESENTATION_CONSUMER, LIVE_PRESENTATION_PURPOSE, LiveModelPlanRequest, LiveModelProvider,
    ObservedLiveModelCapability, PreparedLiveModelProfile,
};

pub use ports::live_transport::{
    CanonicalLiveStart, LiveDispatchCustody, LiveDispatchIntent, LiveSessionControl,
    LiveStartRequest, PreparedLiveTransport,
};

pub use application::AuthorizedLiveSession;
