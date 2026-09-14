//! Conversation-owned Session admission, root Run execution, and finalization.

mod adapters;
mod api;
mod application;
mod domain;
mod ports;

pub use api::{
    ConversationPorts, FINALIZATION_OUTPUT_CONTRACT, FINALIZATION_ROLE_PROMPT, ManagerConfig,
    TurnRequest,
};
pub use application::{
    ConversationService, compact_session, continuation, get_session, project_continuation,
    read_archive, recover_session, resume_session, start_session,
};
pub use domain::{
    AdmittedTurn, CompactionReceipt, CompactionRequest, ContinuationRef, ContinuationSnapshot,
    JournalEntry, MAX_COMPACTION_SUMMARY_BYTES, RecoveryReceipt, RecoveryRequest, RunReceipt,
    RunState, RunTerminal, SessionReadRequest, SessionReceipt, SessionRequest, TurnAdmission,
    TurnAdmissionRequest, TurnMode,
};
pub use floe_agent_runtime::FinalPayloadValidator;
pub use ports::{ConversationRepository, SessionArchiveRepository, SessionRepository};
