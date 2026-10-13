use std::future::Future;

use floe_agent_contract::{AgentFailure, ArchiveReadRequest};

use crate::{SessionArchiveRepository, adapters::context_reader::ConversationArchiveReader};

pub async fn read_archive<Repository, Authorize, AuthorizationFuture>(
    repository: &Repository,
    request: &ArchiveReadRequest,
    authorize: Authorize,
) -> Result<floe_context::ArchiveProjection, AgentFailure>
where
    Repository: SessionArchiveRepository,
    Authorize: FnMut(floe_context::ContextDependency) -> AuthorizationFuture,
    AuthorizationFuture: Future<Output = Result<bool, AgentFailure>>,
{
    let reader = ConversationArchiveReader::new(repository);
    floe_context::read_authorized_archive(&reader, request, authorize).await
}
