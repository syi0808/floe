//! What a remote transport may ask the key holder for.
//!
//! Access states which authority, recipient, nonce and scope an authorization
//! must prove; the vault holds the keys and produces the proof. The transport
//! gets the complete expectation and a signature over it — never a key, a
//! connection, or an open `sign(bytes)` it could point anywhere.

use std::future::Future;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use floe_kernel::AgentFailure;
use sha2::{Digest, Sha256};

use crate::application::remote_view::RemoteProducerIdentity;

/// The owner key a paired producer enrolls against.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteOwnerPublicKey {
    pub key_id: String,
    pub public_key: String,
}

impl RemoteOwnerPublicKey {
    /// The key's own identity, as everything that pins it names it.
    ///
    /// SHA-256 over the key's bytes, rendered as lowercase hex. A key that does
    /// not decode has no bytes, and fingerprints as the empty input does; it is
    /// the pin comparison, not this derivation, that rejects it.
    pub fn fingerprint(&self) -> String {
        let decoded = URL_SAFE_NO_PAD
            .decode(self.public_key.as_bytes())
            .unwrap_or_default();
        Sha256::digest(decoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// One pairing challenge, as the producer issued it.
///
/// The producer states who it is and what it is asking the owner key to sign;
/// the key holder re-checks the whole of it before it signs anything.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemotePairingChallenge {
    pub pairing_id: String,
    pub challenge_id: String,
    pub challenge_b64url: String,
    pub producer_signature: String,
    pub producer: RemoteProducerIdentity,
    pub issuer: RemoteOwnerPublicKey,
    pub expires_at_unix_ms: i64,
}

/// A proof produced under the owner key, and the key that produced it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteEnrollmentSignature {
    pub key_id: String,
    pub signature: String,
}

/// Everything an authorization must match before it may be signed.
///
/// A transport fills this in from the grant it is reading under; the key holder
/// re-checks every field against the producer's challenge before signing. It is
/// stated in full here so neither side can narrow it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteCalendarAuthorizationExpectation {
    pub operation: String,
    pub client_id: String,
    pub device_id: String,
    pub challenge_id: String,
    pub admission_id: String,
    pub query_sha256: String,
    pub result_sha256: String,
    pub grant_id: String,
    pub grant_incarnation: String,
    pub grant_epoch: u64,
    pub source_connector: String,
    pub source_connection: String,
    pub source_execution_owner: String,
    pub source_incarnation: String,
    pub source_epoch: u64,
    pub resources: Vec<String>,
    pub max_items: u32,
    pub max_bytes: u32,
}

/// The key holder a remote read proves itself to the producer with.
pub trait RemoteAuthorizationKeys: Sync {
    /// The producer this Person has pinned; anything else is not theirs.
    fn pinned_producer(
        &self,
    ) -> impl Future<Output = Result<RemoteProducerIdentity, AgentFailure>> + Send;

    fn owner_public_key(
        &self,
    ) -> impl Future<Output = Result<RemoteOwnerPublicKey, AgentFailure>> + Send;

    fn sign_enrollment(
        &self,
        client_id: &str,
        device_id: &str,
        challenge_b64url: &str,
        producer_signature_b64url: &str,
    ) -> impl Future<Output = Result<RemoteEnrollmentSignature, AgentFailure>> + Send;

    /// Sign one authorization, but only if the producer's challenge still says
    /// exactly what `expected` says.
    fn sign_calendar_authorization(
        &self,
        expected: &RemoteCalendarAuthorizationExpectation,
        challenge_b64url: &str,
        producer_signature_b64url: &str,
    ) -> impl Future<Output = Result<RemoteEnrollmentSignature, AgentFailure>> + Send;
}

/// What the paired producer reports about this device's enrollment.
///
/// Enrollment is not complete because the producer says so: the Person's own
/// key has to be confirmed locally and the producer's admin has to have
/// approved it. Both are reported separately so neither can stand in for the
/// other.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteEnrollmentStatus {
    pub enrollment_id: String,
    pub key_id: String,
    pub fingerprint: String,
    pub local_confirmed: bool,
    pub admin_approved: bool,
    pub active: bool,
}
