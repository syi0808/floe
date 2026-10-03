//! Action ownership: proposal admission, approval, external dispatch and the
//! uncertain-result recovery path.
//!
//! The module knows nothing about SQL, keyrings or provider transports. Storage
//! and external authorities are reached through [`ports`].

mod application;
mod domain;
mod ports;

pub use domain::{ActionDigest, MAX_ACTION_BYTES, action_digest, action_uuid, CalendarDestination,
    CalendarTarget, CalendarEffect, ActionOrigin, ActionSourceFence, ActionReviewRef, ActionAuthorization,
    ActionBlockedReason, ActionUnknownReason, ActionNotAppliedReason, EffectIdentity, CalendarWriteResult,
    CommittedCalendarEffect, CalendarReceiptEvidence, CalendarEffectReceipt, NotAppliedProof,
    CalendarEffectOutcome, ExecutionIntent, ActionCollectionState, ActionState, CollectionTicket, ActionRecord,
    ActionIntent, ActionAllowedAction, ActionStatus, ActionCollectionStatus, ActionOriginKind, ActionSnapshot,
    ActionsPage, validate_action_admission, decide_action, stop_action, prepare_action_dispatch, settle_action,
    acknowledge_action_collection, invalidate_action_dependency, invalidate_action_policy, change_action_authority, decision_intent_digest};
pub use ports::{ActionStoreError, ActionsAuthority, AuthorityChange, ActionPage, RecoveryPage, ActionAdmission,
    AdmittedAction, ActionDecisionKind, ActionDecision, ActionReconciliation, DispatchIntent, DispatchAdmission, ExecutionSettlement,
    CollectionAck, PreDispatchStop, PreDispatchState, ActionsRepository, ActionSourceReader, ExpertProposalEvidence,
    ExpertProposalReader, ActionsClock, SystemActionsClock, ActionCalendarExecutor, PreparedCalendarEffect};

pub use application::{
    ActionsService, ActionsDependencies,
    ActionService, ExpertActionService, ExpertCalendarDestination, ExpertCalendarInspection,
    ExpertCalendarRequest, ObservationFence,
};
pub use domain::{
    ActionAuthority, ActionAuthorityMode, ActionBlockReason, ActionFailure, AgentActionAdmission,
    AgentActionEnvelope, AgentActionOrigin, CalendarAction, CalendarActionPolicy,
    CalendarActionState, CalendarCreateReceipt, CalendarMutation, CalendarPreflight,
    EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE, ExpertCalendarProposal, ExpertCalendarProposalDraft,
    ExpertProposalReference, MAX_AGENT_ACTION_BYTES, action_policy_mode_name, action_state_name,
    valid_action_digest,
};
pub use ports::{
    ActionError, ActionErrorCode, ActionRepository, CalendarActionProvider, CalendarSourceReader,
    ExpertActionStore,
};
