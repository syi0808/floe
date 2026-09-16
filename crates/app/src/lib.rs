//! Application composition: service creation, host lifetime, the caller-bound
//! request path and dependency injection.
//!
//! Business judgment belongs to the owning module; this crate only assembles.

mod action_facade;
mod agent_fixture;
pub mod agent_run;
mod api;
mod bootstrap;
mod calendar_facade;
#[cfg(unix)]
mod composition;
mod core;
mod diagnostics;
mod error;
pub mod events;
mod host;
mod inference_routes;
mod local_context;
mod prompts;
mod services;
#[cfg(unix)]
mod vault_host;

/// The values this host's own API names at its boundary.
///
/// A binding reads a receipt and reports a failure; it does not reach past the
/// app into the modules behind it, so only the values these signatures carry
/// are named here — never a module, and never a concrete adapter.
pub use floe_conversation::{RunReceipt, RunState};
pub use floe_diagnostics::{PanicRecord, TraceContext, instrument, panic_record};
pub use floe_kernel::{AgentFailure, CommandId, PersonId, RunId};

pub use action_facade::CalendarActionCommand;
pub use api::{CallerContext, HostError, HostServices, LocalIdentityClaim, LocalIdentityProvider};
pub use bootstrap::local_identity_for_database;
#[cfg(unix)]
pub use composition::{AppComposition, AppOpenError, open};
pub use agent_fixture::{
    AgentFixturePrompt, AgentFixtureResult, AgentFixtureTurn, recover_agent_sample,
    run_persisted_agent_sample,
};
pub use core::{Classification, FloeCore};
pub use services::CalendarActionsResult;
pub use error::{CoreError, ErrorCode};
pub use host::{AppHost, HostRequest};
pub use inference_routes::HostInferenceRoutes;
pub use services::{
    CancelRun, CancelRunOutcome, CancelRunReceipt, CommandReceipt, ContinuationRef,
    ConversationCommands, ProfileSelection, ServiceError, StartTurn, TurnMode,
};
#[cfg(unix)]
pub use vault_host::{ConversationQuery, VaultBridge};
