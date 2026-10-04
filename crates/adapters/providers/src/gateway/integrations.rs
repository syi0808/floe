//! Paired integration operations. OAuth state, provider URLs and secrets remain
//! within the Gateway's authenticated management UI.
use super::{
    credentials::{GatewayConnection, GatewayCredentialStore},
    http::GatewayHttpTransport,
};
use floe_access::{SourcePreviewVerifier, VerifiedGatewayBinding};
use floe_agent_contract::{AgentFailure, BoxFuture};
use floe_connections::*;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;
pub struct GatewayIntegrationAdapter {
    store: GatewayCredentialStore,
    preview: Arc<dyn SourcePreviewVerifier>,
}
impl GatewayIntegrationAdapter {
    pub fn new(store: GatewayCredentialStore, preview: Arc<dyn SourcePreviewVerifier>) -> Self {
        Self { store, preview }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogWire {
    schema_version: u32,
    revision: u64,
    person_id: String,
    device_id: String,
    connectors: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationWire {
    schema_version: u32,
    revision: u64,
    operation_id: Uuid,
    connector_id: String,
    connection_id: String,
    person_id: String,
    device_id: String,
    setup_state: RemoteIntegrationState,
    management_ref: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DisconnectWire {
    schema_version: u32,
    operation_id: Uuid,
    person_id: String,
    device_id: String,
    connection_id: String,
    connector_id: String,
    connection_revision: u64,
    cleanup_state: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeWire {
    schema_version: u32,
    person_id: String,
    device_id: String,
    connection_id: String,
    connection_revision: u64,
    connector_id: String,
    scope: serde_json::Value,
}
impl GatewayIntegrationAdapter {
    async fn connection(
        &self,
        expected: &VerifiedGatewayBinding,
    ) -> Result<GatewayConnection, IntegrationError> {
        let connection = self
            .store
            .load(&expected.person_id, &expected.device_id)
            .await
            .map_err(|error| match error {
                super::credentials::GatewayCredentialError::Conflict => IntegrationError::Conflict,
                super::credentials::GatewayCredentialError::Locked
                | super::credentials::GatewayCredentialError::Unavailable => {
                    IntegrationError::Unavailable
                }
                super::credentials::GatewayCredentialError::Timeout => {
                    IntegrationError::DeadlineExceeded
                }
                super::credentials::GatewayCredentialError::Cancelled => {
                    IntegrationError::Cancelled
                }
                super::credentials::GatewayCredentialError::Indeterminate => {
                    IntegrationError::Unavailable
                }
                super::credentials::GatewayCredentialError::Malformed
                | super::credentials::GatewayCredentialError::Unverified
                | super::credentials::GatewayCredentialError::ForeignIdentity => {
                    IntegrationError::ForeignIdentity
                }
            })?
            // This operation requires its pinned generation. Proven absence
            // cannot recover that authority; transient reads were handled above.
            .ok_or(IntegrationError::ForeignIdentity)?;
        if &connection.binding != expected {
            return Err(IntegrationError::ForeignIdentity);
        }
        Ok(connection)
    }
    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        expected: &VerifiedGatewayBinding,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        scope: &OperationScope,
    ) -> Result<T, IntegrationError> {
        let connection = self.connection(expected).await?;
        let http = GatewayHttpTransport::new().map_err(integration_failure)?;
        let body = body
            .map(|value| serde_json::to_vec(&value).map_err(|_| IntegrationError::InvalidInput))
            .transpose()?;
        let (status, bytes) = http
            .request(
                &connection.endpoint,
                Some(connection.bearer.as_str()),
                method,
                path,
                body,
                scope.deadline(),
                scope.cancellation(),
            )
            .await
            .map_err(integration_failure)?;
        if status != 200 && status != 201 {
            return Err(match status {
                401 | 403 => IntegrationError::ForeignIdentity,
                409 => IntegrationError::Conflict,
                404 => IntegrationError::NotFound,
                400 => IntegrationError::InvalidInput,
                _ => IntegrationError::Unavailable,
            });
        }
        super::json::strict_json_bytes(&bytes, 65536)
            .map_err(|_| IntegrationError::InvalidResponse)?;
        let response =
            serde_json::from_slice(&bytes).map_err(|_| IntegrationError::InvalidResponse)?;
        self.connection(expected).await?;
        Ok(response)
    }
    async fn map_operation(
        &self,
        wire: OperationWire,
        mut reference: IntegrationOperationRef,
        scope: &OperationScope,
    ) -> Result<IntegrationOperation, IntegrationError> {
        if wire.schema_version != 1
            || wire.operation_id != reference.remote_operation_ref
            || wire.person_id != reference.expected.person_id
            || wire.device_id != reference.expected.device_id
            || wire.connector_id != reference.connector_id.as_str()
            || wire.revision == 0
            || wire.revision < reference.remote_revision
            || wire.management_ref != format!("/manage/setup/{}", reference.remote_operation_ref)
        {
            return Err(IntegrationError::InvalidResponse);
        }
        reference.remote_revision = wire.revision;
        let connection_id = floe_context_contract::ConnectionId::try_new(wire.connection_id)
            .map_err(|_| IntegrationError::InvalidResponse)?;
        let connection = self.connection(&reference.expected).await?;
        let management_launch = if matches!(
            wire.setup_state,
            RemoteIntegrationState::AwaitingUser | RemoteIntegrationState::Pending
        ) {
            Some(ValidatedManagementLaunch {
                action_ref: reference.operation_id,
                purpose: LaunchPurpose::AuthorizeIntegration,
                validated_url: format!(
                    "{}{}",
                    connection.endpoint.trim_end_matches('/'),
                    wire.management_ref
                ),
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
            })
        } else {
            None
        };
        let source = if wire.setup_state == RemoteIntegrationState::Connected {
            Some(self.source(&reference, &connection_id, scope).await?)
        } else {
            None
        };
        Ok(IntegrationOperation {
            reference,
            state: wire.setup_state,
            connection_id,
            management_launch,
            source,
        })
    }
    async fn source(
        &self,
        operation: &IntegrationOperationRef,
        connection_id: &floe_context_contract::ConnectionId,
        scope: &OperationScope,
    ) -> Result<RemoteIntegrationSource, IntegrationError> {
        let private = self.connection(&operation.expected).await?;
        let source = crate::control::PreparedServerSource::new(private, self.store.clone());
        let client = super::views::GatewayViewsClient::new(source);
        let mut admitted: Option<floe_access::RemoteViewSourceReference> = None;
        for view in floe_access::source_view_ids(operation.connector_id.as_str()) {
            let resource = floe_context_contract::connection_view_resource(view, connection_id)
                .map_err(|_| IntegrationError::InvalidResponse)?;
            let preview = client
                .view_source_preview(
                    view,
                    operation.connector_id.as_str(),
                    connection_id.as_str(),
                    resource.as_str(),
                    scope.deadline(),
                    scope.cancellation(),
                )
                .await
                .map_err(integration_failure)?;
            let preview = floe_access::SignedSourcePreview {
                descriptor_b64url: preview.descriptor_b64url,
                producer_signature: preview.producer_signature,
                connection_revision: preview.connection_revision,
                producer: super::views::access_producer_identity(&preview.producer),
            };
            let verified = self
                .preview
                .verify(
                    &preview,
                    floe_access::RemotePairingIdentity {
                        person_id: &operation.expected.person_id,
                        client_id: &operation.expected.client_id,
                        device_id: &operation.expected.device_id,
                    },
                    floe_access::RemoteSourceQuery {
                        view_id: view,
                        connector_id: operation.connector_id.as_str(),
                        connection_id: connection_id.as_str(),
                        resource: resource.as_str(),
                    },
                )
                .await
                .map_err(integration_failure)?;
            if admitted.as_ref().is_some_and(|first| {
                first.source_authority != verified.source_authority
                    || first.source_resources != verified.source_resources
                    || first.provider_identity != verified.provider_identity
                    || first.connection_revision != verified.connection_revision
            }) {
                return Err(IntegrationError::ForeignIdentity);
            }
            admitted = Some(verified);
        }
        let admitted = admitted.ok_or(IntegrationError::Unavailable)?;
        let resources = admitted
            .source_resources
            .into_iter()
            .map(|handle| {
                ConnectionResource::new(handle.clone(), handle.as_str().chars().take(200).collect())
                    .map_err(|_| IntegrationError::InvalidResponse)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RemoteIntegrationSource {
            execution_owner_id: floe_context_contract::ExecutionOwnerId::try_new(
                admitted.execution_owner,
            )
            .map_err(|_| IntegrationError::InvalidResponse)?,
            source_authority: admitted.source_authority,
            resources,
            provider_revision: admitted.connection_revision,
        })
    }
}
impl RemoteIntegrationPort for GatewayIntegrationAdapter {
    fn list<'a>(
        &'a self,
        query: IntegrationCatalogQuery,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationCatalog, IntegrationError>> {
        Box::pin(async move {
            let wire: CatalogWire = self
                .request(
                    &query.expected,
                    reqwest::Method::GET,
                    "/v1/connectors",
                    None,
                    scope,
                )
                .await?;
            if wire.schema_version != 1
                || wire.revision == 0
                || wire.person_id != query.expected.person_id
                || wire.device_id != query.expected.device_id
                || wire.connectors.len() > 64
            {
                return Err(IntegrationError::InvalidResponse);
            }
            let mut entries = Vec::new();
            let mut ids = std::collections::BTreeSet::new();
            for value in wire.connectors {
                let id = value["id"]
                    .as_str()
                    .ok_or(IntegrationError::InvalidResponse)?;
                validate_connector(id)?;
                if !ids.insert(id.to_owned()) {
                    return Err(IntegrationError::InvalidResponse);
                }
                let name = value["name"]
                    .as_str()
                    .filter(|name| {
                        !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control)
                    })
                    .ok_or(IntegrationError::InvalidResponse)?;
                let setup_kind = match value["auth_kind"].as_str() {
                    Some("oauth_pkce") => IntegrationSetupKind::BrowserAuthorization,
                    Some("oauth_device") => IntegrationSetupKind::DeviceCode,
                    Some("secret") => IntegrationSetupKind::GatewayManagedSecret,
                    _ => return Err(IntegrationError::InvalidResponse),
                };
                let available = value["available"]
                    .as_bool()
                    .ok_or(IntegrationError::InvalidResponse)?;
                let status = value["status"]
                    .as_str()
                    .ok_or(IntegrationError::InvalidResponse)?;
                let state = match status {
                    "connected" => IntegrationState::Connected,
                    "connecting" => IntegrationState::Connecting,
                    "error" => IntegrationState::Error,
                    "disconnected" if available => IntegrationState::Available,
                    "disconnected" | "unavailable" => IntegrationState::Unavailable,
                    _ => return Err(IntegrationError::InvalidResponse),
                };
                entries.push(IntegrationDescriptor {
                    connector_id: floe_context_contract::ConnectorId::try_new(id)
                        .map_err(|_| IntegrationError::InvalidResponse)?,
                    display_name: name.to_owned(),
                    category: connector_category(id).into(),
                    setup_kind,
                    state,
                    catalog_revision: wire.revision,
                    initial_selection: IntegrationSelection::GatewayManaged,
                });
            }
            Ok(IntegrationCatalog {
                gateway_ref: query.gateway_ref,
                binding: query.expected,
                entries,
            })
        })
    }
    fn begin<'a>(
        &'a self,
        command: BeginIntegration,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationOperation, IntegrationError>> {
        Box::pin(async move {
            validate_connector(command.connector_id.as_str())?;
            if command.operation_id.is_nil() || command.expected_catalog_revision == 0 {
                return Err(IntegrationError::InvalidInput);
            }
            if command.selection != IntegrationSelection::GatewayManaged {
                return Err(IntegrationError::InvalidInput);
            }
            let reference = IntegrationOperationRef {
                operation_id: command.operation_id,
                gateway_ref: command.gateway_ref,
                expected: command.expected.clone(),
                connector_id: command.connector_id.clone(),
                remote_operation_ref: command.operation_id,
                remote_revision: 0,
            };
            let wire=self.request(&command.expected,reqwest::Method::POST,&format!("/v1/connectors/{}/connect",command.connector_id.as_str()),Some(serde_json::json!({"schema_version":1,"operation_id":command.operation_id,"expected_catalog_revision":command.expected_catalog_revision,"scope":{}})),scope).await?;
            self.map_operation(wire, reference, scope).await
        })
    }
    fn observe<'a>(
        &'a self,
        operation: &'a IntegrationOperationRef,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationOperation, IntegrationError>> {
        Box::pin(async move {
            validate_connector(operation.connector_id.as_str())?;
            let wire = self
                .request(
                    &operation.expected,
                    reqwest::Method::GET,
                    &format!(
                        "/v1/connectors/{}/connection-attempts/{}",
                        operation.connector_id.as_str(),
                        operation.remote_operation_ref
                    ),
                    None,
                    scope,
                )
                .await?;
            self.map_operation(wire, operation.clone(), scope).await
        })
    }
    fn cancel<'a>(
        &'a self,
        command: CancelIntegration,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationOperation, IntegrationError>> {
        Box::pin(async move {
            let operation = &command.operation;
            validate_connector(operation.connector_id.as_str())?;
            if command.expected_remote_revision != operation.remote_revision
                || command.expected_remote_revision == 0
            {
                return Err(IntegrationError::Conflict);
            }
            let wire=self.request(&operation.expected,reqwest::Method::POST,&format!("/v1/connectors/{}/connection-attempts/{}/cancel",operation.connector_id.as_str(),operation.remote_operation_ref),Some(serde_json::json!({"schema_version":1,"operation_id":operation.remote_operation_ref,"expected_revision":command.expected_remote_revision})),scope).await?;
            self.map_operation(wire, operation.clone(), scope).await
        })
    }
    fn configure<'a>(
        &'a self,
        command: ConfigureIntegration,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationSnapshot, IntegrationError>> {
        Box::pin(async move {
            validate_connector(command.connector_id.as_str())?;
            let IntegrationSelection::SelectedResources(resources) = command.selection else {
                return Err(IntegrationError::InvalidInput);
            };
            if resources.is_empty()
                || !matches!(
                    command.connector_id.as_str(),
                    "calendar.google" | "calendar.microsoft"
                )
            {
                return Err(IntegrationError::InvalidInput);
            }
            let wire:ScopeWire=self.request(&command.expected,reqwest::Method::PATCH,&format!("/v1/connectors/{}/scope",command.connector_id.as_str()),Some(serde_json::json!({"schema_version":1,"connection_id":command.connection_id,"connection_revision":command.expected_remote_revision,"scope":{"calendar_ids":resources}})),scope).await?;
            if wire.schema_version != 1
                || wire.person_id != command.expected.person_id
                || wire.device_id != command.expected.device_id
                || wire.connection_id != command.connection_id.as_str()
                || wire.connector_id != command.connector_id.as_str()
                || wire.connection_revision <= command.expected_remote_revision
            {
                return Err(IntegrationError::Conflict);
            }
            Ok(IntegrationSnapshot {
                connection_id: command.connection_id,
                revision: wire.connection_revision,
                disconnected: false,
                cleanup_complete: true,
            })
        })
    }
    fn disconnect<'a>(
        &'a self,
        command: DisconnectIntegration,
        scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<IntegrationSnapshot, IntegrationError>> {
        Box::pin(async move {
            validate_connector(command.connector_id.as_str())?;
            let wire:DisconnectWire=self.request(&command.expected,reqwest::Method::POST,&format!("/v1/connectors/{}/disconnect",command.connector_id.as_str()),Some(serde_json::json!({"schema_version":1,"operation_id":command.operation_id,"connection_id":command.connection_id,"connection_revision":command.expected_remote_revision})),scope).await?;
            if wire.schema_version != 1
                || wire.operation_id != command.operation_id
                || wire.person_id != command.expected.person_id
                || wire.device_id != command.expected.device_id
                || wire.connection_id != command.connection_id.as_str()
                || wire.connector_id != command.connector_id.as_str()
                || wire.connection_revision != command.expected_remote_revision
            {
                return Err(IntegrationError::Conflict);
            }
            if !matches!(wire.cleanup_state.as_str(), "pending" | "completed") {
                return Err(IntegrationError::InvalidResponse);
            }
            Ok(IntegrationSnapshot {
                connection_id: command.connection_id,
                revision: wire.connection_revision,
                disconnected: true,
                cleanup_complete: wire.cleanup_state == "completed",
            })
        })
    }
    fn management_launch<'a>(
        &'a self,
        request: ManagementLaunchRequest,
        _scope: &'a OperationScope,
    ) -> BoxFuture<'a, Result<ValidatedManagementLaunch, IntegrationError>> {
        Box::pin(async move {
            if request.operation_id.is_nil()
                || request.purpose != LaunchPurpose::ManageGateway
                || request.expected_binding_generation != request.expected.credential_generation
            {
                return Err(IntegrationError::InvalidInput);
            }
            let connection = self.connection(&request.expected).await?;
            Ok(ValidatedManagementLaunch {
                action_ref: request.operation_id,
                purpose: LaunchPurpose::ManageGateway,
                validated_url: format!("{}/manage", connection.endpoint.trim_end_matches('/')),
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
            })
        })
    }
}
fn validate_connector(id: &str) -> Result<(), IntegrationError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        Err(IntegrationError::InvalidInput)
    } else {
        Ok(())
    }
}
fn connector_category(id: &str) -> &'static str {
    if id.starts_with("calendar.") {
        "calendar"
    } else if matches!(id, "gmail" | "microsoft.mail") {
        "mail"
    } else if id.starts_with("home_assistant.") {
        "home"
    } else {
        "work"
    }
}
fn integration_failure(error: AgentFailure) -> IntegrationError {
    match error {
        AgentFailure::Cancelled => IntegrationError::Cancelled,
        AgentFailure::DeadlineExceeded | AgentFailure::ServerModelTimeout => {
            IntegrationError::DeadlineExceeded
        }
        AgentFailure::PolicyDenied | AgentFailure::CredentialExpired => {
            IntegrationError::ForeignIdentity
        }
        AgentFailure::Conflict => IntegrationError::Conflict,
        _ => IntegrationError::Unavailable,
    }
}
