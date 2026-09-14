use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeCause {
    Panic,
    Internal,
    InvalidInput,
    Storage,
    Unavailable,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SafeStage {
    FfiBoundary,
    AgentRun,
    AgentJob,
    ModelAttempt,
    ExpertInvocation,
}
