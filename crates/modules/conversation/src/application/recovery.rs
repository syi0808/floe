use std::collections::{HashMap, HashSet};

use floe_agent_contract::{
    AgentMessage, BatchCursor, DependencyCoverage, JournalEvent, MessageRole, ModelConversation,
    ModelConversationEntry, ReplayReceipt, TaskState, ValidatedModelBatch, input_digest,
};
use floe_kernel::AgentFailure;

use crate::{AdmittedTurn, ContinuationSnapshot, JournalEntry, RunReceipt, RunState};

pub(super) struct JournalProjection {
    pub(super) model_conversation: ModelConversation,
    pub(super) replay: Vec<ReplayReceipt>,
    pub(super) pending_batch: Option<ValidatedModelBatch>,
    pub(super) cursor: Option<BatchCursor>,
    pub(super) completed_iterations: u32,
    pub(super) usage: floe_execution::budget::ModelUsage,
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
        execution_profile: admitted.receipt.execution_profile.clone(),
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

struct PendingBatch {
    batch: ValidatedModelBatch,
    /// Whether the batch's attempt completed in this journal. A batch without
    /// a local attempt is a resumed batch re-recorded at the journal's start.
    fresh: bool,
    /// Last acknowledged cursor, if any progress was recorded.
    cursor: Option<u32>,
}

fn project_entries(
    source: &RunReceipt,
    entries: &[JournalEntry],
) -> Result<JournalProjection, AgentFailure> {
    let mut exchanges = Vec::new();
    let mut replay = Vec::new();
    let mut attempts = HashSet::new();
    let mut seen_attempts = HashSet::new();
    let mut completed_attempts = HashSet::new();
    let mut tools = HashMap::new();
    let mut seen_calls = HashSet::new();
    let mut delegations = HashMap::new();
    let mut seen_tasks = HashSet::new();
    let mut seen_invocations = HashSet::new();
    let mut seen_batches = HashSet::new();
    let mut batches_seen: u32 = 0;
    let mut pending: Option<PendingBatch> = None;
    let mut uncheckpointed_completion = false;
    let mut completed_iterations = 0;
    let mut usage = floe_execution::budget::ModelUsage::default();
    for (index, entry) in entries.iter().enumerate() {
        if entry.revision != (index as u64) + 1 {
            return Err(AgentFailure::StorageUnavailable);
        }
        match &entry.event {
            JournalEvent::ModelIntent {
                attempt_id,
                projection_ref,
            } => {
                if attempt_id.is_nil()
                    || projection_ref.as_uuid().is_nil()
                    || !seen_attempts.insert(*attempt_id)
                    || !attempts.insert(*attempt_id)
                    || pending.is_some()
                    || uncheckpointed_completion
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            JournalEvent::ModelResult {
                attempt_id,
                usage: result_usage,
            } => {
                if !attempts.remove(attempt_id) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                completed_attempts.insert(*attempt_id);
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
                if call.call_id.is_nil()
                    || call.invocation_key.as_uuid().is_nil()
                    || !seen_invocations.insert(call.invocation_key)
                    || call.tool_id.trim().is_empty()
                    || call.definition_revision == 0
                    || floe_agent_contract::validate_tool_input(&call.input).is_err()
                    || !seen_calls.insert(call.call_id)
                    || tools.insert(call.call_id, call.clone()).is_some()
                    || pending.is_none()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            JournalEvent::ToolResult { result } => {
                let call = tools
                    .remove(&result.call_id)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                result.validate(call.call_id, floe_agent_contract::MAX_OUTPUT_BYTES)?;
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
                if !request.task_id.is_valid()
                    || request.parent_run_id != Some(source.run_id.as_uuid())
                    || request.principal != source.principal
                    || request.invocation_key.as_uuid().is_nil()
                    || !seen_invocations.insert(request.invocation_key)
                    || request.selected_agent_id.trim().is_empty()
                    || request.selected_definition_revision == 0
                    || request.message.trim().is_empty()
                    || request.message.len() > floe_agent_contract::MAX_OUTPUT_BYTES
                    || request.context_refs.len() > 128
                    || !seen_tasks.insert(request.task_id)
                    || delegations
                        .insert(request.task_id, request.clone())
                        .is_some()
                    || pending.is_none()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            JournalEvent::DelegationResult { receipt } => {
                let request = delegations
                    .remove(&receipt.task_id)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                receipt
                    .snapshot
                    .validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
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
                let fresh = seen_attempts.contains(&batch.attempt_id);
                if fresh {
                    if !completed_attempts.contains(&batch.attempt_id) {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                } else if batches_seen != 0 {
                    return Err(AgentFailure::StorageUnavailable);
                }
                batches_seen += 1;
                pending = Some(PendingBatch {
                    batch: batch.clone(),
                    fresh,
                    cursor: None,
                });
            }
            JournalEvent::BatchProgress { cursor } => {
                cursor
                    .validate()
                    .map_err(|_| AgentFailure::StorageUnavailable)?;
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
                        // A fresh batch always starts at zero; a resumed batch
                        // restarts wherever its cursor was.
                        if state.fresh && cursor.next_step_index != 0 {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        state.cursor = Some(cursor.next_step_index);
                    }
                    Some(last) => {
                        if cursor.next_step_index <= last {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        state.cursor = Some(cursor.next_step_index);
                    }
                }
                if cursor.next_step_index as usize == state.batch.steps.len() {
                    pending = None;
                    uncheckpointed_completion = true;
                }
            }
            JournalEvent::Output { .. } => return Err(AgentFailure::Conflict),
        }
    }
    if !attempts.is_empty() {
        // A model attempt that never produced a result leaves nothing to resume.
        return Err(AgentFailure::Interrupted);
    }
    if !tools.is_empty() || !delegations.is_empty() {
        if pending.is_none() {
            return Err(AgentFailure::StorageUnavailable);
        }
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
                execution_profile: "test-local".into(),
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
        }
    }

    #[test]
    fn projection_pairs_settled_work_and_rejects_an_unsettled_intent() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let call = tool_call();
        let result = tool_result(call.call_id);
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: call.tool_id.clone(),
                definition_revision: call.definition_revision,
                input: call.input.clone(),
            }],
        );
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
        let call = tool_call();
        let result = tool_result(call.call_id);
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: call.tool_id.clone(),
                definition_revision: call.definition_revision,
                input: call.input.clone(),
            }],
        );
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
        let task_id = floe_agent_contract::TaskId::new();
        let request = DelegationRequest {
            task_id,
            parent_run_id: Some(admitted.receipt.run_id.as_uuid()),
            principal: admitted.receipt.principal.clone(),
            invocation_key: InvocationKey::new(),
            selected_agent_id: "expert-a".into(),
            selected_definition_revision: 2,
            message: "summarize".into(),
            context_refs: vec!["turn:1".into(), "evidence:9".into()],
        };
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
        let model_batch = ValidatedModelBatch {
            execution_id: Uuid::new_v4(),
            attempt_id,
            projection_ref: ProjectionRef::new(),
            batch_id: Uuid::new_v4(),
            steps: vec![ModelStep::Delegate {
                agent_id: request.selected_agent_id.clone(),
                definition_revision: request.selected_definition_revision,
                message: request.message.clone(),
                context_refs: request.context_refs.clone(),
            }],
            catalog_revision: 1,
            tool_revisions: vec![],
            agent_revisions: vec![floe_agent_contract::PinnedAgentRevision {
                agent_id: request.selected_agent_id.clone(),
                definition_revision: request.selected_definition_revision,
            }],
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
        let call = tool_call();
        let result = tool_result(call.call_id);
        // Two steps, one settled: the projection must return the pending batch
        // with its cursor instead of discarding it as interrupted.
        let model_batch = batch(
            attempt_id,
            vec![
                ModelStep::CallTool {
                    tool_id: call.tool_id.clone(),
                    definition_revision: call.definition_revision,
                    input: call.input.clone(),
                },
                ModelStep::CallTool {
                    tool_id: call.tool_id.clone(),
                    definition_revision: call.definition_revision,
                    input: call.input.clone(),
                },
            ],
        );
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

    #[test]
    fn settled_result_with_stale_cursor_is_recoverable() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let call = tool_call();
        let result = tool_result(call.call_id);
        // The cursor ack was lost after the result ack: the result is settled
        // and the batch is still pending from the last acked cursor.
        let model_batch = batch(
            attempt_id,
            vec![ModelStep::CallTool {
                tool_id: call.tool_id.clone(),
                definition_revision: call.definition_revision,
                input: call.input.clone(),
            }],
        );
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
    fn resumed_batch_re_recorded_first_needs_no_local_attempt() {
        let admitted = admitted();
        let call = tool_call();
        let result = tool_result(call.call_id);
        // No model intent in this journal: the batch was validated by an older
        // run and re-recorded here before its steps resume.
        let model_batch = batch(
            Uuid::new_v4(),
            vec![ModelStep::CallTool {
                tool_id: call.tool_id.clone(),
                definition_revision: call.definition_revision,
                input: call.input.clone(),
            }],
        );
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
}
