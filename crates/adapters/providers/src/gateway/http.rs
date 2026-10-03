//! Bounded private HTTP mechanics. This layer never retries or redirects.
use std::time::Duration;
use floe_agent_contract::AgentFailure;
use floe_execution::Cancellation;
use reqwest::{Client, Method};
use tokio::time::Instant;
use super::inference_wire::{MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES};

#[derive(Clone)]
pub(crate) struct GatewayHttpTransport { client: Client }
impl GatewayHttpTransport {
    pub fn new() -> Result<Self, AgentFailure> {
        let client = Client::builder().connect_timeout(Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none()).no_proxy()
            .build().map_err(|_| AgentFailure::ServerModelUnavailable)?;
        Ok(Self { client })
    }
    pub async fn request(&self, endpoint: &str, token: Option<&str>, method: Method, path: &str,
        body: Option<Vec<u8>>, deadline: Instant, cancellation: &Cancellation)
        -> Result<(u16, Vec<u8>), AgentFailure> {
        if !valid_endpoint(endpoint) || !path.starts_with('/') || path.contains('?') || path.contains('#') {
            return Err(AgentFailure::InvalidInput);
        }
        if body.as_ref().is_some_and(|body| body.len() > MAX_REQUEST_BYTES) { return Err(AgentFailure::BudgetExceeded); }
        if cancellation.is_cancelled() { return Err(AgentFailure::Cancelled); }
        if Instant::now() >= deadline { return Err(AgentFailure::DeadlineExceeded); }
        let mut request = self.client.request(method, format!("{}{path}", endpoint.trim_end_matches('/')))
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(token) = token { request = request.bearer_auth(token); }
        if let Some(body) = body { request = request.header(reqwest::header::CONTENT_TYPE, "application/json").body(body); }
        let exchange = async {
            let mut response = request.send().await.map_err(|_| AgentFailure::ServerModelUnavailable)?;
            if response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok())
                .is_none_or(|v| !v.split(';').next().is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))) {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            if response.content_length().is_some_and(|len| len > MAX_RESPONSE_BYTES as u64) { return Err(AgentFailure::ServerModelInvalidOutput); }
            let status = response.status().as_u16();
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| AgentFailure::ServerModelUnavailable)? {
                if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES { return Err(AgentFailure::ServerModelInvalidOutput); }
                bytes.extend_from_slice(&chunk);
            }
            std::str::from_utf8(&bytes).map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
            Ok((status, bytes))
        };
        tokio::select! {
            _ = cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => Err(AgentFailure::ServerModelTimeout),
            result = exchange => result,
        }
    }
}
pub(crate) fn valid_endpoint(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| url.scheme() == "http" && url.host_str() == Some("127.0.0.1")
        && url.port().is_some_and(|port| port > 0) && url.username().is_empty() && url.password().is_none()
        && url.path() == "/" && url.query().is_none() && url.fragment().is_none())
}
