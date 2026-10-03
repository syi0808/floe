//! Wrapped authority key material never leaves the encrypted adapter.
use base64::{Engine as _,engine::general_purpose::URL_SAFE_NO_PAD};
use floe_access::RemoteOwnerPublicKey;
use floe_agent_contract::AgentFailure;
use ring::{aead,hkdf,rand::{SecureRandom,SystemRandom},signature::{self,KeyPair}};
use uuid::Uuid;
use super::{EncryptedAgentVault,VaultKeyProvider,storage};
const OWNER_WRAP_CONTEXT:&[u8]=b"floe.remote.owner-key.wrap.v1\0";

impl<Keys:VaultKeyProvider> EncryptedAgentVault<Keys>{
    pub(super) fn generate_wrapped_owner_key(&self) -> Result<(String, String, String, String), AgentFailure> {
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

    pub(super) fn wrapping_key(&self) -> Result<[u8; 32], AgentFailure> {
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

    pub(super) async fn validate_owner_key(&self) -> Result<(), AgentFailure> {
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

    pub(super) async fn remote_owner_public_key(&self) -> Result<RemoteOwnerPublicKey, AgentFailure> {
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

    pub(super) async fn load_owner_key(&self) -> Result<(signature::Ed25519KeyPair, String), AgentFailure> {
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

