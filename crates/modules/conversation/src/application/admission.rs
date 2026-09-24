//! Conversation-owned admission judgment for one caller turn.
//!
//! A caller-facing command path has to know, before the durable turn is
//! submitted, whether the request may be accepted and whether it continues an
//! earlier Run. That judgment reads Session and Run state, so it belongs to
//! Conversation; the composition root only forwards the verified caller.

use floe_kernel::{AgentFailure, CommandId};
use uuid::Uuid;

use crate::{
    CommandQuery, ContinuationRef, ConversationRepository, InteractionResumeRef, RunQuery,
    RunReceipt, TurnMode,
};

/// One caller request to admit, in Conversation's own terms.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnPrecheckRequest {
    pub principal: String,
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub mode: TurnMode,
}

impl TurnPrecheckRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.command_id.is_valid() || self.session_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        match &self.mode {
            TurnMode::New => {}
            TurnMode::Continue(reference) => reference.validate()?,
            TurnMode::Resume(reference) => reference.validate()?,
        }
        Ok(())
    }
}

/// What an admitted request means for the durable turn that follows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnPrecheck {
    /// The receipt already recorded for this command, when the command repeats.
    pub existing: Option<RunReceipt>,
    /// Whether the turn continues an earlier Run of the same Session.
    pub continuation: bool,
}

/// Admit one turn request against the recorded command and Run state.
///
/// A repeated command must describe the same continuation it was first
/// admitted with; a first-time continuation must name a terminal Run of the
/// same Session at the expected executor generation and the next level.
pub async fn precheck_turn<Repository: ConversationRepository>(
    repository: &Repository,
    request: TurnPrecheckRequest,
) -> Result<TurnPrecheck, AgentFailure> {
    request.validate()?;
    let existing = super::query::get_command(
        repository,
        CommandQuery {
            principal: request.principal.clone(),
            command_id: request.command_id,
        },
    )
    .await?;
    let continuation = match &request.mode {
        TurnMode::New => false,
        TurnMode::Continue(reference) => {
            match existing.as_ref() {
                Some(receipt) => verify_recorded(receipt, request.session_id, reference)?,
                None => verify_source(repository, &request, reference).await?,
            }
            true
        }
        TurnMode::Resume(reference) => {
            match existing.as_ref() {
                Some(receipt) => verify_recorded_resume(receipt, request.session_id, reference)?,
                None => verify_resume_source(repository, &request, reference).await?,
            }
            true
        }
    };
    Ok(TurnPrecheck {
        existing,
        continuation,
    })
}

/// The command was admitted before: it must describe the same continuation.
fn verify_recorded(
    receipt: &RunReceipt,
    session_id: Uuid,
    reference: &ContinuationRef,
) -> Result<(), AgentFailure> {
    if receipt.session_id != session_id
        || receipt.continuation_of != Some(reference.run_id)
        || receipt.continuation_executor_generation != Some(reference.executor_generation)
        || receipt.continuation_level != reference.level
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

/// The command was admitted before: it must describe the same resume linkage.
fn verify_recorded_resume(
    receipt: &RunReceipt,
    session_id: Uuid,
    reference: &InteractionResumeRef,
) -> Result<(), AgentFailure> {
    if receipt.session_id != session_id
        || receipt.resume_of != Some(reference.origin_run_id)
        || receipt.resume_lineage != reference.lineage
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

/// A first-time resume: the named origin must actually admit this child.
///
/// The receipt's own linkage rule decides: Completed, at the next lineage
/// depth, with chain depth left. The interaction group gate is re-verified
/// atomically at admission, never here.
async fn verify_resume_source<Repository: ConversationRepository>(
    repository: &Repository,
    request: &TurnPrecheckRequest,
    reference: &InteractionResumeRef,
) -> Result<(), AgentFailure> {
    let origin = super::query::get_run(
        repository,
        RunQuery {
            principal: request.principal.clone(),
            run_id: reference.origin_run_id,
        },
    )
    .await?
    .ok_or(AgentFailure::NotFound)?;
    if origin.session_id != request.session_id || origin.resume().as_ref() != Some(reference) {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

/// A first-time continuation: the named Run must actually be continuable.
async fn verify_source<Repository: ConversationRepository>(
    repository: &Repository,
    request: &TurnPrecheckRequest,
    reference: &ContinuationRef,
) -> Result<(), AgentFailure> {
    let source = super::query::get_run(
        repository,
        RunQuery {
            principal: request.principal.clone(),
            run_id: reference.run_id,
        },
    )
    .await?
    .ok_or(AgentFailure::NotFound)?;
    if source.session_id != request.session_id
        || !source.state.is_terminal()
        || source.executor_generation != reference.executor_generation
        || source.continuation_level.checked_add(1) != Some(reference.level)
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

/// One caller turn, as the host asks Conversation to prepare it.
pub struct TurnPreparationRequest<'a> {
    pub principal: String,
    pub person_id: floe_kernel::PersonId,
    pub command_id: CommandId,
    pub session_id: Uuid,
    pub expected_revision: u64,
    /// What the Person asked for, and which of their devices asked.
    pub text: &'a str,
    pub device_id: &'a str,
    /// Whether the caller says this turn continues the Session's last Run.
    pub continuation: bool,
    /// Which results in the transcript carry source data.
    pub boundary: &'a dyn floe_agent_contract::SourceHistoryBoundary,
}

/// The Session a prepared turn runs against, and the Run it continues.
pub struct PreparedTurn {
    pub session: crate::turn::AgentSession,
    pub mode: TurnMode,
}

/// The longest device identity a turn may name.
const MAX_DEVICE_ID_BYTES: usize = 128;

/// Prepare one root turn against the Session it names.
///
/// Whether the Session may take this turn at all is Conversation's: a scoped
/// Session or one holding anything but the Person's own data is not a root
/// conversation, a continuation has to meet the revision it was admitted at
/// unless the command already exists, and a continuation whose transcript still
/// carries source-derived history is reading something the new turn is not
/// authorized for.
pub async fn prepare_turn<Repository, Store>(
    repository: &Repository,
    sessions: &Store,
    request: TurnPreparationRequest<'_>,
) -> Result<PreparedTurn, AgentFailure>
where
    Repository: ConversationRepository,
    Store: crate::turn::SessionStore,
{
    let text = request.text.trim();
    if text.is_empty()
        || text.len() > crate::domain::MAX_TURN_TEXT_BYTES
        || request.device_id.trim().is_empty()
        || request.device_id.len() > MAX_DEVICE_ID_BYTES
    {
        return Err(AgentFailure::InvalidInput);
    }
    let session = sessions.load(request.person_id, request.session_id).await?;
    let existing = if request.continuation {
        super::query::get_command(
            repository,
            CommandQuery {
                principal: request.principal.clone(),
                command_id: request.command_id,
            },
        )
        .await?
    } else {
        None
    };
    if session.scope.is_some()
        || session.data_classes != [floe_agent_contract::DataClass::Personal]
        || (request.continuation
            && session.revision != request.expected_revision
            && existing.is_none())
    {
        return Err(AgentFailure::Conflict);
    }
    if request.continuation
        && crate::turn::carries_source_history(&session.messages, request.boundary)
    {
        return Err(AgentFailure::StaleContext);
    }
    let mode = if request.continuation {
        TurnMode::Continue(continued_run(repository, &request, &session, existing).await?)
    } else {
        TurnMode::New
    };
    Ok(PreparedTurn { session, mode })
}

/// The Run a continuation resumes.
///
/// A command that was already admitted says which Run it continues; a
/// first-time continuation takes the Session's own last Run, which has to be
/// exactly one level behind the snapshot Conversation projects for it.
async fn continued_run<Repository: ConversationRepository>(
    repository: &Repository,
    request: &TurnPreparationRequest<'_>,
    session: &crate::turn::AgentSession,
    existing: Option<RunReceipt>,
) -> Result<ContinuationRef, AgentFailure> {
    if let Some(receipt) = existing {
        return Ok(ContinuationRef {
            run_id: receipt.continuation_of.ok_or(AgentFailure::Conflict)?,
            executor_generation: receipt
                .continuation_executor_generation
                .ok_or(AgentFailure::Conflict)?,
            level: receipt.continuation_level,
        });
    }
    let reference = session
        .continuation
        .as_ref()
        .ok_or(AgentFailure::Conflict)?;
    let snapshot = super::coordinator::continuation(
        repository,
        floe_kernel::RunId::from_uuid(reference.turn_id).ok_or(AgentFailure::Conflict)?,
        &request.principal,
    )
    .await?;
    if reference.level.checked_add(1) != Some(snapshot.reference.level) {
        return Err(AgentFailure::Conflict);
    }
    Ok(snapshot.reference)
}

/// One linked-resume request to prepare, in Conversation's own terms.
///
/// The caller names only the origin linkage. The original text, the profile
/// and the mode all come from the origin's own durable admission: no caller
/// resends text, chooses a parent Run or injects a second user message.
pub struct ResumePreparationRequest {
    pub principal: String,
    pub person_id: floe_kernel::PersonId,
    pub session_id: Uuid,
    pub resume: InteractionResumeRef,
}

/// A prepared resume: the Session, the owner-derived origin intent, and
/// the origin itself. The child keeps the origin's profile and, on the
/// automatic path, admits at the origin's Session revision; an explicit
/// Continue admits at the current Session revision instead.
pub struct PreparedResume {
    pub session: crate::turn::AgentSession,
    pub mode: TurnMode,
    pub text: String,
    pub origin: RunReceipt,
}

/// Prepare one linked resume against the origin it names.
///
/// The origin must be Completed in this Session at exactly the named
/// lineage; its text is read back from the live Session transcript, chasing
/// resume links to the New root. A continuation origin, a compacted-away or
/// deleted origin message, or a chain deeper than admission allows fails
/// closed: the child is never admitted with invented text.
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
    if session.scope.is_some()
        || session.data_classes != [floe_agent_contract::DataClass::Personal]
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

/// The origin's canonical text, chased to the New root that spoke it.
///
/// Resume children push no Session message of their own, so a resume origin
/// resolves through its own origin link. Continuation turns never persisted
/// their new text, and a compacted-away message is gone: both fail closed
/// rather than inventing the intent the child would execute.
async fn derive_origin_text<Repository: ConversationRepository>(
    repository: &Repository,
    request: &ResumePreparationRequest,
    session: &crate::turn::AgentSession,
    origin: &RunReceipt,
) -> Result<String, AgentFailure> {
    let mut current = origin.clone();
    for _ in 0..=crate::MAX_RESUME_LINEAGE {
        let Some(parent_id) = current.resume_of else {
            break;
        };
        current = super::query::get_run(
            repository,
            RunQuery {
                principal: request.principal.clone(),
                run_id: parent_id,
            },
        )
        .await?
        .ok_or(AgentFailure::Conflict)?;
        if current.session_id != request.session_id {
            return Err(AgentFailure::Conflict);
        }
    }
    if current.resume_of.is_some() || current.continuation_of.is_some() {
        return Err(AgentFailure::Conflict);
    }
    session
        .messages
        .iter()
        .find_map(|message| match message {
            crate::turn::AgentMessage::User { turn_id, text }
                if *turn_id == current.run_id.as_uuid() =>
            {
                Some(text.clone())
            }
            _ => None,
        })
        .ok_or(AgentFailure::Conflict)
}
