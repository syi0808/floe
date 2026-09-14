use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    future::Future,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::Path,
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
};

use floe_agent::{
    AGENT_VERSION, AgentBudget, AgentFailure, AgentSession, DataClass, SessionProtection,
    SessionStore,
};
use floe_domain::{ContextDependency, DependencyCoverage, PersonId};
use subtle::ConstantTimeEq;
use turso::{Builder, EncryptionOpts};
use uuid::Uuid;
use zeroize::Zeroizing;

mod access_grants;
mod agent_actions;
mod calendar_grants;
mod context_cleanup;
mod context_dependencies;
mod expert_actions;
mod keyring;
mod learning;
mod personal_grants;
mod registry;
mod remote_authority;
mod remote_calendar_grants;
mod remote_view_grants;
mod session_archive;
pub use access_grants::AccessGrantCleanup;
pub use agent_actions::{AgentActionAdmission, AgentActionEnvelope};
pub use calendar_grants::CalendarGrantAdmission;
pub use keyring::KeyringVaultKeys;
pub use personal_grants::FeasibilityGrantQuery;
pub use remote_authority::{
    RemoteCalendarAuthorizationExpectation, RemoteCalendarSourceReference,
    RemoteEnrollmentSignature, RemoteOwnerPublicKey, RemotePairingChallenge,
    RemoteProducerIdentity, RemoteViewSourceReference,
};
pub use remote_calendar_grants::RemoteCalendarGrantBinding;
pub use remote_view_grants::RemoteViewGrantBinding;
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

pub struct GovernedAgentSessionStore<'vault, Keys> {
    vault: &'vault EncryptedAgentVault<Keys>,
    session_id: Uuid,
    coverage: floe_context::CoverageRegistry,
    liveness: Option<&'vault dyn GovernedDependencyLiveness>,
}

struct VaultTransactionAuthority<'vault, 'transaction, Keys> {
    vault: &'vault EncryptedAgentVault<Keys>,
    transaction: &'transaction turso::transaction::Transaction<'transaction>,
    session_id: Uuid,
    session_revision: u64,
}

impl<Keys: VaultKeyProvider> floe_access::CurrentAuthority
    for VaultTransactionAuthority<'_, '_, Keys>
{
    fn validate_target(
        &self,
        recipient: floe_access::ReleaseRecipient,
        session_id: Uuid,
        session_revision: u64,
    ) -> Result<(), AgentFailure> {
        let floe_access::ReleaseRecipient::Storage {
            person_id,
            vault_id,
        } = recipient;
        self.vault.check_access()?;
        if person_id != self.vault.person_id
            || vault_id != self.vault.vault_id
            || session_id != self.session_id
            || session_revision != self.session_revision
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    fn validate<'a>(
        &'a self,
        dependency: &'a ContextDependency,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
        Box::pin(async move {
            self.vault
                .validate_current_authority_in_transaction(self.transaction, dependency)
                .await
        })
    }
}

impl<Keys: VaultKeyProvider> GovernedAgentSessionStore<'_, Keys> {
    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub async fn record_dependency(
        &self,
        turn_id: Uuid,
        dependency: ContextDependency,
    ) -> Result<(), AgentFailure> {
        if turn_id.is_nil() || dependency.observation_id().is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let stored = if self.coverage.has_turn(turn_id)? {
            None
        } else {
            let session = self
                .vault
                .load(self.vault.person_id, self.session_id)
                .await?;
            if session
                .messages
                .iter()
                .any(|message| message.turn_id() == turn_id)
            {
                Some(
                    self.vault
                        .read_turn_coverage(self.session_id, turn_id)
                        .await?,
                )
            } else {
                None
            }
        };
        self.coverage.record_dependency(turn_id, dependency, stored)
    }

    pub fn record_result_dependency(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
        dependency: ContextDependency,
    ) -> Result<(), AgentFailure> {
        if turn_id.is_nil() || result_id.is_nil() || dependency.observation_id().is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage
            .record_result_dependency(turn_id, result_id, dependency)
    }

    pub fn record_result_independent(
        &self,
        turn_id: Uuid,
        result_id: Uuid,
    ) -> Result<(), AgentFailure> {
        if turn_id.is_nil() || result_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage.record_result_independent(turn_id, result_id)
    }

    pub async fn project_model_request(
        &self,
        request: &mut floe_agent::ModelRequest,
        resolver: Option<&dyn GovernedDependencyResolver>,
    ) -> Result<(), AgentFailure> {
        self.project_model_request_with_coverage(request, resolver)
            .await
            .map(|_| ())
    }

    pub async fn revalidate_current_coverage(
        &self,
        request: &floe_agent::ModelRequest,
        resolver: &dyn GovernedDependencyResolver,
    ) -> Result<(), AgentFailure> {
        if request.session_id != self.session_id {
            return Err(AgentFailure::Conflict);
        }
        let coverage = self.coverage.turn_coverage(request.turn_id)?;
        let coverage = match coverage {
            Some(value) => value,
            None => {
                self.vault
                    .read_turn_coverage(self.session_id, request.turn_id)
                    .await?
            }
        };
        match coverage {
            DependencyCoverage::Independent => Ok(()),
            DependencyCoverage::Unknown => Err(AgentFailure::PolicyDenied),
            DependencyCoverage::Dependent { dependencies } => {
                for dependency in dependencies {
                    resolver.authorize(&dependency, request).await?;
                }
                Ok(())
            }
        }
    }

    pub async fn project_model_request_with_coverage(
        &self,
        request: &mut floe_agent::ModelRequest,
        resolver: Option<&dyn GovernedDependencyResolver>,
    ) -> Result<bool, AgentFailure> {
        if request.session_id != self.session_id {
            return Err(AgentFailure::Conflict);
        }
        let messages = std::mem::take(&mut request.messages);
        let mut coverage_by_turn = HashMap::new();
        for turn_id in messages.iter().map(floe_agent::AgentMessage::turn_id) {
            if !coverage_by_turn.contains_key(&turn_id) {
                let coverage = self
                    .vault
                    .read_turn_coverage(self.session_id, turn_id)
                    .await?;
                coverage_by_turn.insert(turn_id, coverage);
            }
        }
        let mut retained = Vec::with_capacity(messages.len());
        let mut filtered = false;
        for message in messages {
            let turn_id = message.turn_id();
            let coverage = coverage_by_turn
                .get(&turn_id)
                .cloned()
                .unwrap_or(DependencyCoverage::Unknown);
            let allowed = match &coverage {
                DependencyCoverage::Independent => true,
                DependencyCoverage::Unknown => false,
                DependencyCoverage::Dependent { dependencies } => match resolver {
                    Some(resolver) => {
                        let mut allowed = true;
                        for dependency in dependencies {
                            match resolver.authorize(dependency, request).await {
                                Ok(()) => {}
                                Err(AgentFailure::PolicyDenied) => allowed = false,
                                Err(error) => return Err(error),
                            }
                        }
                        allowed
                    }
                    None => false,
                },
            };
            if allowed {
                if let DependencyCoverage::Dependent { dependencies } = coverage {
                    for dependency in dependencies {
                        self.coverage
                            .record_dependency(request.turn_id, dependency, None)
                            .map_err(|error| match error {
                                AgentFailure::InvalidInput => AgentFailure::PolicyDenied,
                                error => error,
                            })?;
                    }
                }
                retained.push(message);
            } else {
                if !matches!(message, floe_agent::AgentMessage::User { .. }) {
                    filtered = true;
                } else {
                    retained.push(message);
                }
            }
        }
        request.messages = retained;
        if filtered || !request.replay.is_empty() {
            request.replay.clear();
        }
        Ok(filtered)
    }
}

impl<Keys: VaultKeyProvider> SessionStore for GovernedAgentSessionStore<'_, Keys> {
    fn protection(&self) -> SessionProtection {
        self.vault.protection()
    }

    async fn load(
        &self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> Result<AgentSession, AgentFailure> {
        self.vault.load(person_id, session_id).await
    }

    async fn compare_and_swap(
        &self,
        session: &AgentSession,
        previous_revision: u64,
    ) -> Result<(), AgentFailure> {
        if session.id != self.session_id {
            return Err(AgentFailure::Conflict);
        }
        let stored = self.vault.load(session.person_id, session.id).await?;
        if stored.revision != previous_revision || session.messages.len() < stored.messages.len() {
            return Err(AgentFailure::Conflict);
        }
        let appended = &session.messages[stored.messages.len()..];
        let mut initial = BTreeMap::new();
        let mut stored_turns = HashMap::new();
        for message in &stored.messages {
            stored_turns.insert(message.turn_id(), ());
        }
        for turn_id in appended.iter().map(floe_agent::AgentMessage::turn_id) {
            if stored_turns.contains_key(&turn_id) && !initial.contains_key(&turn_id) {
                initial.insert(
                    turn_id,
                    self.vault.read_turn_coverage(session.id, turn_id).await?,
                );
            }
        }
        let facts = appended
            .iter()
            .map(|message| floe_context::CoverageMessageFact {
                turn_id: message.turn_id(),
                existing_turn: stored_turns.contains_key(&message.turn_id()),
                is_user: matches!(message, floe_agent::AgentMessage::User { .. }),
                result_id: match message {
                    floe_agent::AgentMessage::Capability { call_id, .. } => Some(*call_id),
                    floe_agent::AgentMessage::Delegation { task, .. } => Some(task.id),
                    _ => None,
                },
            })
            .collect::<Vec<_>>();
        let snapshot = self.coverage.fold_messages(&facts, &initial)?;
        self.vault
            .compare_and_swap_checked_with_liveness(
                session,
                previous_revision,
                &snapshot,
                self.liveness,
                Some(floe_access::ReleaseRecipient::Storage {
                    person_id: session.person_id,
                    vault_id: self.vault.vault_id,
                }),
            )
            .await
    }
}

pub trait GovernedDependencyResolver: Send + Sync {
    fn authorize<'a>(
        &'a self,
        dependency: &'a ContextDependency,
        request: &'a floe_agent::ModelRequest,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>>;
}

pub trait GovernedDependencyLiveness: Send + Sync {
    fn validate(&self, dependency: &ContextDependency) -> Result<(), AgentFailure>;
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
        vault.initialize_access_grant_store().await?;
        vault.initialize_agent_action_store().await?;
        vault.initialize_personal_grant_store(true).await?;
        vault.initialize_calendar_grant_store().await?;
        vault.initialize_remote_calendar_grant_store(true).await?;
        vault.initialize_remote_view_grant_store(true).await?;
        vault.initialize_context_dependencies().await?;
        vault.initialize_context_cleanup(true).await?;
        vault.initialize_remote_authority_store(true).await?;
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
        vault.initialize_access_grant_store().await?;
        vault.initialize_agent_action_store().await?;
        vault.initialize_personal_grant_store(false).await?;
        vault.initialize_calendar_grant_store().await?;
        vault.initialize_remote_calendar_grant_store(false).await?;
        vault.initialize_remote_view_grant_store(false).await?;
        vault.initialize_context_dependencies().await?;
        vault.initialize_context_cleanup(false).await?;
        vault.initialize_remote_authority_store(false).await?;
        vault.expert_registry().await?;
        Ok(vault)
    }

    pub async fn create_session(&self) -> Result<AgentSession, AgentFailure> {
        self.insert_session(AgentSession::new(self.person_id)).await
    }

    pub fn governed_general_store(&self, session_id: Uuid) -> GovernedAgentSessionStore<'_, Keys> {
        GovernedAgentSessionStore {
            vault: self,
            session_id,
            coverage: floe_context::CoverageRegistry::new(),
            liveness: None,
        }
    }

    pub fn governed_general_store_with_liveness<'vault>(
        &'vault self,
        session_id: Uuid,
        liveness: &'vault dyn GovernedDependencyLiveness,
    ) -> GovernedAgentSessionStore<'vault, Keys> {
        let mut store = self.governed_general_store(session_id);
        store.liveness = Some(liveness);
        store
    }

    pub(crate) async fn read_turn_coverage(
        &self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> Result<DependencyCoverage, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = context_dependencies::read_context_dependency_coverage(
            &transaction,
            self.person_id,
            session_id,
            turn_id,
        )
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
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

    async fn initialize_context_dependencies(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(|error| match error {
                turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => AgentFailure::Conflict,
                _ => AgentFailure::StorageUnavailable,
            })?;
        let result = context_dependencies::initialize_context_dependency_store(&transaction).await;
        self.finish_access_grant_transaction(transaction, result)
            .await
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
        self.compare_and_swap_checked(session, previous_revision, &BTreeMap::new())
            .await
    }
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(crate) async fn compare_and_swap_checked(
        &self,
        session: &AgentSession,
        previous_revision: u64,
        coverage: &BTreeMap<Uuid, DependencyCoverage>,
    ) -> Result<(), AgentFailure> {
        self.compare_and_swap_checked_with_liveness(
            session,
            previous_revision,
            coverage,
            None,
            None,
        )
        .await
    }

    async fn compare_and_swap_checked_with_liveness(
        &self,
        session: &AgentSession,
        previous_revision: u64,
        coverage: &BTreeMap<Uuid, DependencyCoverage>,
        liveness: Option<&dyn GovernedDependencyLiveness>,
        release_recipient: Option<floe_access::ReleaseRecipient>,
    ) -> Result<(), AgentFailure> {
        if previous_revision.checked_add(1) != Some(session.revision) {
            return Err(AgentFailure::Conflict);
        }
        let revision = i64::try_from(session.revision).map_err(|_| AgentFailure::Conflict)?;
        let previous = i64::try_from(previous_revision).map_err(|_| AgentFailure::Conflict)?;
        self.payload(session)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(|error| match error {
                turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => AgentFailure::Conflict,
                _ => AgentFailure::StorageUnavailable,
            })?;
        let result = async {
            let mut candidate = session.clone();
            let stored = self.session_on(&transaction, session.id).await?;
            if stored.revision != previous_revision
                || stored.scope != candidate.scope
                || candidate.messages.len() < stored.messages.len()
                || candidate.messages[..stored.messages.len()] != stored.messages
            {
                return Err(AgentFailure::Conflict);
            }
            if stored
                .data_classes
                .iter()
                .any(|class| !candidate.data_classes.contains(class))
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let mut turns = std::collections::BTreeSet::new();
            let mut release_permits = Vec::new();
            let authority = VaultTransactionAuthority {
                vault: self,
                transaction: &transaction,
                session_id: candidate.id,
                session_revision: candidate.revision,
            };
            for message in &candidate.messages[stored.messages.len()..] {
                if turns.insert(message.turn_id()) {
                    let turn_coverage = coverage
                        .get(&message.turn_id())
                        .cloned()
                        .unwrap_or(DependencyCoverage::Unknown);
                    if let Some(recipient) = release_recipient {
                        release_permits.push(
                            floe_access::admit_release(
                                &turn_coverage,
                                recipient,
                                candidate.id,
                                candidate.revision,
                                &authority,
                            )
                            .await?,
                        );
                    }
                    context_dependencies::merge_context_dependency_coverage(
                        &transaction,
                        candidate.person_id,
                        candidate.id,
                        message.turn_id(),
                        turn_coverage,
                    )
                    .await?;
                }
            }
            self.sanitize_session_for_context_cleanup(&transaction, &mut candidate)
                .await?;
            let payload = self.payload(&candidate)?;
            for permit in release_permits {
                floe_access::consume_release(permit).await?;
            }
            if let Some(liveness) = liveness {
                for turn_id in turns {
                    if let Some(DependencyCoverage::Dependent { dependencies }) =
                        coverage.get(&turn_id)
                    {
                        for dependency in dependencies {
                            liveness.validate(dependency)?;
                        }
                    }
                }
            }
            let changed = transaction
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
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
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
    #[cfg(target_os = "android")]
    {
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(match std::io::Error::last_os_error().kind() {
                std::io::ErrorKind::WouldBlock => AgentFailure::Conflict,
                _ => AgentFailure::VaultUnavailable,
            });
        }
    }
    #[cfg(not(target_os = "android"))]
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
