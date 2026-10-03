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
    GovernedSessionRepository, GovernedSessionStore, HistoryProjection, PreparedResume,
    ProjectedModelConversation, ResumePreparationRequest,
    ResumeSuppression, RunCancellationRegistry, admit_unscoped_session, admitted_session,
    cancel_run_command, compact_session, continuation, decide_interaction, expire_interaction,
    get_command, get_run, get_session, list_run_interactions, load_interaction, prepare_resume,
    project_continuation, project_model_conversation_history, 
    read_archive, recover_session, recovered_session,
    resolve_interaction, resume_gate, resume_session, start_session, supersede_interaction,
};
pub use domain::{
    AdmittedExecution, AdmittedTurn, CommandQuery, CompactionReceipt, CompactionRequest,
    ContinuationRef, ContinuationSnapshot, ContinuationToken, ConversationInteraction,
    DecisionAdmission, ExpireInteraction, ExpireOutcome,
    INTERACTION_PENDING_LIFETIME_MS, InteractionDecision, InteractionDecisionKind,
    InteractionOrigin, InteractionRefresh, InteractionRequirement, InteractionRequirementKind,
    InteractionResolution, InteractionResolutionReceipt, InteractionResolutionCause, InteractionResumeRef, InteractionState,
    JournalEntry, MAX_ACTIVE_INTERACTIONS_PER_RUN, MAX_COMPACTION_SUMMARY_BYTES,
    MAX_RESUME_LINEAGE, MAX_REVIEWED_IDENTIFIER_BYTES, MAX_REVIEWED_PURPOSE_BYTES,
    MAX_REVIEWED_SOURCE_BYTES, MAX_REVIEWED_TARGET_BYTES, MAX_STORED_INTERACTIONS_PER_RUN,
    MAX_TARGET_BUNDLE_MEMBERS, MAX_TURN_TEXT_BYTES, NavigationDestination, NavigationOnlyTarget,
    PriorExhaustion, RecoveryReceipt, RecoveryRequest, ReviewedTarget,
    BlockedInteractionLink, RunBlockOrigin, RunBlockRecord, RunQuery, RunReceipt, RunRecord, RunState, RunTerminal,
    SessionReadRequest, SessionReceipt, SessionRequest, StartSessionRequest, StartTurn,
    SupersedeInteraction, TurnAdmission, TurnAdmissionRequest, TurnMode, UnresolvedModelAttempt,
    canonical_requirement_digest, canonical_target_digest, decision_owner_command_id,
    interaction_publication_id, next_state_after_decision, resume_command_id,
    state_after_resolution,
};
pub use domain::{
    CONVERSATION_CONSUMER, CONVERSATION_PURPOSE, CanonicalTurnIntent, normalize_turn_text,
};
pub use floe_agent_contract::{
    ArchivePointer, ArchiveReadRequest, ArchiveSnapshot, ArchivedMessage,
};
pub use floe_agent_runtime::FinalPayloadValidator;
pub use ports::{
    ConversationRepository, InteractionRepository, SessionArchiveRepository, SessionRepository,
};

pub use turn::{
    AgentBudget, AgentContinuation, AgentEvent, AgentEventKind, AgentMessage, AgentOutcome,
    AgentSession, AgentSessionScope, AgentUsage, DelegationExecution, DelegationExecutionState,
    SessionRecoveryPointer, SessionStore,
};

pub use domain::{
    BlockedRunCommit, InteractionResolutionCommit, OwnerResolutionReceipt,
    BlockedReviewEvidence, SourceReviewLink, ReviewPublication, ReviewAuditRecord,
    ResumeChildAdmission, ResumeRequired, TurnInput,
};

pub use application::{
    RunAccountingProjection, apply_terminal, build_resume_required, contract_message,
    project_run_accounting, project_run_receipt, project_session_receipt, project_transcript,
    terminal_messages, validate_terminal_steps,
};

pub use application::{apply_source_interaction, recover_source_interaction};

pub use application::{
    ConversationEvent, ConversationEventBuffer, EventPayload, EventRead, ReadConversationEvents,
    RunEventRecord, validate_run_journal,
};

pub use application::{
    PreparedStartTurn, SessionSnapshot, prepare_start_turn, read_session_snapshot,
};

pub use application::{PublishTaskSourceReview, publish_task_source_review};

pub use application::{
    CommandReceipt, ConversationDependencies, ConversationOwner, InteractionResult,
    RefreshInteraction, ResolveInteraction,
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

pub use application::interrupt_for_activation;

pub use application::{ConversationFailure, ConversationRecovery, project_conversation_failure};

pub use application::validate_task_delegation_lineage;
