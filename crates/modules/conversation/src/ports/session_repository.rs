use std::sync::Arc;
use floe_agent_contract::{ArchiveReadRequest, ArchiveSnapshot, BoxFuture, ExecutionJournal};
use floe_kernel::{AgentFailure, RunId};
use crate::{AdmittedTurn, CancelRunAdmission, CancelRunCommand, CommandQuery, CompactionReceipt, CompactionRequest, JournalEntry, RecoveryReceipt, RecoveryRequest, RunReceipt, RunTerminal, SessionReadRequest, SessionReceipt, SessionRequest, TurnAdmission, TurnAdmissionRequest};

pub trait SessionRepository: Send + Sync {
    fn start_session<'a>(
        &'a self,
        request: crate::StartSessionRequest,
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
