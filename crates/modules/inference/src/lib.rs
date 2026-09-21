//! Typed ownership of model profiles and inference routing.
//!
//! This crate deliberately knows nothing about connector catalogs or source
//! providers. A route describes where computation runs and, independently,
//! who may receive the input data.

mod api;
mod application;
pub mod ports;
mod transfer;

pub use api::{
    DataRecipient, ExecutionLocation, ModelCapabilities, ModelConsumer, ModelProfile, ModelPurpose,
    PlannedRoute, RecipientConstraint, RouteRequest,
};
pub use application::{
    AttemptJournal, AttemptLifecycle, AttemptUpdate, CANONICAL_MODEL_CONSUMER,
    CANONICAL_MODEL_PURPOSE, EVERYDAY_ASSISTANCE_PURPOSE, InferenceRouter, InferenceService,
    LEGACY_INFERENCE_CONSUMER, MODEL_GENERATION_CAPABILITY, ModelAttemptRecord, ModelAttemptState,
    ModelRouteConfig, PurposeAvailability, RemoteModelConnection, RemoteRoute, RoutePairing,
    RoutePlanError, SavedConnectionStore, SavedServerConnection, UsageLedger,
    admit_saved_connection, candidate_route, valid_external_recipient,
};
pub use ports::model_provider::{
    CanonicalModelRequest, CanonicalModelResponse, ModelProvider, PreparedModelProfile,
    PreparedModelTransport,
};
pub use ports::model_transport::{
    ModelStep, ModelTransport, ModelTransportRequest, ModelTransportResponse,
};
pub use transfer::{RouteRecipient, external_transfer_consent};
