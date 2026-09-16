//! Host-side route selection: read the saved credential, admit it against the
//! verified caller, then ask the provider adapter to resolve the route.

use crate::CallerContext;

#[derive(Clone, Copy, Default)]
pub struct HostInferenceRoutes;

impl HostInferenceRoutes {
    pub async fn resolve(
        &self,
        caller: &CallerContext,
    ) -> Result<Option<floe_protocol::AgentRemoteRouteDto>, floe_agent_contract::AgentFailure> {
        let Some(saved) = floe_provider_adapters::control::load_saved_connection()? else {
            return Ok(None);
        };
        let connection = floe_inference::admit_saved_connection(
            saved,
            &caller.person_id().to_string(),
            caller.device_id(),
        )?;
        floe_provider_adapters::models::resolve_remote_model_route(&connection)
            .await
            .map(Some)
    }
}
