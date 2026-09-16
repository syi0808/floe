//! Typed ownership of model profiles and inference routing.
//!
//! This crate deliberately knows nothing about connector catalogs or source
//! providers. A route describes where computation runs and, independently,
//! who may receive the input data.

mod api;
mod application;

pub use api::{
    DataRecipient, ExecutionLocation, ModelCapabilities, ModelConsumer, ModelProfile, ModelPurpose,
    PlannedRoute, RecipientConstraint, RouteRequest,
};
pub use application::{
    AttemptJournal, AttemptLifecycle, AttemptUpdate, EVERYDAY_ASSISTANCE_PURPOSE, InferenceRouter,
    LEGACY_INFERENCE_CONSUMER, MODEL_GENERATION_CAPABILITY, ModelAttemptRecord, ModelAttemptState,
    ModelRouteConfig, PurposeAvailability, RemoteModelConnection, RemoteRoute, RemoteRouteResolver,
    RoutePairing, RoutePlanError, SavedConnectionStore, SavedServerConnection, UsageLedger,
    admit_saved_connection, candidate_route, plan_remote_route, select_remote_route,
    valid_external_recipient,
};
