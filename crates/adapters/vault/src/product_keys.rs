//! Purpose-separated custody for the host product store. Existing opens never
//! generate keys; Fresh creation is admitted by the installation owner.
use crate::{RootKey, StoreError, StoreErrorCode};
use floe_kernel::PersonId;
use std::path::Path;

#[derive(Clone)]
pub struct ProductStoreIdentity {
    pub(crate) person: PersonId,
    pub(crate) device: String,
}
impl ProductStoreIdentity {
    pub fn new(person: PersonId, device: String) -> Result<Self, StoreError> {
        if !person.is_valid()
            || device.is_empty()
            || device.len() > 256
            || device.trim() != device
            || device.chars().any(char::is_control)
        {
            return Err(StoreError::new(
                StoreErrorCode::Validation,
                "invalid product store identity",
            ));
        }
        Ok(Self { person, device })
    }
    fn account(&self) -> String {
        format!("{}/{}", self.person, self.device)
    }
}
fn unavailable() -> StoreError {
    StoreError::new(
        StoreErrorCode::ProductKeyUnavailable,
        "product store key unavailable; existing data was preserved",
    )
}

fn read_failure(error: crate::VaultKeyReadFailure) -> StoreError {
    let (code, message) = match error {
        crate::VaultKeyReadFailure::Missing => (
            StoreErrorCode::ProductKeyMissing,
            "product store key is missing; existing data was preserved",
        ),
        crate::VaultKeyReadFailure::Malformed => (
            StoreErrorCode::ProductKeyMalformed,
            "product store key is malformed; existing data was preserved",
        ),
        crate::VaultKeyReadFailure::Unavailable(_) => return unavailable(),
    };
    StoreError::new(code, message)
}
pub(crate) fn load(path: &Path, identity: &ProductStoreIdentity) -> Result<RootKey, StoreError> {
    #[cfg(feature = "development-storage")]
    {
        crate::vault::development_keys::read_file_key(parent(path)?, &file_name(identity))
            .map_err(read_failure)
    }
    #[cfg(feature = "os-keyring")]
    {
        let _ = path;
        crate::vault::keyring::load_key("com.floe.product-store.v1", &identity.account())
            .map_err(read_failure)
    }
}
pub(crate) fn create(path: &Path, identity: &ProductStoreIdentity) -> Result<RootKey, StoreError> {
    let key = RootKey::generate().map_err(|_| unavailable())?;
    #[cfg(feature = "development-storage")]
    crate::vault::development_keys::create_file_key(parent(path)?, &file_name(identity), &key)
        .map_err(|_| unavailable())?;
    #[cfg(feature = "os-keyring")]
    {
        let _ = path;
        crate::vault::keyring::create_key("com.floe.product-store.v1", &identity.account(), &key)
            .map_err(|_| unavailable())?;
    }
    Ok(key)
}
#[cfg(feature = "development-storage")]
fn parent(path: &Path) -> Result<&Path, StoreError> {
    path.parent().ok_or_else(unavailable)
}
#[cfg(feature = "development-storage")]
fn file_name(identity: &ProductStoreIdentity) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(identity.account().as_bytes());
    let hash: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("product-{hash}.key")
}
