use std::path::Path;

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
