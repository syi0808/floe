//! Development-only key custody. The encrypted engine is unchanged. These
//! private files are intentionally weaker than OS keyring and never a fallback.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

use super::{AgentFailure, PersonId, RootKey, Uuid, VaultKeyProvider, VaultKeyReadFailure};

#[derive(Clone)]
pub struct DevelopmentVaultKeys {
    root: PathBuf,
}

impl DevelopmentVaultKeys {
    /// Composition supplies only an admitted, isolated development Vault root.
    /// Construction and reads never create or repair files.
    pub fn new(vault_root: &Path) -> Result<Self, AgentFailure> {
        if !vault_root.is_absolute()
            || vault_root
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(Self {
            root: vault_root.to_path_buf(),
        })
    }
}

fn private_directory(path: &Path, create: bool) -> Result<(), std::io::Error> {
    if create {
        match fs::DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "invalid development key directory",
        ));
    }
    Ok(())
}

fn unavailable(_: std::io::Error) -> AgentFailure {
    AgentFailure::VaultUnavailable
}

impl VaultKeyProvider for DevelopmentVaultKeys {
    fn inspect_existing(
        &self,
        person: PersonId,
        vault: Uuid,
    ) -> Result<RootKey, VaultKeyReadFailure> {
        let name = key_name(person, vault).map_err(VaultKeyReadFailure::Unavailable)?;
        read_file_key(&self.root, &name)
    }
    fn load(&self, person: PersonId, vault: Uuid) -> Result<RootKey, AgentFailure> {
        self.inspect_existing(person, vault)
            .map_err(|_| AgentFailure::VaultUnavailable)
    }
    fn insert(&self, person: PersonId, vault: Uuid, key: &RootKey) -> Result<(), AgentFailure> {
        create_file_key(&self.root, &key_name(person, vault)?, key)
    }
}
fn key_name(person: PersonId, vault: Uuid) -> Result<String, AgentFailure> {
    if person.0.is_nil() || vault.is_nil() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(format!("{person}-{vault}.key"))
}
fn key_path(root: &Path, name: &str) -> Result<PathBuf, AgentFailure> {
    if !root.is_absolute()
        || root
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        || name.is_empty()
        || Path::new(name).file_name().and_then(|name| name.to_str()) != Some(name)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(root.join(".development-keys").join(name))
}
pub(crate) fn read_file_key(root: &Path, name: &str) -> Result<RootKey, VaultKeyReadFailure> {
    let path = key_path(root, name).map_err(VaultKeyReadFailure::Unavailable)?;
    let directory = root.join(".development-keys");
    for directory in [root, directory.as_path()] {
        private_directory(directory, false).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                VaultKeyReadFailure::Missing
            } else {
                VaultKeyReadFailure::Unavailable(AgentFailure::VaultUnavailable)
            }
        })?;
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                VaultKeyReadFailure::Missing
            } else {
                VaultKeyReadFailure::Unavailable(AgentFailure::VaultUnavailable)
            }
        })?;
    let metadata = file
        .metadata()
        .map_err(|_| VaultKeyReadFailure::Unavailable(AgentFailure::VaultUnavailable))?;
    if !metadata.is_file()
        || metadata.mode() & 0o077 != 0
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(VaultKeyReadFailure::Unavailable(
            AgentFailure::VaultUnavailable,
        ));
    }
    if metadata.len() != 32 {
        return Err(VaultKeyReadFailure::Malformed);
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    file.read_exact(bytes.as_mut())
        .map_err(|_| VaultKeyReadFailure::Unavailable(AgentFailure::VaultUnavailable))?;
    let mut extra = [0u8; 1];
    if file
        .read(&mut extra)
        .map_err(|_| VaultKeyReadFailure::Unavailable(AgentFailure::VaultUnavailable))?
        != 0
    {
        return Err(VaultKeyReadFailure::Malformed);
    }
    Ok(RootKey::from_bytes(*bytes))
}
pub(crate) fn create_file_key(root: &Path, name: &str, key: &RootKey) -> Result<(), AgentFailure> {
    let path = key_path(root, name)?;
    private_directory(root, false).map_err(unavailable)?;
    let directory = root.join(".development-keys");
    private_directory(&directory, true).map_err(unavailable)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(unavailable)?;
    file.write_all(key.as_bytes()).map_err(unavailable)?;
    file.sync_all().map_err(unavailable)?;
    File::open(&directory)
        .and_then(|file| file.sync_all())
        .map_err(unavailable)?;
    File::open(root)
        .and_then(|file| file.sync_all())
        .map_err(unavailable)?;
    // No overwrite or cleanup after uncertainty. The Vault creation marker
    // and the exact key remain available for explicit recovery.
    let observed = read_file_key(root, name).map_err(|_| AgentFailure::VaultUnavailable)?;
    if observed.as_bytes() != key.as_bytes() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}
