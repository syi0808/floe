use std::sync::Arc;

use floe_agent_contract::{ArchiveReadRequest, ArchiveSnapshot, BoxFuture, ExecutionJournal};
use floe_kernel::{AgentFailure, RunId};

use crate::{AdmittedTurn, CancelRunAdmission, CancelRunCommand, CommandQuery, CompactionReceipt, CompactionRequest, JournalEntry, RecoveryReceipt, RecoveryRequest, RunReceipt, RunTerminal, SessionReadRequest, SessionReceipt, SessionRequest, TurnAdmission, TurnAdmissionRequest};

pub trait ConversationRepository: Send + Sync {
    fn find_command<'a>(
        &'a self,
        query: CommandQuery,
    ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>>;

    fn admit_turn<'a>(
        &'a self,
        request: TurnAdmissionRequest,
    ) -> BoxFuture<'a, Result<TurnAdmission, AgentFailure>>;

    fn admit_cancel<'a>(
        &'a self,
        request: CancelRunCommand,
    ) -> BoxFuture<'a, Result<CancelRunAdmission, AgentFailure>>;

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

    fn recover_session<'a>(
        &'a self,
        request: RecoveryRequest,
    ) -> BoxFuture<'a, Result<RecoveryReceipt, AgentFailure>>;

    fn load_journal<'a>(
        &'a self,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<Vec<JournalEntry>, AgentFailure>>;
}

pub trait SessionArchiveRepository: Send + Sync {
    fn compact_session<'a>(
        &'a self,
        request: CompactionRequest,
    ) -> BoxFuture<'a, Result<CompactionReceipt, AgentFailure>>;

    fn read_archive<'a>(
        &'a self,
        request: &'a ArchiveReadRequest,
    ) -> BoxFuture<'a, Result<ArchiveSnapshot, AgentFailure>>;
}

pub trait SessionRepository: Send + Sync {
    fn start_session<'a>(
        &'a self,
        request: SessionRequest,
    ) -> BoxFuture<'a, Result<SessionReceipt, AgentFailure>>;

    fn resume_session<'a>(
        &'a self,
        request: SessionRequest,
    ) -> BoxFuture<'a, Result<SessionReceipt, AgentFailure>>;

    fn get_session<'a>(
        &'a self,
        request: SessionReadRequest,
    ) -> BoxFuture<'a, Result<SessionReceipt, AgentFailure>>;
}
