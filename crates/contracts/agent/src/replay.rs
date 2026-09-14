use crate::{Artifact, DependencyCoverage, RunId, TaskId, TaskState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct AttemptId(pub Uuid);

impl AttemptId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AttemptId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct InvocationKey(pub Uuid);

impl InvocationKey {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn from_uuid(value: Uuid) -> Option<Self> {
        (!value.is_nil()).then_some(Self(value))
    }
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for InvocationKey {
    fn default() -> Self {
        Self::new()
    }
}

pub fn input_digest(input: &str) -> [u8; 32] {
    Sha256::digest(input.as_bytes()).into()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayReceipt {
    pub principal: String,
    pub run_id: Option<RunId>,
    pub task_id: Option<TaskId>,
    pub agent_id: Option<String>,
    pub tool_id: Option<String>,
    pub definition_revision: u64,
    pub input_digest: [u8; 32],
    pub invocation_key: InvocationKey,
    pub call_id: Uuid,
    pub result: String,
    pub task_result: Option<String>,
    pub task_state: Option<TaskState>,
    pub task_artifacts: Vec<Artifact>,
    pub task_issue: Option<crate::AgentFailure>,
    pub tool_artifacts: Vec<Artifact>,
    pub tool_coverage: DependencyCoverage,
    pub tool_issue: Option<crate::AgentFailure>,
}
