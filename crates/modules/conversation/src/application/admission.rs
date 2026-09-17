//! Conversation-owned admission judgment for one caller turn.
//!
//! A caller-facing command path has to know, before the durable turn is
//! submitted, whether the request may be accepted and whether it continues an
//! earlier Run. That judgment reads Session and Run state, so it belongs to
//! Conversation; the composition root only forwards the verified caller.

use floe_kernel::{AgentFailure, CommandId};
use uuid::Uuid;

use crate::{
    CommandQuery, ContinuationRef, ConversationRepository, RunQuery, RunReceipt, TurnMode,
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
        if let TurnMode::Continue(reference) = &self.mode {
            reference.validate()?;
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
    let session = sessions
        .load(request.person_id, request.session_id)
        .await?;
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
