//! Conversation-owned Session admission, root Run execution, and finalization.

mod adapters;
mod api;
mod application;
mod domain;
mod ports;
pub mod prompts;
mod turn;

pub use api::{
    ConversationPorts, FINALIZATION_OUTPUT_CONTRACT,
    FINALIZATION_ROLE_ID, FINALIZATION_ROLE_PROMPT, MANAGER_OUTPUT_CONTRACT,
    ManagerConfig, TurnRequest,
};
pub use application::{
    CancelCommandRequest, CancelRunAdmission, CancelRunCommand, CancelRunReceipt, CancelRunRequest,
    CancelRunStatus, ConversationModelProjection, ConversationService, DecideInteractionCommand,
    GovernedSessionRepository, GovernedSessionStore, HistoryProjection, PreparedResume,
    ProjectedModelConversation, PublishInteractionRequest,
    ResumePreparationRequest, ResumeSuppression, RunCancellationRegistry, admit_unscoped_session, admitted_session,
    cancel_run_command, compact_session, continuation, decide_interaction, expire_interaction,
    get_command, get_run, get_session, list_run_interactions, load_interaction, prepare_resume, project_continuation, project_model_conversation_history,
    publish_interaction, publish_task_projection_review, read_archive, recover_session,
    recovered_session, resolve_interaction, resume_gate,
    resume_session, start_session, supersede_interaction,
};
pub use domain::{
    AdmittedExecution, AdmittedTurn, CommandQuery, InteractionRefresh, CompactionReceipt,
    CompactionRequest, ContinuationRef, ContinuationSnapshot, ConversationInteraction,
    DecisionAdmission, ExpertBindingTarget, ExpireInteraction, ExpireOutcome,
    INTERACTION_PENDING_LIFETIME_MS, InteractionDecision,
    InteractionDecisionKind, InteractionOrigin, InteractionRequirement, InteractionRequirementKind,
    InteractionResolution, InteractionResolutionReceipt, InteractionResumeRef, InteractionState,
    JournalEntry, MAX_ACTIVE_INTERACTIONS_PER_RUN, MAX_COMPACTION_SUMMARY_BYTES,
    MAX_RESUME_LINEAGE, MAX_REVIEWED_IDENTIFIER_BYTES,
    MAX_REVIEWED_PURPOSE_BYTES, MAX_REVIEWED_SOURCE_BYTES, MAX_REVIEWED_TARGET_BYTES,
    MAX_STORED_INTERACTIONS_PER_RUN, MAX_TARGET_BUNDLE_MEMBERS, MAX_TURN_TEXT_BYTES,
    NavigationDestination, NavigationOnlyTarget, PublishAdmission,
    RecoveryReceipt, RecoveryRequest, ReviewedTarget,
    UnresolvedModelAttempt, RunQuery, RunRecord, RunReceipt, RunState, RunTerminal, RunBlockRecord, RunBlockOrigin, PriorExhaustion, SessionReadRequest, SessionReceipt,
    SessionRequest, StartSessionRequest, StartTurn, ContinuationToken, SupersedeInteraction, TurnAdmission, TurnAdmissionRequest, TurnMode,
    canonical_requirement_digest, canonical_target_digest, decision_owner_command_id,
    interaction_publication_id, next_state_after_decision, resume_command_id,
    state_after_resolution,
};
pub use domain::{CanonicalTurnIntent, normalize_turn_text, CONVERSATION_PURPOSE, CONVERSATION_CONSUMER};
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

pub use domain::{TurnInput, ProjectionReviewRecord, ProjectionReviewPublication, BlockedRunCommit, OwnerResolutionReceipt, InteractionResolutionCommit, ResumeRequired, ResumeChildAdmission, PublishTaskProjectionReview};

pub use application::{project_run_receipt, project_session_receipt, project_transcript, contract_message, terminal_messages, apply_terminal, project_run_accounting, RunAccountingProjection, build_resume_required, validate_terminal_steps};

pub use application::{apply_source_interaction, recover_source_interaction};

pub use application::{ConversationEventBuffer, ConversationEvent, EventPayload, RunEventRecord, EventRead, ReadConversationEvents, validate_run_journal};

pub use application::{SessionSnapshot, PreparedStartTurn, prepare_start_turn, read_session_snapshot};

pub use application::{publish_task_source_review, PublishTaskSourceReview};

pub use application::{ConversationOwner, ConversationDependencies, CommandReceipt, ResolveInteraction, RefreshInteraction, InteractionResult};

pub use application::{publish_expert_binding_blockers, interaction_ref_artifacts};

pub use application::{SessionMessage, TaskSummary, ArtifactSummary};

pub use application::{InteractionSnapshot, InteractionStatus, InteractionTarget, InteractionAction, MessageSnapshot, MessageSnapshotRole};

pub use application::{PublicRunState, TurnExecution, ReplyStatus, TurnReport, RunSnapshot, project_run_snapshot, project_run_event};

pub use application::interrupt_for_activation;

pub use application::{ConversationFailure, ConversationRecovery, project_conversation_failure};
