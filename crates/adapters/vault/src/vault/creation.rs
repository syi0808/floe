//! Durable evidence of an interrupted first creation, never an open-time repair.
use super::{AgentFailure, PersonId, Uuid, private_file};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const MARKER: &str = "creation.pending";
const MAX_MARKER_BYTES: u64 = 512;

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreationIdentity {
    marker_version: u32,
    person_id: PersonId,
    vault_id: Uuid,
    encrypted_layout_version: i64,
}

pub(super) struct PendingCreation {
    path: PathBuf,
    identity: CreationIdentity,
}
impl PendingCreation {
    pub(super) fn begin(
        directory: &Path,
        person_id: PersonId,
        vault_id: Uuid,
    ) -> Result<Self, AgentFailure> {
        let identity = CreationIdentity {
            marker_version: 1,
            person_id,
            vault_id,
            encrypted_layout_version: crate::schema::ENCRYPTED_LAYOUT_VERSION,
        };
        let path = directory.join(MARKER);
        let bytes = serde_json::to_vec(&identity).map_err(|_| AgentFailure::VaultUnavailable)?;
        let mut file = private_file()
            .create_new(true)
            .open(&path)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        File::open(directory)
            .and_then(|parent| parent.sync_all())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(Self { path, identity })
    }

    /// Only the exact successful creator retires this proof. Dropping the guard
    /// on any failure intentionally preserves the marker and its existing key.
    pub(super) fn finish(self) -> Result<(), AgentFailure> {
        let directory = self.path.parent().ok_or(AgentFailure::VaultUnavailable)?;
        if read_pending(directory)?.as_ref() != Some(&self.identity) {
            return Err(AgentFailure::VaultUnavailable);
        }
        fs::remove_file(&self.path).map_err(|_| AgentFailure::VaultUnavailable)?;
        File::open(directory)
            .and_then(|parent| parent.sync_all())
            .map_err(|_| AgentFailure::VaultUnavailable)
    }
}

fn owned_file(path: &Path) -> Result<File, AgentFailure> {
    let before = fs::symlink_metadata(path).map_err(|_| AgentFailure::VaultUnavailable)?;
    if !before.is_file() || before.mode() & 0o077 != 0 || before.uid() != unsafe { libc::geteuid() }
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let after = file
        .metadata()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if before.dev() != after.dev() || before.ino() != after.ino() || !after.is_file() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(file)
}
fn read_pending(directory: &Path) -> Result<Option<CreationIdentity>, AgentFailure> {
    let path = directory.join(MARKER);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AgentFailure::VaultUnavailable),
        Ok(_) => {}
    }
    let file = owned_file(&path)?;
    if file
        .metadata()
        .map_err(|_| AgentFailure::VaultUnavailable)?
        .len()
        > MAX_MARKER_BYTES
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut bytes = Vec::new();
    file.take(MAX_MARKER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if bytes.len() as u64 > MAX_MARKER_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    let identity: CreationIdentity =
        serde_json::from_slice(&bytes).map_err(|_| AgentFailure::VaultUnavailable)?;
    if identity.marker_version != 1
        || !identity.person_id.is_valid()
        || identity.vault_id.is_nil()
        || identity.encrypted_layout_version != crate::schema::ENCRYPTED_LAYOUT_VERSION
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(identity))
}

pub(super) fn ensure_complete(directory: &Path, person: PersonId) -> Result<(), AgentFailure> {
    let Some(pending) = read_pending(directory)? else {
        return Ok(());
    };
    if pending.person_id != person {
        return Err(AgentFailure::VaultUnavailable);
    }
    let identity_path = directory.join("vault.id");
    match fs::symlink_metadata(&identity_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(AgentFailure::VaultUnavailable),
        Ok(_) => {
            let mut identity = String::new();
            owned_file(&identity_path)?
                .take(37)
                .read_to_string(&mut identity)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            if identity != pending.vault_id.to_string() {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
    }
    Err(AgentFailure::IncompleteCreation)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaultPresence {
    Missing,
    Existing,
}

/// Read-only physical presence used by the host lifecycle. This neither opens
/// a key nor turns a partially created directory into a fresh-create decision.
pub fn inspect_vault_presence(
    root: &Path,
    person: PersonId,
) -> Result<VaultPresence, AgentFailure> {
    if !person.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let directory = root.join(person.to_string());
    for path in [root, directory.as_path()] {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(VaultPresence::Missing);
            }
            Ok(metadata)
                if metadata.is_dir()
                    && metadata.mode() & 0o077 == 0
                    && metadata.uid() == unsafe { libc::geteuid() } => {}
            _ => return Err(AgentFailure::VaultUnavailable),
        }
    }
    ensure_complete(&directory, person)?;
    let mut identity = String::new();
    owned_file(&directory.join("vault.id"))?
        .take(37)
        .read_to_string(&mut identity)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let id = Uuid::parse_str(&identity).map_err(|_| AgentFailure::VaultUnavailable)?;
    if id.is_nil() || id.to_string() != identity {
        return Err(AgentFailure::VaultUnavailable);
    }
    let database = owned_file(&directory.join("sessions.db"))?;
    if database
        .metadata()
        .map_err(|_| AgentFailure::VaultUnavailable)?
        .len()
        == 0
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(VaultPresence::Existing)
}
