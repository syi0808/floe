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

/// This host's saved local-server credential, as Inference's store port.
#[derive(Clone, Copy, Debug, Default)]
pub struct SavedServerConnectionStore;

impl floe_inference::SavedConnectionStore for SavedServerConnectionStore {
    fn load(&self) -> Result<Option<SavedServerConnection>, AgentFailure> {
        load_saved_connection()
    }
}

/// A prepared loopback server source connection: endpoint, credential and the
/// verified pairing identity it is bound to.
///
/// This is transport state, not a model route. It carries no model purpose,
/// placement, recipient or consent: source reads and catalog observation run
/// under per-call grant authorization instead. The bearer never leaves the
/// adapter; callers outside this crate only see the opaque client and the
/// non-secret pairing identity.
#[derive(Clone, Eq, PartialEq)]
pub struct PreparedServerSource {
    base_url: String,
    bearer_token: String,
    client_id: String,
    person_id: String,
    device_id: String,
}

impl std::fmt::Debug for PreparedServerSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedServerSource")
            .field("base_url", &self.base_url)
            // The credential is named but never rendered, matching RemoteRoute.
            .field("bearer_token", &"[REDACTED]")
            .field("client_id", &self.client_id)
            .field("person_id", &self.person_id)
            .field("device_id", &self.device_id)
            .finish()
    }
}

fn valid_source_endpoint(base_url: &str) -> bool {
    let Some(rest) = base_url.strip_prefix("http://127.0.0.1:") else {
        return false;
    };
    // The stored form keeps a single trailing slash or none; either way the
    // authority is a loopback port and nothing else.
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    !rest.is_empty()
        && rest.bytes().all(|byte| byte.is_ascii_digit())
        && rest.parse::<u16>().is_ok_and(|port| port > 0)
}

fn valid_source_token(token: &str) -> bool {
    token.len() >= 32
        && token.len() <= 256
        && token
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
}

fn valid_client_id(client_id: &str) -> bool {
    !client_id.is_empty()
        && client_id.len() <= 128
        && client_id.trim() == client_id
        && !client_id.chars().any(char::is_control)
}

impl PreparedServerSource {
    fn from_validated(
        base_url: String,
        bearer_token: String,
        client_id: String,
        person_id: String,
        device_id: String,
    ) -> Result<Self, AgentFailure> {
        if !valid_source_endpoint(&base_url)
            || !valid_source_token(&bearer_token)
            || !valid_client_id(&client_id)
            || person_id.trim().is_empty()
            || person_id.trim() != person_id
            || device_id.trim().is_empty()
            || device_id.trim() != device_id
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self {
            base_url,
            bearer_token,
            client_id,
            person_id,
            device_id,
        })
    }

    /// Bind a saved server connection to the verified caller for source use.
    ///
    /// The stored person/device must match the caller exactly; anything else
    /// fails closed instead of downgrading. Recorded model consent is not
    /// carried: it is irrelevant to source transport.
    pub fn admit(
        saved: SavedServerConnection,
        person_id: &str,
        device_id: &str,
    ) -> Result<Self, AgentFailure> {
        if saved.person_id != person_id || saved.device_id != device_id {
            return Err(AgentFailure::PolicyDenied);
        }
        Self::from_validated(
            saved.base_url,
            saved.token,
            saved.client_id,
            saved.person_id,
            saved.device_id,
        )
    }

    /// Prepare a source from route-supplied transport parts.
    ///
    /// Only the outer remote authority/pairing compatibility path supplies its
    /// own route; it keeps owning the caller binding through Access. Shape is
    /// validated here exactly as for saved connections.
    pub fn from_parts(
        base_url: &str,
        bearer_token: &str,
        client_id: &str,
        person_id: &str,
        device_id: &str,
    ) -> Result<Self, AgentFailure> {
        Self::from_validated(
            base_url.to_owned(),
            bearer_token.to_owned(),
            client_id.to_owned(),
            person_id.to_owned(),
            device_id.to_owned(),
        )
    }

    /// Non-secret pairing identity, for the reader and grant checks that quote it.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn person_id(&self) -> &str {
        &self.person_id
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(crate) fn bearer_token(&self) -> &str {
        &self.bearer_token
    }
}

pub fn load_saved_connection() -> Result<Option<SavedServerConnection>, AgentFailure> {
    // No test seam: production credential lookup is unconditional. Tests that
    // need a deterministically absent credential inject a fixed store through
    // the turn request instead of reading this slot at all.
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

#[cfg(test)]
mod tests {
    use super::*;

    const PERSON: &str = "00000000-0000-4000-8000-000000000001";
    const DEVICE: &str = "local-device";

    fn saved() -> SavedServerConnection {
        SavedServerConnection {
            base_url: "http://127.0.0.1:8431".into(),
            token: "source_token_value_that_is_long_enough".into(),
            client_id: "paired-client".into(),
            person_id: PERSON.into(),
            device_id: DEVICE.into(),
            allow_external: false,
            external_recipients: vec![],
        }
    }

    #[test]
    fn prepared_source_binds_verified_caller_and_redacts_the_credential() {
        let prepared = PreparedServerSource::admit(saved(), PERSON, DEVICE).unwrap();
        assert_eq!(prepared.client_id(), "paired-client");
        assert_eq!(prepared.person_id(), PERSON);
        assert_eq!(prepared.device_id(), DEVICE);
        let rendered = format!("{prepared:?}");
        assert!(!rendered.contains("source_token_value_that_is_long_enough"));
        assert!(rendered.contains("[REDACTED]"));

        // Recorded model consent is accepted as stored input but never
        // carried: the prepared source has no consent state at all.
        let mut consented = saved();
        consented.allow_external = true;
        consented.external_recipients = vec!["partner.example".into()];
        assert!(PreparedServerSource::admit(consented, PERSON, DEVICE).is_ok());
    }

    #[test]
    fn prepared_source_rejects_foreign_pairing_and_malformed_transport() {
        assert_eq!(
            PreparedServerSource::admit(saved(), PERSON, "other-device"),
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(
            PreparedServerSource::admit(
                saved(),
                "00000000-0000-4000-8000-000000000002",
                DEVICE
            ),
            Err(AgentFailure::PolicyDenied)
        );

        for invalid in [
            "https://127.0.0.1:8431",
            "http://localhost:8431",
            "http://127.0.0.1:8431/path",
            "http://192.168.1.2:8431",
            "http://127.0.0.1",
            "http://127.0.0.1:0",
            "http://127.0.0.1:8431?query=true",
        ] {
            let mut candidate = saved();
            candidate.base_url = invalid.into();
            assert_eq!(
                PreparedServerSource::admit(candidate, PERSON, DEVICE),
                Err(AgentFailure::InvalidInput),
                "{invalid}"
            );
        }
        for token in ["short".to_owned(), "x".repeat(257), " ".repeat(32)] {
            let mut candidate = saved();
            candidate.token = token;
            assert_eq!(
                PreparedServerSource::admit(candidate, PERSON, DEVICE),
                Err(AgentFailure::InvalidInput)
            );
        }
        for client_id in ["", " padded ", "has control\n"] {
            let mut candidate = saved();
            candidate.client_id = client_id.into();
            assert_eq!(
                PreparedServerSource::admit(candidate, PERSON, DEVICE),
                Err(AgentFailure::InvalidInput),
                "{client_id:?}"
            );
        }
    }

    #[test]
    fn route_supplied_parts_validate_shape_like_saved_connections() {
        let prepared = PreparedServerSource::from_parts(
            "http://127.0.0.1:8431/",
            &"route_token_value_that_is_long_enough".to_string(),
            "route-client",
            PERSON,
            DEVICE,
        )
        .unwrap();
        assert_eq!(prepared.client_id(), "route-client");
        assert!(PreparedServerSource::from_parts(
            "http://not-loopback.invalid",
            &"route_token_value_that_is_long_enough".to_string(),
            "route-client",
            PERSON,
            DEVICE,
        )
        .is_err());
    }
}
