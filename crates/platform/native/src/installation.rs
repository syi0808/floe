//! Internal local installation admission. No Vault key or provider credential
//! is created here. The lock remains held while App validates the encrypted product store.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{NativeLocalIdentity, local_identity_for_database};

const RECORD: &str = "local_installation.json";
const LOCK: &str = "local_installation.lock";
const ATTEMPT: &str = "local_installation.create-attempt";
const READY: &str = "local_installation.ready";
const DEVICE: &str = "local_device_id";
const SELECTION: &str = "selected_profile.json";
const RESET: &str = "local_installation.reset";
const MAX_RECORD_BYTES: u64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeInstallationError {
    Invalid,
    Unavailable,
    Busy,
    Ambiguous,
    Incomplete,
}

impl std::fmt::Display for NativeInstallationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Invalid => "local installation identity or path is invalid",
            Self::Unavailable => "local installation is unreadable",
            Self::Busy => "local installation is being opened by another process",
            Self::Ambiguous => "existing local data has no unambiguous installation identity",
            Self::Incomplete => "local installation is incomplete; existing data was preserved",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalDatabaseAdmission {
    Existing,
    InspectUnfinishedCreation,
    CreateNew,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Initializing,
    Ready,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct InstallationRecord {
    schema_version: u32,
    person_id: String,
    device_id: String,
    phase: Phase,
}

impl InstallationRecord {
    fn validate(&self) -> Result<(), NativeInstallationError> {
        if self.schema_version != 1 || !valid_device(&self.device_id) {
            return Err(NativeInstallationError::Invalid);
        }
        person(&self.person_id)?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CreationAttempt {
    schema_version: u32,
    person_id: String,
    device_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExistingSelection {
    schema_version: u32,
    person_id: String,
}

/// A process-locked installation preparation. `complete` is called only after
/// the exact database has passed the storage owner's schema validation.
pub struct LocalInstallation {
    root: PathBuf,
    database_path: PathBuf,
    identity: NativeLocalIdentity,
    record: Option<InstallationRecord>,
    admission: LocalDatabaseAdmission,
    _lock: File,
}

/// Retained until App's owners and runtimes physically retire. Diagnostics
/// participate in the same exclusion, so explicit hosts cannot bypass an existing installation lease.
pub struct LocalInstallationLease {
    _lock: File,
}

impl LocalInstallationLease {
    pub fn into_lock_file(self) -> File {
        self._lock
    }
}

impl LocalInstallation {
    pub fn into_lease(self) -> LocalInstallationLease {
        LocalInstallationLease { _lock: self._lock }
    }
    pub fn identity(&self) -> &NativeLocalIdentity {
        &self.identity
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn database_admission(&self) -> LocalDatabaseAdmission {
        self.admission
    }

    /// Publish ready after App has opened and validated the encrypted product database.
    /// Re-read physical identity before changing only the installation phase.
    pub fn complete(&mut self) -> Result<(), NativeInstallationError> {
        let current = read_identity(&self.database_path)?;
        if current != self.identity || read_record(&self.root)? != self.record {
            return Err(NativeInstallationError::Invalid);
        }
        let metadata =
            regular_file(&self.database_path)?.ok_or(NativeInstallationError::Incomplete)?;
        if metadata.len() == 0 {
            return Err(NativeInstallationError::Incomplete);
        }
        let ready = InstallationRecord {
            schema_version: 1,
            person_id: self.identity.person_id().to_string(),
            device_id: self.identity.device_id().to_owned(),
            phase: Phase::Ready,
        };
        if self.record.as_ref() == Some(&ready) {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&ready).map_err(|_| NativeInstallationError::Invalid)?;
        if self.record.is_none() {
            write_new(&self.root.join(RECORD), &bytes)?;
        } else {
            let staged = self.root.join(READY);
            match read_bytes(&staged, MAX_RECORD_BYTES)? {
                Some(existing) if existing != bytes => {
                    return Err(NativeInstallationError::Invalid);
                }
                Some(_) => {}
                None => write_new(&staged, &bytes)?,
            }
            fs::rename(&staged, self.root.join(RECORD))
                .map_err(|_| NativeInstallationError::Unavailable)?;
            sync_directory(&self.root)?;
        }
        self.record = Some(ready);
        Ok(())
    }
}

pub fn prepare_local_installation(
    support_directory: &Path,
) -> Result<LocalInstallation, NativeInstallationError> {
    if !support_directory.is_absolute()
        || support_directory
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(NativeInstallationError::Invalid);
    }
    ensure_directory(support_directory)?;
    let root =
        fs::canonicalize(support_directory).map_err(|_| NativeInstallationError::Unavailable)?;
    let lock = acquire_installation_lock(&root)?;

    if regular_file(&root.join(RESET))?.is_some() {
        return Err(NativeInstallationError::Incomplete);
    }
    let prepared = prepare_identity(&root)?;
    Ok(LocalInstallation {
        root,
        database_path: prepared.database_path,
        identity: prepared.identity,
        record: prepared.record,
        admission: prepared.admission,
        _lock: lock,
    })
}

/// Existing-only diagnostic admission. This never repairs, initializes or
/// archives installation state, and excludes every default host/reset.
pub fn lock_existing_local_installation(
    database: &Path,
) -> Result<LocalInstallationLease, NativeInstallationError> {
    let expected = read_identity(database)?;
    let root = database
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or(NativeInstallationError::Invalid)?;
    let root = fs::canonicalize(root).map_err(|_| NativeInstallationError::Unavailable)?;
    let lock = acquire_installation_lock(&root)?;
    if read_identity(database)? != expected {
        return Err(NativeInstallationError::Invalid);
    }
    Ok(LocalInstallationLease { _lock: lock })
}

fn acquire_installation_lock(root: &Path) -> Result<File, NativeInstallationError> {
    let lock_path = root.join(LOCK);
    regular_file(&lock_path)?;
    let lock = private_options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|_| NativeInstallationError::Unavailable)?;
    if !lock
        .metadata()
        .map_err(|_| NativeInstallationError::Unavailable)?
        .is_file()
    {
        return Err(NativeInstallationError::Invalid);
    }
    lock.try_lock().map_err(|error| match error {
        fs::TryLockError::WouldBlock => NativeInstallationError::Busy,
        fs::TryLockError::Error(_) => NativeInstallationError::Unavailable,
    })?;
    Ok(lock)
}

struct PreparedInstallation {
    database_path: PathBuf,
    identity: NativeLocalIdentity,
    record: Option<InstallationRecord>,
    admission: LocalDatabaseAdmission,
}

fn prepare_identity(root: &Path) -> Result<PreparedInstallation, NativeInstallationError> {
    let selected = read_selection(root)?;
    let mut record = read_record(root)?;
    let (person_id, device_id, admission) = if let Some(saved) = record.as_ref() {
        saved.validate()?;
        if let Some(bytes) = read_bytes(&root.join(READY), MAX_RECORD_BYTES)? {
            let staged: InstallationRecord =
                serde_json::from_slice(&bytes).map_err(|_| NativeInstallationError::Invalid)?;
            staged.validate()?;
            if staged.phase != Phase::Ready
                || staged.person_id != saved.person_id
                || staged.device_id != saved.device_id
            {
                return Err(NativeInstallationError::Invalid);
            }
        }
        if selected
            .as_ref()
            .is_some_and(|selected| selected != &saved.person_id)
        {
            return Err(NativeInstallationError::Invalid);
        }
        let database = database_path(&root, &saved.person_id);
        let admission = match saved.phase {
            Phase::Ready => {
                require_identity(&root, saved)?;
                regular_file(&database)?.ok_or(NativeInstallationError::Incomplete)?;
                LocalDatabaseAdmission::Existing
            }
            Phase::Initializing => {
                if selected.is_some() {
                    return Err(NativeInstallationError::Invalid);
                }
                initialize_layout(&root, saved)?;
                reserve_database_creation(&root, saved)?
            }
        };
        (saved.person_id.clone(), saved.device_id.clone(), admission)
    } else {
        // An orphaned bootstrap marker is evidence of an interrupted attempt,
        // never permission to allocate another identity.
        if regular_file(&root.join(ATTEMPT))?.is_some()
            || regular_file(&root.join(READY))?.is_some()
        {
            return Err(NativeInstallationError::Incomplete);
        }
        let existing = if let Some(selected) = selected {
            Some(selected)
        } else {
            discover_existing(&root)?
        };
        if let Some(existing) = existing {
            let database = database_path(&root, &existing);
            let identity = read_identity(&database)?;
            regular_file(&database)?.ok_or(NativeInstallationError::Incomplete)?;
            (
                existing,
                identity.device_id().to_owned(),
                LocalDatabaseAdmission::Existing,
            )
        } else {
            require_fresh_root(&root)?;
            let saved = InstallationRecord {
                schema_version: 1,
                person_id: Uuid::new_v4().to_string(),
                device_id: Uuid::new_v4().to_string(),
                phase: Phase::Initializing,
            };
            let bytes = serde_json::to_vec(&saved).map_err(|_| NativeInstallationError::Invalid)?;
            // Durable random identity precedes every identity/data artifact.
            write_new(&root.join(RECORD), &bytes)?;
            initialize_layout(&root, &saved)?;
            let admission = reserve_database_creation(&root, &saved)?;
            let result = (saved.person_id.clone(), saved.device_id.clone(), admission);
            record = Some(saved);
            result
        }
    };
    let database_path = database_path(&root, &person_id);
    let identity = read_identity(&database_path)?;
    if identity.person_id() != person(&person_id)? || identity.device_id() != device_id {
        return Err(NativeInstallationError::Invalid);
    }
    Ok(PreparedInstallation {
        database_path,
        identity,
        record,
        admission,
    })
}

fn initialize_layout(
    root: &Path,
    saved: &InstallationRecord,
) -> Result<(), NativeInstallationError> {
    let people = root.join("people");
    ensure_directory(&people)?;
    for entry in fs::read_dir(&people).map_err(|_| NativeInstallationError::Unavailable)? {
        let entry = entry.map_err(|_| NativeInstallationError::Unavailable)?;
        if entry.file_name().to_str() != Some(saved.person_id.as_str()) {
            return Err(NativeInstallationError::Incomplete);
        }
    }
    let directory = people.join(&saved.person_id);
    ensure_directory(&directory)?;
    let device_path = root.join(DEVICE);
    match read_bytes(&device_path, 128)? {
        Some(bytes) if bytes != saved.device_id.as_bytes() => {
            return Err(NativeInstallationError::Invalid);
        }
        Some(_) => {}
        None => {
            if regular_file(&root.join(ATTEMPT))?.is_some() || !directory_empty(&directory)? {
                return Err(NativeInstallationError::Incomplete);
            }
            write_new(&device_path, saved.device_id.as_bytes())?;
        }
    }
    require_identity(root, saved)
}

fn reserve_database_creation(
    root: &Path,
    saved: &InstallationRecord,
) -> Result<LocalDatabaseAdmission, NativeInstallationError> {
    let database = database_path(root, &saved.person_id);
    if let Some(bytes) = read_bytes(&root.join(ATTEMPT), MAX_RECORD_BYTES)? {
        let attempt: CreationAttempt =
            serde_json::from_slice(&bytes).map_err(|_| NativeInstallationError::Invalid)?;
        if attempt.schema_version != 1
            || attempt.person_id != saved.person_id
            || attempt.device_id != saved.device_id
        {
            return Err(NativeInstallationError::Invalid);
        }
        regular_file(&database)?.ok_or(NativeInstallationError::Incomplete)?;
        return Ok(LocalDatabaseAdmission::InspectUnfinishedCreation);
    }
    let directory = database.parent().ok_or(NativeInstallationError::Invalid)?;
    if !directory_empty(directory)? {
        return Err(NativeInstallationError::Incomplete);
    }
    let bytes = serde_json::to_vec(&CreationAttempt {
        schema_version: 1,
        person_id: saved.person_id.clone(),
        device_id: saved.device_id.clone(),
    })
    .map_err(|_| NativeInstallationError::Invalid)?;
    write_new(&root.join(ATTEMPT), &bytes)?;
    Ok(LocalDatabaseAdmission::CreateNew)
}

fn require_identity(
    root: &Path,
    saved: &InstallationRecord,
) -> Result<(), NativeInstallationError> {
    let identity = read_identity(&database_path(root, &saved.person_id))?;
    if identity.person_id() != person(&saved.person_id)? || identity.device_id() != saved.device_id
    {
        return Err(NativeInstallationError::Invalid);
    }
    Ok(())
}

fn discover_existing(root: &Path) -> Result<Option<String>, NativeInstallationError> {
    let people = root.join("people");
    if !directory_exists(&people)? {
        return Ok(None);
    }
    let mut selected = None;
    for entry in fs::read_dir(&people).map_err(|_| NativeInstallationError::Unavailable)? {
        let entry = entry.map_err(|_| NativeInstallationError::Unavailable)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| NativeInstallationError::Invalid)?;
        person(&name)?;
        if !directory_exists(&entry.path())? {
            return Err(NativeInstallationError::Invalid);
        }
        regular_file(&entry.path().join("floe.db"))?.ok_or(NativeInstallationError::Incomplete)?;
        if selected.replace(name).is_some() {
            return Err(NativeInstallationError::Ambiguous);
        }
    }
    Ok(selected)
}

fn require_fresh_root(root: &Path) -> Result<(), NativeInstallationError> {
    for entry in fs::read_dir(root).map_err(|_| NativeInstallationError::Unavailable)? {
        let entry = entry.map_err(|_| NativeInstallationError::Unavailable)?;
        match entry.file_name().to_str() {
            Some(LOCK) => {}
            Some(".DS_Store") if regular_file(&entry.path())?.is_some() => {}
            // Reserved auxiliary namespaces are not production installations.
            // Their files are never read, adopted, reset or moved by this path.
            Some("diagnostics" | "recovery" | "development-storage" | "FloeDevelopmentNative")
                if directory_exists(&entry.path())? => {}
            Some("people") if directory_exists(&entry.path())? => {
                if !directory_empty(&entry.path())? {
                    return Err(NativeInstallationError::Incomplete);
                }
            }
            _ => return Err(NativeInstallationError::Incomplete),
        }
    }
    Ok(())
}

fn read_record(root: &Path) -> Result<Option<InstallationRecord>, NativeInstallationError> {
    read_bytes(&root.join(RECORD), MAX_RECORD_BYTES)?
        .map(|bytes| {
            let record: InstallationRecord =
                serde_json::from_slice(&bytes).map_err(|_| NativeInstallationError::Invalid)?;
            record.validate()?;
            Ok(record)
        })
        .transpose()
}

fn read_selection(root: &Path) -> Result<Option<String>, NativeInstallationError> {
    read_bytes(&root.join(SELECTION), 1024)?
        .map(|bytes| {
            let selection: ExistingSelection =
                serde_json::from_slice(&bytes).map_err(|_| NativeInstallationError::Invalid)?;
            if selection.schema_version != 1 {
                return Err(NativeInstallationError::Invalid);
            }
            person(&selection.person_id)?;
            Ok(selection.person_id)
        })
        .transpose()
}

fn person(value: &str) -> Result<Uuid, NativeInstallationError> {
    let id = Uuid::parse_str(value).map_err(|_| NativeInstallationError::Invalid)?;
    if id.is_nil() || id.to_string() != value {
        return Err(NativeInstallationError::Invalid);
    }
    Ok(id)
}

fn valid_device(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value
            .chars()
            .any(|value| value.is_whitespace() || value.is_control())
}

fn read_identity(database: &Path) -> Result<NativeLocalIdentity, NativeInstallationError> {
    let directory = database.parent().ok_or(NativeInstallationError::Invalid)?;
    let people = directory.parent().ok_or(NativeInstallationError::Invalid)?;
    let root = people.parent().ok_or(NativeInstallationError::Invalid)?;
    for path in [root, people, directory] {
        if !directory_exists(path)? {
            return Err(NativeInstallationError::Incomplete);
        }
    }
    let device = read_bytes(&root.join(DEVICE), 128)?.ok_or(NativeInstallationError::Incomplete)?;
    let device = std::str::from_utf8(&device).map_err(|_| NativeInstallationError::Invalid)?;
    if !valid_device(device) {
        return Err(NativeInstallationError::Invalid);
    }
    local_identity_for_database(database).map_err(|error| match error {
        crate::NativeIdentityError::Unavailable => NativeInstallationError::Unavailable,
        crate::NativeIdentityError::Invalid | crate::NativeIdentityError::NotConfigured => {
            NativeInstallationError::Invalid
        }
    })
}

fn database_path(root: &Path, person: &str) -> PathBuf {
    root.join("people").join(person).join("floe.db")
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    options
}

fn regular_file(path: &Path) -> Result<Option<fs::Metadata>, NativeInstallationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(Some(metadata)),
        Ok(_) => Err(NativeInstallationError::Invalid),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(NativeInstallationError::Unavailable),
    }
}

fn read_bytes(path: &Path, maximum: u64) -> Result<Option<Vec<u8>>, NativeInstallationError> {
    let Some(metadata) = regular_file(path)? else {
        return Ok(None);
    };
    if metadata.len() > maximum {
        return Err(NativeInstallationError::Invalid);
    }
    let file = private_options()
        .read(true)
        .open(path)
        .map_err(|_| NativeInstallationError::Unavailable)?;
    if !file
        .metadata()
        .map_err(|_| NativeInstallationError::Unavailable)?
        .is_file()
    {
        return Err(NativeInstallationError::Invalid);
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| NativeInstallationError::Unavailable)?;
    if bytes.len() as u64 > maximum {
        return Err(NativeInstallationError::Invalid);
    }
    Ok(Some(bytes))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), NativeInstallationError> {
    let mut file = private_options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| NativeInstallationError::Unavailable)?;
    file.write_all(bytes)
        .map_err(|_| NativeInstallationError::Unavailable)?;
    file.sync_all()
        .map_err(|_| NativeInstallationError::Unavailable)?;
    sync_directory(path.parent().ok_or(NativeInstallationError::Invalid)?)
}

fn directory_exists(path: &Path) -> Result<bool, NativeInstallationError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(true),
        Ok(_) => Err(NativeInstallationError::Invalid),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(NativeInstallationError::Unavailable),
    }
}

fn directory_empty(path: &Path) -> Result<bool, NativeInstallationError> {
    match fs::read_dir(path)
        .map_err(|_| NativeInstallationError::Unavailable)?
        .next()
    {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(_)) => Err(NativeInstallationError::Unavailable),
    }
}

fn ensure_directory(path: &Path) -> Result<(), NativeInstallationError> {
    if directory_exists(path)? {
        return Ok(());
    }
    let parent = path.parent().ok_or(NativeInstallationError::Invalid)?;
    ensure_directory(parent)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => sync_directory(parent),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if directory_exists(path)? {
                Ok(())
            } else {
                Err(NativeInstallationError::Invalid)
            }
        }
        Err(_) => Err(NativeInstallationError::Unavailable),
    }
}

fn sync_directory(path: &Path) -> Result<(), NativeInstallationError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| NativeInstallationError::Unavailable)
}
