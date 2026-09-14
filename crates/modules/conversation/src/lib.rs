//! Conversation-owned Session admission, root Run execution, and finalization.

mod api;
mod application;
mod domain;
mod ports;

pub use api::{
    ConversationPorts, FINALIZATION_OUTPUT_CONTRACT, FINALIZATION_ROLE_PROMPT, ManagerConfig,
    TurnRequest,
};
pub use application::{ConversationService, continuation, project_continuation, recover_session};
pub use domain::{
    AdmittedTurn, ContinuationRef, ContinuationSnapshot, JournalEntry, RecoveryReceipt,
    RecoveryRequest, RunReceipt, RunState, RunTerminal, TurnAdmission, TurnAdmissionRequest,
    TurnMode,
};
pub use floe_agent_runtime::FinalPayloadValidator;
pub use ports::ConversationRepository;
