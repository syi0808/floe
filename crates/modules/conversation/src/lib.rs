//! Conversation-owned Session admission, root Run execution, and finalization.

mod adapters;
mod api;
mod application;
mod domain;
mod ports;
pub mod prompts;
mod turn;

pub use api::{
    ConversationPorts, FINALIZATION_OUTPUT_CONTRACT, FINALIZATION_ROLE_ID,
    FINALIZATION_ROLE_PROMPT, MANAGER_OUTPUT_CONTRACT, ManagerConfig, TurnRequest,
};
pub use application::{
    CancelCommandRequest, CancelRunAdmission, CancelRunCommand, CancelRunReceipt, CancelRunRequest,
    CancelRunStatus, ConversationModelProjection, ConversationService, DecideInteractionCommand,
    GovernedSessionRepository, GovernedSessionStore, HistoryProjection, JournalLineage,
    MAX_CONTINUATION_JOURNAL_ENTRIES, ModelSelectionJournalProjection, PreparedResume,
    ProjectedModelConversation, ResumeLineageFold, ResumePreparationRequest, ResumeSuppression,
    RunCancellationRegistry, admit_unscoped_session, admitted_session, cancel_run_command,
    continuation, decide_interaction, expire_interaction, fold_resume_lineage, get_command,
    get_run, get_session, list_run_interactions, load_interaction, prepare_resume,
    project_continuation, project_model_conversation_history, project_model_selection_journal,
    read_archive, reconcile_resume_lineage, reconcile_resume_selection, resolve_interaction,
    resume_gate, resume_session, start_session, supersede_interaction,
};
pub use domain::{
    AdmittedExecution, AdmittedTurn, BlockedInteractionLink, COMPACTION_MIN_RETAIN_ENTRIES,
    COMPACTION_PREFERRED_RETAIN_ENTRIES, COMPACTION_TRIGGER_BYTES, COMPACTION_TRIGGER_MESSAGES,
    CommandQuery, CompactionAuthority, CompactionReceipt, CompactionSource, ContinuationRef,
    ContinuationSnapshot, ContinuationToken, ConversationInteraction, DecisionAdmission,
    ExpireInteraction, ExpireOutcome, INTERACTION_PENDING_LIFETIME_MS, InteractionDecision,
    InteractionDecisionKind, InteractionListCursor, InteractionOrigin, InteractionPage,
    InteractionRefresh, InteractionRequirement, InteractionResolution, InteractionResolutionCause,
    InteractionResolutionReceipt, InteractionResumeRef, InteractionState, JournalEntry,
    MAX_ACTIVE_INTERACTIONS_PER_RUN, MAX_COMPACTION_SUMMARY_BYTES, MAX_INTERACTIONS_PER_PAGE,
    MAX_RESUME_LINEAGE, MAX_REVIEWED_IDENTIFIER_BYTES, MAX_REVIEWED_PURPOSE_BYTES,
    MAX_REVIEWED_SOURCE_BYTES, MAX_REVIEWED_TARGET_BYTES, MAX_STORED_INTERACTIONS_PER_RUN,
    MAX_TARGET_BUNDLE_MEMBERS, MAX_TURN_TEXT_BYTES, NavigationDestination, NavigationOnlyTarget,
    PendingRunTerminal, PriorExhaustion, ReviewedTarget, RunBlockOrigin, RunBlockRecord, RunQuery,
    RunReceipt, RunRecord, RunState, RunTerminal, SessionHistoryMessage, SessionHistoryPage,
    SessionReadRequest, SessionReceipt, SessionRequest, StartSessionRequest, StartTurn,
    SupersedeInteraction, TurnAdmission, TurnAdmissionRequest, TurnMode, UnresolvedModelAttempt,
    canonical_requirement_digest, canonical_target_digest, compaction_summary_id,
    decision_owner_command_id, interaction_publication_id, next_state_after_decision,
    resume_command_id, state_after_resolution,
};
pub use domain::{
    CONVERSATION_CONSUMER, CONVERSATION_PURPOSE, CanonicalTurnIntent, normalize_turn_text,
};
pub use floe_agent_contract::{
    ArchivePointer, ArchiveReadRequest, ArchiveSnapshot, ArchivedMessage,
};
pub use floe_agent_runtime::FinalPayloadValidator;
pub use ports::{
    ConversationRepository, InteractionRecoveryCursor, InteractionRepository, RecoveryPage,
    SessionArchiveRepository, SessionRepository,
};

pub use turn::{
    AgentBudget, AgentContinuation, AgentEvent, AgentEventKind, AgentMessage, AgentOutcome,
    AgentSession, AgentSessionScope, AgentUsage, MAX_SESSION_BYTES,
    MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES, MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES,
    SessionRecoveryPointer, SessionStore, TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
    TYPED_AGENT_MESSAGE_SCHEMA_ID, TYPED_AGENT_MESSAGE_SCHEMA_VERSION, TypedAgentMessageEvidence,
    TypedAgentMessageProvenance, TypedAgentMessageReference,
};

pub use domain::{
    BlockedRunCommit, InteractionResolutionCommit, OperationApprovalAdmission,
    OperationApprovalPublication, OwnerResolutionReceipt, ResumeChildAdmission, ResumeRequired,
    ReviewAuditEvidence, ReviewAuditRecord, ReviewPublication, SourceReviewLink, TurnInput,
    operation_approval_interaction,
};

pub use application::{
    RunAccountingProjection, apply_terminal, build_resume_required, contract_message,
    defer_run_terminal, project_lineage_accounting, project_run_accounting, project_run_receipt,
    project_session_receipt, project_transcript, terminal_messages, unresolved_run_delegations,
    validate_terminal_steps,
};

pub use application::{apply_source_interaction, recover_source_interaction};

pub use application::{
    ConversationEvent, ConversationEventBuffer, EventPayload, EventRead, ReadConversationEvents,
    RunEventRecord, validate_run_journal,
};

pub use application::{
    PreparedStartTurn, SessionSnapshot, prepare_start_turn, read_session_snapshot,
};

pub use application::{
    CalendarProposalRequest, CalendarProposalResult, CommandReceipt, ConversationDependencies,
    ConversationOwner, InteractionResult, RefreshInteraction, ResolveInteraction,
};

pub use application::{ArtifactSummary, SessionMessage, TaskSummary};

pub use application::{
    InteractionAction, InteractionSnapshot, InteractionStatus, InteractionTarget, MessageSnapshot,
    MessageSnapshotRole,
};

pub use application::{
    PublicRunState, ReplyStatus, RunSnapshot, TurnExecution, TurnReport, project_run_event,
    project_run_snapshot,
};

pub use application::{ConversationFailure, ConversationRecovery, project_conversation_failure};

pub use application::validate_task_delegation_lineage;

pub use domain::{
    ConversationCommandKind, SessionStartAdmission, SessionStartFailure, SessionStartRefusal,
};

pub use application::memory_forget_tool_result;

pub use application::{
    COMPACTION_CONSUMER, CompactionJournalHead, CompactionRecovery, advance_compaction_journal,
    recover_compaction_journal, validate_compaction_journal, validate_compaction_summary,
};

pub use application::{CompactionClaim, CompactionDisposition, CompactionExecution};

pub use ports::{CompactionAdmission, CompactionRepository};

pub use application::{
    COMPACTION_ROLE_ID, CompactionModelProjection, compaction_role_spec, compaction_seed,
};

pub use application::CompactionDriver;

pub use domain::{AppendSurfaceInput, SurfaceAnchor, SurfaceInput, SurfaceInputOrigin, SurfaceInputReceipt, SubmitSurfaceWork, SurfaceWorkReceipt, WorkId, WorkOrigin, PresentationState, MAX_PENDING_SURFACE_WORK, MAX_SURFACE_PAGE_ITEMS};

pub use ports::SurfaceRepository;

pub use domain::{SurfaceWorkSnapshot, SurfaceWorkState};

pub use domain::SubmitTextWork;

pub use domain::{SurfaceEntryReceipt, SurfaceResultReceipt, SurfaceResultReference};
