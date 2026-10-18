//! The Context owner resolves one canonical published result and its current
//! source-processing projection. Voice schedules presentation; it cannot supply
//! substitute text or waive this source check.
use crate::{LiveProjectionOutcome, LiveResultProjectionRequest, LiveResultReader};
use floe_access::{DependencyAuthorization, DependencyResolver};
use floe_execution::ExecutionScope;
use floe_kernel::AgentFailure;
use std::sync::Arc;

pub struct LiveResultProjector {
    reader: Arc<dyn LiveResultReader>,
    resolver: Arc<dyn DependencyResolver>,
}
impl LiveResultProjector {
    pub fn new(reader: Arc<dyn LiveResultReader>, resolver: Arc<dyn DependencyResolver>) -> Self {
        Self { reader, resolver }
    }
    pub async fn project(
        &self,
        request: LiveResultProjectionRequest<'_>,
        scope: &ExecutionScope,
    ) -> Result<LiveProjectionOutcome, AgentFailure> {
        crate::prepare_live_result_projection(self.reader.as_ref(), request, scope, |dependency| {
            let resolver = self.resolver.clone();
            let authorization = DependencyAuthorization {
                deadline: scope.deadline(),
                cancellation: scope.cancellation().clone(),
            };
            async move {
                match resolver.authorize(&dependency, &authorization).await {
                    Ok(()) => Ok(true),
                    Err(
                        AgentFailure::PolicyDenied
                        | AgentFailure::StaleContext
                        | AgentFailure::CapabilityDenied,
                    ) => Ok(false),
                    Err(error) => Err(error),
                }
            }
        })
        .await
    }
}
