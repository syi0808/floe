use std::{fs, io::Read, path::Path};

use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLocalIdentity {
    person_id: Uuid,
    device_id: String,
}

impl NativeLocalIdentity {
    pub fn person_id(&self) -> Uuid {
        self.person_id
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeIdentityError {
    NotConfigured,
    Invalid,
    Unavailable,
}

pub fn local_identity_for_database(
    database_path: &Path,
) -> Result<NativeLocalIdentity, NativeIdentityError> {
    if database_path.file_name().and_then(|value| value.to_str()) != Some("floe.db") {
        return Err(NativeIdentityError::NotConfigured);
    }
    let person_directory = database_path
        .parent()
        .ok_or(NativeIdentityError::NotConfigured)?;
    let people_directory = person_directory
        .parent()
        .ok_or(NativeIdentityError::NotConfigured)?;
    if people_directory
        .file_name()
        .and_then(|value| value.to_str())
        != Some("people")
    {
        return Err(NativeIdentityError::NotConfigured);
    }
    require_real_directory(person_directory)?;
    require_real_directory(people_directory)?;
    let support_directory = people_directory
        .parent()
        .ok_or(NativeIdentityError::Invalid)?;
    require_real_directory(support_directory)?;
    let person = person_directory
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(NativeIdentityError::Invalid)?;
    let person_id = Uuid::parse_str(person).map_err(|_| NativeIdentityError::Invalid)?;
    if person_id.is_nil() {
        return Err(NativeIdentityError::Invalid);
    }
    let identity_path = support_directory.join("local_device_id");
    let metadata =
        fs::symlink_metadata(&identity_path).map_err(|_| NativeIdentityError::Unavailable)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(NativeIdentityError::Invalid);
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let identity = options
        .open(identity_path)
        .map_err(|_| NativeIdentityError::Unavailable)?;
    if !identity
        .metadata()
        .map_err(|_| NativeIdentityError::Unavailable)?
        .is_file()
    {
        return Err(NativeIdentityError::Invalid);
    }
    let mut device_id = String::new();
    identity
        .take(129)
        .read_to_string(&mut device_id)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::InvalidData => NativeIdentityError::Invalid,
            _ => NativeIdentityError::Unavailable,
        })?;
    if device_id.is_empty()
        || device_id.len() > 128
        || device_id.chars().any(char::is_whitespace)
        || device_id.chars().any(char::is_control)
    {
        return Err(NativeIdentityError::Invalid);
    }
    Ok(NativeLocalIdentity {
        person_id,
        device_id,
    })
}

fn require_real_directory(path: &Path) -> Result<(), NativeIdentityError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| NativeIdentityError::Unavailable)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(NativeIdentityError::Invalid);
    }
    Ok(())
}
