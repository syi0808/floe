use std::collections::{HashMap, HashSet};

use floe_agent_contract::{
    AgentMessage, DependencyCoverage, JournalEvent, MessageRole, ReplayReceipt, TaskState,
    input_digest,
};
use floe_kernel::AgentFailure;

use crate::{AdmittedTurn, ContinuationSnapshot, JournalEntry, RunReceipt, RunState};

pub(super) struct JournalProjection {
    pub(super) messages: Vec<AgentMessage>,
    pub(super) replay: Vec<ReplayReceipt>,
    pub(super) completed_iterations: u32,
    pub(super) usage: floe_execution::budget::ModelUsage,
}

pub fn project_continuation(
    admitted: &AdmittedTurn,
    entries: &[JournalEntry],
) -> Result<ContinuationSnapshot, AgentFailure> {
    admitted.validate()?;
    let projected = project_journal(&admitted.receipt, entries)?;
    let mut messages = admitted.transcript.clone();
    messages.extend(projected.messages);
    if messages.len() > floe_agent_contract::MAX_AGENT_MESSAGES || projected.replay.len() > 128 {
        return Err(AgentFailure::BudgetExceeded);
    }
    messages.iter().try_for_each(AgentMessage::validate)?;
    Ok(ContinuationSnapshot {
        reference: admitted
            .receipt
            .continuation()
            .ok_or(AgentFailure::Conflict)?,
        session_id: admitted.receipt.session_id,
        session_revision: admitted.receipt.session_revision,
        execution_profile: admitted.receipt.execution_profile.clone(),
        messages,
        replay: projected.replay,
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

fn project_entries(
    source: &RunReceipt,
    entries: &[JournalEntry],
) -> Result<JournalProjection, AgentFailure> {
    let mut messages = Vec::new();
    let mut replay = Vec::new();
    let mut attempts = HashSet::new();
    let mut seen_attempts = HashSet::new();
    let mut tools = HashMap::new();
    let mut seen_calls = HashSet::new();
    let mut delegations = HashMap::new();
    let mut seen_tasks = HashSet::new();
    let mut seen_invocations = HashSet::new();
    let mut completed_iterations = 0;
    let mut usage = floe_execution::budget::ModelUsage::default();
    for (index, entry) in entries.iter().enumerate() {
        if entry.revision != (index as u64) + 1 {
            return Err(AgentFailure::StorageUnavailable);
        }
        match &entry.event {
            JournalEvent::ModelIntent { attempt_id } => {
                if attempt_id.is_nil()
                    || !seen_attempts.insert(*attempt_id)
                    || !attempts.insert(*attempt_id)
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
                    tool_id: Some(call.tool_id),
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
                messages.push(AgentMessage {
                    message_id: result.call_id,
                    role: MessageRole::Tool,
                    text: result.text.clone(),
                    call_id: Some(result.call_id),
                    coverage: result.coverage.clone(),
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
                messages.push(AgentMessage {
                    message_id: receipt.task_id.as_uuid(),
                    role: MessageRole::Delegation,
                    text: text.clone(),
                    call_id: None,
                    coverage: receipt.snapshot.coverage.clone(),
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
                if *iteration != completed_iterations + 1 || *iteration > 64 {
                    return Err(AgentFailure::StorageUnavailable);
                }
                completed_iterations = *iteration;
            }
            JournalEvent::Output { .. } => return Err(AgentFailure::Conflict),
        }
    }
    if !attempts.is_empty() || !tools.is_empty() || !delegations.is_empty() {
        return Err(AgentFailure::Interrupted);
    }
    messages.iter().try_for_each(AgentMessage::validate)?;
    Ok(JournalProjection {
        messages,
        replay,
        completed_iterations,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use floe_agent_contract::{CommandId, InvocationKey, ModelUsage, RunId, ToolCall, ToolResult};
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

    #[test]
    fn projection_pairs_settled_work_and_rejects_an_unsettled_intent() {
        let admitted = admitted();
        let attempt_id = Uuid::new_v4();
        let call = ToolCall {
            call_id: Uuid::new_v4(),
            invocation_key: InvocationKey::new(),
            tool_id: "read.context".into(),
            definition_revision: 1,
            input: "{}".into(),
        };
        let result = ToolResult {
            call_id: call.call_id,
            text: "bounded result".into(),
            artifacts: vec![],
            coverage: DependencyCoverage::Independent,
            issue: None,
        };
        let events = [
            JournalEvent::ModelIntent { attempt_id },
            JournalEvent::ModelResult {
                attempt_id,
                usage: ModelUsage {
                    tokens: 1,
                    cost_micros: 1,
                },
            },
            JournalEvent::ToolIntent { call: call.clone() },
            JournalEvent::ToolResult {
                result: result.clone(),
            },
            JournalEvent::Checkpoint { iteration: 1 },
        ];
        let entries = events
            .into_iter()
            .enumerate()
            .map(|(index, event)| JournalEntry {
                revision: index as u64 + 1,
                event,
            })
            .collect::<Vec<_>>();
        let snapshot = project_continuation(&admitted, &entries).unwrap();
        assert_eq!(snapshot.reference.run_id, admitted.receipt.run_id);
        assert_eq!(snapshot.completed_iterations, 1);
        assert_eq!(snapshot.usage.attempts, 1);
        assert_eq!(snapshot.usage.tokens, 1);
        assert_eq!(snapshot.usage.cost_micros, 1);
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(snapshot.replay.len(), 1);
        assert_eq!(snapshot.replay[0].call_id, call.call_id);
        assert_eq!(snapshot.replay[0].result, result.text);

        let unsettled = vec![JournalEntry {
            revision: 1,
            event: JournalEvent::ToolIntent { call },
        }];
        assert!(matches!(
            project_continuation(&admitted, &unsettled),
            Err(AgentFailure::Interrupted)
        ));
    }
}
