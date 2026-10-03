use super::json::strict_json_bytes;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use floe_access::RemoteProducerIdentity;
use floe_agent_contract::AgentFailure;
use ring::signature;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;
const MAX_CHALLENGE_BYTES: usize = 65536;
const PRODUCER_SIGNATURE_DOMAIN: &[u8] = b"floe.remote.producer.v1\0";
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ViewSourcePreviewWire {
    pub(crate) v: u32,
    pub(crate) operation: String,
    pub(crate) challenge_id: String,
    pub(crate) nonce: String,
    pub(crate) view_id: String,
    pub(crate) person_id: String,
    pub(crate) client_id: String,
    pub(crate) device_id: String,
    pub(crate) audience: String,
    pub(crate) connector_id: String,
    pub(crate) connection_id: String,
    pub(crate) connection_revision: u64,
    pub(crate) execution_owner: String,
    pub(crate) incarnation: String,
    pub(crate) epoch: u64,
    pub(crate) resource: String,
    pub(crate) source_resources: Vec<String>,
    pub(crate) provider_identity: String,
    pub(crate) issued_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChallengeWire {
    pub(crate) v: u32,
    pub(crate) operation: String,
    pub(crate) challenge_id: String,
    pub(crate) nonce: String,
    pub(crate) key_id: String,
    pub(crate) person_id: String,
    pub(crate) client_id: String,
    pub(crate) device_id: String,
    pub(crate) audience: String,
    pub(crate) purpose: String,
    #[serde(rename = "consumer")]
    pub(crate) _consumer: String,
    #[serde(default)]
    pub(crate) source: Option<SourceWire>,
    #[serde(default)]
    pub(crate) grant: Option<GrantWire>,
    #[serde(default)]
    pub(crate) resources: Vec<String>,
    #[serde(default)]
    pub(crate) query_sha256: String,
    #[serde(default)]
    pub(crate) max_items: u32,
    #[serde(default)]
    pub(crate) max_bytes: u32,
    #[serde(default)]
    pub(crate) result_sha256: String,
    #[serde(default)]
    pub(crate) admission_id: String,
    pub(crate) issued_at_unix_ms: i64,
    pub(crate) expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceWire {
    pub(crate) connector_id: String,
    pub(crate) connection_id: String,
    pub(crate) execution_owner: String,
    pub(crate) incarnation: String,
    pub(crate) epoch: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GrantWire {
    pub(crate) id: String,
    pub(crate) incarnation: String,
    pub(crate) epoch: u64,
}

pub(crate) fn decode_canonical(value: &str, max: usize) -> Result<Vec<u8>, AgentFailure> {
    if value.is_empty() || value.len() > max.saturating_mul(2) {
        return Err(AgentFailure::InvalidInput);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AgentFailure::InvalidInput)?;
    if decoded.len() > max || URL_SAFE_NO_PAD.encode(&decoded) != value {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(decoded)
}

pub(crate) fn decode_exact(value: &str, length: usize) -> Result<Vec<u8>, AgentFailure> {
    let decoded = decode_canonical(value, length)?;
    if decoded.len() != length {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(decoded)
}

pub(crate) fn validate_producer(identity: &RemoteProducerIdentity) -> Result<(), AgentFailure> {
    if identity.schema_version != 1
        || Uuid::parse_str(&identity.instance_id).is_err()
        || Uuid::parse_str(&identity.execution_owner).is_err()
        || identity.audience.is_empty()
        || identity.audience.len() > 256
        || identity.audience != format!("floe.server:{}", identity.instance_id)
        || Uuid::parse_str(&identity.key_id).is_err()
        || identity.fingerprint.len() != 64
        || identity.fingerprint != identity.fingerprint.to_ascii_lowercase()
        || !identity
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(AgentFailure::InvalidInput);
    }
    let public_key = decode_exact(&identity.public_key, 32)?;
    let digest = Sha256::digest(public_key);
    if encode_hex(&digest) != identity.fingerprint.to_ascii_lowercase() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(crate) fn parse_challenge(data: &[u8]) -> Result<ChallengeWire, AgentFailure> {
    if data.len() > MAX_CHALLENGE_BYTES {
        return Err(AgentFailure::InvalidInput);
    }
    strict_json_bytes(data, MAX_CHALLENGE_BYTES)?;
    let wire: ChallengeWire =
        serde_json::from_slice(data).map_err(|_| AgentFailure::InvalidInput)?;
    let nonce = decode_canonical(&wire.nonce, 32)?;
    if wire.v != 1
        || (wire.operation != "enrollment"
            && wire.operation != "admission"
            && wire.operation != "release")
        || Uuid::parse_str(&wire.challenge_id).is_err()
        || Uuid::parse_str(&wire.key_id).is_err()
        || Uuid::parse_str(&wire.key_id).is_ok_and(|identifier| identifier.is_nil())
        || Uuid::parse_str(&wire.person_id).is_err()
        || Uuid::parse_str(&wire.person_id).is_ok_and(|identifier| identifier.is_nil())
        || !valid_text(&wire.client_id, 128)
        || !valid_text(&wire.device_id, 128)
        || wire.audience.is_empty()
        || wire.issued_at_unix_ms <= 0
        || wire.expires_at_unix_ms <= wire.issued_at_unix_ms
        || nonce.len() != 32
    {
        return Err(AgentFailure::InvalidInput);
    }
    if wire.operation == "enrollment" {
        if wire._consumer != "owner" {
            return Err(AgentFailure::InvalidInput);
        }
        if wire.source.is_some()
            || wire.grant.is_some()
            || !wire.resources.is_empty()
            || !wire.query_sha256.is_empty()
            || wire.max_items != 0
            || wire.max_bytes != 0
            || !wire.result_sha256.is_empty()
        {
            return Err(AgentFailure::InvalidInput);
        }
    } else {
        if !valid_text(&wire._consumer, 256) {
            return Err(AgentFailure::InvalidInput);
        }
        let source = wire.source.as_ref().ok_or(AgentFailure::InvalidInput)?;
        let grant = wire.grant.as_ref().ok_or(AgentFailure::InvalidInput)?;
        if source.connector_id.is_empty()
            || source.connector_id.len() > 128
            || source.connection_id.is_empty()
            || source.connection_id.len() > 128
            || source.execution_owner.is_empty()
            || source.execution_owner.len() > 128
            || Uuid::parse_str(&source.incarnation).is_err()
            || source.epoch == 0
            || Uuid::parse_str(&grant.id).is_err()
            || Uuid::parse_str(&grant.incarnation).is_err()
            || grant.epoch == 0
            || wire.resources.is_empty()
            || wire.resources.len() > 64
            || wire.resources.windows(2).any(|pair| pair[0] >= pair[1])
            || wire.query_sha256.len() != 64
            || !wire
                .query_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || wire.max_items == 0
            || wire.max_items > 128
            || wire.max_bytes == 0
            || wire.max_bytes > 1024 * 1024
        {
            return Err(AgentFailure::InvalidInput);
        }
        if wire.operation == "admission" {
            if !wire.admission_id.is_empty() {
                return Err(AgentFailure::InvalidInput);
            }
            if !wire.result_sha256.is_empty() {
                return Err(AgentFailure::InvalidInput);
            }
        } else if Uuid::parse_str(&wire.admission_id).is_err()
            || wire.result_sha256.len() != 64
            || !wire
                .result_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    Ok(wire)
}

pub(crate) fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

pub(crate) fn verify_signature(
    producer: &RemoteProducerIdentity,
    bytes: &[u8],
    proof: &[u8],
) -> Result<(), AgentFailure> {
    validate_producer(producer)?;
    let key = decode_exact(&producer.public_key, 32)?;
    let mut message = Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len() + bytes.len());
    message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);
    message.extend_from_slice(bytes);
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(&message, proof)
        .map_err(|_| AgentFailure::PolicyDenied)
}

/// Concrete wire verifier injected into the Vault signing adapters.
pub struct GatewayProofVerifier;
impl floe_connections::EnrollmentProofVerifier for GatewayProofVerifier {
    fn verify(
        &self,
        command: &floe_connections::EnrollmentSigningCommand,
    ) -> Result<(), floe_connections::PairingError> {
        let denied = || floe_connections::PairingError::ChangedProducer;
        let wire = parse_challenge(&command.canonical_bytes).map_err(|_| denied())?;
        verify_signature(
            &command.producer,
            &command.canonical_bytes,
            &command.producer_signature,
        )
        .map_err(|_| denied())?;
        if wire.operation != "enrollment"
            || wire.purpose != "owner_enrollment"
            || wire._consumer != "owner"
            || wire.person_id != command.person_id.to_string()
            || wire.device_id != command.device_id
            || wire.client_id != command.client_id
            || wire.key_id != command.issuer.key_id
            || wire.audience != command.producer.audience
            || wire.challenge_id != command.challenge_id.to_string()
            || wire.issued_at_unix_ms != command.issued_at_unix_ms
            || wire.expires_at_unix_ms != command.expires_at_unix_ms
            || command.request_digest != <[u8; 32]>::from(Sha256::digest(&command.canonical_bytes))
        {
            return Err(denied());
        }
        Ok(())
    }
}
impl floe_access::AuthorizationProofVerifier for GatewayProofVerifier {
    fn verify(
        &self,
        command: &floe_access::AuthorizationSigningCommand,
    ) -> Result<floe_access::VerifiedAuthorizationClaims, AgentFailure> {
        let wire = parse_challenge(&command.canonical_bytes)?;
        verify_signature(
            &command.producer,
            &command.canonical_bytes,
            &command.producer_signature,
        )?;
        if wire.audience != command.producer.audience {
            return Err(AgentFailure::PolicyDenied);
        }
        let source = wire.source.ok_or(AgentFailure::PolicyDenied)?;
        let grant = wire.grant.ok_or(AgentFailure::PolicyDenied)?;
        let purpose = match wire.purpose.as_str() {
            "quick_response" | "everyday_assistance" | "deep_work" => {
                floe_access::GrantPurpose::Assistant
            }
            "scheduling" => floe_access::GrantPurpose::Scheduling,
            "summarization" => floe_access::GrantPurpose::Summarization,
            _ => return Err(AgentFailure::PolicyDenied),
        };
        let consumer = floe_access::GrantConsumer::new(wire._consumer)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        Ok(floe_access::VerifiedAuthorizationClaims {
            person_id: wire.person_id,
            key_id: wire.key_id,
            issued_at_unix_ms: wire.issued_at_unix_ms,
            expires_at_unix_ms: wire.expires_at_unix_ms,
            purpose,
            consumer,
            producer: command.producer.clone(),
            expected: floe_access::RemoteViewAuthorizationExpectation {
                operation: wire.operation,
                client_id: wire.client_id,
                device_id: wire.device_id,
                challenge_id: wire.challenge_id,
                admission_id: wire.admission_id,
                query_sha256: wire.query_sha256,
                result_sha256: wire.result_sha256,
                grant_id: grant.id,
                grant_incarnation: grant.incarnation,
                grant_epoch: grant.epoch,
                source_connector: source.connector_id,
                source_connection: source.connection_id,
                source_execution_owner: source.execution_owner,
                source_incarnation: source.incarnation,
                source_epoch: source.epoch,
                resources: wire.resources,
                max_items: wire.max_items,
                max_bytes: wire.max_bytes,
            },
        })
    }
}
pub(crate) fn authorization_command(
    expected: &floe_access::RemoteViewAuthorizationExpectation,
    challenge: &str,
    signature: &str,
    producer: RemoteProducerIdentity,
) -> Result<floe_access::AuthorizationSigningCommand, AgentFailure> {
    let canonical_bytes = decode_canonical(challenge, 65536)?;
    let producer_signature = decode_exact(signature, 64)?;
    let wire = parse_challenge(&canonical_bytes)?;
    let purpose = match wire.purpose.as_str() {
        "quick_response" | "everyday_assistance" | "deep_work" => {
            floe_access::GrantPurpose::Assistant
        }
        "scheduling" => floe_access::GrantPurpose::Scheduling,
        "summarization" => floe_access::GrantPurpose::Summarization,
        _ => return Err(AgentFailure::PolicyDenied),
    };
    let consumer =
        floe_access::GrantConsumer::new(wire._consumer).map_err(|_| AgentFailure::PolicyDenied)?;
    let command = floe_access::AuthorizationSigningCommand {
        operation_id: Uuid::parse_str(&expected.challenge_id)
            .map_err(|_| AgentFailure::PolicyDenied)?,
        request_digest: Sha256::digest(&canonical_bytes).into(),
        expected: expected.clone(),
        purpose,
        consumer,
        producer,
        canonical_bytes,
        producer_signature,
        expires_at_unix_ms: wire.expires_at_unix_ms,
    };
    use floe_access::AuthorizationProofVerifier;
    let claims = GatewayProofVerifier.verify(&command)?;
    if claims.expected != *expected
        || claims.purpose != command.purpose
        || claims.consumer != command.consumer
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(command)
}
