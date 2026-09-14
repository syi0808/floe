use tokio::sync::oneshot;

use crate::AgentFailure;

pub use floe_inference::{ModelAttemptRecord, ModelAttemptState};

#[derive(Debug)]
pub(crate) enum JournalRecord {
    Model(ModelAttemptRecord),
    Capability(Box<crate::CapabilityExecution>),
}

#[derive(Debug)]
pub(crate) struct JournalUpdate {
    pub record: JournalRecord,
    pub acknowledged: oneshot::Sender<Result<(), AgentFailure>>,
}
