use floe_agent_contract::AgentFailure;

use floe_kernel::PersonId;

use crate::turn::{AgentSession, SessionStore};
use crate::{SessionReadRequest, SessionReceipt, SessionRepository, SessionRequest};

pub async fn start_session<Repository: SessionRepository>(
    repository: &Repository,
    request: crate::StartSessionRequest,
) -> Result<crate::SessionStartAdmission, crate::SessionStartFailure> {
    use crate::{
        ConversationCommandKind, SessionStartAdmission as A, SessionStartFailure as F,
        SessionStartRefusal,
    };
    request.validate().map_err(F::NotAdmitted)?;
    let principal = request.principal.clone();
    let admission = repository.start_session(request).await?;
    match admission {
        A::Started(receipt) => verify_receipt(principal, None, receipt)
            .map(A::Started)
            .map_err(F::Indeterminate),
        A::Replayed(receipt) => verify_receipt(principal, None, receipt)
            .map(A::Replayed)
            .map_err(F::Indeterminate),
        A::NotApplied(SessionStartRefusal::ForeignCommand(
            ConversationCommandKind::SessionStart,
        )) => Err(F::Indeterminate(AgentFailure::StorageUnavailable)),
        A::NotApplied(reason) => Ok(A::NotApplied(reason)),
    }
}

pub async fn resume_session<Repository: SessionRepository>(
    repository: &Repository,
    request: SessionRequest,
) -> Result<Option<SessionReceipt>, AgentFailure> {
    request.validate()?;
    let principal = request.principal.clone();
    repository
        .resume_session(request)
        .await?
        .map(|receipt| verify_receipt(principal, None, receipt))
        .transpose()
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

/// The Session a receipt names, once it is one this Person may be shown.
///
/// A receipt is a claim about a Session; this reads the Session it names and
/// checks the two agree. A scoped Session, or one holding anything but the
/// Person's own data, is not a root conversation whatever the receipt says.
pub async fn admitted_session(
    sessions: &impl SessionStore,
    person_id: PersonId,
    receipt: SessionReceipt,
) -> Result<AgentSession, AgentFailure> {
    receipt.validate()?;
    if receipt.principal != person_id.to_string() {
        return Err(AgentFailure::CapabilityDenied);
    }
    let session = sessions.load(person_id, receipt.session_id).await?;
    if session.id != receipt.session_id
        || session.person_id != person_id
        || session.revision != receipt.session_revision
        || session.scope.is_some()
        || session.data_classes != [floe_agent_contract::DataClass::Personal]
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(session)
}

/// That a Session is not bound to one Expert's own scope.
///
/// A scoped Session answers for that Expert alone; a turn raised outside it is
/// not the Session's to take.
pub fn admit_unscoped_session(session: &AgentSession) -> Result<(), AgentFailure> {
    if session.scope.is_some() {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}
