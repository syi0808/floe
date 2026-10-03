//! Durable original-message lookup for an owner-scheduled linked resume.
use crate::{ConversationRepository, InteractionResumeRef, RunQuery, RunReceipt, TurnMode};
use floe_kernel::AgentFailure;
use uuid::Uuid;

/// One linked-resume request to prepare, in Conversation's own terms.
///
/// The caller names only the origin linkage. The original user-message reference
/// and the mode all come from the origin's own durable admission: no caller
/// resends text, chooses a parent Run or injects a second user message.
pub struct ResumePreparationRequest {
    pub principal: String,
    pub person_id: floe_kernel::PersonId,
    pub session_id: Uuid,
    pub resume: InteractionResumeRef,
}

/// A prepared resume: the Session, the owner-derived origin intent, and
/// the origin itself. The child admits at the origin's Session revision;
/// a newer user turn supersedes the pending request.
pub struct PreparedResume {
    pub session: crate::turn::AgentSession,
    pub mode: TurnMode,
    pub text: String,
    pub origin: RunReceipt,
}

/// Prepare one linked resume against the origin it names.
///
/// The origin must be Completed or Blocked in this Session at the exact
/// lineage. Its stable original-message ID locates the existing user text;
/// a missing or compacted-away message fails closed without inventing text.
///
/// Whether the origin's interaction group currently admits a child, and
/// whether a newer turn has superseded the request, is decided atomically
/// inside admission, never from this read.
pub async fn prepare_resume<Repository, Store>(
    repository: &Repository,
    sessions: &Store,
    request: ResumePreparationRequest,
) -> Result<PreparedResume, AgentFailure>
where
    Repository: ConversationRepository,
    Store: crate::turn::SessionStore,
{
    request.resume.validate()?;
    let origin = super::query::get_run(
        repository,
        RunQuery {
            principal: request.principal.clone(),
            run_id: request.resume.origin_run_id,
        },
    )
    .await?
    .ok_or(AgentFailure::NotFound)?;
    if origin.session_id != request.session_id || origin.resume().as_ref() != Some(&request.resume)
    {
        return Err(AgentFailure::Conflict);
    }
    let session = sessions.load(request.person_id, request.session_id).await?;
    if session.scope.is_some() || session.data_classes != [floe_agent_contract::DataClass::Personal]
    {
        return Err(AgentFailure::Conflict);
    }
    let text = derive_origin_text(repository, &request, &session, &origin).await?;
    Ok(PreparedResume {
        session,
        mode: TurnMode::Resume(request.resume),
        text,
        origin,
    })
}

/// The original user text identified by durable admission. Resume children
/// append no user message and retain this same identity across the lineage.
async fn derive_origin_text<Repository: ConversationRepository>(
    _repository: &Repository,
    request: &ResumePreparationRequest,
    session: &crate::turn::AgentSession,
    origin: &RunReceipt,
) -> Result<String, AgentFailure> {
    if origin.session_id != request.session_id || origin.principal != request.principal {
        return Err(AgentFailure::Conflict);
    }
    session
        .messages
        .iter()
        .find_map(|message| match message {
            crate::turn::AgentMessage::User {
                message_id, text, ..
            } if *message_id == origin.user_message_id => Some(text.clone()),
            _ => None,
        })
        .ok_or(AgentFailure::Conflict)
}
