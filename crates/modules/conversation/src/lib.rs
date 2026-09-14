//! Conversation-owned Session admission, root Run execution, and finalization.

mod api;
mod application;
mod domain;
mod ports;

pub use api::{ConversationPorts, ManagerConfig, TurnRequest};
pub use application::ConversationService;
pub use domain::{
    AdmittedTurn, RunReceipt, RunState, RunTerminal, TurnAdmission, TurnAdmissionRequest,
};
pub use floe_agent_runtime::FinalPayloadValidator;
pub use ports::ConversationRepository;
