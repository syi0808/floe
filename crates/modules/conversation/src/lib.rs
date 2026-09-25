//! Conversation-owned Session admission, root Run execution, and finalization.

mod adapters;
mod api;
mod application;
mod domain;
mod ports;
pub mod prompts;
mod turn;

pub use api::{
    CONVERSATION_MODEL_CONSUMER, ConversationPorts, FINALIZATION_OUTPUT_CONTRACT,
    FINALIZATION_ROLE_ID, FINALIZATION_ROLE_PROMPT, MANAGER_OUTPUT_CONTRACT,
    MODEL_CONSENT_LIMITATION, ManagerConfig, TurnRequest,
};
pub use application::{
    CancelCommandRequest, CancelRunAdmission, CancelRunCommand, CancelRunReceipt, CancelRunRequest,
    CancelRunStatus, ConversationModelProjection, ConversationService, DecideInteractionCommand,
    GovernedSessionRepository, GovernedSessionStore, HistoryProjection, PreparedResume,
    PreparedTurn, ProjectedModelConversation, PublishInteractionRequest, PublishModelRequirement,
    ResumePreparationRequest, ResumeSuppression, RunCancellationRegistry, TurnPrecheck,
    TurnPrecheckRequest, TurnPreparationRequest,
    admit_unscoped_session, admitted_session, cancel_run_command, compact_session, continuation,
    decide_interaction, expire_interaction, get_command, get_run, get_session,
    list_run_interactions, load_interaction, precheck_turn,
    prepare_resume, prepare_turn, project_continuation, project_model_conversation_history,
    publish_interaction,
    publish_model_requirement, read_archive, recover_session, recovered_session,
    resolve_interaction, resume_gate, resume_session, start_session, supersede_interaction,
};
pub use domain::{
    AdmittedExecution, AdmittedTurn, AuthorityRevision, CommandQuery, CompactionReceipt,
    CompactionRequest, ContinuationRef, ContinuationSnapshot, ConversationInteraction,
    DecisionAdmission, ExpectedGrantState, ExpireInteraction, ExpireOutcome,
    INTERACTION_PENDING_LIFETIME_MS, InlineObserveTarget, InteractionDecision,
    InteractionDecisionKind, InteractionOrigin, InteractionRequirement, InteractionRequirementKind,
    InteractionResolution, InteractionResolutionReceipt, InteractionResumeRef, InteractionState,
    JournalEntry, MAX_ACTIVE_INTERACTIONS_PER_RUN, MAX_COMPACTION_SUMMARY_BYTES,
    MAX_RECIPIENT_CONSENT_TARGET_BYTES, MAX_RESUME_LINEAGE,
    MAX_REVIEWED_IDENTIFIER_BYTES, MAX_REVIEWED_PURPOSE_BYTES, MAX_REVIEWED_SOURCE_BYTES,
    MAX_REVIEWED_TARGET_BYTES, MAX_STORED_INTERACTIONS_PER_RUN, MAX_TARGET_BUNDLE_MEMBERS,
    MAX_TURN_TEXT_BYTES, NavigationDestination, NavigationOnlyTarget, ProfileSelection,
    PublishAdmission, RecipientConsentTarget, RecoveryReceipt, RecoveryRequest,
    ReviewedBundleMember, ReviewedTarget, RunQuery, RunReceipt, RunState, RunTerminal,
    SessionReadRequest, SessionReceipt, SessionRequest, StartTurn, SupersedeInteraction,
    TurnAdmission, TurnAdmissionRequest, TurnMode, canonical_requirement_digest,
    canonical_target_digest, decision_operation_id, interaction_publication_id,
    next_state_after_decision, resume_command_id, state_after_resolution,
};
pub use domain::{CanonicalTurnIntent, normalize_turn_text};
pub use floe_agent_contract::{
    ArchivePointer, ArchiveReadRequest, ArchiveSnapshot, ArchivedMessage,
};
pub use floe_agent_runtime::FinalPayloadValidator;
pub use ports::{
    ConversationRepository, InteractionRepository, SessionArchiveRepository, SessionRepository,
};

pub use turn::{
    AgentBudget, AgentContinuation, AgentEvent, AgentEventKind, AgentMessage, AgentOutcome,
    AgentSession, AgentSessionScope, AgentUsage,
    DelegationExecution, DelegationExecutionState, SessionRecoveryPointer, SessionStore,
};
