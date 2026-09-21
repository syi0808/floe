use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use floe_access::{
    RemoteAuthorizationKeys, RemoteCalendarAuthorizationExpectation, RemoteEnrollmentSignature,
    RemoteOwnerPublicKey, RemoteProducerIdentity,
};
use floe_agent_contract::AgentFailure;
use floe_connections::{
    PairingConfirmation, PairingConfirmationRequest, PairingIssuer, PairingStatus,
    PairingStatusRequest, ProducerIdentity, RemoteControl,
};
#[cfg(test)]
use floe_inference::RemoteRoute;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};

const MAX_RESPONSE_BYTES: usize = 64 * 1024;

fn valid_connection_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|identifier| {
        identifier.get_version_num() == 4
            && identifier
                .hyphenated()
                .to_string()
                .eq_ignore_ascii_case(value)
    })
}

#[derive(Clone)]
pub struct RemoteAuthorizationClient {
    base_url: String,
    bearer_token: String,
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
pub struct CalendarSourcePreviewResponse {
    pub descriptor_b64url: String,
    pub producer_signature: String,
    #[serde(flatten)]
    pub producer: ProducerIdentityResponse,
    pub expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewSourcePreviewResponse {
    pub descriptor_b64url: String,
    pub producer_signature: String,
    #[serde(flatten)]
    pub producer: ProducerIdentityResponse,
    pub connection_revision: u64,
    pub expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentResponse {
    pub schema_version: u32,
    pub instance_id: String,
    pub execution_owner: String,
    pub audience: String,
    pub producer_key_id: String,
    pub producer_public_key: String,
    pub producer_fingerprint: String,
    pub enrollment_id: String,
    pub challenge_id: String,
    pub key_id: String,
    pub fingerprint: String,
    pub challenge_b64url: String,
    pub producer_signature: String,
    pub expires: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentStatusResponse {
    pub enrollment_id: String,
    pub key_id: String,
    pub fingerprint: String,
    pub local_confirmed: bool,
    pub admin_approved: bool,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PairingConfirmationResponse {
    pub schema_version: u32,
    pub pairing_id: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PairingIssuerResponse {
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PairingStartResponse {
    pub schema_version: u32,
    pub pairing_id: String,
    pub code: String,
    pub proof: String,
    pub expires_at_unix_ms: i64,
    pub person_id: String,
    pub device_id: String,
    pub producer: ProducerIdentityResponse,
    pub issuer: PairingIssuerResponse,
    pub challenge_id: String,
    pub challenge_b64url: String,
    pub producer_signature: String,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PairingStatusResponse {
    pub schema_version: u32,
    pub pairing_id: String,
    pub status: String,
    pub person_id: String,
    pub device_id: String,
    #[serde(default)]
    pub producer: Option<ProducerIdentityResponse>,
    #[serde(default)]
    pub issuer: Option<PairingIssuerResponse>,
    #[serde(default)]
    pub issuer_fingerprint: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CalendarChallengeResponse {
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
pub struct CalendarChallengeParts {
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
    pub policy: CalendarPolicyParts,
    pub source: CalendarSourceParts,
    pub grant: CalendarGrantParts,
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
pub struct CalendarPolicyParts {
    pub incarnation: String,
    pub epoch: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CalendarSourceParts {
    pub connector_id: String,
    pub connection_id: String,
    pub execution_owner: String,
    pub incarnation: String,
    pub epoch: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CalendarGrantParts {
    pub id: String,
    pub incarnation: String,
    pub epoch: u64,
}

pub fn parse_calendar_challenge(
    challenge_b64url: &str,
) -> Result<CalendarChallengeParts, AgentFailure> {
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
    serde_json::from_slice(&bytes).map_err(|_| AgentFailure::InvalidInput)
}

pub fn calendar_query_sha256(
    range_start_unix_ms: i64,
    range_end_unix_ms: i64,
    cursor: &str,
    limit: usize,
) -> Result<String, AgentFailure> {
    let query = CalendarQueryRequest {
        range_start_unix_ms,
        range_end_unix_ms,
        cursor,
        limit,
    };
    let query = serde_json::to_value(query).map_err(|_| AgentFailure::InvalidInput)?;
    let bytes = serde_json::to_vec(&query).map_err(|_| AgentFailure::InvalidInput)?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Serialize)]
struct CalendarAdmissionRequest<'value> {
    schema_version: u32,
    connector_id: &'value str,
    connection_id: &'value str,
    connection_revision: u64,
    resources: Vec<&'value str>,
    policy: CalendarPolicyRequest<'value>,
    grant: CalendarGrantRequest<'value>,
    purpose: &'value str,
    consumer: &'value str,
    max_items: u32,
    max_bytes: u32,
    query: CalendarQueryRequest<'value>,
}

#[derive(Serialize)]
struct CalendarPolicyRequest<'value> {
    incarnation: &'value str,
    epoch: u64,
}

#[derive(Serialize)]
struct CalendarGrantRequest<'value> {
    id: &'value str,
    incarnation: &'value str,
    epoch: u64,
}

#[derive(Serialize)]
struct CalendarQueryRequest<'value> {
    range_start_unix_ms: i64,
    range_end_unix_ms: i64,
    cursor: &'value str,
    limit: usize,
}

#[derive(Serialize)]
struct CalendarProofRequest<'value> {
    schema_version: u32,
    proof: CalendarProof<'value>,
}

#[derive(Serialize)]
struct CalendarProof<'value> {
    challenge_id: &'value str,
    key_id: &'value str,
    signature: &'value str,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct BeginRequest<'value> {
    key_id: &'value str,
    public_key: &'value str,
    audience: &'value str,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CompleteRequest<'value> {
    enrollment_id: &'value str,
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
    pub policy_incarnation: &'value str,
    pub policy_epoch: u64,
    pub grant_id: &'value str,
    pub grant_incarnation: &'value str,
    pub grant_epoch: u64,
    pub purpose: &'value str,
    pub consumer: &'value str,
    pub max_items: u32,
    pub max_bytes: u32,
    pub query: serde_json::Value,
}

impl RemoteAuthorizationClient {
    /// Speak to one loopback server under its bearer credential.
    ///
    /// Transport only: the caller (a prepared source or a route-supplied
    /// outer operation) owns identity binding and authorization.
    pub fn new(base_url: &str, bearer_token: &str) -> Result<Self, AgentFailure> {
        let address = Url::parse(base_url).map_err(|_| AgentFailure::InvalidInput)?;
        if address.scheme() != "http"
            || !address.username().is_empty()
            || address.password().is_some()
            || address.host_str() != Some("127.0.0.1")
            || address.path() != "/"
            || address.query().is_some()
            || address.fragment().is_some()
            || address.port().is_none()
            || bearer_token.len() < 32
            || bearer_token.len() > 256
            || !bearer_token
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            bearer_token: bearer_token.to_owned(),
        })
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

    pub async fn calendar_source_preview(
        &self,
        connector_id: &str,
        connection_id: &str,
        resource: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<CalendarSourcePreviewResponse, AgentFailure> {
        if !matches!(connector_id, "calendar.google" | "calendar.microsoft")
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
        let response: CalendarSourcePreviewResponse = self
            .request(
                "POST",
                "/v1/authority/calendar/source",
                Some(body),
                deadline,
                cancellation,
            )
            .await?;
        if response.producer.schema_version != 1
            || response.descriptor_b64url.is_empty()
            || response.producer_signature.is_empty()
            || response.expires_at_unix_ms <= 0
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        Ok(response)
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
            || response.expires_at_unix_ms <= 0
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        Ok(response)
    }

    pub async fn begin_enrollment(
        &self,
        owner_key: &RemoteOwnerPublicKey,
        audience: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<EnrollmentResponse, AgentFailure> {
        let request = BeginRequest {
            key_id: &owner_key.key_id,
            public_key: &owner_key.public_key,
            audience,
        };
        self.request(
            "POST",
            "/v1/authority/enrollment/begin",
            Some(serde_json::to_value(request).map_err(|_| AgentFailure::InvalidInput)?),
            deadline,
            cancellation,
        )
        .await
    }

    pub async fn complete_enrollment(
        &self,
        enrollment: &EnrollmentResponse,
        signature: &RemoteEnrollmentSignature,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<(), AgentFailure> {
        if signature.key_id != enrollment.key_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let request = CompleteRequest {
            enrollment_id: &enrollment.enrollment_id,
            challenge_id: &enrollment.challenge_id,
            key_id: &signature.key_id,
            signature: &signature.signature,
        };
        let _: serde_json::Value = self
            .request(
                "POST",
                "/v1/authority/enrollment/complete",
                Some(serde_json::to_value(request).map_err(|_| AgentFailure::InvalidInput)?),
                deadline,
                cancellation,
            )
            .await?;
        Ok(())
    }

    pub async fn enrollment_status(
        &self,
        enrollment_id: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<EnrollmentStatusResponse, AgentFailure> {
        if enrollment_id.is_empty()
            || enrollment_id.len() > 128
            || enrollment_id
                .bytes()
                .any(|byte| !byte.is_ascii_alphanumeric() && byte != b'-')
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.request(
            "GET",
            &format!("/v1/authority/enrollment/{enrollment_id}"),
            None,
            deadline,
            cancellation,
        )
        .await
    }

    pub async fn enroll<Keys: RemoteAuthorizationKeys>(
        &self,
        keys: &Keys,
        client_id: &str,
        device_id: &str,
        pinned_producer: &RemoteProducerIdentity,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<EnrollmentStatusResponse, AgentFailure> {
        let producer = self.producer_identity(deadline, cancellation).await?;
        let observed = RemoteProducerIdentity {
            schema_version: producer.schema_version,
            instance_id: producer.instance_id,
            execution_owner: producer.execution_owner,
            audience: producer.audience,
            key_id: producer.key_id,
            public_key: producer.public_key,
            fingerprint: producer.fingerprint,
        };
        if &observed != pinned_producer || keys.pinned_producer().await? != *pinned_producer {
            return Err(AgentFailure::PolicyDenied);
        }
        let owner_key = keys.owner_public_key().await?;
        let enrollment = self
            .begin_enrollment(
                &owner_key,
                &pinned_producer.audience,
                deadline,
                cancellation,
            )
            .await?;
        let signature = keys
            .sign_enrollment(
                client_id,
                device_id,
                &enrollment.challenge_b64url,
                &enrollment.producer_signature,
            )
            .await?;
        self.complete_enrollment(&enrollment, &signature, deadline, cancellation)
            .await?;
        self.enrollment_status(&enrollment.enrollment_id, deadline, cancellation)
            .await
    }

    pub async fn begin_calendar_admission(
        &self,
        connector_id: &str,
        connection_id: &str,
        connection_revision: u64,
        resource: &str,
        policy_incarnation: &str,
        policy_epoch: u64,
        grant_id: &str,
        grant_incarnation: &str,
        grant_epoch: u64,
        purpose: &str,
        consumer: &str,
        max_items: u32,
        max_bytes: u32,
        range_start_unix_ms: i64,
        range_end_unix_ms: i64,
        cursor: &str,
        limit: usize,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<CalendarChallengeResponse, AgentFailure> {
        if !matches!(connector_id, "calendar.google" | "calendar.microsoft")
            || !valid_connection_id(connection_id)
            || connection_revision == 0
            || resource.is_empty()
            || resource.len() > 256
            || policy_incarnation.is_empty()
            || grant_id.is_empty()
            || grant_incarnation.is_empty()
            || purpose.is_empty()
            || consumer.is_empty()
            || max_items == 0
            || max_items > 128
            || max_bytes == 0
            || max_bytes > 1 << 20
            || range_start_unix_ms < 0
            || range_end_unix_ms <= range_start_unix_ms
            || range_end_unix_ms - range_start_unix_ms > 32 * 86_400_000
            || cursor.len() > 2048
            || cursor.chars().any(char::is_control)
            || !(1..=128).contains(&limit)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let body = CalendarAdmissionRequest {
            schema_version: 1,
            connector_id,
            connection_id,
            connection_revision,
            resources: vec![resource],
            policy: CalendarPolicyRequest {
                incarnation: policy_incarnation,
                epoch: policy_epoch,
            },
            grant: CalendarGrantRequest {
                id: grant_id,
                incarnation: grant_incarnation,
                epoch: grant_epoch,
            },
            purpose,
            consumer,
            max_items,
            max_bytes,
            query: CalendarQueryRequest {
                range_start_unix_ms,
                range_end_unix_ms,
                cursor,
                limit,
            },
        };
        self.request(
            "POST",
            "/v1/views/calendar.timeline/admit",
            Some(serde_json::to_value(body).map_err(|_| AgentFailure::InvalidInput)?),
            deadline,
            cancellation,
        )
        .await
    }

    pub async fn begin_view_admission(
        &self,
        request: RemoteViewAuthorizationRequest<'_>,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<CalendarChallengeResponse, AgentFailure> {
        if !matches!(
            request.path,
            "/v1/views/mail.communication/admit"
                | "/v1/views/work.context/admit"
                | "/v1/views/life.logistics/admit"
        ) || request.connector_id.is_empty()
            || !valid_connection_id(request.connection_id)
            || request.connection_revision == 0
            || request.resource.is_empty()
            || request.resource.len() > 256
            || request.policy_incarnation.is_empty()
            || request.policy_epoch == 0
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
        let body = CalendarAdmissionRequest {
            schema_version: 1,
            connector_id: request.connector_id,
            connection_id: request.connection_id,
            connection_revision: request.connection_revision,
            resources: vec![request.resource],
            policy: CalendarPolicyRequest {
                incarnation: request.policy_incarnation,
                epoch: request.policy_epoch,
            },
            grant: CalendarGrantRequest {
                id: request.grant_id,
                incarnation: request.grant_incarnation,
                epoch: request.grant_epoch,
            },
            purpose: request.purpose,
            consumer: request.consumer,
            max_items: request.max_items,
            max_bytes: request.max_bytes,
            query: CalendarQueryRequest {
                range_start_unix_ms: 0,
                range_end_unix_ms: 1,
                cursor: "",
                limit: 1,
            },
        };
        let mut encoded = serde_json::to_value(body).map_err(|_| AgentFailure::InvalidInput)?;
        encoded["query"] = request.query;
        self.request("POST", request.path, Some(encoded), deadline, cancellation)
            .await
    }

    pub async fn read_view_admission<Keys: RemoteAuthorizationKeys>(
        &self,
        keys: &Keys,
        expected: &RemoteCalendarAuthorizationExpectation,
        challenge: &CalendarChallengeResponse,
        path: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<CalendarChallengeResponse, AgentFailure> {
        if !path.ends_with("/read") || challenge.operation != "admission" {
            return Err(AgentFailure::InvalidInput);
        }
        let signature = keys
            .sign_calendar_authorization(
                expected,
                &challenge.challenge_b64url,
                &challenge.producer_signature,
            )
            .await?;
        let body = CalendarProofRequest {
            schema_version: 1,
            proof: CalendarProof {
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

    pub async fn release_view<Keys: RemoteAuthorizationKeys>(
        &self,
        keys: &Keys,
        expected: &RemoteCalendarAuthorizationExpectation,
        challenge: &CalendarChallengeResponse,
        path: &str,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<serde_json::Value, AgentFailure> {
        if !path.ends_with("/release") || challenge.operation != "release" {
            return Err(AgentFailure::InvalidInput);
        }
        let signature = keys
            .sign_calendar_authorization(
                expected,
                &challenge.challenge_b64url,
                &challenge.producer_signature,
            )
            .await?;
        let body = CalendarProofRequest {
            schema_version: 1,
            proof: CalendarProof {
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

    pub async fn read_calendar_admission<Keys: RemoteAuthorizationKeys>(
        &self,
        keys: &Keys,
        expected: &RemoteCalendarAuthorizationExpectation,
        challenge: &CalendarChallengeResponse,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<CalendarChallengeResponse, AgentFailure> {
        if challenge.operation != "admission" {
            return Err(AgentFailure::InvalidInput);
        }
        let signature = keys
            .sign_calendar_authorization(
                expected,
                &challenge.challenge_b64url,
                &challenge.producer_signature,
            )
            .await?;
        let body = CalendarProofRequest {
            schema_version: 1,
            proof: CalendarProof {
                challenge_id: &challenge.challenge_id,
                key_id: &signature.key_id,
                signature: &signature.signature,
            },
        };
        self.request(
            "POST",
            "/v1/views/calendar.timeline/read",
            Some(serde_json::to_value(body).map_err(|_| AgentFailure::InvalidInput)?),
            deadline,
            cancellation,
        )
        .await
    }

    pub async fn release_calendar<Keys: RemoteAuthorizationKeys>(
        &self,
        keys: &Keys,
        expected: &RemoteCalendarAuthorizationExpectation,
        challenge: &CalendarChallengeResponse,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<serde_json::Value, AgentFailure> {
        if challenge.operation != "release" {
            return Err(AgentFailure::InvalidInput);
        }
        let signature = keys
            .sign_calendar_authorization(
                expected,
                &challenge.challenge_b64url,
                &challenge.producer_signature,
            )
            .await?;
        let body = CalendarProofRequest {
            schema_version: 1,
            proof: CalendarProof {
                challenge_id: &challenge.challenge_id,
                key_id: &signature.key_id,
                signature: &signature.signature,
            },
        };
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct CalendarViewResponse {
            schema_version: u32,
            view: Box<serde_json::value::RawValue>,
        }
        let response: CalendarViewResponse = self
            .request(
                "POST",
                "/v1/views/calendar.timeline/release",
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
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let client = Client::builder()
            .timeout(timeout.min(Duration::from_secs(10)))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let mut request = match method {
            "GET" => client.get(format!("{}{path}", self.base_url)),
            "POST" => client.post(format!("{}{path}", self.base_url)),
            _ => return Err(AgentFailure::InvalidInput),
        }
        .bearer_auth(&self.bearer_token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let mut response = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            response = request.send() => response.map_err(|_| AgentFailure::CapabilityUnavailable)?,
        };
        match response.status() {
            StatusCode::BAD_REQUEST => return Err(AgentFailure::InvalidInput),
            StatusCode::UNAUTHORIZED => return Err(AgentFailure::CredentialExpired),
            StatusCode::CONFLICT => return Err(AgentFailure::Conflict),
            status if !status.is_success() => return Err(AgentFailure::CapabilityUnavailable),
            _ => {}
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            chunk = response.chunk() => chunk.map_err(|_| AgentFailure::CapabilityUnavailable)?,
        } {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| AgentFailure::CapabilityUnavailable)
    }
}

/// The pairing endpoints of a paired producer, spoken over HTTP.
///
/// This is transport only: it posts, parses, and maps a status code onto a
/// failure. What a pairing report means is Connections' own judgment.
#[derive(Clone)]
pub struct HttpRemoteControl {
    base_url: String,
}

impl HttpRemoteControl {
    /// The loopback producer this device pairs with.
    pub fn new(base_url: &str) -> Result<Self, AgentFailure> {
        let address = Url::parse(base_url).map_err(|_| AgentFailure::InvalidInput)?;
        if address.scheme() != "http"
            || !address.username().is_empty()
            || address.password().is_some()
            || address.host_str() != Some("127.0.0.1")
            || address.path() != "/"
            || address.query().is_some()
            || address.fragment().is_some()
            || address.port().is_none()
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
        })
    }

    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: serde_json::Value,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<T, AgentFailure> {
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let client = Client::builder()
            .timeout(timeout.min(Duration::from_secs(10)))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let request = client.post(format!("{}{path}", self.base_url)).json(&body);
        let mut response = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            response = request.send() => response.map_err(|_| AgentFailure::CapabilityUnavailable)?,
        };
        match response.status() {
            StatusCode::BAD_REQUEST => return Err(AgentFailure::InvalidInput),
            StatusCode::UNAUTHORIZED => return Err(AgentFailure::CredentialExpired),
            StatusCode::CONFLICT => return Err(AgentFailure::Conflict),
            status if !status.is_success() => return Err(AgentFailure::CapabilityUnavailable),
            _ => {}
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            chunk = response.chunk() => chunk.map_err(|_| AgentFailure::CapabilityUnavailable)?,
        } {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| AgentFailure::CapabilityUnavailable)
    }
}

impl RemoteControl for HttpRemoteControl {
    async fn confirm(
        &self,
        request: PairingConfirmationRequest,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<PairingConfirmation, AgentFailure> {
        let response: PairingConfirmationResponse = self
            .request(
                "/pair/confirm",
                serde_json::json!({
                    "schema_version": 1,
                    "pairing_id": request.pairing_id,
                    "proof": request.polling_proof,
                    "challenge_id": request.challenge_id,
                    "key_id": request.key_id,
                    "signature": request.signature,
                }),
                deadline,
                cancellation,
            )
            .await?;
        Ok(PairingConfirmation {
            schema_version: response.schema_version,
            pairing_id: response.pairing_id,
            status: response.status,
        })
    }

    async fn status(
        &self,
        request: PairingStatusRequest,
        deadline: tokio::time::Instant,
        cancellation: &floe_execution::Cancellation,
    ) -> Result<PairingStatus, AgentFailure> {
        let response: PairingStatusResponse = self
            .request(
                "/pair/poll",
                serde_json::json!({
                    "schema_version": 1,
                    "pairing_id": request.pairing_id,
                    "proof": request.polling_proof,
                }),
                deadline,
                cancellation,
            )
            .await?;
        Ok(pairing_report(response))
    }
}

fn producer_identity(value: ProducerIdentityResponse) -> ProducerIdentity {
    ProducerIdentity {
        schema_version: value.schema_version,
        instance_id: value.instance_id,
        execution_owner: value.execution_owner,
        audience: value.audience,
        key_id: value.key_id,
        public_key: value.public_key,
        fingerprint: value.fingerprint,
    }
}

fn pairing_issuer(value: PairingIssuerResponse) -> PairingIssuer {
    PairingIssuer {
        key_id: value.key_id,
        public_key: value.public_key,
        fingerprint: value.fingerprint,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use floe_agent_contract::PersonId;
    use floe_vault::{EncryptedAgentVault, VaultKey, VaultKeyProvider};
    use tokio::io::AsyncWriteExt;
    use uuid::Uuid;

    use super::*;

    fn route(address: std::net::SocketAddr) -> RemoteRoute {
        RemoteRoute {
            base_url: format!("http://127.0.0.1:{}", address.port()),
            bearer_token: "secret_token_value_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: true,
            allow_external: false,
            recipient: Some("fixture.example".into()),
            pairing: None,
        }
    }

    fn authorization_client(address: std::net::SocketAddr) -> RemoteAuthorizationClient {
        let route = route(address);
        RemoteAuthorizationClient::new(&route.base_url, &route.bearer_token).unwrap()
    }

    #[derive(Clone, Default)]
    pub(super) struct TestKeys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for TestKeys {
        fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    #[tokio::test]
    async fn producer_identity_uses_authenticated_loopback_http() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 2048];
            let read = tokio::io::AsyncReadExt::read(&mut socket, &mut request)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.starts_with("GET /v1/authority/producer HTTP/1.1\r\n"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer secret_token_value_that_is_long_enough")
            );
            let body = r#"{"schema_version":1,"instance_id":"00000000-0000-4000-8000-000000000001","execution_owner":"00000000-0000-4000-8000-000000000002","audience":"floe.server:00000000-0000-4000-8000-000000000001","key_id":"00000000-0000-4000-8000-000000000003","public_key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","fingerprint":"0000000000000000000000000000000000000000000000000000000000000000"}"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let client = authorization_client(address);
        let identity = client
            .producer_identity(
                tokio::time::Instant::now() + Duration::from_secs(5),
                &floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            identity.audience,
            "floe.server:00000000-0000-4000-8000-000000000001"
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn pairing_transport_uses_proof_auth_and_strict_statuses() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for expected_path in ["/pair/confirm", "/pair/poll"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 4096];
                let read = tokio::io::AsyncReadExt::read(&mut socket, &mut request)
                    .await
                    .unwrap();
                let request = String::from_utf8_lossy(&request[..read]);
                assert!(request.starts_with(&format!("POST {expected_path} HTTP/1.1\r\n")));
                assert!(!request.to_ascii_lowercase().contains("authorization:"));
                let body = if expected_path == "/pair/confirm" {
                    r#"{"schema_version":1,"pairing_id":"00000000-0000-4000-8000-000000000001","status":"local_confirmed"}"#
                } else {
                    r#"{"schema_version":1,"pairing_id":"00000000-0000-4000-8000-000000000001","status":"approved","person_id":"00000000-0000-4000-8000-000000000002","device_id":"device","producer":{"schema_version":1,"instance_id":"00000000-0000-4000-8000-000000000003","execution_owner":"00000000-0000-4000-8000-000000000004","audience":"audience","key_id":"00000000-0000-4000-8000-000000000005","public_key":"public","fingerprint":"fingerprint"},"issuer":{"key_id":"00000000-0000-4000-8000-000000000006","public_key":"issuer","fingerprint":"issuer-fingerprint"},"issuer_fingerprint":"issuer-fingerprint","client_id":"00000000-0000-4000-8000-000000000001","token":"token"}"#
                };
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let service = floe_connections::PairingService::new(
            HttpRemoteControl::new(&format!("http://127.0.0.1:{}", address.port())).unwrap(),
        );
        let pair_id = "00000000-0000-4000-8000-000000000001";
        let cancellation = floe_execution::Cancellation::default();
        let confirmation = service
            .confirm(
                PairingConfirmationRequest {
                    pairing_id: pair_id.into(),
                    polling_proof: "polling-proof".into(),
                    challenge_id: pair_id.into(),
                    key_id: "00000000-0000-4000-8000-000000000006".into(),
                    signature: "owner-signature".into(),
                },
                tokio::time::Instant::now() + Duration::from_secs(5),
                &cancellation,
            )
            .await
            .unwrap();
        assert_eq!(confirmation.status, "local_confirmed");
        let status = service
            .status(
                PairingStatusRequest {
                    pairing_id: pair_id.into(),
                    polling_proof: "polling-proof".into(),
                },
                tokio::time::Instant::now() + Duration::from_secs(5),
                &cancellation,
            )
            .await
            .unwrap();
        assert_eq!(status.status, "approved");
        assert_eq!(status.token.as_deref(), Some("token"));
        assert_eq!(status.client_id.as_deref(), Some(pair_id));
        assert_eq!(
            status.issuer.as_ref().unwrap().fingerprint,
            "issuer-fingerprint"
        );
        server.await.unwrap();
    }

    #[test]
    fn pairing_start_response_matches_go_contract() {
        let response: PairingStartResponse = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "pairing_id": "00000000-0000-4000-8000-000000000001",
                "code": "ABCD1234",
                "proof": "polling-proof",
                "expires_at_unix_ms": 1893456000000,
                "person_id": "00000000-0000-4000-8000-000000000002",
                "device_id": "device",
                "producer": {
                    "schema_version": 1,
                    "instance_id": "00000000-0000-4000-8000-000000000003",
                    "execution_owner": "00000000-0000-4000-8000-000000000004",
                    "audience": "floe.server:00000000-0000-4000-8000-000000000003",
                    "key_id": "00000000-0000-4000-8000-000000000005",
                    "public_key": "cHVibGlj",
                    "fingerprint": "fingerprint"
                },
                "issuer": {
                    "key_id": "00000000-0000-4000-8000-000000000006",
                    "public_key": "aXNzdWVy",
                    "fingerprint": "issuer-fingerprint"
                },
                "challenge_id": "00000000-0000-4000-8000-000000000007",
                "challenge_b64url": "Y2hhbGxlbmdl",
                "producer_signature": "c2lnbmF0dXJl"
            }"#,
        )
        .unwrap();
        assert_eq!(response.schema_version, 1);
        assert_eq!(response.code, "ABCD1234");
        assert_eq!(response.issuer.fingerprint, "issuer-fingerprint");
    }

    #[tokio::test]
    async fn oversized_response_is_rejected_before_body_allocation() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 512];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut request)
                .await
                .unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 65537\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
        });
        let client = authorization_client(address);
        assert_eq!(
            client
                .producer_identity(
                    tokio::time::Instant::now() + Duration::from_secs(5),
                    &floe_execution::Cancellation::default()
                )
                .await,
            Err(AgentFailure::BudgetExceeded)
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn chunked_response_is_bounded_without_content_length() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 512];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut request)
                .await
                .unwrap();
            let first = "a".repeat(40_000);
            let second = "b".repeat(40_000);
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{}\r\n{:x}\r\n{}\r\n0\r\n\r\n",
                        first.len(),
                        first,
                        second.len(),
                        second
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let client = authorization_client(address);
        assert_eq!(
            client
                .producer_identity(
                    tokio::time::Instant::now() + Duration::from_secs(5),
                    &floe_execution::Cancellation::default()
                )
                .await,
            Err(AgentFailure::BudgetExceeded)
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn stalled_response_honors_cancellation() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 512];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut request).await;
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let client = authorization_client(address);
        let cancellation = floe_execution::Cancellation::default();
        let request_client = client.clone();
        let request_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            request_client
                .producer_identity(
                    tokio::time::Instant::now() + Duration::from_secs(5),
                    &request_cancellation,
                )
                .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancellation.cancel();
        assert_eq!(task.await.unwrap(), Err(AgentFailure::Cancelled));
        server.abort();
    }

    #[tokio::test]
    async fn forged_producer_signature_is_rejected_before_complete_request() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys)
            .await
            .unwrap_or_else(|failure| panic!("vault setup failed: {failure:?}"));
        let owner = vault.remote_owner_public_key().await.unwrap();
        let public_key = [9u8; 32];
        let instance_id = "00000000-0000-4000-8000-000000000001";
        let producer = RemoteProducerIdentity {
            schema_version: 1,
            instance_id: instance_id.into(),
            execution_owner: "00000000-0000-4000-8000-000000000002".into(),
            audience: format!("floe.server:{instance_id}"),
            key_id: "00000000-0000-4000-8000-000000000003".into(),
            public_key: URL_SAFE_NO_PAD.encode(public_key),
            fingerprint: "8c0cc17a04942cc4f8e0fe0b302606d3108860c126428ba2ceeb5f9ed41c2b05".into(),
        };
        vault.remote_pin_producer(producer.clone()).await.unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let challenge = serde_json::json!({
            "v": 1,
            "operation": "enrollment",
            "challenge_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "nonce": URL_SAFE_NO_PAD.encode([0u8; 32]),
            "key_id": owner.key_id.clone(),
            "person_id": person_id.to_string(),
            "client_id": "client-1",
            "device_id": "device-1",
            "audience": producer.audience.clone(),
            "purpose": "owner_enrollment",
            "consumer": "owner",
            "issued_at_unix_ms": now,
            "expires_at_unix_ms": now + 30_000,
        });
        let challenge_bytes = serde_json::to_vec(&challenge).unwrap();
        let challenge_b64url = URL_SAFE_NO_PAD.encode(challenge_bytes);
        let forged_signature = URL_SAFE_NO_PAD.encode([1u8; 64]);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for body in [
                format!(
                    "{{\"schema_version\":1,\"instance_id\":\"{instance_id}\",\"execution_owner\":\"00000000-0000-4000-8000-000000000002\",\"audience\":\"floe.server:{instance_id}\",\"key_id\":\"00000000-0000-4000-8000-000000000003\",\"public_key\":\"{}\",\"fingerprint\":\"{}\"}}",
                    URL_SAFE_NO_PAD.encode(public_key),
                    "8c0cc17a04942cc4f8e0fe0b302606d3108860c126428ba2ceeb5f9ed41c2b05",
                ),
                format!(
                    "{{\"schema_version\":1,\"instance_id\":\"{instance_id}\",\"execution_owner\":\"00000000-0000-4000-8000-000000000002\",\"audience\":\"floe.server:{instance_id}\",\"producer_key_id\":\"00000000-0000-4000-8000-000000000003\",\"producer_public_key\":\"{}\",\"producer_fingerprint\":\"{}\",\"enrollment_id\":\"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa\",\"challenge_id\":\"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa\",\"key_id\":\"{}\",\"fingerprint\":\"1111111111111111111111111111111111111111111111111111111111111111\",\"challenge_b64url\":\"{}\",\"producer_signature\":\"{}\",\"expires\":0}}",
                    URL_SAFE_NO_PAD.encode(public_key),
                    "8c0cc17a04942cc4f8e0fe0b302606d3108860c126428ba2ceeb5f9ed41c2b05",
                    owner.key_id,
                    challenge_b64url,
                    forged_signature,
                ),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 8192];
                let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut request)
                    .await
                    .unwrap();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let client = authorization_client(address);
        assert_eq!(
            client
                .enroll(
                    &vault,
                    "client-1",
                    "device-1",
                    &producer,
                    tokio::time::Instant::now() + Duration::from_secs(5),
                    &floe_execution::Cancellation::default(),
                )
                .await,
            Err(AgentFailure::PolicyDenied)
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn enrollment_begin_complete_status_flow_uses_fixed_routes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for (path, body) in [
                (
                    "/v1/authority/enrollment/begin",
                    r#"{"schema_version":1,"instance_id":"00000000-0000-4000-8000-000000000001","execution_owner":"00000000-0000-4000-8000-000000000002","audience":"floe.server:00000000-0000-4000-8000-000000000001","producer_key_id":"00000000-0000-4000-8000-000000000003","producer_public_key":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","producer_fingerprint":"0000000000000000000000000000000000000000000000000000000000000000","enrollment_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","challenge_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","key_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","fingerprint":"1111111111111111111111111111111111111111111111111111111111111111","challenge_b64url":"AQ","producer_signature":"AQ","expires":0}"#,
                ),
                (
                    "/v1/authority/enrollment/complete",
                    r#"{"status":"pending_admin"}"#,
                ),
                (
                    "/v1/authority/enrollment/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                    r#"{"enrollment_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","key_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","fingerprint":"1111111111111111111111111111111111111111111111111111111111111111","local_confirmed":true,"admin_approved":false,"active":false}"#,
                ),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 8192];
                let read = tokio::io::AsyncReadExt::read(&mut socket, &mut request)
                    .await
                    .unwrap();
                let request = String::from_utf8_lossy(&request[..read]);
                assert!(request.starts_with(&format!(
                    "{} ",
                    if path.ends_with("begin") {
                        "POST"
                    } else if path.ends_with("complete") {
                        "POST"
                    } else {
                        "GET"
                    }
                )));
                assert!(request.contains(path));
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let client = authorization_client(address);
        let owner = RemoteOwnerPublicKey {
            key_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            public_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
        };
        let enrollment = client
            .begin_enrollment(
                &owner,
                "floe.server:00000000-0000-4000-8000-000000000001",
                tokio::time::Instant::now() + Duration::from_secs(5),
                &floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        client
            .complete_enrollment(
                &enrollment,
                &RemoteEnrollmentSignature {
                    key_id: owner.key_id.clone(),
                    signature: "AQ".into(),
                },
                tokio::time::Instant::now() + Duration::from_secs(5),
                &floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        let status = client
            .enrollment_status(
                &enrollment.enrollment_id,
                tokio::time::Instant::now() + Duration::from_secs(5),
                &floe_execution::Cancellation::default(),
            )
            .await
            .unwrap();
        assert!(!status.active);
        server.await.unwrap();
    }
}

/// The producer identity as Access states it.
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

/// The enrollment state as Access states it.
pub fn enrollment_status(status: EnrollmentStatusResponse) -> floe_access::RemoteEnrollmentStatus {
    floe_access::RemoteEnrollmentStatus {
        enrollment_id: status.enrollment_id,
        key_id: status.key_id,
        fingerprint: status.fingerprint,
        local_confirmed: status.local_confirmed,
        admin_approved: status.admin_approved,
        active: status.active,
    }
}

/// One pairing report, read as Connections states it.
///
/// Nothing here decides whether the report may be acted on; the caller admits
/// it through Connections before it does anything with it.
pub fn pairing_report(response: PairingStatusResponse) -> PairingStatus {
    PairingStatus {
        schema_version: response.schema_version,
        pairing_id: response.pairing_id,
        status: response.status,
        person_id: response.person_id,
        device_id: response.device_id,
        producer: response.producer.map(producer_identity),
        issuer: response.issuer.map(pairing_issuer),
        issuer_fingerprint: response.issuer_fingerprint,
        client_id: response.client_id,
        token: response.token,
    }
}

/// How long each authority call is given, on top of whatever budget the caller
/// already set. These are transport budgets, not policy.
const PRODUCER_IDENTITY_BUDGET: Duration = Duration::from_secs(10);
const ENROLLMENT_BUDGET: Duration = Duration::from_secs(30);

/// The paired producer's authority endpoints, as Access asks for them.
///
/// Access decides which producer may be trusted and in what order an enrollment
/// happens; this only speaks HTTP to the one it is pointed at, under the key
/// holder it was given.
pub struct RemoteAuthorityEndpoint<'a, Keys> {
    client: RemoteAuthorizationClient,
    client_id: String,
    /// A locked vault has no key to enroll under; asking who the producer is
    /// still works without one.
    keys: Option<&'a Keys>,
}

impl<'a, Keys: RemoteAuthorizationKeys> RemoteAuthorityEndpoint<'a, Keys> {
    pub fn from_current_connection(
        store: &impl floe_inference::SavedConnectionStore,
        person_id: &str,
        device_id: &str,
        keys: Option<&'a Keys>,
    ) -> Result<Self, AgentFailure> {
        let saved = store.load()?.ok_or(AgentFailure::PolicyDenied)?;
        let source = super::PreparedServerSource::admit(saved, person_id, device_id)?;
        Ok(Self {
            client: RemoteAuthorizationClient::new(source.base_url(), source.bearer_token())?,
            client_id: source.client_id().to_owned(),
            keys,
        })
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }
}

fn bounded(window: &floe_access::RemoteCallWindow, budget: Duration) -> tokio::time::Instant {
    window.deadline.min(tokio::time::Instant::now() + budget)
}

impl<Keys: RemoteAuthorizationKeys> floe_access::RemoteAuthorityTransport
    for RemoteAuthorityEndpoint<'_, Keys>
{
    fn producer_identity<'a>(
        &'a self,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_access::BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
        Box::pin(async move {
            let producer = self
                .client
                .producer_identity(
                    bounded(window, PRODUCER_IDENTITY_BUDGET),
                    &window.cancellation,
                )
                .await?;
            Ok(access_producer_identity(&producer))
        })
    }

    fn enroll<'a>(
        &'a self,
        client_id: &'a str,
        device_id: &'a str,
        producer: &'a RemoteProducerIdentity,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_access::BoxFuture<'a, Result<floe_access::RemoteEnrollmentStatus, AgentFailure>> {
        Box::pin(async move {
            let status = self
                .client
                .enroll(
                    self.keys.ok_or(AgentFailure::VaultUnavailable)?,
                    client_id,
                    device_id,
                    producer,
                    bounded(window, ENROLLMENT_BUDGET),
                    &window.cancellation,
                )
                .await?;
            Ok(enrollment_status(status))
        })
    }

    fn enrollment_status<'a>(
        &'a self,
        enrollment_id: &'a str,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_access::BoxFuture<'a, Result<floe_access::RemoteEnrollmentStatus, AgentFailure>> {
        Box::pin(async move {
            let status = self
                .client
                .enrollment_status(
                    enrollment_id,
                    bounded(window, PRODUCER_IDENTITY_BUDGET),
                    &window.cancellation,
                )
                .await?;
            Ok(enrollment_status(status))
        })
    }
}

impl<Keys: RemoteAuthorizationKeys> floe_access::RemoteGrantTransport
    for RemoteAuthorityEndpoint<'_, Keys>
{
    fn producer_identity<'a>(
        &'a self,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_access::BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
        Box::pin(async move {
            let producer = self
                .client
                .producer_identity(
                    bounded(window, PRODUCER_IDENTITY_BUDGET),
                    &window.cancellation,
                )
                .await?;
            Ok(access_producer_identity(&producer))
        })
    }

    fn view_source_preview<'a>(
        &'a self,
        query: floe_access::RemoteSourceQuery<'a>,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_access::BoxFuture<'a, Result<floe_access::SignedSourcePreview, AgentFailure>> {
        Box::pin(async move {
            let preview = self
                .client
                .view_source_preview(
                    query.view_id,
                    query.connector_id,
                    query.connection_id,
                    query.resource,
                    bounded(window, PRODUCER_IDENTITY_BUDGET),
                    &window.cancellation,
                )
                .await?;
            Ok(floe_access::SignedSourcePreview {
                descriptor_b64url: preview.descriptor_b64url,
                producer_signature: preview.producer_signature,
                connection_revision: preview.connection_revision,
                producer: access_producer_identity(&preview.producer),
            })
        })
    }

    fn calendar_source_preview<'a>(
        &'a self,
        query: floe_access::RemoteCalendarQuery<'a>,
        window: &'a floe_access::RemoteCallWindow,
    ) -> floe_access::BoxFuture<'a, Result<floe_access::SignedCalendarPreview, AgentFailure>> {
        Box::pin(async move {
            let preview = self
                .client
                .calendar_source_preview(
                    query.connector_id,
                    query.connection_id,
                    query.resource,
                    bounded(window, PRODUCER_IDENTITY_BUDGET),
                    &window.cancellation,
                )
                .await?;
            Ok(floe_access::SignedCalendarPreview {
                descriptor_b64url: preview.descriptor_b64url,
                producer_signature: preview.producer_signature,
                producer: access_producer_identity(&preview.producer),
            })
        })
    }
}

impl std::fmt::Debug for PairingStatusResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PairingStatusResponse")
            .field("pairing_id", &self.pairing_id)
            .field("status", &self.status)
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod current_authority_tests {
    use super::*;
    use crate::control::CurrentSavedConnectionStore;
    use floe_inference::{SavedConnectionStore, SavedServerConnection};
    use floe_vault::EncryptedAgentVault;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Clone)]
    struct Store {
        saved: Arc<Mutex<Option<SavedServerConnection>>>,
        loads: Arc<AtomicUsize>,
    }

    impl SavedConnectionStore for Store {
        fn load(&self) -> Result<Option<SavedServerConnection>, AgentFailure> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(self.saved.lock().unwrap().clone())
        }
    }

    #[test]
    fn authority_preparation_reloads_private_credentials_and_exact_identity_without_discovery() {
        let person = uuid::Uuid::new_v4().to_string();
        let saved = SavedServerConnection {
            base_url: "http://127.0.0.1:8431".into(),
            token: "first_private_token_value_long_enough".into(),
            client_id: "client-1".into(),
            person_id: person.clone(),
            device_id: "device-1".into(),
            allow_external: false,
            external_recipients: vec![],
        };
        let store = Store {
            saved: Arc::new(Mutex::new(Some(saved))),
            loads: Arc::new(AtomicUsize::new(0)),
        };
        let current = CurrentSavedConnectionStore::new(store.clone());
        type Endpoint<'store> =
            RemoteAuthorityEndpoint<'store, EncryptedAgentVault<super::tests::TestKeys>>;
        let first = Endpoint::from_current_connection(&current, &person, "device-1", None).unwrap();
        assert_eq!(first.client_id(), "client-1");
        assert_eq!(
            first.client.bearer_token,
            "first_private_token_value_long_enough"
        );
        store.saved.lock().unwrap().as_mut().unwrap().token =
            "second_private_token_value_long_enough".into();
        let second =
            Endpoint::from_current_connection(&current, &person, "device-1", None).unwrap();
        assert_eq!(
            second.client.bearer_token,
            "second_private_token_value_long_enough"
        );
        for (claimed_person, claimed_device) in [
            (uuid::Uuid::new_v4().to_string(), "device-1"),
            (person.clone(), "foreign-device"),
        ] {
            assert!(matches!(
                Endpoint::from_current_connection(&current, &claimed_person, claimed_device, None),
                Err(AgentFailure::PolicyDenied)
            ));
        }
        *store.saved.lock().unwrap() = None;
        assert!(matches!(
            Endpoint::from_current_connection(&current, &person, "device-1", None),
            Err(AgentFailure::PolicyDenied)
        ));
        assert_eq!(store.loads.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn setup_rejects_embedded_credentials_and_status_debug_redacts_new_token() {
        for endpoint in [
            "http://user:secret@127.0.0.1:8431",
            "http://user@127.0.0.1:8431",
            "https://example.com",
        ] {
            assert!(HttpRemoteControl::new(endpoint).is_err());
        }
        let status: PairingStatusResponse = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "pairing_id": "pairing", "status": "approved",
            "person_id": "person", "device_id": "device", "client_id": "pairing",
            "token": "private_new_pairing_token"
        }))
        .unwrap();
        assert!(!format!("{status:?}").contains("private_new_pairing_token"));
    }
}
