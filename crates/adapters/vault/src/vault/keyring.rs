use keyring_core::{Entry, Error};
use zeroize::Zeroizing;

use super::{AgentFailure, PersonId, Uuid, VaultKey, VaultKeyProvider};

const SERVICE: &str = "com.floe.agent-vault.v1";

#[derive(Clone, Copy)]
pub struct KeyringVaultKeys;

impl VaultKeyProvider for KeyringVaultKeys {
    fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
        read_key(&entry(person_id, vault_id)?)
    }

    fn insert(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
        key: &VaultKey,
    ) -> Result<(), AgentFailure> {
        insert_key(&entry(person_id, vault_id)?, key)
    }
}

#[cfg(target_os = "macos")]
fn entry(person_id: PersonId, vault_id: Uuid) -> Result<Entry, AgentFailure> {
    use apple_native_keyring_store::keychain::{Cred, MacKeychainDomain};
    Cred::build(
        MacKeychainDomain::User,
        SERVICE,
        &format!("{person_id}/{vault_id}"),
    )
    .map_err(|_| AgentFailure::VaultUnavailable)
}

#[cfg(target_os = "ios")]
fn entry(person_id: PersonId, vault_id: Uuid) -> Result<Entry, AgentFailure> {
    use apple_native_keyring_store::protected::{AccessPolicy, Cred};
    Cred::build(
        SERVICE,
        &format!("{person_id}/{vault_id}"),
        AccessPolicy::WhenUnlockedThisDeviceOnly,
        None,
        false,
    )
    .map_err(|_| AgentFailure::VaultUnavailable)
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn entry(_: PersonId, _: Uuid) -> Result<Entry, AgentFailure> {
    let _ = SERVICE;
    Err(AgentFailure::VaultUnavailable)
}

fn read_key(entry: &Entry) -> Result<VaultKey, AgentFailure> {
    let secret = Zeroizing::new(
        entry
            .get_secret()
            .map_err(|_| AgentFailure::VaultUnavailable)?,
    );
    if secret.len() != 32 {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut key = VaultKey::from_bytes([0; 32]);
    key.0.copy_from_slice(&secret);
    Ok(key)
}

fn insert_key(entry: &Entry, key: &VaultKey) -> Result<(), AgentFailure> {
    match entry.get_secret() {
        Err(Error::NoEntry) => entry
            .set_secret(key.as_bytes())
            .map_err(|_| AgentFailure::VaultUnavailable),
        Ok(secret) => {
            let _secret = Zeroizing::new(secret);
            Err(AgentFailure::VaultUnavailable)
        }
        Err(_) => Err(AgentFailure::VaultUnavailable),
    }
}
