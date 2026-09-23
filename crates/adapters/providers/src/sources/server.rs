use std::{
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use crate::control::authorization::{
    RemoteAuthorizationClient, RemoteViewAuthorizationRequest, parse_calendar_challenge,
};
use crate::control::PreparedServerSource;
use floe_access::{RemoteAuthorizationKeys, RemoteCalendarAuthorizationExpectation};
use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::AgentFailure;
use floe_connections::{CalendarConnectionRef, ConnectorCatalogObservation};
use floe_context::{
    AttentionView, CalendarContextView, CommunicationView, ConfirmedInteractionView, LogisticsView,
    MAX_CALENDAR_CONTEXT_BYTES, MAX_COMMUNICATION_BYTES, MAX_COMMUNICATION_ITEMS,
    MAX_PERSONAL_CONTEXT_BYTES, MAX_PORTFOLIO_VIEW_BYTES, PeopleView, WellbeingView,
    WorkContextView, validate_attention_view, validate_calendar_context_view,
    validate_communication_view, validate_confirmed_interaction_view, validate_logistics_view,
    validate_people_view, validate_wellbeing_view, validate_work_context_view,
};
use floe_execution::limits::{CallLimiter, CallLimits};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;

/// One authorized view read, as the grant it runs under states it.
///
/// Turning a grant into the admission the paired server checks — which
/// authority, which epochs, which bounds — is this transport's shape, not its
/// caller's.
pub struct AuthorizedViewRead<'a> {
    pub view_id: &'a str,
    pub grant: &'a floe_access::DataAccessGrant,
    pub consumer_policy: floe_access::ConsumerPolicyAuthority,
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

pub struct CalendarContextRequest<'input> {
    pub connector_id: &'input str,
    pub connection_id: &'input str,
    pub connection_revision: u64,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub cursor: &'input str,
    pub limit: usize,
}

fn valid_connection_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|identifier| {
        identifier.get_version_num() == 4
            && identifier
                .hyphenated()
                .to_string()
                .eq_ignore_ascii_case(value)
    })
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
    pub fn from_current_connection(
        store: &impl floe_inference::SavedConnectionStore,
        person_id: &str,
        device_id: &str,
    ) -> Result<Option<Self>, AgentFailure> {
        store
            .load()?
            .map(|stored| PreparedServerSource::admit(stored, person_id, device_id).map(Self::new))
            .transpose()
    }

    /// The prepared source this client reads through: endpoint, credential
    /// and pairing identity. The credential stays inside the adapter.
    pub fn source(&self) -> &PreparedServerSource {
        &self.source
    }

    pub fn authorization_client(&self) -> Result<RemoteAuthorizationClient, AgentFailure> {
        RemoteAuthorizationClient::new(self.source.base_url(), self.source.bearer_token())
    }

    #[cfg(test)]
    pub(crate) fn set_call_limiter(&mut self, limiter: CallLimiter) {
        self.source_calls = limiter;
    }

    #[cfg(test)]
    pub(crate) fn call_limiter(&self) -> &CallLimiter {
        &self.source_calls
    }

    /// Read one view the Person's grant admits, through their paired server.
    pub async fn read_admitted_view<Keys: RemoteAuthorizationKeys>(
        &self,
        keys: &Keys,
        read: AuthorizedViewRead<'_>,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<serde_json::Value, AgentFailure> {
        let source = read.grant.source();
        let connector = source.connector();
        let connection = source.connection_id();
        let grant_id = read.grant.id().as_uuid().to_string();
        let grant_incarnation = read.grant.authority().incarnation().to_string();
        let policy_incarnation = read.consumer_policy.incarnation().to_string();
        let max_items = u32::try_from(read.max_items).map_err(|_| AgentFailure::BudgetExceeded)?;
        let max_bytes = u32::try_from(read.max_bytes).map_err(|_| AgentFailure::BudgetExceeded)?;
        let path = format!("/v1/views/{}/admit", read.view_id);
        let expected = RemoteCalendarAuthorizationExpectation {
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
            source_incarnation: source.source_authority().incarnation().to_string(),
            source_epoch: source.source_authority().epoch().get(),
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
            policy_incarnation: &policy_incarnation,
            policy_epoch: read.consumer_policy.epoch().get(),
            grant_id: &grant_id,
            grant_incarnation: &grant_incarnation,
            grant_epoch: read.grant.authority().access_epoch().get(),
            purpose: if read.view_id == floe_context::CALENDAR_CONTEXT_VIEW_ID {
                "everyday_assistance"
            } else {
                "assistant"
            },
            consumer: read.consumer,
            max_items,
            max_bytes,
            query: read.query,
        };
        self.read_authorized_view(keys, request, expected, deadline, cancellation)
            .await
    }

    pub async fn read_authorized_view<Keys: RemoteAuthorizationKeys>(
        &self,
        keys: &Keys,
        request: RemoteViewAuthorizationRequest<'_>,
        mut expected: RemoteCalendarAuthorizationExpectation,
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
        let parts = parse_calendar_challenge(&challenge.challenge_b64url)?;
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
        let release_parts = parse_calendar_challenge(&release.challenge_b64url)?;
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

    pub async fn read_communication_view(
        &self,
        query: &str,
        cursor: usize,
        limit: usize,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<CommunicationView, AgentFailure> {
        if query.len() > 512 || cursor > 10_000 || !(1..=MAX_COMMUNICATION_ITEMS).contains(&limit) {
            return Err(AgentFailure::InvalidInput);
        }
        self.read_view(
            "/v1/views/mail.communication",
            json!({
                "schema_version": AGENT_VERSION,
                "query": query,
                "cursor": cursor,
                "limit": limit,
            }),
            MAX_COMMUNICATION_BYTES,
            deadline,
            cancellation,
            |view, now| validate_communication_view(view, now, limit, MAX_COMMUNICATION_BYTES),
        )
        .await
    }

    pub async fn read_work_context_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<WorkContextView, AgentFailure> {
        self.read_view(
            "/v1/views/work.context",
            json!({"schema_version": AGENT_VERSION}),
            MAX_PORTFOLIO_VIEW_BYTES,
            deadline,
            cancellation,
            validate_work_context_view,
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
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let client = Client::builder()
            .timeout(timeout.min(Duration::from_secs(10)))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let send = client
            .get(format!(
                "{}{path}",
                self.source.base_url().trim_end_matches('/')
            ))
            .bearer_auth(self.source.bearer_token())
            .send();
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            response = send => response.map_err(|error| if error.is_timeout() { AgentFailure::DeadlineExceeded } else { AgentFailure::CapabilityUnavailable })?,
        };
        match response.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED => return Err(AgentFailure::CredentialExpired),
            StatusCode::TOO_MANY_REQUESTS => return Err(AgentFailure::QuotaExceeded),
            _ => return Err(AgentFailure::CapabilityUnavailable),
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        serde_json::from_slice(&bytes).map_err(|_| AgentFailure::CapabilityUnavailable)
    }

    pub async fn read_calendar_context_view(
        &self,
        request: CalendarContextRequest<'_>,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<CalendarContextView, AgentFailure> {
        if !matches!(
            request.connector_id,
            "calendar.google" | "calendar.microsoft"
        ) || !valid_connection_id(request.connection_id)
            || request.connection_revision == 0
            || request.range_start_unix_ms < 0
            || request.range_end_unix_ms <= request.range_start_unix_ms
            || request.range_end_unix_ms - request.range_start_unix_ms > 32 * 86_400_000
            || request.cursor.len() > 2048
            || request.cursor.chars().any(char::is_control)
            || !(1..=128).contains(&request.limit)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let input = json!({
            "schema_version": AGENT_VERSION,
            "connector_id": request.connector_id,
            "connection_id": request.connection_id,
            "connection_revision": request.connection_revision,
            "range_start_unix_ms": request.range_start_unix_ms,
            "range_end_unix_ms": request.range_end_unix_ms,
            "cursor": request.cursor,
            "limit": request.limit,
        });
        self.read_view(
            "/v1/views/calendar.timeline",
            input,
            MAX_CALENDAR_CONTEXT_BYTES,
            deadline,
            cancellation,
            validate_calendar_context_view,
        )
        .await
    }

    pub async fn read_confirmed_interaction_view(
        &self,
        people: &PeopleView,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<ConfirmedInteractionView, AgentFailure> {
        self.read_personal_view(
            "/v1/views/relationships.confirmed_interactions",
            deadline,
            cancellation,
            |view, now| validate_confirmed_interaction_view(view, people, now),
        )
        .await
    }

    pub async fn read_logistics_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<LogisticsView, AgentFailure> {
        self.read_view(
            "/v1/views/life.logistics",
            json!({"schema_version": AGENT_VERSION}),
            MAX_PORTFOLIO_VIEW_BYTES,
            deadline,
            cancellation,
            validate_logistics_view,
        )
        .await
    }

    pub async fn read_people_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<PeopleView, AgentFailure> {
        self.read_personal_view(
            "/v1/views/people.identity",
            deadline,
            cancellation,
            validate_people_view,
        )
        .await
    }

    pub async fn read_attention_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<AttentionView, AgentFailure> {
        self.read_personal_view(
            "/v1/views/attention.coarse",
            deadline,
            cancellation,
            validate_attention_view,
        )
        .await
    }

    pub async fn read_wellbeing_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<WellbeingView, AgentFailure> {
        self.read_personal_view(
            "/v1/views/wellbeing.derived",
            deadline,
            cancellation,
            validate_wellbeing_view,
        )
        .await
    }

    async fn read_personal_view<View: DeserializeOwned>(
        &self,
        path: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
        validate: impl FnOnce(&View, i64) -> Result<(), AgentFailure>,
    ) -> Result<View, AgentFailure> {
        self.read_view(
            path,
            json!({"schema_version": AGENT_VERSION}),
            MAX_PERSONAL_CONTEXT_BYTES,
            deadline,
            cancellation,
            validate,
        )
        .await
    }

    async fn read_view<View: DeserializeOwned>(
        &self,
        path: &str,
        input: serde_json::Value,
        max_bytes: usize,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
        validate: impl FnOnce(&View, i64) -> Result<(), AgentFailure>,
    ) -> Result<View, AgentFailure> {
        let _permit = self
            .source_calls
            .acquire(input.to_string().len(), deadline, cancellation)
            .await?;
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let client = Client::builder()
            .timeout(timeout.min(Duration::from_secs(10)))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| AgentFailure::ServerModelUnavailable)?;
        let send = client
            .post(format!(
                "{}{path}",
                self.source.base_url().trim_end_matches('/')
            ))
            .bearer_auth(self.source.bearer_token())
            .json(&input)
            .send();
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            response = send => response.map_err(|error| if error.is_timeout() { AgentFailure::DeadlineExceeded } else { AgentFailure::CapabilityUnavailable })?,
        };
        match response.status() {
            StatusCode::BAD_REQUEST => return Err(AgentFailure::InvalidInput),
            StatusCode::CONFLICT => return Err(AgentFailure::Conflict),
            StatusCode::UNAUTHORIZED => return Err(AgentFailure::CredentialExpired),
            StatusCode::TOO_MANY_REQUESTS => return Err(AgentFailure::QuotaExceeded),
            status if !status.is_success() => return Err(AgentFailure::CapabilityUnavailable),
            _ => {}
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        if bytes.len() > max_bytes + 4096 {
            return Err(AgentFailure::BudgetExceeded);
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Response<View> {
            schema_version: u32,
            view: View,
        }
        let response: Response<View> =
            serde_json::from_slice(&bytes).map_err(|_| AgentFailure::CapabilityUnavailable)?;
        if response.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?;
        validate(
            &response.view,
            i64::try_from(now.as_millis()).map_err(|_| AgentFailure::StaleContext)?,
        )?;
        Ok(response.view)
    }
}

#[cfg(test)]
mod tests {
    use floe_inference::SavedServerConnection;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::models::server::ServerModelRunner;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const PERSON: &str = "00000000-0000-4000-8000-000000000001";
    const DEVICE: &str = "local-device";

    fn source(base_url: &str) -> PreparedServerSource {
        PreparedServerSource::from_parts(
            base_url,
            "secret_token_value_that_is_long_enough",
            "paired-client",
            PERSON,
            DEVICE,
        )
        .unwrap()
    }

    fn saved(base_url: &str) -> SavedServerConnection {
        SavedServerConnection {
            base_url: base_url.into(),
            token: "secret_token_value_that_is_long_enough".into(),
            client_id: "paired-client".into(),
            person_id: PERSON.into(),
            device_id: DEVICE.into(),
            allow_external: false,
            external_recipients: vec![],
        }
    }

    fn model_route() -> floe_inference::RemoteRoute {
        floe_inference::RemoteRoute {
            base_url: "http://127.0.0.1:8431".into(),
            bearer_token: "secret_token_value_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: true,
            allow_external: false,
            recipient: Some("fixture.example".into()),
            pairing: None,
        }
    }

    #[tokio::test]
    async fn cancelled_queued_source_read_never_opens_a_provider_connection() {
        use std::{future::Future, task::Poll};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut runner = ServerSourceClient::new(source(&format!(
            "http://{}",
            listener.local_addr().unwrap()
        )));
        runner.set_call_limiter(
            CallLimiter::new(CallLimits {
                max_running: 1,
                max_pending: 1,
                max_context_bytes: 65_536,
                max_total_context_bytes: 131_072,
            })
            .unwrap(),
        );
        let parent = floe_execution::Cancellation::new();
        let child = parent.child_scope();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let active = runner
            .call_limiter()
            .acquire(0, deadline, &parent)
            .await
            .unwrap();
        let mut waiting = Box::pin(runner.read_communication_view("", 0, 1, deadline, &child));
        assert!(
            std::future::poll_fn(|context| Poll::Ready(waiting.as_mut().poll(context)))
                .await
                .is_pending()
        );
        child.cancel();
        assert!(matches!(waiting.await, Err(AgentFailure::Cancelled)));
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
        assert!(!parent.is_cancelled());
        let model = ServerModelRunner::new_model_only(model_route()).unwrap();
        assert!(
            model
                .model_call_limiter()
                .acquire(0, deadline, &parent)
                .await
                .is_ok()
        );
        drop(active);
        assert!(
            runner
                .call_limiter()
                .acquire(65_536, deadline, &parent)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn communication_view_read_is_authenticated_bounded_and_validated() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let expected_length = loop {
                let mut chunk = [0_u8; 4096];
                let read = socket.read(&mut chunk).await.unwrap();
                request.extend_from_slice(&chunk[..read]);
                let text = String::from_utf8_lossy(&request);
                if let Some(header_end) = text.find("\r\n\r\n") {
                    let content_length = text[..header_end]
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap();
                    if request.len() >= header_end + 4 + content_length {
                        break header_end + 4 + content_length;
                    }
                }
            };
            let request = String::from_utf8(request[..expected_length].to_vec()).unwrap();
            assert!(request.starts_with("POST /v1/views/mail.communication HTTP/1.1\r\n"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer secret_token_value_that_is_long_enough")
            );
            assert!(request.contains(r#""query":"reply""#));
            assert!(!request.contains("send"));
            let body = serde_json::json!({
                "schema_version": 1,
                "view": {
                    "schema_version": 1,
                    "view_id": "mail.communication",
                    "source_handle": "mail:fixture",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "coverage_complete": true,
                    "items": [{
                        "evidence_handle": "mail:message",
                        "thread_handle": "mail:thread",
                        "received_unix_ms": now - 2,
                        "from": "alex@example.com",
                        "to": "person@example.com",
                        "subject": "Reply needed",
                        "snippet": "Please reply by Friday",
                        "labels": ["INBOX"]
                    }]
                }
            })
            .to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(), body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let model = ServerSourceClient::new(source(&format!("http://{address}")));
        let view = model
            .read_communication_view(
                "reply",
                0,
                25,
                tokio::time::Instant::now() + Duration::from_secs(5),
                &floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        assert_eq!(view.items[0].subject, "Reply needed");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn portfolio_view_reads_use_fixed_routes_and_strict_validation() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        for (path, view) in [
            (
                "/v1/views/work.context",
                json!({
                    "schema_version": 1,
                    "view_id": "work.context",
                    "source_handle": "work:fixture",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "coverage_complete": true,
                    "scope_handle": "workspace:fixture",
                    "items": []
                }),
            ),
            (
                "/v1/views/life.logistics",
                json!({
                    "schema_version": 1,
                    "view_id": "life.logistics",
                    "source_handle": "logistics:fixture",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "coverage_complete": true,
                    "items": []
                }),
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let expected_path = path.to_owned();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 4096];
                let read = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..read]);
                assert!(request.starts_with(&format!("POST {expected_path} HTTP/1.1\r\n")));
                assert!(request.contains(r#"{"schema_version":1}"#));
                let body = json!({"schema_version": 1, "view": view}).to_string();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(), body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            });
            let model = ServerSourceClient::new(source(&format!("http://{address}")));
            if path.ends_with("work.context") {
                model
                    .read_work_context_view(
                        tokio::time::Instant::now() + Duration::from_secs(5),
                        &floe_execution::Cancellation::default(),
                    )
                    .await
                    .unwrap();
            } else {
                model
                    .read_logistics_view(
                        tokio::time::Instant::now() + Duration::from_secs(5),
                        &floe_execution::Cancellation::default(),
                    )
                    .await
                    .unwrap();
            }
            server.await.unwrap();
        }
    }

    #[test]
    fn prepare_binds_saved_connection_without_network_or_model_state() {
        // Absence is local-only, not an error.
        assert!(
            ServerSourceClient::from_current_connection(
                &crate::control::CurrentSavedConnectionStore::fixed(None),
                PERSON,
                DEVICE
            )
            .unwrap()
            .is_none()
        );
        let prepared = ServerSourceClient::from_current_connection(
            &crate::control::CurrentSavedConnectionStore::fixed(Some(saved(
                "http://127.0.0.1:8431",
            ))),
            PERSON,
            DEVICE,
        )
        .unwrap()
        .unwrap();
        assert_eq!(prepared.source().client_id(), "paired-client");
        assert_eq!(prepared.source().person_id(), PERSON);
        assert_eq!(prepared.source().device_id(), DEVICE);
        // A foreign pairing fails closed instead of preparing a client.
        assert_eq!(
            ServerSourceClient::from_current_connection(
                &crate::control::CurrentSavedConnectionStore::fixed(Some(saved(
                    "http://127.0.0.1:8431"
                ))),
                PERSON,
                "other-device"
            )
            .err(),
            Some(AgentFailure::PolicyDenied)
        );
        let mut malformed = saved("http://127.0.0.1:8431");
        malformed.base_url = "http://not-loopback.invalid".into();
        assert_eq!(
            ServerSourceClient::from_current_connection(
                &crate::control::CurrentSavedConnectionStore::fixed(Some(malformed)),
                PERSON,
                DEVICE
            )
            .err(),
            Some(AgentFailure::InvalidInput)
        );
    }

    #[tokio::test]
    async fn source_reads_never_touch_model_or_catalog_discovery() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let server = tokio::spawn(async move {
            for expected_path in ["/v1/views/work.context", "/v1/views/life.logistics"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 4096];
                let read = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..read]);
                let path = request
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default();
                assert!(
                    path != "/v1/inference-purposes" && path != "/v1/connectors",
                    "source reads must never trigger discovery: {path}"
                );
                assert!(request.starts_with(&format!("POST {expected_path} HTTP/1.1\r\n")));
                let view = if expected_path.ends_with("work.context") {
                    json!({
                        "schema_version": 1,
                        "view_id": "work.context",
                        "source_handle": "work:fixture",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 299_999,
                        "coverage_complete": true,
                        "scope_handle": "workspace:fixture",
                        "items": []
                    })
                } else {
                    json!({
                        "schema_version": 1,
                        "view_id": "life.logistics",
                        "source_handle": "logistics:fixture",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 299_999,
                        "coverage_complete": true,
                        "items": []
                    })
                };
                let body = json!({"schema_version": 1, "view": view}).to_string();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(), body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let client = ServerSourceClient::new(source(&format!("http://{address}")));
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let cancellation = floe_execution::Cancellation::default();
        client
            .read_work_context_view(deadline, &cancellation)
            .await
            .unwrap();
        client
            .read_logistics_view(deadline, &cancellation)
            .await
            .unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connector_catalog_is_observed_on_demand_and_projected_by_connections() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let expected_token = source("http://127.0.0.1:9").bearer_token().to_owned();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4096];
            let read = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("GET /v1/connectors HTTP/1.1\r\n"));
            let expected = format!("authorization: bearer {expected_token}");
            assert!(request.to_ascii_lowercase().contains(&expected));
            let body = json!({
                "schema_version": 1,
                "person_id": PERSON,
                "device_id": DEVICE,
                "connectors": [
                    {
                        "id": "calendar.google",
                        "status": "connected",
                        "connection_id": "00000000-0000-4000-8000-000000000010",
                        "connection_revision": 7,
                    },
                    {
                        "id": "calendar.microsoft",
                        "status": "disconnected",
                        "connection_id": "00000000-0000-4000-8000-000000000011",
                        "connection_revision": 3,
                    },
                    {
                        "id": "mail.gmail",
                        "status": "connected",
                        "connection_id": "00000000-0000-4000-8000-000000000012",
                        "connection_revision": 1,
                    },
                ],
            })
            .to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(), body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let client = ServerSourceClient::new(source(&format!("http://{address}")));
        let observed = client
            .observe_calendar_connections(
                tokio::time::Instant::now() + Duration::from_secs(5),
                &floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            observed,
            vec![CalendarConnectionRef {
                connector_id: "calendar.google".into(),
                connection_id: "00000000-0000-4000-8000-000000000010".into(),
                connection_revision: 7,
            }]
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connector_catalog_for_another_caller_or_malformed_fails_closed() {
        for (body, expected) in [
            (
                json!({
                    "schema_version": 1,
                    "person_id": "00000000-0000-4000-8000-000000000099",
                    "device_id": DEVICE,
                    "connectors": [],
                }),
                AgentFailure::PolicyDenied,
            ),
            (
                json!({
                    "schema_version": 1,
                    "person_id": PERSON,
                    "device_id": DEVICE,
                    "connectors": [
                        {"id": "calendar.google", "status": "connected"},
                    ],
                }),
                AgentFailure::CapabilityUnavailable,
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let body = body.to_string();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 4096];
                let read = socket.read(&mut request).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&request[..read])
                        .starts_with("GET /v1/connectors HTTP/1.1\r\n")
                );
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(), body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            });
            let client = ServerSourceClient::new(source(&format!("http://{address}")));
            assert_eq!(
                client
                    .observe_calendar_connections(
                        tokio::time::Instant::now() + Duration::from_secs(5),
                        &floe_execution::Cancellation::default(),
                    )
                    .await,
                Err(expected)
            );
            server.await.unwrap();
        }
    }
}

/// The producer identity as Access states it.
fn observed_producer(
    producer: &crate::control::authorization::ProducerIdentityResponse,
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
pub struct AuthorizedSourceClient<'a, Keys> {
    pub client: &'a ServerSourceClient,
    pub keys: &'a Keys,
}

impl<'a, Keys> AuthorizedSourceClient<'a, Keys> {
    pub fn new(client: &'a ServerSourceClient, keys: &'a Keys) -> Self {
        Self { client, keys }
    }
}

impl<Keys: RemoteAuthorizationKeys> floe_access::RemoteGrantTransport
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

    fn calendar_source_preview<'a>(
        &'a self,
        query: floe_access::RemoteCalendarQuery<'a>,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_agent_contract::BoxFuture<'a, Result<floe_access::SignedCalendarPreview, AgentFailure>>
    {
        Box::pin(async move {
            let preview = self
                .client
                .authorization_client()?
                .calendar_source_preview(
                    query.connector_id,
                    query.connection_id,
                    query.resource,
                    window.deadline,
                    &window.cancellation,
                )
                .await?;
            Ok(floe_access::SignedCalendarPreview {
                descriptor_b64url: preview.descriptor_b64url,
                producer_signature: preview.producer_signature,
                producer: observed_producer(&preview.producer),
            })
        })
    }
}

impl<Keys: RemoteAuthorizationKeys> floe_context::RemoteViewTransport
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
                        grant: &read.binding.grant,
                        consumer_policy: read.binding.consumer_policy,
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
