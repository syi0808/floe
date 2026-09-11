mod action_authority;
mod agent_action;
#[cfg(unix)]
mod agent_calendar;
mod agent_fixture;
#[cfg(unix)]
mod agent_vault;
mod calendar;
mod calendar_action;
mod calendar_view;
mod connected_calendar;
mod core;
mod error;
mod native_context;
pub mod ports;
mod store;

pub use action_authority::{ActionAuthority, ActionAuthorityMode};
pub use agent_action::{
    AgentActionOrigin, ExpertCalendarDestination, ExpertCalendarInspection, ExpertCalendarRequest,
    ExpertProposalReference,
};
#[cfg(unix)]
pub use agent_calendar::{
    CalendarAgentProposal, CalendarAgentTurnRequest, CalendarAgentTurnResult,
};
pub use agent_fixture::{
    AgentFixturePrompt, AgentFixtureResult, AgentFixtureTurn, recover_agent_sample,
};
#[cfg(unix)]
pub use agent_vault::{
    EncryptedAgentVault, KeyringVaultKeys, SessionCompactionResult, SessionSearchHit, VaultKey,
    VaultKeyProvider,
};
pub use calendar_action::{
    ActionBlockReason, ActionFailure, CalendarAction, CalendarActionPolicy, CalendarActionProvider,
    CalendarActionState, CalendarCreateReceipt, CalendarMutation, CalendarPreflight,
};
pub use calendar_view::{
    CalendarObservation, CalendarObserveRequest, CalendarReadAccess, CalendarReadAccessRequest,
    CalendarReadAccessStamp, CalendarTimelineGrant, CalendarTimelineViews, ProjectedCalendarItem,
    ProjectedCalendarObservation,
};
pub use core::{Classification, FloeCore};
pub use error::{CoreError, ErrorCode};
pub(crate) use store::TursoStore;
