use std::{
    collections::BTreeMap,
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use floe_agent_contract::{AgentFailure, ModelPlacement, SessionProtection};
use floe_agent_contract::AGENT_VERSION;
use floe_inference::{
    ModelStep, ModelTransport, ModelTransportRequest, ModelTransportResponse,
};
use floe_execution::limits::{CallLimiter, CallLimits};
use floe_connections::{CalendarConnectionRef, ConnectorCatalogObservation};
use floe_inference::{ModelRouteConfig, PurposeAvailability, RemoteModelConnection, RemoteRoute};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::json;

pub struct ServerModelRunner {
    route: ModelRouteConfig,
    placement: ModelPlacement,
    model_calls: CallLimiter,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PurposeInventory {
    schema_version: u32,
    purposes: BTreeMap<String, ObservedPurpose>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedPurpose {
    available: bool,
    requires_external_consent: bool,
    placement: Option<String>,
    recipient: Option<String>,
}

impl ObservedPurpose {
    fn into_availability(self) -> PurposeAvailability {
        PurposeAvailability {
            available: self.available,
            requires_external_consent: self.requires_external_consent,
            placement: self.placement,
            recipient: self.recipient,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedConnectorCatalog {
    schema_version: u32,
    person_id: String,
    device_id: String,
    connectors: Vec<serde_json::Value>,
}

fn model_calls() -> &'static CallLimiter {
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

/// The paired local server, as Inference's route resolver port.
#[derive(Clone, Copy, Debug, Default)]
pub struct RemoteModelRouteResolver;

impl floe_inference::RemoteRouteResolver<ResolvedRemoteConnection> for RemoteModelRouteResolver {
    fn resolve<'a>(
        &'a self,
        connection: &'a RemoteModelConnection,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ResolvedRemoteConnection, AgentFailure>> {
        Box::pin(resolve_remote_model_route(connection))
    }
}

/// What one resolution observed: the route Inference decided, and — separately —
/// the source catalog Connections projected. They travel together only because
/// one HTTP round trip produced both; nothing merges them.
pub struct ResolvedRemoteConnection {
    pub route: RemoteRoute,
    pub calendar_connections: Vec<CalendarConnectionRef>,
}

/// Fetch the facts the local server reports, then let Inference decide the route
/// and Connections project the connector catalog. No policy is decided here.
pub async fn resolve_remote_model_route(
    connection: &RemoteModelConnection,
) -> Result<ResolvedRemoteConnection, AgentFailure> {
    let candidate = floe_inference::candidate_route(connection)?;

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|_| AgentFailure::ServerModelUnavailable)?;
    let inventory: PurposeInventory = authenticated_json(
        &client,
        &candidate.base_url,
        "/v1/inference-purposes",
        &candidate.bearer_token,
    )
    .await?;
    if inventory.schema_version != 1 {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    let availability = inventory
        .purposes
        .into_iter()
        .find(|(name, _)| name == floe_inference::EVERYDAY_ASSISTANCE_PURPOSE)
        .map(|(_, purpose)| purpose.into_availability())
        .ok_or(AgentFailure::ServerModelInvalidOutput)?;
    let planned = floe_inference::plan_remote_route(connection, candidate, &availability)?;

    let catalog: Option<ObservedConnectorCatalog> = authenticated_json(
        &client,
        &planned.base_url,
        "/v1/connectors",
        &planned.bearer_token,
    )
    .await
    .ok();
    let calendar_connections = catalog
        .and_then(|catalog| {
            floe_connections::project_calendar_connections(
                &ConnectorCatalogObservation {
                    schema_version: catalog.schema_version,
                    person_id: catalog.person_id,
                    device_id: catalog.device_id,
                    connectors: catalog.connectors,
                },
                &connection.person_id,
                &connection.device_id,
            )
        })
        .unwrap_or_default();
    Ok(ResolvedRemoteConnection {
        route: planned,
        calendar_connections,
    })
}

async fn authenticated_json<Response: for<'de> Deserialize<'de>>(
    client: &Client,
    base_url: &str,
    path: &str,
    bearer_token: &str,
) -> Result<Response, AgentFailure> {
    let url = format!("{}{path}", base_url.trim_end_matches('/'));
    let mut response = client
        .get(url)
        .bearer_auth(bearer_token)
        .send()
        .await
        .map_err(|_| AgentFailure::ServerModelUnavailable)?;
    match response.status() {
        StatusCode::OK => {}
        StatusCode::UNAUTHORIZED => return Err(AgentFailure::CredentialExpired),
        StatusCode::FORBIDDEN => return Err(AgentFailure::PolicyDenied),
        StatusCode::TOO_MANY_REQUESTS => return Err(AgentFailure::QuotaExceeded),
        _ => return Err(AgentFailure::ServerModelUnavailable),
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AgentFailure::ServerModelUnavailable)?
    {
        if body.len().saturating_add(chunk.len()) > 65_536 {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| AgentFailure::ServerModelInvalidOutput)
}

impl ServerModelRunner {
    pub fn new_model_only(route: RemoteRoute) -> Result<Self, AgentFailure> {
        let model_route = ModelRouteConfig::from_route(&route)?;
        let placement = if route.external {
            ModelPlacement::Remote
        } else {
            ModelPlacement::DeviceLocal
        };
        Ok(Self {
            route: model_route,
            placement,
            model_calls: model_calls().clone(),
        })
    }

    #[cfg(test)]
    pub(crate) fn model_call_limiter(&self) -> &CallLimiter {
        &self.model_calls
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

fn model_input(request: &ModelTransportRequest) -> Result<serde_json::Value, AgentFailure> {
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
    let envelope = request.envelope.clone();
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

trait ReplayRoute {
    fn base_url(&self) -> &str;
    fn purpose(&self) -> &str;
    fn external(&self) -> bool;
}

impl ReplayRoute for ModelRouteConfig {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn purpose(&self) -> &str {
        &self.purpose
    }

    fn external(&self) -> bool {
        self.external
    }
}

impl ReplayRoute for RemoteRoute {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn purpose(&self) -> &str {
        &self.purpose
    }

    fn external(&self) -> bool {
        self.external
    }
}

fn restore_replay<Route: ReplayRoute>(
    replay: &[floe_agent_contract::ModelReplay],
    route: &Route,
    input: &mut serde_json::Value,
) -> Result<(), AgentFailure> {
    let mut seen = std::collections::HashSet::new();
    let mut source = None;
    let mut offset = 0;
    while offset < replay.len() {
        let first = &replay[offset].replay;
        if first.gateway != route.base_url()
            || first.purpose != route.purpose()
            || first.external != route.external()
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

impl ModelTransport for ServerModelRunner {
    fn placement(&self) -> ModelPlacement {
        self.placement
    }

    async fn generate(&self, request: ModelTransportRequest) -> Result<ModelTransportResponse, AgentFailure> {
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
        let mut input = model_input(&request)?;
        self.route.admit()?;
        restore_replay(&request.replay, &self.route, &mut input)?;
        let body = json!({
            "schema_version": 1,
            "purpose": self.route.purpose,
            "data_classes": request.policy.data_classes,
            "allow_external": self.route.allow_external,
            "expected_recipient": self.route.recipient,
            "instructions": request.prompt.render(),
            "input": input
        });
        if body["input"].to_string().len() > 32768 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let body = serde_json::to_vec(&body).map_err(|_| AgentFailure::InvalidInput)?;
        let _permit = self
            .model_calls
            .acquire(body.len(), request.deadline, &request.cancellation)
            .await?;
        self.route.admit()?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?;
        request.policy.authorize(
            self.placement,
            SessionProtection::Encrypted,
            &request.context,
            u64::try_from(now.as_millis()).map_err(|_| AgentFailure::StaleContext)?,
        )?;
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
        let send = client
            .post(format!(
                "{}/v1/agent",
                self.route.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.route.bearer_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
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
            Some(floe_agent_contract::ProviderReplay {
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
        Ok(ModelTransportResponse {
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn replay_groups_all_calls_and_results_and_rejects_partial_batches() {
        let route = route();
        let local_ids = [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()];
        let base = floe_agent_contract::ProviderReplay {
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
                floe_agent_contract::ModelReplay {
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

    /// One attempt's immutable input, as the Session owner would have shaped it.
    fn transport_request() -> ModelTransportRequest {
        use floe_agent_contract::prompts::{
            BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION, CAPABILITY_PROTOCOL,
            CAPABILITY_PROTOCOL_REVISION, PromptAssembly, PromptComponentKind, PromptRole,
            product_component,
        };
        let prompt = PromptAssembly {
            schema_version: 1,
            role: PromptRole::Manager,
            components: vec![
                product_component(
                    PromptComponentKind::BehaviorKernel,
                    "behavior-kernel",
                    BEHAVIOR_KERNEL_REVISION,
                    BEHAVIOR_KERNEL,
                ),
                product_component(
                    PromptComponentKind::Role,
                    "fixture-role",
                    1,
                    "Answer the fixture assignment.",
                ),
                product_component(
                    PromptComponentKind::CapabilityProtocol,
                    "capability-protocol",
                    CAPABILITY_PROTOCOL_REVISION,
                    CAPABILITY_PROTOCOL,
                ),
            ],
        };
        prompt.validate().unwrap();
        let policy = floe_agent_contract::InferencePolicyDecision {
            purpose: "everyday_assistance".into(),
            data_classes: vec![floe_agent_contract::DataClass::Synthetic],
            allowed_placements: vec![ModelPlacement::Remote],
            performance_class: "fast".into(),
            projection_version: 1,
            external_transfer_consent: floe_agent_contract::TransferConsent::Granted,
            bounded_sensitive_projection: false,
        };
        let context = floe_agent_contract::AgentContext {
            projection_version: 1,
            persona: None,
            optional_context_issues: vec![],
            memories: vec![],
            evidence: vec![],
        };
        ModelTransportRequest {
            schema_version: 1,
            attempt_id: uuid::Uuid::new_v4(),
            prompt: prompt.clone(),
            policy: policy.clone(),
            context: context.clone(),
            envelope: floe_agent_contract::ContextEnvelope {
                schema_version: 1,
                stable_instructions: prompt.clone(),
                scoped_instructions: floe_agent_contract::ScopedInstructions {
                    purpose: policy.purpose.clone(),
                    available_capabilities: vec![],
                    active_experts: vec![],
                },
                contextual_data: floe_agent_contract::ContextualData {
                    projection_version: 1,
                    memories: vec![],
                    optional_context_issues: vec![],
                    evidence: vec![],
                },
                conversation: floe_agent_contract::ConversationContext {
                    history: vec![],
                    current_turn: vec![json!({"role": "user", "content": "Hello"})],
                },
                runtime: floe_agent_contract::RuntimeContext {
                    max_output_bytes: 1024,
                },
                manifest: floe_agent_contract::ContextManifest {
                    prompt_components: prompt
                        .components
                        .iter()
                        .map(|component| floe_agent_contract::PromptManifestEntry {
                            kind: component.kind,
                            source: component.source.clone(),
                            revision: component.revision,
                        })
                        .collect(),
                    evidence: vec![],
                    memories: vec![],
                    agent_cards: vec![],
                },
            },
            capabilities: vec![],
            active_agents: vec![],
            replay: vec![],
            remaining_tokens: 512,
            remaining_cost_micros: 0,
            max_output_bytes: 1024,
            deadline: tokio::time::Instant::now() + Duration::from_secs(5),
            cancellation: floe_execution::Cancellation::new(),
        }
    }

    fn route() -> RemoteRoute {
        RemoteRoute {
            base_url: "http://127.0.0.1:8431".into(),
            bearer_token: "secret_token_value_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: true,
            allow_external: false,
            recipient: Some("fixture.example".into()),
            pairing: None,
        }
    }

    async fn inventory_server(
        responses: Vec<(&'static str, serde_json::Value)>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for (path, response) in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0_u8; 1024];
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert_ne!(count, 0);
                    request.extend_from_slice(&chunk[..count]);
                    if request.windows(4).any(|value| value == b"\r\n\r\n") {
                        break;
                    }
                    assert!(request.len() <= 16_384);
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n")
                );
                let response = response.to_string();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            response.len(),
                            response
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        (address, server)
    }

    fn connection(base_url: String) -> RemoteModelConnection {
        RemoteModelConnection {
            base_url,
            bearer_token: "a".repeat(32),
            client_id: "paired-client".into(),
            person_id: "00000000-0000-4000-8000-000000000001".into(),
            device_id: "local-device".into(),
            allow_external: false,
            external_recipients: vec![],
        }
    }

    #[tokio::test]
    async fn host_route_snapshot_uses_server_authority_and_scoped_catalog() {
        let connection_id = uuid::Uuid::new_v4();
        let (base_url, server) = inventory_server(vec![
            (
                "/v1/inference-purposes",
                json!({
                    "schema_version": 1,
                    "purposes": {
                        "everyday_assistance": {
                            "available": true,
                            "requires_external_consent": false,
                            "placement": "server_local"
                        }
                    }
                }),
            ),
            (
                "/v1/connectors",
                json!({
                    "schema_version": 1,
                    "person_id": "00000000-0000-4000-8000-000000000001",
                    "device_id": "local-device",
                    "connectors": [{
                        "id": "calendar.google",
                        "status": "connected",
                        "connection_id": connection_id,
                        "connection_revision": 4
                    }]
                }),
            ),
        ])
        .await;
        let route = resolve_remote_model_route(&connection(base_url))
            .await
            .unwrap();
        assert!(!route.route.external);
        assert!(!route.route.allow_external);
        assert_eq!(route.route.recipient, None);
        assert_eq!(route.calendar_connections.len(), 1);
        assert_eq!(
            route.calendar_connections[0].connection_id,
            connection_id.to_string()
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn external_route_requires_saved_consent_for_exact_recipient() {
        let (base_url, server) = inventory_server(vec![(
            "/v1/inference-purposes",
            json!({
                "schema_version": 1,
                "purposes": {
                    "everyday_assistance": {
                        "available": true,
                        "requires_external_consent": true,
                        "placement": "external",
                        "recipient": "model.example"
                    }
                }
            }),
        )])
        .await;
        assert!(matches!(
            resolve_remote_model_route(&connection(base_url)).await,
            Err(AgentFailure::ConsentRequired)
        ));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn optional_catalog_failure_does_not_remove_the_model_route() {
        let (base_url, server) = inventory_server(vec![
            (
                "/v1/inference-purposes",
                json!({
                    "schema_version": 1,
                    "purposes": {
                        "everyday_assistance": {
                            "available": true,
                            "requires_external_consent": false,
                            "placement": "server_local"
                        }
                    }
                }),
            ),
            (
                "/v1/connectors",
                json!({"schema_version": 1, "person_id": "wrong", "device_id": "wrong", "connectors": []}),
            ),
        ])
        .await;
        let route = resolve_remote_model_route(&connection(base_url))
            .await
            .unwrap();
        assert!(route.calendar_connections.is_empty());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn model_only_route_ignores_source_bindings() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut route = route();
        route.base_url = format!("http://{}", listener.local_addr().unwrap());
        // A source catalog is not part of the model route, so a model-only
        // runner never sees one.
        let runner = ServerModelRunner::new_model_only(route).unwrap();
        assert_eq!(runner.placement(), ModelPlacement::Remote);
        assert!(!runner.route.allow_external);
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }

    #[tokio::test]
    async fn model_only_dispatch_uses_model_endpoint_without_source_bindings() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut config = route();
        config.base_url = format!("http://{}", listener.local_addr().unwrap());
        config.allow_external = true;
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let (header_end, content_length) = loop {
                let mut chunk = [0_u8; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                assert_ne!(count, 0);
                bytes.extend_from_slice(&chunk[..count]);
                assert!(bytes.len() <= 65_536);
                let text = String::from_utf8_lossy(&bytes);
                if let Some(header_end) = text.find("\r\n\r\n") {
                    let content_length = text[..header_end]
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap();
                    if bytes.len() >= header_end + 4 + content_length {
                        break (header_end, content_length);
                    }
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]).to_ascii_lowercase();
            assert!(headers.starts_with("post /v1/agent http/1.1\r\n"));
            assert!(
                headers.contains("authorization: bearer secret_token_value_that_is_long_enough")
            );
            let body: serde_json::Value =
                serde_json::from_slice(&bytes[header_end + 4..header_end + 4 + content_length])
                    .unwrap();
            assert_eq!(body["purpose"], "everyday_assistance");
            assert_eq!(body["allow_external"], true);
            assert_eq!(body["expected_recipient"], "fixture.example");
            assert!(!body.to_string().contains("unavailable.source"));
            let response = json!({
                "schema_version": 1,
                "purpose": "everyday_assistance",
                "trace_id": "a".repeat(32),
                "routing": {"placement": "remote", "external_transfer": true, "replay_source": ""},
                "output": json!({"output": [{"kind": "answer", "text": "Fixture answer"}], "used_tokens": 3}).to_string()
            }).to_string();
            socket.write_all(format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(), response
            ).as_bytes()).await.unwrap();
        });
        let request = transport_request();
        let mut denied = config.clone();
        denied.allow_external = false;
        assert!(matches!(
            ServerModelRunner::new_model_only(denied)
                .unwrap()
                .generate(request.clone())
                .await,
            Err(AgentFailure::ConsentRequired)
        ));
        let mut missing_recipient = config.clone();
        missing_recipient.recipient = None;
        assert!(matches!(
            ServerModelRunner::new_model_only(missing_recipient)
                .unwrap()
                .generate(request.clone())
                .await,
            Err(AgentFailure::InvalidInput)
        ));
        let response = ServerModelRunner::new_model_only(config)
            .unwrap()
            .generate(request)
            .await
            .unwrap();
        assert_eq!(
            response.output,
            vec![ModelStep::Answer {
                text: "Fixture answer".into()
            }]
        );
        assert_eq!(response.used_tokens, 3);
        server.await.unwrap();
    }

    #[test]
    fn replay_restores_original_ids_without_runner_memory_and_rejects_foreign_routes() {
        let route = route();
        let call_id = uuid::Uuid::new_v4();
        let replay = floe_agent_contract::ModelReplay {
            call_id,
            replay: floe_agent_contract::ProviderReplay {
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
        let reloaded = floe_agent_contract::ModelReplay {
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
            restore_replay(
                std::slice::from_ref(&replay),
                &foreign,
                &mut original.clone()
            ),
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
        assert!(ServerModelRunner::new_model_only(valid.clone()).is_ok());
        let rendered = format!("{valid:?}");
        assert!(!rendered.contains(&valid.bearer_token));
        assert!(rendered.contains("[REDACTED]"));

        for invalid in [
            "https://127.0.0.1:8431",
            "http://localhost:8431",
            "http://127.0.0.1:8431/path",
            "http://192.168.1.2:8431",
            "http://127.0.0.1",
            "http://127.0.0.1:8431?query=true",
            "http://127.0.0.1:8431#fragment",
        ] {
            let mut candidate = route();
            candidate.base_url = invalid.into();
            assert!(ServerModelRunner::new_model_only(candidate.clone()).is_err());
        }

        for token in ["short".to_owned(), "x".repeat(257), " ".repeat(32)] {
            let mut candidate = route();
            candidate.bearer_token = token;
            assert!(ServerModelRunner::new_model_only(candidate).is_err());
        }
        let mut wrong_purpose = route();
        wrong_purpose.purpose = "other".into();
        assert!(ServerModelRunner::new_model_only(wrong_purpose).is_err());

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
