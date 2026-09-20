use std::collections::{HashMap, HashSet};

use floe_agent_contract::{
    AgentMessage, BatchCursor, DependencyCoverage, JournalEvent, MessageRole, ModelConversation,
    ModelConversationEntry, ModelStep, ProjectionRef, ReplayReceipt, TaskState,
    ValidatedModelBatch, input_digest,
};
use floe_kernel::AgentFailure;
use uuid::Uuid;

use crate::{AdmittedTurn, ContinuationSnapshot, JournalEntry, RunReceipt, RunState};

pub(super) struct JournalProjection {
    pub(super) model_conversation: ModelConversation,
    pub(super) replay: Vec<ReplayReceipt>,
    pub(super) pending_batch: Option<ValidatedModelBatch>,
    pub(super) cursor: Option<BatchCursor>,
    pub(super) completed_iterations: u32,
    pub(super) usage: floe_execution::budget::ModelUsage,
    pub(super) lineage: JournalLineage,
}

/// How this run's journal began. A child takes over a parent's pending batch
/// only after durably re-recording the exact batch and its starting cursor;
/// a batch re-record without its initial cursor is not a takeover.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum JournalLineage {
    Empty,
    Fresh,
    ResumeBatchOnly { batch: ValidatedModelBatch },
    ResumeClaimed {
        batch: ValidatedModelBatch,
        cursor: BatchCursor,
    },
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
    Ok(ContinuationSnapshot {
        reference: admitted
            .receipt
            .continuation()
            .ok_or(AgentFailure::Conflict)?,
        session_id: admitted.receipt.session_id,
        session_revision: admitted.receipt.session_revision,
        profile: admitted.receipt.profile.clone(),
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
    project_entries(source, entries)
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
    project_entries(source, entries)
}

struct PendingTool {
    ordinal: u32,
    call: floe_agent_contract::ToolCall,
}

struct PendingDelegation {
    ordinal: u32,
    request: floe_agent_contract::DelegationRequest,
}

struct PendingBatch {
    batch: ValidatedModelBatch,
    /// Whether the batch's attempt completed in this journal. A batch without
    /// a local attempt is a resumed batch re-recorded at the journal's start.
    fresh: bool,
    /// Last acknowledged cursor, if any progress was recorded.
    cursor: Option<u32>,
    /// Ordinal settled by an intent/result pair but not yet consumed by a
    /// cursor advance. A crash between result and cursor ack leaves this set.
    settled_step: Option<u32>,
}

impl PendingBatch {
    fn next_index(&self) -> usize {
        self.cursor.unwrap_or(0) as usize
    }

    fn next_step(&self) -> Result<&ModelStep, AgentFailure> {
        self.batch
            .steps
            .get(self.next_index())
            .ok_or(AgentFailure::StorageUnavailable)
    }
}

/// One model attempt's projection binding and terminal state. A validated
/// batch must name the attempt and projection its own intent recorded. One
/// attempt binds at most one fresh batch; a second fresh batch on the same
/// attempt is journal corruption.
struct AttemptState {
    projection_ref: ProjectionRef,
    completed: bool,
    batch_bound: bool,
}

fn project_entries(
    source: &RunReceipt,
    entries: &[JournalEntry],
) -> Result<JournalProjection, AgentFailure> {
    let mut exchanges = Vec::new();
    let mut replay = Vec::new();
    let mut attempts: HashMap<Uuid, AttemptState> = HashMap::new();
    let mut tools: HashMap<Uuid, PendingTool> = HashMap::new();
    let mut seen_calls = HashSet::new();
    let mut delegations: HashMap<floe_kernel::TaskId, PendingDelegation> = HashMap::new();
    let mut seen_tasks = HashSet::new();
    let mut seen_invocations = HashSet::new();
    let mut seen_batches = HashSet::new();
    let mut batches_seen: u32 = 0;
    let mut execution_id: Option<Uuid> = None;
    let mut fresh_catalog_revision: Option<u64> = None;
    let mut pending: Option<PendingBatch> = None;
    let mut uncheckpointed_completion = false;
    let mut completed_iterations = 0;
    let mut usage = floe_execution::budget::ModelUsage::default();
    let mut lineage = JournalLineage::Empty;
    for (index, entry) in entries.iter().enumerate() {
        if entry.revision != (index as u64) + 1 {
            return Err(AgentFailure::StorageUnavailable);
        }
        match &entry.event {
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref,
            } => {
                // A resumed batch without its initial cursor never started;
                // a local attempt before that cursor is a skipped takeover.
                if matches!(lineage, JournalLineage::ResumeBatchOnly { .. }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if attempt_id.is_nil()
                    || projection_ref.as_uuid().is_nil()
                    || attempts.contains_key(attempt_id)
                    || pending.is_some()
                    || uncheckpointed_completion
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                attempts.insert(
                    *attempt_id,
                    AttemptState {
                        projection_ref: *projection_ref,
                        completed: false,
                        batch_bound: false,
                    },
                );
                if matches!(lineage, JournalLineage::Empty) {
                    lineage = JournalLineage::Fresh;
                }
            }
            JournalEvent::ModelResult {
                attempt_id,
                usage: result_usage,
            } => {
                if matches!(lineage, JournalLineage::ResumeBatchOnly { .. }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let state = attempts
                    .get_mut(attempt_id)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if state.completed {
                    return Err(AgentFailure::StorageUnavailable);
                }
                state.completed = true;
                usage.attempts = usage
                    .attempts
                    .checked_add(1)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                usage.tokens = usage
                    .tokens
                    .checked_add(result_usage.tokens)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                usage.cost_micros = usage
                    .cost_micros
                    .checked_add(result_usage.cost_micros)
                    .ok_or(AgentFailure::StorageUnavailable)?;
            }
            JournalEvent::ToolIntent { call } => {
                // A batch-only re-record never started: cursor.unwrap_or(0)
                // must not make it look as if it started at ordinal 0.
                if matches!(lineage, JournalLineage::ResumeBatchOnly { .. }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                // The intent must exactly match the current validated step:
                // kind, payload, and stable identity. Anything else is a
                // forged journal and fails closed as storage corruption.
                let Some(state) = pending.as_ref() else {
                    return Err(AgentFailure::StorageUnavailable);
                };
                if state.settled_step.is_some() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let ordinal = state.cursor.unwrap_or(0);
                let step = state.next_step()?;
                let (step_tool_id, step_revision, step_input) = match step {
                    ModelStep::CallTool {
                        tool_id,
                        definition_revision,
                        input,
                    } => (tool_id, definition_revision, input),
                    _ => return Err(AgentFailure::StorageUnavailable),
                };
                if call.tool_id != *step_tool_id
                    || call.definition_revision != *step_revision
                    || call.input != *step_input
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let expected_key = floe_agent_runtime::stable_invocation_key(
                    state.batch.execution_id,
                    state.batch.batch_id,
                    ordinal,
                    floe_agent_runtime::InvocationKind::Tool,
                );
                let expected_call = floe_agent_runtime::stable_call_id(
                    state.batch.execution_id,
                    state.batch.batch_id,
                    ordinal,
                );
                if call.invocation_key != expected_key || call.call_id != expected_call {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if call.call_id.is_nil()
                    || call.invocation_key.as_uuid().is_nil()
                    || !seen_invocations.insert(call.invocation_key)
                    || call.tool_id.trim().is_empty()
                    || call.definition_revision == 0
                    || floe_agent_contract::validate_tool_input(&call.input).is_err()
                    || !seen_calls.insert(call.call_id)
                    || tools
                        .insert(
                            call.call_id,
                            PendingTool {
                                ordinal,
                                call: call.clone(),
                            },
                        )
                        .is_some()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            JournalEvent::ToolResult { result } => {
                if matches!(lineage, JournalLineage::ResumeBatchOnly { .. }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let settled = tools
                    .remove(&result.call_id)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let Some(state) = pending.as_mut() else {
                    return Err(AgentFailure::StorageUnavailable);
                };
                // Only the current cursor's active intent may settle, and only
                // once: the cursor has not advanced past its ordinal yet.
                if state.cursor.unwrap_or(0) != settled.ordinal
                    || state.settled_step.is_some()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let call = settled.call;
                result.validate(call.call_id, floe_agent_contract::MAX_OUTPUT_BYTES)?;
                state.settled_step = Some(settled.ordinal);
                let receipt = ReplayReceipt {
                    principal: source.principal.clone(),
                    run_id: Some(source.run_id),
                    task_id: None,
                    agent_id: None,
                    tool_id: Some(call.tool_id.clone()),
                    definition_revision: call.definition_revision,
                    input_digest: input_digest(&call.input),
                    invocation_key: call.invocation_key,
                    call_id: call.call_id,
                    result: result.text.clone(),
                    task_result: None,
                    task_state: None,
                    task_artifacts: vec![],
                    task_coverage: DependencyCoverage::Unknown,
                    task_issue: None,
                    tool_artifacts: result.artifacts.clone(),
                    tool_coverage: result.coverage.clone(),
                    tool_issue: result.issue.as_ref().map(|issue| issue.failure),
                };
                exchanges.push(ModelConversationEntry::ToolExchange {
                    call,
                    result: result.clone(),
                });
                replay.push(receipt);
            }
            JournalEvent::DelegationIntent { request } => {
                if matches!(lineage, JournalLineage::ResumeBatchOnly { .. }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                // Same binding as tools: the intent must exactly match the
                // current validated delegation step, including stable ids.
                let Some(state) = pending.as_ref() else {
                    return Err(AgentFailure::StorageUnavailable);
                };
                if state.settled_step.is_some() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let ordinal = state.cursor.unwrap_or(0);
                let step = state.next_step()?;
                let (step_agent, step_revision, step_message, step_refs) = match step {
                    ModelStep::Delegate {
                        agent_id,
                        definition_revision,
                        message,
                        context_refs,
                    } => (agent_id, definition_revision, message, context_refs),
                    _ => return Err(AgentFailure::StorageUnavailable),
                };
                if request.selected_agent_id != *step_agent
                    || request.selected_definition_revision != *step_revision
                    || request.message != *step_message
                    || request.context_refs != *step_refs
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let expected_key = floe_agent_runtime::stable_invocation_key(
                    state.batch.execution_id,
                    state.batch.batch_id,
                    ordinal,
                    floe_agent_runtime::InvocationKind::Delegation,
                );
                let expected_task = floe_agent_runtime::stable_task_id(
                    state.batch.execution_id,
                    state.batch.batch_id,
                    ordinal,
                );
                if request.invocation_key != expected_key || request.task_id != expected_task
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if !request.task_id.is_valid()
                    || request.parent_run_id != Some(source.run_id.as_uuid())
                    || request.principal != source.principal
                    || request.invocation_key.as_uuid().is_nil()
                    || !seen_invocations.insert(request.invocation_key)
                    || request.selected_agent_id.trim().is_empty()
                    || request.selected_definition_revision == 0
                    || request.message.trim().is_empty()
                    || request.message.len() > floe_agent_contract::MAX_OUTPUT_BYTES
                    || !floe_agent_contract::valid_context_refs(&request.context_refs)
                    || !seen_tasks.insert(request.task_id)
                    || delegations
                        .insert(
                            request.task_id,
                            PendingDelegation {
                                ordinal,
                                request: request.clone(),
                            },
                        )
                        .is_some()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            JournalEvent::DelegationResult { receipt } => {
                if matches!(lineage, JournalLineage::ResumeBatchOnly { .. }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let settled = delegations
                    .remove(&receipt.task_id)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let Some(state) = pending.as_mut() else {
                    return Err(AgentFailure::StorageUnavailable);
                };
                if state.cursor.unwrap_or(0) != settled.ordinal
                    || state.settled_step.is_some()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let request = settled.request;
                receipt
                    .snapshot
                    .validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
                state.settled_step = Some(settled.ordinal);
                if receipt.snapshot.task_id != request.task_id
                    || receipt.snapshot.parent_run_id != request.parent_run_id
                    || receipt.snapshot.principal != request.principal
                    || receipt.snapshot.agent_id != request.selected_agent_id
                    || receipt.snapshot.definition_revision != request.selected_definition_revision
                    || matches!(
                        receipt.snapshot.state,
                        TaskState::Submitted | TaskState::Working
                    )
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let text = receipt
                    .snapshot
                    .result
                    .clone()
                    .unwrap_or_else(|| format!("task {:?}", receipt.snapshot.state));
                exchanges.push(ModelConversationEntry::DelegationExchange {
                    request: request.clone(),
                    receipt: receipt.as_ref().clone(),
                });
                replay.push(ReplayReceipt {
                    principal: request.principal,
                    run_id: Some(source.run_id),
                    task_id: Some(request.task_id),
                    agent_id: Some(request.selected_agent_id),
                    tool_id: None,
                    definition_revision: request.selected_definition_revision,
                    input_digest: input_digest(&request.message),
                    invocation_key: request.invocation_key,
                    call_id: request.task_id.as_uuid(),
                    result: text,
                    task_result: receipt.snapshot.result.clone(),
                    task_state: Some(receipt.snapshot.state),
                    task_artifacts: receipt.snapshot.artifacts.clone(),
                    task_coverage: receipt.snapshot.coverage.clone(),
                    task_issue: receipt.snapshot.issue,
                    tool_artifacts: vec![],
                    tool_coverage: DependencyCoverage::Unknown,
                    tool_issue: None,
                });
            }
            JournalEvent::Checkpoint { iteration } => {
                if matches!(lineage, JournalLineage::ResumeBatchOnly { .. }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                // A checkpoint watermarks exactly one completed batch: no batch
                // may be pending and no completion may pass uncheckpointed.
                if pending.is_some()
                    || !uncheckpointed_completion
                    || *iteration != completed_iterations + 1
                    || *iteration > 64
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                uncheckpointed_completion = false;
                completed_iterations = *iteration;
            }
            JournalEvent::ValidatedBatch { batch } => {
                batch
                    .validate(floe_agent_contract::MAX_OUTPUT_BYTES)
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                if !seen_batches.insert(batch.batch_id)
                    || pending.is_some()
                    || uncheckpointed_completion
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                // A batch follows its attempt's result, unless it is a resumed
                // batch re-recorded at the journal's start with no local attempt.
                let fresh = attempts.contains_key(&batch.attempt_id);
                if fresh {
                    let state = attempts
                        .get_mut(&batch.attempt_id)
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    if !state.completed
                        || state.projection_ref != batch.projection_ref
                        || state.batch_bound
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    state.batch_bound = true;
                    // One drive validates every fresh batch against one
                    // catalog; only the leading resumed re-record may differ.
                    // Whether that catalog is still current is the Engine
                    // resume's pinned-revision check, not this projection's.
                    match fresh_catalog_revision {
                        Some(expected) if batch.catalog_revision != expected => {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        Some(_) => {}
                        None => fresh_catalog_revision = Some(batch.catalog_revision),
                    }
                } else if batches_seen != 0 {
                    return Err(AgentFailure::StorageUnavailable);
                }
                // Every batch in one journal — resumed re-record included —
                // runs under the execution that wrote the journal.
                match execution_id {
                    Some(expected) if batch.execution_id != expected => {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    Some(_) => {}
                    None => execution_id = Some(batch.execution_id),
                }
                batches_seen += 1;
                match &lineage {
                    JournalLineage::Empty if !fresh => {
                        lineage = JournalLineage::ResumeBatchOnly {
                            batch: batch.clone(),
                        };
                    }
                    JournalLineage::Empty => {
                        lineage = JournalLineage::Fresh;
                    }
                    JournalLineage::ResumeBatchOnly { .. }
                    | JournalLineage::Fresh
                    | JournalLineage::ResumeClaimed { .. } => {}
                }
                pending = Some(PendingBatch {
                    batch: batch.clone(),
                    fresh,
                    cursor: None,
                    settled_step: None,
                });
            }
            JournalEvent::BatchProgress { cursor } => {
                cursor
                    .validate()
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                // Exact cursor binding: the cursor is the authoritative plan
                // index, so it may only advance by exactly one, and only with
                // completion evidence for the step it leaves behind.
                let step_text: Option<(u32, String)> = {
                    let Some(state) = pending.as_mut() else {
                        return Err(AgentFailure::StorageUnavailable);
                    };
                    if cursor.batch_id != state.batch.batch_id
                        || cursor.next_step_index as usize > state.batch.steps.len()
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    match state.cursor {
                        None => {
                            // A fresh batch always starts at zero; a resumed
                            // batch restarts wherever its cursor was. Initial
                            // progress carries no in-journal evidence and
                            // rebuilds no preamble: older journals hold it.
                            if state.fresh && cursor.next_step_index != 0 {
                                return Err(AgentFailure::StorageUnavailable);
                            }
                            state.cursor = Some(cursor.next_step_index);
                            None
                        }
                        Some(last) => {
                            if cursor.next_step_index != last + 1 {
                                return Err(AgentFailure::StorageUnavailable);
                            }
                            let step = state
                                .batch
                                .steps
                                .get(last as usize)
                                .ok_or(AgentFailure::StorageUnavailable)?;
                            match step {
                                ModelStep::Preamble { text } => {
                                    if state.settled_step.is_some() {
                                        return Err(AgentFailure::StorageUnavailable);
                                    }
                                    let text = text.clone();
                                    state.cursor = Some(cursor.next_step_index);
                                    Some((last, text))
                                }
                                ModelStep::CallTool { .. }
                                | ModelStep::Delegate { .. } => {
                                    if state.settled_step != Some(last) {
                                        return Err(AgentFailure::StorageUnavailable);
                                    }
                                    state.settled_step = None;
                                    state.cursor = Some(cursor.next_step_index);
                                    None
                                }
                                ModelStep::Answer { .. } => {
                                    // Terminal answers carry no intent/result
                                    // evidence. Projections of timed-out or
                                    // failed runs still complete them through
                                    // the cursor so iteration accounting stays
                                    // exact; live output stays the Output path.
                                    if state.settled_step.is_some() {
                                        return Err(AgentFailure::StorageUnavailable);
                                    }
                                    state.cursor = Some(cursor.next_step_index);
                                    None
                                }
                            }
                        }
                    }
                };
                if let Some((ordinal, text)) = step_text {
                    let Some(state) = pending.as_ref() else {
                        return Err(AgentFailure::StorageUnavailable);
                    };
                    let message_id = floe_agent_runtime::stable_preamble_id(
                        state.batch.execution_id,
                        state.batch.batch_id,
                        ordinal,
                    );
                    let entry = ModelConversationEntry::Preamble { message_id, text };
                    entry
                        .validate()
                        .map_err(|_| AgentFailure::StorageUnavailable)?;
                    exchanges.push(entry);
                }
                // The first progress for a leading resumed re-record is the
                // durable takeover claim: store the exact starting cursor.
                if let JournalLineage::ResumeBatchOnly { batch } = &lineage {
                    lineage = JournalLineage::ResumeClaimed {
                        batch: batch.clone(),
                        cursor: cursor.clone(),
                    };
                }
                let completed = pending
                    .as_ref()
                    .is_some_and(|state| cursor.next_step_index as usize == state.batch.steps.len());
                if completed {
                    pending = None;
                    uncheckpointed_completion = true;
                }
            }
            JournalEvent::Output { .. } => return Err(AgentFailure::Conflict),
        }
    }
    if attempts.values().any(|state| !state.completed) {
        // A model attempt that never produced a result leaves nothing to resume.
        return Err(AgentFailure::Interrupted);
    }
    if !tools.is_empty() || !delegations.is_empty() {
        if pending.is_none() {
            return Err(AgentFailure::StorageUnavailable);
        }
    }
    if pending.is_none() && uncheckpointed_completion {
        // A durable final cursor proves its batch completed even when the
        // crash landed before the iteration checkpoint: consume the iteration
        // so the next run cannot replay it for free. Nothing is re-executed,
        // and the next run derives its remaining budget from this count
        // rather than re-journaling the missing checkpoint. A 65th durable
        // completion is corruption, never clamped into range.
        completed_iterations = completed_iterations
            .checked_add(1)
            .filter(|iterations| *iterations <= 64)
            .ok_or(AgentFailure::StorageUnavailable)?;
    }
    let (pending_batch, cursor) = pending
        .map(|state| {
            let cursor = BatchCursor {
                batch_id: state.batch.batch_id,
                next_step_index: state.cursor.unwrap_or(0),
            };
            (Some(state.batch), Some(cursor))
        })
        .unwrap_or((None, None));
    Ok(JournalProjection {
        model_conversation: ModelConversation {
            history: Vec::new(),
            current_turn: exchanges,
        },
        replay,
        pending_batch,
        cursor,
        completed_iterations,
        usage,
        lineage,
    })
}

#[cfg(test)]
mod tests {
    use floe_agent_contract::{
        BatchCursor, CommandId, DelegationRequest, InvocationKey, ModelStep, ModelUsage,
        PinnedToolRevision, ProjectionRef, RunId, TaskReceipt, TaskSnapshot, ToolCall, ToolResult,
        ValidatedModelBatch,
    };
    use uuid::Uuid;

    use super::*;
    use crate::{AdmittedTurn, RunReceipt};

    fn admitted() -> AdmittedTurn {
        let command_id = CommandId::new();
        let run_id = RunId::new();
        AdmittedTurn {
            receipt: RunReceipt {
                run_id,
                command_id,
                session_id: Uuid::new_v4(),
                principal: "person-a".into(),
                request_digest: [1; 32],
                state: RunState::TimedOut,
                output: None,
                coverage: DependencyCoverage::Unknown,
                issue: Some(AgentFailure::DeadlineExceeded),
                session_revision: 2,
                aggregate_revision: 2,
                executor_generation: 1,
                continuation_of: None,
                continuation_executor_generation: None,
                continuation_level: 0,
                retry_of: None,
                profile: crate::ProfileSelection::Auto,
            },
            transcript: vec![AgentMessage {
                message_id: command_id.as_uuid(),
                role: MessageRole::User,
                text: "hello".into(),
                call_id: None,
                coverage: DependencyCoverage::Independent,
            }],
        }
    }

    fn entries(events: Vec<JournalEvent>) -> Vec<JournalEntry> {
        events
            .into_iter()
            .enumerate()
            .map(|(index, event)| JournalEntry {
                revision: index as u64 + 1,
                event,
            })
            .collect()
    }

    fn tool_call() -> ToolCall {
        ToolCall {
            call_id: Uuid::new_v4(),
            invocation_key: InvocationKey::new(),
            tool_id: "read.context".into(),
            definition_revision: 3,
            input: r#"{"path":"a"}"#.into(),
        }
    }

    fn tool_result(call_id: Uuid) -> ToolResult {
        ToolResult {
            call_id,
            text: "bounded result".into(),
            artifacts: vec![],
            coverage: DependencyCoverage::Independent,
            issue: None,
        }
    }

    fn batch(attempt_id: Uuid, steps: Vec<ModelStep>) -> ValidatedModelBatch {
        ValidatedModelBatch {
            execution_id: Uuid::new_v4(),
            attempt_id,
            projection_ref: ProjectionRef::new(),
            batch_id: Uuid::new_v4(),
            steps,
            catalog_revision: 1,
            tool_revisions: vec![PinnedToolRevision {
                tool_id: "read.context".into(),
                definition_revision: 3,
            }],
            agent_revisions: vec![],
            projection_coverage: DependencyCoverage::Independent,
        }
    }

    fn tool_call_for(
        batch: &ValidatedModelBatch,
        ordinal: u32,
        tool_id: &str,
        definition_revision: u64,
        input: &str,
    ) -> ToolCall {
        ToolCall {
            call_id: floe_agent_runtime::stable_call_id(
                batch.execution_id,
                batch.batch_id,
                ordinal,
            ),
            invocation_key: floe_agent_runtime::stable_invocation_key(
                batch.execution_id,
                batch.batch_id,
                ordinal,
                floe_agent_runtime::InvocationKind::Tool,
            ),
            tool_id: tool_id.into(),
            definition_revision,
            input: input.into(),
        }
    }

    fn delegation_request_for(
        admitted: &AdmittedTurn,
        batch: &ValidatedModelBatch,
        ordinal: u32,
        agent_id: &str,
        definition_revision: u64,
        message: &str,
        context_refs: Vec<String>,
    ) -> DelegationRequest {
        DelegationRequest {
            task_id: floe_agent_runtime::stable_task_id(
                batch.execution_id,
                batch.batch_id,
                ordinal,
            ),
            parent_run_id: Some(admitted.receipt.run_id.as_uuid()),
            principal: admitted.receipt.principal.clone(),
            invocation_key: floe_agent_runtime::stable_invocation_key(
                batch.execution_id,
                batch.batch_id,
                ordinal,
                floe_agent_runtime::InvocationKind::Delegation,
            ),
            selected_agent_id: agent_id.into(),
            selected_definition_revision: definition_revision,
            message: message.into(),
            context_refs,
        }
    }

    fn pending_delegation_batch(
        admitted: &AdmittedTurn,
        attempt_id: Uuid,
        context_refs: Vec<String>,
    ) -> (ValidatedModelBatch, Vec<JournalEvent>) {
        let execution_id = Uuid::new_v4();
        let batch = ValidatedModelBatch {
            execution_id,
            attempt_id,
            projection_ref: ProjectionRef::new(),
            batch_id: Uuid::new_v4(),
            steps: vec![ModelStep::Delegate {
                agent_id: "expert-a".into(),
                definition_revision: 2,
                message: "summarize".into(),
                context_refs: context_refs.clone(),
            }],
            catalog_revision: 1,
            tool_revisions: vec![],
            agent_revisions: vec![floe_agent_contract::PinnedAgentRevision {
                agent_id: "expert-a".into(),
                definition_revision: 2,
            }],
            projection_coverage: DependencyCoverage::Independent,
        };
        let batch_id = batch.batch_id;
        let prefix = vec![
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: batch.projection_ref,
            },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ValidatedBatch { batch: batch.clone() },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
        ];
        let _ = admitted;
        (batch, prefix)
    }

    #[test]
    fn projection_pairs_settled_work_and_rejects_an_unsettled_intent() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        let call = tool_call_for(&model_batch, 0, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::ToolIntent { call: call.clone() },
                JournalEvent::ToolResult {
                    result: result.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 1,
                    },
                },
                JournalEvent::Checkpoint { iteration: 1 },
            ]),
        )
        .unwrap();
        assert_eq!(snapshot.reference.run_id, admitted.receipt.run_id);
        assert_eq!(snapshot.completed_iterations, 1);
        assert_eq!(snapshot.usage.attempts, 1);
        assert_eq!(snapshot.usage.tokens, 1);
        assert_eq!(snapshot.usage.cost_micros, 1);
        assert_eq!(snapshot.model_conversation.history.len(), 1);
        assert_eq!(snapshot.model_conversation.current_turn.len(), 1);
        assert!(snapshot.pending_batch.is_none());
        assert!(snapshot.batch_cursor.is_none());
        assert_eq!(snapshot.replay.len(), 1);
        assert_eq!(snapshot.replay[0].call_id, call.call_id);
        assert_eq!(snapshot.replay[0].result, result.text);

        let unsettled = entries(vec![JournalEvent::ModelIntent {
            attempt_id,
            projection_ref: ProjectionRef::new(),
        }]);
        assert!(matches!(
            project_continuation(&admitted, &unsettled),
            Err(AgentFailure::Interrupted)
        ));
    }

    #[test]
    fn tool_exchange_preserves_original_input_and_revision() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        let call = tool_call_for(&model_batch, 0, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::ToolIntent { call: call.clone() },
                JournalEvent::ToolResult {
                    result: result.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 1,
                    },
                },
                JournalEvent::Checkpoint { iteration: 1 },
            ]),
        )
        .unwrap();
        assert!(
            matches!(
                snapshot.model_conversation.current_turn.as_slice(),
                [ModelConversationEntry::ToolExchange {
                    call: recovered_call,
                    result: recovered_result,
                }] if recovered_call == &call && recovered_result == &result
            ),
            "exchange must keep the original call and result: {:?}",
            snapshot.model_conversation.current_turn
        );
    }

    #[test]
    fn delegation_exchange_preserves_context_refs() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let context_refs = vec!["turn:1".into(), "evidence:9".into()];
        let model_batch = ValidatedModelBatch {
            execution_id: Uuid::new_v4(),
            attempt_id,
            projection_ref: ProjectionRef::new(),
            batch_id: Uuid::new_v4(),
            steps: vec![ModelStep::Delegate {
                agent_id: "expert-a".into(),
                definition_revision: 2,
                message: "summarize".into(),
                context_refs: context_refs.clone(),
            }],
            catalog_revision: 1,
            tool_revisions: vec![],
            agent_revisions: vec![floe_agent_contract::PinnedAgentRevision {
                agent_id: "expert-a".into(),
                definition_revision: 2,
            }],
            projection_coverage: DependencyCoverage::Independent,
        };
        let request = delegation_request_for(
            &admitted,
            &model_batch,
            0,
            "expert-a",
            2,
            "summarize",
            context_refs,
        );
        let task_id = request.task_id;
        let receipt = TaskReceipt {
            task_id,
            snapshot: TaskSnapshot {
                task_id,
                parent_run_id: request.parent_run_id,
                principal: request.principal.clone(),
                agent_id: request.selected_agent_id.clone(),
                definition_revision: request.selected_definition_revision,
                state: TaskState::Completed,
                result: Some("summary".into()),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                issue: None,
            },
            replay: None,
        };
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::DelegationIntent {
                    request: request.clone(),
                },
                JournalEvent::DelegationResult {
                    receipt: Box::new(receipt.clone()),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 1,
                    },
                },
                JournalEvent::Checkpoint { iteration: 1 },
            ]),
        )
        .unwrap();
        assert!(
            matches!(
                snapshot.model_conversation.current_turn.as_slice(),
                [ModelConversationEntry::DelegationExchange {
                    request: recovered_request,
                    receipt: recovered_receipt,
                }] if recovered_request == &request && recovered_receipt == &receipt
            ),
            "exchange must keep the original request and receipt: {:?}",
            snapshot.model_conversation.current_turn
        );
    }

    #[test]
    fn validated_batch_and_cursor_survive_projection() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        // Two steps, one settled: the projection must return the pending batch
        // with its cursor instead of discarding it as interrupted.
        let model_batch = batch(
            attempt_id,
            vec![
                ModelStep::CallTool {
                    tool_id: "read.context".into(),
                    definition_revision: 3,
                    input: r#"{"path":"a"}"#.into(),
                },
                ModelStep::CallTool {
                    tool_id: "read.context".into(),
                    definition_revision: 3,
                    input: r#"{"path":"a"}"#.into(),
                },
            ],
        );
        let call = tool_call_for(&model_batch, 0, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::ToolIntent { call: call.clone() },
                JournalEvent::ToolResult {
                    result: result.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 1,
                    },
                },
            ]),
        )
        .unwrap();
        assert_eq!(snapshot.pending_batch.as_ref(), Some(&model_batch));
        assert_eq!(
            snapshot.batch_cursor,
            Some(BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 1,
            })
        );
        assert_eq!(snapshot.model_conversation.current_turn.len(), 1);
        assert_eq!(snapshot.completed_iterations, 0);
    }

    fn history_dependency() -> floe_agent_contract::ContextDependency {
        use floe_context_contract::{
            ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
            GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
            GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
        };
        let person = floe_kernel::PersonId::new();
        let source = GrantSourceBinding::try_new(
            person,
            ConnectionId::try_new("connection").unwrap(),
            ConnectorId::try_new("connector").unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let now = chrono::Utc::now();
        floe_agent_contract::ContextDependency::try_new(
            person,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"fingerprint".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - chrono::Duration::minutes(1),
            now + chrono::Duration::minutes(5),
        )
        .unwrap()
    }

    #[test]
    fn crashed_answer_batch_preserves_exact_projection_coverage() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let coverage =
            DependencyCoverage::dependent(history_dependency()).unwrap();
        let mut model_batch = answer_batch(attempt_id, Uuid::new_v4(), ProjectionRef::new());
        model_batch.projection_coverage = coverage.clone();
        // Crash after the batch/cursor ack, before the answer commits: the
        // pending batch must carry the exact answering projection coverage.
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
            ]),
        )
        .unwrap();
        assert_eq!(snapshot.pending_batch.as_ref(), Some(&model_batch));
        assert_eq!(
            snapshot
                .pending_batch
                .as_ref()
                .map(|batch| &batch.projection_coverage),
            Some(&coverage)
        );
        assert_eq!(
            snapshot.batch_cursor,
            Some(BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 0,
            })
        );
    }

    #[test]
    fn malformed_projection_coverage_is_storage_fault() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let mut model_batch = answer_batch(attempt_id, Uuid::new_v4(), ProjectionRef::new());
        model_batch.projection_coverage = DependencyCoverage::Dependent {
            dependencies: vec![],
        };
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ModelIntent {
                        attempt_id,
                        projection_ref: model_batch.projection_ref,
                    },
                    JournalEvent::ModelResult {
                        attempt_id,
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    },
                    JournalEvent::ValidatedBatch { batch: model_batch },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn settled_result_with_stale_cursor_is_recoverable() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        // The cursor ack was lost after the result ack: the result is settled
        // and the batch is still pending from the last acked cursor.
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        let call = tool_call_for(&model_batch, 0, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::ToolIntent { call: call.clone() },
                JournalEvent::ToolResult {
                    result: result.clone(),
                },
            ]),
        )
        .unwrap();
        assert_eq!(snapshot.pending_batch.as_ref(), Some(&model_batch));
        assert_eq!(
            snapshot.batch_cursor,
            Some(BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 0,
            })
        );
        assert_eq!(snapshot.model_conversation.current_turn.len(), 1);
        assert_eq!(snapshot.replay.len(), 1);
    }

    #[test]
    fn validated_batch_projection_ref_must_match_model_intent() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: "{}".into(),
            }],
        );
        let mut intent_ref = ProjectionRef::new();
        while intent_ref == model_batch.projection_ref {
            intent_ref = ProjectionRef::new();
        }
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ModelIntent {
                        attempt_id,
                        projection_ref: intent_ref,
                    },
                    JournalEvent::ModelResult {
                        attempt_id,
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    },
                    JournalEvent::ValidatedBatch {
                        batch: model_batch,
                    },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn validated_batch_attempt_must_have_completed_result() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: "{}".into(),
            }],
        );
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ModelIntent {
                        attempt_id,
                        projection_ref: model_batch.projection_ref,
                    },
                    JournalEvent::ValidatedBatch {
                        batch: model_batch,
                    },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn validated_batches_must_share_execution_id() {
        let admitted = admitted();
        let execution_id = Uuid::new_v4();
        let first_attempt = Uuid::new_v4();
        let mut first = batch(
            first_attempt,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        first.execution_id = execution_id;
        let first_batch_id = first.batch_id;
        let call = tool_call_for(&first, 0, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        let second_attempt = Uuid::new_v4();
        let mut second = batch(second_attempt, first.steps.clone());
        while second.execution_id == execution_id {
            second.execution_id = Uuid::new_v4();
        }
        // A complete first iteration, then a second batch from a different
        // execution: the journal mixes two executions and must fail closed.
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ModelIntent {
                        attempt_id: first_attempt,
                        projection_ref: first.projection_ref,
                    },
                    JournalEvent::ModelResult {
                        attempt_id: first_attempt,
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    },
                    JournalEvent::ValidatedBatch { batch: first },
                    JournalEvent::BatchProgress {
                        cursor: BatchCursor {
                            batch_id: first_batch_id,
                            next_step_index: 0,
                        },
                    },
                    JournalEvent::ToolIntent { call: call.clone() },
                    JournalEvent::ToolResult {
                        result: result.clone(),
                    },
                    JournalEvent::BatchProgress {
                        cursor: BatchCursor {
                            batch_id: first_batch_id,
                            next_step_index: 1,
                        },
                    },
                    JournalEvent::Checkpoint { iteration: 1 },
                    JournalEvent::ModelIntent {
                        attempt_id: second_attempt,
                        projection_ref: second.projection_ref,
                    },
                    JournalEvent::ModelResult {
                        attempt_id: second_attempt,
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    },
                    JournalEvent::ValidatedBatch { batch: second },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn two_fresh_batches_sharing_execution_id_project() {
        let admitted = admitted();
        let execution_id = Uuid::new_v4();
        let first_attempt = Uuid::new_v4();
        let mut first = batch(
            first_attempt,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        first.execution_id = execution_id;
        let first_batch_id = first.batch_id;
        let call = tool_call_for(&first, 0, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        let second_attempt = Uuid::new_v4();
        let mut second = batch(
            second_attempt,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        second.execution_id = execution_id;
        let second_batch_id = second.batch_id;
        let second_call = tool_call_for(&second, 0, "read.context", 3, r#"{"path":"a"}"#);
        let second_result = tool_result(second_call.call_id);
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id: first_attempt,
                    projection_ref: first.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id: first_attempt,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch { batch: first },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: first_batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::ToolIntent { call: call.clone() },
                JournalEvent::ToolResult {
                    result: result.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: first_batch_id,
                        next_step_index: 1,
                    },
                },
                JournalEvent::Checkpoint { iteration: 1 },
                JournalEvent::ModelIntent {
                    attempt_id: second_attempt,
                    projection_ref: second.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id: second_attempt,
                    usage: ModelUsage {
                        tokens: 2,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch { batch: second },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: second_batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::ToolIntent {
                    call: second_call.clone(),
                },
                JournalEvent::ToolResult {
                    result: second_result.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: second_batch_id,
                        next_step_index: 1,
                    },
                },
                JournalEvent::Checkpoint { iteration: 2 },
            ]),
        )
        .unwrap();
        assert_eq!(snapshot.completed_iterations, 2);
        assert!(snapshot.pending_batch.is_none());
        assert_eq!(snapshot.usage.attempts, 2);
        assert_eq!(snapshot.usage.tokens, 3);
    }

    fn completed_iteration(execution_id: Uuid, iteration: u32) -> Vec<JournalEvent> {
        let attempt_id = Uuid::new_v4();
        let mut completed = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        completed.execution_id = execution_id;
        let batch_id = completed.batch_id;
        let call = tool_call_for(&completed, 0, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        vec![
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: completed.projection_ref,
            },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ValidatedBatch { batch: completed },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
            JournalEvent::ToolIntent { call },
            JournalEvent::ToolResult { result },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 1,
                },
            },
            JournalEvent::Checkpoint { iteration },
        ]
    }

    fn uncheckpointed_completion(execution_id: Uuid) -> Vec<JournalEvent> {
        let mut events = completed_iteration(execution_id, 0);
        assert!(matches!(
            events.pop(),
            Some(JournalEvent::Checkpoint { iteration: 0 })
        ));
        events
    }

    #[test]
    fn final_cursor_without_iteration_checkpoint_consumes_iteration() {
        let admitted = admitted();
        let snapshot = project_continuation(
            &admitted,
            &entries(uncheckpointed_completion(Uuid::new_v4())),
        )
        .unwrap();
        assert!(snapshot.pending_batch.is_none());
        assert!(snapshot.batch_cursor.is_none());
        assert_eq!(snapshot.completed_iterations, 1);
        assert_eq!(snapshot.model_conversation.current_turn.len(), 1);
        assert_eq!(snapshot.replay.len(), 1);
    }

    #[test]
    fn max_iteration_cannot_be_bypassed_by_crash_after_final_cursor() {
        let admitted = admitted();
        let execution_id = Uuid::new_v4();
        let mut events = Vec::new();
        for iteration in 1..=63 {
            events.extend(completed_iteration(execution_id, iteration));
        }
        events.extend(uncheckpointed_completion(execution_id));
        assert_eq!(events.len(), 63 * 8 + 7);
        let snapshot = project_continuation(&admitted, &entries(events)).unwrap();
        assert!(snapshot.pending_batch.is_none());
        // The crashed iteration is consumed at the representable bound
        // instead of granting a free replay past the cap.
        assert_eq!(snapshot.completed_iterations, 64);
    }

    #[test]
    fn stale_tool_observation_survives_recovery() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        // A stale call keeps its exact attempted input and revision; the
        // host-generated result carries the retryable invalid-output issue.
        let model_batch = batch(
            attempt_id,
            vec![
                ModelStep::CallTool {
                    tool_id: "read.context".into(),
                    definition_revision: 99,
                    input: r#"{"path":"a"}"#.into(),
                },
                ModelStep::CallTool {
                    tool_id: "read.context".into(),
                    definition_revision: 3,
                    input: r#"{"path":"a"}"#.into(),
                },
            ],
        );
        let stale_call =
            tool_call_for(&model_batch, 0, "read.context", 99, r#"{"path":"a"}"#);
        let stale_result = ToolResult {
            call_id: stale_call.call_id,
            text: "tool descriptor is stale".into(),
            artifacts: vec![],
            coverage: DependencyCoverage::Independent,
            issue: Some(floe_agent_contract::OutcomeIssue {
                failure: AgentFailure::InvalidModelOutput,
                retryable: true,
            }),
        };
        let call = tool_call_for(&model_batch, 1, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        // Crash after the later real result, before its cursor ack: the soft
        // observation must still project alongside the real one.
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::ToolIntent {
                    call: stale_call.clone(),
                },
                JournalEvent::ToolResult {
                    result: stale_result.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 1,
                    },
                },
                JournalEvent::ToolIntent { call: call.clone() },
                JournalEvent::ToolResult {
                    result: result.clone(),
                },
            ]),
        )
        .unwrap();
        assert_eq!(snapshot.pending_batch.as_ref(), Some(&model_batch));
        assert_eq!(snapshot.model_conversation.current_turn.len(), 2);
        assert!(
            matches!(
                snapshot.model_conversation.current_turn.as_slice(),
                [ModelConversationEntry::ToolExchange {
                    call: recovered_call,
                    result: recovered_result,
                }, ModelConversationEntry::ToolExchange { .. }]
                if recovered_call == &stale_call && recovered_result == &stale_result
            ),
            "stale observation must survive with its exact call and result: {:?}",
            snapshot.model_conversation.current_turn
        );
        assert_eq!(snapshot.replay.len(), 2);
    }

    #[test]
    fn unknown_agent_rejection_survives_recovery() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        // The host never dispatched: the intent carries the exact attempted
        // request and the terminal rejection carries no result.
        let model_batch = ValidatedModelBatch {
            execution_id: Uuid::new_v4(),
            attempt_id,
            projection_ref: ProjectionRef::new(),
            batch_id: Uuid::new_v4(),
            steps: vec![ModelStep::Delegate {
                agent_id: "missing-expert".into(),
                definition_revision: 1,
                message: "summarize".into(),
                context_refs: vec!["turn:1".into()],
            }],
            catalog_revision: 1,
            tool_revisions: vec![],
            agent_revisions: vec![],
            projection_coverage: DependencyCoverage::Independent,
        };
        let request = delegation_request_for(
            &admitted,
            &model_batch,
            0,
            "missing-expert",
            1,
            "summarize",
            vec!["turn:1".into()],
        );
        let task_id = request.task_id;
        let receipt = TaskReceipt {
            task_id,
            snapshot: TaskSnapshot {
                task_id,
                parent_run_id: request.parent_run_id,
                principal: request.principal.clone(),
                agent_id: request.selected_agent_id.clone(),
                definition_revision: request.selected_definition_revision,
                state: TaskState::Rejected,
                result: None,
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                issue: Some(AgentFailure::InvalidModelOutput),
            },
            replay: None,
        };
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::DelegationIntent {
                    request: request.clone(),
                },
                JournalEvent::DelegationResult {
                    receipt: Box::new(receipt.clone()),
                },
            ]),
        )
        .unwrap();
        assert_eq!(snapshot.pending_batch.as_ref(), Some(&model_batch));
        assert!(
            matches!(
                snapshot.model_conversation.current_turn.as_slice(),
                [ModelConversationEntry::DelegationExchange {
                    request: recovered_request,
                    receipt: recovered_receipt,
                }] if recovered_request == &request && recovered_receipt == &receipt
            ),
            "rejection must survive with its exact request and receipt: {:?}",
            snapshot.model_conversation.current_turn
        );
        assert_eq!(snapshot.replay.len(), 1);
        assert_eq!(
            snapshot.replay[0].task_state,
            Some(TaskState::Rejected)
        );
    }

    #[test]
    fn soft_result_and_following_real_result_keep_original_batch_order() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![
                ModelStep::CallTool {
                    tool_id: "missing.tool".into(),
                    definition_revision: 1,
                    input: "{}".into(),
                },
                ModelStep::CallTool {
                    tool_id: "read.context".into(),
                    definition_revision: 3,
                    input: r#"{"path":"a"}"#.into(),
                },
            ],
        );
        let soft_call = tool_call_for(&model_batch, 0, "missing.tool", 1, "{}");
        let soft_result = ToolResult {
            call_id: soft_call.call_id,
            text: "tool is not registered".into(),
            artifacts: vec![],
            coverage: DependencyCoverage::Independent,
            issue: Some(floe_agent_contract::OutcomeIssue {
                failure: AgentFailure::InvalidModelOutput,
                retryable: true,
            }),
        };
        let call = tool_call_for(&model_batch, 1, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::ToolIntent {
                    call: soft_call.clone(),
                },
                JournalEvent::ToolResult {
                    result: soft_result.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 1,
                    },
                },
                JournalEvent::ToolIntent { call: call.clone() },
                JournalEvent::ToolResult {
                    result: result.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 2,
                    },
                },
                JournalEvent::Checkpoint { iteration: 1 },
            ]),
        )
        .unwrap();
        assert!(snapshot.pending_batch.is_none());
        assert_eq!(snapshot.completed_iterations, 1);
        assert!(
            matches!(
                snapshot.model_conversation.current_turn.as_slice(),
                [
                    ModelConversationEntry::ToolExchange {
                        call: first_call,
                        result: first_result,
                    },
                    ModelConversationEntry::ToolExchange {
                        call: second_call,
                        result: second_result,
                    },
                ] if first_call == &soft_call
                    && first_result == &soft_result
                    && second_call == &call
                    && second_result == &result
            ),
            "batch order must be preserved: {:?}",
            snapshot.model_conversation.current_turn
        );
    }

    #[test]
    fn duplicate_model_result_is_storage_fault() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let usage = ModelUsage {
            tokens: 1,
            cost_micros: 1,
        };
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ModelIntent {
                        attempt_id,
                        projection_ref: ProjectionRef::new(),
                    },
                    JournalEvent::ModelResult {
                        attempt_id,
                        usage,
                    },
                    JournalEvent::ModelResult {
                        attempt_id,
                        usage,
                    },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn resumed_first_batch_without_local_attempt_is_still_allowed() {
        let admitted = admitted();
        // No model intent in this journal: the batch was validated by an older
        // run and re-recorded here before its steps resume.
        let model_batch = batch(
            Uuid::new_v4(),
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        let call = tool_call_for(&model_batch, 0, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::ToolIntent { call: call.clone() },
                JournalEvent::ToolResult {
                    result: result.clone(),
                },
            ]),
        )
        .unwrap();
        assert_eq!(snapshot.pending_batch.as_ref(), Some(&model_batch));
        assert_eq!(snapshot.model_conversation.current_turn.len(), 1);
    }

    #[test]
    fn malformed_cursor_or_batch_identity_is_storage_fault() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: "{}".into(),
            }],
        );
        let prefix = vec![
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: model_batch.projection_ref,
            },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ValidatedBatch {
                batch: model_batch.clone(),
            },
        ];

        // Cursor past the end.
        let mut events = prefix.clone();
        events.push(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 2,
            },
        });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));

        // Cursor for a batch that was never recorded.
        let mut events = prefix.clone();
        events.push(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: Uuid::new_v4(),
                next_step_index: 0,
            },
        });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));

        // Non-monotonic cursor.
        let mut events = prefix.clone();
        events.push(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 0,
            },
        });
        events.push(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 0,
            },
        });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));

        // Duplicate batch id.
        let mut events = prefix.clone();
        events.push(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 0,
            },
        });
        events.push(JournalEvent::ValidatedBatch {
            batch: model_batch.clone(),
        });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));

        // Batch whose locally-seen attempt never completed.
        let lonely_attempt = Uuid::new_v4();
        let lonely = batch(lonely_attempt, model_batch.steps.clone());
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ModelIntent {
                        attempt_id: lonely_attempt,
                        projection_ref: lonely.projection_ref,
                    },
                    JournalEvent::ValidatedBatch { batch: lonely },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));

        // A resumed batch re-recorded after a local batch is not leading.
        let mut events = prefix.clone();
        events.push(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 1,
            },
        });
        events.push(JournalEvent::Checkpoint { iteration: 1 });
        let rerecorded = batch(Uuid::new_v4(), model_batch.steps.clone());
        events.push(JournalEvent::ValidatedBatch {
            batch: rerecorded,
        });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));

        // Checkpoint while a batch is still pending.
        let mut events = prefix.clone();
        events.push(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 0,
            },
        });
        events.push(JournalEvent::Checkpoint { iteration: 1 });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    fn answer_batch(
        attempt_id: Uuid,
        execution_id: Uuid,
        projection_ref: ProjectionRef,
    ) -> ValidatedModelBatch {
        ValidatedModelBatch {
            execution_id,
            attempt_id,
            projection_ref,
            batch_id: Uuid::new_v4(),
            steps: vec![ModelStep::Answer {
                text: "done".into(),
                artifacts: vec![],
            }],
            catalog_revision: 1,
            tool_revisions: vec![],
            agent_revisions: vec![],
            projection_coverage: DependencyCoverage::Independent,
        }
    }

    fn completed_answer_iteration(execution_id: Uuid, iteration: u32) -> Vec<JournalEvent> {
        let attempt_id = Uuid::new_v4();
        let batch = answer_batch(attempt_id, execution_id, ProjectionRef::new());
        let batch_id = batch.batch_id;
        vec![
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: batch.projection_ref,
            },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ValidatedBatch { batch },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 1,
                },
            },
            JournalEvent::Checkpoint { iteration },
        ]
    }

    #[test]
    fn one_model_attempt_cannot_bind_two_fresh_batches() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let projection_ref = ProjectionRef::new();
        let execution_id = Uuid::new_v4();
        let first = answer_batch(attempt_id, execution_id, projection_ref);
        let first_batch_id = first.batch_id;
        let mut second = answer_batch(attempt_id, execution_id, projection_ref);
        while second.batch_id == first_batch_id {
            second.batch_id = Uuid::new_v4();
        }
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ModelIntent {
                        attempt_id,
                        projection_ref,
                    },
                    JournalEvent::ModelResult {
                        attempt_id,
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    },
                    JournalEvent::ValidatedBatch { batch: first },
                    JournalEvent::BatchProgress {
                        cursor: BatchCursor {
                            batch_id: first_batch_id,
                            next_step_index: 0,
                        },
                    },
                    JournalEvent::BatchProgress {
                        cursor: BatchCursor {
                            batch_id: first_batch_id,
                            next_step_index: 1,
                        },
                    },
                    JournalEvent::Checkpoint { iteration: 1 },
                    JournalEvent::ValidatedBatch { batch: second },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn different_attempts_can_each_bind_one_batch() {
        let admitted = admitted();
        let execution_id = Uuid::new_v4();
        let first_attempt = Uuid::new_v4();
        let first = answer_batch(first_attempt, execution_id, ProjectionRef::new());
        let first_batch_id = first.batch_id;
        let second_attempt = Uuid::new_v4();
        let second = answer_batch(second_attempt, execution_id, ProjectionRef::new());
        let second_batch_id = second.batch_id;
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id: first_attempt,
                    projection_ref: first.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id: first_attempt,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch { batch: first },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: first_batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: first_batch_id,
                        next_step_index: 1,
                    },
                },
                JournalEvent::Checkpoint { iteration: 1 },
                JournalEvent::ModelIntent {
                    attempt_id: second_attempt,
                    projection_ref: second.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id: second_attempt,
                    usage: ModelUsage {
                        tokens: 2,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch { batch: second },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: second_batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: second_batch_id,
                        next_step_index: 1,
                    },
                },
                JournalEvent::Checkpoint { iteration: 2 },
            ]),
        )
        .unwrap();
        assert_eq!(snapshot.completed_iterations, 2);
        assert!(snapshot.pending_batch.is_none());
        assert_eq!(snapshot.usage.attempts, 2);
        assert_eq!(snapshot.usage.tokens, 3);
    }

    fn pending_answer_batch(
        attempt_id: Uuid,
    ) -> (ValidatedModelBatch, Vec<JournalEvent>) {
        let batch = answer_batch(attempt_id, Uuid::new_v4(), ProjectionRef::new());
        let batch_id = batch.batch_id;
        let prefix = vec![
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: batch.projection_ref,
            },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ValidatedBatch { batch: batch.clone() },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
        ];
        (batch, prefix)
    }

    fn delegation_request(
        admitted: &AdmittedTurn,
        context_refs: Vec<String>,
    ) -> DelegationRequest {
        DelegationRequest {
            task_id: floe_agent_contract::TaskId::new(),
            parent_run_id: Some(admitted.receipt.run_id.as_uuid()),
            principal: admitted.receipt.principal.clone(),
            invocation_key: InvocationKey::new(),
            selected_agent_id: "expert-a".into(),
            selected_definition_revision: 2,
            message: "summarize".into(),
            context_refs,
        }
    }

    #[test]
    fn oversized_context_ref_in_journal_is_storage_fault() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        // Count over the bound with small refs so the batch itself still fits
        // the wire budget: the journal intent must fail on the ref bound.
        let oversized: Vec<String> = (0..(floe_agent_contract::MAX_CONTEXT_REFS + 1))
            .map(|index| format!("turn:{index}"))
            .collect();
        assert_eq!(oversized.len(), floe_agent_contract::MAX_CONTEXT_REFS + 1);
        let (batch, mut events) =
            pending_delegation_batch(&admitted, attempt_id, oversized.clone());
        let request = delegation_request_for(
            &admitted,
            &batch,
            0,
            "expert-a",
            2,
            "summarize",
            oversized,
        );
        events.push(JournalEvent::DelegationIntent { request });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn maximum_valid_context_ref_survives_recovery() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        // Maximum count with small refs so the validated batch still fits the
        // wire budget; a 64 KiB single ref can never sit in a batch.
        let context_refs: Vec<String> = (0..floe_agent_contract::MAX_CONTEXT_REFS)
            .map(|index| format!("turn:{index}"))
            .collect();
        assert_eq!(context_refs.len(), floe_agent_contract::MAX_CONTEXT_REFS);
        let (batch, mut events) =
            pending_delegation_batch(&admitted, attempt_id, context_refs.clone());
        let request = delegation_request_for(
            &admitted,
            &batch,
            0,
            "expert-a",
            2,
            "summarize",
            context_refs,
        );
        let receipt = TaskReceipt {
            task_id: request.task_id,
            snapshot: TaskSnapshot {
                task_id: request.task_id,
                parent_run_id: request.parent_run_id,
                principal: request.principal.clone(),
                agent_id: request.selected_agent_id.clone(),
                definition_revision: request.selected_definition_revision,
                state: TaskState::Completed,
                result: Some("summary".into()),
                artifacts: vec![],
                coverage: DependencyCoverage::Independent,
                issue: None,
            },
            replay: None,
        };
        events.push(JournalEvent::DelegationIntent {
            request: request.clone(),
        });
        events.push(JournalEvent::DelegationResult {
            receipt: Box::new(receipt),
        });
        events.push(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: batch.batch_id,
                next_step_index: 1,
            },
        });
        events.push(JournalEvent::Checkpoint { iteration: 1 });
        let snapshot = project_continuation(&admitted, &entries(events)).unwrap();
        assert_eq!(snapshot.completed_iterations, 1);
        assert!(snapshot.pending_batch.is_none());
        assert_eq!(snapshot.replay.len(), 1);
        assert_eq!(
            snapshot.replay[0].task_state,
            Some(TaskState::Completed)
        );
    }

    #[test]
    fn sixty_fifth_completed_iteration_is_storage_fault() {
        let admitted = admitted();
        let execution_id = Uuid::new_v4();
        let mut events = Vec::new();
        for iteration in 1..=64 {
            events.extend(completed_answer_iteration(execution_id, iteration));
        }
        // One more durable completion past the cap, crashed before its
        // checkpoint: recovery must fail closed, never clamp to 64.
        let attempt_id = Uuid::new_v4();
        let batch = answer_batch(attempt_id, execution_id, ProjectionRef::new());
        let batch_id = batch.batch_id;
        events.extend(vec![
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: batch.projection_ref,
            },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ValidatedBatch { batch },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 0,
                },
            },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id,
                    next_step_index: 1,
                },
            },
        ]);
        assert!(events.len() < 512);
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn tool_intent_must_match_current_validated_step() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let (_, mut events) = pending_answer_batch(attempt_id);
        events.push(JournalEvent::ToolIntent { call: tool_call() });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn delegation_intent_must_match_current_validated_step() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let (_, mut events) = pending_answer_batch(attempt_id);
        events.push(JournalEvent::DelegationIntent {
            request: delegation_request(&admitted, vec!["turn:1".into()]),
        });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn tool_intent_payload_must_match_validated_step() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: "{}".into(),
            }],
        );
        let mut call = tool_call_for(&model_batch, 0, "read.context", 3, "{}");
        call.input = r#"{"x":1}"#.into();
        let prefix = vec![
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: model_batch.projection_ref,
            },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ValidatedBatch {
                batch: model_batch.clone(),
            },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: model_batch.batch_id,
                    next_step_index: 0,
                },
            },
        ];
        let mut events = prefix;
        events.push(JournalEvent::ToolIntent { call });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn delegation_intent_payload_must_match_validated_step() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let (batch, mut events) =
            pending_delegation_batch(&admitted, attempt_id, vec!["a".into()]);
        let request =
            delegation_request_for(&admitted, &batch, 0, "expert-a", 2, "summarize", vec!["b".into()]);
        events.push(JournalEvent::DelegationIntent { request });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn tool_intent_requires_exact_stable_identity() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        // Same payload, random stable identity: must fail closed.
        let call = tool_call();
        assert_eq!(call.tool_id, "read.context");
        let mut events = vec![
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: model_batch.projection_ref,
            },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ValidatedBatch {
                batch: model_batch.clone(),
            },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: model_batch.batch_id,
                    next_step_index: 0,
                },
            },
        ];
        events.push(JournalEvent::ToolIntent { call });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn delegation_intent_requires_exact_stable_identity() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let context_refs = vec!["turn:1".into()];
        let (batch, mut events) =
            pending_delegation_batch(&admitted, attempt_id, context_refs.clone());
        // Same payload, random stable identity: must fail closed.
        let request = delegation_request(&admitted, context_refs);
        events.push(JournalEvent::DelegationIntent { request });
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn cursor_cannot_skip_executable_step() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        // No intent/result between the two cursors: skipping the executable
        // step must fail closed.
        let events = vec![
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref: model_batch.projection_ref,
            },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ValidatedBatch {
                batch: model_batch.clone(),
            },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: model_batch.batch_id,
                    next_step_index: 0,
                },
            },
            JournalEvent::BatchProgress {
                cursor: BatchCursor {
                    batch_id: model_batch.batch_id,
                    next_step_index: 1,
                },
            },
        ];
        assert!(matches!(
            project_continuation(&admitted, &entries(events)),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn preamble_advances_cursor_and_recovers_deterministically() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let model_batch = batch(
            attempt_id,
            vec![
                ModelStep::Preamble {
                    text: "thinking".into(),
                },
                ModelStep::CallTool {
                    tool_id: "read.context".into(),
                    definition_revision: 3,
                    input: r#"{"path":"a"}"#.into(),
                },
            ],
        );
        // Crash after the preamble cursor, before any tool work.
        let snapshot = project_continuation(
            &admitted,
            &entries(vec![
                JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: model_batch.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 0,
                    },
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 1,
                    },
                },
            ]),
        )
        .unwrap();
        assert_eq!(
            snapshot.batch_cursor,
            Some(BatchCursor {
                batch_id: model_batch.batch_id,
                next_step_index: 1,
            })
        );
        let expected_id = floe_agent_runtime::stable_preamble_id(
            model_batch.execution_id,
            model_batch.batch_id,
            0,
        );
        assert!(
            matches!(
                snapshot.model_conversation.current_turn.as_slice(),
                [ModelConversationEntry::Preamble { message_id, text }]
                    if *message_id == expected_id && text == "thinking"
            ),
            "preamble must recover once with its stable id: {:?}",
            snapshot.model_conversation.current_turn
        );
        // Resume starts the tool at ordinal 1 under its stable identity.
        let pending = snapshot.pending_batch.as_ref().expect("batch stays pending");
        assert!(matches!(
            pending.steps.get(1),
            Some(ModelStep::CallTool { .. })
        ));
        let expected_call = floe_agent_runtime::stable_call_id(
            model_batch.execution_id,
            model_batch.batch_id,
            1,
        );
        let next_call = tool_call_for(&model_batch, 1, "read.context", 3, r#"{"path":"a"}"#);
        assert_eq!(next_call.call_id, expected_call);
    }

    #[test]
    fn leading_resumed_batch_without_cursor_is_not_claimed() {
        let admitted = admitted();
        let model_batch = batch(
            Uuid::new_v4(),
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        // A crash between ValidatedBatch and its initial BatchProgress leaves
        // a re-record that never started: pending projects, but lineage proves
        // no durable takeover claim was recorded.
        let projected = project_journal(
            &admitted.receipt,
            &entries(vec![JournalEvent::ValidatedBatch {
                batch: model_batch.clone(),
            }]),
        )
        .unwrap();
        assert!(
            matches!(&projected.lineage, JournalLineage::ResumeBatchOnly { batch }
                if batch == &model_batch),
            "batch-only re-record must not count as claimed: {:?}",
            projected.lineage
        );
        assert_eq!(projected.pending_batch.as_ref(), Some(&model_batch));
    }

    #[test]
    fn resumed_batch_cannot_execute_before_initial_cursor() {
        let admitted = admitted();
        let model_batch = batch(
            Uuid::new_v4(),
            vec![ModelStep::CallTool {
                tool_id: "read.context".into(),
                definition_revision: 3,
                input: r#"{"path":"a"}"#.into(),
            }],
        );
        let call = tool_call_for(&model_batch, 0, "read.context", 3, r#"{"path":"a"}"#);
        // Tool work before the initial cursor would exploit the
        // cursor.unwrap_or(0) ambiguity: it must fail closed.
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ValidatedBatch {
                        batch: model_batch.clone(),
                    },
                    JournalEvent::ToolIntent { call },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));
        // A fresh model attempt before takeover skips the pending plan.
        let attempt_id = Uuid::new_v4();
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ValidatedBatch {
                        batch: model_batch.clone(),
                    },
                    JournalEvent::ModelIntent {
                        attempt_id,
                        projection_ref: ProjectionRef::new(),
                    },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));
        // Delegation work before the initial cursor fails the same way.
        let context_refs = vec!["turn:1".into()];
        let (delegation_batch, _) =
            pending_delegation_batch(&admitted, Uuid::new_v4(), context_refs.clone());
        let delegation_request = delegation_request_for(
            &admitted,
            &delegation_batch,
            0,
            "expert-a",
            2,
            "summarize",
            context_refs,
        );
        assert!(matches!(
            project_continuation(
                &admitted,
                &entries(vec![
                    JournalEvent::ValidatedBatch {
                        batch: delegation_batch,
                    },
                    JournalEvent::DelegationIntent {
                        request: delegation_request,
                    },
                ]),
            ),
            Err(AgentFailure::StorageUnavailable)
        ));
    }

    #[test]
    fn leading_resumed_cursor_is_preserved_exactly() {
        let admitted = admitted();
        let model_batch = batch(
            Uuid::new_v4(),
            vec![
                ModelStep::CallTool {
                    tool_id: "read.context".into(),
                    definition_revision: 3,
                    input: r#"{"path":"a"}"#.into(),
                },
                ModelStep::CallTool {
                    tool_id: "read.context".into(),
                    definition_revision: 3,
                    input: r#"{"path":"a"}"#.into(),
                },
            ],
        );
        // A resumed batch may restart past ordinal 0: the exact starting
        // cursor is the takeover claim, never normalized to zero.
        let initial = BatchCursor {
            batch_id: model_batch.batch_id,
            next_step_index: 1,
        };
        let projected = project_journal(
            &admitted.receipt,
            &entries(vec![
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: initial.clone(),
                },
            ]),
        )
        .unwrap();
        assert!(
            matches!(&projected.lineage, JournalLineage::ResumeClaimed { batch, cursor }
                if batch == &model_batch && cursor == &initial),
            "initial resume cursor must be preserved exactly: {:?}",
            projected.lineage
        );
        assert_eq!(projected.cursor, Some(initial.clone()));
        // The claim survives later completion and fresh model attempts.
        let call = tool_call_for(&model_batch, 1, "read.context", 3, r#"{"path":"a"}"#);
        let result = tool_result(call.call_id);
        let fresh_attempt = Uuid::new_v4();
        let mut fresh = answer_batch(
            fresh_attempt,
            model_batch.execution_id,
            ProjectionRef::new(),
        );
        fresh.catalog_revision = model_batch.catalog_revision;
        let fresh_id = fresh.batch_id;
        let projected = project_journal(
            &admitted.receipt,
            &entries(vec![
                JournalEvent::ValidatedBatch {
                    batch: model_batch.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: initial.clone(),
                },
                JournalEvent::ToolIntent { call: call.clone() },
                JournalEvent::ToolResult {
                    result: result.clone(),
                },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: model_batch.batch_id,
                        next_step_index: 2,
                    },
                },
                JournalEvent::Checkpoint { iteration: 1 },
                JournalEvent::ModelIntent {
                    attempt_id: fresh_attempt,
                    projection_ref: fresh.projection_ref,
                },
                JournalEvent::ModelResult {
                    attempt_id: fresh_attempt,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                },
                JournalEvent::ValidatedBatch { batch: fresh },
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: fresh_id,
                        next_step_index: 0,
                    },
                },
            ]),
        )
        .unwrap();
        assert!(
            matches!(&projected.lineage, JournalLineage::ResumeClaimed { batch, cursor }
                if batch == &model_batch && cursor == &initial),
            "lineage must keep the original claim: {:?}",
            projected.lineage
        );
    }
}
