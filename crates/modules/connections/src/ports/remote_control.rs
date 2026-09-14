use floe_execution::Cancellation;
use floe_kernel::AgentFailure;
use tokio::time::Instant;

use crate::{PairingConfirmation, PairingConfirmationRequest, PairingStatus, PairingStatusRequest};

#[allow(async_fn_in_trait)]
pub trait RemoteControl: Send + Sync {
    async fn confirm(
        &self,
        request: PairingConfirmationRequest,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<PairingConfirmation, AgentFailure>;

    async fn status(
        &self,
        request: PairingStatusRequest,
        deadline: Instant,
        cancellation: &Cancellation,
    ) -> Result<PairingStatus, AgentFailure>;
}
