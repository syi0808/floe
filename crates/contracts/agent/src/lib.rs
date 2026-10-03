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
mod interaction;
mod message;
mod model;
mod model_conversation;
mod model_plan;
mod ports;
mod projection;
pub mod prompts;
mod replay;
mod task_execution;

pub use archive::{
    ArchivePointer, ArchiveReadRequest, ArchiveReader, ArchiveSnapshot, ArchivedMessage,
    MAX_ARCHIVE_PROJECTION_BYTES, MAX_ARCHIVE_PROJECTION_MESSAGES,
};
pub use capability::{
    CapabilityExecution, CapabilityExecutionState, CapabilityJournal, ModelReplay, ProviderReplay,
};
pub use context::{AgentContext, InferencePolicyDecision, MAX_CONTEXT_ISSUES};
pub use delegation::{
    DelegationContextInput, DelegationExecutionContext, DelegationRequest, MAX_DELEGATION_DEVICE_ID_BYTES,
    MAX_DELEGATION_EXECUTION_CONTEXT_BYTES, TaskReceipt, TaskSnapshot, TaskState,
    delegation_request_digest, valid_context_refs,
};
pub use endpoint::{
    AgentEndpoint, EndpointInvocation, EndpointResources, EndpointSettlement, ExpertBlockReport, ExpertExecutionOutcome, ExpertReport,
    MAX_ENDPOINT_SETTLEMENT_BYTES,
};
pub use envelope::{
    AgentCardManifestEntry, AttemptContext, CONTEXT_ENVELOPE_SCHEMA_VERSION, ContextEnvelope,
    ContextManifest, ContextualData, DiscoveryContext, EvidenceManifestEntry,
    ExpertEnvironmentManifestEntry, MAX_RESPONSE_CONTRACT_BYTES, MAX_SCOPED_PURPOSE_BYTES,
    MemoryManifestEntry, PromptManifestEntry, RunInstructions, content_sha256,
};
pub use expert::{PackageKind, PackageRef};
pub use expert_model::{
    CapabilityDescriptor, ExpertCapabilityObservation, ExpertModel, ExpertModelAnswer,
    ExpertModelCall, ExpertModelOutcome, ExpertReasoner, ExpertReasoningStep, ExpertStep,
    ExpertStepOutcome, ExpertStepResult, ExpertTranscriptEntry,
};
pub use floe_context_contract::{
    CalendarProvider, CalendarReadAccessStamp, CalendarScope, ContextDependency, ContextEvidence,
    ContextIssue, ContextIssueReason, ContextMemory, ContextSource, DataClass, DependencyCoverage,
    EpistemicStatus, ExpertTimelineView, LearningEvidenceRef, MAX_CONTEXT_EVIDENCE,
    MAX_CONTEXT_EVIDENCE_BYTES, MAX_CONTEXT_MEMORIES, MAX_CONTEXT_MEMORY_BYTES,
    MAX_TIMELINE_VIEW_BYTES, MAX_TIMELINE_VIEW_DAYS, MAX_TIMELINE_VIEW_ITEMS,
    MemoryContextSnapshot, PersonalMemoryKind, SourceAccessBlockers, SourceAuthority, SourceGrant, TimelineViewItem,
};
pub use floe_execution::budget::ModelAccounting;
pub use floe_execution::{BoxFuture, CancelReason, Cancellation, ExecutionScope};
pub use floe_kernel::{
    AGENT_VERSION, AgentFailure, AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction,
    AgentRetryPolicy, CommandId, OwnerActor, PersonId, RunId, ScopeId, TaskId, TraceContext,
};
pub use history::{HistoryMessageSize, bounded_history_start};
pub use interaction::{
    USER_INTERACTION_MEDIA_TYPE, UserInteractionKind, UserInteractionRef, UserInteractionStatus,
};
pub use message::{
    AgentCard, AgentMessage, Artifact, ArtifactPart, MessageRole, OutcomeIssue, ToolResult,
};
pub use model::{
    AgentDefinition, AllowedCatalog, EngineRequest, EngineResumeState, EngineStep, ModelRequest,
    ModelResponse, ModelStep, ModelUsage, RoleSpec, ToolCall, ToolDescriptor, ValidatedFinalPayload, validate_tool_input,
};
pub use model_conversation::{
    MAX_CONTEXT_REFS, MAX_MODEL_CONVERSATION_BYTES, ModelConversation, ModelConversationEntry,
};
pub use ports::{
    BatchCursor, DelegationPort, ExecutionJournal, JournalAck, JournalEntry, JournalEvent, ModelPort,
    ModelProjectionPort, PinnedAgentRevision, PinnedToolRevision, PreparedModelCall, ToolPort,
    ToolInvocationOutcome, ValidatedModelBatch,
};
pub use projection::{
    AuthorizedModelProjection, MAX_CORRECTION_BYTES, MAX_INPUT_DATA_CLASSES, MODEL_CORRECTION_TEXT,
    ModelCorrection, ModelProjectionOutcome, ModelProjectionRequest, ProjectionRef,
    SourceProjectionReview,
};
pub use replay::{AttemptId, InvocationKey, ReplayReceipt, input_digest};
pub use task_execution::{TaskBlockage, TaskExecutionEvidence, TaskExecutionKey,
    TaskExecutionReceipt, TaskExecutionReceiptRef, TaskModelAccounting, UnresolvedModelAttempt};

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

pub use model_plan::{
    ModelBindingDigest, ModelCapabilities, ModelCapability, ModelPlanRequest, PreparedModelPlan,
    ProcessingBoundary,
};
