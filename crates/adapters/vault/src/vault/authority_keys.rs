//! Operation-scoped wrapped signing keys. Retention is not live authority.
use super::gateway_pairing_store::{expectation_on, pairing_on};
use super::{EncryptedAgentVault, VaultKeyProvider, storage};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use floe_access::{GatewayCredentialExpectation, RemoteOwnerPublicKey};
use floe_agent_contract::AgentFailure;
use floe_connections::PairingRecord;
use ring::{
    aead, hkdf,
    rand::{SecureRandom, SystemRandom},
    signature::{self, KeyPair},
};
use turso::Connection;
use uuid::Uuid;
use zeroize::Zeroizing;

const ENROLLMENT_WRAP_CONTEXT: &[u8] = b"floe.remote.enrollment-key.wrap.v1\0";

pub(super) struct WrappedEnrollmentKey {
    pub issuer: RemoteOwnerPublicKey,
    pub nonce: String,
    pub ciphertext: String,
}

fn enrollment_aad(operation: &PairingRecord, issuer: &RemoteOwnerPublicKey) -> Vec<u8> {
    let mut aad = ENROLLMENT_WRAP_CONTEXT.to_vec();
    for value in [
        operation.person_id.to_string(),
        operation.device_id.clone(),
        operation.operation_id.to_string(),
        issuer.key_id.clone(),
        issuer.public_key.clone(),
    ] {
        aad.extend_from_slice(&(value.len() as u64).to_be_bytes());
        aad.extend_from_slice(value.as_bytes());
    }
    aad
}

pub(super) async fn enrollment_issuer_on(
    connection: &Connection,
    operation: Uuid,
) -> Result<RemoteOwnerPublicKey, AgentFailure> {
    let mut rows = connection.query(
        "SELECT issuer_key_id,issuer_public_key FROM gateway_pairing_private WHERE operation_id=?",
        (operation.to_string(),),
    ).await.map_err(storage)?;
    let row = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?;
    let issuer = RemoteOwnerPublicKey {
        key_id: row.get(0).map_err(storage)?,
        public_key: row.get(1).map_err(storage)?,
    };
    let key_id = Uuid::parse_str(&issuer.key_id).map_err(|_| AgentFailure::VaultUnavailable)?;
    if key_id.is_nil()
        || key_id.to_string() != issuer.key_id
        || rows.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    decode_exact(&issuer.public_key, 32).map_err(|_| AgentFailure::VaultUnavailable)?;
    Ok(issuer)
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) fn generate_enrollment_key(
        &self,
        operation: &PairingRecord,
    ) -> Result<WrappedEnrollmentKey, AgentFailure> {
        self.check_access()?;
        if operation.person_id != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let random = SystemRandom::new();
        let pkcs8 = signature::Ed25519KeyPair::generate_pkcs8(&random)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let issuer = RemoteOwnerPublicKey {
            key_id: Uuid::new_v4().to_string(),
            public_key: URL_SAFE_NO_PAD.encode(key_pair.public_key().as_ref()),
        };
        let mut nonce = [0u8; 12];
        random
            .fill(&mut nonce)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let wrapping_key = self.enrollment_wrapping_key()?;
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, wrapping_key.as_ref())
                .map_err(|_| AgentFailure::VaultUnavailable)?,
        );
        let mut ciphertext = Zeroizing::new(pkcs8.as_ref().to_vec());
        key.seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(enrollment_aad(operation, &issuer)),
            &mut *ciphertext,
        )
        .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(WrappedEnrollmentKey {
            issuer,
            nonce: URL_SAFE_NO_PAD.encode(nonce),
            ciphertext: URL_SAFE_NO_PAD.encode(ciphertext.as_slice()),
        })
    }

    fn enrollment_wrapping_key(&self) -> Result<Zeroizing<[u8; 32]>, AgentFailure> {
        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, b"floe.remote.enrollment-key.salt.v1\0");
        let pseudo = salt.extract(self.key.as_bytes());
        let info = [ENROLLMENT_WRAP_CONTEXT];
        let output = pseudo
            .expand(&info, hkdf::HKDF_SHA256)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let mut key = Zeroizing::new([0u8; 32]);
        output
            .fill(key.as_mut())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(key)
    }

    /// Loads immutable material only. Callers must prove the required live phase
    /// in this same transaction before signing; historic recovery may read it.
    pub(super) async fn enrollment_key_on(
        &self,
        connection: &Connection,
        operation_id: Uuid,
    ) -> Result<(signature::Ed25519KeyPair, RemoteOwnerPublicKey), AgentFailure> {
        self.check_access()?;
        let operation = pairing_on(connection, self.person_id, operation_id)
            .await?
            .ok_or(AgentFailure::VaultUnavailable)?;
        let issuer = enrollment_issuer_on(connection, operation_id).await?;
        let mut rows = connection.query(
            "SELECT issuer_nonce,issuer_ciphertext FROM gateway_pairing_private WHERE operation_id=?",
            (operation_id.to_string(),),
        ).await.map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        let nonce = decode_exact(&row.get::<String>(0).map_err(storage)?, 12)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let mut ciphertext = Zeroizing::new(
            decode_canonical(&row.get::<String>(1).map_err(storage)?, 4096)
                .map_err(|_| AgentFailure::VaultUnavailable)?,
        );
        let nonce_bytes: [u8; 12] = nonce
            .try_into()
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let wrapping_key = self.enrollment_wrapping_key()?;
        let key = aead::LessSafeKey::new(
            aead::UnboundKey::new(&aead::AES_256_GCM, wrapping_key.as_ref())
                .map_err(|_| AgentFailure::VaultUnavailable)?,
        );
        let plaintext = key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce_bytes),
                aead::Aad::from(enrollment_aad(&operation, &issuer)),
                ciphertext.as_mut_slice(),
            )
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let key_pair = signature::Ed25519KeyPair::from_pkcs8(plaintext)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        if URL_SAFE_NO_PAD.encode(key_pair.public_key().as_ref()) != issuer.public_key {
            return Err(AgentFailure::VaultUnavailable);
        }
        self.check_access()?;
        Ok((key_pair, issuer))
    }

    pub(super) async fn validate_current_enrollment_key(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            match expectation_on(&tx).await? {
                GatewayCredentialExpectation::Pending { operation_id }
                | GatewayCredentialExpectation::Committed { operation_id, .. } => {
                    self.enrollment_key_on(&tx, operation_id).await?;
                }
                // Restored expectations may coexist with retained historical keys.
                // Forgotten authority is proved by its receipt at its read boundary.
                GatewayCredentialExpectation::Unpaired
                | GatewayCredentialExpectation::Forgotten { .. } => {}
            }
            self.check_access()
        }
        .await;
        self.finish_access_grant_transaction(tx, result).await
    }
}
pub(super) fn decode_canonical(value: &str, max: usize) -> Result<Vec<u8>, AgentFailure> {
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

pub(super) fn decode_exact(value: &str, length: usize) -> Result<Vec<u8>, AgentFailure> {
    let decoded = decode_canonical(value, length)?;
    if decoded.len() != length {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(decoded)
}
