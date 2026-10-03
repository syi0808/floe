//! Prepared source transport. Credentials remain adapter-private and every use
//! is tied to the same verified Gateway binding as model transport.
use crate::gateway::credentials::{GatewayConnection, GatewayCredentialStore};
use floe_agent_contract::AgentFailure;

#[derive(Clone)]
pub struct PreparedServerSource {
    pub(crate) connection: GatewayConnection,
    store: GatewayCredentialStore,
}
impl std::fmt::Debug for PreparedServerSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedServerSource")
            .field("binding", &self.connection.binding)
            .finish_non_exhaustive()
    }
}
impl PreparedServerSource {
    pub(crate) fn new(connection: GatewayConnection, store: GatewayCredentialStore) -> Self {
        Self { connection, store }
    }
    pub fn binding(&self) -> &floe_access::VerifiedGatewayBinding {
        &self.connection.binding
    }
    pub fn client_id(&self) -> &str {
        &self.connection.binding.client_id
    }
    pub fn person_id(&self) -> &str {
        &self.connection.binding.person_id
    }
    pub fn device_id(&self) -> &str {
        &self.connection.binding.device_id
    }
    pub(crate) fn base_url(&self) -> &str {
        &self.connection.endpoint
    }
    pub(crate) fn bearer_token(&self) -> &str {
        self.connection.bearer.as_str()
    }
    pub(crate) async fn revalidate(&self) -> Result<(), AgentFailure> {
        let current = self
            .store
            .load(self.person_id(), self.device_id())
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .ok_or(AgentFailure::PolicyDenied)?;
        if current.binding != self.connection.binding
            || current.endpoint != self.connection.endpoint
            || current.bearer.as_str() != self.connection.bearer.as_str()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
}
