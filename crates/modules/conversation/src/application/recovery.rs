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
    pub(super) execution_id: Option<Uuid>,
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
    ResumeBatchOnly {
        batch: ValidatedModelBatch,
    },
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
    project_entries(source, entries, false)
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
    parent_task_id: Option<floe_kernel::TaskId>,
    projection_ref: ProjectionRef,
    completed: bool,
    batch_bound: bool,
}

fn project_entries(
    source: &RunReceipt,
    entries: &[JournalEntry],
    storage_validation: bool,
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
    let mut pending: Option<PendingBatch> = None;
    let mut uncheckpointed_completion = false;
    let mut completed_iterations: u32 = 0;
    let mut segment_iterations = 0;
    let mut finalization_prior_execution = None;
    let mut usage = floe_execution::budget::ModelUsage::default();
    let mut lineage = JournalLineage::Empty;
    let mut output_seen = false;
    for (index, entry) in entries.iter().enumerate() {
        if output_seen || entry.revision != (index as u64) + 1 {
            return Err(AgentFailure::StorageUnavailable);
        }
        match &entry.event {
            JournalEvent::FinalizationStarted {
                prior_execution_id,
                abandoned_cursor,
                prior_exhaustion,
            } => {
                let expected_cursor = pending.as_ref().and_then(|state| {
                    state.cursor.map(|next_step_index| BatchCursor {
                        batch_id: state.batch.batch_id,
                        next_step_index,
                    })
                });
                if finalization_prior_execution.is_some()
                    || execution_id != Some(*prior_execution_id)
                    || prior_execution_id.is_nil()
                    || !matches!(
                        prior_exhaustion,
                        AgentFailure::BudgetExceeded | AgentFailure::Stalled
                    )
                    || expected_cursor != *abandoned_cursor
                    || pending.as_ref().is_some_and(|state| state.cursor.is_none())
                    || !tools.is_empty()
                    || !delegations.is_empty()
                    || attempts.values().any(|attempt| !attempt.completed)
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                pending = None;
                uncheckpointed_completion = false;
                finalization_prior_execution = Some(*prior_execution_id);
                execution_id = None;
                segment_iterations = 0;
            }
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref,
                plan,
                parent_task_id,
                reservation_ceiling,
            } => {
                // A resumed batch without its initial cursor never started;
                // a local attempt before that cursor is a skipped takeover.
                if matches!(lineage, JournalLineage::ResumeBatchOnly { .. }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if attempt_id.is_nil()
                    || projection_ref.as_uuid().is_nil()
                    || attempts.contains_key(attempt_id)
                    || attempts
                        .values()
                        .any(|prior| !prior.completed && prior.parent_task_id == *parent_task_id)
                    || (parent_task_id.is_none() && pending.is_some())
                    || (parent_task_id.is_none() && uncheckpointed_completion)
                    || parent_task_id.is_some_and(|task_id| !delegations.contains_key(&task_id))
                    || reservation_ceiling.validate().is_err()
                    || plan.validate().is_err()
                    || plan.principal != source.principal
                    || plan.device_id != source.device_id
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                attempts.insert(
                    *attempt_id,
                    AttemptState {
                        parent_task_id: *parent_task_id,
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
                accounting,
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
                accounting
                    .validate_charge(result_usage.tokens, result_usage.cost_micros)
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                if state
                    .parent_task_id
                    .is_some_and(|task_id| !delegations.contains_key(&task_id))
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                state.completed = true;
                if accounting.unknown_cost {
                    usage.estimated_cost_micros = usage
                        .estimated_cost_micros
                        .checked_add(result_usage.cost_micros)
                        .ok_or(AgentFailure::StorageUnavailable)?;
                }
                if accounting.unknown_tokens {
                    usage.estimated_tokens = usage
                        .estimated_tokens
                        .checked_add(result_usage.tokens)
                        .ok_or(AgentFailure::StorageUnavailable)?;
                }
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
                if state.cursor.unwrap_or(0) != settled.ordinal || state.settled_step.is_some() {
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
                // The intent carries the exact context bound to the validated
                // batch: a missing, malformed, or substituted context is
                // journal corruption, never silently reconstructed.
                request
                    .execution_context
                    .validate()
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
                if state.batch.delegation_context.as_ref() != Some(&request.execution_context) {
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
                if request.invocation_key != expected_key || request.task_id != expected_task {
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
                if attempts.values().any(|attempt| {
                    !attempt.completed && attempt.parent_task_id == Some(receipt.task_id)
                }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if matches!(lineage, JournalLineage::ResumeBatchOnly { .. }) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let settled = delegations
                    .remove(&receipt.task_id)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let Some(state) = pending.as_mut() else {
                    return Err(AgentFailure::StorageUnavailable);
                };
                if state.cursor.unwrap_or(0) != settled.ordinal || state.settled_step.is_some() {
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
                let input_digest = floe_agent_contract::delegation_request_digest(&request);
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
                    input_digest,
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
                    || *iteration != segment_iterations + 1
                    || *iteration > 64
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                uncheckpointed_completion = false;
                segment_iterations = *iteration;
                completed_iterations += 1;
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
                        || state.parent_task_id.is_some()
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    state.batch_bound = true;
                } else if batches_seen != 0 {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if batch.catalog_revision != source.expert_environment.revision {
                    return Err(AgentFailure::StorageUnavailable);
                }
                // Every batch in one journal — resumed re-record included —
                // runs under the execution that wrote the journal.
                match execution_id {
                    Some(expected) if batch.execution_id != expected => {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    Some(_) => {}
                    None => {
                        if finalization_prior_execution == Some(batch.execution_id) {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        execution_id = Some(batch.execution_id);
                    }
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
                                ModelStep::CallTool { .. } | ModelStep::Delegate { .. } => {
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
                let completed = pending.as_ref().is_some_and(|state| {
                    cursor.next_step_index as usize == state.batch.steps.len()
                });
                if completed {
                    pending = None;
                    uncheckpointed_completion = true;
                }
            }
            JournalEvent::Output { text, artifacts } => {
                if !storage_validation {
                    return Err(AgentFailure::Conflict);
                }
                let state = pending.as_ref().ok_or(AgentFailure::StorageUnavailable)?;
                if state.cursor.is_none()
                    || state.settled_step.is_some()
                    || !tools.is_empty()
                    || !delegations.is_empty()
                    || attempts.values().any(|attempt| !attempt.completed)
                    || state.next_index() + 1 != state.batch.steps.len()
                    || !matches!(state.next_step()?, ModelStep::Answer { text: expected, artifacts: expected_artifacts }
                        if text == expected && artifacts == expected_artifacts)
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                output_seen = true;
                pending = None;
            }
        }
    }
    if !storage_validation && attempts.values().any(|state| !state.completed) {
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
        execution_id,
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

/// Validate a durable journal prefix, including an acknowledged in-flight
/// intent. Recovery uses the same parser but additionally refuses reissue of
/// any model attempt whose terminal result is missing.
pub fn validate_run_journal(
    source: &RunReceipt,
    entries: &[JournalEntry],
) -> Result<(), AgentFailure> {
    source.validate()?;
    if entries.len() > 512 {
        return Err(AgentFailure::BudgetExceeded);
    }
    project_entries(source, entries, true)?;
    // Every acknowledged intent reserves one journal slot for its settlement.
    // Capacity exhaustion can stop new work, never strand terminal evidence.
    let mut pending_models = HashSet::new();
    let mut pending_tools = HashSet::new();
    let mut pending_tasks = HashSet::new();
    for entry in entries {
        match &entry.event {
            JournalEvent::ModelIntent { attempt_id, .. } => {
                pending_models.insert(*attempt_id);
            }
            JournalEvent::ModelResult { attempt_id, .. } => {
                pending_models.remove(attempt_id);
            }
            JournalEvent::ToolIntent { call } => {
                pending_tools.insert(call.call_id);
            }
            JournalEvent::ToolResult { result } => {
                pending_tools.remove(&result.call_id);
            }
            JournalEvent::DelegationIntent { request } => {
                pending_tasks.insert(request.task_id);
            }
            JournalEvent::DelegationResult { receipt } => {
                pending_tasks.remove(&receipt.task_id);
            }
            _ => {}
        }
    }
    if entries.len() + pending_models.len() + pending_tools.len() + pending_tasks.len() > 512 {
        return Err(AgentFailure::BudgetExceeded);
    }
    super::storage_projection::project_run_accounting(source, entries)?;
    Ok(())
}
