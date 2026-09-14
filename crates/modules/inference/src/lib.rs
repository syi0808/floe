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
    AttemptJournal, AttemptLifecycle, InferenceRouter, ModelAttemptRecord, ModelAttemptState,
    RoutePlanError,
};
