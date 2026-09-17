use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use floe_agent_contract::AgentFailure;
use floe_access::GrantState;

/// The remote authority values this vault stores. Who a producer is and what a
/// signed source names are Access's; this module holds and signs the records.
pub use floe_access::{RemoteProducerIdentity, RemoteViewSourceReference};
use floe_context_contract::{GrantOperation, GrantPurpose, ProcessingRestriction, SourceAuthority};
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteCalendarSourceReference {
    pub person_id: String,
    pub client_id: String,
    pub device_id: String,
    pub audience: String,
    pub connector_id: String,
    pub connection_id: String,
    pub execution_owner: String,
    pub source_authority: SourceAuthority,
    pub resource: String,
    pub provider_identity: String,
}


#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalendarSourcePreviewWire {
    v: u32,
    operation: String,
    challenge_id: String,
    nonce: String,
    person_id: String,
    client_id: String,
    device_id: String,
    audience: String,
    connector_id: String,
    connection_id: String,
    execution_owner: String,
    incarnation: String,
    epoch: u64,
    resource: String,
    provider_identity: String,
    issued_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewSourcePreviewWire {
    v: u32,
    operation: String,
    challenge_id: String,
    nonce: String,
    view_id: String,
    person_id: String,
    client_id: String,
    device_id: String,
    audience: String,
    connector_id: String,
    connection_id: String,
    connection_revision: u64,
    execution_owner: String,
    incarnation: String,
    epoch: u64,
    resource: String,
    provider_identity: String,
    issued_at_unix_ms: i64,
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
    policy: Option<PolicyWire>,
    #[serde(default)]
    source: Option<SourceWire>,
    #[serde(default)]
    grant: Option<GrantWire>,
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
    #[serde(default)]
    admission_id: String,
    issued_at_unix_ms: i64,
    expires_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyWire {
    incarnation: String,
    epoch: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceWire {
    connector_id: String,
    connection_id: String,
    execution_owner: String,
    incarnation: String,
    epoch: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantWire {
    id: String,
    incarnation: String,
    epoch: u64,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn verify_remote_view_source_preview(
        &self,
        descriptor_b64url: &str,
        producer_signature_b64url: &str,
        person_id: &str,
        client_id: &str,
        device_id: &str,
        view_id: &str,
        connector_id: &str,
        connection_id: &str,
        resource: &str,
    ) -> Result<RemoteViewSourceReference, AgentFailure> {
        let descriptor = decode_canonical(descriptor_b64url, MAX_PRODUCER_PROOF_BYTES)?;
        let producer_signature = decode_exact(producer_signature_b64url, 64)?;
        strict_json_bytes(&descriptor, MAX_PRODUCER_PROOF_BYTES)?;
        let wire: ViewSourcePreviewWire =
            serde_json::from_slice(&descriptor).map_err(|_| AgentFailure::InvalidInput)?;
        let producer = self.remote_pinned_producer().await?;
        let producer_key = decode_exact(&producer.public_key, 32)?;
        let mut message = Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len() + descriptor.len());
        message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);
        message.extend_from_slice(&descriptor);
        signature::UnparsedPublicKey::new(&signature::ED25519, producer_key)
            .verify(&message, &producer_signature)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if wire.v != 1
            || wire.operation != "remote_view_source_preview"
            || wire.view_id != view_id
            || wire.person_id != person_id
            || person_id != self.person_id.to_string()
            || wire.client_id != client_id
            || wire.device_id != device_id
            || wire.audience != producer.audience
            || wire.connector_id != connector_id
            || wire.connection_id != connection_id
            || wire.resource != resource
            || wire.execution_owner != producer.execution_owner
            || !valid_text(&wire.nonce, 256)
            || wire.connection_revision == 0
            || !valid_text(&wire.provider_identity, 256)
            || wire.issued_at_unix_ms <= 0
            || Uuid::parse_str(&wire.challenge_id).is_err()
            || Uuid::parse_str(&wire.incarnation).is_err()
            || Uuid::parse_str(&wire.incarnation).is_ok_and(|identifier| identifier.is_nil())
            || wire.epoch == 0
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let source_authority = SourceAuthority::from_parts(
            Uuid::parse_str(&wire.incarnation).map_err(|_| AgentFailure::PolicyDenied)?,
            std::num::NonZeroU64::new(wire.epoch).ok_or(AgentFailure::PolicyDenied)?,
        )
        .ok_or(AgentFailure::PolicyDenied)?;
        Ok(RemoteViewSourceReference {
            view_id: wire.view_id,
            person_id: wire.person_id,
            client_id: wire.client_id,
            device_id: wire.device_id,
            audience: wire.audience,
            connector_id: wire.connector_id,
            connection_id: wire.connection_id,
            connection_revision: wire.connection_revision,
            execution_owner: wire.execution_owner,
            source_authority,
            resource: wire.resource,
            provider_identity: wire.provider_identity,
        })
    }
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
                    "CREATE TABLE remote_authority_challenges (challenge_id TEXT PRIMARY KEY, operation TEXT NOT NULL, admission_id TEXT NOT NULL, query_sha256 TEXT NOT NULL, result_sha256 TEXT NOT NULL, grant_id TEXT NOT NULL, grant_incarnation TEXT NOT NULL, grant_epoch INTEGER NOT NULL, source_connector TEXT NOT NULL, source_connection TEXT NOT NULL, source_execution_owner TEXT NOT NULL, source_incarnation TEXT NOT NULL, source_epoch INTEGER NOT NULL, max_items INTEGER NOT NULL, max_bytes INTEGER NOT NULL, expires_at_unix_ms INTEGER NOT NULL, consumed_at_unix_ms INTEGER NOT NULL)",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "CREATE TABLE remote_authority_clock (id INTEGER PRIMARY KEY CHECK(id = 1), last_now_unix_ms INTEGER NOT NULL)",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction
                .execute("INSERT INTO remote_authority_clock VALUES (1, 0)", ())
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
                "remote_authority_clock",
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
            let mut clock_rows = transaction
                .query("SELECT last_now_unix_ms FROM remote_authority_clock WHERE id = 1", ())
                .await
                .map_err(storage)?;
            if clock_rows.next().await.map_err(storage)?.is_none() {
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

    pub async fn verify_remote_calendar_source_preview(
        &self,
        descriptor_b64url: &str,
        producer_signature_b64url: &str,
        person_id: &str,
        client_id: &str,
        device_id: &str,
        connector_id: &str,
        connection_id: &str,
        resource: &str,
    ) -> Result<RemoteCalendarSourceReference, AgentFailure> {
        let descriptor = decode_canonical(descriptor_b64url, MAX_PRODUCER_PROOF_BYTES)?;
        let producer_signature = decode_exact(producer_signature_b64url, 64)?;
        strict_json_bytes(&descriptor, MAX_PRODUCER_PROOF_BYTES)?;
        let wire: CalendarSourcePreviewWire =
            serde_json::from_slice(&descriptor).map_err(|_| AgentFailure::InvalidInput)?;
        let producer = self.remote_pinned_producer().await?;
        let producer_key = decode_exact(&producer.public_key, 32)?;
        let mut message = Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len() + descriptor.len());
        message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);
        message.extend_from_slice(&descriptor);
        signature::UnparsedPublicKey::new(&signature::ED25519, producer_key)
            .verify(&message, &producer_signature)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if wire.v != 1
            || wire.operation != "calendar_source_preview"
            || Uuid::parse_str(&wire.challenge_id).is_ok_and(|identifier| identifier.is_nil())
            || decode_exact(&wire.nonce, 32).is_err()
            || wire.person_id != person_id
            || wire.client_id != client_id
            || wire.device_id != device_id
            || wire.audience != producer.audience
            || wire.connector_id != connector_id
            || wire.connection_id != connection_id
            || wire.resource != resource
            || wire.execution_owner != producer.execution_owner
            || !valid_text(&wire.provider_identity, 256)
            || wire.issued_at_unix_ms <= 0
            || Uuid::parse_str(&wire.incarnation).is_err()
            || Uuid::parse_str(&wire.incarnation).is_ok_and(|identifier| identifier.is_nil())
            || wire.epoch == 0
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let source_authority = SourceAuthority::from_parts(
            Uuid::parse_str(&wire.incarnation).map_err(|_| AgentFailure::PolicyDenied)?,
            std::num::NonZeroU64::new(wire.epoch).ok_or(AgentFailure::PolicyDenied)?,
        )
        .ok_or(AgentFailure::PolicyDenied)?;
        Ok(RemoteCalendarSourceReference {
            person_id: wire.person_id,
            client_id: wire.client_id,
            device_id: wire.device_id,
            audience: wire.audience,
            connector_id: wire.connector_id,
            connection_id: wire.connection_id,
            execution_owner: wire.execution_owner,
            source_authority,
            resource: wire.resource,
            provider_identity: wire.provider_identity,
        })
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
            self.advance_remote_clock_and_cleanup(&transaction, now)
                .await?;
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
                    "INSERT INTO remote_authority_challenges (challenge_id, operation, admission_id, query_sha256, result_sha256, grant_id, grant_incarnation, grant_epoch, source_connector, source_connection, source_execution_owner, source_incarnation, source_epoch, max_items, max_bytes, expires_at_unix_ms, consumed_at_unix_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    vec![turso::Value::from(wire.challenge_id.clone()), turso::Value::from(wire.operation.clone()), turso::Value::from(String::new()), turso::Value::from(wire.query_sha256.clone()), turso::Value::from(wire.result_sha256.clone()), turso::Value::from(String::new()), turso::Value::from(String::new()), turso::Value::from(0i64), turso::Value::from(String::new()), turso::Value::from(String::new()), turso::Value::from(String::new()), turso::Value::from(String::new()), turso::Value::from(0i64), turso::Value::from(i64::from(wire.max_items)), turso::Value::from(i64::from(wire.max_bytes)), turso::Value::from(wire.expires_at_unix_ms), turso::Value::from(now)],
                )
                .await
                .map_err(storage)?;
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

    pub async fn remote_sign_pairing(
        &self,
        challenge: &RemotePairingChallenge,
        person_id: &str,
        client_id: &str,
        device_id: &str,
    ) -> Result<RemoteEnrollmentSignature, AgentFailure> {
        if person_id != self.person_id.to_string()
            || !valid_text(client_id, 128)
            || !valid_text(device_id, 128)
            || !valid_text(&challenge.pairing_id, 128)
            || Uuid::parse_str(&challenge.pairing_id).is_err()
            || Uuid::parse_str(&challenge.pairing_id).is_ok_and(|identifier| identifier.is_nil())
            || challenge.issuer != self.remote_owner_public_key().await?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        validate_producer(&challenge.producer).map_err(|_| AgentFailure::PolicyDenied)?;
        let encoded = decode_canonical(&challenge.challenge_b64url, MAX_CHALLENGE_BYTES)?;
        let producer_signature = decode_exact(&challenge.producer_signature, 64)?;
        let wire = parse_challenge(&encoded)?;
        if Uuid::parse_str(&challenge.challenge_id).is_ok_and(|identifier| identifier.is_nil())
            || Uuid::parse_str(&challenge.challenge_id).is_err()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AgentFailure::VaultUnavailable)?
            .as_millis() as i64;
        if challenge.expires_at_unix_ms != wire.expires_at_unix_ms
            || wire.expires_at_unix_ms <= now
            || wire.issued_at_unix_ms > now.saturating_add(5_000)
            || wire.operation != "enrollment"
            || wire.purpose != "owner_enrollment"
            || wire._consumer != "owner"
            || wire.challenge_id != challenge.challenge_id
            || wire.person_id != person_id
            || wire.client_id != client_id
            || wire.device_id != device_id
            || wire.key_id != challenge.issuer.key_id
            || wire.audience != challenge.producer.audience
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let producer_key = decode_exact(&challenge.producer.public_key, 32)?;
        let challenge_digest = encode_hex(&Sha256::digest(&encoded));
        let mut message = Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len() + encoded.len());
        message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);
        message.extend_from_slice(&encoded);
        signature::UnparsedPublicKey::new(&signature::ED25519, producer_key)
            .verify(&message, &producer_signature)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        self.validate_owner_key().await?;
        let (private_key, key_id) = self.load_owner_key().await?;
        if key_id != challenge.issuer.key_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result: Result<(), AgentFailure> = async {
            self.advance_remote_clock_and_cleanup(&transaction, now)
                .await?;
            let mut existing = transaction
                .query(
                    "SELECT challenge_id FROM remote_authority_challenges WHERE challenge_id = ?",
                    [challenge.pairing_id.clone()],
                )
                .await
                .map_err(storage)?;
            if existing.next().await.map_err(storage)?.is_some() {
                return Err(AgentFailure::Conflict);
            }
            transaction
                .execute(
                    "INSERT INTO remote_authority_challenges (challenge_id, operation, admission_id, query_sha256, result_sha256, grant_id, grant_incarnation, grant_epoch, source_connector, source_connection, source_execution_owner, source_incarnation, source_epoch, max_items, max_bytes, expires_at_unix_ms, consumed_at_unix_ms) VALUES (?, 'pairing', '', ?, ?, '', '', 0, '', '', '', '', 0, 0, 0, ?, ?)",
                    (
                        challenge.pairing_id.clone(),
                        challenge_digest,
                        challenge.producer.fingerprint.clone(),
                        challenge.expires_at_unix_ms,
                        now,
                    ),
                )
                .await
                .map_err(storage)?;
            Ok(())
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await?;
        let mut owner_message = Vec::with_capacity(OWNER_SIGNATURE_DOMAIN.len() + encoded.len());
        owner_message.extend_from_slice(OWNER_SIGNATURE_DOMAIN);
        owner_message.extend_from_slice(&encoded);
        let signature = private_key.sign(&owner_message);
        Ok(RemoteEnrollmentSignature {
            key_id,
            signature: URL_SAFE_NO_PAD.encode(signature.as_ref()),
        })
    }

    pub async fn finalize_remote_pairing(
        &self,
        pairing_id: &str,
        challenge: &RemotePairingChallenge,
        active: bool,
    ) -> Result<RemoteProducerIdentity, AgentFailure> {
        if !active {
            return Err(AgentFailure::PolicyDenied);
        }
        if pairing_id != challenge.pairing_id
            || !valid_text(pairing_id, 128)
            || Uuid::parse_str(&challenge.challenge_id).is_err()
            || Uuid::parse_str(&challenge.challenge_id).is_ok_and(|identifier| identifier.is_nil())
        {
            return Err(AgentFailure::PolicyDenied);
        }
        validate_producer(&challenge.producer).map_err(|_| AgentFailure::PolicyDenied)?;
        let encoded = decode_canonical(&challenge.challenge_b64url, MAX_CHALLENGE_BYTES)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let wire = parse_challenge(&encoded).map_err(|_| AgentFailure::PolicyDenied)?;
        let producer_signature = decode_exact(&challenge.producer_signature, 64)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AgentFailure::VaultUnavailable)?
            .as_millis() as i64;
        if wire.operation != "enrollment"
            || wire.challenge_id != challenge.challenge_id
            || wire.client_id != pairing_id
            || wire.audience != challenge.producer.audience
            || wire.purpose != "owner_enrollment"
            || wire._consumer != "owner"
            || wire.expires_at_unix_ms != challenge.expires_at_unix_ms
            || wire.expires_at_unix_ms <= now
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let producer_key = decode_exact(&challenge.producer.public_key, 32)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let mut producer_message =
            Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len() + encoded.len());
        producer_message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);
        producer_message.extend_from_slice(&encoded);
        signature::UnparsedPublicKey::new(&signature::ED25519, producer_key)
            .verify(&producer_message, &producer_signature)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let challenge_digest = encode_hex(&Sha256::digest(&encoded));
        let mut rows = self
            .connection()?
            .query(
                "SELECT operation, query_sha256, result_sha256, expires_at_unix_ms FROM remote_authority_challenges WHERE challenge_id = ?",
                [pairing_id],
            )
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let row = rows
            .next()
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .ok_or(AgentFailure::PolicyDenied)?;
        let operation: String = row.get(0).map_err(|_| AgentFailure::PolicyDenied)?;
        let stored_digest: String = row.get(1).map_err(|_| AgentFailure::PolicyDenied)?;
        let stored_fingerprint: String = row.get(2).map_err(|_| AgentFailure::PolicyDenied)?;
        let stored_expiry: i64 = row.get(3).map_err(|_| AgentFailure::PolicyDenied)?;
        if operation != "pairing"
            || stored_digest != challenge_digest
            || stored_fingerprint != challenge.producer.fingerprint
            || stored_expiry != challenge.expires_at_unix_ms
        {
            return Err(AgentFailure::PolicyDenied);
        }
        self.remote_pin_producer(challenge.producer.clone()).await?;
        Ok(challenge.producer.clone())
    }

    pub async fn remote_sign_calendar_authorization(
        &self,
        expected: &RemoteCalendarAuthorizationExpectation,
        challenge_b64url: &str,
        producer_signature_b64url: &str,
    ) -> Result<RemoteEnrollmentSignature, AgentFailure> {
        if !valid_text(&expected.client_id, 128) || !valid_text(&expected.device_id, 128) {
            return Err(AgentFailure::InvalidInput);
        }
        let challenge = decode_canonical(challenge_b64url, MAX_CHALLENGE_BYTES)?;
        let producer_signature = decode_exact(producer_signature_b64url, 64)?;
        let wire = parse_challenge(&challenge)?;
        if wire.operation != expected.operation
            || wire.challenge_id != expected.challenge_id
            || wire.client_id != expected.client_id
            || wire.device_id != expected.device_id
            || wire.admission_id != expected.admission_id
            || wire.query_sha256 != expected.query_sha256
            || wire.result_sha256 != expected.result_sha256
            || wire.grant.as_ref().is_none_or(|grant| {
                grant.id != expected.grant_id
                    || grant.incarnation != expected.grant_incarnation
                    || grant.epoch != expected.grant_epoch
            })
            || wire.source.as_ref().is_none_or(|source| {
                source.connector_id != expected.source_connector
                    || source.connection_id != expected.source_connection
                    || source.execution_owner != expected.source_execution_owner
                    || source.incarnation != expected.source_incarnation
                    || source.epoch != expected.source_epoch
            })
            || wire.resources != expected.resources
            || wire.max_items != expected.max_items
            || wire.max_bytes != expected.max_bytes
            || (expected.operation != "admission" && expected.operation != "release")
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if wire.person_id != self.person_id.to_string() {
            return Err(AgentFailure::PolicyDenied);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AgentFailure::VaultUnavailable)?
            .as_millis() as i64;
        if wire.expires_at_unix_ms <= now || wire.issued_at_unix_ms > now.saturating_add(5_000) {
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
            self.advance_remote_clock_and_cleanup(&transaction, now)
                .await?;
            self.verify_pinned_producer_in_transaction(
                &transaction,
                &wire,
                &challenge,
                &producer_signature,
            )
            .await?;
            if wire.operation == "release" {
                self.validate_release_binding_in_transaction(&transaction, &wire, now)
                    .await?;
            }
            self.validate_remote_calendar_grant_in_transaction(&transaction, &wire)
                .await?;
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
                    "INSERT INTO remote_authority_challenges (challenge_id, operation, admission_id, query_sha256, result_sha256, grant_id, grant_incarnation, grant_epoch, source_connector, source_connection, source_execution_owner, source_incarnation, source_epoch, max_items, max_bytes, expires_at_unix_ms, consumed_at_unix_ms) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    vec![turso::Value::from(wire.challenge_id.clone()), turso::Value::from(wire.operation.clone()), turso::Value::from(wire.admission_id.clone()), turso::Value::from(wire.query_sha256.clone()), turso::Value::from(wire.result_sha256.clone()), turso::Value::from(wire.grant.as_ref().map_or(String::new(), |grant| grant.id.clone())), turso::Value::from(wire.grant.as_ref().map_or(String::new(), |grant| grant.incarnation.clone())), turso::Value::from(wire.grant.as_ref().map_or(0i64, |grant| grant.epoch as i64)), turso::Value::from(wire.source.as_ref().map_or(String::new(), |source| source.connector_id.clone())), turso::Value::from(wire.source.as_ref().map_or(String::new(), |source| source.connection_id.clone())), turso::Value::from(wire.source.as_ref().map_or(String::new(), |source| source.execution_owner.clone())), turso::Value::from(wire.source.as_ref().map_or(String::new(), |source| source.incarnation.clone())), turso::Value::from(wire.source.as_ref().map_or(0i64, |source| source.epoch as i64)), turso::Value::from(i64::from(wire.max_items)), turso::Value::from(i64::from(wire.max_bytes)), turso::Value::from(wire.expires_at_unix_ms), turso::Value::from(now)],
                )
                .await
                .map_err(storage)?;
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

    async fn validate_remote_calendar_grant_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        wire: &ChallengeWire,
    ) -> Result<(), AgentFailure> {
        let policy = wire.policy.as_ref().ok_or(AgentFailure::PolicyDenied)?;
        if Uuid::parse_str(&policy.incarnation).is_err() || policy.epoch == 0 {
            return Err(AgentFailure::PolicyDenied);
        }
        let source = wire.source.as_ref().ok_or(AgentFailure::PolicyDenied)?;
        let grant_wire = wire.grant.as_ref().ok_or(AgentFailure::PolicyDenied)?;
        if Uuid::parse_str(&grant_wire.id)
            .map_err(|_| AgentFailure::PolicyDenied)?
            .is_nil()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut policy_rows = transaction
            .query(
                "SELECT policy_incarnation, policy_epoch FROM remote_calendar_grant_mappings WHERE grant_id = ? AND person_id = ?",
                (grant_wire.id.clone(), self.person_id.to_string()),
            )
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let policy_row = policy_rows
            .next()
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .ok_or(AgentFailure::PolicyDenied)?;
        let mapped_policy_incarnation = policy_row
            .get::<String>(0)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let mapped_policy_epoch = policy_row
            .get::<i64>(1)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if mapped_policy_incarnation != policy.incarnation
            || mapped_policy_epoch <= 0
            || mapped_policy_epoch as u64 != policy.epoch
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut grant_rows = transaction
            .query(
                "SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ? AND authority_owner = ?",
                (grant_wire.id.clone(), self.person_id.to_string(), self.vault_id.to_string()),
            )
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let grant_row = grant_rows
            .next()
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .ok_or(AgentFailure::PolicyDenied)?;
        let grant_payload = grant_row
            .get::<String>(11)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let grant: floe_access::DataAccessGrant =
            serde_json::from_str(&grant_payload).map_err(|_| AgentFailure::PolicyDenied)?;
        if grant.state() != GrantState::Active
            || grant.review_required()
            || grant.source().person_id() != self.person_id
            || grant.source().connection_id().as_str() != source.connection_id
            || grant.source().connector().as_str() != source.connector_id
            || grant.source().execution_owner().as_str() != source.execution_owner
            || grant.source().source_authority().incarnation().to_string() != source.incarnation
            || grant.source().source_authority().epoch().get() != source.epoch
            || grant.authority().incarnation().to_string() != grant_wire.incarnation
            || grant.authority().access_epoch().get() != grant_wire.epoch
            || !grant.scope().operations().contains(&GrantOperation::Read)
            || !grant
                .scope()
                .resources()
                .iter()
                .map(|resource| resource.as_str())
                .eq(wire.resources.iter().map(String::as_str))
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let purpose = match wire.purpose.as_str() {
            "quick_response" | "everyday_assistance" => GrantPurpose::Assistant,
            "scheduling" => GrantPurpose::Scheduling,
            "summarization" => GrantPurpose::Summarization,
            _ => return Err(AgentFailure::PolicyDenied),
        };
        if !grant.scope().purposes().contains(&purpose)
            || !grant
                .scope()
                .consumers()
                .iter()
                .any(|consumer| consumer.identifier() == wire._consumer)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        match grant.scope().processing() {
            ProcessingRestriction::LocalOnly => {}
            ProcessingRestriction::ApprovedRecipient { recipient, .. }
                if recipient != &wire.audience =>
            {
                return Err(AgentFailure::PolicyDenied);
            }
            ProcessingRestriction::ApprovedRecipient { .. } => {}
        }
        Ok(())
    }

    async fn advance_remote_clock_and_cleanup(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        now: i64,
    ) -> Result<(), AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT last_now_unix_ms FROM remote_authority_clock WHERE id = 1",
                (),
            )
            .await
            .map_err(storage)?;
        let last_now = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<i64>(0)
            .map_err(storage)?;
        if now < last_now {
            return Err(AgentFailure::VaultUnavailable);
        }
        transaction
            .execute(
                "UPDATE remote_authority_clock SET last_now_unix_ms = ? WHERE id = 1",
                [now],
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "DELETE FROM remote_authority_challenges WHERE expires_at_unix_ms < ?",
                [now.saturating_sub(35_000)],
            )
            .await
            .map_err(storage)?;
        Ok(())
    }

    async fn validate_release_binding_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        wire: &ChallengeWire,
        now: i64,
    ) -> Result<(), AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT operation, query_sha256, result_sha256, grant_id, grant_incarnation, grant_epoch, source_connector, source_connection, source_execution_owner, source_incarnation, source_epoch, max_items, max_bytes, expires_at_unix_ms FROM remote_authority_challenges WHERE challenge_id = ? AND operation = 'admission'",
                [wire.admission_id.clone()],
            )
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let row = rows
            .next()
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .ok_or(AgentFailure::PolicyDenied)?;
        let fields = (
            row.get::<String>(1),
            row.get::<String>(2),
            row.get::<String>(3),
            row.get::<String>(4),
            row.get::<i64>(5),
            row.get::<String>(6),
            row.get::<String>(7),
            row.get::<String>(8),
            row.get::<String>(9),
            row.get::<i64>(10),
            row.get::<i64>(11),
            row.get::<i64>(12),
            row.get::<i64>(13),
        );
        let (
            Ok(query_digest),
            Ok(result_digest),
            Ok(grant_id),
            Ok(grant_incarnation),
            Ok(grant_epoch),
            Ok(source_connector),
            Ok(source_connection),
            Ok(source_owner),
            Ok(source_incarnation),
            Ok(source_epoch),
            Ok(max_items),
            Ok(max_bytes),
            Ok(admission_expires),
        ) = fields
        else {
            return Err(AgentFailure::PolicyDenied);
        };
        let source = wire.source.as_ref().ok_or(AgentFailure::PolicyDenied)?;
        let grant = wire.grant.as_ref().ok_or(AgentFailure::PolicyDenied)?;
        if !result_digest.is_empty()
            || query_digest != wire.query_sha256
            || grant_id != grant.id
            || grant_incarnation != grant.incarnation
            || grant_epoch as u64 != grant.epoch
            || source_connector != source.connector_id
            || source_connection != source.connection_id
            || source_owner != source.execution_owner
            || source_incarnation != source.incarnation
            || source_epoch as u64 != source.epoch
            || max_items as u64 != u64::from(wire.max_items)
            || max_bytes as u64 != u64::from(wire.max_bytes)
            || admission_expires <= now
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    async fn verify_pinned_producer_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        wire: &ChallengeWire,
        challenge: &[u8],
        producer_signature: &[u8],
    ) -> Result<(), AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT identity_json FROM remote_authority_producer WHERE id = 1",
                (),
            )
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let row = rows
            .next()
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .ok_or(AgentFailure::PolicyDenied)?;
        let identity_json = row
            .get::<String>(0)
            .map_err(|_| AgentFailure::PolicyDenied)?;
        strict_json(&identity_json, MAX_PRODUCER_PROOF_BYTES)?;
        let identity: RemoteProducerIdentity =
            serde_json::from_str(&identity_json).map_err(|_| AgentFailure::PolicyDenied)?;
        validate_producer(&identity).map_err(|_| AgentFailure::PolicyDenied)?;
        if identity.audience != wire.audience
            || wire
                .source
                .as_ref()
                .is_none_or(|source| source.execution_owner != identity.execution_owner)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let producer_key = decode_exact(&identity.public_key, 32)?;
        let mut message = Vec::with_capacity(PRODUCER_SIGNATURE_DOMAIN.len() + challenge.len());
        message.extend_from_slice(PRODUCER_SIGNATURE_DOMAIN);
        message.extend_from_slice(challenge);
        signature::UnparsedPublicKey::new(&signature::ED25519, producer_key)
            .verify(&message, producer_signature)
            .map_err(|_| AgentFailure::PolicyDenied)
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
        if wire.policy.is_some()
            || wire.source.is_some()
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
        let policy = wire.policy.as_ref().ok_or(AgentFailure::InvalidInput)?;
        let source = wire.source.as_ref().ok_or(AgentFailure::InvalidInput)?;
        let grant = wire.grant.as_ref().ok_or(AgentFailure::InvalidInput)?;
        if Uuid::parse_str(&policy.incarnation).is_err()
            || policy.epoch == 0
            || source.connector_id.is_empty()
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

    // FIXME(stage-2): glob import of the retired floe-domain crate
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

    #[tokio::test]
    async fn pairing_proof_is_strict_single_use_and_pins_only_after_activation() {
        let root = tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys)
            .await
            .unwrap();
        let producer_key = signature::Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let producer_pair = signature::Ed25519KeyPair::from_pkcs8(producer_key.as_ref()).unwrap();
        let producer_public = producer_pair.public_key().as_ref();
        let producer = RemoteProducerIdentity {
            schema_version: 1,
            instance_id: "00000000-0000-4000-8000-000000000031".into(),
            execution_owner: "00000000-0000-4000-8000-000000000032".into(),
            audience: "floe.server:00000000-0000-4000-8000-000000000031".into(),
            key_id: "00000000-0000-4000-8000-000000000033".into(),
            public_key: URL_SAFE_NO_PAD.encode(producer_public),
            fingerprint: encode_hex(&Sha256::digest(producer_public)),
        };
        let issuer = vault.remote_owner_public_key().await.unwrap();
        let pairing_id = "00000000-0000-4000-8000-000000000034";
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let descriptor = serde_json::json!({
            "v": 1,
            "operation": "enrollment",
            "challenge_id": pairing_id,
            "nonce": URL_SAFE_NO_PAD.encode([3u8; 32]),
            "key_id": issuer.key_id,
            "person_id": person_id.to_string(),
            "client_id": pairing_id,
            "device_id": "pairing-device",
            "audience": producer.audience,
            "purpose": "owner_enrollment",
            "consumer": "owner",
            "issued_at_unix_ms": now,
            "expires_at_unix_ms": now + 30_000,
        });
        let descriptor_bytes = serde_json::to_vec(&descriptor).unwrap();
        let mut producer_message = Vec::from(PRODUCER_SIGNATURE_DOMAIN);
        producer_message.extend_from_slice(&descriptor_bytes);
        let producer_signature = producer_pair.sign(&producer_message);
        let challenge = RemotePairingChallenge {
            pairing_id: pairing_id.into(),
            challenge_id: pairing_id.into(),
            challenge_b64url: URL_SAFE_NO_PAD.encode(&descriptor_bytes),
            producer_signature: URL_SAFE_NO_PAD.encode(producer_signature.as_ref()),
            producer,
            issuer,
            expires_at_unix_ms: now + 30_000,
        };
        assert!(vault.remote_pinned_producer().await.is_err());
        let proof = vault
            .remote_sign_pairing(
                &challenge,
                &person_id.to_string(),
                pairing_id,
                "pairing-device",
            )
            .await
            .unwrap();
        assert_eq!(proof.key_id, challenge.issuer.key_id);
        assert_eq!(
            vault
                .remote_sign_pairing(
                    &challenge,
                    &person_id.to_string(),
                    pairing_id,
                    "pairing-device",
                )
                .await,
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            vault
                .finalize_remote_pairing(&challenge.pairing_id, &challenge, false)
                .await,
            Err(AgentFailure::PolicyDenied)
        );
        assert!(vault.remote_pinned_producer().await.is_err());
        vault
            .finalize_remote_pairing(&challenge.pairing_id, &challenge, true)
            .await
            .unwrap();
        vault
            .finalize_remote_pairing(&challenge.pairing_id, &challenge, true)
            .await
            .unwrap();
        let mut changed = challenge.clone();
        changed.challenge_id = "00000000-0000-4000-8000-000000000035".into();
        assert_eq!(
            vault
                .remote_sign_pairing(
                    &changed,
                    &person_id.to_string(),
                    pairing_id,
                    "pairing-device",
                )
                .await,
            Err(AgentFailure::PolicyDenied)
        );
        let mut changed_producer = challenge.clone();
        changed_producer.producer.fingerprint = "changed".into();
        assert_eq!(
            vault
                .finalize_remote_pairing(&changed_producer.pairing_id, &changed_producer, true,)
                .await,
            Err(AgentFailure::PolicyDenied)
        );
        let mut changed_challenge = challenge.clone();
        changed_challenge.challenge_id = "00000000-0000-4000-8000-000000000035".into();
        assert_eq!(
            vault
                .finalize_remote_pairing(&changed_challenge.pairing_id, &changed_challenge, true)
                .await,
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[tokio::test]
    async fn signed_calendar_source_preview_binds_pairing_and_exact_source() {
        let root = tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys)
            .await
            .unwrap();
        let producer_key = signature::Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let producer_pair = signature::Ed25519KeyPair::from_pkcs8(producer_key.as_ref()).unwrap();
        let producer_public = producer_pair.public_key().as_ref();
        let producer = RemoteProducerIdentity {
            schema_version: 1,
            instance_id: "00000000-0000-4000-8000-000000000021".into(),
            execution_owner: "00000000-0000-4000-8000-000000000022".into(),
            audience: "floe.server:00000000-0000-4000-8000-000000000021".into(),
            key_id: "00000000-0000-4000-8000-000000000023".into(),
            public_key: URL_SAFE_NO_PAD.encode(producer_public),
            fingerprint: encode_hex(&Sha256::digest(producer_public)),
        };
        vault.remote_pin_producer(producer.clone()).await.unwrap();
        let descriptor = serde_json::json!({
            "v": 1,
            "operation": "calendar_source_preview",
            "challenge_id": "00000000-0000-4000-8000-000000000024",
            "nonce": URL_SAFE_NO_PAD.encode([7u8; 32]),
            "person_id": person_id.to_string(),
            "client_id": "paired-client",
            "device_id": "paired-device",
            "audience": producer.audience,
            "connector_id": "calendar.google",
            "connection_id": "00000000-0000-4000-8000-000000000025",
            "execution_owner": producer.execution_owner,
            "incarnation": "00000000-0000-4000-8000-000000000026",
            "epoch": 7,
            "resource": "primary",
            "provider_identity": "google:subject-a",
            "issued_at_unix_ms": 1_700_000_000_000i64,
        });
        let descriptor_bytes = serde_json::to_vec(&descriptor).unwrap();
        let mut signed = Vec::from(PRODUCER_SIGNATURE_DOMAIN);
        signed.extend_from_slice(&descriptor_bytes);
        let signature = producer_pair.sign(&signed);
        let reference = vault
            .verify_remote_calendar_source_preview(
                &URL_SAFE_NO_PAD.encode(&descriptor_bytes),
                &URL_SAFE_NO_PAD.encode(signature.as_ref()),
                &person_id.to_string(),
                "paired-client",
                "paired-device",
                "calendar.google",
                "00000000-0000-4000-8000-000000000025",
                "primary",
            )
            .await
            .unwrap();
        assert_eq!(reference.provider_identity, "google:subject-a");
        assert_eq!(reference.source_authority.epoch().get(), 7);
        let mut changed = descriptor.clone();
        changed["resource"] = serde_json::Value::String("other".into());
        let changed_bytes = serde_json::to_vec(&changed).unwrap();
        assert!(
            vault
                .verify_remote_calendar_source_preview(
                    &URL_SAFE_NO_PAD.encode(&changed_bytes),
                    &URL_SAFE_NO_PAD.encode(signature.as_ref()),
                    &person_id.to_string(),
                    "paired-client",
                    "paired-device",
                    "calendar.google",
                    "00000000-0000-4000-8000-000000000025",
                    "primary",
                )
                .await
                .is_err()
        );
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

    #[tokio::test]
    async fn remote_calendar_admission_and_release_are_grant_fenced_for_google_and_microsoft() {
        for (connector, connection_id, resource) in [
            (
                "calendar.google",
                "00000000-0000-4000-8000-000000000101",
                "primary",
            ),
            (
                "calendar.microsoft",
                "00000000-0000-4000-8000-000000000102",
                "work",
            ),
        ] {
            let root = tempdir().unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let person_id = PersonId::new();
            let keys = TestKeys::default();
            let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
                .await
                .unwrap();
            let producer_pkcs8 =
                signature::Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
            let producer_pair =
                signature::Ed25519KeyPair::from_pkcs8(producer_pkcs8.as_ref()).unwrap();
            let producer_instance = if connector.ends_with("google") {
                "00000000-0000-4000-8000-000000000111"
            } else {
                "00000000-0000-4000-8000-000000000112"
            };
            let audience = format!("floe.server:{producer_instance}");
            let producer = RemoteProducerIdentity {
                schema_version: 1,
                instance_id: producer_instance.into(),
                execution_owner: if connector.ends_with("google") {
                    "00000000-0000-4000-8000-000000000121".into()
                } else {
                    "00000000-0000-4000-8000-000000000122".into()
                },
                audience: audience.clone(),
                key_id: Uuid::new_v4().to_string(),
                public_key: URL_SAFE_NO_PAD.encode(producer_pair.public_key().as_ref()),
                fingerprint: encode_hex(&Sha256::digest(producer_pair.public_key().as_ref())),
            };
            vault.remote_pin_producer(producer.clone()).await.unwrap();
            let source_authority = SourceAuthority::new();
            let source = GrantSourceBinding::try_new(
                person_id,
                ConnectionId::try_new(connection_id).unwrap(),
                ConnectorId::try_new(connector).unwrap(),
                ExecutionOwnerId::try_new(producer.execution_owner.clone()).unwrap(),
                source_authority,
            )
            .unwrap();
            let consumer = GrantConsumer::builtin("calendar.expert").unwrap();
            let scope = GrantScope::try_new(
                vec![ResourceHandle::try_new(resource).unwrap()],
                vec![GrantDataCategory::Content],
                vec![GrantOperation::Read],
                vec![GrantPurpose::Assistant],
                vec![consumer.clone()],
                ProcessingRestriction::LocalOnly,
            )
            .unwrap();
            let grant = vault
                .review_and_activate_remote_calendar_grant(
                    GrantId::new(),
                    None,
                    source.clone(),
                    scope.clone(),
                    None,
                )
                .await
                .unwrap();
            let binding = vault
                .remote_calendar_grant_binding(connector, connection_id, source_authority, resource)
                .await
                .unwrap();
            let owner_key = vault.remote_owner_public_key().await.unwrap();
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            let query_digest = encode_hex(&Sha256::digest(br#"{"limit":1}"#));
            let base_challenge = |operation: &str,
                                  challenge_id: Uuid,
                                  admission_id: &str,
                                  result_digest: &str| {
                serde_json::json!({
                    "v": 1,
                    "operation": operation,
                    "challenge_id": challenge_id.to_string(),
                    "nonce": URL_SAFE_NO_PAD.encode([7u8; 32]),
                    "key_id": owner_key.key_id,
                    "person_id": person_id.to_string(),
                    "client_id": "paired-client",
                    "device_id": "paired-device",
                    "audience": audience,
                    "purpose": "everyday_assistance",
                    "consumer": consumer.identifier(),
                    "policy": {"incarnation": binding.consumer_policy.incarnation().to_string(), "epoch": binding.consumer_policy.epoch().get()},
                    "source": {"connector_id": connector, "connection_id": connection_id, "execution_owner": source.execution_owner().as_str(), "incarnation": source_authority.incarnation().to_string(), "epoch": source_authority.epoch().get()},
                    "grant": {"id": grant.id().as_uuid().to_string(), "incarnation": grant.authority().incarnation().to_string(), "epoch": grant.authority().access_epoch().get()},
                    "resources": [resource],
                    "query_sha256": query_digest,
                    "max_items": 1,
                    "max_bytes": 4096,
                    "result_sha256": result_digest,
                    "admission_id": admission_id,
                    "issued_at_unix_ms": now,
                    "expires_at_unix_ms": now + 30_000
                })
            };
            let sign_challenge = |challenge: &serde_json::Value| {
                let bytes = serde_json::to_vec(challenge).unwrap();
                let mut message = Vec::from(PRODUCER_SIGNATURE_DOMAIN);
                message.extend_from_slice(&bytes);
                (bytes, producer_pair.sign(&message))
            };
            let admission_id = Uuid::new_v4();
            let admission = base_challenge("admission", admission_id, "", "");
            let (admission_bytes, admission_signature) = sign_challenge(&admission);
            let admission_expected = RemoteCalendarAuthorizationExpectation {
                operation: "admission".into(),
                client_id: "paired-client".into(),
                device_id: "paired-device".into(),
                challenge_id: admission_id.to_string(),
                admission_id: String::new(),
                query_sha256: query_digest.clone(),
                result_sha256: String::new(),
                grant_id: grant.id().as_uuid().to_string(),
                grant_incarnation: grant.authority().incarnation().to_string(),
                grant_epoch: grant.authority().access_epoch().get(),
                source_connector: connector.into(),
                source_connection: connection_id.into(),
                source_execution_owner: source.execution_owner().as_str().into(),
                source_incarnation: source_authority.incarnation().to_string(),
                source_epoch: source_authority.epoch().get(),
                resources: vec![resource.into()],
                max_items: 1,
                max_bytes: 4096,
            };
            let owner_signature = vault
                .remote_sign_calendar_authorization(
                    &admission_expected,
                    &URL_SAFE_NO_PAD.encode(&admission_bytes),
                    &URL_SAFE_NO_PAD.encode(admission_signature.as_ref()),
                )
                .await
                .unwrap();
            assert!(!owner_signature.signature.is_empty());

            let result_digest = encode_hex(&Sha256::digest(b"calendar-result"));
            let release_id = Uuid::new_v4();
            let release = base_challenge(
                "release",
                release_id,
                &admission_id.to_string(),
                &result_digest,
            );
            let (release_bytes, release_signature) = sign_challenge(&release);
            let mut release_expected = admission_expected.clone();
            release_expected.operation = "release".into();
            release_expected.challenge_id = release_id.to_string();
            release_expected.admission_id = admission_id.to_string();
            release_expected.result_sha256 = result_digest;
            assert!(
                vault
                    .remote_sign_calendar_authorization(
                        &release_expected,
                        &URL_SAFE_NO_PAD.encode(&release_bytes),
                        &URL_SAFE_NO_PAD.encode(release_signature.as_ref()),
                    )
                    .await
                    .is_ok()
            );
            assert_eq!(
                vault
                    .remote_sign_calendar_authorization(
                        &release_expected,
                        &URL_SAFE_NO_PAD.encode(&release_bytes),
                        &URL_SAFE_NO_PAD.encode(release_signature.as_ref()),
                    )
                    .await,
                Err(AgentFailure::Conflict)
            );

            let mut altered = admission_expected.clone();
            altered.query_sha256 = "0".repeat(64);
            assert_eq!(
                vault
                    .remote_sign_calendar_authorization(
                        &altered,
                        &URL_SAFE_NO_PAD.encode(&admission_bytes),
                        &URL_SAFE_NO_PAD.encode(admission_signature.as_ref()),
                    )
                    .await,
                Err(AgentFailure::PolicyDenied)
            );
            let mut wrong_recipient = admission.clone();
            wrong_recipient["audience"] = serde_json::json!("floe.server:wrong-recipient");
            let (wrong_recipient_bytes, wrong_recipient_signature) =
                sign_challenge(&wrong_recipient);
            assert_eq!(
                vault
                    .remote_sign_calendar_authorization(
                        &admission_expected,
                        &URL_SAFE_NO_PAD.encode(&wrong_recipient_bytes),
                        &URL_SAFE_NO_PAD.encode(wrong_recipient_signature.as_ref()),
                    )
                    .await,
                Err(AgentFailure::PolicyDenied)
            );
            let mut wrong_resource = admission.clone();
            wrong_resource["resources"] = serde_json::json!(["different-calendar"]);
            let (wrong_resource_bytes, wrong_resource_signature) = sign_challenge(&wrong_resource);
            assert_eq!(
                vault
                    .remote_sign_calendar_authorization(
                        &admission_expected,
                        &URL_SAFE_NO_PAD.encode(&wrong_resource_bytes),
                        &URL_SAFE_NO_PAD.encode(wrong_resource_signature.as_ref()),
                    )
                    .await,
                Err(AgentFailure::PolicyDenied)
            );
            let second_admission_id = Uuid::new_v4();
            let second_admission = base_challenge("admission", second_admission_id, "", "");
            let (second_admission_bytes, second_admission_signature) =
                sign_challenge(&second_admission);
            let mut second_admission_expected = admission_expected.clone();
            second_admission_expected.challenge_id = second_admission_id.to_string();
            let overflow_connection = vault.database.connect().unwrap();
            for index in 0..128 {
                overflow_connection
                    .execute(
                        &format!(
                            "INSERT INTO remote_authority_challenges (challenge_id, operation, admission_id, query_sha256, result_sha256, grant_id, grant_incarnation, grant_epoch, source_connector, source_connection, source_execution_owner, source_incarnation, source_epoch, max_items, max_bytes, expires_at_unix_ms, consumed_at_unix_ms) VALUES ('overflow-{index}', 'admission', '', '', '', '', '', 1, '', '', '', '', 1, 1, 1, {}, {})",
                            now - 60_000,
                            now - 60_000
                        ),
                        (),
                    )
                    .await
                    .unwrap();
            }
            vault
                .remote_sign_calendar_authorization(
                    &second_admission_expected,
                    &URL_SAFE_NO_PAD.encode(&second_admission_bytes),
                    &URL_SAFE_NO_PAD.encode(second_admission_signature.as_ref()),
                )
                .await
                .unwrap();
            let mut overflow_rows = overflow_connection
                .query("SELECT COUNT(*) FROM remote_authority_challenges", ())
                .await
                .unwrap();
            assert!(
                overflow_rows
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .get::<i64>(0)
                    .unwrap()
                    <= 3
            );
            let second_result_digest = encode_hex(&Sha256::digest(b"second-calendar-result"));
            let second_release_id = Uuid::new_v4();
            let second_release = base_challenge(
                "release",
                second_release_id,
                &second_admission_id.to_string(),
                &second_result_digest,
            );
            let (second_release_bytes, second_release_signature) = sign_challenge(&second_release);
            let mut second_release_expected = second_admission_expected;
            second_release_expected.operation = "release".into();
            second_release_expected.challenge_id = second_release_id.to_string();
            second_release_expected.admission_id = second_admission_id.to_string();
            second_release_expected.result_sha256 = second_result_digest;
            let paused = vault
                .pause_remote_calendar_grant(grant.id(), grant.authority())
                .await
                .unwrap();
            assert_eq!(
                vault
                    .remote_sign_calendar_authorization(
                        &second_release_expected,
                        &URL_SAFE_NO_PAD.encode(&second_release_bytes),
                        &URL_SAFE_NO_PAD.encode(second_release_signature.as_ref()),
                    )
                    .await,
                Err(AgentFailure::PolicyDenied)
            );
            assert_ne!(paused.authority(), grant.authority());
            drop(paused);
            let corruption_connection = vault.database.connect().unwrap();
            corruption_connection
                .execute(
                    "UPDATE remote_calendar_grant_mappings SET payload = '{}' WHERE grant_id = ?",
                    [grant.id().as_uuid().to_string()],
                )
                .await
                .unwrap();
            drop(vault);
            assert!(
                EncryptedAgentVault::open(root.path(), person_id, keys)
                    .await
                    .is_err()
            );
        }
    }
}
