//! Action ownership: proposal admission, approval, external dispatch and the
//! uncertain-result recovery path.
//!
//! The module knows nothing about SQL, keyrings or provider transports. Storage
//! and external authorities are reached through [`ports`].

mod application;
mod domain;
mod ports;

pub use application::{
    ActionService, ExpertActionService, ExpertCalendarDestination, ExpertCalendarInspection,
    ExpertCalendarRequest, ObservationFence, validate_calendar_source_handle,
};
pub use domain::{
    ActionAuthority, ActionAuthorityMode, ActionBlockReason, ActionFailure, AgentActionAdmission,
    AgentActionEnvelope, AgentActionOrigin, CalendarAction, CalendarActionPolicy,
    CalendarActionState, CalendarCreateReceipt, CalendarMutation, CalendarPreflight,
    ExpertProposalReference, MAX_AGENT_ACTION_BYTES, action_policy_mode_name, action_state_name,
    valid_action_digest,
};
pub use ports::{
    ActionError, ActionErrorCode, ActionRepository, CalendarActionProvider, ExpertActionStore,
};
