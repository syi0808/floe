use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use floe_agent::AgentFailure;
use ring::{
    aead, hkdf,
    rand::{SecureRandom, SystemRandom},
    signature::{self, KeyPair},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use turso::transaction::TransactionBehavior;
use uuid::Uuid;

use super::{EncryptedAgentVault, VaultKeyProvider, storage};

const SCHEMA_VERSION: i64 = 1;
const MAX_CHALLENGE_BYTES: usize = 64 * 1024;
const MAX_JSON_DEPTH: usize = 16;
const MAX_PRODUCER_PROOF_BYTES: usize = 4 * 1024;
const OWNER_WRAP_CONTEXT: &[u8] = b"floe.remote.owner-key.wrap.v1\0";
const PRODUCER_SIGNATURE_DOMAIN: &[u8] = b"floe.remote.producer.v1\0";
const OWNER_SIGNATURE_DOMAIN: &[u8] = b"floe.remote.authorization.v1\0";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteProducerIdentity {
    pub schema_version: u32,
    pub instance_id: String,
    pub execution_owner: String,
    pub audience: String,
    pub key_id: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteOwnerPublicKey {
    pub key_id: String,
    pub public_key: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteEnrollmentSignature {
    pub key_id: String,
    pub signature: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChallengeWire {
    v: u32,
    operation: String,
    challenge_id: String,
    nonce: String,
    key_id: String,
    person_id: String,
    client_id: String,
    device_id: String,
    audience: String,
    purpose: String,
    #[serde(rename = "consumer")]
    _consumer: String,
    #[serde(default)]
    policy: Option<EmptyObject>,
    #[serde(default)]
    source: Option<EmptyObject>,
    #[serde(default)]
    grant: Option<EmptyObject>,
    #[serde(default)]
    resources: Vec<String>,
    #[serde(default)]
    query_sha256: String,
    #[serde(default)]
    max_items: u32,
    #[serde(default)]
    max_bytes: u32,
    #[serde(default)]
    result_sha256: String,
    issued_at_unix_ms: i64,
    expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyObject {}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(crate) async fn initialize_remote_authority_store(
        &self,
        fresh: bool,
    ) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        if !fresh {
            self.validate_owner_key().await?;
        }
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result: Result<(), AgentFailure> = async {
            if fresh {
            transaction
                .execute(
                    "CREATE TABLE remote_authority_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL)",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "INSERT INTO remote_authority_schema VALUES (1, ?)",
                    [SCHEMA_VERSION],
                )
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "CREATE TABLE remote_authority_owner (id INTEGER PRIMARY KEY CHECK(id = 1), key_id TEXT NOT NULL, public_key TEXT NOT NULL, nonce TEXT NOT NULL, ciphertext TEXT NOT NULL)",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "CREATE TABLE remote_authority_producer (id INTEGER PRIMARY KEY CHECK(id = 1), identity_json TEXT NOT NULL)",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "CREATE TABLE remote_authority_challenges (challenge_id TEXT PRIMARY KEY, consumed_at_unix_ms INTEGER NOT NULL)",
                    (),
                )
                .await
                .map_err(storage)?;
            let (key_id, public_key, nonce, ciphertext) = self.generate_wrapped_owner_key()?;
            transaction
                .execute(
                    "INSERT INTO remote_authority_owner VALUES (1, ?, ?, ?, ?)",
                    (key_id, public_key, nonce, ciphertext),
                )
                .await
                .map_err(storage)?;
            } else {
            let mut rows = transaction
                .query(
                    "SELECT version FROM remote_authority_schema WHERE id = 1",
                    (),
                )
                .await
                .map_err(storage)?;
            let version = rows
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?;
            if version != SCHEMA_VERSION {
                return Err(AgentFailure::UnsupportedVersion);
            }
            for table in [
                "remote_authority_owner",
                "remote_authority_producer",
                "remote_authority_challenges",
            ] {
                transaction
                    .query(&format!("SELECT * FROM {table} LIMIT 0"), ())
                    .await
                    .map_err(storage)?;
            }
            let mut owner_rows = transaction
                .query("SELECT key_id, public_key, nonce, ciphertext FROM remote_authority_owner WHERE id = 1", ())
                .await
                .map_err(storage)?;
            if owner_rows.next().await.map_err(storage)?.is_none() {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
            Ok(())
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    fn generate_wrapped_owner_key(&self) -> Result<(String, String, String, String), AgentFailure> {
        let random = SystemRandom::new();
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&random)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let key_id = Uuid::new_v4().to_string();
        let mut nonce = [0u8; 12];
        random
            .fill(&mut nonce)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let wrapping_key = self.wrapping_key()?;
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &wrapping_key)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let less_safe = aead::LessSafeKey::new(unbound);
        let mut plaintext = pkcs8.as_ref().to_vec();
        less_safe
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(OWNER_WRAP_CONTEXT),
                &mut plaintext,
            )
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok((
            key_id,
            URL_SAFE_NO_PAD.encode(key_pair.public_key().as_ref()),
            URL_SAFE_NO_PAD.encode(nonce),
            URL_SAFE_NO_PAD.encode(plaintext),
        ))
    }

    fn wrapping_key(&self) -> Result<[u8; 32], AgentFailure> {
        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, b"floe.remote.owner-key.salt.v1\0");
        let pseudo = salt.extract(self.key.as_bytes());
        let info = [OWNER_WRAP_CONTEXT];
        let output = pseudo
            .expand(&info, hkdf::HKDF_SHA256)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let mut key = [0u8; 32];
        output
            .fill(&mut key)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(key)
    }

    async fn validate_owner_key(&self) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query("SELECT key_id, public_key, nonce, ciphertext FROM remote_authority_owner WHERE id = 1", ())
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        let key_id = row.get::<String>(0).map_err(storage)?;
        let public_key = decode_canonical(&row.get::<String>(1).map_err(storage)?, 32)?;
        let nonce = decode_exact(&row.get::<String>(2).map_err(storage)?, 12)?;
        let ciphertext = decode_canonical(&row.get::<String>(3).map_err(storage)?, 4096)?;
        if Uuid::parse_str(&key_id).is_err()
            || Uuid::parse_str(&key_id).is_ok_and(|identifier| identifier.is_nil())
            || ciphertext.len() < 16
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&nonce);
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &self.wrapping_key()?)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let less_safe = aead::LessSafeKey::new(unbound);
        let mut plaintext = ciphertext;
        let decrypted = less_safe
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce_bytes),
                aead::Aad::from(OWNER_WRAP_CONTEXT),
                &mut plaintext,
            )
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(decrypted)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        if key_pair.public_key().as_ref() != public_key {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(())
    }

    pub async fn remote_owner_public_key(&self) -> Result<RemoteOwnerPublicKey, AgentFailure> {
        self.validate_owner_key().await?;
        let mut rows = self
            .connection()?
            .query(
                "SELECT key_id, public_key FROM remote_authority_owner WHERE id = 1",
                (),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        Ok(RemoteOwnerPublicKey {
            key_id: row.get(0).map_err(storage)?,
            public_key: row.get(1).map_err(storage)?,
        })
    }

    pub async fn remote_pin_producer(
        &self,
        identity: RemoteProducerIdentity,
    ) -> Result<(), AgentFailure> {
        validate_producer(&identity)?;
        let encoded = serde_json::to_string(&identity).map_err(|_| AgentFailure::InvalidInput)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result: Result<(), AgentFailure> = async {
            let mut rows = transaction
                .query(
                    "SELECT identity_json FROM remote_authority_producer WHERE id = 1",
                    (),
                )
                .await
                .map_err(storage)?;
            if let Some(row) = rows.next().await.map_err(storage)? {
                let existing = row.get::<String>(0).map_err(storage)?;
                if existing != encoded {
                    return Err(AgentFailure::Conflict);
                }
            } else {
                transaction
                    .execute(
                        "INSERT INTO remote_authority_producer VALUES (1, ?)",
                        [encoded],
                    )
                    .await
                    .map_err(storage)?;
            }
            Ok(())
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub async fn remote_pinned_producer(&self) -> Result<RemoteProducerIdentity, AgentFailure> {
        let mut rows = self
            .connection()?
            .query(
                "SELECT identity_json FROM remote_authority_producer WHERE id = 1",
                (),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        let encoded = row.get::<String>(0).map_err(storage)?;
        strict_json(&encoded, MAX_PRODUCER_PROOF_BYTES)?;
        let identity: RemoteProducerIdentity =
            serde_json::from_str(&encoded).map_err(|_| AgentFailure::VaultUnavailable)?;
        validate_producer(&identity)?;
        Ok(identity)
    }

    pub async fn remote_sign_enrollment(
        &self,
        client_id: &str,
        device_id: &str,
        challenge_b64url: &str,
        producer_signature_b64url: &str,
    ) -> Result<RemoteEnrollmentSignature, AgentFailure> {
        if client_id.is_empty()
            || client_id.len() > 128
            || device_id.is_empty()
            || device_id.len() > 128
        {
            return Err(AgentFailure::InvalidInput);
        }
        let challenge = decode_canonical(challenge_b64url, MAX_CHALLENGE_BYTES)?;
        let producer_signature = decode_exact(producer_signature_b64url, 64)?;
        let wire = parse_challenge(&challenge)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AgentFailure::VaultUnavailable)?
            .as_millis() as i64;
        if wire.expires_at_unix_ms <= now || wire.issued_at_unix_ms > now.saturating_add(5_000) {
            return Err(AgentFailure::PolicyDenied);
        }
        if wire.operation != "enrollment"
            || wire.purpose != "owner_enrollment"
            || wire.client_id != client_id
            || wire.device_id != device_id
            || wire.person_id != self.person_id.to_string()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let owner = self.remote_owner_public_key().await?;
        if wire.key_id != owner.key_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let producer = self.remote_pinned_producer().await?;
        if wire.audience != producer.audience {
            return Err(AgentFailure::PolicyDenied);
        }
        let producer_key = decode_exact(&producer.public_key, 32)?;
        let mut producer_message =
            Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len() + challenge.len());
        producer_message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);
        producer_message.extend_from_slice(&challenge);
        signature::UnparsedPublicKey::new(&signature::ED25519, producer_key)
            .verify(&producer_message, &producer_signature)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        self.validate_owner_key().await?;
        let (private_key, key_id) = self.load_owner_key().await?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result: Result<(), AgentFailure> = async {
            let mut count_rows = transaction
                .query("SELECT COUNT(*) FROM remote_authority_challenges", ())
                .await
                .map_err(storage)?;
            if count_rows
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?
                >= 128
            {
                return Err(AgentFailure::QuotaExceeded);
            }
            let mut existing_challenge = transaction
                .query(
                    "SELECT challenge_id FROM remote_authority_challenges WHERE challenge_id = ?",
                    [wire.challenge_id.clone()],
                )
                .await
                .map_err(storage)?;
            if existing_challenge.next().await.map_err(storage)?.is_some() {
                return Err(AgentFailure::Conflict);
            }
            transaction
                .execute(
                    "INSERT INTO remote_authority_challenges (challenge_id, consumed_at_unix_ms) VALUES (?, ?)",
                    (wire.challenge_id.clone(), now),
                )
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            Ok(())
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await?;
        let mut owner_message = Vec::with_capacity(OWNER_SIGNATURE_DOMAIN.len() + challenge.len());
        owner_message.extend_from_slice(OWNER_SIGNATURE_DOMAIN);
        owner_message.extend_from_slice(&challenge);
        let signature = private_key.sign(&owner_message);
        Ok(RemoteEnrollmentSignature {
            key_id,
            signature: URL_SAFE_NO_PAD.encode(signature.as_ref()),
        })
    }

    async fn load_owner_key(&self) -> Result<(signature::Ed25519KeyPair, String), AgentFailure> {
        let mut rows = self
            .connection()?
            .query(
                "SELECT key_id, nonce, ciphertext FROM remote_authority_owner WHERE id = 1",
                (),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        let key_id = row.get::<String>(0).map_err(storage)?;
        let nonce = decode_exact(&row.get::<String>(1).map_err(storage)?, 12)?;
        let ciphertext = decode_canonical(&row.get::<String>(2).map_err(storage)?, 4096)?;
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&nonce);
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &self.wrapping_key()?)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let less_safe = aead::LessSafeKey::new(unbound);
        let mut ciphertext = ciphertext;
        let plaintext = less_safe
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce_bytes),
                aead::Aad::from(OWNER_WRAP_CONTEXT),
                &mut ciphertext,
            )
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(plaintext)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok((key_pair, key_id))
    }
}

fn decode_canonical(value: &str, max: usize) -> Result<Vec<u8>, AgentFailure> {
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

fn decode_exact(value: &str, length: usize) -> Result<Vec<u8>, AgentFailure> {
    let decoded = decode_canonical(value, length)?;
    if decoded.len() != length {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(decoded)
}

fn validate_producer(identity: &RemoteProducerIdentity) -> Result<(), AgentFailure> {
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

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn parse_challenge(data: &[u8]) -> Result<ChallengeWire, AgentFailure> {
    if data.len() > MAX_CHALLENGE_BYTES {
        return Err(AgentFailure::InvalidInput);
    }
    strict_json_bytes(data, MAX_CHALLENGE_BYTES)?;
    let wire: ChallengeWire =
        serde_json::from_slice(data).map_err(|_| AgentFailure::InvalidInput)?;
    let nonce = decode_canonical(&wire.nonce, 32)?;
    if wire.v != 1
        || wire.operation != "enrollment"
        || Uuid::parse_str(&wire.challenge_id).is_err()
        || Uuid::parse_str(&wire.key_id).is_err()
        || Uuid::parse_str(&wire.key_id).is_ok_and(|identifier| identifier.is_nil())
        || Uuid::parse_str(&wire.person_id).is_err()
        || Uuid::parse_str(&wire.person_id).is_ok_and(|identifier| identifier.is_nil())
        || !valid_text(&wire.client_id, 128)
        || !valid_text(&wire.device_id, 128)
        || wire.audience.is_empty()
        || wire._consumer != "owner"
        || wire.policy.is_some()
        || wire.source.is_some()
        || wire.grant.is_some()
        || !wire.resources.is_empty()
        || !wire.query_sha256.is_empty()
        || wire.max_items != 0
        || wire.max_bytes != 0
        || !wire.result_sha256.is_empty()
        || wire.issued_at_unix_ms <= 0
        || wire.expires_at_unix_ms <= wire.issued_at_unix_ms
        || nonce.len() != 32
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(wire)
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

fn strict_json(value: &str, max: usize) -> Result<(), AgentFailure> {
    strict_json_bytes(value.as_bytes(), max)
}

fn strict_json_bytes(data: &[u8], max: usize) -> Result<(), AgentFailure> {
    if data.len() > max {
        return Err(AgentFailure::InvalidInput);
    }
    let mut parser = JsonGuard { data, offset: 0 };
    parser.value(0)?;
    parser.whitespace();
    if parser.offset != data.len() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

struct JsonGuard<'data> {
    data: &'data [u8],
    offset: usize,
}

impl JsonGuard<'_> {
    fn whitespace(&mut self) {
        while self
            .data
            .get(self.offset)
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.offset += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<(), AgentFailure> {
        if depth > MAX_JSON_DEPTH {
            return Err(AgentFailure::InvalidInput);
        }
        self.whitespace();
        match self.data.get(self.offset).copied() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => self.string().map(|_| ()),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            _ => Err(AgentFailure::InvalidInput),
        }
    }

    fn object(&mut self, depth: usize) -> Result<(), AgentFailure> {
        self.offset += 1;
        self.whitespace();
        let mut keys = Vec::new();
        if self.data.get(self.offset) == Some(&b'}') {
            self.offset += 1;
            return Ok(());
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            if keys.iter().any(|candidate: &String| candidate == &key) {
                return Err(AgentFailure::InvalidInput);
            }
            keys.push(key);
            self.whitespace();
            if self.data.get(self.offset) != Some(&b':') {
                return Err(AgentFailure::InvalidInput);
            }
            self.offset += 1;
            self.value(depth)?;
            self.whitespace();
            match self.data.get(self.offset) {
                Some(b',') => self.offset += 1,
                Some(b'}') => {
                    self.offset += 1;
                    return Ok(());
                }
                _ => return Err(AgentFailure::InvalidInput),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<(), AgentFailure> {
        self.offset += 1;
        self.whitespace();
        if self.data.get(self.offset) == Some(&b']') {
            self.offset += 1;
            return Ok(());
        }
        loop {
            self.value(depth)?;
            self.whitespace();
            match self.data.get(self.offset) {
                Some(b',') => self.offset += 1,
                Some(b']') => {
                    self.offset += 1;
                    return Ok(());
                }
                _ => return Err(AgentFailure::InvalidInput),
            }
        }
    }

    fn string(&mut self) -> Result<String, AgentFailure> {
        let start = self.offset;
        if self.data.get(self.offset) != Some(&b'"') {
            return Err(AgentFailure::InvalidInput);
        }
        self.offset += 1;
        while let Some(byte) = self.data.get(self.offset).copied() {
            self.offset += 1;
            match byte {
                b'"' => {
                    return serde_json::from_slice(&self.data[start..self.offset])
                        .map_err(|_| AgentFailure::InvalidInput);
                }
                b'\\' => {
                    self.offset += 1;
                    if self.data.get(self.offset - 1).is_none() {
                        return Err(AgentFailure::InvalidInput);
                    }
                }
                0..=0x1f => return Err(AgentFailure::InvalidInput),
                _ => {}
            }
        }
        Err(AgentFailure::InvalidInput)
    }

    fn number(&mut self) -> Result<(), AgentFailure> {
        let start = self.offset;
        while self
            .data
            .get(self.offset)
            .is_some_and(|byte| matches!(byte, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.offset += 1;
        }
        if start == self.offset {
            Err(AgentFailure::InvalidInput)
        } else {
            Ok(())
        }
    }

    fn literal(&mut self, literal: &[u8]) -> Result<(), AgentFailure> {
        if self.data.get(self.offset..self.offset + literal.len()) == Some(literal) {
            self.offset += literal.len();
            Ok(())
        } else {
            Err(AgentFailure::InvalidInput)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        os::unix::fs::PermissionsExt,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize},
        },
    };

    use floe_domain::PersonId;
    use tempfile::tempdir;

    use super::*;

    #[derive(Clone, Default)]
    struct TestKeys(Arc<TestKeyState>);

    #[derive(Default)]
    struct TestKeyState {
        values: Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>,
        unavailable: AtomicBool,
        loads: AtomicUsize,
        fail_at: AtomicUsize,
    }

    impl TestKeys {
        fn deny_access(&self) {
            self.0
                .unavailable
                .store(true, std::sync::atomic::Ordering::Release);
        }

        fn fail_after_loads(&self, additional_loads: usize) {
            let current = self.0.loads.load(std::sync::atomic::Ordering::Acquire);
            self.0.fail_at.store(
                current + additional_loads,
                std::sync::atomic::Ordering::Release,
            );
        }
    }

    impl VaultKeyProvider for TestKeys {
        fn load(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
        ) -> Result<super::super::VaultKey, AgentFailure> {
            let load_number = self
                .0
                .loads
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
                + 1;
            if self
                .0
                .unavailable
                .load(std::sync::atomic::Ordering::Acquire)
                || (self.0.fail_at.load(std::sync::atomic::Ordering::Acquire) != 0
                    && load_number >= self.0.fail_at.load(std::sync::atomic::Ordering::Acquire))
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            self.0
                .values
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(super::super::VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
            key: &super::super::VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .values
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    #[test]
    fn strict_json_rejects_nested_duplicates_and_trailing_bytes() {
        assert!(strict_json_bytes(br#"{"outer":{"value":1,"value":2}}"#, 1024).is_err());
        assert!(strict_json_bytes(br#"{"value":1} {}"#, 1024).is_err());
        assert!(strict_json_bytes(br#"{"value":1}"#, 1024).is_ok());
    }

    #[test]
    fn canonical_crypto_text_requires_exact_lengths() {
        let encoded = URL_SAFE_NO_PAD.encode([7u8; 32]);
        assert!(decode_exact(&encoded, 32).is_ok());
        assert!(decode_exact(&URL_SAFE_NO_PAD.encode([7u8; 31]), 32).is_err());
        assert!(decode_exact(&(encoded + "="), 32).is_err());
    }

    #[test]
    fn producer_audience_and_fingerprint_are_bound() {
        let public_key = [9u8; 32];
        let digest = Sha256::digest(public_key);
        let identity = RemoteProducerIdentity {
            schema_version: 1,
            instance_id: "00000000-0000-4000-8000-000000000001".into(),
            execution_owner: "00000000-0000-4000-8000-000000000002".into(),
            audience: "floe.server:00000000-0000-4000-8000-000000000001".into(),
            key_id: "00000000-0000-4000-8000-000000000003".into(),
            public_key: URL_SAFE_NO_PAD.encode(public_key),
            fingerprint: encode_hex(&digest),
        };
        assert!(validate_producer(&identity).is_ok());
        let mut changed = identity.clone();
        changed.audience = "caller-controlled".into();
        assert!(validate_producer(&changed).is_err());
    }

    #[test]
    fn shared_producer_fixture_verifies_exact_bytes() {
        #[derive(Deserialize)]
        struct Fixture {
            challenge_bytes: String,
            public_key_b64url: String,
            signature_b64url: String,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../../../fixtures/remote-authorization/producer-v1.json"
        ))
        .unwrap();
        let challenge = fixture.challenge_bytes.as_bytes();
        assert!(parse_challenge(challenge).is_ok());
        let public_key = decode_exact(&fixture.public_key_b64url, 32).unwrap();
        let producer_signature = decode_exact(&fixture.signature_b64url, 64).unwrap();
        let mut signed = Vec::from(PRODUCER_SIGNATURE_DOMAIN);
        signed.extend_from_slice(challenge);
        assert!(
            signature::UnparsedPublicKey::new(&signature::ED25519, public_key)
                .verify(&signed, &producer_signature)
                .is_ok()
        );
        let mut modified = signed;
        modified.push(b' ');
        assert!(
            signature::UnparsedPublicKey::new(
                &signature::ED25519,
                decode_exact(&fixture.public_key_b64url, 32).unwrap()
            )
            .verify(&modified, &producer_signature)
            .is_err()
        );
    }

    #[tokio::test]
    async fn encrypted_owner_key_survives_reopen_and_pin_is_immutable() {
        let root = tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let owner_key = vault.remote_owner_public_key().await.unwrap();
        let producer_public = [9u8; 32];
        let producer_digest = Sha256::digest(producer_public);
        let producer = RemoteProducerIdentity {
            schema_version: 1,
            instance_id: "00000000-0000-4000-8000-000000000001".into(),
            execution_owner: "00000000-0000-4000-8000-000000000002".into(),
            audience: "floe.server:00000000-0000-4000-8000-000000000001".into(),
            key_id: "00000000-0000-4000-8000-000000000003".into(),
            public_key: URL_SAFE_NO_PAD.encode(producer_public),
            fingerprint: encode_hex(&producer_digest),
        };
        vault.remote_pin_producer(producer.clone()).await.unwrap();
        vault.remote_pin_producer(producer.clone()).await.unwrap();
        let mut changed = producer.clone();
        changed.execution_owner = "00000000-0000-4000-8000-000000000004".into();
        assert_eq!(
            vault.remote_pin_producer(changed).await,
            Err(AgentFailure::Conflict)
        );
        drop(vault);
        let reopened = EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap();
        assert_eq!(reopened.remote_owner_public_key().await.unwrap(), owner_key);
        assert_eq!(reopened.remote_pinned_producer().await.unwrap(), producer);
        let mut persisted = Vec::new();
        for entry in fs::read_dir(root.path().join(person_id.to_string())).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                persisted.extend(fs::read(path).unwrap());
            }
        }
        assert!(
            !persisted
                .windows(owner_key.public_key.len())
                .any(|window| window == owner_key.public_key.as_bytes())
        );
    }

    #[tokio::test]
    async fn signed_enrollment_is_owner_bound_and_replay_denied_after_reopen() {
        let root = tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let owner_key = vault.remote_owner_public_key().await.unwrap();
        let producer_key = signature::Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let producer_pair = signature::Ed25519KeyPair::from_pkcs8(producer_key.as_ref()).unwrap();
        let producer_public = producer_pair.public_key().as_ref();
        let producer_instance = "00000000-0000-4000-8000-000000000011";
        let producer = RemoteProducerIdentity {
            schema_version: 1,
            instance_id: producer_instance.into(),
            execution_owner: "00000000-0000-4000-8000-000000000012".into(),
            audience: format!("floe.server:{producer_instance}"),
            key_id: "00000000-0000-4000-8000-000000000013".into(),
            public_key: URL_SAFE_NO_PAD.encode(producer_public),
            fingerprint: encode_hex(&Sha256::digest(producer_public)),
        };
        vault.remote_pin_producer(producer).await.unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let challenge = serde_json::json!({
            "v": 1,
            "operation": "enrollment",
            "challenge_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "nonce": URL_SAFE_NO_PAD.encode([0u8; 32]),
            "key_id": owner_key.key_id,
            "person_id": person_id.to_string(),
            "client_id": "client-1",
            "device_id": "device-1",
            "audience": "floe.server:00000000-0000-4000-8000-000000000011",
            "purpose": "owner_enrollment",
            "consumer": "owner",
            "issued_at_unix_ms": now,
            "expires_at_unix_ms": now + 30_000,
        });
        let challenge_bytes = serde_json::to_vec(&challenge).unwrap();
        let mut producer_message = Vec::from(PRODUCER_SIGNATURE_DOMAIN);
        producer_message.extend_from_slice(&challenge_bytes);
        let producer_signature = producer_pair.sign(&producer_message);
        let challenge_text = URL_SAFE_NO_PAD.encode(&challenge_bytes);
        let signature_text = URL_SAFE_NO_PAD.encode(producer_signature.as_ref());
        let owner_signature = vault
            .remote_sign_enrollment("client-1", "device-1", &challenge_text, &signature_text)
            .await
            .unwrap();
        let owner_public = decode_exact(&owner_key.public_key, 32).unwrap();
        let mut owner_message = Vec::from(OWNER_SIGNATURE_DOMAIN);
        owner_message.extend_from_slice(&challenge_bytes);
        signature::UnparsedPublicKey::new(&signature::ED25519, owner_public)
            .verify(
                &owner_message,
                &decode_exact(&owner_signature.signature, 64).unwrap(),
            )
            .unwrap();
        let mut failed_challenge = challenge.clone();
        failed_challenge["challenge_id"] =
            serde_json::json!("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        let failed_bytes = serde_json::to_vec(&failed_challenge).unwrap();
        let mut failed_message = Vec::from(PRODUCER_SIGNATURE_DOMAIN);
        failed_message.extend_from_slice(&failed_bytes);
        let failed_producer_signature = producer_pair.sign(&failed_message);
        vault
            .connection()
            .unwrap()
            .execute(
                "CREATE TRIGGER remote_authority_insert_failure BEFORE INSERT ON remote_authority_challenges BEGIN SELECT RAISE(ABORT, 'test insert failure'); END",
                (),
            )
            .await
            .unwrap();
        assert_eq!(
            vault
                .remote_sign_enrollment(
                    "client-1",
                    "device-1",
                    &URL_SAFE_NO_PAD.encode(&failed_bytes),
                    &URL_SAFE_NO_PAD.encode(failed_producer_signature.as_ref()),
                )
                .await,
            Err(AgentFailure::StorageUnavailable)
        );
        assert_eq!(
            vault.remote_owner_public_key().await,
            Err(AgentFailure::VaultUnavailable)
        );
        drop(vault);
        let reopened = EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap();
        assert_eq!(
            reopened
                .remote_sign_enrollment("client-1", "device-1", &challenge_text, &signature_text)
                .await,
            Err(AgentFailure::Conflict)
        );
    }

    #[tokio::test]
    async fn missing_authority_schema_fails_reopen_closed() {
        let root = tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        vault
            .connection()
            .unwrap()
            .execute("DROP TABLE remote_authority_owner", ())
            .await
            .unwrap();
        vault.checkpoint().await.unwrap();
        drop(vault);
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person_id, keys).await,
            Err(AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable)
        ));
    }

    #[tokio::test]
    async fn malformed_owner_nonce_private_and_public_fail_reopen_without_panic() {
        for (column, value) in [
            ("nonce", "AA".to_owned()),
            ("public_key", URL_SAFE_NO_PAD.encode([0u8; 32])),
            ("ciphertext", URL_SAFE_NO_PAD.encode([0u8; 32])),
        ] {
            let root = tempdir().unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let person_id = PersonId::new();
            let keys = TestKeys::default();
            let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
                .await
                .unwrap();
            vault
                .connection()
                .unwrap()
                .execute(
                    &format!("UPDATE remote_authority_owner SET {column} = ? WHERE id = 1"),
                    [value],
                )
                .await
                .unwrap();
            vault.checkpoint().await.unwrap();
            drop(vault);
            let reopened = EncryptedAgentVault::open(root.path(), person_id, keys).await;
            assert!(
                matches!(
                    &reopened,
                    Err(AgentFailure::InvalidInput
                        | AgentFailure::VaultUnavailable
                        | AgentFailure::StorageUnavailable,)
                ),
                "corrupt {column} reopened: {:?}",
                reopened.as_ref().err()
            );
        }
    }

    #[tokio::test]
    async fn owner_key_access_loss_fails_before_signing() {
        let root = tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let challenge = serde_json::json!({
            "v": 1,
            "operation": "enrollment",
            "challenge_id": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            "nonce": URL_SAFE_NO_PAD.encode([0u8; 32]),
            "key_id": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "person_id": person_id.to_string(),
            "client_id": "client-1",
            "device_id": "device-1",
            "audience": "floe.server:00000000-0000-4000-8000-000000000001",
            "purpose": "owner_enrollment",
            "consumer": "owner",
            "issued_at_unix_ms": now,
            "expires_at_unix_ms": now + 30_000,
        });
        let challenge_bytes = serde_json::to_vec(&challenge).unwrap();
        keys.deny_access();
        assert_eq!(
            vault
                .remote_sign_enrollment(
                    "client-1",
                    "device-1",
                    &URL_SAFE_NO_PAD.encode(challenge_bytes),
                    &URL_SAFE_NO_PAD.encode([0u8; 64]),
                )
                .await,
            Err(AgentFailure::VaultUnavailable)
        );
    }

    #[tokio::test]
    async fn owner_key_access_loss_after_commit_latches_without_returning_signature() {
        let root = tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let owner = vault.remote_owner_public_key().await.unwrap();
        let producer_pkcs8 =
            signature::Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let producer_pair = signature::Ed25519KeyPair::from_pkcs8(producer_pkcs8.as_ref()).unwrap();
        let instance_id = "00000000-0000-4000-8000-000000000011";
        let producer = RemoteProducerIdentity {
            schema_version: 1,
            instance_id: instance_id.into(),
            execution_owner: "00000000-0000-4000-8000-000000000012".into(),
            audience: format!("floe.server:{instance_id}"),
            key_id: "00000000-0000-4000-8000-000000000013".into(),
            public_key: URL_SAFE_NO_PAD.encode(producer_pair.public_key().as_ref()),
            fingerprint: encode_hex(&Sha256::digest(producer_pair.public_key().as_ref())),
        };
        vault.remote_pin_producer(producer.clone()).await.unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let challenge = serde_json::json!({
            "v": 1,
            "operation": "enrollment",
            "challenge_id": "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
            "nonce": URL_SAFE_NO_PAD.encode([0u8; 32]),
            "key_id": owner.key_id,
            "person_id": person_id.to_string(),
            "client_id": "client-1",
            "device_id": "device-1",
            "audience": producer.audience,
            "purpose": "owner_enrollment",
            "consumer": "owner",
            "issued_at_unix_ms": now,
            "expires_at_unix_ms": now + 30_000,
        });
        let challenge_bytes = serde_json::to_vec(&challenge).unwrap();
        let mut producer_message = Vec::from(PRODUCER_SIGNATURE_DOMAIN);
        producer_message.extend_from_slice(&challenge_bytes);
        let producer_signature = producer_pair.sign(&producer_message);
        keys.fail_after_loads(7);
        assert_eq!(
            vault
                .remote_sign_enrollment(
                    "client-1",
                    "device-1",
                    &URL_SAFE_NO_PAD.encode(&challenge_bytes),
                    &URL_SAFE_NO_PAD.encode(producer_signature.as_ref()),
                )
                .await,
            Err(AgentFailure::VaultUnavailable)
        );
        let connection = vault.database.connect().unwrap();
        let mut rows = connection
            .query("SELECT COUNT(*) FROM remote_authority_challenges", ())
            .await
            .unwrap();
        assert_eq!(
            rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
            1
        );
    }
}
