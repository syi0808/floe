use std::path::Path;

#[cfg(debug_assertions)]
pub use floe_native::DevelopmentResetReason;
pub use floe_native::{
    LocalDatabaseAdmission, LocalInstallation, LocalInstallationLease, NativeInstallationError,
    lock_existing_local_installation, prepare_local_installation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedLocalIdentity {
    pub person_id: uuid::Uuid,
    pub device_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalIdentityError {
    Invalid,
    Unavailable,
}

pub fn local_identity_for_database(
    database_path: &Path,
) -> Result<Option<VerifiedLocalIdentity>, LocalIdentityError> {
    match floe_native::local_identity_for_database(database_path) {
        Ok(identity) => Ok(Some(VerifiedLocalIdentity {
            person_id: identity.person_id(),
            device_id: identity.device_id().into(),
        })),
        Err(floe_native::NativeIdentityError::NotConfigured) => Ok(None),
        Err(floe_native::NativeIdentityError::Invalid) => Err(LocalIdentityError::Invalid),
        Err(floe_native::NativeIdentityError::Unavailable) => Err(LocalIdentityError::Unavailable),
    }
}
