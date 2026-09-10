use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use floe_agent::{
    AGENT_VERSION, AgentBudget, AgentFailure, AgentSession, DataClass, SessionProtection,
    SessionStore,
};
use floe_domain::PersonId;
use subtle::ConstantTimeEq;
use turso::{Builder, EncryptionOpts};
use uuid::Uuid;
use zeroize::Zeroizing;

mod expert_actions;
mod keyring;
mod learning;
mod registry;
mod session_archive;
pub use keyring::KeyringVaultKeys;
pub use session_archive::*;

pub struct VaultKey(Zeroizing<[u8; 32]>);

impl VaultKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn generate() -> Result<Self, AgentFailure> {
        let mut key = Self::from_bytes([0; 32]);
        getrandom::fill(key.0.as_mut()).map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(key)
    }

    fn hex(&self) -> String {
        const DIGITS: &[u8] = b"0123456789abcdef";
        let mut output = String::with_capacity(64);
        for byte in self.as_bytes() {
            output.push(DIGITS[(byte >> 4) as usize] as char);
            output.push(DIGITS[(byte & 15) as usize] as char);
        }
        output
    }
}

pub trait VaultKeyProvider: Send + Sync {
    fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure>;

    fn insert(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
        key: &VaultKey,
    ) -> Result<(), AgentFailure>;
}

pub struct EncryptedAgentVault<Keys> {
    database: turso::Database,
    key: VaultKey,
    keys: Keys,
    person_id: PersonId,
    vault_id: Uuid,
    unavailable: AtomicBool,
    _host_lock: File,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn create(
        root: &Path,
        person_id: PersonId,
        keys: Keys,
    ) -> Result<Self, AgentFailure> {
        private_directory(root)?;
        let directory = root.join(person_id.to_string());
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::AlreadyExists => AgentFailure::Conflict,
                _ => AgentFailure::VaultUnavailable,
            })?;
        let host_lock = lock_directory(&directory)?;
        let vault_id = Uuid::new_v4();
        let mut marker = private_file()
            .create_new(true)
            .open(directory.join("vault.id"))
            .map_err(unavailable)?;
        marker
            .write_all(vault_id.to_string().as_bytes())
            .map_err(unavailable)?;
        marker.sync_all().map_err(unavailable)?;
        File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(unavailable)?;
        File::open(root)
            .and_then(|root| root.sync_all())
            .map_err(unavailable)?;
        let key = VaultKey::generate()?;
        keys.insert(person_id, vault_id, &key)
            .map_err(unavailable)?;
        let stored_key = keys.load(person_id, vault_id).map_err(unavailable)?;
        if !bool::from(key.as_bytes().ct_eq(stored_key.as_bytes())) {
            return Err(AgentFailure::VaultUnavailable);
        }
        let path = directory.join("sessions.db");
        private_file()
            .create_new(true)
            .open(&path)
            .map_err(unavailable)?;
        let database = encrypted_database(&path, &key).await?;
        let vault = Self {
            database,
            key,
            keys,
            person_id,
            vault_id,
            unavailable: AtomicBool::new(false),
            _host_lock: host_lock,
        };
        let connection = vault.connection()?;
        connection.execute("CREATE TABLE vault_identity (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL, person_id TEXT NOT NULL, vault_id TEXT NOT NULL)", ()).await.map_err(unavailable)?;
        connection
            .execute(
                "INSERT INTO vault_identity VALUES (1, 1, ?, ?)",
                (person_id.to_string(), vault_id.to_string()),
            )
            .await
            .map_err(unavailable)?;
        connection.execute("CREATE TABLE agent_sessions (id TEXT PRIMARY KEY, revision INTEGER NOT NULL, payload TEXT NOT NULL)", ()).await.map_err(unavailable)?;
        vault.initialize_session_archive().await?;
        vault.initialize_learning_store().await?;
        vault.checkpoint().await?;
        File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(unavailable)?;
        Ok(vault)
    }

    pub async fn open(root: &Path, person_id: PersonId, keys: Keys) -> Result<Self, AgentFailure> {
        private_directory(root)?;
        let directory = root.join(person_id.to_string());
        private_directory(&directory)?;
        let host_lock = lock_directory(&directory)?;
        let mut marker = String::new();
        private_file()
            .open(directory.join("vault.id"))
            .map_err(unavailable)?
            .take(37)
            .read_to_string(&mut marker)
            .map_err(unavailable)?;
        if marker.len() != 36 {
            return Err(AgentFailure::VaultUnavailable);
        }
        let vault_id = Uuid::parse_str(&marker).map_err(unavailable)?;
        let path = directory.join("sessions.db");
        for name in ["sessions.db", "sessions.db-wal", "sessions.db-shm"] {
            let candidate = directory.join(name);
            match fs::symlink_metadata(candidate) {
                Ok(metadata)
                    if metadata.is_file() && (name != "sessions.db" || metadata.len() > 0) => {}
                Err(error)
                    if name != "sessions.db" && error.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err(AgentFailure::VaultUnavailable),
            }
        }
        let key = keys.load(person_id, vault_id).map_err(unavailable)?;
        let database = encrypted_database(&path, &key).await?;
        let vault = Self {
            database,
            key,
            keys,
            person_id,
            vault_id,
            unavailable: AtomicBool::new(false),
            _host_lock: host_lock,
        };
        let connection = vault.connection()?;
        let mut rows = connection
            .query(
                "SELECT version, person_id, vault_id FROM vault_identity WHERE id = 1",
                (),
            )
            .await
            .map_err(unavailable)?;
        let identity = rows
            .next()
            .await
            .map_err(unavailable)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        if ![1, 2].contains(&identity.get::<i64>(0).map_err(unavailable)?)
            || identity.get::<String>(1).map_err(unavailable)? != person_id.to_string()
            || identity.get::<String>(2).map_err(unavailable)? != vault_id.to_string()
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        connection
            .query(
                "SELECT id, revision, payload FROM agent_sessions LIMIT 0",
                (),
            )
            .await
            .map_err(unavailable)?;
        vault.initialize_session_archive().await?;
        vault.initialize_learning_store().await?;
        match vault.expert_registry().await {
            Ok(_) => {}
            Err(AgentFailure::VaultUnavailable) => {
                vault.reset_expert_registry().await?;
            }
            Err(failure) => return Err(failure),
        }
        Ok(vault)
    }

    pub async fn create_session(&self) -> Result<AgentSession, AgentFailure> {
        self.insert_session(AgentSession::new(self.person_id)).await
    }

    pub async fn resume_session(&self) -> Result<AgentSession, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT id FROM agent_sessions WHERE json_extract(payload, '$.scope') IS NULL AND json_extract(payload, '$.data_classes[0]') = 'personal' ORDER BY rowid DESC LIMIT 1",
                (),
            )
            .await
            .map_err(storage)?;
        if let Some(row) = rows.next().await.map_err(storage)? {
            let id =
                Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(unavailable)?;
            let session = self.load(self.person_id, id).await?;
            if session.scope.is_some() || session.data_classes != [DataClass::Personal] {
                return Err(AgentFailure::PolicyDenied);
            }
            return Ok(session);
        }
        self.create_session().await
    }

    pub async fn create_sample_session(&self) -> Result<AgentSession, AgentFailure> {
        let mut session = AgentSession::new(self.person_id);
        session.data_classes = vec![DataClass::Synthetic];
        self.insert_session(session).await
    }

    pub async fn resume_sample_session(&self) -> Result<AgentSession, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT id FROM agent_sessions WHERE json_extract(payload, '$.scope') IS NULL AND json_extract(payload, '$.data_classes[0]') = 'synthetic' ORDER BY rowid DESC LIMIT 1",
                (),
            )
            .await
            .map_err(storage)?;
        if let Some(row) = rows.next().await.map_err(storage)? {
            let id =
                Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(unavailable)?;
            let session = self.load(self.person_id, id).await?;
            if session.scope.is_some() || session.data_classes != [DataClass::Synthetic] {
                return Err(AgentFailure::PolicyDenied);
            }
            return Ok(session);
        }
        self.create_sample_session().await
    }

    pub fn check_access(&self) -> Result<(), AgentFailure> {
        self.connection().map(|_| ())
    }

    async fn insert_session(&self, session: AgentSession) -> Result<AgentSession, AgentFailure> {
        let payload = self.payload(&session)?;
        self.connection()?
            .execute(
                "INSERT INTO agent_sessions (id, revision, payload) VALUES (?, 0, ?)",
                (session.id.to_string(), payload),
            )
            .await
            .map_err(storage)?;
        Ok(session)
    }

    pub async fn checkpoint(&self) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
            .await
            .map_err(storage)?;
        while rows.next().await.map_err(storage)?.is_some() {}
        Ok(())
    }

    fn connection(&self) -> Result<turso::Connection, AgentFailure> {
        if self.unavailable.load(Ordering::Acquire) {
            return Err(AgentFailure::VaultUnavailable);
        }
        let key = self.keys.load(self.person_id, self.vault_id);
        if !key.is_ok_and(|key| bool::from(key.as_bytes().ct_eq(self.key.as_bytes()))) {
            self.unavailable.store(true, Ordering::Release);
            return Err(AgentFailure::VaultUnavailable);
        }
        self.database.connect().map_err(storage)
    }

    fn payload(&self, session: &AgentSession) -> Result<String, AgentFailure> {
        if session.person_id != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        if session.schema_version != AGENT_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if session.data_classes.is_empty()
            || session
                .scope
                .is_some_and(|scope| session.data_classes != [scope.data_class()])
            || session
                .data_classes
                .iter()
                .any(|class| matches!(class, DataClass::Credential | DataClass::DeviceOnlyRaw))
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let payload = serde_json::to_string(session).map_err(storage)?;
        if payload.len() > AgentBudget::default().max_session_bytes {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(payload)
    }
}

impl<Keys: VaultKeyProvider> SessionStore for EncryptedAgentVault<Keys> {
    fn protection(&self) -> SessionProtection {
        if self.unavailable.load(Ordering::Acquire) {
            SessionProtection::KeyUnavailable
        } else {
            SessionProtection::Encrypted
        }
    }

    async fn load(
        &self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> Result<AgentSession, AgentFailure> {
        if person_id != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT revision, payload FROM agent_sessions WHERE id = ?",
                [session_id.to_string()],
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        let payload = row.get::<String>(1).map_err(storage)?;
        if payload.len() > AgentBudget::default().max_session_bytes {
            return Err(AgentFailure::BudgetExceeded);
        }
        let session: AgentSession = serde_json::from_str(&payload).map_err(unavailable)?;
        self.payload(&session)?;
        if session.id != session_id
            || i64::try_from(session.revision).ok() != Some(row.get::<i64>(0).map_err(storage)?)
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(session)
    }

    async fn compare_and_swap(
        &self,
        session: &AgentSession,
        previous_revision: u64,
    ) -> Result<(), AgentFailure> {
        if previous_revision.checked_add(1) != Some(session.revision) {
            return Err(AgentFailure::Conflict);
        }
        let revision = i64::try_from(session.revision).map_err(|_| AgentFailure::Conflict)?;
        let previous = i64::try_from(previous_revision).map_err(|_| AgentFailure::Conflict)?;
        let payload = self.payload(session)?;
        let stored = self.load(session.person_id, session.id).await?;
        if stored.revision != previous_revision
            || stored.scope != session.scope
            || session.messages.len() < stored.messages.len()
            || session.messages[..stored.messages.len()] != stored.messages
        {
            return Err(AgentFailure::Conflict);
        }
        if stored
            .data_classes
            .iter()
            .any(|class| !session.data_classes.contains(class))
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let changed = self
            .connection()?
            .execute(
                "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                (revision, payload, session.id.to_string(), previous),
            )
            .await
            .map_err(storage)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }
}

async fn encrypted_database(path: &Path, key: &VaultKey) -> Result<turso::Database, AgentFailure> {
    Builder::new_local(path.to_str().ok_or(AgentFailure::VaultUnavailable)?)
        .experimental_encryption(true)
        .with_encryption(EncryptionOpts {
            cipher: "aes256gcm".into(),
            hexkey: key.hex(),
        })
        .build()
        .await
        .map_err(unavailable)
}

fn private_directory(path: &Path) -> Result<(), AgentFailure> {
    let metadata = fs::symlink_metadata(path).map_err(unavailable)?;
    if !metadata.is_dir()
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

fn private_file() -> OpenOptions {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW);
    options
}

fn lock_directory(directory: &Path) -> Result<File, AgentFailure> {
    let lock = private_file()
        .create(true)
        .truncate(false)
        .open(directory.join("host.lock"))
        .map_err(unavailable)?;
    lock.try_lock().map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => AgentFailure::Conflict,
        std::fs::TryLockError::Error(_) => AgentFailure::VaultUnavailable,
    })?;
    Ok(lock)
}

fn unavailable(_: impl std::fmt::Debug) -> AgentFailure {
    AgentFailure::VaultUnavailable
}

fn storage(_: impl std::fmt::Debug) -> AgentFailure {
    AgentFailure::StorageUnavailable
}
