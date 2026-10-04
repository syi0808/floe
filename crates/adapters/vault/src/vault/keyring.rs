use keyring_core::{Entry, Error};
use zeroize::Zeroizing;

use super::{AgentFailure, PersonId, RootKey, Uuid, VaultKeyProvider, VaultKeyReadFailure};

const SERVICE: &str = "com.floe.agent-vault.v1";

#[derive(Clone, Copy)]
pub struct KeyringVaultKeys;

impl VaultKeyProvider for KeyringVaultKeys {
    fn inspect_existing(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
    ) -> Result<RootKey, VaultKeyReadFailure> {
        let entry = entry(SERVICE, &format!("{person_id}/{vault_id}"))
            .map_err(VaultKeyReadFailure::Unavailable)?;
        read_key_classified(&entry)
    }

    fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<RootKey, AgentFailure> {
        read_key(&entry(SERVICE, &format!("{person_id}/{vault_id}"))?)
    }

    fn insert(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
        key: &RootKey,
    ) -> Result<(), AgentFailure> {
        insert_key(&entry(SERVICE, &format!("{person_id}/{vault_id}"))?, key)
    }
}

#[cfg(target_os = "macos")]
fn entry(service: &str, account: &str) -> Result<Entry, AgentFailure> {
    use apple_native_keyring_store::keychain::{Cred, MacKeychainDomain};
    Cred::build(MacKeychainDomain::User, service, account)
        .map_err(|_| AgentFailure::VaultUnavailable)
}

#[cfg(target_os = "ios")]
fn entry(service: &str, account: &str) -> Result<Entry, AgentFailure> {
    use apple_native_keyring_store::protected::{AccessPolicy, Cred};
    Cred::build(
        service,
        account,
        AccessPolicy::WhenUnlockedThisDeviceOnly,
        None,
        false,
    )
    .map_err(|_| AgentFailure::VaultUnavailable)
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn entry(_: &str, _: &str) -> Result<Entry, AgentFailure> {
    let _ = SERVICE;
    Err(AgentFailure::VaultUnavailable)
}

fn read_key(entry: &Entry) -> Result<RootKey, AgentFailure> {
    read_key_classified(entry).map_err(|_| AgentFailure::VaultUnavailable)
}

fn read_key_classified(entry: &Entry) -> Result<RootKey, VaultKeyReadFailure> {
    let secret = Zeroizing::new(entry.get_secret().map_err(|error| match error {
        Error::NoEntry => VaultKeyReadFailure::Missing,
        // Access denial, locked storage, ambiguity and platform faults
        // never establish that an existing key is absent or malformed.
        _ => VaultKeyReadFailure::Unavailable(AgentFailure::VaultUnavailable),
    })?);
    if secret.len() != 32 {
        return Err(VaultKeyReadFailure::Malformed);
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&secret);
    Ok(RootKey::from_bytes(bytes))
}

fn insert_key(entry: &Entry, key: &RootKey) -> Result<(), AgentFailure> {
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

pub(crate) fn load_key(service: &str, account: &str) -> Result<RootKey, VaultKeyReadFailure> {
    read_key_classified(&entry(service, account).map_err(VaultKeyReadFailure::Unavailable)?)
}
pub(crate) fn create_key(service: &str, account: &str, key: &RootKey) -> Result<(), AgentFailure> {
    let slot = entry(service, account)?;
    insert_key(&slot, key)?;
    let observed = read_key(&slot)?;
    use subtle::ConstantTimeEq;
    if !bool::from(key.as_bytes().ct_eq(observed.as_bytes())) {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}
