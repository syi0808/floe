use std::{
    collections::BTreeMap,
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::{
    AgentFailure, MAX_CONTEXT_REFS, MAX_OUTPUT_BYTES, ModelPlacement, SessionProtection,
    valid_context_refs,
};
use floe_execution::limits::{CallLimiter, CallLimits};
use floe_inference::{ModelRouteConfig, PurposeAvailability, RemoteRoute};
use floe_inference::{ModelStep, ModelTransport, ModelTransportRequest, ModelTransportResponse};
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

/// Canonical model profile observation: saved private connection
/// → `/v1/inference-purposes` → non-secret profile. Never calls
/// `/v1/connectors`; that path remains a Connections/source concern for the
/// temporary legacy source/Expert route only.
async fn fetch_canonical_model_purposes(
    base_url: &str,
    bearer_token: &str,
) -> Result<PurposeInventory, AgentFailure> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|_| AgentFailure::ServerModelUnavailable)?;
    let inventory: PurposeInventory =
        authenticated_json(&client, base_url, "/v1/inference-purposes", bearer_token).await?;
    if inventory.schema_version != 1 {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    Ok(inventory)
}

fn canonical_server_profile(
    inventory: PurposeInventory,
) -> Result<floe_inference::ModelProfile, AgentFailure> {
    let availability = inventory
        .purposes
        .into_iter()
        .find(|(name, _)| name == floe_inference::EVERYDAY_ASSISTANCE_PURPOSE)
        .map(|(_, purpose)| purpose.into_availability())
        .ok_or(AgentFailure::ServerModelInvalidOutput)?;
    let (execution_location, data_recipient) = match (
        availability.placement.as_deref(),
        availability.recipient.as_deref(),
    ) {
        (Some("server_local"), None) => (
            floe_inference::ExecutionLocation::Gateway,
            floe_inference::DataRecipient::Device,
        ),
        (Some("external"), Some(recipient))
            if floe_inference::valid_external_recipient(recipient) =>
        {
            (
                floe_inference::ExecutionLocation::Remote,
                floe_inference::DataRecipient::external(recipient)
                    .ok_or(AgentFailure::ServerModelInvalidOutput)?,
            )
        }
        _ => return Err(AgentFailure::ServerModelInvalidOutput),
    };
    Ok(floe_inference::ModelProfile {
        id: "server-model".into(),
        purpose: floe_inference::ModelPurpose::new(floe_inference::EVERYDAY_ASSISTANCE_PURPOSE)
            .ok_or(AgentFailure::InvalidInput)?,
        consumer: floe_inference::ModelConsumer::new("conversation.root")
            .ok_or(AgentFailure::InvalidInput)?,
        execution_location,
        data_recipient,
        capabilities: floe_inference::ModelCapabilities(vec!["chat".into()]),
        available: availability.available,
    })
}

/// Canonical server provider. Base URL and bearer stay private inside the
/// prepared transport; Inference only sees non-secret profile facts.
pub struct ServerModelProvider {
    base_url: String,
    bearer_token: String,
    allow_external: bool,
}

impl ServerModelProvider {
    pub fn new(base_url: String, bearer_token: String) -> Result<Self, AgentFailure> {
        if !base_url.starts_with("http://127.0.0.1:") || bearer_token.len() < 32 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self {
            base_url,
            bearer_token,
            allow_external: false,
        })
    }

    /// Build from a saved connection already admitted against the verified
    /// caller. Secrets stay in this adapter; only non-secret profile facts
    /// ever leave through `observe_profiles`. Recipient authority is always
    /// re-read from the saved connection store at Access-check time; this
    /// provider keeps no recipient snapshot.
    pub fn for_connection(
        connection: &floe_inference::RemoteModelConnection,
    ) -> Result<Self, AgentFailure> {
        let provider = Self::new(
            connection.base_url.clone(),
            connection.bearer_token.clone(),
        )?;
        Ok(Self {
            allow_external: connection.allow_external,
            ..provider
        })
    }
}

pub struct PreparedServerTransport {
    base_url: String,
    bearer_token: String,
    allow_external: bool,
    recipient: Option<String>,
    model_calls: CallLimiter,
}

impl floe_inference::PreparedModelTransport for PreparedServerTransport {
    async fn generate(
        &self,
        request: floe_inference::CanonicalModelRequest,
    ) -> Result<floe_inference::CanonicalModelResponse, AgentFailure> {
        request.validate()?;
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if request.deadline <= tokio::time::Instant::now() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        // Canonical transport never consults legacy policy/context and never
        // performs the old `InferencePolicyDecision::authorize`: dispatch
        // authority already arrived through the Access permit/fence, and the
        // envelope/catalog own everything the wire may carry.
        let instructions = request.envelope.stable_instructions.render();
        if instructions.len() > 4096 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let input = canonical_model_input(&request)?;
        let body = json!({
            "schema_version": 1,
            "purpose": floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            "data_classes": request.input_data_classes,
            "allow_external": self.allow_external,
            "expected_recipient": self.recipient,
            "instructions": instructions,
            "input": input,
        });
        if body["input"].to_string().len() > 32768 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let body = serde_json::to_vec(&body).map_err(|_| AgentFailure::InvalidInput)?;
        let _permit = self
            .model_calls
            .acquire(body.len(), request.deadline, &request.cancellation)
            .await?;
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
        let send = client
            .post(format!(
                "{}/v1/agent",
                self.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.bearer_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send();
        let response = tokio::select! {
            _ = request.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            response = send => response.map_err(|error| if error.is_timeout() { AgentFailure::DeadlineExceeded } else { AgentFailure::ServerModelUnavailable })?,
        };
        match response.status() {
            StatusCode::OK => {}
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
        let expected_placement = if self.recipient.is_some() {
            "remote"
        } else {
            "server_local"
        };
        if response.schema_version != 1
            || response.purpose != floe_inference::EVERYDAY_ASSISTANCE_PURPOSE
            || response.trace_id.len() != 32
            || response.routing.external_transfer != self.recipient.is_some()
            || response.routing.placement != expected_placement
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let output = decode_output(&response.output)?;
        // Map wire steps back to canonical agent steps with the exact catalog
        // definition revision; Engine remains the whole-batch grammar owner.
        // Inference re-checks revisions in `map_output`, so this mapping
        // resolves identity, never authority.
        let mut steps = Vec::with_capacity(output.output.len());
        let mut calls = 0;
        for step in output.output {
            if matches!(step, ModelStep::Call { .. }) {
                calls += 1;
            }
            steps.push(map_canonical_step(step, &request.catalog)?);
        }
        if calls > 0 {
            let unique: std::collections::HashSet<_> = output.call_ids.iter().collect();
            if output.call_ids.len() != calls
                || unique.len() != calls
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
        } else if !output.call_ids.is_empty() || output.replay.is_some() {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
        if serde_json::to_vec(&steps)
            .map_err(|_| AgentFailure::ServerModelInvalidOutput)?
            .len()
            > request.max_output_bytes.min(16384)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(floe_inference::CanonicalModelResponse {
            output: steps,
            used_tokens: output.used_tokens.max(1),
            cost_micros: 0,
        })
    }
}

/// Canonical `/v1/agent` input from the immutable envelope and the bounded
/// catalog. Mirrors the legacy wire shape; the only authority inputs are the
/// envelope (already authorized) and the catalog (already bounded).
fn canonical_model_input(
    request: &floe_inference::CanonicalModelRequest,
) -> Result<serde_json::Value, AgentFailure> {
    let mut aliases: std::collections::HashSet<_> = request
        .catalog
        .tools
        .iter()
        .map(|tool| tool_name(&tool.id))
        .collect();
    if aliases.len() != request.catalog.tools.len() {
        return Err(AgentFailure::InvalidInput);
    }
    if !request.catalog.cards.is_empty() && !aliases.insert(tool_name(DELEGATION_CAPABILITY_ID)) {
        return Err(AgentFailure::InvalidInput);
    }
    let envelope = &request.envelope;
    let mut messages = vec![json!({"role": "user", "content": json!({
        "scoped_instructions": envelope.scoped_instructions,
        "contextual_data": envelope.contextual_data,
        "runtime": envelope.runtime,
        "manifest": envelope.manifest,
    }).to_string()})];
    for mut message in super::wire::wire_messages(&envelope.conversation.history)
        .into_iter()
        .chain(super::wire::wire_messages(
            &envelope.conversation.current_turn,
        ))
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
    let mut tools: Vec<_> = request
        .catalog
        .tools
        .iter()
        .map(|tool| {
            let parameters: serde_json::Value = serde_json::from_str(&tool.input_schema)
                .unwrap_or_else(|_| json!({"type": "object", "properties": {}}));
            json!({
                "type": "function",
                "function": {
                    "name": tool_name(&tool.id),
                    "description": tool.description,
                    "parameters": parameters,
                    "strict": false
                }
            })
        })
        .collect();
    if !request.catalog.cards.is_empty() {
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
                            "enum": request.catalog.cards.iter().map(|definition| definition.card.id.clone()).collect::<Vec<_>>()
                        },
                        "message": {"type": "string", "minLength": 1, "maxLength": 4096},
                        "context_refs": {
                            "type": "array",
                            "items": {"type": "string", "maxLength": MAX_OUTPUT_BYTES},
                            "maxItems": MAX_CONTEXT_REFS
                        }
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

/// Resolve one decoded wire step to its canonical agent step. Tool and Expert
/// identity resolve against the bounded catalog; the definition revision is
/// copied from the catalog entry the wire alias matched.
fn map_canonical_step(
    step: ModelStep,
    catalog: &floe_agent_contract::AllowedCatalog,
) -> Result<floe_agent_contract::ModelStep, AgentFailure> {
    match step {
        ModelStep::Answer { text } => Ok(floe_agent_contract::ModelStep::Answer {
            text,
            artifacts: vec![],
        }),
        ModelStep::Preamble { text } => Ok(floe_agent_contract::ModelStep::Preamble { text }),
        ModelStep::Call {
            capability_id,
            input,
        } if capability_id == tool_name(DELEGATION_CAPABILITY_ID) => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct DelegationInput {
                agent_id: String,
                message: String,
                #[serde(default)]
                context_refs: Vec<String>,
            }
            let delegation: DelegationInput =
                serde_json::from_str(&input).map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
            if !valid_context_refs(&delegation.context_refs) {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            let definition = catalog
                .cards
                .iter()
                .find(|definition| definition.card.id == delegation.agent_id)
                .ok_or(AgentFailure::CapabilityDenied)?;
            if delegation.message.trim().is_empty() {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            Ok(floe_agent_contract::ModelStep::Delegate {
                agent_id: definition.card.id.clone(),
                definition_revision: definition.definition_revision,
                message: delegation.message,
                context_refs: delegation.context_refs,
            })
        }
        ModelStep::Call {
            capability_id,
            input,
        } => {
            if serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&input).is_err()
            {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            let descriptor = catalog
                .tools
                .iter()
                .find(|tool| tool_name(&tool.id) == capability_id)
                .ok_or(AgentFailure::CapabilityDenied)?;
            Ok(floe_agent_contract::ModelStep::CallTool {
                tool_id: descriptor.id.clone(),
                definition_revision: descriptor.definition_revision,
                input,
            })
        }
        ModelStep::Delegate {
            agent_id,
            message,
            context_refs,
        } => {
            if !valid_context_refs(&context_refs) {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            let definition = catalog
                .cards
                .iter()
                .find(|definition| definition.card.id == agent_id)
                .ok_or(AgentFailure::CapabilityDenied)?;
            if message.trim().is_empty() {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            Ok(floe_agent_contract::ModelStep::Delegate {
                agent_id: definition.card.id.clone(),
                definition_revision: definition.definition_revision,
                message,
                context_refs,
            })
        }
    }
}

impl floe_inference::ModelProvider for ServerModelProvider {
    type Prepared = PreparedServerTransport;

    async fn observe_profiles(
        &self,
    ) -> Vec<floe_inference::PreparedModelProfile<Self::Prepared>> {
        let Ok(inventory) =
            fetch_canonical_model_purposes(&self.base_url, &self.bearer_token).await
        else {
            return Vec::new();
        };
        let Ok(profile) = canonical_server_profile(inventory) else {
            return Vec::new();
        };
        // The prepared transport is pinned to the exact recipient the server
        // inventory declared for this profile. Access admits that recipient
        // before handoff; the wire echoes it back as `expected_recipient`.
        let recipient = match &profile.data_recipient {
            floe_inference::DataRecipient::Device => None,
            floe_inference::DataRecipient::External(recipient) => Some(recipient.clone()),
        };
        vec![floe_inference::PreparedModelProfile {
            profile,
            transport: PreparedServerTransport {
                base_url: self.base_url.clone(),
                bearer_token: self.bearer_token.clone(),
                allow_external: self.allow_external,
                recipient,
                model_calls: model_calls().clone(),
            },
        }]
    }
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
    for mut message in super::wire::wire_messages(&envelope.conversation.history)
        .into_iter()
        .chain(super::wire::wire_messages(
            &envelope.conversation.current_turn,
        ))
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
                        "message": {"type": "string", "minLength": 1, "maxLength": 4096},
                        "context_refs": {
                            "type": "array",
                            "items": {"type": "string", "maxLength": MAX_OUTPUT_BYTES},
                            "maxItems": MAX_CONTEXT_REFS
                        }
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
            ModelStep::Delegate {
                agent_id,
                message,
                context_refs,
            } if !agent_id.is_empty()
                && !message.trim().is_empty()
                && valid_context_refs(context_refs) => {}
            _ => return Err(AgentFailure::ServerModelInvalidOutput),
        }
    }
    Ok(result)
}

impl ModelTransport for ServerModelRunner {
    fn placement(&self) -> ModelPlacement {
        self.placement
    }

    async fn generate(
        &self,
        request: ModelTransportRequest,
    ) -> Result<ModelTransportResponse, AgentFailure> {
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
                        #[serde(default)]
                        context_refs: Vec<String>,
                    }
                    let delegation: DelegationInput = serde_json::from_str(&input)
                        .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
                    if !valid_context_refs(&delegation.context_refs) {
                        return Err(AgentFailure::ServerModelInvalidOutput);
                    }
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
                        context_refs: delegation.context_refs,
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
                    response_contract: String::new(),
                    available_capabilities: vec![],
                    active_experts: vec![],
                    correction: None,
                },
                contextual_data: floe_agent_contract::ContextualData {
                    projection_version: 1,
                    memories: vec![],
                    optional_context_issues: vec![],
                    evidence: vec![],
                },
                conversation: floe_agent_contract::ModelConversation {
                    history: vec![],
                    current_turn: vec![floe_agent_contract::ModelConversationEntry::User {
                        message_id: uuid::Uuid::new_v4(),
                        text: "Hello".into(),
                    }],
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
    fn delegation_decode_preserves_context_refs() {
        let output = decode_output(
            r#"{"output":[{"kind":"delegate","agent_id":"expert-a","message":"hi","context_refs":["turn:1","evidence:9"]}],"used_tokens":3}"#,
        )
        .unwrap();
        assert!(matches!(
            output.output.as_slice(),
            [ModelStep::Delegate { context_refs, .. }]
                if context_refs == &["turn:1".to_string(), "evidence:9".to_string()]
        ));
    }

    #[test]
    fn delegation_decode_defaults_missing_context_refs_to_empty() {
        let output = decode_output(
            r#"{"output":[{"kind":"delegate","agent_id":"expert-a","message":"hi"}],"used_tokens":3}"#,
        )
        .unwrap();
        assert!(matches!(
            output.output.as_slice(),
            [ModelStep::Delegate { context_refs, .. }] if context_refs.is_empty()
        ));
    }

    #[test]
    fn delegation_decode_rejects_oversized_context_refs() {
        let refs = std::iter::repeat_n("\"r\",", 129).collect::<String>();
        let refs = refs.strip_suffix(',').unwrap();
        let payload = format!(
            "{{\"output\":[{{\"kind\":\"delegate\",\"agent_id\":\"expert-a\",\"message\":\"hi\",\"context_refs\":[{refs}]}}],\"used_tokens\":3}}"
        );
        assert!(matches!(
            decode_output(&payload),
            Err(AgentFailure::ServerModelInvalidOutput)
        ));
    }

    #[test]
    fn delegate_tool_schema_carries_optional_context_refs() {
        use floe_agent_contract::AgentCard;
        let mut request = transport_request();
        request.active_agents = vec![AgentCard {
            schema_version: floe_agent_contract::AGENT_SCHEMA_VERSION,
            protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
            id: "expert-a".into(),
            version: "1".into(),
            name: "expert-a".into(),
            description: "fixture expert".into(),
            supported_placements: vec![floe_agent_contract::ModelPlacement::Remote],
            domain_tags: vec![],
            skills: vec![],
        }];
        let input = model_input(&request).unwrap();
        let delegate = input["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["function"]["name"] == tool_name(DELEGATION_CAPABILITY_ID))
            .expect("delegate tool is advertised")
            .clone();
        let parameters = &delegate["function"]["parameters"];
        assert_eq!(parameters["properties"]["context_refs"]["type"], "array");
        assert_eq!(
            parameters["properties"]["context_refs"]["maxItems"],
            json!(MAX_CONTEXT_REFS)
        );
        assert!(!parameters["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "context_refs"));
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

    #[tokio::test]
    async fn canonical_discovery_calls_purposes_and_never_connectors() {
        let (base_url, server) = inventory_server(vec![(
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
        )])
        .await;
        let inventory = fetch_canonical_model_purposes(&base_url, &"a".repeat(32))
            .await
            .unwrap();
        let profile = canonical_server_profile(inventory).unwrap();
        assert_eq!(profile.id, "server-model");
        assert_eq!(
            profile.data_recipient,
            floe_inference::DataRecipient::Device
        );
        // Only the single purposes call happened; a second connectors call
        // would have left the test server pending and this await would hang.
        server.await.unwrap();
    }

    #[tokio::test]
    async fn canonical_generate_posts_agent_wire_and_maps_answer() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            // Canonical discovery first: purposes, never connectors.
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
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("GET /v1/inference-purposes HTTP/1.1\r\n"));
            let inventory = json!({
                "schema_version": 1,
                "purposes": {
                    "everyday_assistance": {
                        "available": true,
                        "requires_external_consent": false,
                        "placement": "server_local"
                    }
                }
            })
            .to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        inventory.len(),
                        inventory
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            // Then the canonical model call.
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let length = loop {
                let mut chunk = [0_u8; 4096];
                let count = socket.read(&mut chunk).await.unwrap();
                assert_ne!(count, 0);
                request.extend_from_slice(&chunk[..count]);
                let text = String::from_utf8_lossy(&request);
                if let Some(header_end) = text.find("\r\n\r\n") {
                    let length = text[..header_end]
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + length {
                        break length;
                    }
                }
            };
            let text = String::from_utf8(request).unwrap();
            assert!(text.starts_with("POST /v1/agent HTTP/1.1\r\n"));
            let expected_auth =
                format!("authorization: Bearer {}\r\n", "c".repeat(32));
            assert!(
                text.to_ascii_lowercase()
                    .contains(&expected_auth.to_ascii_lowercase())
            );
            let (_, body) = text.split_once("\r\n\r\n").unwrap();
            let body: serde_json::Value = serde_json::from_str(&body[..length]).unwrap();
            assert_eq!(body["schema_version"], 1);
            assert_eq!(body["purpose"], "everyday_assistance");
            assert!(!body["instructions"].as_str().unwrap().is_empty());
            assert!(body["input"]["messages"].as_array().unwrap().len() >= 2);
            let output = json!({
                "output": [{"kind": "answer", "text": "Hello from server"}],
                "used_tokens": 7,
            });
            let response = json!({
                "schema_version": 1,
                "purpose": "everyday_assistance",
                "trace_id": "b".repeat(32),
                "routing": {
                    "placement": "server_local",
                    "external_transfer": false,
                    "replay_source": "b".repeat(64),
                },
                "output": output.to_string(),
            })
            .to_string();
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
        });
        let provider =
            ServerModelProvider::new(base_url, "c".repeat(32)).unwrap();
        let observed = floe_inference::ModelProvider::observe_profiles(&provider).await;
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].profile.id, "server-model");
        let legacy = transport_request();
        let request = floe_inference::CanonicalModelRequest {
            attempt_id: uuid::Uuid::new_v4(),
            envelope: legacy.envelope,
            catalog: floe_agent_contract::AllowedCatalog {
                cards: vec![],
                tools: vec![],
                revision: 1,
            },
            input_data_classes: vec![floe_agent_contract::DataClass::Synthetic],
            remaining_tokens: 512,
            remaining_cost_micros: 0,
            max_output_bytes: 1024,
            deadline: tokio::time::Instant::now() + Duration::from_secs(10),
            cancellation: floe_execution::Cancellation::new(),
        };
        let response = floe_inference::PreparedModelTransport::generate(
            &observed[0].transport,
            request,
        )
        .await
        .unwrap();
        assert!(matches!(
            response.output.as_slice(),
            [floe_agent_contract::ModelStep::Answer { text, .. }]
                if text == "Hello from server"
        ));
        assert_eq!(response.used_tokens, 7);
        assert_eq!(response.cost_micros, 0);
        server.await.unwrap();
    }

    #[test]
    fn canonical_provider_public_surface_contains_no_secret() {
        let _provider = ServerModelProvider::new(
            "http://127.0.0.1:9".into(),
            "b".repeat(32),
        )
        .unwrap();
        // Base URL and bearer stay private inside the provider/transport:
        // no public field or getter exposes them, and the canonical
        // request/response/profile types have no such fields.
        let request_fields = [
            "attempt_id",
            "envelope",
            "catalog",
            "input_data_classes",
            "remaining_tokens",
            "remaining_cost_micros",
            "max_output_bytes",
            "deadline",
            "cancellation",
        ];
        for field in request_fields {
            assert!(!field.contains("bearer"));
            assert!(!field.contains("base_url"));
        }
    }
}
