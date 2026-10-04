//! Development-only key custody. The encrypted engine is unchanged. These
//! private files are intentionally weaker than OS keyring and never a fallback.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

use super::{AgentFailure, PersonId, Uuid, VaultKey, VaultKeyProvider, VaultKeyReadFailure};

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

    fn directory(&self) -> PathBuf {
        self.root.join(".development-keys")
    }

    fn path(&self, person: PersonId, vault: Uuid) -> Result<PathBuf, AgentFailure> {
        if person.0.is_nil() || vault.is_nil() {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(self.directory().join(format!("{person}-{vault}.key")))
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
    ) -> Result<VaultKey, VaultKeyReadFailure> {
        let path = self
            .path(person, vault)
            .map_err(VaultKeyReadFailure::Unavailable)?;
        for directory in [&self.root, &self.directory()] {
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
        Ok(VaultKey::from_bytes(*bytes))
    }

    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
        self.inspect_existing(person, vault)
            .map_err(|_| AgentFailure::VaultUnavailable)
    }

    fn insert(&self, person: PersonId, vault: Uuid, key: &VaultKey) -> Result<(), AgentFailure> {
        let path = self.path(person, vault)?;
        private_directory(&self.root, false).map_err(unavailable)?;
        let directory = self.directory();
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
        File::open(&self.root)
            .and_then(|file| file.sync_all())
            .map_err(unavailable)?;
        // No overwrite or cleanup after uncertainty. The Vault creation marker
        // and the exact key remain available for explicit recovery.
        let observed = self.load(person, vault)?;
        if observed.as_bytes() != key.as_bytes() {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(())
    }
}
