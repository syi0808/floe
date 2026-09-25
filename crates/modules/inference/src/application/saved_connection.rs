//! Admission of a stored local-server connection into an inference route.
//!
//! The stored credential is only usable by the exact verified principal that
//! saved it. Recipient approval belongs to Access contextual consent.

use floe_agent_contract::AgentFailure;

#[derive(Clone, Eq, PartialEq)]
pub struct RemoteModelConnection {
    pub base_url: String,
    pub bearer_token: String,
    pub client_id: String,
    pub person_id: String,
    pub device_id: String,
}

/// A saved connection exactly as it was persisted, before admission.
#[derive(Clone, Eq, PartialEq)]
pub struct SavedServerConnection {
    pub base_url: String,
    pub token: String,
    pub client_id: String,
    pub person_id: String,
    pub device_id: String,
}

impl std::fmt::Debug for SavedServerConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SavedServerConnection")
            .field("base_url", &self.base_url)
            // Credentials are named but never rendered.
            .field("token", &"[REDACTED]")
            .field("client_id", &self.client_id)
            .field("person_id", &self.person_id)
            .field("device_id", &self.device_id)
            .finish()
    }
}

/// Bind a saved connection to the verified caller.
pub fn admit_saved_connection(
    value: SavedServerConnection,
    person_id: &str,
    device_id: &str,
) -> Result<RemoteModelConnection, AgentFailure> {
    if value.person_id != person_id
        || value.device_id != device_id
        || value.client_id.trim() != value.client_id
        || value.client_id.is_empty()
        || value.client_id.len() > 128
        || value.client_id.chars().any(char::is_control)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(RemoteModelConnection {
        base_url: value.base_url,
        bearer_token: value.token,
        client_id: value.client_id,
        person_id: value.person_id,
        device_id: value.device_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERSON: &str = "00000000-0000-4000-8000-000000000001";
    const DEVICE: &str = "local-device";

    fn saved() -> SavedServerConnection {
        SavedServerConnection {
            base_url: "http://127.0.0.1:8431".into(),
            token: "a".repeat(32),
            client_id: "paired-client".into(),
            person_id: PERSON.into(),
            device_id: DEVICE.into(),
        }
    }

    #[test]
    fn saved_connection_is_bound_to_verified_host_identity() {
        assert!(admit_saved_connection(saved(), PERSON, DEVICE).is_ok());
        let mut wrong_identity = saved();
        wrong_identity.device_id = "other-device".into();
        assert_eq!(
            admit_saved_connection(wrong_identity, PERSON, DEVICE).err(),
            Some(AgentFailure::PolicyDenied)
        );
    }
}
