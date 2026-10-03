use floe_context_contract::DependencyCoverage;
use floe_execution::BoxFuture;
use floe_kernel::AgentFailure;
use uuid::Uuid;

pub trait EvidenceReader: Send + Sync {
    fn read_turn_coverage<'a>(
        &'a self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> BoxFuture<'a, Result<DependencyCoverage, AgentFailure>>;
}
impl<T: EvidenceReader + ?Sized> EvidenceReader for std::sync::Arc<T> {
    fn read_turn_coverage<'a>(
        &'a self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> BoxFuture<'a, Result<DependencyCoverage, AgentFailure>> {
        (**self).read_turn_coverage(session_id, turn_id)
    }
}
