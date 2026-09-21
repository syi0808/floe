//! Canonical root model provider: device Foundation is always observed;
//! server profiles come only from the saved connection admitted against the
//! verified caller. The turn's pre-resolved route is never consulted here.

use floe_agent_contract::AgentFailure;

use super::foundation::{FoundationModelProvider, PreparedFoundationTransport};
use super::server::{PreparedServerTransport, ServerModelProvider};

/// Device Foundation plus, when this caller saved one, the admitted server
/// connection. Secrets never leave the prepared transports.
pub struct RootModelProvider {
    foundation: FoundationModelProvider,
    server: Option<ServerModelProvider>,
}

impl RootModelProvider {
    /// Build from verified caller identity and the product-supplied saved
    /// server connection, if any. A saved connection bound to another
    /// person/device fails closed instead of downgrading to device-only.
    pub fn for_saved_connection(
        saved: Option<floe_inference::SavedServerConnection>,
        person_id: &str,
        device_id: &str,
    ) -> Result<Self, AgentFailure> {
        let server = saved
            .map(|stored| {
                let admitted =
                    floe_inference::admit_saved_connection(stored, person_id, device_id)?;
                ServerModelProvider::for_connection(&admitted)
            })
            .transpose()?;
        Ok(Self {
            foundation: FoundationModelProvider::encrypted(),
            server,
        })
    }
}

/// Opaque prepared capability: the profile facts are public, the transport
/// keeps its credentials.
pub enum PreparedRootTransport {
    Foundation(PreparedFoundationTransport),
    Server(PreparedServerTransport),
}

impl floe_inference::PreparedModelTransport for PreparedRootTransport {
    async fn generate(
        &self,
        request: floe_inference::CanonicalModelRequest,
    ) -> Result<floe_inference::CanonicalModelResponse, AgentFailure> {
        match self {
            Self::Foundation(transport) => transport.generate(request).await,
            Self::Server(transport) => transport.generate(request).await,
        }
    }
}

impl floe_inference::ModelProvider for RootModelProvider {
    type Prepared = PreparedRootTransport;

    async fn observe_profiles(
        &self,
    ) -> Vec<floe_inference::PreparedModelProfile<Self::Prepared>> {
        let mut observed: Vec<floe_inference::PreparedModelProfile<PreparedRootTransport>> = self
            .foundation
            .observe_profiles()
            .await
            .into_iter()
            .map(|prepared| floe_inference::PreparedModelProfile {
                profile: prepared.profile,
                transport: PreparedRootTransport::Foundation(prepared.transport),
            })
            .collect();
        if let Some(server) = &self.server {
            observed.extend(
                server
                    .observe_profiles()
                    .await
                    .into_iter()
                    .map(|prepared| floe_inference::PreparedModelProfile {
                        profile: prepared.profile,
                        transport: PreparedRootTransport::Server(prepared.transport),
                    }),
            );
        }
        observed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERSON: &str = "00000000-0000-4000-8000-000000000001";
    const DEVICE: &str = "local-device";

    fn saved() -> floe_inference::SavedServerConnection {
        floe_inference::SavedServerConnection {
            base_url: "http://127.0.0.1:8431".into(),
            token: "t".repeat(32),
            client_id: "paired-client".into(),
            person_id: PERSON.into(),
            device_id: DEVICE.into(),
            allow_external: true,
            external_recipients: vec!["partner.example".into()],
        }
    }

    #[test]
    fn saved_connection_is_bound_to_verified_caller_identity() {
        // No saved connection: device-only.
        let device_only = RootModelProvider::for_saved_connection(None, PERSON, DEVICE).unwrap();
        assert!(device_only.server.is_none());

        // Foreign identity fails closed instead of downgrading silently.
        assert!(RootModelProvider::for_saved_connection(
            Some(saved()),
            PERSON,
            "other-device",
        )
        .is_err());
        assert!(RootModelProvider::for_saved_connection(
            Some(saved()),
            "00000000-0000-4000-8000-000000000002",
            DEVICE,
        )
        .is_err());

        // Matching identity admits the server.
        let admitted =
            RootModelProvider::for_saved_connection(Some(saved()), PERSON, DEVICE).unwrap();
        assert!(admitted.server.is_some());
    }

    #[test]
    fn device_only_connection_still_admits_server() {
        let mut local = saved();
        local.allow_external = false;
        local.external_recipients = vec![];
        let admitted =
            RootModelProvider::for_saved_connection(Some(local), PERSON, DEVICE).unwrap();
        assert!(admitted.server.is_some());
    }
}
