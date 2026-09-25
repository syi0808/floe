mod admission;
mod archive;
mod cancellation;
mod coordinator;
mod finalization;
pub mod governed_session;
mod history_projection;
mod interactions;
mod model_projection;
mod query;
mod recovery;
mod resume;
mod session;

pub use admission::{
    PreparedResume, PreparedTurn, ResumePreparationRequest, TurnPrecheck, TurnPrecheckRequest,
    TurnPreparationRequest, precheck_turn, prepare_resume, prepare_turn,
};
pub use archive::{compact_session, read_archive};
pub use cancellation::{
    CancelCommandRequest, CancelRunAdmission, CancelRunCommand, CancelRunReceipt, CancelRunRequest,
    CancelRunStatus, RunCancellationRegistry, cancel_run_command,
};
pub use coordinator::{ConversationService, continuation, recover_session};
pub use governed_session::{GovernedSessionRepository, GovernedSessionStore};
pub use history_projection::{
    HistoryProjection, ProjectedModelConversation, project_model_conversation_history,
};
pub use interactions::{
    DecideInteractionCommand, PublishInteractionRequest, PublishModelRequirement,
    decide_interaction, expire_interaction, list_run_interactions, load_interaction,
    publish_interaction, publish_model_requirement, resolve_interaction, supersede_interaction,
};
pub use model_projection::ConversationModelProjection;
pub use query::{get_command, get_run};
pub use recovery::project_continuation;
pub use resume::{ResumeSuppression, resume_gate};
pub use session::{
    admit_unscoped_session, admitted_session, get_session, recovered_session, resume_session,
    start_session,
};

#[cfg(test)]
mod tests;
