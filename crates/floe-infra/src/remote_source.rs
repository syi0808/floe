use std::{
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use crate::remote_authorization::{
    RemoteAuthorizationClient, RemoteViewAuthorizationRequest, parse_calendar_challenge,
};
use floe_agent::{
    AGENT_VERSION, AgentFailure, AttentionView, CalendarContextView, CommunicationView,
    ConfirmedInteractionView, LogisticsView, MAX_CALENDAR_CONTEXT_BYTES, MAX_COMMUNICATION_BYTES,
    MAX_COMMUNICATION_ITEMS, MAX_PERSONAL_CONTEXT_BYTES, MAX_PORTFOLIO_VIEW_BYTES, PeopleView,
    WellbeingView, WorkContextView, validate_attention_view, validate_calendar_context_view,
    validate_communication_view, validate_confirmed_interaction_view, validate_logistics_view,
    validate_people_view, validate_wellbeing_view, validate_work_context_view,
};
use floe_core::{EncryptedAgentVault, RemoteCalendarAuthorizationExpectation, VaultKeyProvider};
use floe_execution::limits::{CallLimiter, CallLimits};
use floe_protocol::{AgentRemoteCalendarConnectionDto, AgentRemoteRouteDto};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;

pub struct ServerSourceClient {
    route: AgentRemoteRouteDto,
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

fn validate_route(route: &AgentRemoteRouteDto) -> Result<(), AgentFailure> {
    let address = Url::parse(&route.base_url).map_err(|_| AgentFailure::InvalidInput)?;
    if address.scheme() != "http"
        || address.host_str() != Some("127.0.0.1")
        || address.path() != "/"
        || address.query().is_some()
        || address.fragment().is_some()
        || address.port().is_none()
        || route.bearer_token.len() < 32
        || route.bearer_token.len() > 256
        || !route
            .bearer_token
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
        || route.purpose != "everyday_assistance"
        || route.calendar_connections.len() > 2
        || route.calendar_connections.iter().any(|connection| {
            !matches!(
                connection.connector_id.as_str(),
                "calendar.google" | "calendar.microsoft"
            ) || !valid_connection_id(&connection.connection_id)
                || connection.connection_revision == 0
        })
        || route
            .calendar_connections
            .iter()
            .enumerate()
            .any(|(index, connection)| {
                route.calendar_connections[..index]
                    .iter()
                    .any(|candidate| candidate.connector_id == connection.connector_id)
            })
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl ServerSourceClient {
    pub fn new(route: AgentRemoteRouteDto) -> Result<Self, AgentFailure> {
        validate_route(&route)?;
        Ok(Self {
            route,
            source_calls: source_calls().clone(),
        })
    }

    pub fn calendar_connections(&self) -> &[AgentRemoteCalendarConnectionDto] {
        &self.route.calendar_connections
    }

    pub fn authorization_client(&self) -> Result<RemoteAuthorizationClient, AgentFailure> {
        RemoteAuthorizationClient::new(&self.route)
    }

    #[cfg(test)]
    pub(crate) fn set_call_limiter(&mut self, limiter: CallLimiter) {
        self.source_calls = limiter;
    }

    #[cfg(test)]
    pub(crate) fn call_limiter(&self) -> &CallLimiter {
        &self.source_calls
    }

    pub async fn read_authorized_view<Keys: VaultKeyProvider>(
        &self,
        vault: &EncryptedAgentVault<Keys>,
        request: RemoteViewAuthorizationRequest<'_>,
        mut expected: RemoteCalendarAuthorizationExpectation,
        deadline: tokio::time::Instant,
        cancellation: &floe_agent::Cancellation,
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
        let client = RemoteAuthorizationClient::new(&self.route)?;
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
            || self.route.pairing.as_ref().is_some_and(|pairing| {
                parts.client_id != pairing.client_id || parts.device_id != pairing.device_id
            })
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
                vault,
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
                vault,
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
        cancellation: &floe_agent::Cancellation,
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
        cancellation: &floe_agent::Cancellation,
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

    pub async fn read_calendar_context_view(
        &self,
        request: CalendarContextRequest<'_>,
        deadline: tokio::time::Instant,
        cancellation: &floe_agent::Cancellation,
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
        cancellation: &floe_agent::Cancellation,
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
        cancellation: &floe_agent::Cancellation,
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
        cancellation: &floe_agent::Cancellation,
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
        cancellation: &floe_agent::Cancellation,
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
        cancellation: &floe_agent::Cancellation,
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
        cancellation: &floe_agent::Cancellation,
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
        cancellation: &floe_agent::Cancellation,
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
                self.route.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.route.bearer_token)
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
