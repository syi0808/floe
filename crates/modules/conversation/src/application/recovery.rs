use crate::{AdmittedTurn, ContinuationSnapshot, JournalEntry, RunReceipt, RunState};
use floe_agent_contract::{
    AgentMessage, JournalEvent, MessageRole, ModelConversation, ModelConversationEntry,
};
pub(super) use floe_agent_runtime::{JournalLineage, JournalProjection};
use floe_kernel::AgentFailure;

/// Durable transcript history as basic typed model history.
///
/// User, preamble, and assistant messages map directly. A delegation summary in
/// the transcript is already lossy text, so it stays a lossy assistant entry.
/// Tool messages never reached the model from history on the old path either
/// (history capabilities were dropped at the wire), so they are skipped rather
/// than rebuilt with invented inputs.
pub(super) fn project_transcript_history(
    transcript: &[AgentMessage],
) -> Result<Vec<ModelConversationEntry>, AgentFailure> {
    transcript.iter().try_for_each(AgentMessage::validate)?;
    Ok(transcript
        .iter()
        .filter_map(|message| match message.role {
            MessageRole::User => Some(ModelConversationEntry::User {
                message_id: message.message_id,
                text: message.text.clone(),
            }),
            MessageRole::Preamble => Some(ModelConversationEntry::Preamble {
                message_id: message.message_id,
                text: message.text.clone(),
            }),
            MessageRole::Assistant | MessageRole::Delegation => {
                Some(ModelConversationEntry::Assistant {
                    message_id: message.message_id,
                    text: message.text.clone(),
                })
            }
            MessageRole::Tool => None,
        })
        .collect())
}

pub fn project_continuation(
    admitted: &AdmittedTurn,
    entries: &[JournalEntry],
) -> Result<ContinuationSnapshot, AgentFailure> {
    admitted.validate()?;
    let projected = project_journal(&admitted.receipt, entries)?;
    let history = project_transcript_history(&admitted.transcript)?;
    let model_conversation = ModelConversation {
        history,
        current_turn: projected.model_conversation.current_turn,
    };
    if model_conversation.len() > floe_agent_contract::MAX_AGENT_MESSAGES
        || projected.replay.len() > 128
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(ContinuationSnapshot {
        user_message_id: admitted.receipt.user_message_id,
        expert_environment: admitted.receipt.expert_environment,
        reference: admitted
            .receipt
            .continuation()
            .ok_or(AgentFailure::Conflict)?,
        session_id: admitted.receipt.session_id,
        session_revision: admitted.receipt.session_revision,
        model_conversation,
        replay: projected.replay,
        pending_batch: projected.pending_batch,
        batch_cursor: projected.cursor,
        completed_iterations: projected.completed_iterations,
        usage: projected.usage,
    })
}

pub(super) fn project_journal(
    source: &RunReceipt,
    entries: &[JournalEntry],
) -> Result<JournalProjection, AgentFailure> {
    source.validate()?;
    if !matches!(
        (&source.state, source.issue),
        (RunState::TimedOut, Some(AgentFailure::DeadlineExceeded))
            | (RunState::Failed, Some(AgentFailure::BudgetExceeded))
    ) || entries.len() > 512
    {
        return Err(AgentFailure::Conflict);
    }
    project_entries(source, entries, false)
}

pub(super) fn project_active_journal(
    source: &RunReceipt,
    entries: &[JournalEntry],
) -> Result<JournalProjection, AgentFailure> {
    source.validate()?;
    if source.state != RunState::Working
        || source.output.is_some()
        || source.issue.is_some()
        || entries.len() > 512
    {
        return Err(AgentFailure::Conflict);
    }
    project_entries(source, entries, true)
}

pub(crate) fn journal_binding(
    source: &RunReceipt,
    entries: &[JournalEntry],
) -> floe_agent_runtime::JournalExecutionBinding {
    let execution_id = match entries.first().map(|entry| &entry.event) {
        Some(JournalEvent::ValidatedBatch { batch }) => batch.execution_id,
        _ => source.run_id.as_uuid(),
    };
    floe_agent_runtime::JournalExecutionBinding {
        principal: source.principal.clone(),
        device_id: source.device_id.clone(),
        execution_id,
        catalog_revision: source.expert_environment.revision,
        root_run_id: Some(source.run_id),
        owning_task_id: None,
    }
}

fn project_entries(
    source: &RunReceipt,
    entries: &[JournalEntry],
    durable: bool,
) -> Result<JournalProjection, AgentFailure> {
    floe_agent_runtime::project_execution_journal(
        &journal_binding(source, entries),
        entries,
        if durable {
            floe_agent_runtime::JournalProjectionMode::DurablePrefix
        } else {
            floe_agent_runtime::JournalProjectionMode::Recoverable
        },
    )
}

/// Root admission/continuation policy remains here; event ordering, reserved
/// settlement capacity and accounting are validated by the common runtime.
pub fn validate_run_journal(
    source: &RunReceipt,
    entries: &[JournalEntry],
) -> Result<(), AgentFailure> {
    source.validate()?;
    project_entries(source, entries, true)?;
    Ok(())
}
