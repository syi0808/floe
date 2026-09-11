use std::time::{Duration, SystemTime};

use floe_agent::{
    AGENT_VERSION, AgentFailure, AttentionView, CalendarContextView, CommunicationView,
    ConfirmedInteractionView, LogisticsView, MAX_CALENDAR_CONTEXT_BYTES, MAX_COMMUNICATION_BYTES,
    MAX_COMMUNICATION_ITEMS, MAX_PERSONAL_CONTEXT_BYTES, MAX_PORTFOLIO_VIEW_BYTES, ModelPlacement,
    ModelRequest, ModelResponse, ModelRunner, ModelStep, PeopleView, SessionProtection,
    WellbeingView, WorkContextView, validate_attention_view, validate_calendar_context_view,
    validate_communication_view, validate_confirmed_interaction_view, validate_logistics_view,
    validate_people_view, validate_wellbeing_view, validate_work_context_view,
};
use floe_protocol::{AgentRemoteCalendarConnectionDto, AgentRemoteRouteDto};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;

pub struct ServerModelRunner {
    route: AgentRemoteRouteDto,
    placement: ModelPlacement,
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

impl ServerModelRunner {
    pub fn new(route: AgentRemoteRouteDto) -> Result<Self, AgentFailure> {
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
        let placement = if route.external {
            ModelPlacement::Remote
        } else {
            ModelPlacement::DeviceLocal
        };
        Ok(Self { route, placement })
    }

    pub fn calendar_connections(&self) -> &[AgentRemoteCalendarConnectionDto] {
        &self.route.calendar_connections
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

#[derive(Deserialize)]
struct GenerateResponse {
    schema_version: u32,
    purpose: String,
    output: String,
    routing: RoutingResponse,
    trace_id: String,
}

#[derive(Deserialize)]
struct RoutingResponse {
    placement: String,
    external_transfer: bool,
    replay_source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentOutput {
    output: Vec<ModelStep>,
    used_tokens: u64,
    #[serde(default)]
    replay: Option<serde_json::Value>,
    #[serde(default)]
    call_ids: Vec<String>,
}

fn gateway_failure(error: &serde_json::Value) -> AgentFailure {
    match error["error"]["code"].as_str() {
        Some("invalid_proposal") => AgentFailure::ServerModelInvalidOutput,
        Some("credential_expired") => AgentFailure::CredentialExpired,
        Some("quota_exceeded") => AgentFailure::QuotaExceeded,
        Some("model_timeout") => AgentFailure::ServerModelTimeout,
        Some("request_rejected" | "validation") => AgentFailure::ServerModelRequestRejected,
        Some(code) if code.starts_with("invalid_agent_") => {
            AgentFailure::ServerModelRequestRejected
        }
        _ => AgentFailure::ServerModelUnavailable,
    }
}

fn tool_name(identifier: &str) -> String {
    let hash = identifier
        .bytes()
        .fold(0xcbf29ce484222325_u64, |hash, value| {
            (hash ^ u64::from(value)).wrapping_mul(0x100000001b3)
        });
    format!("floe_{hash:016x}")
}

const DELEGATION_CAPABILITY_ID: &str = "floe.a2a.delegate";

fn rewrite_tool_calls(message: &mut serde_json::Value) -> Result<(), AgentFailure> {
    let Some(calls) = message
        .get_mut("tool_calls")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(());
    };
    for call in calls {
        let function = &mut call["function"];
        let identifier = function["name"]
            .as_str()
            .ok_or(AgentFailure::InvalidInput)?;
        function["name"] = json!(tool_name(identifier));
        function["arguments"] = json!(function["arguments"].to_string());
    }
    Ok(())
}

fn model_input(request: &ModelRequest) -> Result<serde_json::Value, AgentFailure> {
    let mut aliases: std::collections::HashSet<_> = request
        .capabilities
        .iter()
        .map(|capability| tool_name(&capability.id))
        .collect();
    if aliases.len() != request.capabilities.len() {
        return Err(AgentFailure::InvalidInput);
    }
    if !request.active_agents.is_empty() && !aliases.insert(tool_name(DELEGATION_CAPABILITY_ID)) {
        return Err(AgentFailure::InvalidInput);
    }
    let envelope = request.context_envelope()?;
    let mut messages = vec![json!({"role": "user", "content": json!({
        "scoped_instructions": envelope.scoped_instructions,
        "contextual_data": envelope.contextual_data,
        "runtime": envelope.runtime,
        "manifest": envelope.manifest,
    }).to_string()})];
    for mut message in envelope
        .conversation
        .history
        .into_iter()
        .chain(envelope.conversation.current_turn)
    {
        rewrite_tool_calls(&mut message)?;
        if message["role"] == "tool" {
            let content = if message["status"] == "error" {
                json!({"status":"error", "failure": message["failure"]})
            } else {
                json!({"status":"success", "content": message["content"]})
            };
            message = json!({"role":"tool", "tool_call_id":message["tool_call_id"],"content":content.to_string()});
        }
        messages.push(message);
    }
    let mut tools: Vec<_> = request.capabilities.iter().map(|capability| json!({
        "type": "function",
        "function": {
            "name": tool_name(&capability.id),
            "description": capability.id,
            "parameters": capability.input_schema.clone().unwrap_or_else(|| json!({"type":"object","properties":{}})),
            "strict": false
        }
    })).collect();
    if !request.active_agents.is_empty() {
        tools.push(json!({
            "type": "function",
            "function": {
                "name": tool_name(DELEGATION_CAPABILITY_ID),
                "description": "Delegate a natural-language assignment to one active Expert agent.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "agent_id": {
                            "type": "string",
                            "enum": request.active_agents.iter().map(|card| card.id.clone()).collect::<Vec<_>>()
                        },
                        "message": {"type": "string", "minLength": 1, "maxLength": 4096}
                    },
                    "required": ["agent_id", "message"],
                    "additionalProperties": false
                },
                "strict": false
            }
        }));
    }
    Ok(json!({"messages": messages, "tools": tools}))
}

fn restore_replay(
    replay: &[floe_agent::ModelReplay],
    route: &AgentRemoteRouteDto,
    input: &mut serde_json::Value,
) -> Result<(), AgentFailure> {
    let mut seen = std::collections::HashSet::new();
    let mut source = None;
    let mut offset = 0;
    while offset < replay.len() {
        let first = &replay[offset].replay;
        if first.gateway != route.base_url
            || first.purpose != route.purpose
            || first.external != route.external
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if source.as_ref().is_some_and(|value| value != &first.source) {
            return Err(AgentFailure::InvalidInput);
        }
        source = Some(first.source.clone());
        let count = first.call_ids.len();
        if count == 0 || count > 8 || offset + count > replay.len() {
            return Err(AgentFailure::InvalidInput);
        }
        let messages = input["messages"]
            .as_array_mut()
            .ok_or(AgentFailure::InvalidInput)?;
        let mut start = None;
        let mut calls = vec![];
        let mut results = vec![];
        for (index, saved) in replay[offset..offset + count].iter().enumerate() {
            let mut canonical = saved.replay.clone();
            canonical.provider_call_id = first.provider_call_id.clone();
            if canonical != *first
                || saved.replay.provider_call_id != first.call_ids[index]
                || !seen.insert(saved.call_id)
            {
                return Err(AgentFailure::InvalidInput);
            }
            let local_id = saved.call_id.to_string();
            let position = messages
                .iter()
                .position(|message| message["tool_calls"][0]["id"] == local_id)
                .ok_or(AgentFailure::InvalidInput)?;
            let beginning = *start.get_or_insert(position);
            if position != beginning + index * 2
                || position + 1 >= messages.len()
                || messages[position]["tool_calls"].as_array().map(Vec::len) != Some(1)
                || messages[position + 1]["role"] != "tool"
                || messages[position + 1]["tool_call_id"] != local_id
            {
                return Err(AgentFailure::InvalidInput);
            }
            let mut call = messages[position]["tool_calls"][0].clone();
            call["id"] = json!(saved.replay.provider_call_id);
            let mut result = messages[position + 1].clone();
            result["tool_call_id"] = json!(saved.replay.provider_call_id);
            calls.push(call);
            results.push(result);
        }
        let mut assistant = json!({"role":"assistant", "tool_calls":calls});
        if !first.preamble.is_empty() {
            assistant["content"] = json!(first.preamble);
        }
        if !first.items.is_null() {
            assistant["provider_items"] = first.items.clone();
        }
        let beginning = start.ok_or(AgentFailure::InvalidInput)?;
        messages.splice(
            beginning..beginning + count * 2,
            std::iter::once(assistant).chain(results),
        );
        offset += count;
    }
    if let Some(source) = source {
        input["replay_source"] = json!(source);
    }
    Ok(())
}

fn decode_output(output: &str) -> Result<AgentOutput, AgentFailure> {
    let result: AgentOutput =
        serde_json::from_str(output).map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
    if result.output.is_empty() || result.output.len() > 16 {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    for step in &result.output {
        match step {
            ModelStep::Answer { text } | ModelStep::Preamble { text }
                if !text.trim().is_empty() => {}
            ModelStep::Call {
                capability_id,
                input,
            } if !capability_id.is_empty()
                && serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(input)
                    .is_ok() => {}
            ModelStep::Delegate { agent_id, message }
                if !agent_id.is_empty() && !message.trim().is_empty() => {}
            _ => return Err(AgentFailure::ServerModelInvalidOutput),
        }
    }
    Ok(result)
}

impl ModelRunner for ServerModelRunner {
    fn placement(&self) -> ModelPlacement {
        self.placement
    }

    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        request.prompt.validate()?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?;
        request.policy.authorize(
            self.placement,
            SessionProtection::Encrypted,
            &request.context,
            u64::try_from(now.as_millis()).map_err(|_| AgentFailure::StaleContext)?,
        )?;
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let timeout = request
            .deadline
            .saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let client = Client::builder()
            .timeout(timeout.min(Duration::from_secs(30)))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| AgentFailure::ServerModelUnavailable)?;
        let mut input = model_input(&request)?;
        restore_replay(&request.replay, &self.route, &mut input)?;
        let body = json!({
            "schema_version": 1,
            "purpose": self.route.purpose,
            "data_classes": request.policy.data_classes,
            "allow_external": self.route.allow_external,
            "instructions": request.prompt.render(),
            "input": input
        });
        if body["input"].to_string().len() > 32768 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let send = client
            .post(format!(
                "{}/v1/agent",
                self.route.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.route.bearer_token)
            .json(&body)
            .send();
        let response = tokio::select! {
            _ = request.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            response = send => response.map_err(|error| if error.is_timeout() { AgentFailure::DeadlineExceeded } else { AgentFailure::ServerModelUnavailable })?,
        };
        match response.status() {
            StatusCode::CONFLICT => return Err(AgentFailure::PolicyDenied),
            StatusCode::UNAUTHORIZED => return Err(AgentFailure::CredentialExpired),
            StatusCode::FORBIDDEN => return Err(AgentFailure::ConsentRequired),
            StatusCode::TOO_MANY_REQUESTS => return Err(AgentFailure::QuotaExceeded),
            status if !status.is_success() => {
                let error: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|_| AgentFailure::ServerModelUnavailable)?;
                return Err(gateway_failure(&error));
            }
            _ => {}
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| AgentFailure::ServerModelUnavailable)?;
        if bytes.len() > 65_536 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let response: GenerateResponse =
            serde_json::from_slice(&bytes).map_err(|_| AgentFailure::ServerModelUnavailable)?;
        if response.schema_version != 1
            || response.purpose != self.route.purpose
            || response.trace_id.len() != 32
            || response.routing.external_transfer != self.route.external
            || response.routing.placement
                != if self.route.external {
                    "remote"
                } else {
                    "server_local"
                }
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut output = decode_output(&response.output)?;
        let mut call_count = 0;
        let mut preambles = vec![];
        for step in &mut output.output {
            match step.clone() {
                ModelStep::Call {
                    capability_id,
                    input,
                } if capability_id == tool_name(DELEGATION_CAPABILITY_ID) => {
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct DelegationInput {
                        agent_id: String,
                        message: String,
                    }
                    let delegation: DelegationInput = serde_json::from_str(&input)
                        .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
                    if !request
                        .active_agents
                        .iter()
                        .any(|card| card.id == delegation.agent_id)
                        || delegation.message.trim().is_empty()
                    {
                        return Err(AgentFailure::CapabilityDenied);
                    }
                    *step = ModelStep::Delegate {
                        agent_id: delegation.agent_id,
                        message: delegation.message,
                    };
                    call_count += 1;
                }
                ModelStep::Call { capability_id, .. } => {
                    let descriptor = request
                        .capabilities
                        .iter()
                        .find(|capability| tool_name(&capability.id) == capability_id)
                        .ok_or(AgentFailure::CapabilityDenied)?;
                    *step = match step.clone() {
                        ModelStep::Call { input, .. } => ModelStep::Call {
                            capability_id: descriptor.id.clone(),
                            input,
                        },
                        _ => unreachable!(),
                    };
                    call_count += 1;
                }
                ModelStep::Preamble { text } => preambles.push(text.clone()),
                _ => {}
            }
        }
        if serde_json::to_vec(&output.output)
            .map_err(|_| AgentFailure::ServerModelInvalidOutput)?
            .len()
            > request.max_output_bytes.min(16384)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let replay = if call_count > 0 {
            let unique: std::collections::HashSet<_> = output.call_ids.iter().collect();
            if output.call_ids.len() != call_count
                || unique.len() != call_count
                || output
                    .call_ids
                    .iter()
                    .any(|id| id.is_empty() || id.len() > 128)
                || response.routing.replay_source.len() != 64
                || !response
                    .routing
                    .replay_source
                    .bytes()
                    .all(|value| value.is_ascii_hexdigit())
            {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            Some(floe_agent::ProviderReplay {
                gateway: self.route.base_url.clone(),
                purpose: self.route.purpose.clone(),
                external: self.route.external,
                source: response.routing.replay_source,
                provider_call_id: output.call_ids[0].clone(),
                call_ids: output.call_ids,
                preamble: preambles.join("\n"),
                items: output.replay.unwrap_or(serde_json::Value::Null),
            })
        } else {
            if !output.call_ids.is_empty() || output.replay.is_some() {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            None
        };
        Ok(ModelResponse {
            replay,
            schema_version: AGENT_VERSION,
            output: output.output,
            used_tokens: output.used_tokens.max(1),
            cost_micros: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn replay_groups_all_calls_and_results_and_rejects_partial_batches() {
        let route = route();
        let local_ids = [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()];
        let base = floe_agent::ProviderReplay {
            gateway: route.base_url.clone(),
            purpose: route.purpose.clone(),
            external: route.external,
            source: "a".repeat(64),
            call_ids: vec!["provider_a".into(), "provider_b".into()],
            provider_call_id: "provider_a".into(),
            preamble: "Checking both.".into(),
            items: json!([
                {"type":"reasoning","encrypted_content":"private"},
                {"type":"function_call","call_id":"provider_a","name":"read","arguments":"{}"},
                {"type":"function_call","call_id":"provider_b","name":"read","arguments":"{}"}
            ]),
        };
        let replay: Vec<_> = local_ids
            .iter()
            .enumerate()
            .map(|(index, call_id)| {
                let mut record = base.clone();
                record.provider_call_id = base.call_ids[index].clone();
                floe_agent::ModelReplay {
                    call_id: *call_id,
                    replay: record,
                }
            })
            .collect();
        let messages: Vec<_> = local_ids.iter().flat_map(|call_id| [
            json!({"role":"assistant","tool_calls":[{"id":call_id,"function":{"name":"read","arguments":"{}"}}]}),
            json!({"role":"tool","tool_call_id":call_id,"content":"observed"}),
        ]).collect();
        let original = json!({"messages":messages});
        let mut restored = original.clone();
        restore_replay(&replay, &route, &mut restored).unwrap();
        assert_eq!(restored["messages"].as_array().unwrap().len(), 3);
        assert_eq!(
            restored["messages"][0]["tool_calls"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(restored["messages"][0]["content"], "Checking both.");
        assert_eq!(restored["messages"][0]["provider_items"], base.items);
        assert_eq!(restored["messages"][1]["tool_call_id"], "provider_a");
        assert_eq!(restored["messages"][2]["tool_call_id"], "provider_b");
        assert_eq!(
            restore_replay(&replay[..1], &route, &mut original.clone()),
            Err(AgentFailure::InvalidInput)
        );
        let mut altered = replay.clone();
        altered[1].replay.items = json!([]);
        assert_eq!(
            restore_replay(&altered, &route, &mut original.clone()),
            Err(AgentFailure::InvalidInput)
        );
    }

    use super::*;

    fn route() -> AgentRemoteRouteDto {
        AgentRemoteRouteDto {
            base_url: "http://127.0.0.1:8431".into(),
            bearer_token: "secret_token_value_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: true,
            allow_external: false,
            calendar_connections: vec![],
        }
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
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let mut route = route();
        route.base_url = format!("http://{address}");
        let model = ServerModelRunner::new(route).unwrap();
        let view = model
            .read_communication_view(
                "reply",
                0,
                25,
                tokio::time::Instant::now() + Duration::from_secs(5),
                &floe_agent::Cancellation::default(),
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
            let mut route = route();
            route.base_url = format!("http://{address}");
            let model = ServerModelRunner::new(route).unwrap();
            if path.ends_with("work.context") {
                model
                    .read_work_context_view(
                        tokio::time::Instant::now() + Duration::from_secs(5),
                        &floe_agent::Cancellation::default(),
                    )
                    .await
                    .unwrap();
            } else {
                model
                    .read_logistics_view(
                        tokio::time::Instant::now() + Duration::from_secs(5),
                        &floe_agent::Cancellation::default(),
                    )
                    .await
                    .unwrap();
            }
            server.await.unwrap();
        }
    }

    #[test]
    fn replay_restores_original_ids_without_runner_memory_and_rejects_foreign_routes() {
        let route = route();
        let call_id = uuid::Uuid::new_v4();
        let replay = floe_agent::ModelReplay {
            call_id,
            replay: floe_agent::ProviderReplay {
                gateway: route.base_url.clone(),
                purpose: route.purpose.clone(),
                external: route.external,
                source: "a".repeat(64),
                call_ids: vec!["original".into()],
                preamble: String::new(),
                provider_call_id: "original".into(),
                items: json!([{"type":"reasoning","encrypted_content":"opaque"},{"type":"function_call","call_id":"original","name":"read","arguments":"{}"}]),
            },
        };
        let original = json!({"messages":[
            {"role":"assistant","tool_calls":[{"id":call_id.to_string(),"function":{"name":"read","arguments":"{}"}}]},
            {"role":"tool","tool_call_id":call_id.to_string(),"content":"observed"}
        ]});
        let encoded = serde_json::to_string(&replay.replay).unwrap();
        let reloaded = floe_agent::ModelReplay {
            call_id,
            replay: serde_json::from_str(&encoded).unwrap(),
        };
        let mut restored = original.clone();
        restore_replay(&[reloaded], &route, &mut restored).unwrap();
        assert_eq!(restored["messages"][0]["tool_calls"][0]["id"], "original");
        assert_eq!(restored["messages"][1]["tool_call_id"], "original");
        assert_eq!(
            restored["messages"][0]["provider_items"][0]["encrypted_content"],
            "opaque"
        );
        assert_eq!(restored["replay_source"], "a".repeat(64));
        let mut foreign = route.clone();
        foreign.base_url = "http://127.0.0.1:9431".into();
        assert_eq!(
            restore_replay(&[replay.clone()], &foreign, &mut original.clone()),
            Err(AgentFailure::PolicyDenied)
        );
        let mut orphan = replay;
        orphan.call_id = uuid::Uuid::new_v4();
        assert_eq!(
            restore_replay(&[orphan], &route, &mut original.clone()),
            Err(AgentFailure::InvalidInput)
        );
    }

    #[test]
    fn route_accepts_only_loopback_and_redacts_credentials() {
        let valid = route();
        assert!(ServerModelRunner::new(valid.clone()).is_ok());
        let rendered = format!("{valid:?}");
        assert!(!rendered.contains(&valid.bearer_token));
        assert!(rendered.contains("[REDACTED]"));

        for invalid in [
            "https://127.0.0.1:8431",
            "http://localhost:8431",
            "http://127.0.0.1:8431/path",
            "http://192.168.1.2:8431",
        ] {
            let mut candidate = route();
            candidate.base_url = invalid.into();
            assert!(ServerModelRunner::new(candidate).is_err());
        }

        for (connection_id, revision) in [
            ("not-a-uuid", 1),
            ("00000000-0000-3000-8000-000000000001", 1),
            ("00000000-0000-4000-8000-000000000001", 0),
        ] {
            let mut candidate = route();
            candidate.calendar_connections = vec![AgentRemoteCalendarConnectionDto {
                connector_id: "calendar.google".into(),
                connection_id: connection_id.into(),
                connection_revision: revision,
            }];
            assert!(ServerModelRunner::new(candidate).is_err());
        }
    }

    #[test]
    fn native_output_is_server_normalized_not_model_json() {
        assert!(
            decode_output(r#"{"output":[{"kind":"answer","text":"Hello"}],"used_tokens":12}"#)
                .is_ok()
        );
        assert!(
            decode_output(
                r#"{"output":[{"kind":"answer","text":"Hello","input":null}],"used_tokens":12}"#
            )
            .is_err()
        );
        assert!(decode_output(r#"{"output":[{"kind":"call","capability_id":"floe_read","input":"{}"}],"used_tokens":12}"#).is_ok());
        assert!(decode_output(r#"{"output":[{"kind":"call","capability_id":"floe_read","input":"null"}],"used_tokens":12}"#).is_err());
        assert_ne!(tool_name("a.b"), tool_name("a_b"));
        assert_eq!(tool_name("hello"), "floe_a430d84680aabd0b");
    }

    #[test]
    fn tool_call_rewrite_does_not_add_field_to_plain_messages() {
        let mut plain = json!({"role":"user","content":"hello"});
        rewrite_tool_calls(&mut plain).unwrap();
        assert_eq!(plain, json!({"role":"user","content":"hello"}));
        assert!(plain.get("tool_calls").is_none());

        let mut assistant = json!({
            "role":"assistant",
            "tool_calls":[{
                "id":"call-1",
                "function":{"name":"calendar.read","arguments":{"date":"today"}}
            }]
        });
        rewrite_tool_calls(&mut assistant).unwrap();
        assert_eq!(
            assistant["tool_calls"][0]["function"]["name"],
            tool_name("calendar.read")
        );
        assert_eq!(
            assistant["tool_calls"][0]["function"]["arguments"],
            r#"{"date":"today"}"#
        );
    }

    #[test]
    fn gateway_failures_preserve_actionable_categories() {
        for (code, failure) in [
            ("invalid_proposal", AgentFailure::ServerModelInvalidOutput),
            ("credential_expired", AgentFailure::CredentialExpired),
            ("quota_exceeded", AgentFailure::QuotaExceeded),
            ("model_timeout", AgentFailure::ServerModelTimeout),
            ("request_rejected", AgentFailure::ServerModelRequestRejected),
            ("validation", AgentFailure::ServerModelRequestRejected),
            (
                "invalid_agent_tool_parameters",
                AgentFailure::ServerModelRequestRejected,
            ),
            ("model_unavailable", AgentFailure::ServerModelUnavailable),
        ] {
            assert_eq!(gateway_failure(&json!({"error":{"code":code}})), failure);
        }
    }
}
