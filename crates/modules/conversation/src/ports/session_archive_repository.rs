use std::sync::Arc;
use floe_agent_contract::{ArchiveReadRequest, ArchiveSnapshot, BoxFuture, ExecutionJournal};
use floe_kernel::{AgentFailure, RunId};
use crate::{AdmittedTurn, CancelRunAdmission, CancelRunCommand, CommandQuery, CompactionReceipt, CompactionRequest, JournalEntry, RecoveryReceipt, RecoveryRequest, RunReceipt, RunTerminal, SessionReadRequest, SessionReceipt, SessionRequest, TurnAdmission, TurnAdmissionRequest};

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

