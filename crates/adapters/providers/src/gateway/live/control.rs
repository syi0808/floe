use super::wire;
use crate::gateway::{
    credentials::{GatewayConnection, GatewayCredentialStore},
    http::GatewayHttpTransport,
    inference_wire,
};
use floe_agent_contract::AgentFailure;
use floe_execution::Cancellation;
use reqwest::Method;
use serde::Serialize;
use std::sync::Arc;
use tokio::time::Instant;

pub(super) struct Control {
    pub store: GatewayCredentialStore,
    pub connection: GatewayConnection,
    pub http: GatewayHttpTransport,
}
impl Control {
    pub async fn current(&self) -> Result<(), AgentFailure> {
        let current = self
            .store
            .load(
                &self.connection.binding.person_id,
                &self.connection.binding.device_id,
            )
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .ok_or(AgentFailure::PolicyDenied)?;
        if current.binding != self.connection.binding
            || current.endpoint != self.connection.endpoint
            || current.bearer.as_str() != self.connection.bearer.as_str()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
        deadline: Instant,
        cancel: &Cancellation,
    ) -> Result<(u16, Vec<u8>), AgentFailure> {
        floe_execution::tasks::run_bounded(
            async {
                self.current().await?;
                let out = self
                    .http
                    .request_with_limits(
                        &self.connection.endpoint,
                        Some(self.connection.bearer.as_str()),
                        method,
                        path,
                        body,
                        deadline,
                        cancel,
                        wire::MAX_REQUEST,
                        wire::MAX_RESPONSE,
                    )
                    .await?;
                self.current().await?;
                Ok(out)
            },
            deadline,
            cancel,
        )
        .await
    }
    pub async fn get(
        &self,
        path: &str,
        deadline: Instant,
        cancel: &Cancellation,
    ) -> Result<(u16, Vec<u8>), AgentFailure> {
        self.request(Method::GET, path, None, deadline, cancel)
            .await
    }
    pub async fn post(
        &self,
        path: &str,
        value: &impl Serialize,
        deadline: Instant,
        cancel: &Cancellation,
    ) -> Result<Vec<u8>, AgentFailure> {
        let body = serde_json::to_vec(value).map_err(|_| AgentFailure::InvalidInput)?;
        let (status, bytes) = self
            .request(Method::POST, path, Some(body), deadline, cancel)
            .await?;
        success(status, &bytes)?;
        Ok(bytes)
    }
}
pub(super) fn success(status: u16, bytes: &[u8]) -> Result<(), AgentFailure> {
    if status == 200 {
        return Ok(());
    }
    Err(inference_wire::failure(status, bytes)
        .map(|pair| pair.1)
        .unwrap_or(AgentFailure::ServerModelInvalidOutput))
}

// Cleanup cannot create/recreate calls. No runtime or network availability is
// assumed during Drop; unused tickets also have a bounded server expiry.
pub(super) struct TicketLease {
    pub control: Arc<Control>,
    pub id: String,
}
impl Drop for TicketLease {
    fn drop(&mut self) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let control = self.control.clone();
        let ticket = self.id.clone();
        runtime.spawn(async move {
            let cancel = Cancellation::new();
            let deadline = Instant::now() + std::time::Duration::from_secs(2);
            let _ = control
                .post(
                    "/v1/live/release",
                    &serde_json::json!({"schema_version":1,"ticket_id":ticket}),
                    deadline,
                    &cancel,
                )
                .await;
        });
    }
}
