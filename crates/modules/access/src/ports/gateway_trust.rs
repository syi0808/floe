use crate::RemoteProducerIdentity;
use floe_execution::BoxFuture;
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Exact nonsecret durable credential expectation. Connections owns transitions;
/// Access reads it to distinguish proven absence from lost/restored credentials.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum GatewayCredentialExpectation {
    Unpaired,
    Pending { operation_id: Uuid },
    Committed { operation_id: Uuid, generation: u64 },
    Forgotten { operation_id: Uuid, generation: u64 },
}
pub trait GatewayTrustReader: Send + Sync {
    fn credential_expectation<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<GatewayCredentialExpectation, AgentFailure>>;
    fn pinned_producer<'a>(&'a self)
    -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>>;
}
