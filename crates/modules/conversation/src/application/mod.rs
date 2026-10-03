mod admission;
mod archive;
mod cancellation;
mod coordinator;
mod events;
mod finalization;
pub use events::{
    ConversationEvent, ConversationEventBuffer, EventPayload, EventRead, ReadConversationEvents,
    RunEventRecord,
};
pub mod governed_session;
mod history_projection;
mod interaction_resolution;
mod interactions;
pub use interaction_resolution::{apply_source_interaction, recover_source_interaction};
mod model_projection;
mod query;
mod recovery;
mod resume;
mod session;
mod source_review;
mod storage_projection;
pub use source_review::{
    PublishTaskSourceReview, publish_task_projection_review, publish_task_source_review,
};
pub use storage_projection::{
    RunAccountingProjection, apply_terminal, contract_message, interrupt_for_activation,
    project_run_accounting, project_run_receipt, project_session_receipt, project_transcript,
    terminal_messages, validate_terminal_steps,
};

pub use admission::{PreparedResume, ResumePreparationRequest, prepare_resume};
pub use archive::{compact_session, read_archive};
pub use cancellation::{
    CancelCommandRequest, CancelRunAdmission, CancelRunCommand, CancelRunReceipt, CancelRunRequest,
    CancelRunStatus, RunCancellationRegistry, cancel_run_command,
};
pub use coordinator::{continuation, recover_session};
pub use governed_session::{GovernedSessionRepository, GovernedSessionStore};
pub use history_projection::{
    HistoryProjection, ProjectedModelConversation, project_model_conversation_history,
};
pub use interactions::{
    DecideInteractionCommand, PublishInteractionRequest, decide_interaction, expire_interaction,
    list_run_interactions, load_interaction, publish_interaction, resolve_interaction,
    supersede_interaction,
};
pub use model_projection::ConversationModelProjection;
pub use query::{get_command, get_run};
pub use recovery::{project_continuation, validate_run_journal};
pub use resume::{ResumeSuppression, build_resume_required, resume_gate};
pub use session::{
    admit_unscoped_session, admitted_session, get_session, recovered_session, resume_session,
    start_session,
};

mod start;
pub use start::{PreparedStartTurn, prepare_start_turn, read_session_snapshot};

mod manager_policy;
mod service;
pub use service::{
    CommandReceipt, ConversationDependencies, ConversationOwner, ConversationService,
    InteractionResult, MessageSnapshot, MessageSnapshotRole, RefreshInteraction,
    ResolveInteraction,
};

mod task_interactions;
pub use task_interactions::{interaction_ref_artifacts, publish_expert_binding_blockers};

mod session_projection;
pub use session_projection::{ArtifactSummary, SessionMessage, SessionSnapshot, TaskSummary};

mod interaction_projection;
pub use interaction_projection::{
    InteractionAction, InteractionSnapshot, InteractionStatus, InteractionTarget,
};

mod run_projection;
pub use run_projection::{
    PublicRunState, ReplyStatus, RunSnapshot, TurnExecution, TurnReport, project_run_event,
    project_run_snapshot,
};

mod failure_projection;
pub use failure_projection::{
    ConversationFailure, ConversationRecovery, project_conversation_failure,
};
