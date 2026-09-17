mod admission;
mod archive;
mod cancellation;
mod coordinator;
mod finalization;
pub mod governed_session;
mod history_projection;
mod query;
mod recovery;
mod session;

pub use admission::{
    PreparedTurn, TurnPrecheck, TurnPrecheckRequest, TurnPreparationRequest, precheck_turn,
    prepare_turn,
};
pub use archive::{compact_session, read_archive};
pub use cancellation::{
    CancelCommandRequest, CancelRunAdmission, CancelRunCommand, CancelRunReceipt, CancelRunRequest,
    CancelRunStatus, RunCancellationRegistry, cancel_run_command,
};
pub use coordinator::{ConversationService, continuation, recover_session};
pub use governed_session::{GovernedSessionRepository, GovernedSessionStore};
pub use history_projection::{HistoryProjection, narrow_by_source_boundary, project_history_into};
pub use query::{get_command, get_run};
pub use recovery::project_continuation;
pub use session::{
    admit_unscoped_session, admitted_session, get_session, recovered_session, resume_session,
    start_session,
};

#[cfg(test)]
mod tests;
