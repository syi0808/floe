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
            .map_err(|_| AgentFailure::ModelUnavailable)?;
        let body = json!({
            "schema_version": 2,
            "purpose": self.route.purpose,
            "allow_external": self.route.allow_external,
            "instructions": request.system_instructions,
            "input": {
                "scoped": {"policy": request.policy, "context": request.context},
                "recent_messages": request.messages,
                "allowed_capabilities": request.capabilities,
                "max_output_bytes": request.max_output_bytes.min(16384),
            },
            "output_schema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "kind": {"type": "string", "enum": ["answer", "call"]},
                    "text": {"type": "string"},
                    "capability_id": {"type": "string"},
                    "input": {"type": "string"}
                },
                "required": ["kind"]
            }
        });
        let send = client
            .post(format!(
                "{}/v2/generate",
                self.route.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.route.bearer_token)
            .json(&body)
            .send();
        let response = tokio::select! {
            _ = request.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            response = send => response.map_err(|error| if error.is_timeout() { AgentFailure::DeadlineExceeded } else { AgentFailure::ModelUnavailable })?,
        };
        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(AgentFailure::CredentialExpired),
            StatusCode::FORBIDDEN => return Err(AgentFailure::ConsentRequired),
            StatusCode::TOO_MANY_REQUESTS => return Err(AgentFailure::QuotaExceeded),
            status if !status.is_success() => return Err(AgentFailure::ModelUnavailable),
            _ => {}
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| AgentFailure::ModelUnavailable)?;
        if bytes.len() > 65_536 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let response: GenerateResponse =
            serde_json::from_slice(&bytes).map_err(|_| AgentFailure::InvalidModelOutput)?;
        if response.schema_version != 2
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
            return Err(AgentFailure::InvalidModelOutput);
        }
        let step: ModelStep =
            serde_json::from_str(&response.output).map_err(|_| AgentFailure::InvalidModelOutput)?;
        if let ModelStep::Call { capability_id, .. } = &step
            && !request
                .capabilities
                .iter()
                .any(|capability| capability.id == *capability_id)
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        if serde_json::to_vec(&step)
            .map_err(|_| AgentFailure::InvalidModelOutput)?
            .len()
            > request.max_output_bytes.min(16384)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(ModelResponse {
            schema_version: AGENT_VERSION,
            step,
            used_tokens: 4096,
            cost_micros: 0,
        })
    }
}

#[cfg(test)]
mod tests {
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
}
