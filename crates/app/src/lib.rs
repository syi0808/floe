//! Application composition: service creation, host lifetime, the caller-bound
//! request path and dependency injection.
//!
//! Business judgment belongs to the owning module; this crate only assembles.

mod agent_run;
mod api;
mod bootstrap;
mod calendar_facade;
#[cfg(unix)]
mod composition;
mod core;
mod events;
mod host;
mod inference_routes;
mod local_context;
mod services;
#[cfg(unix)]
mod vault_host;

pub use api::{CallerContext, HostError, HostServices, LocalIdentityClaim, LocalIdentityProvider};
pub use bootstrap::local_identity_for_database;
#[cfg(unix)]
pub use composition::{AppComposition, AppOpenError, open};
pub use core::{Classification, FloeCore};
pub use host::{AppHost, HostRequest};
pub use inference_routes::HostInferenceRoutes;
pub use services::{
    CancelRun, CancelRunOutcome, CancelRunReceipt, CommandReceipt, ContinuationRef,
    ConversationCommands, ProfileSelection, ServiceError, StartTurn, TurnMode,
};
#[cfg(unix)]
pub use vault_host::{ConversationQuery, VaultBridge};
