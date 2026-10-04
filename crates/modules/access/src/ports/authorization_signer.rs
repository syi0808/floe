use crate::{RemoteProducerIdentity, RemoteViewAuthorizationExpectation};
use floe_context_contract::{GrantConsumer, GrantPurpose};
use floe_execution::BoxFuture;
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub enum AuthorizationSigningCommand<'a> {
    AssistantView(AssistantAuthorizationSigningCommand),
    DayCalendarRefresh(crate::ProductCalendarSigningCommand<'a>),
}

#[derive(Clone, Debug)]
pub struct AssistantAuthorizationSigningCommand {
    pub operation_id: Uuid,
    pub request_digest: [u8; 32],
    pub expected: RemoteViewAuthorizationExpectation,
    pub purpose: GrantPurpose,
    pub consumer: GrantConsumer,
    pub producer: RemoteProducerIdentity,
    pub canonical_bytes: Vec<u8>,
    pub producer_signature: Vec<u8>,
    pub expires_at_unix_ms: i64,
}
impl AssistantAuthorizationSigningCommand {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        use sha2::Digest;
        let digest: [u8; 32] = sha2::Sha256::digest(&self.canonical_bytes).into();
        if self.operation_id.is_nil()
            || self.request_digest != digest
            || self.canonical_bytes.is_empty()
            || self.canonical_bytes.len() > 64 * 1024
            || self.producer_signature.len() != 64
            || self.expires_at_unix_ms <= chrono::Utc::now().timestamp_millis()
            || !matches!(self.expected.operation.as_str(), "admission" | "release")
            || self.expected.resources.len() != 1
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationSignature {
    pub key_id: String,
    pub signature: String,
}

pub trait AuthorizationSigner: Send + Sync {
    fn sign_authorization<'a>(
        &'a self,
        command: AuthorizationSigningCommand<'a>,
    ) -> BoxFuture<'a, Result<AuthorizationSignature, AgentFailure>>;
}

/// Normalized claims returned only after strict transport decoding and producer
/// signature verification. They must match the owner's requested authorization
/// before any Vault grant check or signing occurs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedAuthorizationClaims {
    pub person_id: String,
    pub key_id: String,
    pub issued_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub expected: RemoteViewAuthorizationExpectation,
    pub purpose: GrantPurpose,
    pub consumer: GrantConsumer,
    pub producer: RemoteProducerIdentity,
}

pub trait AuthorizationProofVerifier: Send + Sync {
    fn verify_product(
        &self,
        command: &crate::ProductCalendarSigningCommand<'_>,
    ) -> Result<crate::ProductCalendarChallenge, AgentFailure>;

    fn verify(
        &self,
        command: &AssistantAuthorizationSigningCommand,
    ) -> Result<VerifiedAuthorizationClaims, AgentFailure>;
}

impl AssistantAuthorizationSigningCommand {
    pub fn validate_claims(
        &self,
        claims: &VerifiedAuthorizationClaims,
        person_id: floe_kernel::PersonId,
        owner_key_id: &str,
        now_unix_ms: i64,
    ) -> Result<(), AgentFailure> {
        self.validate()?;
        if claims.person_id != person_id.to_string()
            || claims.key_id != owner_key_id
            || claims.expected != self.expected
            || claims.purpose != self.purpose
            || claims.consumer != self.consumer
            || claims.producer != self.producer
            || claims.expires_at_unix_ms != self.expires_at_unix_ms
            || claims.issued_at_unix_ms <= 0
            || claims.issued_at_unix_ms > now_unix_ms.saturating_add(5_000)
            || claims.expires_at_unix_ms <= now_unix_ms
            || claims.expires_at_unix_ms <= claims.issued_at_unix_ms
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
}
