//! What a dependency re-authorization is allowed to know.
//!
//! Re-admitting a recorded dependency needs the placements the current run may
//! use and the window it must finish in — not the Session, the transcript or the
//! ledger the run happens to carry.

use std::future::Future;

use floe_context_contract::{ContextDependency, ModelPlacement};
use floe_execution::Cancellation;
use floe_kernel::AgentFailure;
use tokio::time::Instant;

#[derive(Clone)]
pub struct DependencyAuthorization {
    pub allowed_placements: Vec<ModelPlacement>,
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
