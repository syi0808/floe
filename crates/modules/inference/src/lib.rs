//! Typed ownership of model profiles and inference routing.
//!
//! This crate deliberately knows nothing about connector catalogs or source
//! providers. A route describes where computation runs and, independently,
//! who may receive the input data.

mod api;
mod application;
mod ports;
mod transfer;

pub use api::{
    DataRecipient, ExecutionLocation, InferenceExecutionConstraint, ModelCapabilities,
    ModelConsumer, ModelProfile, ModelPurpose, PlannedRoute, RecipientConstraint, RouteRequest,
};
pub use application::{
    AttemptJournal, AttemptLifecycle, AttemptUpdate, CANONICAL_MODEL_CONSUMER,
    CANONICAL_MODEL_PURPOSE, EVERYDAY_ASSISTANCE_PURPOSE, InferenceAvailability, InferenceExecutor,
    InferenceRouter, InferenceService, ModelAttemptRecord, ModelAttemptState,
    RemoteModelConnection, RoutePlanError,
    SavedConnectionStore, SavedServerConnection, UsageLedger, admit_saved_connection,
};
pub use ports::model_provider::{
    AdmittedDispatchTarget, CanonicalModelRequest, CanonicalModelResponse, ModelProvider, PreparedModelProfile,
    PreparedModelTransport,
};
pub use transfer::{RouteRecipient, external_transfer_consent};
