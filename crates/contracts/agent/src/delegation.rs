use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AgentFailure, Artifact, DependencyCoverage, TaskId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Submitted,
    Working,
    Completed,
    Failed,
    Rejected,
    Cancelled,
    TimedOut,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSnapshot {
    pub task_id: TaskId,
    pub parent_run_id: Option<Uuid>,
    pub principal: String,
    pub agent_id: String,
    pub definition_revision: u64,
    pub state: TaskState,
    pub result: Option<String>,
    pub artifacts: Vec<Artifact>,
    pub coverage: DependencyCoverage,
    pub issue: Option<AgentFailure>,
}

impl TaskSnapshot {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        if !self.task_id.is_valid()
            || self.principal.trim().is_empty()
            || self.agent_id.trim().is_empty()
            || self.definition_revision == 0
            || self
                .result
                .as_deref()
                .is_some_and(|result| result.len() > maximum_bytes)
            || self
                .artifacts
                .iter()
                .any(|artifact| artifact.validate(maximum_bytes).is_err())
            || self.coverage.validate().is_err()
            || serde_json::to_vec(self)
                .map(|encoded| encoded.len() > maximum_bytes)
                .unwrap_or(true)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationRequest {
    pub task_id: TaskId,
    pub parent_run_id: Option<Uuid>,
    pub principal: String,
    pub invocation_key: crate::InvocationKey,
    pub selected_agent_id: String,
    pub selected_definition_revision: u64,
    pub message: String,
    pub context_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReceipt {
    pub task_id: TaskId,
    pub snapshot: TaskSnapshot,
    pub replay: Option<crate::ReplayReceipt>,
}
