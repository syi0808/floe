use std::time::{Duration, SystemTime};

use floe_agent::{
    AGENT_VERSION, AgentFailure, ModelPlacement, ModelRequest, ModelResponse, ModelRunner,
    ModelStep, SessionProtection,
};
use floe_protocol::AgentRemoteRouteDto;
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde_json::json;

pub struct ServerModelRunner {
    route: AgentRemoteRouteDto,
    placement: ModelPlacement,
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

fn model_input(request: &ModelRequest) -> Result<serde_json::Value, AgentFailure> {
    let aliases: std::collections::HashSet<_> = request
        .capabilities
        .iter()
        .map(|capability| tool_name(&capability.id))
        .collect();
    if aliases.len() != request.capabilities.len() {
        return Err(AgentFailure::InvalidInput);
    }
    let (history, current) = request.model_conversation();
    if !current.iter().any(|message| message["role"] == "user") {
        return Err(AgentFailure::InvalidInput);
    }
    let mut messages = vec![json!({"role": "user", "content": json!({
        "scoped": {"policy": request.policy, "context": request.context}
    }).to_string()})];
    for mut message in history.into_iter().chain(current) {
        if let Some(calls) = message["tool_calls"].as_array_mut() {
            for call in calls {
                let function = &mut call["function"];
                let identifier = function["name"]
                    .as_str()
                    .ok_or(AgentFailure::InvalidInput)?;
                function["name"] = json!(tool_name(identifier));
                function["arguments"] = json!(function["arguments"].to_string());
            }
        }
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
    let tools: Vec<_> = request.capabilities.iter().map(|capability| json!({
        "type": "function",
        "function": {
            "name": tool_name(&capability.id),
            "description": capability.id,
            "parameters": capability.input_schema.clone().unwrap_or_else(|| json!({"type":"object","properties":{}})),
            "strict": false
        }
    })).collect();
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
            "instructions": request.system_instructions,
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
            StatusCode::BAD_GATEWAY => {
                let error: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|_| AgentFailure::ServerModelUnavailable)?;
                return Err(gateway_failure(&error));
            }
            status if !status.is_success() => return Err(AgentFailure::ServerModelUnavailable),
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
            match step {
                ModelStep::Call { capability_id, .. } => {
                    let descriptor = request
                        .capabilities
                        .iter()
                        .find(|capability| tool_name(&capability.id) == *capability_id)
                        .ok_or(AgentFailure::CapabilityDenied)?;
                    *capability_id = descriptor.id.clone();
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
    fn gateway_failures_preserve_actionable_categories() {
        for (code, failure) in [
            ("invalid_proposal", AgentFailure::ServerModelInvalidOutput),
            ("credential_expired", AgentFailure::CredentialExpired),
            ("quota_exceeded", AgentFailure::QuotaExceeded),
            ("request_rejected", AgentFailure::ServerModelUnavailable),
            ("model_unavailable", AgentFailure::ServerModelUnavailable),
        ] {
            assert_eq!(gateway_failure(&json!({"error":{"code":code}})), failure);
        }
    }
}
