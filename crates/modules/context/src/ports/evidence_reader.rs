use std::future::Future;

use floe_agent_contract::AgentFailure;
use floe_context_contract::DependencyCoverage;
use uuid::Uuid;

pub trait EvidenceReader: Send + Sync {
    fn read_turn_coverage(
        &self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> impl Future<Output = Result<DependencyCoverage, AgentFailure>> + Send;
}
