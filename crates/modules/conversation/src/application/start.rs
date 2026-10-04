//! Owner-issued continuation references and canonical product turn preparation.
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor, RunId};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AgentSession, CanonicalTurnIntent, CommandQuery, ContinuationToken, ConversationRepository,
    RunReceipt, SessionStore, StartTurn, TurnMode,
};

use super::session_projection::{SessionSnapshot, project_session_snapshot};

pub struct PreparedStartTurn {
    pub session: AgentSession,
    pub intent: CanonicalTurnIntent,
    pub existing: Option<RunReceipt>,
}

pub async fn prepare_start_turn<R: ConversationRepository + ?Sized, S: SessionStore>(
    repository: &R,
    sessions: &S,
    actor: &OwnerActor,
    request: &StartTurn,
    scope: &ExecutionScope,
) -> Result<PreparedStartTurn, AgentFailure> {
    actor.validate()?;
    request.validate()?;
    let session = scope
        .run(sessions.load(actor.person_id, request.session_id))
        .await?;
    validate_session(actor, &session, request.session_id)?;
    let existing = scope
        .run(repository.find_command(CommandQuery {
            principal: actor.person_id.to_string(),
            command_id: request.command_id,
        }))
        .await?;
    if existing.as_ref().is_some_and(|receipt| {
        receipt.principal != actor.person_id.to_string()
            || receipt.device_id != actor.device_id
            || receipt.session_id != request.session_id
    }) {
        return Err(AgentFailure::PolicyDenied);
    }
    if existing.is_none() && session.revision != request.expected_revision {
        return Err(AgentFailure::Conflict);
    }
    let text = crate::normalize_turn_text(&request.text)?;
    let mode = if let Some(reference) = &request.continuation_ref {
        let run_id = match &existing {
            Some(receipt) => receipt.continuation_of.ok_or(AgentFailure::Conflict)?,
            None => current_continuation_run(&session).ok_or(AgentFailure::Conflict)?,
        };
        let source = scope
            .run(repository.load_receipt(run_id))
            .await?
            .ok_or(AgentFailure::NotFound)?;
        let expected = issue_continuation(actor, &source)?.ok_or(AgentFailure::Conflict)?;
        if expected != *reference
            || source.session_id != request.session_id
            || existing.is_none() && source.session_revision != session.revision
        {
            return Err(AgentFailure::Conflict);
        }
        if existing.is_none() {
            let original = session
                .messages
                .iter()
                .find_map(|message| match message {
                    crate::AgentMessage::User {
                        message_id, text, ..
                    } if *message_id == source.user_message_id => Some(text),
                    _ => None,
                })
                .ok_or(AgentFailure::Conflict)?;
            if *original != text {
                return Err(AgentFailure::Conflict);
            }
        }
        TurnMode::Continue(source.continuation().ok_or(AgentFailure::Conflict)?)
    } else {
        if existing
            .as_ref()
            .is_some_and(|receipt| receipt.continuation_of.is_some() || receipt.resume_of.is_some())
        {
            return Err(AgentFailure::Conflict);
        }
        TurnMode::New
    };
    let intent = CanonicalTurnIntent {
        session_id: request.session_id,
        expected_revision: request.expected_revision,
        text,
        mode,
        retry_of: request.retry_of,
    };
    if let Some(receipt) = &existing {
        receipt.validate()?;
        if receipt.request_digest != intent.digest(&actor.person_id.to_string())? {
            return Err(AgentFailure::Conflict);
        }
    }
    Ok(PreparedStartTurn {
        session,
        intent,
        existing,
    })
}

pub async fn read_session_snapshot<R: ConversationRepository + ?Sized, S: SessionStore>(
    repository: &R,
    sessions: &S,
    actor: &OwnerActor,
    session_id: Uuid,
    before_message_id: Option<Uuid>,
    scope: &ExecutionScope,
) -> Result<SessionSnapshot, AgentFailure> {
    actor.validate()?;
    if session_id.is_nil() {
        return Err(AgentFailure::InvalidInput);
    }
    let session = scope
        .run(sessions.load(actor.person_id, session_id))
        .await?;
    validate_session(actor, &session, session_id)?;
    let continuation_ref = match current_continuation_run(&session) {
        Some(run_id) => {
            let source = scope
                .run(repository.load_receipt(run_id))
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if source.session_id != session.id || source.session_revision != session.revision {
                return Err(AgentFailure::StorageUnavailable);
            }
            issue_continuation(actor, &source)?
        }
        None => None,
    };
    project_session_snapshot(session, continuation_ref, before_message_id)
}

fn validate_session(
    actor: &OwnerActor,
    session: &AgentSession,
    session_id: Uuid,
) -> Result<(), AgentFailure> {
    if session.id != session_id || session.person_id != actor.person_id {
        return Err(AgentFailure::PolicyDenied);
    }
    if session.scope.is_some() || session.data_classes != [floe_agent_contract::DataClass::Personal]
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn current_continuation_run(session: &AgentSession) -> Option<RunId> {
    if session.active_turn.is_some() {
        return None;
    }
    RunId::from_uuid(session.continuation.as_ref()?.turn_id)
}

fn issue_continuation(
    actor: &OwnerActor,
    source: &RunReceipt,
) -> Result<Option<ContinuationToken>, AgentFailure> {
    source.validate()?;
    if source.principal != actor.person_id.to_string() {
        return Err(AgentFailure::PolicyDenied);
    }
    if source.device_id != actor.device_id {
        return Ok(None);
    }
    let Some(reference) = source.continuation() else {
        return Ok(None);
    };
    let mut digest = Sha256::new();
    digest.update(b"floe.conversation.continuation.v1\0");
    digest.update(actor.person_id.0.as_bytes());
    digest.update((actor.device_id.len() as u64).to_be_bytes());
    digest.update(actor.device_id.as_bytes());
    digest.update(source.session_id.as_bytes());
    digest.update(source.run_id.as_uuid().as_bytes());
    digest.update(source.session_revision.to_be_bytes());
    digest.update(reference.executor_generation.to_be_bytes());
    digest.update([reference.level]);
    let bytes: [u8; 32] = digest.finalize().into();
    let mut id = [0; 16];
    id.copy_from_slice(&bytes[..16]);
    id[6] = (id[6] & 0x0f) | 0x80;
    id[8] = (id[8] & 0x3f) | 0x80;
    Ok(Some(ContinuationToken {
        id: Uuid::from_bytes(id),
    }))
}
