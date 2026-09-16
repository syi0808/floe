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
