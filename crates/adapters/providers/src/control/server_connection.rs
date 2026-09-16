//! Reads the saved local-server credential from the OS key store.
//!
//! The adapter only decodes the stored bytes; admission against the verified
//! principal belongs to Inference.

use floe_agent_contract::AgentFailure;
use floe_inference::SavedServerConnection;
use serde::Deserialize;

const SERVER_CREDENTIAL_SERVICE: &str = "app.floe.local-server";
const SERVER_CREDENTIAL_ACCOUNT: &str = "connection-v1";
const MAX_CREDENTIAL_BYTES: usize = 4096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredServerConnection {
    base_url: String,
    token: String,
    client_id: String,
    person_id: String,
    device_id: String,
    allow_external: bool,
    external_recipients: Vec<String>,
}

pub fn load_saved_connection() -> Result<Option<SavedServerConnection>, AgentFailure> {
    let secret = floe_native::read_generic_password(
        SERVER_CREDENTIAL_SERVICE,
        SERVER_CREDENTIAL_ACCOUNT,
        MAX_CREDENTIAL_BYTES,
    )
    .map_err(|error| match error {
        floe_native::KeychainError::TooLarge => AgentFailure::PolicyDenied,
        _ => AgentFailure::ServerModelUnavailable,
    })?;
    let Some(secret) = secret else {
        return Ok(None);
    };
    let stored: StoredServerConnection =
        serde_json::from_slice(&secret).map_err(|_| AgentFailure::PolicyDenied)?;
    Ok(Some(SavedServerConnection {
        base_url: stored.base_url,
        token: stored.token,
        client_id: stored.client_id,
        person_id: stored.person_id,
        device_id: stored.device_id,
        allow_external: stored.allow_external,
        external_recipients: stored.external_recipients,
    }))
}
