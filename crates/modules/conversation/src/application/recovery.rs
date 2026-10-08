use crate::{AdmittedTurn, ContinuationSnapshot, JournalEntry, RunReceipt, RunState};
use floe_agent_contract::{
    AgentFailure, AgentMessage, BatchCursor, JournalEvent, MessageRole, ModelConversation,
    ModelConversationEntry, ModelSelectionState, ValidatedModelBatch,
};
pub use floe_agent_runtime::JournalLineage;
use floe_agent_runtime::JournalProjection;

pub const MAX_CONTINUATION_JOURNAL_ENTRIES: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelSelectionJournalProjection {
    pub selection: ModelSelectionState,
    pub lineage: JournalLineage,
    pub pending_batch: Option<ValidatedModelBatch>,
    pub cursor: Option<BatchCursor>,
}

/// The shared fold state for one oldest-to-newest continuation chain.
/// `execution_selection` remains pinned for the current Run after its claimed
/// batch finishes; `carried_selection` resets when there is no pending batch,
/// so a later Run begins a fresh execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeLineageFold {
    pub pending: Option<(ValidatedModelBatch, BatchCursor)>,
    pub carried_selection: ModelSelectionState,
    pub execution_selection: ModelSelectionState,
}

impl Default for ResumeLineageFold {
    fn default() -> Self {
        Self {
            pending: None,
            carried_selection: ModelSelectionState::Fresh,
            execution_selection: ModelSelectionState::Fresh,
        }
    }
}

/// Advance pending-batch and pin state for one journal projection in an
/// oldest-to-newest ancestry fold. Conversation recovery and the durable Run
/// append owner use this same transition.
pub fn fold_resume_lineage(
    previous: &ResumeLineageFold,
    lineage: &JournalLineage,
    live: Option<(ValidatedModelBatch, BatchCursor)>,
    local_selection: &ModelSelectionState,
) -> Result<ResumeLineageFold, AgentFailure> {
    let had_pending = previous.pending.is_some();
    let execution_selection =
        if had_pending && matches!(lineage, JournalLineage::ResumeClaimed { .. }) {
            reconcile_resume_selection(&previous.carried_selection, local_selection)?
        } else {
            local_selection.clone()
        };
    let pending = reconcile_resume_lineage(previous.pending.clone(), lineage, live)?;
    let carried_selection = if pending.is_none() {
        ModelSelectionState::Fresh
    } else if had_pending {
        reconcile_resume_selection(&previous.carried_selection, local_selection)?
    } else {
        local_selection.clone()
    };
    Ok(ResumeLineageFold {
        pending,
        carried_selection,
        execution_selection,
    })
}

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
    let model_selection = if projected.pending_batch.is_some() {
        match projected.model_selection.clone() {
            floe_agent_contract::ModelSelectionState::Fresh => {
                // This projection has no parent lineage from which to prove
                // a batch-only resume's original model selection.
                floe_agent_contract::ModelSelectionState::Unproven
            }
            state => state,
        }
    } else {
        floe_agent_contract::ModelSelectionState::Fresh
    };
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
        model_selection,
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

/// Project only the selection and resume evidence needed by an owner append
/// transaction. This keeps historical decoding on the same permissive path.
pub fn project_model_selection_journal(
    source: &RunReceipt,
    entries: &[JournalEntry],
) -> Result<ModelSelectionJournalProjection, AgentFailure> {
    let projection = match source.state {
        RunState::Working => project_active_journal(source, entries)?,
        RunState::TimedOut | RunState::Failed => project_journal(source, entries)?,
        _ => return Err(AgentFailure::Conflict),
    };
    Ok(ModelSelectionJournalProjection {
        selection: projection.model_selection,
        lineage: projection.lineage,
        pending_batch: projection.pending_batch,
        cursor: projection.cursor,
    })
}

/// Restore a pin only when the child's existing journal proves an exact
/// pending-batch takeover. Unproven evidence remains fail-closed.
pub fn reconcile_resume_selection(
    prior: &ModelSelectionState,
    local: &ModelSelectionState,
) -> Result<ModelSelectionState, AgentFailure> {
    use ModelSelectionState::{Fresh, Pinned, Unproven};
    match (prior, local) {
        (Pinned(previous), Pinned(current)) if previous == current => Ok(Pinned(previous.clone())),
        (Pinned(_), Pinned(_)) => Err(AgentFailure::StorageUnavailable),
        (Pinned(previous), Fresh) => Ok(Pinned(previous.clone())),
        (Pinned(_), Unproven) => Ok(Unproven),
        (Unproven, Pinned(_)) => Err(AgentFailure::StorageUnavailable),
        (Unproven, Fresh | Unproven) => Ok(Unproven),
        (Fresh, state) => Ok(state.clone()),
    }
}

/// Oldest-to-newest pending-batch reconciliation shared by recovery and the
/// durable Run owner append path.
pub fn reconcile_resume_lineage(
    carried: Option<(ValidatedModelBatch, BatchCursor)>,
    lineage: &JournalLineage,
    live: Option<(ValidatedModelBatch, BatchCursor)>,
) -> Result<Option<(ValidatedModelBatch, BatchCursor)>, AgentFailure> {
    match (carried, lineage) {
        (None, JournalLineage::Empty | JournalLineage::Fresh) => Ok(live),
        (None, JournalLineage::ResumeBatchOnly { .. } | JournalLineage::ResumeClaimed { .. }) => {
            Err(AgentFailure::StorageUnavailable)
        }
        (Some(parent), JournalLineage::Empty) => Ok(Some(parent)),
        (Some((parent_batch, parent_cursor)), JournalLineage::ResumeBatchOnly { batch }) => {
            if *batch == parent_batch {
                Ok(Some((parent_batch, parent_cursor)))
            } else {
                Err(AgentFailure::StorageUnavailable)
            }
        }
        (Some((parent_batch, parent_cursor)), JournalLineage::ResumeClaimed { batch, cursor }) => {
            if *batch == parent_batch && *cursor == parent_cursor {
                Ok(live)
            } else {
                Err(AgentFailure::StorageUnavailable)
            }
        }
        (Some(_), JournalLineage::Fresh) => Err(AgentFailure::StorageUnavailable),
    }
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
