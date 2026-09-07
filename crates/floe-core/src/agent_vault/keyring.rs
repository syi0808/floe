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

#[cfg(not(target_os = "macos"))]
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

#[cfg(test)]
mod tests {
    use keyring_core::api::CredentialStoreApi;

    use super::*;

    #[test]
    fn binary_keys_roundtrip_and_existing_keys_are_not_replaced() {
        let store = keyring_core::mock::Store::new().unwrap();
        let entry = store.build(SERVICE, "synthetic-key-slot", None).unwrap();
        assert!(read_key(&entry).is_err());
        let key = VaultKey::from_bytes([123; 32]);
        insert_key(&entry, &key).unwrap();
        assert!(read_key(&entry).unwrap().as_bytes() == key.as_bytes());
        assert_eq!(
            insert_key(&entry, &VaultKey::from_bytes([42; 32])),
            Err(AgentFailure::VaultUnavailable)
        );
        assert!(read_key(&entry).unwrap().as_bytes() == key.as_bytes());
        entry.set_secret(&[1; 31]).unwrap();
        assert!(read_key(&entry).is_err());
    }

    #[test]
    fn unavailable_keyring_does_not_trigger_creation() {
        let store = keyring_core::mock::Store::new().unwrap();
        let entry = store
            .build(SERVICE, "synthetic-unavailable-slot", None)
            .unwrap();
        let credential = entry
            .as_any()
            .downcast_ref::<keyring_core::mock::Cred>()
            .unwrap();
        credential.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other(
            "fixture",
        ))));
        assert_eq!(
            insert_key(&entry, &VaultKey::from_bytes([42; 32])),
            Err(AgentFailure::VaultUnavailable)
        );
        assert!(matches!(entry.get_secret(), Err(Error::NoEntry)));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apple_backend_is_explicit_person_scoped_and_uses_login_keychain() {
        use apple_native_keyring_store::keychain::{Cred, MacKeychainDomain};
        let person = PersonId::new();
        let vault = Uuid::new_v4();
        let entry = entry(person, vault).unwrap();
        let credential = entry.as_any().downcast_ref::<Cred>().unwrap();
        assert_eq!(credential.domain, MacKeychainDomain::User);
        assert_eq!(credential.service, SERVICE);
        assert_eq!(credential.account, format!("{person}/{vault}"));
    }
}
