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
    /// Load and admit the current saved connection for the verified caller.
    /// A saved connection bound to another
    /// person/device fails closed instead of downgrading to device-only.
    pub fn from_current_connection(
        store: &impl floe_inference::SavedConnectionStore,
        person_id: &str,
        device_id: &str,
    ) -> Result<Self, AgentFailure> {
        Self::from_current_connection_scoped(
            store,
            person_id,
            device_id,
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            floe_inference::CANONICAL_MODEL_CONSUMER,
        )
    }

    /// Build under a domain purpose/consumer: device Foundation plus, when
    /// this caller saved one, the admitted server connection. Both legs
    /// observe the same domain scope.
    pub fn from_current_connection_scoped(
        store: &impl floe_inference::SavedConnectionStore,
        person_id: &str,
        device_id: &str,
        purpose: &str,
        consumer: &str,
    ) -> Result<Self, AgentFailure> {
        let server = store
            .load()?
            .map(|stored| {
                let admitted =
                    floe_inference::admit_saved_connection(stored, person_id, device_id)?;
                ServerModelProvider::for_connection_scoped(&admitted, purpose, consumer)
            })
            .transpose()?;
        Ok(Self {
            foundation: FoundationModelProvider::scoped(
                floe_agent_contract::SessionProtection::Encrypted,
                purpose,
                consumer,
            )?,
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
        target: floe_inference::AdmittedDispatchTarget,
    ) -> Result<floe_inference::CanonicalModelResponse, AgentFailure> {
        match self {
            Self::Foundation(transport) => transport.generate(request, target).await,
            Self::Server(transport) => transport.generate(request, target).await,
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
        }
    }

    #[test]
    fn saved_connection_is_bound_to_verified_caller_identity() {
        // No saved connection: device-only.
        let device_only = RootModelProvider::from_current_connection(
            &crate::control::CurrentSavedConnectionStore::fixed(None),
            PERSON,
            DEVICE,
        )
        .unwrap();
        assert!(device_only.server.is_none());

        // Foreign identity fails closed instead of downgrading silently.
        assert!(
            RootModelProvider::from_current_connection(
                &crate::control::CurrentSavedConnectionStore::fixed(Some(saved())),
                PERSON,
                "other-device",
            )
            .is_err()
        );
        assert!(
            RootModelProvider::from_current_connection(
                &crate::control::CurrentSavedConnectionStore::fixed(Some(saved())),
                "00000000-0000-4000-8000-000000000002",
                DEVICE,
            )
            .is_err()
        );

        // Matching identity admits the server.
        let admitted = RootModelProvider::from_current_connection(
            &crate::control::CurrentSavedConnectionStore::fixed(Some(saved())),
            PERSON,
            DEVICE,
        )
        .unwrap();
        assert!(admitted.server.is_some());
    }

    #[test]
    fn paired_connection_admits_server_without_recipient_approval() {
        let local = saved();
        let admitted = RootModelProvider::from_current_connection(
            &crate::control::CurrentSavedConnectionStore::fixed(Some(local)),
            PERSON,
            DEVICE,
        )
        .unwrap();
        assert!(admitted.server.is_some());
    }

    #[tokio::test]
    async fn scoped_providers_observe_the_domain_scope() {
        use floe_inference::ModelProvider;
        // Device leg: the same Foundation model observes the domain pair.
        let device = RootModelProvider::from_current_connection_scoped(
            &crate::control::CurrentSavedConnectionStore::fixed(None),
            PERSON,
            DEVICE,
            "governed-memory-review",
            "knowledge.learner",
        )
        .unwrap();
        let observed = device.observe_profiles().await;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].profile.purpose.as_str(), "governed-memory-review");
        assert_eq!(observed[0].profile.consumer.as_str(), "knowledge.learner");
        assert_eq!(
            observed[0].profile.execution_location,
            floe_inference::ExecutionLocation::Device
        );

        // Root scope is unchanged: the canonical root pair.
        let root = RootModelProvider::from_current_connection(
            &crate::control::CurrentSavedConnectionStore::fixed(None),
            PERSON,
            DEVICE,
        )
        .unwrap();
        let observed = root.observe_profiles().await;
        assert_eq!(observed.len(), 1);
        assert_eq!(
            observed[0].profile.purpose.as_str(),
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE
        );
        assert_eq!(
            observed[0].profile.consumer.as_str(),
            floe_inference::CANONICAL_MODEL_CONSUMER
        );

        // Empty scope fails closed at composition, never as an unscoped profile.
        assert!(
            RootModelProvider::from_current_connection_scoped(
                &crate::control::CurrentSavedConnectionStore::fixed(None),
                PERSON,
                DEVICE,
                "",
                "c",
            )
            .is_err()
        );
        assert!(
            RootModelProvider::from_current_connection_scoped(
                &crate::control::CurrentSavedConnectionStore::fixed(None),
                PERSON,
                DEVICE,
                "p",
                "",
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn remote_source_capability_does_not_require_an_available_model() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut connection = saved();
        connection.base_url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let store = crate::control::CurrentSavedConnectionStore::fixed(Some(connection));
        let source =
            crate::sources::ServerSourceClient::from_current_connection(&store, PERSON, DEVICE)
                .unwrap();
        assert!(source.is_some());
        let provider = RootModelProvider::from_current_connection_scoped(
            &store,
            PERSON,
            DEVICE,
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            floe_agent_contract::DELEGATED_EXPERT_INFERENCE_CONSUMER,
        )
        .unwrap();
        let availability = floe_inference::InferenceAvailability::observe(
            &provider,
            floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            floe_agent_contract::DELEGATED_EXPERT_INFERENCE_CONSUMER,
        )
        .await;
        assert!(
            !availability.can_execute(floe_inference::InferenceExecutionConstraint::RemoteOnly)
        );
        assert!(
            crate::sources::ServerSourceClient::from_current_connection(&store, PERSON, DEVICE)
                .unwrap()
                .is_some()
        );
    }
}
