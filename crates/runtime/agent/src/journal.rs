//! One role-neutral parser for canonical execution journals.
use std::collections::{HashMap, HashSet};
use floe_agent_contract::{AgentFailure, Artifact, BatchCursor, DependencyCoverage, JournalEntry,
    JournalEvent, ModelConversation, ModelConversationEntry, ModelStep, ProjectionRef,
    ReplayReceipt, RunId, TaskExecutionEvidence, TaskExecutionReceipt, TaskId, TaskModelAccounting,
    TaskReceipt, TaskState, UnresolvedModelAttempt, ValidatedModelBatch, input_digest};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct JournalExecutionBinding {
    pub principal: String,
    pub device_id: String,
    pub execution_id: Uuid,
    pub catalog_revision: u64,
    pub root_run_id: Option<RunId>,
    pub owning_task_id: Option<TaskId>,
}
impl JournalExecutionBinding {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.principal.is_empty() || self.device_id.is_empty() || self.execution_id.is_nil()
            || (self.owning_task_id.is_some() && self.catalog_revision == 0)
            || self.root_run_id.is_some_and(|id| !id.is_valid())
            || self.owning_task_id.is_some_and(|id| !id.is_valid())
        { return Err(AgentFailure::InvalidInput); }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JournalProjectionMode {
    DurablePrefix,
    Recoverable,
    /// Only the owner of an eligible terminal Run may adopt this settled Task blocker.
    ContinueSettledDelegation,
}

#[derive(Clone, Debug)]
pub enum JournalBlockage {
    SourceRead { call_id: Uuid, blockers: floe_agent_contract::SourceAccessBlockers },
    Delegation { receipt: TaskReceipt },
}

#[derive(Clone, Debug)]
pub struct JournalProjection {
    pub execution_id: Option<Uuid>,
    pub model_conversation: ModelConversation,
    pub replay: Vec<ReplayReceipt>,
    pub pending_batch: Option<ValidatedModelBatch>,
    pub cursor: Option<BatchCursor>,
    pub completed_iterations: u32,
    pub usage: floe_execution::budget::ModelUsage,
    pub lineage: JournalLineage,
    pub journal_revision: u64,
    pub journal_digest: [u8; 32],
    pub output: Option<(String, Vec<Artifact>)>,
    pub blockage: Option<JournalBlockage>,
    pub own_accounting: TaskModelAccounting,
    pub delegated_receipts: Vec<TaskExecutionReceipt>,
    pub attempt_refs: Vec<Uuid>,
    pub unresolved_attempts: Vec<UnresolvedModelAttempt>,
    pub task_refs: Vec<TaskId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JournalLineage {
    Empty,
    Fresh,
    ResumeBatchOnly { batch: ValidatedModelBatch },
    ResumeClaimed { batch: ValidatedModelBatch, cursor: BatchCursor },
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
    parent_task_id: Option<TaskId>,
    projection_ref: ProjectionRef,
    completed: bool,
    batch_bound: bool,
}

pub fn project_execution_journal(
    source: &JournalExecutionBinding,
    entries: &[JournalEntry],
    mode: JournalProjectionMode,
) -> Result<JournalProjection, AgentFailure> {
    source.validate()?;
    validate_journal_capacity(entries)?;
    let storage_validation = mode == JournalProjectionMode::DurablePrefix;
    let mut blockage = None;
    let mut output = None;
    let mut exchanges = Vec::new();
    let mut replay = Vec::new();
    let mut attempts: HashMap<Uuid, AttemptState> = HashMap::new();
    let mut tools: HashMap<Uuid, PendingTool> = HashMap::new();
    let mut seen_calls = HashSet::new();
    let mut delegations: HashMap<TaskId, PendingDelegation> = HashMap::new();
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
    let mut lineage = JournalLineage::Empty;
    let mut output_seen = false;
    for (index, entry) in entries.iter().enumerate() {
        if output_seen || blockage.is_some() || entry.revision != (index as u64) + 1 {
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
                    || pending.is_some()
                    || uncheckpointed_completion
                    || *parent_task_id != source.owning_task_id
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
                state.completed = true;
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
                let expected_key = crate::stable_invocation_key(
                    state.batch.execution_id,
                    state.batch.batch_id,
                    ordinal,
                    crate::InvocationKind::Tool,
                );
                let expected_call = crate::stable_call_id(
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
                    run_id: source.root_run_id,
                    task_id: source.owning_task_id,
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
                    task_execution: None,
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
            JournalEvent::ToolReviewRequired { call_id, blockers } => {
                blockers.validate().map_err(|_| AgentFailure::StorageUnavailable)?;
                let settled = tools.remove(call_id).ok_or(AgentFailure::StorageUnavailable)?;
                let state = pending.as_mut().ok_or(AgentFailure::StorageUnavailable)?;
                if state.cursor != Some(settled.ordinal) || state.settled_step.is_some() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                state.settled_step = Some(settled.ordinal);
                blockage = Some(JournalBlockage::SourceRead { call_id: *call_id, blockers: blockers.clone() });
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
                let expected_key = crate::stable_invocation_key(
                    state.batch.execution_id,
                    state.batch.batch_id,
                    ordinal,
                    crate::InvocationKind::Delegation,
                );
                let expected_task = crate::stable_task_id(
                    state.batch.execution_id,
                    state.batch.batch_id,
                    ordinal,
                );
                if request.invocation_key != expected_key || request.task_id != expected_task {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if !request.task_id.is_valid()
                    || request.parent_run_id != source.root_run_id.map(|id| id.as_uuid())
                    || source.owning_task_id.is_some()
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
                if state.cursor.unwrap_or(0) != settled.ordinal || state.settled_step.is_some() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let request = settled.request;
                receipt.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
                state.settled_step = Some(settled.ordinal);
                if receipt.snapshot.task_id != request.task_id
                    || (receipt.snapshot.parent_run_id != request.parent_run_id
                        && !matches!(&lineage, JournalLineage::ResumeClaimed { .. }))
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
                if receipt.snapshot.parent_run_id != request.parent_run_id {
                    let replayed = receipt.replay.as_ref().ok_or(AgentFailure::StorageUnavailable)?;
                    let mut original = request.clone();
                    original.parent_run_id = receipt.snapshot.parent_run_id;
                    if replayed.invocation_key != request.invocation_key
                        || replayed.task_id != Some(request.task_id)
                        || replayed.input_digest != floe_agent_contract::delegation_request_digest(&original)
                        || match &receipt.execution {
                            TaskExecutionEvidence::Admitted(execution) => replayed.task_execution.as_ref() != Some(execution),
                            TaskExecutionEvidence::Unadmitted => replayed.task_execution.is_some(),
                        }
                    { return Err(AgentFailure::StorageUnavailable); }
                }
                let blocked = receipt.snapshot.state == TaskState::Blocked;
                if blocked {
                    blockage = Some(JournalBlockage::Delegation { receipt: receipt.as_ref().clone() });
                }
                let text = receipt
                    .snapshot
                    .result
                    .clone()
                    .unwrap_or_else(|| format!("task {:?}", receipt.snapshot.state));
                let mut original_request = request.clone();
                original_request.parent_run_id = receipt.snapshot.parent_run_id;
                let input_digest = floe_agent_contract::delegation_request_digest(&original_request);
                if !blocked {
                    exchanges.push(ModelConversationEntry::DelegationExchange {
                        request: original_request,
                        receipt: receipt.as_ref().clone(),
                    });
                }
                replay.push(ReplayReceipt {
                    principal: request.principal,
                    run_id: receipt.snapshot.parent_run_id.and_then(RunId::from_uuid),
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
                    task_execution: match &receipt.execution {
                        TaskExecutionEvidence::Admitted(value) => Some(value.clone()),
                        TaskExecutionEvidence::Unadmitted => None,
                    },
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
                        || state.parent_task_id != source.owning_task_id
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    state.batch_bound = true;
                } else if batches_seen != 0 {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if batch.catalog_revision != source.catalog_revision {
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
                        if (finalization_prior_execution.is_none() && batch.execution_id != source.execution_id)
                            || finalization_prior_execution == Some(batch.execution_id) {
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
                    let message_id = crate::stable_preamble_id(
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
                output = Some((text.clone(), artifacts.clone()));
                pending = None;
            }
        }
    }
    if !storage_validation && blockage.is_some()
        && !(mode == JournalProjectionMode::ContinueSettledDelegation
            && matches!(&blockage, Some(JournalBlockage::Delegation { .. })))
    { return Err(AgentFailure::AccessReviewRequired); }
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
    let (own_accounting, delegated_receipts, total) = project_model_accounting(entries)?;
    let task_refs = entries.iter().filter_map(|entry| match &entry.event {
        JournalEvent::DelegationIntent { request } => Some(request.task_id), _ => None,
    }).collect();
    Ok(JournalProjection {
        journal_revision: entries.last().map_or(0, |entry| entry.revision),
        journal_digest: journal_digest(entries)?,
        output,
        blockage,
        attempt_refs: total.attempt_refs,
        unresolved_attempts: total.unresolved_attempts,
        own_accounting,
        delegated_receipts,
        task_refs,
        execution_id,
        model_conversation: ModelConversation {
            history: Vec::new(),
            current_turn: exchanges,
        },
        replay,
        pending_batch,
        cursor,
        completed_iterations,
        usage: total.usage,
        lineage,
    })
}


pub fn journal_digest(entries: &[JournalEntry]) -> Result<[u8; 32], AgentFailure> {
    let encoded = serde_json::to_vec(&("floe.execution-journal.sha256.v1", entries))
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    Ok(Sha256::digest(encoded).into())
}

/// An acknowledged intent always has room for its terminal settlement.
pub fn validate_journal_capacity(entries: &[JournalEntry]) -> Result<(), AgentFailure> {
    let mut models = HashSet::new();
    let mut tools = HashSet::new();
    let mut tasks = HashSet::new();
    for entry in entries {
        match &entry.event {
            JournalEvent::ModelIntent { attempt_id, .. } => { models.insert(*attempt_id); }
            JournalEvent::ModelResult { attempt_id, .. } => { models.remove(attempt_id); }
            JournalEvent::ToolIntent { call } => { tools.insert(call.call_id); }
            JournalEvent::ToolResult { result } => { tools.remove(&result.call_id); }
            JournalEvent::ToolReviewRequired { call_id, .. } => { tools.remove(call_id); }
            JournalEvent::DelegationIntent { request } => { tasks.insert(request.task_id); }
            JournalEvent::DelegationResult { receipt } => { tasks.remove(&receipt.task_id); }
            _ => {}
        }
    }
    if entries.len() + models.len() + tools.len() + tasks.len() > 512 {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(())
}

fn checked_add(value: &mut u64, amount: u64) -> Result<(), AgentFailure> {
    *value = value.checked_add(amount).ok_or(AgentFailure::StorageUnavailable)?;
    Ok(())
}

fn append_accounting(total: &mut TaskModelAccounting, value: &TaskModelAccounting)
    -> Result<(), AgentFailure>
{
    value.validate()?;
    if value.attempt_refs.iter().any(|id| total.attempt_refs.contains(id)) {
        return Err(AgentFailure::StorageUnavailable);
    }
    total.attempt_refs.extend_from_slice(&value.attempt_refs);
    total.unresolved_attempts.extend_from_slice(&value.unresolved_attempts);
    total.usage.attempts = total.usage.attempts.checked_add(value.usage.attempts)
        .ok_or(AgentFailure::StorageUnavailable)?;
    total.unknown_token_attempts = total.unknown_token_attempts.checked_add(value.unknown_token_attempts)
        .ok_or(AgentFailure::StorageUnavailable)?;
    total.unknown_cost_attempts = total.unknown_cost_attempts.checked_add(value.unknown_cost_attempts)
        .ok_or(AgentFailure::StorageUnavailable)?;
    checked_add(&mut total.usage.tokens, value.usage.tokens)?;
    checked_add(&mut total.usage.cost_micros, value.usage.cost_micros)?;
    checked_add(&mut total.usage.estimated_tokens, value.usage.estimated_tokens)?;
    checked_add(&mut total.usage.estimated_cost_micros, value.usage.estimated_cost_micros)?;
    total.validate()
}

/// Compose distinct root journals and immutable Task receipts across a continuation chain.
/// A repeated Task execution contributes once; changed evidence for that execution is corruption.
pub fn aggregate_model_accounting(
    own: &[TaskModelAccounting],
    delegated: &[TaskExecutionReceipt],
) -> Result<TaskModelAccounting, AgentFailure> {
    let mut total = TaskModelAccounting::default();
    for value in own { append_accounting(&mut total, value)?; }
    let mut executions = HashMap::new();
    for receipt in delegated {
        receipt.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
        if let Some(prior) = executions.insert(receipt.reference.execution, receipt) {
            if prior != receipt { return Err(AgentFailure::StorageUnavailable); }
            continue;
        }
        append_accounting(&mut total, &receipt.accounting)?;
    }
    Ok(total)
}

fn project_model_accounting(entries: &[JournalEntry])
    -> Result<(TaskModelAccounting, Vec<TaskExecutionReceipt>, TaskModelAccounting), AgentFailure>
{
    let mut own = TaskModelAccounting::default();
    let mut pending = HashMap::new();
    let mut delegated = Vec::new();
    for entry in entries {
        match &entry.event {
            JournalEvent::ModelIntent { attempt_id, reservation_ceiling, .. } => {
                if own.attempt_refs.contains(attempt_id) { return Err(AgentFailure::StorageUnavailable); }
                own.attempt_refs.push(*attempt_id);
                own.usage.attempts = own.usage.attempts.checked_add(1)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                pending.insert(*attempt_id, *reservation_ceiling);
            }
            JournalEvent::ModelResult { attempt_id, usage, accounting } => {
                pending.remove(attempt_id).ok_or(AgentFailure::StorageUnavailable)?;
                accounting.validate_charge(usage.tokens, usage.cost_micros)?;
                checked_add(&mut own.usage.tokens, usage.tokens)?;
                checked_add(&mut own.usage.cost_micros, usage.cost_micros)?;
                if accounting.unknown_tokens {
                    checked_add(&mut own.usage.estimated_tokens, usage.tokens)?;
                    own.unknown_token_attempts = own.unknown_token_attempts.checked_add(1).ok_or(AgentFailure::StorageUnavailable)?;
                }
                if accounting.unknown_cost {
                    checked_add(&mut own.usage.estimated_cost_micros, usage.cost_micros)?;
                    own.unknown_cost_attempts = own.unknown_cost_attempts.checked_add(1).ok_or(AgentFailure::StorageUnavailable)?;
                }
            }
            JournalEvent::DelegationResult { receipt } => {
                if let TaskExecutionEvidence::Admitted(execution) = &receipt.execution {
                    delegated.push(execution.clone());
                }
            }
            _ => {}
        }
    }
    for attempt_id in &own.attempt_refs {
        if let Some(ceiling) = pending.get(attempt_id) {
            checked_add(&mut own.usage.tokens, ceiling.tokens)?;
            checked_add(&mut own.usage.cost_micros, ceiling.cost_micros)?;
            checked_add(&mut own.usage.estimated_tokens, ceiling.tokens)?;
            checked_add(&mut own.usage.estimated_cost_micros, ceiling.cost_micros)?;
            own.unknown_token_attempts = own.unknown_token_attempts.checked_add(1).ok_or(AgentFailure::StorageUnavailable)?;
            own.unknown_cost_attempts = own.unknown_cost_attempts.checked_add(1).ok_or(AgentFailure::StorageUnavailable)?;
            own.unresolved_attempts.push(UnresolvedModelAttempt {
                attempt_id: *attempt_id,
                reservation_ceiling: *ceiling,
                accounting: floe_agent_contract::ModelAccounting {
                    observed_tokens: None, observed_cost_micros: None,
                    unknown_tokens: true, unknown_cost: true,
                },
            });
        }
    }
    own.validate()?;
    let total = aggregate_model_accounting(std::slice::from_ref(&own), &delegated)?;
    Ok((own, delegated, total))
}
