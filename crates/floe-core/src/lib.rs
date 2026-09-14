mod action_authority;
mod agent_action;
#[cfg(unix)]
mod agent_calendar;
mod agent_fixture;
#[cfg(unix)]
mod agent_vault;
mod context_evidence;
mod context_history;
pub use context_history::bounded_model_history_start;
mod calendar;
mod calendar_action;
mod calendar_lease;
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
    CalendarExpertEndpointRequest, CalendarExpertEndpointResult,
};
pub use agent_fixture::{
    AgentFixturePrompt, AgentFixtureResult, AgentFixtureTurn, recover_agent_sample,
};
#[cfg(unix)]
pub use agent_vault::{
    AccessGrantCleanup, AgentActionAdmission, AgentActionEnvelope, CalendarGrantAdmission,
    EncryptedAgentVault, FeasibilityGrantQuery, GovernedAgentSessionStore,
    GovernedDependencyLiveness, GovernedDependencyResolver, KeyringVaultKeys,
    RemoteCalendarAuthorizationExpectation, RemoteCalendarGrantBinding,
    RemoteCalendarSourceReference, RemoteEnrollmentSignature, RemoteOwnerPublicKey,
    RemotePairingChallenge, RemoteProducerIdentity, RemoteViewGrantBinding,
    RemoteViewSourceReference, SessionCompactionResult, SessionSearchHit, VaultKey,
    VaultKeyProvider, VaultTaskActivation, VaultTaskAdmission, VaultTaskRecord,
};
pub use calendar_action::{
    ActionBlockReason, ActionFailure, CalendarAction, CalendarActionPolicy, CalendarActionProvider,
    CalendarActionState, CalendarCreateReceipt, CalendarMutation, CalendarPreflight,
};
pub use calendar_view::{
    CalendarObservation, CalendarObserveRequest, CalendarReadAccess, CalendarReadAccessAdmission,
    CalendarReadAccessRequest, CalendarReadAccessStamp, CalendarTimelineGrant,
    CalendarTimelineViews, ProjectedCalendarItem, ProjectedCalendarObservation,
};
pub use core::{Classification, FloeCore};
pub use error::{CoreError, ErrorCode};
pub(crate) use store::TursoStore;
