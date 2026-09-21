//! Conversation-owned Session admission, root Run execution, and finalization.

mod adapters;
mod api;
mod application;
mod domain;
mod ports;
pub mod prompts;
mod turn;

pub use adapters::model_transport::{TransportModelRunner, transport_request};
pub use api::{
    CONVERSATION_MODEL_CONSUMER, ConversationPorts, FINALIZATION_OUTPUT_CONTRACT,
    FINALIZATION_ROLE_ID, FINALIZATION_ROLE_PROMPT, MANAGER_OUTPUT_CONTRACT, ManagerConfig,
    TurnRequest,
};
pub use application::{
    CancelCommandRequest, CancelRunAdmission, CancelRunCommand, CancelRunReceipt, CancelRunRequest,
    CancelRunStatus, ConversationModelProjection, ConversationService, GovernedSessionRepository,
    GovernedSessionStore, HistoryProjection, PreparedTurn, ProjectedModelConversation,
    RunCancellationRegistry,
    TurnPrecheck, TurnPrecheckRequest, TurnPreparationRequest, admit_unscoped_session,
    admitted_session, cancel_run_command, compact_session, continuation, get_command, get_run,
    get_session, narrow_by_source_boundary, precheck_turn, prepare_turn, project_continuation,
    project_history_into, project_model_conversation_history, read_archive, recover_session,
    recovered_session, resume_session, start_session,
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

pub use turn::{
    AgentBudget, AgentCommand, AgentContinuation, AgentEvent, AgentEventKind, AgentMessage,
    AgentOutcome, AgentRuntime, AgentSession, AgentSessionScope, AgentUsage, CapabilityHost,
    CapabilityInvocation, DelegationExecution, DelegationExecutionState, ModelRequest,
    ModelResponse, ModelRunner, SessionRecoveryPointer, SessionStore,
    bounded_source_history_start, carries_source_history, generate_with_recovery,
};
