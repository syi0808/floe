//! Build-selected custody and isolated installation admission. No runtime
//! switch or OS-store failure can downgrade the production storage profile.
use crate::AppOpenError;
use floe_kernel::AgentFailure;
use std::path::{Path, PathBuf};

#[cfg(all(not(feature = "development-storage"), target_os = "android"))]
pub(crate) use crate::android_vault_keys::AndroidVaultKeys as ProfileVaultKeys;
#[cfg(feature = "development-storage")]
pub(crate) use floe_vault::DevelopmentVaultKeys as ProfileVaultKeys;
#[cfg(all(not(feature = "development-storage"), not(target_os = "android")))]
pub(crate) use floe_vault::KeyringVaultKeys as ProfileVaultKeys;

pub(crate) fn vault_keys(root: &Path) -> Result<ProfileVaultKeys, AgentFailure> {
    #[cfg(feature = "development-storage")]
    {
        floe_vault::DevelopmentVaultKeys::new(root)
    }
    #[cfg(not(feature = "development-storage"))]
    {
        let _ = root;
        Ok(ProfileVaultKeys)
    }
}

pub fn storage_profile_code() -> u32 {
    if cfg!(feature = "development-storage") {
        2
    } else {
        1
    }
}

const MARKER: &str = "floe-development-profile";
#[cfg(feature = "development-storage")]
const MARKER_BYTES: &[u8] = b"Floe isolated development storage v1\n";
fn invalid() -> AppOpenError {
    AppOpenError::Runtime("storage profile is unavailable or does not match this build".into())
}

pub(crate) fn support_directory(base: &Path) -> Result<PathBuf, AppOpenError> {
    if !base.is_absolute()
        || base
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(invalid());
    }
    #[cfg(not(feature = "development-storage"))]
    {
        reject_development(base)?;
        Ok(base.to_path_buf())
    }
    #[cfg(feature = "development-storage")]
    {
        use std::{fs, io::Write, os::unix::fs::OpenOptionsExt};
        let root = base.join("development-storage");
        // Only this dedicated child is created; the existing installation at
        // base is not discovered, imported or preflighted by this build.
        private_directory(&root, true)?;
        let marker = root.join(MARKER);
        match fs::symlink_metadata(&marker) {
            Ok(_) => validate_marker(&root)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if fs::read_dir(&root).map_err(|_| invalid())?.next().is_some() {
                    return Err(invalid());
                }
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                    .open(&marker)
                    .map_err(|_| invalid())?;
                file.write_all(MARKER_BYTES)
                    .and_then(|_| file.sync_all())
                    .map_err(|_| invalid())?;
                fs::File::open(&root)
                    .and_then(|file| file.sync_all())
                    .map_err(|_| invalid())?;
            }
            Err(_) => return Err(invalid()),
        }
        let client = root.join("client");
        private_directory(&client, true)?;
        Ok(client)
    }
}

pub(crate) fn validate_database(path: &Path) -> Result<(), AppOpenError> {
    let path = std::fs::canonicalize(path).map_err(|_| invalid())?;
    let root = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(invalid)?;
    #[cfg(not(feature = "development-storage"))]
    {
        reject_development(root)
    }
    #[cfg(feature = "development-storage")]
    {
        if root.file_name().and_then(|name| name.to_str()) != Some("client") {
            return Err(invalid());
        }
        private_directory(root, false)?;
        let profile = root.parent().ok_or_else(invalid)?;
        if profile.file_name().and_then(|name| name.to_str()) != Some("development-storage") {
            return Err(invalid());
        }
        validate_marker(profile)
    }
}

#[cfg(not(feature = "development-storage"))]
fn reject_development(root: &Path) -> Result<(), AppOpenError> {
    for directory in [Some(root), root.parent()].into_iter().flatten() {
        match std::fs::symlink_metadata(directory.join(MARKER)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(invalid()),
        }
    }
    Ok(())
}

#[cfg(feature = "development-storage")]
fn private_directory(path: &Path, create: bool) -> Result<(), AppOpenError> {
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt, MetadataExt},
    };
    if create {
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(invalid()),
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| invalid())?;
    if !metadata.is_dir()
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(feature = "development-storage")]
fn validate_marker(root: &Path) -> Result<(), AppOpenError> {
    use std::{
        fs::OpenOptions,
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    private_directory(root, false)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root.join(MARKER))
        .map_err(|_| invalid())?;
    let metadata = file.metadata().map_err(|_| invalid())?;
    if !metadata.is_file()
        || metadata.mode() & 0o077 != 0
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.len() != MARKER_BYTES.len() as u64
    {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(64)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes != MARKER_BYTES {
        return Err(invalid());
    }
    Ok(())
}
