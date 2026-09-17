//! Stable, role-neutral agent values and ports.
//!
//! This crate contains no session, task, provider, or product-domain owner.
//! Owners admit a request and supply an [`ExecutionJournal`] to the runtime.

mod archive;
mod capability;
mod context;
mod delegation;
mod endpoint;
mod envelope;
mod expert;
mod expert_model;
mod history;
mod message;
mod model;
mod ports;
pub mod prompts;
mod replay;
mod timeline_view;

pub use archive::{
    ArchivePointer, ArchiveReadRequest, ArchiveReader, ArchiveSnapshot, ArchivedMessage,
    MAX_ARCHIVE_PROJECTION_BYTES, MAX_ARCHIVE_PROJECTION_MESSAGES,
};
pub use capability::{
    CapabilityExecution, CapabilityExecutionState, CapabilityJournal, ModelReplay, ProviderReplay,
};
pub use context::{AgentContext, InferencePolicyDecision, MAX_CONTEXT_ISSUES};
pub use delegation::{DelegationRequest, TaskReceipt, TaskSnapshot, TaskState};
pub use endpoint::{
    AgentEndpoint, EndpointInvocation, EndpointSettlement, ExpertReport,
    MAX_ENDPOINT_SETTLEMENT_BYTES,
};
pub use envelope::{
    AgentCardManifestEntry, ContextEnvelope, ContextManifest, ContextualData, ConversationContext,
    EvidenceManifestEntry, MemoryManifestEntry, PromptManifestEntry, RuntimeContext,
    ScopedInstructions,
};
pub use expert::{
    AdmittedExpert, ExpertAssignments, ExpertBudget, ExpertFocusProposal, ExpertInput,
    ExpertInsight, ExpertInvocation, ExpertResult, MAX_EXPERT_VIEW_BYTES, PackageKind, PackageRef,
    ViewCancellation, check_running,
};
pub use expert_model::{
    CapabilityDescriptor, ExpertModel, ExpertModelAnswer, ExpertModelCall, ExpertReasoner,
    ExpertReasoningStep, ExpertStep, ExpertStepOutcome, ExpertTranscriptEntry,
    SourceHistoryBoundary,
};
pub use floe_context_contract::{
    CalendarProvider, CalendarReadAccessStamp, CalendarScope, ContextDependency, ContextEvidence,
    ContextIssue, ContextIssueReason, ContextMemory, ContextSource, DataClass, ExpertTimelineView,
    MAX_TIMELINE_VIEW_BYTES, MAX_TIMELINE_VIEW_DAYS, MAX_TIMELINE_VIEW_ITEMS, TimelineViewItem,
    DependencyCoverage, EpistemicStatus, LearningEvidenceRef, MAX_CONTEXT_EVIDENCE,
    MAX_CONTEXT_EVIDENCE_BYTES, MAX_CONTEXT_MEMORIES, MAX_CONTEXT_MEMORY_BYTES,
    MemoryContextSnapshot, ModelPlacement, PersonalMemoryKind, SourceAuthority, SourceGrant,
    TransferConsent,
};
pub use floe_execution::{CancelReason, Cancellation, ExecutionScope};
pub use floe_kernel::{
    AGENT_VERSION, AgentFailure, AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction,
    AgentRetryPolicy, CommandId, PersonId, RunId, ScopeId, TaskId, TraceContext,
};
pub use history::{HistoryMessageSize, bounded_history_start};
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
pub use timeline_view::TimelineViewRead;

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
