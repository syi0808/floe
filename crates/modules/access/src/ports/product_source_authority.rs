use floe_context_contract::GrantSourceBinding;
use floe_execution::{BoxFuture, ExecutionScope};
use floe_kernel::{AgentFailure, OwnerActor};
use crate::ProductSourceObservation;

pub trait ProductSourceAuthority: Send + Sync {
    fn observe_current<'a>(&'a self, actor: &'a OwnerActor, source: &'a GrantSourceBinding, scope: &'a ExecutionScope) -> BoxFuture<'a, Result<ProductSourceObservation, AgentFailure>>;
}
