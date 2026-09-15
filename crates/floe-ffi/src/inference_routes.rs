use floe_agent::AgentFailure;
use floe_app::CallerContext;
use floe_infra::remote_model::{RemoteModelConnection, resolve_remote_model_route};
use floe_protocol::AgentRemoteRouteDto;
use serde::Deserialize;
use tokio::runtime::Runtime;

const SERVER_CREDENTIAL_SERVICE: &str = "app.floe.local-server";
const SERVER_CREDENTIAL_ACCOUNT: &str = "connection-v1";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedServerConnection {
    base_url: String,
    token: String,
    client_id: String,
    person_id: String,
    device_id: String,
    allow_external: bool,
    external_recipients: Vec<String>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct HostInferenceRoutes;

impl HostInferenceRoutes {
    pub(crate) fn resolve(
        &self,
        runtime: &Runtime,
        caller: &CallerContext,
    ) -> Result<Option<AgentRemoteRouteDto>, AgentFailure> {
        let Some(connection) = load_saved_connection()? else {
            return Ok(None);
        };
        let connection = validate_saved_connection(connection, caller)?;
        runtime
            .block_on(resolve_remote_model_route(&connection))
            .map(Some)
    }
}

fn validate_saved_connection(
    value: SavedServerConnection,
    caller: &CallerContext,
) -> Result<RemoteModelConnection, AgentFailure> {
    if value.person_id != caller.person_id().to_string()
        || value.device_id != caller.device_id()
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

#[cfg(target_os = "macos")]
fn load_saved_connection() -> Result<Option<SavedServerConnection>, AgentFailure> {
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};
    use security_framework_sys::base::errSecItemNotFound;
    use zeroize::Zeroizing;

    let mut query = ItemSearchOptions::new();
    query
        .class(ItemClass::generic_password())
        .service(SERVER_CREDENTIAL_SERVICE)
        .account(SERVER_CREDENTIAL_ACCOUNT)
        .load_data(true)
        .skip_authenticated_items(true);
    let results = match query.search() {
        Ok(results) => results,
        Err(error) if error.code() == errSecItemNotFound => return Ok(None),
        Err(_) => return Err(AgentFailure::ServerModelUnavailable),
    };
    let mut results = results.into_iter();
    let secret = match (results.next(), results.next()) {
        (Some(SearchResult::Data(secret)), None) => Zeroizing::new(secret),
        _ => return Err(AgentFailure::ServerModelUnavailable),
    };
    if secret.len() > 4096 {
        return Err(AgentFailure::PolicyDenied);
    }
    serde_json::from_slice(&secret)
        .map(Some)
        .map_err(|_| AgentFailure::PolicyDenied)
}

#[cfg(not(target_os = "macos"))]
fn load_saved_connection() -> Result<Option<SavedServerConnection>, AgentFailure> {
    let _ = (SERVER_CREDENTIAL_SERVICE, SERVER_CREDENTIAL_ACCOUNT);
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_app::LocalIdentityClaim;

    fn caller() -> CallerContext {
        let host = floe_app::AppHost::bootstrap_claim(
            TestServices,
            LocalIdentityClaim {
                person_id: uuid::Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap(),
                device_id: "local-device".into(),
            },
        )
        .unwrap();
        host.request(uuid::Uuid::new_v4()).unwrap().caller().clone()
    }

    struct TestServices;

    impl floe_app::HostServices for TestServices {
        fn shutdown(&self) -> Result<(), floe_app::HostError> {
            Ok(())
        }
    }

    fn saved() -> SavedServerConnection {
        SavedServerConnection {
            base_url: "http://127.0.0.1:8431".into(),
            token: "a".repeat(32),
            client_id: "paired-client".into(),
            person_id: "00000000-0000-4000-8000-000000000001".into(),
            device_id: "local-device".into(),
            allow_external: false,
            external_recipients: vec![],
        }
    }

    #[test]
    fn saved_connection_is_bound_to_verified_host_identity_and_consent() {
        assert!(validate_saved_connection(saved(), &caller()).is_ok());
        let mut wrong_identity = saved();
        wrong_identity.device_id = "other-device".into();
        assert_eq!(
            validate_saved_connection(wrong_identity, &caller()).err(),
            Some(AgentFailure::PolicyDenied)
        );
        let mut inconsistent_consent = saved();
        inconsistent_consent.allow_external = true;
        assert_eq!(
            validate_saved_connection(inconsistent_consent, &caller()).err(),
            Some(AgentFailure::PolicyDenied)
        );
    }
}
