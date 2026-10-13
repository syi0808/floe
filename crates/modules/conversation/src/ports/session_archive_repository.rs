use floe_agent_contract::{ArchiveReadRequest, ArchiveSnapshot, BoxFuture};
use floe_kernel::AgentFailure;

pub trait SessionArchiveRepository: Send + Sync {
    fn read_archive<'a>(
        &'a self,
        request: &'a ArchiveReadRequest,
    ) -> BoxFuture<'a, Result<ArchiveSnapshot, AgentFailure>>;
}
