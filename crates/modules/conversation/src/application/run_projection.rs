//! Conversation-owned execution and reply semantics for product observation.
use crate::{RunEventRecord, RunReceipt, RunState};
use floe_kernel::{AgentFailure, RunId};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicRunState {
    Accepted,
    Executing,
    Finalizing,
    Cancelling,
    Blocked,
    Finished,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnExecution {
    Completed,
    Partial,
    Failed,
    Cancelled,
    Indeterminate,
    Blocked,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyStatus {
    Generated,
    NotProduced,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TurnReport {
    pub execution: TurnExecution,
    pub reply: ReplyStatus,
    pub issues: Vec<AgentFailure>,
    pub action_refs: Vec<Uuid>,
    pub interaction_refs: Vec<Uuid>,
    pub final_message_ref: Option<Uuid>,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct RunSnapshot {
    pub run_id: RunId,
    pub session_id: Uuid,
    pub revision: u64,
    pub executor_generation: u64,
    pub state: PublicRunState,
    pub progress: String,
    pub task_refs: Vec<Uuid>,
    pub attempt_refs: Vec<Uuid>,
    pub report: Option<TurnReport>,
}

pub fn project_run_snapshot(run: &RunReceipt) -> Result<RunSnapshot, AgentFailure> {
    run.validate()?;
    project_run_event(RunEventRecord {
        run_id: run.run_id,
        session_id: run.session_id,
        aggregate_revision: run.aggregate_revision,
        executor_generation: run.executor_generation,
        state: run.state,
        generated_reply: run.output.is_some(),
        issue: run.issue,
        attempt_refs: run.attempt_refs.clone(),
        task_refs: run.task_refs.clone(),
        interaction_refs: run
            .blocked
            .as_ref()
            .map_or_else(Vec::new, |blocked| blocked.interaction_refs()),
    })
}

pub fn project_run_event(run: RunEventRecord) -> Result<RunSnapshot, AgentFailure> {
    if !run.run_id.is_valid()
        || run.session_id.is_nil()
        || run.aggregate_revision == 0
        || run.executor_generation == 0
        || run.attempt_refs.len() > floe_execution::budget::MAX_MODEL_ATTEMPTS_PER_SCOPE
        || run.task_refs.len() > 64
        || run.interaction_refs.len() > crate::MAX_ACTIVE_INTERACTIONS_PER_RUN
        || !valid_refs(&run.attempt_refs)
        || !valid_refs(&run.task_refs)
        || !valid_refs(&run.interaction_refs)
        || run.state == RunState::Blocked
            && (run.generated_reply || run.issue.is_some() || run.interaction_refs.is_empty())
        || run.state != RunState::Blocked && !run.interaction_refs.is_empty()
        || run.state == RunState::Working && (run.generated_reply || run.issue.is_some())
        || run.state == RunState::Completed && (!run.generated_reply || run.issue.is_some())
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    let report = if run.state.is_terminal() {
        Some(TurnReport {
            execution: match run.state {
                RunState::Completed => TurnExecution::Completed,
                RunState::Blocked => TurnExecution::Blocked,
                RunState::Failed if run.generated_reply => TurnExecution::Partial,
                RunState::Failed | RunState::TimedOut => TurnExecution::Failed,
                RunState::Cancelled => TurnExecution::Cancelled,
                RunState::Interrupted | RunState::Working => TurnExecution::Indeterminate,
            },
            reply: if run.generated_reply {
                ReplyStatus::Generated
            } else {
                ReplyStatus::NotProduced
            },
            issues: run.issue.into_iter().collect(),
            action_refs: vec![],
            interaction_refs: run.interaction_refs,
            final_message_ref: run.generated_reply.then_some(run.run_id.as_uuid()),
        })
    } else {
        None
    };
    Ok(RunSnapshot {
        run_id: run.run_id,
        session_id: run.session_id,
        revision: run.aggregate_revision,
        executor_generation: run.executor_generation,
        state: match run.state {
            RunState::Working => PublicRunState::Executing,
            RunState::Blocked => PublicRunState::Blocked,
            _ => PublicRunState::Finished,
        },
        progress: match run.state {
            RunState::Working => "executing",
            RunState::Blocked => "blocked",
            RunState::Completed => "completed",
            RunState::Failed => "failed",
            RunState::Cancelled => "cancelled",
            RunState::TimedOut => "timed_out",
            RunState::Interrupted => "interrupted",
        }
        .into(),
        task_refs: run.task_refs,
        attempt_refs: run.attempt_refs,
        report,
    })
}

fn valid_refs(refs: &[Uuid]) -> bool {
    !refs.iter().any(Uuid::is_nil)
        && refs.iter().collect::<std::collections::HashSet<_>>().len() == refs.len()
}
