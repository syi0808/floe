use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
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


/// Everything an authorization must match before it may be signed.
///
/// A transport fills this in from the grant it is reading under; the key holder
/// re-checks every field against the producer's challenge before signing. It is
/// stated in full here so neither side can narrow it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteViewAuthorizationExpectation {
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

