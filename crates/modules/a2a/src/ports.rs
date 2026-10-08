use floe_agent_contract::{AgentFailure, BoxFuture, TaskReceipt};
use floe_conversation_contract::MessageAdmissionRequest;
use floe_execution::ExecutionScope;
use floe_kernel::TaskId;

use crate::{A2aArtifact, A2aEnvelope, A2aFailure, A2aPeerId, A2aPeerTaskId, A2aTaskObservation};

/// Host Task lifecycle remains owned by the host module. This port returns the
/// existing immutable receipt and creates no A2A-owned Task aggregate.
pub struct HostedTaskAdmission {
    pub message: MessageAdmissionRequest,
    pub artifacts: Vec<A2aArtifact>,
}

pub trait HostedTaskPort: Send + Sync {
    fn admit<'a>(
        &'a self,
        request: HostedTaskAdmission,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>>;

    fn read<'a>(
        &'a self,
        task_id: TaskId,
    ) -> BoxFuture<'a, Result<Option<floe_agent_contract::TaskSnapshot>, AgentFailure>>;
}

/// External or in-process bindings implement this transport-neutral contract.
/// An in-process binding calls the same peer operation directly; it must not
/// add loopback HTTP. Remote observations remain distinct from local receipts.
pub trait A2aPeerExchangePort: Send + Sync {
    fn send_message<'a>(
        &'a self,
        peer_id: A2aPeerId,
        envelope: A2aEnvelope,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<A2aTaskObservation, A2aFailure>>;

    fn observe_task<'a>(
        &'a self,
        peer_id: A2aPeerId,
        task_id: A2aPeerTaskId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<A2aTaskObservation, A2aFailure>>;
}
