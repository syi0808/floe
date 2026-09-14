use floe_agent_contract::AgentFailure;

use crate::{SessionReadRequest, SessionReceipt, SessionRepository, SessionRequest};

pub async fn start_session<Repository: SessionRepository>(
    repository: &Repository,
    request: SessionRequest,
) -> Result<SessionReceipt, AgentFailure> {
    request.validate()?;
    let principal = request.principal.clone();
    verify_receipt(principal, None, repository.start_session(request).await?)
}

pub async fn resume_session<Repository: SessionRepository>(
    repository: &Repository,
    request: SessionRequest,
) -> Result<SessionReceipt, AgentFailure> {
    request.validate()?;
    let principal = request.principal.clone();
    verify_receipt(principal, None, repository.resume_session(request).await?)
}

pub async fn get_session<Repository: SessionRepository>(
    repository: &Repository,
    request: SessionReadRequest,
) -> Result<SessionReceipt, AgentFailure> {
    request.validate()?;
    let principal = request.principal.clone();
    let session_id = request.session_id;
    verify_receipt(
        principal,
        Some(session_id),
        repository.get_session(request).await?,
    )
}

fn verify_receipt(
    principal: String,
    session_id: Option<uuid::Uuid>,
    receipt: SessionReceipt,
) -> Result<SessionReceipt, AgentFailure> {
    receipt.validate()?;
    if receipt.principal != principal || session_id.is_some_and(|id| receipt.session_id != id) {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(receipt)
}
