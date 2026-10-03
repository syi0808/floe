use crate::control::PreparedServerSource;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use floe_access::{
    AuthorizationSigner, RemoteProducerIdentity, RemoteViewAuthorizationExpectation,
};
use floe_agent_contract::AgentFailure;
use serde::{Deserialize, Serialize};
#[derive(Clone)]
pub struct GatewayViewsClient {
    source: PreparedServerSource,
}
fn valid_connection_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProducerIdentityResponse {
    pub schema_version: u32,
    pub instance_id: String,
    pub execution_owner: String,
    pub audience: String,
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewSourcePreviewResponse {
    pub descriptor_b64url: String,
    pub producer_signature: String,
    #[serde(flatten)]
    pub producer: ProducerIdentityResponse,
    pub connection_revision: u64,
    pub source_resources: Vec<String>,
    pub expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewChallengeResponse {
    pub schema_version: u32,
    pub operation: String,
    pub challenge_id: String,
    pub challenge_b64url: String,
    pub producer_signature: String,
    pub producer: ProducerIdentityResponse,
    pub expires: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewChallengeParts {
    pub v: u32,
    pub operation: String,
    pub challenge_id: String,
    pub nonce: String,
    pub key_id: String,
    pub person_id: String,
    pub client_id: String,
    pub device_id: String,
    pub audience: String,
    pub purpose: String,
    pub consumer: String,
    pub source: RemoteViewSourceParts,
    pub grant: RemoteViewGrantParts,
    pub resources: Vec<String>,
    pub query_sha256: String,
    pub max_items: u32,
    pub max_bytes: u32,
    pub result_sha256: String,
    pub admission_id: String,
    pub issued_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewSourceParts {
    pub connector_id: String,
    pub connection_id: String,
    pub execution_owner: String,
    pub incarnation: String,
    pub epoch: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewGrantParts {
    pub id: String,
    pub incarnation: String,
    pub epoch: u64,
}

pub fn parse_remote_view_challenge(
    challenge_b64url: &str,
) -> Result<RemoteViewChallengeParts, AgentFailure> {
    if challenge_b64url.is_empty() || challenge_b64url.len() > 96 * 1024 {
        return Err(AgentFailure::InvalidInput);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(challenge_b64url)
        .map_err(|_| AgentFailure::InvalidInput)?;
    if bytes.is_empty()
        || bytes.len() > 64 * 1024
        || URL_SAFE_NO_PAD.encode(&bytes) != challenge_b64url
    {
        return Err(AgentFailure::InvalidInput);
    }
    super::json::strict_json_bytes(&bytes, 65536)?;
    serde_json::from_slice(&bytes).map_err(|_| AgentFailure::InvalidInput)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Serialize)]
struct RemoteViewProofRequest<'value> {
    schema_version: u32,
    proof: RemoteViewProof<'value>,
}

#[derive(Serialize)]
struct RemoteViewProof<'value> {
    challenge_id: &'value str,
    key_id: &'value str,
    signature: &'value str,
}

#[derive(Clone, Debug)]
pub struct RemoteViewAuthorizationRequest<'value> {
    pub path: &'value str,
    pub connector_id: &'value str,
    pub connection_id: &'value str,
    pub connection_revision: u64,
    pub resource: &'value str,
    pub grant_id: &'value str,
    pub grant_incarnation: &'value str,
    pub grant_epoch: u64,
    pub purpose: &'value str,
    pub consumer: &'value str,
    pub max_items: u32,
    pub max_bytes: u32,
    pub query: serde_json::Value,
}

impl GatewayViewsClient {
    pub(crate) fn new(source: PreparedServerSource) -> Self {
        Self { source }
    }
    pub async fn producer_identity(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<ProducerIdentityResponse, AgentFailure> {
        self.request(
            "GET",
            "/v1/authority/producer",
            None,
            deadline,
            cancellation,
        )
        .await
    }
    pub async fn view_source_preview(
        &self,
        view_id: &str,
        connector_id: &str,
        connection_id: &str,
        resource: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<RemoteViewSourcePreviewResponse, AgentFailure> {
        if !matches!(
            view_id,
            "mail.communication" | "work.context" | "life.logistics" | "calendar.timeline"
        ) || connector_id.is_empty()
            || !valid_connection_id(connection_id)
            || resource.is_empty()
            || resource.len() > 256
            || resource.chars().any(char::is_control)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let body = serde_json::json!({
            "connector_id": connector_id,
            "connection_id": connection_id,
            "resource": resource,
        });
        let response: RemoteViewSourcePreviewResponse = self
            .request(
                "POST",
                &format!("/v1/views/{view_id}/source-preview"),
                Some(body),
                deadline,
                cancellation,
            )
            .await?;
        if response.producer.schema_version != 1
            || response.descriptor_b64url.is_empty()
            || response.producer_signature.is_empty()
            || response.connection_revision == 0
            || response.source_resources.is_empty()
            || response
                .source_resources
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || response
                .source_resources
                .iter()
                .any(|resource| floe_access::ResourceHandle::try_new(resource.as_str()).is_err())
            || response.expires_at_unix_ms <= 0
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        Ok(response)
    }
    pub async fn begin_view_admission(
        &self,
        request: RemoteViewAuthorizationRequest<'_>,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<RemoteViewChallengeResponse, AgentFailure> {
        if !matches!(
            request.path,
            "/v1/views/mail.communication/admit"
                | "/v1/views/work.context/admit"
                | "/v1/views/life.logistics/admit"
                | "/v1/views/calendar.timeline/admit"
        ) || request.connector_id.is_empty()
            || !valid_connection_id(request.connection_id)
            || request.connection_revision == 0
            || request.resource.is_empty()
            || request.resource.len() > 256
            || request.grant_id.is_empty()
            || request.grant_incarnation.is_empty()
            || request.grant_epoch == 0
            || request.purpose.is_empty()
            || request.consumer.is_empty()
            || request.max_items == 0
            || request.max_items > 128
            || request.max_bytes == 0
            || request.max_bytes > 1 << 20
        {
            return Err(AgentFailure::InvalidInput);
        }
        let encoded = serde_json::json!({
            "schema_version": 1,
            "connector_id": request.connector_id,
            "connection_id": request.connection_id,
            "connection_revision": request.connection_revision,
            "resources": [request.resource],
            "grant": {
                "id": request.grant_id,
                "incarnation": request.grant_incarnation,
                "epoch": request.grant_epoch,
            },
            "purpose": request.purpose,
            "consumer": request.consumer,
            "max_items": request.max_items,
            "max_bytes": request.max_bytes,
            "query": request.query,
        });
        self.request("POST", request.path, Some(encoded), deadline, cancellation)
            .await
    }
    pub async fn read_view_admission<Keys: AuthorizationSigner + ?Sized>(
        &self,
        keys: &Keys,
        expected: &RemoteViewAuthorizationExpectation,
        challenge: &RemoteViewChallengeResponse,
        path: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<RemoteViewChallengeResponse, AgentFailure> {
        if !path.ends_with("/read") || challenge.operation != "admission" {
            return Err(AgentFailure::InvalidInput);
        }
        let signature = keys
            .sign_authorization(super::proof::authorization_command(
                expected,
                &challenge.challenge_b64url,
                &challenge.producer_signature,
                access_producer_identity(&challenge.producer),
            )?)
            .await?;
        let body = RemoteViewProofRequest {
            schema_version: 1,
            proof: RemoteViewProof {
                challenge_id: &challenge.challenge_id,
                key_id: &signature.key_id,
                signature: &signature.signature,
            },
        };
        self.request(
            "POST",
            path,
            Some(serde_json::to_value(body).map_err(|_| AgentFailure::InvalidInput)?),
            deadline,
            cancellation,
        )
        .await
    }
    pub async fn release_view<Keys: AuthorizationSigner + ?Sized>(
        &self,
        keys: &Keys,
        expected: &RemoteViewAuthorizationExpectation,
        challenge: &RemoteViewChallengeResponse,
        path: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<serde_json::Value, AgentFailure> {
        if !path.ends_with("/release") || challenge.operation != "release" {
            return Err(AgentFailure::InvalidInput);
        }
        let signature = keys
            .sign_authorization(super::proof::authorization_command(
                expected,
                &challenge.challenge_b64url,
                &challenge.producer_signature,
                access_producer_identity(&challenge.producer),
            )?)
            .await?;
        let body = RemoteViewProofRequest {
            schema_version: 1,
            proof: RemoteViewProof {
                challenge_id: &challenge.challenge_id,
                key_id: &signature.key_id,
                signature: &signature.signature,
            },
        };
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ViewResponse {
            schema_version: u32,
            view: Box<serde_json::value::RawValue>,
        }
        let response: ViewResponse = self
            .request(
                "POST",
                path,
                Some(serde_json::to_value(body).map_err(|_| AgentFailure::InvalidInput)?),
                deadline,
                cancellation,
            )
            .await?;
        if response.schema_version != 1 {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        if sha256_hex(response.view.get().as_bytes()) != expected.result_sha256 {
            return Err(AgentFailure::PolicyDenied);
        }
        serde_json::from_str(response.view.get()).map_err(|_| AgentFailure::CapabilityUnavailable)
    }
    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<T, AgentFailure> {
        self.source.revalidate().await?;
        let http = super::http::GatewayHttpTransport::new()?;
        let method = match method {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            _ => return Err(AgentFailure::InvalidInput),
        };
        let body = body
            .map(|value| serde_json::to_vec(&value).map_err(|_| AgentFailure::InvalidInput))
            .transpose()?;
        let (status, bytes) = http
            .request(
                self.source.base_url(),
                Some(self.source.bearer_token()),
                method,
                path,
                body,
                deadline,
                cancellation,
            )
            .await?;
        if status != 200 {
            return Err(match status {
                400 => AgentFailure::InvalidInput,
                401 => AgentFailure::CredentialExpired,
                403 => AgentFailure::PolicyDenied,
                409 => AgentFailure::Conflict,
                _ => AgentFailure::CapabilityUnavailable,
            });
        }
        super::json::strict_json_bytes(&bytes, 65536)?;
        let value =
            serde_json::from_slice(&bytes).map_err(|_| AgentFailure::CapabilityUnavailable)?;
        self.source.revalidate().await?;
        Ok(value)
    }
}
pub fn access_producer_identity(identity: &ProducerIdentityResponse) -> RemoteProducerIdentity {
    RemoteProducerIdentity {
        schema_version: identity.schema_version,
        instance_id: identity.instance_id.clone(),
        execution_owner: identity.execution_owner.clone(),
        audience: identity.audience.clone(),
        key_id: identity.key_id.clone(),
        public_key: identity.public_key.clone(),
        fingerprint: identity.fingerprint.clone(),
    }
}
