use std::{sync::OnceLock, time::Duration};

use crate::control::PreparedServerSource;
use crate::gateway::views::{
    GatewayViewsClient, RemoteViewAuthorizationRequest, parse_remote_view_challenge,
};
use floe_access::{AuthorizationSigner, RemoteViewAuthorizationExpectation};
use floe_agent_contract::AgentFailure;
use floe_connections::{CalendarConnectionRef, ConnectorCatalogObservation, ConnectorSnapshot};
use floe_execution::limits::{CallLimiter, CallLimits};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};

/// One authorized view read, as the grant it runs under states it.
///
/// Turning a grant into the admission the paired server checks — which
/// authority, which epochs, which bounds — is this transport's shape, not its
/// caller's.
pub struct AuthorizedViewRead<'a> {
    pub view_id: &'a str,
    pub grant: &'a floe_access::DataAccessGrant,
    pub source_authority: floe_context_contract::SourceAuthority,
    pub consumer: &'a str,
    pub resource: &'a str,
    pub connection_revision: u64,
    pub max_items: usize,
    pub max_bytes: usize,
    pub query: serde_json::Value,
    pub client_id: &'a str,
    pub device_id: &'a str,
}

/// The paired server's source transport: reads and lazy catalog observation.
///
/// The client owns no model state. What the server offers as sources is
/// observed lazily, only when a caller actually needs source enumeration, and
/// projected by Connections; model profile discovery never shares this path.
pub struct ServerSourceClient {
    source: PreparedServerSource,
    source_calls: CallLimiter,
}

fn source_calls() -> &'static CallLimiter {
    static LIMIT: OnceLock<CallLimiter> = OnceLock::new();
    LIMIT.get_or_init(provider_call_limit)
}

fn provider_call_limit() -> CallLimiter {
    CallLimiter::new(CallLimits {
        max_running: 4,
        max_pending: 8,
        max_context_bytes: 65_536,
        max_total_context_bytes: 12 * 65_536,
    })
    .expect("valid static provider limits")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A connector catalog exactly as the paired server reported it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedConnectorCatalog {
    schema_version: u32,
    person_id: String,
    device_id: String,
    connectors: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedConnectionSnapshots {
    schema_version: u32,
    person_id: String,
    device_id: String,
    connections: Vec<ConnectorSnapshot>,
}

/// Largest connector catalog the transport will read.
const MAX_CATALOG_BYTES: usize = 65_536;

impl ServerSourceClient {
    pub fn new(source: PreparedServerSource) -> Self {
        Self {
            source,
            source_calls: source_calls().clone(),
        }
    }

    /// Prepare the source client for one verified caller from its saved
    /// server connection, if it has one. Pure: no network, no model profile
    /// discovery. Absence means local-only; a foreign or malformed stored
    /// connection fails closed.
    pub async fn from_current_connection(
        store: &crate::gateway::GatewayCredentialStore,
        person_id: &str,
        device_id: &str,
    ) -> Result<Option<Self>, AgentFailure> {
        Ok(store
            .load(person_id, device_id)
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .map(|connection| Self::new(PreparedServerSource::new(connection, store.clone()))))
    }

    /// The prepared source this client reads through: endpoint, credential
    /// and pairing identity. The credential stays inside the adapter.
    pub fn source(&self) -> &PreparedServerSource {
        &self.source
    }

    pub(crate) fn authorization_client(&self) -> Result<GatewayViewsClient, AgentFailure> {
        Ok(GatewayViewsClient::new(self.source.clone()))
    }

    /// Read one view the Person's grant admits, through their paired server.
    pub async fn read_admitted_view<Keys: AuthorizationSigner + ?Sized>(
        &self,
        keys: &Keys,
        read: AuthorizedViewRead<'_>,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<serde_json::Value, AgentFailure> {
        self.source.revalidate().await?;
        let source = read.grant.source();
        let connector = source.connector();
        let connection = source.connection_id();
        let grant_id = read.grant.id().as_uuid().to_string();
        let grant_incarnation = read.grant.authority().incarnation().to_string();
        let max_items = u32::try_from(read.max_items).map_err(|_| AgentFailure::BudgetExceeded)?;
        let max_bytes = u32::try_from(read.max_bytes).map_err(|_| AgentFailure::BudgetExceeded)?;
        let path = format!("/v1/views/{}/admit", read.view_id);
        let expected = RemoteViewAuthorizationExpectation {
            operation: "".into(),
            client_id: read.client_id.into(),
            device_id: read.device_id.into(),
            challenge_id: String::new(),
            admission_id: String::new(),
            query_sha256: String::new(),
            result_sha256: String::new(),
            grant_id: grant_id.clone(),
            grant_incarnation: grant_incarnation.clone(),
            grant_epoch: read.grant.authority().access_epoch().get(),
            source_connector: connector.as_str().into(),
            source_connection: connection.as_str().into(),
            source_execution_owner: source.execution_owner().as_str().into(),
            source_incarnation: read.source_authority.incarnation().to_string(),
            source_epoch: read.source_authority.epoch().get(),
            resources: vec![read.resource.to_owned()],
            max_items,
            max_bytes,
        };
        let request = RemoteViewAuthorizationRequest {
            path: &path,
            connector_id: connector.as_str(),
            connection_id: connection.as_str(),
            connection_revision: read.connection_revision,
            resource: read.resource,
            grant_id: &grant_id,
            grant_incarnation: &grant_incarnation,
            grant_epoch: read.grant.authority().access_epoch().get(),
            purpose: "everyday_assistance",
            consumer: read.consumer,
            max_items,
            max_bytes,
            query: read.query,
        };
        self.read_authorized_view(keys, request, expected, deadline, cancellation)
            .await
    }

    pub(crate) async fn read_authorized_view<Keys: AuthorizationSigner + ?Sized>(
        &self,
        keys: &Keys,
        request: RemoteViewAuthorizationRequest<'_>,
        mut expected: RemoteViewAuthorizationExpectation,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<serde_json::Value, AgentFailure> {
        if !request.path.ends_with("/admit") {
            return Err(AgentFailure::InvalidInput);
        }
        let _permit = self
            .source_calls
            .acquire(request.query.to_string().len(), deadline, cancellation)
            .await?;
        let consumer = request.consumer;
        let read_path = format!("{}/read", request.path.trim_end_matches("/admit"));
        let release_path = format!("{}/release", request.path.trim_end_matches("/admit"));
        let client = self.authorization_client()?;
        let query_bytes =
            serde_json::to_vec(&request.query).map_err(|_| AgentFailure::InvalidInput)?;
        let query_digest = sha256_hex(&query_bytes);
        let challenge = client
            .begin_view_admission(request, deadline, cancellation)
            .await?;
        let parts = parse_remote_view_challenge(&challenge.challenge_b64url)?;
        if parts.operation != "admission"
            || parts.query_sha256 != query_digest
            || parts.source.connector_id != expected.source_connector
            || parts.source.connection_id != expected.source_connection
            || parts.resources != expected.resources
            || parts.consumer != consumer
            || parts.max_items != expected.max_items
            || parts.max_bytes != expected.max_bytes
            || parts.client_id != self.source.client_id()
            || parts.device_id != self.source.device_id()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        expected.operation = "admission".into();
        expected.challenge_id = parts.challenge_id.clone();
        expected.query_sha256 = query_digest;
        expected.admission_id.clear();
        expected.result_sha256.clear();
        let release = client
            .read_view_admission(
                keys,
                &expected,
                &challenge,
                &read_path,
                deadline,
                cancellation,
            )
            .await?;
        let release_parts = parse_remote_view_challenge(&release.challenge_b64url)?;
        if release_parts.operation != "release"
            || release_parts.admission_id != parts.challenge_id
            || release_parts.query_sha256 != expected.query_sha256
            || release_parts.resources != expected.resources
            || release_parts.max_items != expected.max_items
            || release_parts.max_bytes != expected.max_bytes
            || release_parts.consumer != parts.consumer
            || release_parts.result_sha256.len() != 64
            || !release_parts
                .result_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(AgentFailure::PolicyDenied);
        }
        expected.operation = "release".into();
        expected.challenge_id = release_parts.challenge_id;
        expected.admission_id = parts.challenge_id;
        expected.result_sha256 = release_parts.result_sha256;
        client
            .release_view(
                keys,
                &expected,
                &release,
                &release_path,
                deadline,
                cancellation,
            )
            .await
    }

    /// Observe the paired server's connector catalog, projected to the
    /// connected calendar sources.
    ///
    /// Lazy and source-owned: called only when a caller actually needs source
    /// enumeration, never during turn admission or model profile discovery.
    /// Connections projects the observation; a catalog that names another
    /// caller fails closed, and a malformed one reads as unavailable.
    pub async fn observe_calendar_connections(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Vec<CalendarConnectionRef>, AgentFailure> {
        self.source.revalidate().await?;
        let catalog: ObservedConnectorCatalog = self
            .authenticated_get("/v1/connectors", deadline, cancellation)
            .await?;
        if catalog.person_id != self.source.person_id()
            || catalog.device_id != self.source.device_id()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        floe_connections::project_calendar_connections(
            &ConnectorCatalogObservation {
                schema_version: catalog.schema_version,
                person_id: catalog.person_id,
                device_id: catalog.device_id,
                connectors: catalog.connectors,
            },
            self.source.person_id(),
            self.source.device_id(),
        )
        .ok_or(AgentFailure::CapabilityUnavailable)
    }

    pub async fn observe_source_connections(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Vec<ConnectorSnapshot>, AgentFailure> {
        let observed: ObservedConnectionSnapshots = self
            .authenticated_get("/v1/connections", deadline, cancellation)
            .await?;
        if observed.schema_version != 1
            || observed.person_id != self.source.person_id()
            || observed.device_id != self.source.device_id()
            || observed.connections.len() > 64
            || observed.connections.iter().any(|snapshot| {
                snapshot.descriptor.id != snapshot.connection.connector_id
                    || snapshot.connection.person_id.as_deref() != Some(self.source.person_id())
            })
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(observed.connections)
    }

    async fn authenticated_get<Response: DeserializeOwned>(
        &self,
        path: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<Response, AgentFailure> {
        let _permit = self
            .source_calls
            .acquire(path.len(), deadline, cancellation)
            .await?;
        self.source.revalidate().await?;
        let http = crate::gateway::http::GatewayHttpTransport::new()?;
        let (status, bytes) = http
            .request(
                self.source.base_url(),
                Some(self.source.bearer_token()),
                reqwest::Method::GET,
                path,
                None,
                deadline,
                cancellation,
            )
            .await?;
        if status != 200 {
            return Err(match status {
                401 => AgentFailure::CredentialExpired,
                403 => AgentFailure::PolicyDenied,
                _ => AgentFailure::CapabilityUnavailable,
            });
        }
        crate::gateway::json::strict_json_bytes(&bytes, MAX_CATALOG_BYTES)?;
        self.source.revalidate().await?;
        serde_json::from_slice(&bytes).map_err(|_| AgentFailure::CapabilityUnavailable)
    }
}

/// The producer identity as Access states it.
fn observed_producer(
    producer: &crate::gateway::views::ProducerIdentityResponse,
) -> floe_access::RemoteProducerIdentity {
    floe_access::RemoteProducerIdentity {
        schema_version: producer.schema_version,
        instance_id: producer.instance_id.clone(),
        execution_owner: producer.execution_owner.clone(),
        audience: producer.audience.clone(),
        key_id: producer.key_id.clone(),
        public_key: producer.public_key.clone(),
        fingerprint: producer.fingerprint.clone(),
    }
}

/// The paired server together with the key holder that proves this device to it.
///
/// The transport only fetches and reads; which producer may be trusted, and what
/// the descriptor it signs authorizes, are decided by the caller.
pub struct AuthorizedSourceClient<'a, Keys: ?Sized> {
    pub client: &'a ServerSourceClient,
    pub keys: &'a Keys,
}

impl<'a, Keys: ?Sized> AuthorizedSourceClient<'a, Keys> {
    pub fn new(client: &'a ServerSourceClient, keys: &'a Keys) -> Self {
        Self { client, keys }
    }
}

impl<Keys: AuthorizationSigner + ?Sized> floe_access::RemoteGrantTransport
    for AuthorizedSourceClient<'_, Keys>
{
    fn producer_identity<'a>(
        &'a self,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_agent_contract::BoxFuture<'a, Result<floe_access::RemoteProducerIdentity, AgentFailure>>
    {
        Box::pin(async move {
            let producer = self
                .client
                .authorization_client()?
                .producer_identity(window.deadline, &window.cancellation)
                .await?;
            Ok(observed_producer(&producer))
        })
    }

    fn view_source_preview<'a>(
        &'a self,
        query: floe_access::RemoteSourceQuery<'a>,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_agent_contract::BoxFuture<'a, Result<floe_access::SignedSourcePreview, AgentFailure>>
    {
        Box::pin(async move {
            let preview = self
                .client
                .authorization_client()?
                .view_source_preview(
                    query.view_id,
                    query.connector_id,
                    query.connection_id,
                    query.resource,
                    window.deadline,
                    &window.cancellation,
                )
                .await?;
            Ok(floe_access::SignedSourcePreview {
                descriptor_b64url: preview.descriptor_b64url,
                producer_signature: preview.producer_signature,
                connection_revision: preview.connection_revision,
                producer: observed_producer(&preview.producer),
            })
        })
    }
}

impl<Keys: AuthorizationSigner + ?Sized> floe_context::RemoteViewTransport
    for AuthorizedSourceClient<'_, Keys>
{
    fn read_admitted_view<'a>(
        &'a self,
        read: floe_context::AdmittedRemoteRead<'a>,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_agent_contract::BoxFuture<'a, Result<serde_json::Value, AgentFailure>> {
        Box::pin(async move {
            self.client
                .read_admitted_view(
                    self.keys,
                    AuthorizedViewRead {
                        view_id: read.view_id,
                        grant: read.grant,
                        source_authority: read.source_authority,
                        consumer: read.consumer,
                        resource: read.resource,
                        connection_revision: read.connection_revision,
                        max_items: read.max_items,
                        max_bytes: read.max_bytes,
                        query: read.query,
                        client_id: read.pairing.client_id,
                        device_id: read.pairing.device_id,
                    },
                    window.deadline,
                    &window.cancellation,
                )
                .await
        })
    }
}
