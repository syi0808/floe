//! The capability executions a turn journals while the model is running.
//!
//! Model attempts are journaled by the Inference ledger; this channel carries
//! only the records Conversation itself owns.

use tokio::sync::oneshot;

use floe_agent_contract::{AgentFailure, BoxFuture, CapabilityExecution, CapabilityJournal};

#[derive(Debug)]
pub(crate) struct CapabilityUpdate {
    pub record: Box<CapabilityExecution>,
    pub acknowledged: oneshot::Sender<Result<(), AgentFailure>>,
}

/// Where a capability execution is recorded while the turn is in flight.
#[derive(Clone, Debug, Default)]
pub(crate) struct CapabilityJournalSender {
    sender: Option<tokio::sync::mpsc::UnboundedSender<CapabilityUpdate>>,
}

impl CapabilityJournalSender {
    pub(crate) fn new(sender: tokio::sync::mpsc::UnboundedSender<CapabilityUpdate>) -> Self {
        Self {
            sender: Some(sender),
        }
    }
}

impl CapabilityJournal for CapabilityJournalSender {
    fn record<'a>(
        &'a self,
        record: CapabilityExecution,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            let Some(sender) = &self.sender else {
                return Ok(());
            };
            let (acknowledged, receiver) = oneshot::channel();
            sender
                .send(CapabilityUpdate {
                    record: Box::new(record),
                    acknowledged,
                })
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            receiver
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?
        })
    }
}
