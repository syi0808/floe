//! One purpose-bound Gateway-primary planner and source-admitted model dispatcher.

mod api;
mod application;
mod ports;

pub use api::{ModelConsumer, ModelPurpose};
pub use application::{InferenceAvailability, InferenceService};
pub use floe_agent_contract::{
    ModelBindingDigest, ModelCapabilities, ModelCapability, ModelPlanRequest,
    PreparedModelPlan, ProcessingBoundary,
};
pub use ports::model_provider::{
    AdmittedDispatchTarget, CanonicalModelRequest, CanonicalModelResponse, LocalAvailabilityReason,
    LocalObservation, ModelObservationError, ModelProvider, ObservedModelCapability,
    PreparedModelProfile, PreparedModelTransport, PrimaryAbsence, PrimaryObservation,
    ProviderUsageObservation,
};
