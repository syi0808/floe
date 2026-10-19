//! Canonical startup and result projections share current source authorization.
//! Neither Voice nor the UI can substitute text or waive source review.
use crate::{
    LivePrefixProjectionRequest, LivePrefixReader, LiveProjectionOutcome,
    LiveResultProjectionRequest, LiveResultReader,
};
use floe_access::{DependencyAuthorization, DependencyResolver};
use floe_context_contract::ContextDependency;
use floe_execution::ExecutionScope;
use floe_kernel::AgentFailure;
use std::sync::Arc;

pub struct LiveContextProjector {
    prefix_reader: Arc<dyn LivePrefixReader>,
    result_reader: Arc<dyn LiveResultReader>,
    resolver: Arc<dyn DependencyResolver>,
}
impl LiveContextProjector {
    pub fn new(
        prefix_reader: Arc<dyn LivePrefixReader>,
        result_reader: Arc<dyn LiveResultReader>,
        resolver: Arc<dyn DependencyResolver>,
    ) -> Self {
        Self {
            prefix_reader,
            result_reader,
            resolver,
        }
    }
    pub async fn project_prefix(
        &self,
        request: LivePrefixProjectionRequest<'_>,
        scope: &ExecutionScope,
    ) -> Result<LiveProjectionOutcome, AgentFailure> {
        crate::prepare_live_prefix_projection(
            self.prefix_reader.as_ref(),
            request,
            scope,
            |dependency| self.authorize(dependency, scope),
        )
        .await
    }
    pub async fn project_result(
        &self,
        request: LiveResultProjectionRequest<'_>,
        scope: &ExecutionScope,
    ) -> Result<LiveProjectionOutcome, AgentFailure> {
        crate::prepare_live_result_projection(
            self.result_reader.as_ref(),
            request,
            scope,
            |dependency| self.authorize(dependency, scope),
        )
        .await
    }
    async fn authorize(
        &self,
        dependency: ContextDependency,
        scope: &ExecutionScope,
    ) -> Result<bool, AgentFailure> {
        let authorization = DependencyAuthorization {
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
        };
        match self.resolver.authorize(&dependency, &authorization).await {
            Ok(()) => Ok(true),
            Err(
                AgentFailure::PolicyDenied
                | AgentFailure::StaleContext
                | AgentFailure::CapabilityDenied,
            ) => Ok(false),
            Err(error) => Err(error),
        }
    }
}
