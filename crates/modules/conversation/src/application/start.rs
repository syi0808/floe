//! Owner-issued continuation references and canonical product turn preparation.
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor, RunId};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AgentSession, CanonicalTurnIntent, CommandQuery, ContinuationToken, ConversationRepository,
    RunReceipt, SessionStore, StartTurn, TurnMode,
};

use super::session_projection::{SessionSnapshot, project_session_snapshot};

pub struct PreparedStartTurn {
    pub session: Option<AgentSession>,
    pub intent: CanonicalTurnIntent,
    pub existing: Option<RunReceipt>,
}

pub async fn prepare_start_turn<R: ConversationRepository + ?Sized, S: SessionStore>(
    repository: &R,
    sessions: &S,
    actor: &OwnerActor,
    request: &StartTurn,
    scope: &ExecutionScope,
) -> Result<PreparedStartTurn, CommandFailure<AgentFailure>> {
    actor.validate().map_err(CommandFailure::NotAdmitted)?;
    if !request.command_id.is_valid() {
        return Err(CommandFailure::NotApplied(AgentFailure::InvalidInput));
    }

    // The command receipt is the replay boundary. Read it before loading or
    // validating mutable Session state so a lost-ACK retry can recover after
    // the Session has advanced.
    let existing = scope
        .run(repository.find_command(CommandQuery {
            principal: actor.person_id.to_string(),
            command_id: request.command_id,
        }))
        .await
        .map_err(CommandFailure::Indeterminate)?;
    if let Some(receipt) = existing {
        let intent = async {
            if receipt.principal != actor.person_id.to_string()
                || receipt.device_id != actor.device_id
                || receipt.session_id != request.session_id
                || receipt.resume_of.is_some()
            {
                return Err(AgentFailure::Conflict);
            }
            receipt.validate()?;
            let text = crate::normalize_turn_text(&request.text)?;
            let mode = match (&request.continuation_ref, receipt.continuation_of) {
                (Some(token), Some(source_run_id)) => {
                    let source = scope
                        .run(repository.load_receipt(source_run_id))
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    if source.session_id != request.session_id
                        || issue_continuation(actor, &source)? != Some(token.clone())
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    TurnMode::Continue(
                        source
                            .continuation()
                            .ok_or(AgentFailure::StorageUnavailable)?,
                    )
                }
                (None, None) => TurnMode::New,
                _ => return Err(AgentFailure::Conflict),
            };
            let intent = CanonicalTurnIntent {
                session_id: request.session_id,
                expected_revision: request.expected_revision,
                text,
                mode,
                retry_of: request.retry_of,
            };
            if receipt.request_digest != intent.digest(&actor.person_id.to_string())? {
                return Err(AgentFailure::Conflict);
            }
            Ok(intent)
        }
        .await
        .map_err(CommandFailure::Indeterminate)?;
        return Ok(PreparedStartTurn {
            session: None,
            intent,
            existing: Some(receipt),
        });
    }

    let occupant = scope
        .run(repository.command_occupant(request.command_id))
        .await
        .map_err(CommandFailure::Indeterminate)?;
    if occupant.is_some() {
        return Err(CommandFailure::Indeterminate(AgentFailure::Conflict));
    }

    request.validate().map_err(CommandFailure::NotApplied)?;
    let prepared = async {
        let session = scope
            .run(sessions.load(actor.person_id, request.session_id))
            .await?;
        validate_session(actor, &session, request.session_id)?;
        if session.revision != request.expected_revision {
            return Err(AgentFailure::Conflict);
        }
        let text = crate::normalize_turn_text(&request.text)?;
        let mode = if let Some(reference) = &request.continuation_ref {
            let run_id = current_continuation_run(&session).ok_or(AgentFailure::Conflict)?;
            let source = scope
                .run(repository.load_receipt(run_id))
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let expected = issue_continuation(actor, &source)?.ok_or(AgentFailure::Conflict)?;
            if expected != *reference
                || source.session_id != request.session_id
                || source.session_revision != session.revision
            {
                return Err(AgentFailure::Conflict);
            }
            let original = scope
                .run(
                    repository
                        .read_session_user_message(request.session_id, source.user_message_id),
                )
                .await?
                .and_then(|entry| match entry.message {
                    crate::AgentMessage::User { text, .. } => Some(text),
                    _ => None,
                })
                .ok_or(AgentFailure::Conflict)?;
            if original != text {
                return Err(AgentFailure::Conflict);
            }
            TurnMode::Continue(source.continuation().ok_or(AgentFailure::Conflict)?)
        } else {
            TurnMode::New
        };
        let intent = CanonicalTurnIntent {
            session_id: request.session_id,
            expected_revision: request.expected_revision,
            text,
            mode,
            retry_of: request.retry_of,
        };
        Ok::<_, AgentFailure>(PreparedStartTurn {
            session: Some(session),
            intent,
            existing: None,
        })
    }
    .await
    .map_err(CommandFailure::NotApplied)?;
    Ok(prepared)
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
    let history_page = scope
        .run(repository.read_session_history_page(
            session_id,
            before_message_id,
            256,
            2 * 1024 * 1024,
        ))
        .await?;
    project_session_snapshot(session, continuation_ref, history_page)
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
