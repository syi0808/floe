//! Owner-issued continuation references and canonical product turn preparation.
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, CommandFailure, OwnerActor, RunId};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    AgentSession, CanonicalTurnIntent, CommandQuery, ContinuationToken, ContinueTurn,
    ConversationRepository, RunReceipt, SessionStore, TurnMode,
};

use super::session_projection::{SessionSnapshot, project_session_snapshot};

pub struct PreparedContinueTurn {
    pub session: Option<AgentSession>,
    pub intent: CanonicalTurnIntent,
    pub existing: Option<RunReceipt>,
}

pub async fn prepare_continue_turn<R: ConversationRepository + ?Sized, S: SessionStore>(
    repository: &R,
    sessions: &S,
    actor: &OwnerActor,
    request: &ContinueTurn,
    scope: &ExecutionScope,
) -> Result<PreparedContinueTurn, CommandFailure<AgentFailure>> {
    actor.validate().map_err(CommandFailure::NotAdmitted)?;
    request.validate().map_err(CommandFailure::NotApplied)?;
    // Exact receipt lookup precedes mutable Session checks. A later Run or
    // semantic compaction cannot invalidate recovery of a lost positive ACK.
    let existing = scope
        .run(repository.find_command(CommandQuery {
            principal: actor.person_id.to_string(),
            command_id: request.command_id,
        }))
        .await
        .map_err(CommandFailure::Indeterminate)?;
    if let Some(receipt) = existing {
        let intent = async {
            receipt.validate()?;
            if receipt.principal != actor.person_id.to_string()
                || receipt.device_id != actor.device_id
                || receipt.session_id != request.session_id
                || receipt.resume_of.is_some()
                || receipt.retry_of.is_some()
            {
                return Err(AgentFailure::Conflict);
            }
            let parent = receipt.continuation_of.ok_or(AgentFailure::Conflict)?;
            let source = scope
                .run(repository.load_receipt(parent))
                .await?
                .ok_or(AgentFailure::Conflict)?;
            if issue_continuation(actor, &source)?.as_ref() != Some(&request.continuation_ref) {
                return Err(AgentFailure::Conflict);
            }
            let text = original_text(
                repository,
                request.session_id,
                receipt.user_message_id,
                scope,
            )
            .await?;
            let intent = CanonicalTurnIntent {
                session_id: request.session_id,
                expected_revision: request.expected_revision,
                text,
                mode: TurnMode::Continue(source.continuation().ok_or(AgentFailure::Conflict)?),
                retry_of: None,
            };
            if intent.digest(&actor.person_id.to_string())? != receipt.request_digest {
                return Err(AgentFailure::Conflict);
            }
            Ok(intent)
        }
        .await
        .map_err(CommandFailure::Indeterminate)?;
        return Ok(PreparedContinueTurn {
            session: None,
            intent,
            existing: Some(receipt),
        });
    }
    if scope
        .run(repository.command_occupant(request.command_id))
        .await
        .map_err(CommandFailure::Indeterminate)?
        .is_some()
    {
        return Err(CommandFailure::Indeterminate(AgentFailure::Conflict));
    }
    async {
        let session = scope
            .run(sessions.load(actor.person_id, request.session_id))
            .await?;
        validate_session(actor, &session, request.session_id)?;
        if session.revision != request.expected_revision {
            return Err(AgentFailure::Conflict);
        }
        let run_id = current_continuation_run(&session).ok_or(AgentFailure::Conflict)?;
        let source = scope
            .run(repository.load_receipt(run_id))
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if issue_continuation(actor, &source)?.as_ref() != Some(&request.continuation_ref)
            || source.session_id != request.session_id
            || source.session_revision != session.revision
        {
            return Err(AgentFailure::Conflict);
        }
        let text = original_text(
            repository,
            request.session_id,
            source.user_message_id,
            scope,
        )
        .await?;
        let intent = CanonicalTurnIntent {
            session_id: request.session_id,
            expected_revision: request.expected_revision,
            text,
            mode: TurnMode::Continue(source.continuation().ok_or(AgentFailure::Conflict)?),
            retry_of: None,
        };
        Ok(PreparedContinueTurn {
            session: Some(session),
            intent,
            existing: None,
        })
    }
    .await
    .map_err(CommandFailure::NotApplied)
}

async fn original_text<R: ConversationRepository + ?Sized>(
    repository: &R,
    session_id: Uuid,
    message_id: Uuid,
    scope: &ExecutionScope,
) -> Result<String, AgentFailure> {
    let entry = scope
        .run(repository.read_session_user_message(session_id, message_id))
        .await?
        .ok_or(AgentFailure::Conflict)?;
    let crate::AgentMessage::User { text, .. } = entry.message else {
        return Err(AgentFailure::Conflict);
    };
    if crate::normalize_turn_text(&text)? != text {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(text)
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
