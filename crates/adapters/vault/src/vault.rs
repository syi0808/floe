use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    future::Future,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::Path,
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::RootKey;
use floe_access::{ContextDependency, DependencyCoverage};
use floe_agent_contract::{AgentFailure, DataClass, SessionProtection};
use floe_conversation::{AgentBudget, AgentSession, SessionStore};
#[cfg(test)]
use floe_kernel::AGENT_VERSION;
use floe_kernel::PersonId;
#[cfg(test)]
use std::sync::atomic::AtomicU8;
use subtle::ConstantTimeEq;
use turso::{Builder, EncryptionOpts};
use uuid::Uuid;

mod access_grants;
mod agent_actions;
mod authority_keys;
mod connection_reviews;
mod context_cleanup;
mod context_dependencies;
mod creation;
pub use creation::{VaultPresence, inspect_vault_presence};
mod conversation_core;
mod conversation_core_reads;
mod conversation_delegation_recovery;
mod conversation_interactions;
mod conversations;
mod expert_actions;
pub(crate) mod expert_binding_reviews;
mod owner_custody;
mod owner_transcript_reads;
mod typed_history;
pub use expert_actions::VaultExpertProposalReader;
#[cfg(feature = "development-storage")]
pub(crate) mod development_keys;
mod gateway_authority;
mod gateway_pairing_store;
#[cfg(feature = "os-keyring")]
pub(crate) mod keyring;
mod learning;
mod registry;
pub use gateway_authority::{VaultAuthorizationSigner, VaultEnrollmentSigner};
mod schema_lifecycle;
mod session_archive;
mod tasks;
pub use access_grants::AccessGrantCleanup;
pub use conversations::{
    VaultConversationActivation, VaultConversationAdmission, VaultConversationCancelAdmission,
    VaultConversationCancelReceipt, VaultConversationCancelRequest, VaultConversationJournalEntry,
    VaultManagerConversationAdmission,
};
#[cfg(feature = "development-storage")]
pub use development_keys::DevelopmentVaultKeys;
#[cfg(feature = "os-keyring")]
pub use keyring::KeyringVaultKeys;

/// Exact key-read classification shared by custody adapters.
pub enum VaultKeyReadFailure {
    Missing,
    Malformed,
    Unavailable(AgentFailure),
}
pub use session_archive::*;

pub trait VaultKeyProvider: Send + Sync {
    fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<RootKey, AgentFailure>;

    /// Classified exact-key reads use the same custody as ordinary open.
    /// Only proven absence or malformed bytes receive those classifications.
    fn inspect_existing(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
    ) -> Result<RootKey, VaultKeyReadFailure> {
        self.load(person_id, vault_id)
            .map_err(VaultKeyReadFailure::Unavailable)
    }

    fn insert(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
        key: &RootKey,
    ) -> Result<(), AgentFailure>;
}

pub struct EncryptedAgentVault<Keys> {
    database: turso::Database,
    key: RootKey,
    keys: Keys,
    person_id: PersonId,
    vault_id: Uuid,
    unavailable: AtomicBool,
    journal_writes: tokio::sync::Mutex<()>,
    conversation_executor_generation: AtomicU64,
    task_executor_generation: AtomicU64,
    #[cfg(test)]
    conversation_core_ack_loss: AtomicBool,
    #[cfg(test)]
    conversation_core_failure_before_pending: AtomicBool,
    #[cfg(test)]
    conversation_core_resume_fault_after_owner_claim: AtomicBool,
    #[cfg(test)]
    conversation_core_fault_after_input_mapping: AtomicBool,
    #[cfg(test)]
    conversation_core_fault_after_recorder_open: AtomicBool,
    #[cfg(test)]
    conversation_core_fault_after_owner_settlement: AtomicBool,
    #[cfg(test)]
    conversation_recovery_fault_stage: AtomicU8,
    #[cfg(test)]
    conversation_core_typed_write_fault: AtomicU8,
    #[cfg(test)]
    typed_history_payload_hydrations: AtomicU64,
    #[cfg(test)]
    context_coverage_payload_hydrations: AtomicU64,
    #[cfg(test)]
    owner_transcript_entry_hydrations: AtomicU64,
    _host_lock: File,
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
                .validate_current_grant_in_transaction(self.transaction, dependency)
                .await
        })
    }
}

/// The Session store a governed turn runs against is Conversation's; this
/// adapter only supplies the storage behind it.
pub use floe_context::{DependencyLiveness, DependencyResolver};

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Seal this exact opened generation. Retained owner handles fail all
    /// existing access fences; no file, key or uncertain effect is removed.
    pub fn seal(&self) {
        self.unavailable.store(true, Ordering::Release);
    }

    pub fn person_id(&self) -> PersonId {
        self.person_id
    }

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
        let host_lock = lock_directory(&directory, true)?;
        let vault_id = Uuid::new_v4();
        let pending_creation = creation::PendingCreation::begin(&directory, person_id, vault_id)?;
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
        let key = RootKey::generate()?;
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
            journal_writes: tokio::sync::Mutex::new(()),
            conversation_executor_generation: AtomicU64::new(0),
            task_executor_generation: AtomicU64::new(0),
            #[cfg(test)]
            conversation_core_ack_loss: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_failure_before_pending: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_resume_fault_after_owner_claim: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_fault_after_input_mapping: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_fault_after_recorder_open: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_fault_after_owner_settlement: AtomicBool::new(false),
            #[cfg(test)]
            conversation_recovery_fault_stage: AtomicU8::new(0),
            #[cfg(test)]
            conversation_core_typed_write_fault: AtomicU8::new(0),
            #[cfg(test)]
            typed_history_payload_hydrations: AtomicU64::new(0),
            #[cfg(test)]
            context_coverage_payload_hydrations: AtomicU64::new(0),
            #[cfg(test)]
            owner_transcript_entry_hydrations: AtomicU64::new(0),
            _host_lock: host_lock,
        };
        vault.create_schema().await?;
        vault.validate_stored_records().await?;
        vault.checkpoint().await?;
        File::open(&path)
            .and_then(|file| file.sync_all())
            .map_err(unavailable)?;
        File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(unavailable)?;
        pending_creation.finish()?;
        Ok(vault)
    }

    pub async fn open(root: &Path, person_id: PersonId, keys: Keys) -> Result<Self, AgentFailure> {
        private_directory(root)?;
        let directory = root.join(person_id.to_string());
        private_directory(&directory)?;
        let host_lock = lock_directory(&directory, false)?;
        creation::ensure_complete(&directory, person_id)?;
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
            journal_writes: tokio::sync::Mutex::new(()),
            conversation_executor_generation: AtomicU64::new(0),
            task_executor_generation: AtomicU64::new(0),
            #[cfg(test)]
            conversation_core_ack_loss: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_failure_before_pending: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_resume_fault_after_owner_claim: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_fault_after_input_mapping: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_fault_after_recorder_open: AtomicBool::new(false),
            #[cfg(test)]
            conversation_core_fault_after_owner_settlement: AtomicBool::new(false),
            #[cfg(test)]
            conversation_recovery_fault_stage: AtomicU8::new(0),
            #[cfg(test)]
            conversation_core_typed_write_fault: AtomicU8::new(0),
            #[cfg(test)]
            typed_history_payload_hydrations: AtomicU64::new(0),
            #[cfg(test)]
            context_coverage_payload_hydrations: AtomicU64::new(0),
            #[cfg(test)]
            owner_transcript_entry_hydrations: AtomicU64::new(0),
            _host_lock: host_lock,
        };
        let connection = vault.connection()?;
        crate::schema::inspect(&connection, crate::schema::Layout::Encrypted)
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?;
        let mut rows = connection
            .query(
                "SELECT version, person_id, vault_id FROM vault_identity WHERE id = 1",
                (),
            )
            .await
            .map_err(database_failure)?;
        let identity = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        if identity.get::<i64>(0).map_err(unavailable)? != crate::schema::ENCRYPTED_LAYOUT_VERSION
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
            .map_err(database_failure)?;
        vault.validate_stored_records().await?;
        Ok(vault)
    }

    async fn validate_stored_records(&self) -> Result<(), AgentFailure> {
        self.validate_session_records().await?;
        self.validate_access_grant_store().await?;
        self.validate_actions_store().await?;
        self.validate_expert_binding_reviews().await?;
        self.validate_context_dependencies().await?;
        let connection = self.connection()?;
        if crate::schema::conversation_core_family_present(&connection)
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?
        {
            self.validate_conversation_core_store().await?;
        }
        conversations::validate_schema(&connection).await?;
        tasks::validate_schema(&connection).await?;
        self.validate_context_cleanup().await?;
        self.validate_current_enrollment_key().await?;
        self.expert_registry().await?;
        self.check_access()
    }

    /// The Session store Conversation drives, backed by this vault.
    pub fn governed_general_store(
        &self,
        session_id: Uuid,
    ) -> floe_conversation::GovernedSessionStore<'_, Self> {
        floe_conversation::GovernedSessionStore::new(self, session_id)
    }

    pub fn governed_general_store_with_liveness<'vault>(
        &'vault self,
        session_id: Uuid,
        liveness: &'vault dyn DependencyLiveness,
    ) -> floe_conversation::GovernedSessionStore<'vault, Self> {
        self.governed_general_store(session_id)
            .with_liveness(liveness)
    }

    pub(crate) async fn read_turn_coverage(
        &self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> Result<DependencyCoverage, AgentFailure> {
        // One snapshot SELECT, deliberately without a write transaction: the
        // proposal gate resolves dependencies while holding its own Immediate
        // transaction, and a nested writer Busy-fails.
        let connection = self.connection()?;
        let coverage = context_dependencies::read_context_dependency_coverage(
            &connection,
            self.person_id,
            session_id,
            turn_id,
        )
        .await?;
        self.check_access()?;
        Ok(coverage)
    }

    pub(crate) async fn journal_transaction<'v, 'c>(
        &'v self,
        connection: &'c mut turso::Connection,
    ) -> Result<
        (
            crate::write_fence::JournalWriteGuard<'v>,
            turso::transaction::Transaction<'c>,
        ),
        AgentFailure,
    > {
        use crate::write_fence::{JournalWriteGuard, WriteAdmissionFailure};
        self.check_access()?;
        let mut writer = JournalWriteGuard::acquire(&self.unavailable, &self.journal_writes)
            .map_err(|failure| match failure {
                WriteAdmissionFailure::Busy => AgentFailure::StorageBusy,
                WriteAdmissionFailure::Unavailable => AgentFailure::VaultUnavailable,
            })?;
        writer.arm().map_err(|_| AgentFailure::VaultUnavailable)?;
        match connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
        {
            Ok(transaction) => Ok((writer, transaction)),
            Err(error) => {
                let failure = database_failure(error);
                if failure == AgentFailure::StorageBusy {
                    writer.settled();
                }
                Err(failure)
            }
        }
    }

    pub fn check_access(&self) -> Result<(), AgentFailure> {
        self.connection().map(|_| ())
    }

    pub async fn checkpoint(&self) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
            .await
            .map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if row.get::<i64>(0).map_err(storage)? != 0
            || rows.next().await.map_err(database_failure)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }

    async fn validate_context_dependencies(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let result = context_dependencies::validate_context_dependency_store(&transaction).await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub(crate) fn connection(&self) -> Result<turso::Connection, AgentFailure> {
        if self.unavailable.load(Ordering::Acquire) {
            return Err(AgentFailure::VaultUnavailable);
        }
        let key = self.keys.load(self.person_id, self.vault_id);
        if !key.is_ok_and(|key| bool::from(key.as_bytes().ct_eq(self.key.as_bytes()))) {
            self.unavailable.store(true, Ordering::Release);
            return Err(AgentFailure::VaultUnavailable);
        }
        self.database.connect().map_err(database_failure)
    }

    fn payload(&self, session: &AgentSession) -> Result<String, AgentFailure> {
        session.validate_owner_snapshot(self.person_id)?;
        if session.scope.is_none()
            && session.data_classes == [DataClass::Personal]
            && !session.messages.is_empty()
        {
            return Err(AgentFailure::UnsupportedVersion);
        }
        let payload = serde_json::to_string(session).map_err(storage)?;
        validate_session_payload_size(&payload)?;
        Ok(payload)
    }

    fn decode_session_payload(&self, payload: &str) -> Result<AgentSession, AgentFailure> {
        validate_session_payload_size(payload)?;
        let session: AgentSession = serde_json::from_str(payload).map_err(unavailable)?;
        if session.scope.is_none()
            && session.data_classes == [DataClass::Personal]
            && !session.messages.is_empty()
        {
            return Err(AgentFailure::UnsupportedVersion);
        }
        self.payload(&session)?;
        Ok(session)
    }
}

fn validate_session_payload_size(payload: &str) -> Result<(), AgentFailure> {
    // `str::len` measures UTF-8 bytes, matching the serialized bytes stored in Vault.
    if payload.as_bytes().len() > AgentBudget::default().max_session_bytes {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(())
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
        if let Some(session) = self.read_manager_session_shell(session_id).await? {
            return Ok(session);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT revision, payload FROM agent_sessions WHERE id = ?",
                [session_id.to_string()],
            )
            .await
            .map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::NotFound)?;
        let payload = row.get::<String>(1).map_err(storage)?;
        let session = self.decode_session_payload(&payload)?;
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
        liveness: Option<&dyn DependencyLiveness>,
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
            .map_err(database_failure)?;
        let result = async {
            let candidate = session.clone();
            let stored = if let Some(stored) = self
                .manager_session_shell_on(&transaction, session.id)
                .await
                .map_err(crate::vault::conversation_core::owner_store_failure)?
            {
                if !candidate.messages.is_empty() {
                    return Err(AgentFailure::UnsupportedVersion);
                }
                stored
            } else {
                self.session_on(&transaction, session.id).await?
            };
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
                .map_err(database_failure)?;
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

async fn encrypted_database(path: &Path, key: &RootKey) -> Result<turso::Database, AgentFailure> {
    Builder::new_local(path.to_str().ok_or(AgentFailure::VaultUnavailable)?)
        .experimental_encryption(true)
        .with_encryption(EncryptionOpts {
            cipher: "aes256gcm".into(),
            hexkey: key.hex(),
        })
        .build()
        .await
        .map_err(database_failure)
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

fn lock_directory(directory: &Path, creating: bool) -> Result<File, AgentFailure> {
    let lock = private_file()
        .create_new(creating)
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

// Contention is not a semantic rejection. Commit/rollback uncertainty is handled
// by each transaction finisher and must not be reclassified as safe to retry.
pub(crate) fn database_failure(error: turso::Error) -> AgentFailure {
    crate::schema::SchemaFailure::from_database(error).into_agent()
}

fn storage(_: impl std::fmt::Debug) -> AgentFailure {
    AgentFailure::StorageUnavailable
}

/// The storage behind a governed Session turn.
///
/// The SQL transaction, the compare-and-swap, the authority re-check and the
/// encryption stay here; which messages that turn may show a model, and what its
/// coverage means, do not.
impl<Keys: VaultKeyProvider> floe_conversation::GovernedSessionRepository
    for EncryptedAgentVault<Keys>
{
    fn protection(&self) -> SessionProtection {
        <Self as SessionStore>::protection(self)
    }

    fn person_id(&self) -> PersonId {
        self.person_id
    }

    fn load<'a>(
        &'a self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> floe_agent_contract::BoxFuture<'a, Result<AgentSession, AgentFailure>> {
        Box::pin(async move { <Self as SessionStore>::load(self, person_id, session_id).await })
    }

    fn read_turn_coverage<'a>(
        &'a self,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> floe_agent_contract::BoxFuture<'a, Result<DependencyCoverage, AgentFailure>> {
        Box::pin(async move { self.read_turn_coverage(session_id, turn_id).await })
    }

    fn commit_session_with_coverage<'a>(
        &'a self,
        session: &'a AgentSession,
        previous_revision: u64,
        coverage: &'a BTreeMap<Uuid, DependencyCoverage>,
        liveness: Option<&'a dyn DependencyLiveness>,
    ) -> floe_agent_contract::BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.compare_and_swap_checked_with_liveness(
                session,
                previous_revision,
                coverage,
                liveness,
                Some(floe_access::ReleaseRecipient::Storage {
                    person_id: session.person_id,
                    vault_id: self.vault_id,
                }),
            )
            .await
        })
    }
}

#[cfg(test)]
mod session_payload_tests {
    use std::{collections::HashMap, os::unix::fs::PermissionsExt, path::PathBuf, sync::Mutex};

    use super::*;
    use floe_conversation::{AgentMessage, AgentSessionScope};

    const OLD_SESSION_QUERY_LIMIT: usize = 256 * 1024;
    const CURRENT_SESSION_LIMIT: usize = 2_097_152;

    #[derive(Default)]
    struct TestKeys(Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>);

    impl VaultKeyProvider for TestKeys {
        fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<RootKey, AgentFailure> {
            self.0
                .lock()
                .map_err(|_| AgentFailure::VaultUnavailable)?
                .get(&(person_id, vault_id))
                .copied()
                .map(RootKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
            key: &RootKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .map_err(|_| AgentFailure::VaultUnavailable)?
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("floe-session-test-{}", Uuid::new_v4()));
            std::fs::create_dir(&path).expect("create isolated Vault test root");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("restrict isolated Vault test root");
            Self(path)
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct SessionVault {
        // Drop the Vault's open database and host lock before removing its root.
        vault: EncryptedAgentVault<TestKeys>,
        _root: TestRoot,
        person_id: PersonId,
    }

    impl SessionVault {
        async fn new() -> Self {
            let root = TestRoot::new();
            let person_id = PersonId::new();
            let vault = EncryptedAgentVault::create(&root.0, person_id, TestKeys::default())
                .await
                .expect("create isolated encrypted Vault");
            Self {
                vault,
                _root: root,
                person_id,
            }
        }

        async fn insert_payload(&self, id: Uuid, row_revision: i64, payload: String) {
            let connection = self.vault.connection().expect("connect to test Vault");
            connection
                .execute(
                    "INSERT INTO agent_sessions (id, revision, payload) VALUES (?, ?, ?)",
                    (id.to_string(), row_revision, payload),
                )
                .await
                .expect("insert exact session fixture");
        }

        async fn load_transactionally(&self, id: Uuid) -> Result<AgentSession, AgentFailure> {
            let connection = self.vault.connection()?;
            self.vault.session_on(&connection, id).await
        }

        async fn load(&self, person_id: PersonId, id: Uuid) -> Result<AgentSession, AgentFailure> {
            <EncryptedAgentVault<TestKeys> as SessionStore>::load(&self.vault, person_id, id).await
        }
    }

    fn session_with_exact_payload_bytes(
        person_id: PersonId,
        payload_bytes: usize,
    ) -> (AgentSession, String) {
        let mut session = AgentSession::new(person_id);
        session.scope = Some(AgentSessionScope::Calendar {
            setup_id: Uuid::new_v4(),
            provider: floe_agent_contract::CalendarProvider::Fixture,
        });
        session.data_classes = vec![DataClass::Synthetic];
        session.messages.push(AgentMessage::User {
            turn_id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            text: String::new(),
        });
        let empty_payload = serde_json::to_string(&session).expect("serialize empty fixture");
        let fill_bytes = payload_bytes
            .checked_sub(empty_payload.len())
            .expect("boundary leaves room for realistic session JSON");
        let mut text = "é".repeat(fill_bytes / 2);
        if fill_bytes % 2 != 0 {
            text.push('x');
        }
        let AgentMessage::User { text: content, .. } = &mut session.messages[0] else {
            unreachable!("the fixture starts with a User message");
        };
        *content = text;

        let payload = serde_json::to_string(&session).expect("serialize exact-size fixture");
        assert_eq!(payload.as_bytes().len(), payload_bytes);
        assert!(content_is_multibyte(&session));
        (session, payload)
    }

    fn content_is_multibyte(session: &AgentSession) -> bool {
        match session.messages.first() {
            Some(AgentMessage::User { text, .. }) => text.len() > text.chars().count(),
            _ => false,
        }
    }

    fn session_with_bounded_message_history(person_id: PersonId) -> (AgentSession, String) {
        let mut session = AgentSession::new(person_id);
        session.scope = Some(AgentSessionScope::Calendar {
            setup_id: Uuid::new_v4(),
            provider: floe_agent_contract::CalendarProvider::Fixture,
        });
        session.data_classes = vec![DataClass::Synthetic];
        for turn_number in 0..32 {
            let turn_id = Uuid::new_v4();
            let user_prefix = format!("Question for turn {turn_number}: ");
            let user_text = format!("{user_prefix}{}", "u".repeat(4_096 - user_prefix.len()));
            session.messages.push(AgentMessage::User {
                turn_id,
                message_id: Uuid::new_v4(),
                text: user_text,
            });

            let assistant_prefix = format!("Draft response for turn {turn_number}: ");
            let assistant_text = format!(
                "{assistant_prefix}{}",
                "a".repeat(4_096 - assistant_prefix.len())
            );
            session.messages.push(AgentMessage::Assistant {
                turn_id,
                text: assistant_text,
            });
        }

        let payload = serde_json::to_string(&session).expect("serialize bounded history fixture");
        assert_eq!(session.messages.len(), 64);
        assert!(session.messages.len() < 128);
        assert!(payload.as_bytes().len() > OLD_SESSION_QUERY_LIMIT);
        assert!(payload.as_bytes().len() <= CURRENT_SESSION_LIMIT);
        for message in &session.messages {
            match message {
                AgentMessage::User { text, .. } => {
                    assert_eq!(text.len(), 4_096);
                    assert!(text.len() <= floe_conversation::MAX_TURN_TEXT_BYTES);
                }
                AgentMessage::Assistant { text, .. } => {
                    assert_eq!(text.len(), 4_096);
                    assert!(text.len() <= floe_agent_contract::MAX_OUTPUT_BYTES);
                }
                _ => unreachable!("the bounded transcript uses User and Assistant messages"),
            }
        }
        (session, payload)
    }

    #[tokio::test]
    async fn transactional_reads_accept_payloads_at_and_below_256_kibibytes() {
        let store = SessionVault::new().await;
        assert_eq!(
            AgentBudget::default().max_session_bytes,
            CURRENT_SESSION_LIMIT
        );

        for size in [OLD_SESSION_QUERY_LIMIT - 1, OLD_SESSION_QUERY_LIMIT] {
            let (session, payload) = session_with_exact_payload_bytes(store.person_id, size);
            assert_eq!(payload.as_bytes().len(), size);
            store.insert_payload(session.id, 0, payload).await;

            assert_eq!(
                store.load_transactionally(session.id).await,
                Ok(session.clone()),
                "transactional read should accept serialized payload size {size}"
            );
        }
    }

    #[tokio::test]
    async fn session_store_load_accepts_payloads_above_256_kibibytes_through_two_mibibytes() {
        let store = SessionVault::new().await;
        assert_eq!(
            AgentBudget::default().max_session_bytes,
            CURRENT_SESSION_LIMIT
        );

        for size in [OLD_SESSION_QUERY_LIMIT + 1, CURRENT_SESSION_LIMIT] {
            let (session, payload) = session_with_exact_payload_bytes(store.person_id, size);
            assert_eq!(payload.as_bytes().len(), size);
            store.insert_payload(session.id, 0, payload).await;

            assert_eq!(
                store.load(store.person_id, session.id).await,
                Ok(session.clone()),
                "SessionStore::load should accept serialized payload size {size}"
            );
        }
    }

    #[tokio::test]
    async fn transactional_cas_can_update_a_session_above_256_kibibytes() {
        let store = SessionVault::new().await;
        let (session, payload) =
            session_with_exact_payload_bytes(store.person_id, OLD_SESSION_QUERY_LIMIT + 1);
        store.insert_payload(session.id, 0, payload).await;

        let mut updated = session.clone();
        updated.revision = 1;
        updated.messages.push(AgentMessage::User {
            turn_id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            text: "A second person-provided message.".to_owned(),
        });
        <EncryptedAgentVault<TestKeys> as SessionStore>::compare_and_swap(
            &store.vault,
            &updated,
            0,
        )
        .await
        .expect("CAS should read and update a valid session over the old query limit");

        assert_eq!(store.load(store.person_id, session.id).await, Ok(updated));
    }

    #[tokio::test]
    async fn transactional_cas_reads_realistic_bounded_history_above_256_kibibytes() {
        let store = SessionVault::new().await;
        let (session, payload) = session_with_bounded_message_history(store.person_id);
        store.insert_payload(session.id, 0, payload).await;

        let mut updated = session.clone();
        updated.revision = 1;
        updated.messages.push(AgentMessage::User {
            turn_id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            text: "Follow-up question.".to_owned(),
        });
        assert!(updated.messages.len() < 128);
        <EncryptedAgentVault<TestKeys> as SessionStore>::compare_and_swap(
            &store.vault,
            &updated,
            0,
        )
        .await
        .expect("CAS should read and update a bounded multi-message history");

        assert_eq!(
            store.load_transactionally(session.id).await,
            Ok(updated),
            "the transactional reader should retain the admitted history"
        );
    }

    #[tokio::test]
    async fn transactional_cas_accepts_the_inclusive_two_mibibyte_write_limit() {
        let store = SessionVault::new().await;
        let (session, payload) =
            session_with_exact_payload_bytes(store.person_id, CURRENT_SESSION_LIMIT);
        assert_eq!(payload.as_bytes().len(), CURRENT_SESSION_LIMIT);
        store.insert_payload(session.id, 0, payload).await;

        let mut updated = session;
        updated.revision = 1;
        assert_eq!(
            serde_json::to_string(&updated)
                .expect("serialize revision update")
                .as_bytes()
                .len(),
            CURRENT_SESSION_LIMIT
        );
        <EncryptedAgentVault<TestKeys> as SessionStore>::compare_and_swap(
            &store.vault,
            &updated,
            0,
        )
        .await
        .expect("the current session byte limit is inclusive");
        assert_eq!(store.load(store.person_id, updated.id).await, Ok(updated));
    }

    #[tokio::test]
    async fn oversized_reads_and_writes_return_budget_exceeded_before_json_decode() {
        let store = SessionVault::new().await;
        let (oversized_session, oversized_payload) =
            session_with_exact_payload_bytes(store.person_id, CURRENT_SESSION_LIMIT + 1);
        store
            .insert_payload(oversized_session.id, 0, oversized_payload.clone())
            .await;

        assert_eq!(
            store.load(store.person_id, oversized_session.id).await,
            Err(AgentFailure::BudgetExceeded)
        );
        assert_eq!(
            store.load_transactionally(oversized_session.id).await,
            Err(AgentFailure::BudgetExceeded)
        );

        let mut oversized_candidate = oversized_session.clone();
        oversized_candidate.revision = 1;
        assert_eq!(
            serde_json::to_string(&oversized_candidate)
                .expect("serialize oversized write fixture")
                .as_bytes()
                .len(),
            CURRENT_SESSION_LIMIT + 1
        );
        assert_eq!(
            <EncryptedAgentVault<TestKeys> as SessionStore>::compare_and_swap(
                &store.vault,
                &oversized_candidate,
                0,
            )
            .await,
            Err(AgentFailure::BudgetExceeded)
        );

        let malformed_id = Uuid::new_v4();
        store
            .insert_payload(malformed_id, 0, "{".repeat(CURRENT_SESSION_LIMIT + 1))
            .await;
        assert_eq!(
            store.load(store.person_id, malformed_id).await,
            Err(AgentFailure::BudgetExceeded)
        );
        assert_eq!(
            store.load_transactionally(malformed_id).await,
            Err(AgentFailure::BudgetExceeded)
        );
    }

    #[tokio::test]
    async fn session_reads_preserve_absence_person_schema_and_revision_checks() {
        let store = SessionVault::new().await;
        let missing_id = Uuid::new_v4();
        assert_eq!(
            store.load(store.person_id, missing_id).await,
            Err(AgentFailure::NotFound)
        );
        assert_eq!(
            store.load_transactionally(missing_id).await,
            Err(AgentFailure::NotFound)
        );

        let (valid, payload) = session_with_exact_payload_bytes(store.person_id, 1_024);
        store.insert_payload(valid.id, 1, payload).await;
        assert_eq!(
            store.load(PersonId::new(), valid.id).await,
            Err(AgentFailure::NotFound),
            "the requested caller remains person-scoped"
        );
        assert_eq!(
            store.load(store.person_id, valid.id).await,
            Err(AgentFailure::VaultUnavailable),
            "the stored row revision remains checked"
        );

        let mut wrong_person = valid.clone();
        wrong_person.id = Uuid::new_v4();
        wrong_person.person_id = PersonId::new();
        let wrong_person_payload =
            serde_json::to_string(&wrong_person).expect("serialize wrong-person fixture");
        store
            .insert_payload(wrong_person.id, 0, wrong_person_payload)
            .await;
        assert_eq!(
            store.load(store.person_id, wrong_person.id).await,
            Err(AgentFailure::NotFound)
        );
        assert_eq!(
            store.load_transactionally(wrong_person.id).await,
            Err(AgentFailure::NotFound)
        );

        let wrong_row_id = Uuid::new_v4();
        let mut mismatched_id = wrong_person.clone();
        mismatched_id.id = Uuid::new_v4();
        mismatched_id.person_id = store.person_id;
        let mismatched_id_payload =
            serde_json::to_string(&mismatched_id).expect("serialize mismatched-ID fixture");
        store
            .insert_payload(wrong_row_id, 0, mismatched_id_payload)
            .await;
        assert_eq!(
            store.load(store.person_id, wrong_row_id).await,
            Err(AgentFailure::VaultUnavailable)
        );
        assert_eq!(
            store.load_transactionally(wrong_row_id).await,
            Err(AgentFailure::VaultUnavailable)
        );

        let mut unsupported = valid;
        unsupported.id = Uuid::new_v4();
        unsupported.revision = 0;
        unsupported.schema_version = AGENT_VERSION + 1;
        let unsupported_payload =
            serde_json::to_string(&unsupported).expect("serialize unsupported schema fixture");
        store
            .insert_payload(unsupported.id, 0, unsupported_payload)
            .await;
        assert_eq!(
            store.load(store.person_id, unsupported.id).await,
            Err(AgentFailure::UnsupportedVersion)
        );
        assert_eq!(
            store.load_transactionally(unsupported.id).await,
            Err(AgentFailure::UnsupportedVersion)
        );
    }
}
