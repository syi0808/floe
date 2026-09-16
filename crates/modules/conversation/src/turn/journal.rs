//! The capability executions a turn journals while the model is running.
//!
//! Model attempts are journaled by the Inference ledger; this channel carries
//! only the records Conversation itself owns.

use tokio::sync::oneshot;

use floe_agent_contract::AgentFailure;

#[derive(Debug)]
pub(crate) struct CapabilityUpdate {
    pub record: Box<crate::turn::CapabilityExecution>,
    pub acknowledged: oneshot::Sender<Result<(), AgentFailure>>,
}

/// Where a capability execution is recorded while the turn is in flight.
#[derive(Clone, Debug, Default)]
pub(crate) struct CapabilityJournal {
    sender: Option<tokio::sync::mpsc::UnboundedSender<CapabilityUpdate>>,
}

impl CapabilityJournal {
    pub(crate) fn new(sender: tokio::sync::mpsc::UnboundedSender<CapabilityUpdate>) -> Self {
        Self {
            sender: Some(sender),
        }
    }

    pub(crate) async fn record(
        &self,
        record: crate::turn::CapabilityExecution,
    ) -> Result<(), AgentFailure> {
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
    }
}
