//! Application composition: service creation, host lifetime, the caller-bound
//! request path and dependency injection.
//!
//! Business judgment belongs to the owning module; this crate only assembles.

mod action_facade;
mod agent_fixture;
pub mod agent_run;
mod api;
mod bootstrap;
mod bridge;
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

/// The module values this host hands across its own boundary.
///
/// A binding talks to the app, not to the modules behind it: everything it
/// needs to name is re-exported here, so the ABI never depends on which module
/// owns a value today.
pub mod modules {
    pub use floe_actions as actions;
    pub use floe_agent_contract as agent_contract;
    pub use floe_diagnostics as diagnostics;
    pub use floe_kernel as kernel;
    pub use floe_conversation as conversation;
    pub use floe_day as day;
    pub use floe_knowledge as knowledge;
    pub use floe_provider_adapters::sources::native_calendar;
}

pub use api::{CallerContext, HostError, HostServices, LocalIdentityClaim, LocalIdentityProvider};
pub use bootstrap::local_identity_for_database;
pub use bridge::{
    BridgeResult, FloeHandle, action_error, agent_failure, check_version, conversion_error,
    core_error, error,
    host_error, invalid, parse_date, parse_id, parse_person, parse_time, protocol_payload,
    unsupported_version,
};
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
