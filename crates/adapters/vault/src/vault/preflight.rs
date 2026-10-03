//! Existing encrypted-profile inspection before App activates any owner.
//! This does not initialize schemas, replay journals or create/change keys.

use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

use super::keyring::VaultKeyReadFailure;
use super::{AgentFailure, PersonId, Uuid, VaultKeyProvider};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaultResetReason {
    MissingKey,
    MalformedKey,
    UnsupportedSchema,
    IdentityMismatch,
    /// The existing owned database returned a typed corrupt/not-a-database
    /// failure. This is not proof that the key itself is wrong.
    StoredDataCorrupt,
}

/// Proof remains exclusive until App has moved the old development profile.
/// The guard is deliberately neither cloneable nor serializable.
pub struct VaultResetEvidence {
    reason: VaultResetReason,
    _host_lock: File,
}
impl VaultResetEvidence {
    pub fn reason(&self) -> VaultResetReason {
        self.reason
    }
    pub fn lock_file(&self) -> &File {
        &self._host_lock
    }
}

pub enum VaultOpenInspection {
    /// Identity and required schema/version metadata passed. Historical payloads
    /// and runtime authority still undergo normal validation during real open.
    Compatible,
    Absent,
    Resettable(VaultResetEvidence),
    Unavailable(AgentFailure),
}

enum InspectionFailure {
    Resettable(VaultResetReason),
    Unavailable(AgentFailure),
}
type InspectionResult<T> = Result<T, InspectionFailure>;

/// Inspect only the existing encrypted Vault for this Person. App alone decides
/// whether its explicitly approved development-profile policy may archive it.
pub async fn inspect_existing_vault<Keys: VaultKeyProvider>(
    root: &Path,
    person_id: PersonId,
    keys: &Keys,
) -> VaultOpenInspection {
    if person_id.0.is_nil() {
        return VaultOpenInspection::Unavailable(AgentFailure::InvalidInput);
    }
    let directory = root.join(person_id.to_string());
    for path in [root, directory.as_path()] {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return VaultOpenInspection::Absent;
            }
            Ok(metadata) if metadata.is_dir() && private_owned(&metadata) => {}
            _ => return VaultOpenInspection::Unavailable(AgentFailure::VaultUnavailable),
        }
    }
    // Never create a lock while deciding whether an existing profile is owned.
    // A partial layout without this exclusion stays unavailable.
    let host_lock = match existing_file(&directory.join("host.lock"), true) {
        Ok(lock) => lock,
        Err(_) => return VaultOpenInspection::Unavailable(AgentFailure::VaultUnavailable),
    };
    if let Err(error) = host_lock.try_lock() {
        return VaultOpenInspection::Unavailable(match error {
            std::fs::TryLockError::WouldBlock => AgentFailure::Conflict,
            std::fs::TryLockError::Error(_) => AgentFailure::VaultUnavailable,
        });
    }
    match inspect_locked(&directory, person_id, keys).await {
        Ok(()) => VaultOpenInspection::Compatible,
        Err(InspectionFailure::Resettable(reason)) => {
            VaultOpenInspection::Resettable(VaultResetEvidence {
                reason,
                _host_lock: host_lock,
            })
        }
        Err(InspectionFailure::Unavailable(failure)) => VaultOpenInspection::Unavailable(failure),
    }
}

fn private_owned(metadata: &fs::Metadata) -> bool {
    metadata.mode() & 0o077 == 0 && metadata.uid() == unsafe { libc::geteuid() }
}

fn existing_file(path: &Path, writable: bool) -> InspectionResult<File> {
    let before = fs::symlink_metadata(path).map_err(io_failure)?;
    if !before.is_file() || !private_owned(&before) {
        return Err(unavailable());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(writable)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(io_failure)?;
    let actual = file.metadata().map_err(io_failure)?;
    if !actual.is_file()
        || !private_owned(&actual)
        || actual.dev() != before.dev()
        || actual.ino() != before.ino()
    {
        return Err(unavailable());
    }
    Ok(file)
}

async fn inspect_locked<Keys: VaultKeyProvider>(
    directory: &Path,
    person_id: PersonId,
    keys: &Keys,
) -> InspectionResult<()> {
    super::creation::ensure_complete(directory, person_id)
        .map_err(InspectionFailure::Unavailable)?;
    let mut marker = existing_file(&directory.join("vault.id"), false)?;
    if marker.metadata().map_err(io_failure)?.len() != 36 {
        return Err(reset(VaultResetReason::IdentityMismatch));
    }
    let mut identity = String::new();
    (&mut marker)
        .take(37)
        .read_to_string(&mut identity)
        .map_err(io_failure)?;
    let vault_id = Uuid::parse_str(&identity)
        .ok()
        .filter(|id| !id.is_nil() && id.to_string() == identity)
        .ok_or_else(|| reset(VaultResetReason::IdentityMismatch))?;
    let path = directory.join("sessions.db");
    let database_file = existing_file(&path, false)?;
    if database_file.metadata().map_err(io_failure)?.len() == 0 {
        // An empty database is not evidence of an unsupported stored schema.
        // It may be an interrupted first creation with no committed layout.
        return Err(unavailable());
    }
    for name in ["sessions.db-wal", "sessions.db-shm"] {
        let sidecar = directory.join(name);
        match fs::symlink_metadata(&sidecar) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            // Turso owns sidecar creation modes. The private parent directory
            // protects them; do not invent a stricter format than normal open.
            Ok(metadata) if metadata.is_file() && metadata.uid() == unsafe { libc::geteuid() } => {}
            _ => return Err(unavailable()),
        }
    }
    let key = keys
        .inspect_existing(person_id, vault_id)
        .map_err(|failure| match failure {
            VaultKeyReadFailure::Missing => reset(VaultResetReason::MissingKey),
            VaultKeyReadFailure::Malformed => reset(VaultResetReason::MalformedKey),
            VaultKeyReadFailure::Unavailable(failure) => InspectionFailure::Unavailable(failure),
        })?;
    let database = turso::Builder::new_local(path.to_str().ok_or_else(unavailable)?)
        .read_only(true)
        .experimental_encryption(true)
        .with_encryption(turso::EncryptionOpts {
            cipher: "aes256gcm".into(),
            hexkey: key.hex(),
        })
        .build()
        .await
        .map_err(database_failure)?;
    let connection = database.connect().map_err(database_failure)?;
    crate::schema::inspect(&connection, crate::schema::Layout::Encrypted)
        .await
        .map_err(schema_failure)?;
    let mut rows = connection
        .query(
            "SELECT id, version, person_id, vault_id FROM vault_identity LIMIT 2",
            (),
        )
        .await
        .map_err(database_failure)?;
    let row = rows
        .next()
        .await
        .map_err(database_failure)?
        .ok_or_else(|| reset(VaultResetReason::IdentityMismatch))?;
    if row.get::<i64>(0).map_err(database_failure)? != 1
        || row.get::<String>(2).map_err(database_failure)? != person_id.to_string()
        || row.get::<String>(3).map_err(database_failure)? != vault_id.to_string()
        || rows.next().await.map_err(database_failure)?.is_some()
    {
        return Err(reset(VaultResetReason::IdentityMismatch));
    }
    drop(rows);
    Ok(())
}

fn reset(reason: VaultResetReason) -> InspectionFailure {
    InspectionFailure::Resettable(reason)
}
fn unavailable() -> InspectionFailure {
    InspectionFailure::Unavailable(AgentFailure::VaultUnavailable)
}
fn io_failure(_: std::io::Error) -> InspectionFailure {
    unavailable()
}
fn database_failure(error: turso::Error) -> InspectionFailure {
    match error {
        turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => {
            InspectionFailure::Unavailable(AgentFailure::Conflict)
        }
        turso::Error::Corrupt(_) | turso::Error::NotAdb(_) => {
            reset(VaultResetReason::StoredDataCorrupt)
        }
        // Pinned Turso erases decryption/checksum and some I/O failures into
        // Error(String). Its text cannot distinguish safe reset evidence.
        _ => InspectionFailure::Unavailable(AgentFailure::StorageUnavailable),
    }
}

fn schema_failure(failure: crate::schema::SchemaFailure) -> InspectionFailure {
    match failure {
        crate::schema::SchemaFailure::Unsupported { .. } => {
            reset(VaultResetReason::UnsupportedSchema)
        }
        crate::schema::SchemaFailure::StoredCorrupt => reset(VaultResetReason::StoredDataCorrupt),
        crate::schema::SchemaFailure::Busy => {
            InspectionFailure::Unavailable(AgentFailure::Conflict)
        }
        crate::schema::SchemaFailure::Unavailable
        | crate::schema::SchemaFailure::InvalidDefinition => {
            InspectionFailure::Unavailable(AgentFailure::StorageUnavailable)
        }
    }
}
