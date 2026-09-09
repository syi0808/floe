use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::{AgentFailure, ModelPlacement, ModelUsage};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelAttemptState {
    Started,
    Accepted,
    Rejected,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAttemptRecord {
    pub id: Uuid,
    pub turn_id: Uuid,
    pub scope_id: Uuid,
    pub attempt: u8,
    pub placement: ModelPlacement,
    pub state: ModelAttemptState,
    pub failure: Option<AgentFailure>,
    pub usage: ModelUsage,
}

#[derive(Debug)]
pub(crate) enum JournalRecord {
    Model(ModelAttemptRecord),
    Capability(crate::CapabilityExecution),
}

#[derive(Debug)]
pub(crate) struct JournalUpdate {
    pub record: JournalRecord,
    pub acknowledged: oneshot::Sender<Result<(), AgentFailure>>,
}
