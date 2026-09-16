//! Injection site for remote route selection.
//!
//! The judgment — whether a saved connection exists, whether it may be bound to
//! this caller, and what the paired server offers — belongs to Inference and the
//! provider adapters. This type only supplies the verified caller identity and
//! the concrete ports.

use crate::CallerContext;

#[derive(Clone, Copy, Default)]
pub struct HostInferenceRoutes;

impl HostInferenceRoutes {
    pub async fn resolve(
        &self,
        caller: &CallerContext,
    ) -> Result<Option<floe_protocol::AgentRemoteRouteDto>, floe_agent_contract::AgentFailure> {
        floe_inference::select_remote_route(
            &floe_provider_adapters::control::SavedServerConnectionStore,
            &floe_provider_adapters::models::RemoteModelRouteResolver,
            &caller.person_id().to_string(),
            caller.device_id(),
        )
        .await
    }
}
