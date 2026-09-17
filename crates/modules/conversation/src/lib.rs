//! Conversation-owned Session admission, root Run execution, and finalization.

pub mod adapters;
mod api;
pub mod prompts;
pub mod turn;
mod application;
mod domain;
mod ports;

pub use adapters::model_transport::{TransportModelRunner, transport_request};
pub use api::{
    ConversationPorts, FINALIZATION_OUTPUT_CONTRACT, FINALIZATION_ROLE_PROMPT, ManagerConfig,
    TurnRequest,
};
pub use application::{
    CancelCommandRequest, CancelRunAdmission, admitted_session, CancelRunCommand, CancelRunReceipt, CancelRunRequest,
    CancelRunStatus, ConversationService, GovernedSessionRepository, GovernedSessionStore,
    HistoryProjection, PreparedTurn, RunCancellationRegistry, TurnPrecheck,
    TurnPrecheckRequest, TurnPreparationRequest, cancel_run_command, prepare_turn, compact_session, continuation, get_command, get_run,
    get_session, narrow_by_source_boundary, precheck_turn, project_continuation,
    project_history_into, read_archive, recover_session, resume_session, start_session,
};
pub use domain::{
    AdmittedExecution, AdmittedTurn, CommandQuery, CompactionReceipt, CompactionRequest,
    ContinuationRef, ContinuationSnapshot, JournalEntry, MAX_COMPACTION_SUMMARY_BYTES,
    MAX_TURN_TEXT_BYTES, ProfileSelection, RecoveryReceipt, RecoveryRequest, RunQuery, RunReceipt,
    RunState, RunTerminal, SessionReadRequest, SessionReceipt, SessionRequest, StartTurn,
    TurnAdmission, TurnAdmissionRequest, TurnMode,
};
pub use domain::{CanonicalTurnIntent, normalize_turn_text};
pub use floe_agent_contract::{
    ArchivePointer, ArchiveReadRequest, ArchiveSnapshot, ArchivedMessage,
};
pub use floe_agent_runtime::FinalPayloadValidator;
pub use ports::{ConversationRepository, SessionArchiveRepository, SessionRepository};

pub use turn::*;
