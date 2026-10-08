use floe_agent_contract::{AgentFailure, BoxFuture, TaskReceipt};
use floe_conversation_contract::{AgentInstanceId, MessageAdmissionRequest};
use floe_execution::ExecutionScope;
use floe_kernel::TaskId;

use crate::{A2aArtifact, A2aEnvelope, A2aFailure, A2aPeerId, A2aPeerTaskId, A2aTaskObservation};

/// Host Task lifecycle remains owned by the host module. This port returns the
/// existing immutable receipt and creates no A2A-owned Task aggregate.
pub struct HostedTaskAdmission {
    pub message: MessageAdmissionRequest,
    pub artifacts: Vec<A2aArtifact>,
}

impl HostedTaskAdmission {
    /// Confirm the artifacts still match the semantic evidence reference after
    /// crossing the host Task port boundary.
    pub fn validate(&self) -> Result<(), A2aFailure> {
        crate::mapping::validate_artifact_commitment(&self.message, &self.artifacts)
    }
}

/// Implementations validate the admitted scope and target agent against host
/// authority; identity/origin fields do not grant access. They call
/// `HostedTaskAdmission::validate` and persist the supplied artifacts under the
/// message's evidence commitment before returning the existing Task receipt.
/// Reads are scoped to the supplied host agent and execution scope as well.
pub trait HostedTaskPort: Send + Sync {
    fn admit<'a>(
        &'a self,
        request: HostedTaskAdmission,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>>;

    fn read<'a>(
        &'a self,
        task_id: TaskId,
        host_agent_instance_id: AgentInstanceId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<floe_agent_contract::TaskSnapshot>, AgentFailure>>;
}

/// External or in-process bindings implement this transport-neutral contract.
/// An in-process binding calls the same peer operation directly; it must not
/// add loopback HTTP. Remote observations remain distinct from local receipts.
/// Every call is bounded by the supplied scope's cancellation and deadline.
/// The binding must authorize that scope for the supplied host agent, validate
/// the exact peer/task binding, and treat IDs as selectors rather than grants.
pub trait A2aPeerExchangePort: Send + Sync {
    fn send_message<'a>(
        &'a self,
        peer_id: A2aPeerId,
        host_agent_instance_id: AgentInstanceId,
        envelope: A2aEnvelope,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<A2aTaskObservation, A2aFailure>>;

    fn observe_task<'a>(
        &'a self,
        peer_id: A2aPeerId,
        host_agent_instance_id: AgentInstanceId,
        task_id: A2aPeerTaskId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<A2aTaskObservation, A2aFailure>>;

    /// Request remote Task cancellation within `scope` and return the resulting
    /// peer observation. The observation does not prove effect rollback or
    /// local Task settlement. The binding must verify the host agent's admitted
    /// scope and the peer-scoped Task binding before sending the request.
    fn request_task_cancellation<'a>(
        &'a self,
        peer_id: A2aPeerId,
        host_agent_instance_id: AgentInstanceId,
        task_id: A2aPeerTaskId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<A2aTaskObservation, A2aFailure>>;
}
