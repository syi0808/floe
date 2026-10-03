use crate::{CompactionReceipt, CompactionRequest};
use floe_agent_contract::{ArchiveReadRequest, ArchiveSnapshot, BoxFuture};
use floe_kernel::AgentFailure;

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
