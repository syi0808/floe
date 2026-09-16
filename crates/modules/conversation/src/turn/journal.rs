use tokio::sync::oneshot;

use floe_agent_contract::AgentFailure;

pub use floe_inference::{ModelAttemptRecord, ModelAttemptState};

#[derive(Debug)]
pub(crate) enum JournalRecord {
    Model(ModelAttemptRecord),
    Capability(Box<crate::turn::CapabilityExecution>),
}

#[derive(Debug)]
pub(crate) struct JournalUpdate {
    pub record: JournalRecord,
    pub acknowledged: oneshot::Sender<Result<(), AgentFailure>>,
}
