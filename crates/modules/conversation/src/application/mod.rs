mod admission;
mod archive;
mod cancellation;
mod coordinator;
mod finalization;
mod events;
pub use events::{ConversationEventBuffer, ConversationEvent, EventPayload, RunEventRecord, EventRead, ReadConversationEvents};
pub mod governed_session;
mod history_projection;
mod interactions;
mod interaction_resolution;
pub use interaction_resolution::{apply_source_interaction, recover_source_interaction};
mod model_projection;
mod query;
mod recovery;
mod resume;
mod session;
mod source_review;
mod storage_projection;
pub use storage_projection::{project_run_receipt, project_session_receipt, project_transcript, contract_message, terminal_messages, apply_terminal, project_run_accounting, RunAccountingProjection, validate_terminal_steps, interrupt_for_activation};
pub use source_review::{publish_task_projection_review, publish_task_source_review, PublishTaskSourceReview};

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
    DecideInteractionCommand, PublishInteractionRequest,
    decide_interaction, expire_interaction, list_run_interactions, load_interaction,
    publish_interaction,
    resolve_interaction, supersede_interaction,
};
pub use model_projection::ConversationModelProjection;
pub use query::{get_command, get_run};
pub use recovery::{project_continuation, validate_run_journal};
pub use resume::{ResumeSuppression, resume_gate, build_resume_required};
pub use session::{
    admit_unscoped_session, admitted_session, get_session, recovered_session, resume_session,
    start_session,
};

mod start;
pub use start::{PreparedStartTurn, prepare_start_turn, read_session_snapshot};

mod service;
mod manager_policy;
pub use service::{ConversationService, ConversationOwner, ConversationDependencies, CommandReceipt, ResolveInteraction, RefreshInteraction, InteractionResult, MessageSnapshot, MessageSnapshotRole};

mod task_interactions;
pub use task_interactions::{publish_expert_binding_blockers, interaction_ref_artifacts};

mod session_projection;
pub use session_projection::{SessionSnapshot, SessionMessage, TaskSummary, ArtifactSummary};

mod interaction_projection;
pub use interaction_projection::{InteractionSnapshot, InteractionStatus, InteractionTarget, InteractionAction};

mod run_projection;
pub use run_projection::{PublicRunState, TurnExecution, ReplyStatus, TurnReport, RunSnapshot, project_run_snapshot, project_run_event};

mod failure_projection;
pub use failure_projection::{ConversationFailure, ConversationRecovery, project_conversation_failure};
