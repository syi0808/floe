//! Settlement of one exact product command. Contention is not a semantic rejection.
use crate::{
    ConnectionsCommandFailure, ConnectionsCommandIdentity, ConnectionsCommandResolution,
    ConnectionsProductRepository,
};
use floe_kernel::AgentFailure;
pub(super) async fn settle_product_command<T>(
    products: &dyn ConnectionsProductRepository,
    result: Result<T, AgentFailure>,
    classify: fn(AgentFailure) -> ConnectionsCommandFailure,
    identity: Option<ConnectionsCommandIdentity>,
) -> Result<T, ConnectionsCommandFailure> {
    match result {
        Ok(value) => Ok(value),
        Err(reason) => {
            let failure = classify(reason);
            if matches!(
                failure,
                ConnectionsCommandFailure::Admitted(_) | ConnectionsCommandFailure::NotApplied(_)
            ) {
                return Err(failure);
            }
            // No inference about earlier attempts is possible from a busy read.
            // Keep the exact request; a later retry must read its real outcome.
            if reason == AgentFailure::StorageBusy {
                return Err(ConnectionsCommandFailure::Indeterminate(reason));
            }
            let Some(identity) = identity else {
                return Err(failure);
            };
            match products.reject_unadmitted_command(identity, reason).await {
                Ok(ConnectionsCommandResolution::NotApplied(reason)) => {
                    Err(ConnectionsCommandFailure::NotApplied(reason))
                }
                Ok(ConnectionsCommandResolution::Admitted) => {
                    Err(ConnectionsCommandFailure::Admitted(reason))
                }
                Err(reason) => Err(ConnectionsCommandFailure::Indeterminate(reason)),
            }
        }
    }
}
