//! Admission of a stored local-server connection into an inference route.
//!
//! The stored credential is only usable by the exact verified principal that
//! saved it, and external recipients must match recorded consent exactly.

use floe_agent_contract::AgentFailure;

use crate::RemoteModelConnection;

/// A saved connection exactly as it was persisted, before admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedServerConnection {
    pub base_url: String,
    pub token: String,
    pub client_id: String,
    pub person_id: String,
    pub device_id: String,
    pub allow_external: bool,
    pub external_recipients: Vec<String>,
}

/// Bind a saved connection to the verified caller and its recorded consent.
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
        || value.external_recipients.len() > 16
        || value.external_recipients.iter().any(|recipient| {
            recipient.trim() != recipient
                || recipient.is_empty()
                || recipient.len() > 253
                || recipient.chars().any(char::is_control)
        })
        || value
            .external_recipients
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != value.external_recipients.len()
        || value.allow_external != !value.external_recipients.is_empty()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(RemoteModelConnection {
        base_url: value.base_url,
        bearer_token: value.token,
        client_id: value.client_id,
        person_id: value.person_id,
        device_id: value.device_id,
        allow_external: value.allow_external,
        external_recipients: value.external_recipients,
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
            allow_external: false,
            external_recipients: vec![],
        }
    }

    #[test]
    fn saved_connection_is_bound_to_verified_host_identity_and_consent() {
        assert!(admit_saved_connection(saved(), PERSON, DEVICE).is_ok());
        let mut wrong_identity = saved();
        wrong_identity.device_id = "other-device".into();
        assert_eq!(
            admit_saved_connection(wrong_identity, PERSON, DEVICE).err(),
            Some(AgentFailure::PolicyDenied)
        );
        let mut inconsistent_consent = saved();
        inconsistent_consent.allow_external = true;
        assert_eq!(
            admit_saved_connection(inconsistent_consent, PERSON, DEVICE).err(),
            Some(AgentFailure::PolicyDenied)
        );
    }
}
