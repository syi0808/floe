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
        .map_err(|_| NativeIdentityError::Invalid)?;
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

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn product_database_path_binds_person_and_device_identity() {
        let root = tempfile::tempdir().unwrap();
        let person_id = Uuid::new_v4();
        let person_directory = root.path().join("people").join(person_id.to_string());
        fs::create_dir_all(&person_directory).unwrap();
        fs::write(root.path().join("local_device_id"), "local-device-1").unwrap();

        let identity = local_identity_for_database(&person_directory.join("floe.db")).unwrap();

        assert_eq!(identity.person_id(), person_id);
        assert_eq!(identity.device_id(), "local-device-1");
    }

    #[test]
    fn non_product_paths_are_not_misclassified_as_verified_identity() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            local_identity_for_database(&root.path().join("test.db")),
            Err(NativeIdentityError::NotConfigured)
        );
    }

    #[test]
    fn malformed_or_missing_device_identity_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let person_directory = root.path().join("people").join(Uuid::new_v4().to_string());
        fs::create_dir_all(&person_directory).unwrap();
        let database = person_directory.join("floe.db");
        assert_eq!(
            local_identity_for_database(&database),
            Err(NativeIdentityError::Unavailable)
        );
        let mut identity = fs::File::create(root.path().join("local_device_id")).unwrap();
        writeln!(identity, "device-with-newline").unwrap();
        assert_eq!(
            local_identity_for_database(&database),
            Err(NativeIdentityError::Invalid)
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_identity_file_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let person_directory = root.path().join("people").join(Uuid::new_v4().to_string());
        fs::create_dir_all(&person_directory).unwrap();
        let target = root.path().join("identity-target");
        fs::write(&target, "local-device-1").unwrap();
        symlink(target, root.path().join("local_device_id")).unwrap();
        assert_eq!(
            local_identity_for_database(&person_directory.join("floe.db")),
            Err(NativeIdentityError::Invalid)
        );
    }
}
