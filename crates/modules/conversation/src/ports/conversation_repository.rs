use crate::{
    AdmittedTurn, CancelRunAdmission, CancelRunCommand, CommandQuery, JournalEntry, RunReceipt,
    RunTerminal, TurnAdmission, TurnAdmissionRequest,
};
use floe_agent_contract::{BoxFuture, ExecutionJournal};
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor, RunId};
use std::sync::Arc;

pub trait ConversationRepository: Send + Sync {
    /// Read a bounded reverse page from normalized Manager custody. Product
    /// cursor aliases resolve to exact transcript references in the adapter.
    fn read_session_history_page<'a>(
        &'a self,
        session_id: uuid::Uuid,
        before_message_id: Option<uuid::Uuid>,
        limit: usize,
        byte_limit: usize,
    ) -> BoxFuture<'a, Result<crate::SessionHistoryPage, AgentFailure>> {
        let _ = (session_id, before_message_id, limit, byte_limit);
        Box::pin(async { Err(AgentFailure::UnsupportedVersion) })
    }

    /// Exact retained User text for Continue and linked Resume preparation.
    fn read_session_user_message<'a>(
        &'a self,
        session_id: uuid::Uuid,
        message_id: uuid::Uuid,
    ) -> BoxFuture<'a, Result<Option<crate::SessionHistoryMessage>, AgentFailure>> {
        let _ = (session_id, message_id);
        Box::pin(async { Err(AgentFailure::UnsupportedVersion) })
    }

    fn recovery_runs<'a>(
        &'a self,
        actor: &'a OwnerActor,
        after: Option<RunId>,
        limit: usize,
    ) -> BoxFuture<'a, Result<crate::RecoveryPage<RunId, RunId>, AgentFailure>>;
    /// Complete an immutable deferred failure only after all Task results exist.
    fn settle_pending_terminal<'a>(
        &'a self,
        actor: &'a OwnerActor,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>>;
    /// Settle only an already-recorded delegation intent after owner recovery.
    fn reconcile_delegation<'a>(
        &'a self,
        run_id: RunId,
        receipt: floe_agent_contract::TaskReceipt,
    ) -> BoxFuture<'a, Result<(), AgentFailure>>;

    fn finish_blocked_run<'a>(
        &'a self,
        commit: crate::BlockedRunCommit,
    ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>>;

    fn find_command<'a>(
        &'a self,
        query: CommandQuery,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>>;

    /// Read the immutable owner-wide command-ID occupant set. Admission
    /// rechecks this inside its transaction; fresh preflight must not report
    /// NotApplied when another Conversation command kind already used the ID.
    fn command_occupant<'a>(
        &'a self,
        command_id: floe_kernel::CommandId,
    ) -> BoxFuture<'a, Result<Option<crate::ConversationCommandKind>, AgentFailure>>;

    fn admit_turn<'a>(
        &'a self,
        request: TurnAdmissionRequest,
    ) -> BoxFuture<'a, Result<TurnAdmission, CommandFailure<AgentFailure>>>;

    fn admit_cancel<'a>(
        &'a self,
        request: CancelRunCommand,
    ) -> BoxFuture<'a, Result<CancelRunAdmission, CommandFailure<AgentFailure>>>;

    fn journal(&self, run_id: RunId) -> Result<Arc<dyn ExecutionJournal>, AgentFailure>;

    fn finish_run<'a>(
        &'a self,
        run_id: RunId,
        expected_aggregate_revision: u64,
        terminal: RunTerminal,
    ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>>;

    fn load_run<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Option<AdmittedTurn>, AgentFailure>>;

    fn load_receipt<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>>;

    fn load_journal<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Vec<JournalEntry>, AgentFailure>>;
}
