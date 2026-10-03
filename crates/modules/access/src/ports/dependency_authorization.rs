//! What a dependency re-authorization is allowed to know.
//!
//! Re-admitting a recorded dependency needs only the window it must finish in —
//! not the Session, the transcript, the ledger the run happens to carry, or the
//! model plan it may later be shown to. Source/grant/dependency
//! authority is independent of planning; Device and Gateway processing restrictions are
//! enforced by Access model dispatch, not by reauthorization.

use std::future::Future;

use floe_context_contract::ContextDependency;
use floe_execution::Cancellation;
use floe_kernel::AgentFailure;
use tokio::time::Instant;

#[derive(Clone)]
pub struct DependencyAuthorization {
    pub deadline: Instant,
    pub cancellation: Cancellation,
}

/// Re-admit one recorded dependency under the current run's constraints.
pub trait DependencyResolver: Send + Sync {
    fn authorize<'a>(
        &'a self,
        dependency: &'a ContextDependency,
        request: &'a DependencyAuthorization,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>>;
}

/// Whether a dependency's observation is still live, without any I/O.
pub trait DependencyLiveness: Send + Sync {
    fn validate(&self, dependency: &ContextDependency) -> Result<(), AgentFailure>;
}

impl<T: DependencyResolver + ?Sized> DependencyResolver for std::sync::Arc<T> {
    fn authorize<'a>(
        &'a self,
        dependency: &'a ContextDependency,
        request: &'a DependencyAuthorization,
    ) -> floe_execution::BoxFuture<'a, Result<(), AgentFailure>> {
        (**self).authorize(dependency, request)
    }
}
impl<T: DependencyLiveness + ?Sized> DependencyLiveness for std::sync::Arc<T> {
    fn validate(&self, dependency: &ContextDependency) -> Result<(), AgentFailure> {
        (**self).validate(dependency)
    }
}
