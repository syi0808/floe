use std::future::Future;

use floe_agent_contract::{AgentFailure, ArchiveReadRequest};

use crate::{
    CompactionReceipt, CompactionRequest, SessionArchiveRepository,
    adapters::context_reader::ConversationArchiveReader,
};

pub async fn compact_session<Repository: SessionArchiveRepository>(
    repository: &Repository,
    request: CompactionRequest,
) -> Result<CompactionReceipt, AgentFailure> {
    request.validate()?;
    let expected_session_id = request.session_id;
    let expected_revision = request
        .expected_session_revision
        .checked_add(1)
        .ok_or(AgentFailure::Conflict)?;
    let expected_turn_id = request.through_turn_id;
    let expected_summary = request.summary.clone();
    let receipt = repository.compact_session(request).await?;
    receipt.validate()?;
    if receipt.session_id != expected_session_id
        || receipt.session_revision != expected_revision
        || receipt.pointer.source_revision.checked_add(1) != Some(receipt.session_revision)
        || receipt.pointer.through_turn_id != expected_turn_id
        || receipt.summary.text != expected_summary
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(receipt)
}

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
