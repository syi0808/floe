//! Stable, role-neutral agent values and ports.
//!
//! This crate contains no session, task, provider, or product-domain owner.
//! Owners admit a request and supply an [`ExecutionJournal`] to the runtime.

mod delegation;
mod message;
mod model;
mod ports;
mod replay;

pub use delegation::{DelegationRequest, TaskReceipt, TaskSnapshot, TaskState};
pub use floe_context_contract::{
    ContextIssue, ContextIssueReason, ContextSource, DataClass, DependencyCoverage, ModelPlacement,
    TransferConsent,
};
pub use floe_execution::{CancelReason, Cancellation, ExecutionScope};
pub use floe_kernel::{
    AgentFailure, AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction,
    AgentRetryPolicy, CommandId, RunId, ScopeId, TaskId, TraceContext,
};
pub use message::{
    AgentCard, AgentMessage, Artifact, ArtifactPart, MessageRole, OutcomeIssue, ToolResult,
};
pub use model::{
    AgentDefinition, AllowedCatalog, BoundedContext, EngineRequest, EngineStep, ModelRequest,
    ModelResponse, ModelStep, ModelUsage, RoleSpec, ToolCall, ToolDescriptor, validate_tool_input,
};
pub use ports::{
    BoxFuture, DelegationPort, ExecutionJournal, JournalAck, JournalEvent, ModelPort, ToolPort,
};
pub use replay::{AttemptId, InvocationKey, ReplayReceipt, input_digest};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionProtection {
    SyntheticOnly,
    Encrypted,
    KeyUnavailable,
}

pub const AGENT_SCHEMA_VERSION: u32 = 1;
pub const A2A_PROTOCOL_VERSION: &str = "1.0";
pub const MAX_AGENT_MESSAGES: usize = 128;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
