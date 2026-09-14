use std::sync::Arc;

use floe_agent_contract::{BoxFuture, ExecutionJournal};
use floe_kernel::{AgentFailure, RunId};

use crate::{AdmittedTurn, RunReceipt, RunTerminal, TurnAdmission, TurnAdmissionRequest};

pub trait ConversationRepository: Send + Sync {
    fn admit_turn<'a>(
        &'a self,
        request: TurnAdmissionRequest,
    ) -> BoxFuture<'a, Result<TurnAdmission, AgentFailure>>;

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
}
