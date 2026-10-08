//! Encrypted owner storage for lossless typed Session evidence.
//!
//! The transaction-scoped methods are the composition seam for a future
//! Conversation-plus-Core write. They do not update Session or Core themselves.

use floe_access::DependencyCoverage;
use floe_agent_contract::AgentFailure;
use floe_conversation::{
    AgentMessage, MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES, MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES,
    TYPED_AGENT_MESSAGE_OWNER_NAMESPACE, TypedAgentMessageEvidence, TypedAgentMessageProvenance,
    TypedAgentMessageReference,
};
use floe_kernel::PersonId;
#[cfg(test)]
use std::sync::atomic::Ordering;
use turso::transaction::Transaction;
use uuid::Uuid;

use super::{EncryptedAgentVault, VaultKeyProvider, context_dependencies, database_failure};

/// Exact typed payload plus coverage read from the live owner row for its turn.
/// This is evidence only; Context/Access must still reauthorize before use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedTypedAgentMessage {
    pub reference: TypedAgentMessageReference,
    pub message: AgentMessage,
    pub coverage: DependencyCoverage,
}

#[derive(Clone, Debug)]
struct StoredHeader {
    reference: TypedAgentMessageReference,
    actual_payload_bytes: usize,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Insert exact owner-encoded evidence in the caller's transaction.
    ///
    /// This is intentionally not wired into production Session writes. Later
    /// Conversation/Core composition can call it with the same transaction as
    /// owner state and the opaque Core evidence digest.
    pub(super) async fn insert_typed_agent_message_on(
        &self,
        transaction: &Transaction<'_>,
        session_id: Uuid,
        entry_id: Uuid,
        provenance: TypedAgentMessageProvenance,
        message: &AgentMessage,
    ) -> Result<TypedAgentMessageReference, AgentFailure> {
        if session_id.is_nil() || entry_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        if provenance != TypedAgentMessageProvenance::OwnerRecorded {
            // Pure snapshot preparation does not authorize a storage import.
            // This ordinary typed-history insert never persists imported rows.
            return Err(AgentFailure::InvalidInput);
        }
        self.check_access()?;
        let evidence = TypedAgentMessageEvidence::encode(
            self.person_id,
            session_id,
            entry_id,
            provenance,
            message,
        )?;
        let reference = evidence.reference().clone();
        let payload = std::str::from_utf8(evidence.payload_bytes())
            .map_err(|_| AgentFailure::InvalidInput)?
            .to_owned();
        let actual_record_bytes = evidence.encoded_byte_length()?;
        if actual_record_bytes > MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }

        let mut session_rows = transaction
            .query(
                "SELECT id FROM agent_sessions WHERE id = ? LIMIT 2",
                [session_id.to_string()],
            )
            .await
            .map_err(database_failure)?;
        if session_rows
            .next()
            .await
            .map_err(database_failure)?
            .is_none()
            || session_rows
                .next()
                .await
                .map_err(database_failure)?
                .is_some()
        {
            return Err(AgentFailure::NotFound);
        }

        crate::schema::ensure_typed_history_family(transaction)
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?;

        if let Some(header) = load_header_on(transaction, self.person_id, &reference).await? {
            header.reference.validate_for_storage()?;
            if header.reference != reference {
                return Err(AgentFailure::Conflict);
            }
            if header.actual_payload_bytes != reference.encoded_byte_length() as usize {
                return Err(AgentFailure::VaultUnavailable);
            }
            if header.actual_payload_bytes > MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            let existing_payload = load_payload_on(
                transaction,
                &reference,
                #[cfg(test)]
                &self.typed_history_payload_hydrations,
            )
            .await?;
            let existing = TypedAgentMessageEvidence::from_stored(
                reference.clone(),
                existing_payload.into_bytes(),
            )?;
            if existing.payload_bytes() != evidence.payload_bytes() {
                return Err(AgentFailure::Conflict);
            }
            return Ok(reference);
        }

        let changed = transaction
            .execute(
                "INSERT INTO agent_conversation_typed_history_entries (person_id, owner_namespace, session_id, entry_id, turn_id, provenance, schema_id, schema_version, payload_byte_length, digest, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    reference.person_id().to_string(),
                    reference.owner_namespace(),
                    reference.session_id().to_string(),
                    reference.entry_id().to_string(),
                    reference.turn_id().to_string(),
                    reference.provenance().as_storage_value(),
                    reference.schema_id(),
                    i64::from(reference.schema_version()),
                    i64::try_from(reference.encoded_byte_length())
                        .map_err(|_| AgentFailure::BudgetExceeded)?,
                    digest_to_hex(reference.digest()),
                    payload,
                ),
            )
            .await
            .map_err(database_failure)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        Ok(reference)
    }

    /// Resolve one entry without scanning or hydrating the rest of its Session.
    /// Optional-family absence and coverage absence both remain explicit.
    pub(super) async fn resolve_typed_agent_message_on(
        &self,
        transaction: &Transaction<'_>,
        reference: &TypedAgentMessageReference,
        byte_budget: usize,
    ) -> Result<ResolvedTypedAgentMessage, AgentFailure> {
        if byte_budget == 0 {
            return Err(AgentFailure::BudgetExceeded);
        }
        reference.validate_for_storage()?;
        if reference.person_id() != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        self.check_access()?;
        if !crate::schema::typed_history_family_present(transaction)
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?
        {
            return Err(AgentFailure::NotFound);
        }

        let header = load_header_on(transaction, self.person_id, reference)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        let resolved = self
            .resolve_header_payload_on(
                transaction,
                header,
                self.person_id,
                reference.session_id(),
                reference.owner_namespace(),
                reference.digest(),
                byte_budget,
            )
            .await?;
        if resolved.reference != *reference {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(resolved)
    }

    /// Resolve from the trusted owner scope and opaque Core evidence digest.
    /// The unique scoped digest index recovers the complete owner reference;
    /// the caller never needs to reconstruct or scan for its fields.
    pub(super) async fn resolve_typed_agent_message_by_digest_on(
        &self,
        transaction: &Transaction<'_>,
        person_id: PersonId,
        session_id: Uuid,
        owner_namespace: &str,
        expected_digest: [u8; 32],
        byte_budget: usize,
    ) -> Result<ResolvedTypedAgentMessage, AgentFailure> {
        if byte_budget == 0 {
            return Err(AgentFailure::BudgetExceeded);
        }
        if person_id != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        if session_id.is_nil() || expected_digest == [0; 32] {
            return Err(AgentFailure::InvalidInput);
        }
        if owner_namespace != TYPED_AGENT_MESSAGE_OWNER_NAMESPACE {
            return Err(AgentFailure::PolicyDenied);
        }
        self.check_access()?;
        if !crate::schema::typed_history_family_present(transaction)
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?
        {
            return Err(AgentFailure::NotFound);
        }

        let header = load_header_by_digest_on(
            transaction,
            person_id,
            session_id,
            owner_namespace,
            expected_digest,
        )
        .await?
        .ok_or(AgentFailure::NotFound)?;
        self.resolve_header_payload_on(
            transaction,
            header,
            person_id,
            session_id,
            owner_namespace,
            expected_digest,
            byte_budget,
        )
        .await
    }

    async fn resolve_header_payload_on(
        &self,
        transaction: &Transaction<'_>,
        header: StoredHeader,
        person_id: PersonId,
        session_id: Uuid,
        owner_namespace: &str,
        expected_digest: [u8; 32],
        byte_budget: usize,
    ) -> Result<ResolvedTypedAgentMessage, AgentFailure> {
        let reference = &header.reference;
        reference.validate_for_storage()?;
        if reference.person_id() != person_id
            || reference.session_id() != session_id
            || reference.owner_namespace() != owner_namespace
            || reference.digest() != expected_digest
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let declared_bytes = usize::try_from(reference.encoded_byte_length())
            .map_err(|_| AgentFailure::BudgetExceeded)?;
        if header.actual_payload_bytes != declared_bytes {
            return Err(AgentFailure::VaultUnavailable);
        }
        let record_bytes = declared_bytes
            .checked_add(reference.encoded_reference_bytes()?)
            .ok_or(AgentFailure::BudgetExceeded)?;
        if record_bytes > MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES || record_bytes > byte_budget {
            return Err(AgentFailure::BudgetExceeded);
        }

        // The declared and physical byte budgets are proven before this query
        // hydrates the one payload.
        let payload = load_payload_on(
            transaction,
            reference,
            #[cfg(test)]
            &self.typed_history_payload_hydrations,
        )
        .await?;
        let evidence =
            TypedAgentMessageEvidence::from_stored(reference.clone(), payload.into_bytes())?;
        let message = evidence.decode_message()?;
        let coverage = context_dependencies::read_context_dependency_coverage(
            transaction,
            person_id,
            session_id,
            reference.turn_id(),
        )
        .await?;
        Ok(ResolvedTypedAgentMessage {
            reference: reference.clone(),
            message,
            coverage,
        })
    }
}

async fn load_header_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    reference: &TypedAgentMessageReference,
) -> Result<Option<StoredHeader>, AgentFailure> {
    let mut rows = transaction
        .query(
        "SELECT person_id, owner_namespace, schema_id, schema_version, turn_id, provenance, payload_byte_length, digest, length(CAST(payload AS BLOB)) FROM agent_conversation_typed_history_entries WHERE person_id = ? AND owner_namespace = ? AND session_id = ? AND entry_id = ? LIMIT 2",
            (
                person_id.to_string(),
                TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                reference.session_id().to_string(),
                reference.entry_id().to_string(),
            ),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }

    let schema_version = u32::try_from(row.get::<i64>(3).map_err(storage)?)
        .map_err(|_| AgentFailure::UnsupportedVersion)?;
    let declared_length = u64::try_from(row.get::<i64>(6).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let actual_payload_bytes = usize::try_from(row.get::<i64>(8).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let stored_reference = TypedAgentMessageReference::from_stored_fields(
        row.get::<String>(1).map_err(storage)?,
        row.get::<String>(2).map_err(storage)?,
        schema_version,
        person_id_from_string(person_id, row.get::<String>(0).map_err(storage)?)?,
        reference.session_id(),
        reference.entry_id(),
        parse_uuid(row.get::<String>(4).map_err(storage)?)?,
        TypedAgentMessageProvenance::from_storage_value(&row.get::<String>(5).map_err(storage)?)?,
        declared_length,
        digest_from_hex(row.get::<String>(7).map_err(storage)?)?,
    );
    Ok(Some(StoredHeader {
        reference: stored_reference,
        actual_payload_bytes,
    }))
}

async fn load_header_by_digest_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    session_id: Uuid,
    owner_namespace: &str,
    expected_digest: [u8; 32],
) -> Result<Option<StoredHeader>, AgentFailure> {
    let mut rows = transaction
        .query(
            "SELECT person_id, owner_namespace, session_id, entry_id, schema_id, schema_version, turn_id, provenance, payload_byte_length, digest, length(CAST(payload AS BLOB)) FROM agent_conversation_typed_history_entries WHERE person_id = ? AND owner_namespace = ? AND session_id = ? AND digest = ? LIMIT 2",
            (
                person_id.to_string(),
                owner_namespace,
                session_id.to_string(),
                digest_to_hex(expected_digest),
            ),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }

    let schema_version = u32::try_from(row.get::<i64>(5).map_err(storage)?)
        .map_err(|_| AgentFailure::UnsupportedVersion)?;
    let declared_length = u64::try_from(row.get::<i64>(8).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let actual_payload_bytes = usize::try_from(row.get::<i64>(10).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let stored_reference = TypedAgentMessageReference::from_stored_fields(
        row.get::<String>(1).map_err(storage)?,
        row.get::<String>(4).map_err(storage)?,
        schema_version,
        person_id_from_string(person_id, row.get::<String>(0).map_err(storage)?)?,
        parse_uuid(row.get::<String>(2).map_err(storage)?)?,
        parse_uuid(row.get::<String>(3).map_err(storage)?)?,
        parse_uuid(row.get::<String>(6).map_err(storage)?)?,
        TypedAgentMessageProvenance::from_storage_value(&row.get::<String>(7).map_err(storage)?)?,
        declared_length,
        digest_from_hex(row.get::<String>(9).map_err(storage)?)?,
    );
    Ok(Some(StoredHeader {
        reference: stored_reference,
        actual_payload_bytes,
    }))
}

async fn load_payload_on(
    transaction: &Transaction<'_>,
    reference: &TypedAgentMessageReference,
    #[cfg(test)] hydration_count: &std::sync::atomic::AtomicU64,
) -> Result<String, AgentFailure> {
    #[cfg(test)]
    hydration_count.fetch_add(1, Ordering::Relaxed);
    let mut rows = transaction
        .query(
            "SELECT payload FROM agent_conversation_typed_history_entries WHERE person_id = ? AND owner_namespace = ? AND session_id = ? AND entry_id = ? LIMIT 2",
            (
                reference.person_id().to_string(),
                TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                reference.session_id().to_string(),
                reference.entry_id().to_string(),
            ),
        )
        .await
        .map_err(database_failure)?;
    let row = rows
        .next()
        .await
        .map_err(database_failure)?
        .ok_or(AgentFailure::NotFound)?;
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    let payload = row.get::<String>(0).map_err(storage)?;
    if payload.as_bytes().is_empty()
        || payload.as_bytes().len() > MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(payload)
}

fn parse_uuid(value: String) -> Result<Uuid, AgentFailure> {
    let parsed = Uuid::parse_str(&value).map_err(|_| AgentFailure::VaultUnavailable)?;
    if parsed.to_string() != value || parsed.is_nil() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(parsed)
}

fn person_id_from_string(expected: PersonId, value: String) -> Result<PersonId, AgentFailure> {
    if value != expected.to_string() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(expected)
}

fn digest_to_hex(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn digest_from_hex(value: String) -> Result<[u8; 32], AgentFailure> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut digest = [0; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
    }
    Ok(digest)
}

fn storage(_: impl std::fmt::Debug) -> AgentFailure {
    AgentFailure::StorageUnavailable
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use floe_agent_contract::{
        AgentFailure, DependencyCoverage, MAX_OUTPUT_BYTES, TaskBlockage, TaskExecutionKey,
        TaskExecutionReceiptRef, TaskSnapshot, TaskState, UserInteractionKind,
    };
    use floe_conversation::{
        AgentMessage, AgentSession, MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES,
        MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES, SessionRecoveryPointer, TypedAgentMessageEvidence,
        TypedAgentMessageProvenance, TypedAgentMessageReference,
    };
    use floe_kernel::{PersonId, TaskId};

    use super::*;
    use crate::{RootKey, VaultKeyProvider};

    #[derive(Clone, Default)]
    struct TestKeys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

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
            let path = std::env::temp_dir().join(format!("floe-typed-history-{}", Uuid::new_v4()));
            std::fs::create_dir(&path).expect("create isolated encrypted Vault root");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("restrict isolated encrypted Vault root");
            Self(path)
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct TestVault {
        vault: EncryptedAgentVault<TestKeys>,
        keys: TestKeys,
        person_id: PersonId,
        root: TestRoot,
    }

    impl TestVault {
        async fn new() -> Self {
            Self::for_person(PersonId::new()).await
        }

        async fn for_person(person_id: PersonId) -> Self {
            let root = TestRoot::new();
            let keys = TestKeys::default();
            let vault = EncryptedAgentVault::create(&root.0, person_id, keys.clone())
                .await
                .expect("create encrypted test Vault");
            Self {
                vault,
                keys,
                person_id,
                root,
            }
        }

        async fn insert_session(&self, session_id: Uuid, messages: &[AgentMessage]) {
            let mut session = AgentSession::new(self.person_id);
            session.id = session_id;
            session.messages = messages.to_vec();
            let payload = serde_json::to_string(&session).expect("serialize Session fixture");
            self.vault
                .connection()
                .expect("connect to Vault")
                .execute(
                    "INSERT INTO agent_sessions (id, revision, payload) VALUES (?, ?, ?)",
                    (session_id.to_string(), 0, payload),
                )
                .await
                .expect("insert Session owner fixture");
        }

        async fn insert_message(
            &self,
            session_id: Uuid,
            entry_id: Uuid,
            message: &AgentMessage,
        ) -> Result<TypedAgentMessageReference, AgentFailure> {
            let mut connection = self.vault.connection()?;
            let transaction = connection
                .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
                .await
                .map_err(database_failure)?;
            let result = self
                .vault
                .insert_typed_agent_message_on(
                    &transaction,
                    session_id,
                    entry_id,
                    TypedAgentMessageProvenance::OwnerRecorded,
                    message,
                )
                .await;
            match result {
                Ok(reference) => {
                    transaction.commit().await.map_err(database_failure)?;
                    Ok(reference)
                }
                Err(failure) => {
                    transaction.rollback().await.map_err(database_failure)?;
                    Err(failure)
                }
            }
        }

        async fn resolve(
            &self,
            reference: &TypedAgentMessageReference,
        ) -> Result<ResolvedTypedAgentMessage, AgentFailure> {
            self.resolve_with_budget(reference, MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES)
                .await
        }

        async fn resolve_with_budget(
            &self,
            reference: &TypedAgentMessageReference,
            byte_budget: usize,
        ) -> Result<ResolvedTypedAgentMessage, AgentFailure> {
            let mut connection = self.vault.connection()?;
            let transaction = connection
                .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
                .await
                .map_err(database_failure)?;
            let result = self
                .vault
                .resolve_typed_agent_message_on(&transaction, reference, byte_budget)
                .await;
            match result {
                Ok(value) => {
                    transaction.commit().await.map_err(database_failure)?;
                    Ok(value)
                }
                Err(failure) => {
                    transaction.rollback().await.map_err(database_failure)?;
                    Err(failure)
                }
            }
        }

        async fn resolve_digest_with_scope(
            &self,
            person_id: PersonId,
            session_id: Uuid,
            owner_namespace: &str,
            digest: [u8; 32],
            byte_budget: usize,
        ) -> Result<ResolvedTypedAgentMessage, AgentFailure> {
            let mut connection = self.vault.connection()?;
            let transaction = connection
                .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
                .await
                .map_err(database_failure)?;
            let result = self
                .vault
                .resolve_typed_agent_message_by_digest_on(
                    &transaction,
                    person_id,
                    session_id,
                    owner_namespace,
                    digest,
                    byte_budget,
                )
                .await;
            match result {
                Ok(value) => {
                    transaction.commit().await.map_err(database_failure)?;
                    Ok(value)
                }
                Err(failure) => {
                    transaction.rollback().await.map_err(database_failure)?;
                    Err(failure)
                }
            }
        }

        async fn update_text_entry(
            &self,
            session_id: Uuid,
            entry_id: Uuid,
            column: &str,
            value: &str,
        ) {
            let statement = format!(
                "UPDATE agent_conversation_typed_history_entries SET {column} = ? WHERE person_id = ? AND owner_namespace = ? AND session_id = ? AND entry_id = ?"
            );
            self.vault
                .connection()
                .expect("connect to Vault")
                .execute(
                    &statement,
                    (
                        value,
                        self.person_id.to_string(),
                        TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                        session_id.to_string(),
                        entry_id.to_string(),
                    ),
                )
                .await
                .expect("corrupt selected typed-history row for test");
        }

        async fn update_integer_entry(
            &self,
            session_id: Uuid,
            entry_id: Uuid,
            column: &str,
            value: i64,
        ) {
            let statement = format!(
                "UPDATE agent_conversation_typed_history_entries SET {column} = ? WHERE person_id = ? AND owner_namespace = ? AND session_id = ? AND entry_id = ?"
            );
            self.vault
                .connection()
                .expect("connect to Vault")
                .execute(
                    &statement,
                    (
                        value,
                        self.person_id.to_string(),
                        TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                        session_id.to_string(),
                        entry_id.to_string(),
                    ),
                )
                .await
                .expect("corrupt selected typed-history row for test");
        }
    }

    fn task_snapshot(task_id: TaskId, state: TaskState) -> TaskSnapshot {
        let (result, issue) = match state {
            TaskState::Completed => (Some("completed result".to_owned()), None),
            TaskState::Failed => (None, Some(AgentFailure::ModelUnavailable)),
            _ => (None, None),
        };
        let blockage = (state == TaskState::Blocked).then_some(TaskBlockage::Binding {
            requirement_keys: vec!["required.source".to_owned()],
        });
        TaskSnapshot {
            task_id,
            parent_run_id: Some(Uuid::new_v4()),
            principal: "person:typed-history-test".to_owned(),
            agent_id: "expert:test".to_owned(),
            definition_revision: 1,
            state,
            result,
            artifacts: Vec::new(),
            coverage: DependencyCoverage::Independent,
            issue,
            blockage,
        }
    }

    fn task_execution_reference(task_id: TaskId) -> TaskExecutionReceiptRef {
        TaskExecutionReceiptRef {
            execution: TaskExecutionKey {
                task_id,
                execution_id: Uuid::new_v4(),
                executor_generation: 1,
            },
            task_revision: 2,
            journal_revision: 1,
            digest: [0x6d; 32],
        }
    }

    fn messages(turn_id: Uuid) -> Vec<AgentMessage> {
        let completed_task_id = TaskId::new();
        let mut messages = vec![
            AgentMessage::User {
                turn_id,
                message_id: Uuid::new_v4(),
                text: "a question".to_owned(),
            },
            AgentMessage::Preamble {
                turn_id,
                text: "conversation preamble".to_owned(),
            },
            AgentMessage::Assistant {
                turn_id,
                text: "an answer".to_owned(),
            },
            AgentMessage::Compaction {
                turn_id,
                summary: "a compact summary".to_owned(),
                recovery: SessionRecoveryPointer {
                    archive_id: Uuid::new_v4(),
                    source_revision: 3,
                    through_turn_id: Uuid::new_v4(),
                    archived_message_count: 4,
                },
            },
            AgentMessage::Capability {
                turn_id,
                call_id: Uuid::new_v4(),
                capability_id: "notes.search".to_owned(),
                input: "query".to_owned(),
                result: Ok("found notes".to_owned()),
            },
            AgentMessage::Capability {
                turn_id,
                call_id: Uuid::new_v4(),
                capability_id: "calendar.read".to_owned(),
                input: "today".to_owned(),
                result: Err(AgentFailure::CapabilityDenied),
            },
            AgentMessage::Delegation {
                turn_id,
                task: task_snapshot(completed_task_id, TaskState::Completed),
                execution_receipt: Some(task_execution_reference(completed_task_id)),
            },
        ];
        for state in [
            TaskState::Submitted,
            TaskState::Working,
            TaskState::Blocked,
            TaskState::Failed,
            TaskState::Rejected,
            TaskState::Cancelled,
            TaskState::TimedOut,
            TaskState::Interrupted,
        ] {
            let task_id = TaskId::new();
            messages.push(AgentMessage::Delegation {
                turn_id,
                task: task_snapshot(task_id, state),
                execution_receipt: Some(task_execution_reference(task_id)),
            });
        }
        messages.push(AgentMessage::Interaction {
            turn_id,
            interaction_id: Uuid::new_v4(),
            interaction_kind: UserInteractionKind::SourceAccess,
        });
        messages
    }

    fn changed_reference_field(
        reference: &TypedAgentMessageReference,
        field: &str,
        value: serde_json::Value,
    ) -> TypedAgentMessageReference {
        let mut object = serde_json::to_value(reference).expect("serialize evidence reference");
        object[field] = value;
        serde_json::from_value(object).expect("deserialize changed evidence reference")
    }

    async fn read_row_bytes(store: &TestVault, session_id: Uuid, entry_id: Uuid) -> Vec<u8> {
        let connection = store.vault.connection().expect("connect to Vault");
        let mut rows = connection
            .query(
                "SELECT person_id, owner_namespace, session_id, entry_id, turn_id, provenance, schema_id, schema_version, payload_byte_length, digest, payload FROM agent_conversation_typed_history_entries WHERE person_id = ? AND owner_namespace = ? AND session_id = ? AND entry_id = ?",
                (
                    store.person_id.to_string(),
                    TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                    session_id.to_string(),
                    entry_id.to_string(),
                ),
            )
            .await
            .expect("read selected typed-history row");
        let row = rows
            .next()
            .await
            .expect("query typed-history row")
            .expect("row exists");
        let values = (
            row.get::<String>(0).expect("person"),
            row.get::<String>(1).expect("owner namespace"),
            row.get::<String>(2).expect("session"),
            row.get::<String>(3).expect("entry"),
            row.get::<String>(4).expect("turn"),
            row.get::<String>(5).expect("provenance"),
            row.get::<String>(6).expect("schema identifier"),
            row.get::<i64>(7).expect("schema version"),
            row.get::<i64>(8).expect("payload length"),
            row.get::<String>(9).expect("digest"),
            row.get::<String>(10).expect("payload"),
        );
        serde_json::to_vec(&values).expect("encode selected database row")
    }

    #[tokio::test]
    async fn encrypted_roundtrip_preserves_every_agent_message_variant_and_classification() {
        let store = TestVault::new().await;
        let turn_id = Uuid::new_v4();
        let messages = messages(turn_id);
        let session_id = Uuid::new_v4();
        store.insert_session(session_id, &messages).await;

        let connection = store.vault.connection().expect("connect to Vault");
        assert!(
            !crate::schema::typed_history_family_present(&connection)
                .await
                .expect("inspect optional evidence family")
        );
        let missing_reference = TypedAgentMessageEvidence::encode(
            store.person_id,
            session_id,
            Uuid::new_v4(),
            TypedAgentMessageProvenance::OwnerRecorded,
            &messages[0],
        )
        .expect("encode User message")
        .reference()
        .clone();
        assert_eq!(
            store.resolve(&missing_reference).await.unwrap_err(),
            AgentFailure::NotFound
        );
        assert!(
            !crate::schema::typed_history_family_present(&connection)
                .await
                .expect("read did not initialize the optional family")
        );

        // Exercise the committed path and exact byte-preserving resolution.
        let mut references = Vec::new();
        for (index, message) in messages.iter().enumerate() {
            references.push(
                store
                    .insert_message(session_id, Uuid::from_u128(index as u128 + 1), message)
                    .await
                    .expect("commit typed evidence"),
            );
        }
        assert!(
            crate::schema::typed_history_family_present(&connection)
                .await
                .expect("explicit writes initialize optional family")
        );

        for (entry_index, (reference, expected)) in references.iter().zip(&messages).enumerate() {
            let resolved = store
                .resolve(reference)
                .await
                .expect("resolve one typed entry");
            assert_eq!(&resolved.message, expected, "variant index {entry_index}");
            assert_eq!(resolved.coverage, DependencyCoverage::Unknown);
            assert_eq!(
                floe_conversation_contract::MessageEvidenceReference::from_digest(
                    reference.digest()
                )
                .digest(),
                reference.digest()
            );
            assert_eq!(
                expected.may_derive_from_source(),
                resolved.message.may_derive_from_source(),
                "source-derived classification for variant index {entry_index}"
            );
        }
        assert!(messages[2].may_derive_from_source());
        assert!(matches!(
            &messages[4],
            AgentMessage::Capability { result: Ok(_), .. }
        ));
        assert!(!messages[5].may_derive_from_source());
        let delegated: Vec<_> = messages
            .iter()
            .filter_map(|message| match message {
                AgentMessage::Delegation { task, .. } => Some((message, task.state)),
                _ => None,
            })
            .collect();
        assert_eq!(delegated.len(), 9);
        assert!(delegated[0].0.may_derive_from_source());
        assert_eq!(delegated[0].1, TaskState::Completed);
        assert!(
            delegated[1..]
                .iter()
                .all(|(message, _)| !message.may_derive_from_source())
        );
        assert!(matches!(
            messages.last(),
            Some(AgentMessage::Interaction { .. })
        ));
        store
            .vault
            .typed_history_payload_hydrations
            .store(0, Ordering::Relaxed);
        let _ = store
            .resolve(references.last().expect("Interaction reference"))
            .await
            .expect("resolve interaction");
        assert_eq!(
            store
                .vault
                .typed_history_payload_hydrations
                .load(Ordering::Relaxed),
            1,
            "one lookup hydrates only the requested payload"
        );

        let TestVault {
            vault,
            keys,
            person_id,
            root,
        } = store;
        drop(vault);
        let reopened = EncryptedAgentVault::open(&root.0, person_id, keys.clone())
            .await
            .expect("reopen encrypted test Vault");
        let mut connection = reopened.connection().expect("connect to reopened Vault");
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
            .await
            .expect("begin reopened read transaction");
        let resolved = reopened
            .resolve_typed_agent_message_on(
                &transaction,
                &references[3],
                MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES,
            )
            .await
            .expect("resolve retained archive pointer after reopen");
        transaction.commit().await.expect("finish reopened read");
        assert_eq!(resolved.message, messages[3]);
        drop(reopened);
        drop(root);
    }

    #[tokio::test]
    async fn core_digest_resolves_one_scoped_message_without_the_owner_reference() {
        let store = TestVault::new().await;
        let session_id = Uuid::new_v4();
        let message = AgentMessage::Assistant {
            turn_id: Uuid::new_v4(),
            text: "resolved from opaque Core evidence".to_owned(),
        };
        store
            .insert_session(session_id, std::slice::from_ref(&message))
            .await;
        let typed_reference = store
            .insert_message(session_id, Uuid::new_v4(), &message)
            .await
            .expect("insert owner-typed evidence");
        let envelope_bytes = typed_reference.encoded_byte_length() as usize
            + typed_reference
                .encoded_reference_bytes()
                .expect("measure reference bytes");
        let core_reference = floe_conversation_contract::MessageEvidenceReference::from_digest(
            typed_reference.digest(),
        );
        let digest = core_reference.digest();
        drop(typed_reference);

        store
            .vault
            .typed_history_payload_hydrations
            .store(0, Ordering::Relaxed);
        assert_eq!(
            store
                .resolve_digest_with_scope(
                    store.person_id,
                    session_id,
                    TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                    digest,
                    0,
                )
                .await
                .unwrap_err(),
            AgentFailure::BudgetExceeded
        );
        assert_eq!(
            store
                .resolve_digest_with_scope(
                    store.person_id,
                    session_id,
                    TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                    digest,
                    envelope_bytes - 1,
                )
                .await
                .unwrap_err(),
            AgentFailure::BudgetExceeded
        );
        assert_eq!(
            store
                .resolve_digest_with_scope(
                    store.person_id,
                    Uuid::new_v4(),
                    TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                    digest,
                    envelope_bytes,
                )
                .await
                .unwrap_err(),
            AgentFailure::NotFound
        );
        assert_eq!(
            store
                .resolve_digest_with_scope(
                    PersonId::new(),
                    session_id,
                    TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                    digest,
                    envelope_bytes,
                )
                .await
                .unwrap_err(),
            AgentFailure::PolicyDenied
        );
        assert_eq!(
            store
                .resolve_digest_with_scope(
                    store.person_id,
                    session_id,
                    "floe.conversation.other",
                    digest,
                    envelope_bytes,
                )
                .await
                .unwrap_err(),
            AgentFailure::PolicyDenied
        );
        assert_eq!(
            store
                .resolve_digest_with_scope(
                    store.person_id,
                    session_id,
                    TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                    [0x44; 32],
                    envelope_bytes,
                )
                .await
                .unwrap_err(),
            AgentFailure::NotFound
        );
        assert_eq!(
            store
                .vault
                .typed_history_payload_hydrations
                .load(Ordering::Relaxed),
            0,
            "zero/undersized budgets and wrong scopes reject before hydration"
        );

        let resolved = store
            .resolve_digest_with_scope(
                store.person_id,
                session_id,
                TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                core_reference.digest(),
                envelope_bytes,
            )
            .await
            .expect("resolve from Core's opaque digest alone");
        assert_eq!(resolved.message, message);
        assert_eq!(resolved.reference.digest(), digest);
        assert_eq!(resolved.coverage, DependencyCoverage::Unknown);
        assert_eq!(
            store
                .vault
                .typed_history_payload_hydrations
                .load(Ordering::Relaxed),
            1,
            "digest lookup hydrates only its one matching payload"
        );
    }

    #[tokio::test]
    async fn scoped_digest_index_rejects_duplicate_and_conflicting_rows() {
        let store = TestVault::new().await;
        let session_id = Uuid::new_v4();
        let entry_id = Uuid::new_v4();
        let message = AgentMessage::Assistant {
            turn_id: Uuid::new_v4(),
            text: "one content-addressed entry".to_owned(),
        };
        store
            .insert_session(session_id, std::slice::from_ref(&message))
            .await;
        let reference = store
            .insert_message(session_id, entry_id, &message)
            .await
            .expect("insert source entry");
        let evidence = TypedAgentMessageEvidence::encode(
            store.person_id,
            session_id,
            entry_id,
            TypedAgentMessageProvenance::OwnerRecorded,
            &message,
        )
        .expect("encode source entry");
        let payload = std::str::from_utf8(evidence.payload_bytes())
            .expect("typed payload is UTF-8")
            .to_owned();
        let connection = store.vault.connection().expect("connect to Vault");
        for (duplicate_entry_id, conflicting_payload) in [
            (Uuid::new_v4(), payload.clone()),
            (Uuid::new_v4(), "conflicting payload".to_owned()),
        ] {
            let insertion = connection
                .execute(
                    "INSERT INTO agent_conversation_typed_history_entries (person_id, owner_namespace, session_id, entry_id, turn_id, provenance, schema_id, schema_version, payload_byte_length, digest, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        store.person_id.to_string(),
                        TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                        session_id.to_string(),
                        duplicate_entry_id.to_string(),
                        reference.turn_id().to_string(),
                        reference.provenance().as_storage_value(),
                        reference.schema_id(),
                        i64::from(reference.schema_version()),
                        i64::try_from(reference.encoded_byte_length()).unwrap(),
                        digest_to_hex(reference.digest()),
                        conflicting_payload,
                    ),
                )
                .await;
            assert!(
                insertion.is_err(),
                "the unique scoped digest index rejects duplicate or conflicting rows"
            );
        }

        let resolved = store
            .resolve_digest_with_scope(
                store.person_id,
                session_id,
                TYPED_AGENT_MESSAGE_OWNER_NAMESPACE,
                reference.digest(),
                MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES,
            )
            .await
            .expect("the sole original row remains resolvable");
        assert_eq!(resolved.message, message);
        assert_eq!(resolved.reference.entry_id(), entry_id);
    }

    #[tokio::test]
    async fn missing_and_unknown_coverage_stay_unknown_while_resolver_reads_live_owner_rows() {
        let store = TestVault::new().await;
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let message = AgentMessage::Assistant {
            turn_id,
            text: "answer".to_owned(),
        };
        store
            .insert_session(session_id, std::slice::from_ref(&message))
            .await;
        let reference = store
            .insert_message(session_id, Uuid::new_v4(), &message)
            .await
            .expect("insert assistant evidence");
        assert_eq!(
            store.resolve(&reference).await.unwrap().coverage,
            DependencyCoverage::Unknown
        );

        let mut connection = store.vault.connection().expect("connect to Vault");
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .expect("begin coverage write");
        context_dependencies::merge_context_dependency_coverage(
            &transaction,
            store.person_id,
            session_id,
            turn_id,
            DependencyCoverage::Unknown,
        )
        .await
        .expect("persist explicit Unknown coverage");
        let merged = context_dependencies::merge_context_dependency_coverage(
            &transaction,
            store.person_id,
            session_id,
            turn_id,
            DependencyCoverage::Independent,
        )
        .await
        .expect("owner merge remains conservative");
        assert_eq!(merged, DependencyCoverage::Unknown);
        transaction.commit().await.expect("commit coverage write");
        assert_eq!(
            store.resolve(&reference).await.unwrap().coverage,
            DependencyCoverage::Unknown
        );
    }

    #[tokio::test]
    async fn references_reject_cross_person_session_and_owner_namespace_scopes() {
        let store = TestVault::new().await;
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let entry_id = Uuid::new_v4();
        let message = AgentMessage::Assistant {
            turn_id,
            text: "scoped answer".to_owned(),
        };
        store
            .insert_session(session_a, std::slice::from_ref(&message))
            .await;
        store
            .insert_session(session_b, std::slice::from_ref(&message))
            .await;
        let reference_a = store
            .insert_message(session_a, entry_id, &message)
            .await
            .expect("insert Session A evidence");
        let _reference_b = store
            .insert_message(session_b, entry_id, &message)
            .await
            .expect("insert same entry identity in Session B");

        let other_person = TestVault::new().await;
        assert_eq!(
            other_person.resolve(&reference_a).await.unwrap_err(),
            AgentFailure::PolicyDenied
        );

        let wrong_session = changed_reference_field(
            &reference_a,
            "session_id",
            serde_json::Value::String(session_b.to_string()),
        );
        assert!(matches!(
            store.resolve(&wrong_session).await,
            Err(AgentFailure::VaultUnavailable | AgentFailure::NotFound)
        ));

        let wrong_namespace = changed_reference_field(
            &reference_a,
            "owner_namespace",
            serde_json::Value::String("floe.other-owner".to_owned()),
        );
        assert_eq!(
            store.resolve(&wrong_namespace).await.unwrap_err(),
            AgentFailure::PolicyDenied
        );

        let wrong_person = changed_reference_field(
            &reference_a,
            "person_id",
            serde_json::to_value(PersonId::new()).expect("encode PersonId"),
        );
        assert_eq!(
            store.resolve(&wrong_person).await.unwrap_err(),
            AgentFailure::PolicyDenied
        );
    }

    #[tokio::test]
    async fn exact_replay_preserves_row_bytes_and_conflicting_key_fails_closed() {
        let store = TestVault::new().await;
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let message = AgentMessage::Assistant {
            turn_id,
            text: "stable content".to_owned(),
        };
        store
            .insert_session(session_id, std::slice::from_ref(&message))
            .await;
        let entry_id = Uuid::new_v4();
        let reference = store
            .insert_message(session_id, entry_id, &message)
            .await
            .expect("insert evidence");
        let before = read_row_bytes(&store, session_id, entry_id).await;
        assert_eq!(
            store
                .insert_message(session_id, entry_id, &message)
                .await
                .expect("exact replay"),
            reference
        );
        assert_eq!(read_row_bytes(&store, session_id, entry_id).await, before);

        let conflicting = AgentMessage::Assistant {
            turn_id,
            text: "conflicting content".to_owned(),
        };
        assert_eq!(
            store
                .insert_message(session_id, entry_id, &conflicting)
                .await
                .unwrap_err(),
            AgentFailure::Conflict
        );
        assert_eq!(read_row_bytes(&store, session_id, entry_id).await, before);
    }

    #[tokio::test]
    async fn escaped_multi_field_agent_message_over_sixty_four_kib_roundtrips_within_record_bound()
    {
        let store = TestVault::new().await;
        let session_id = Uuid::new_v4();
        let message = AgentMessage::Capability {
            turn_id: Uuid::new_v4(),
            call_id: Uuid::new_v4(),
            capability_id: "capability.large".to_owned(),
            input: "\n".repeat(MAX_OUTPUT_BYTES),
            result: Ok("\t".repeat(MAX_OUTPUT_BYTES)),
        };
        let serialized = serde_json::to_vec(&message).expect("serialize valid capability message");
        assert!(serialized.len() > MAX_OUTPUT_BYTES);
        store
            .insert_session(session_id, std::slice::from_ref(&message))
            .await;
        let reference = store
            .insert_message(session_id, Uuid::new_v4(), &message)
            .await
            .expect("store the complete typed capability message");
        assert_eq!(reference.encoded_byte_length() as usize, serialized.len());
        let evidence = TypedAgentMessageEvidence::encode(
            store.person_id,
            session_id,
            reference.entry_id(),
            TypedAgentMessageProvenance::OwnerRecorded,
            &message,
        )
        .expect("encode complete typed capability message");
        assert!(evidence.encoded_byte_length().unwrap() <= MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES);

        store
            .vault
            .typed_history_payload_hydrations
            .store(0, Ordering::Relaxed);
        assert_eq!(
            store
                .resolve_with_budget(&reference, MAX_OUTPUT_BYTES)
                .await
                .unwrap_err(),
            AgentFailure::BudgetExceeded
        );
        assert_eq!(
            store
                .vault
                .typed_history_payload_hydrations
                .load(Ordering::Relaxed),
            0,
            "a smaller caller budget rejects before loading the payload"
        );
        assert_eq!(store.resolve(&reference).await.unwrap().message, message);
    }

    #[tokio::test]
    async fn more_than_two_hundred_fifty_six_entries_remain_addressable() {
        const STORED_ENTRIES: usize = 300;
        let store = TestVault::new().await;
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let message = AgentMessage::Assistant {
            turn_id,
            text: "retained message".to_owned(),
        };
        let session_messages = vec![message.clone(); STORED_ENTRIES];
        store.insert_session(session_id, &session_messages).await;

        let mut connection = store.vault.connection().expect("connect to Vault");
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .expect("begin bounded writes");
        let mut references = Vec::with_capacity(STORED_ENTRIES);
        for index in 0..STORED_ENTRIES {
            references.push(
                store
                    .vault
                    .insert_typed_agent_message_on(
                        &transaction,
                        session_id,
                        Uuid::from_u128(index as u128 + 1),
                        TypedAgentMessageProvenance::OwnerRecorded,
                        &message,
                    )
                    .await
                    .expect("one bounded entry write does not count historical rows"),
            );
        }
        transaction.commit().await.expect("commit bounded writes");
        assert_eq!(references.len(), STORED_ENTRIES);
        assert_eq!(
            store.resolve(&references[0]).await.unwrap().message,
            message
        );
        assert_eq!(
            store
                .resolve(references.last().expect("last entry"))
                .await
                .unwrap()
                .message,
            message
        );
    }

    #[tokio::test]
    async fn corrupted_metadata_digest_and_payload_fail_closed() {
        let store = TestVault::new().await;
        let session_id = Uuid::new_v4();
        let turn_id = Uuid::new_v4();
        let message = AgentMessage::Assistant {
            turn_id,
            text: "immutable payload".to_owned(),
        };
        store
            .insert_session(session_id, std::slice::from_ref(&message))
            .await;
        let entry_id = Uuid::new_v4();
        let reference = store
            .insert_message(session_id, entry_id, &message)
            .await
            .expect("insert evidence");

        store
            .update_text_entry(
                session_id,
                entry_id,
                "schema_id",
                "floe.conversation.agent-message.v2",
            )
            .await;
        assert_eq!(
            store.resolve(&reference).await.unwrap_err(),
            AgentFailure::UnsupportedVersion
        );
        store
            .update_text_entry(
                session_id,
                entry_id,
                "schema_id",
                "floe.conversation.agent-message",
            )
            .await;

        store
            .update_integer_entry(session_id, entry_id, "schema_version", 2)
            .await;
        assert_eq!(
            store.resolve(&reference).await.unwrap_err(),
            AgentFailure::UnsupportedVersion
        );
        store
            .update_integer_entry(session_id, entry_id, "schema_version", 1)
            .await;

        store
            .update_integer_entry(session_id, entry_id, "payload_byte_length", 0)
            .await;
        assert!(store.resolve(&reference).await.is_err());
        store
            .update_integer_entry(
                session_id,
                entry_id,
                "payload_byte_length",
                i64::try_from(reference.encoded_byte_length()).unwrap(),
            )
            .await;

        store
            .update_text_entry(
                session_id,
                entry_id,
                "digest",
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .await;
        assert!(store.resolve(&reference).await.is_err());
        store
            .update_text_entry(
                session_id,
                entry_id,
                "digest",
                &digest_to_hex(reference.digest()),
            )
            .await;

        let original = std::str::from_utf8(
            TypedAgentMessageEvidence::encode(
                store.person_id,
                session_id,
                entry_id,
                TypedAgentMessageProvenance::OwnerRecorded,
                &message,
            )
            .expect("encode original payload")
            .payload_bytes(),
        )
        .expect("UTF-8 payload")
        .to_owned();
        let mut damaged = original.clone();
        damaged.replace_range(0..1, "[");
        store
            .update_text_entry(session_id, entry_id, "payload", &damaged)
            .await;
        assert_eq!(
            store.resolve(&reference).await.unwrap_err(),
            AgentFailure::VaultUnavailable
        );
        store
            .update_text_entry(session_id, entry_id, "payload", &original)
            .await;
        assert_eq!(store.resolve(&reference).await.unwrap().message, message);
    }

    #[tokio::test]
    async fn declared_size_rejection_precedes_payload_hydration() {
        let store = TestVault::new().await;
        let session_id = Uuid::new_v4();
        let message = AgentMessage::Assistant {
            turn_id: Uuid::new_v4(),
            text: "bounded".to_owned(),
        };
        store
            .insert_session(session_id, std::slice::from_ref(&message))
            .await;
        let entry_id = Uuid::new_v4();
        let reference = store
            .insert_message(session_id, entry_id, &message)
            .await
            .expect("insert evidence");

        store
            .update_integer_entry(
                session_id,
                entry_id,
                "payload_byte_length",
                i64::try_from(MAX_TYPED_AGENT_MESSAGE_PAYLOAD_BYTES + 1).unwrap(),
            )
            .await;
        store
            .vault
            .typed_history_payload_hydrations
            .store(0, Ordering::Relaxed);
        assert_eq!(
            store.resolve(&reference).await.unwrap_err(),
            AgentFailure::BudgetExceeded
        );
        assert_eq!(
            store
                .vault
                .typed_history_payload_hydrations
                .load(Ordering::Relaxed),
            0,
            "the metadata check returned before selecting payload bytes"
        );
    }

    #[tokio::test]
    async fn owner_session_and_typed_evidence_roll_back_together() {
        let store = TestVault::new().await;
        let session_id = Uuid::new_v4();
        let message = AgentMessage::User {
            turn_id: Uuid::new_v4(),
            message_id: Uuid::new_v4(),
            text: "transactional input".to_owned(),
        };
        let mut session = AgentSession::new(store.person_id);
        session.id = session_id;
        session.messages.push(message.clone());
        let session_payload = serde_json::to_string(&session).expect("serialize Session");
        let mut connection = store.vault.connection().expect("connect to Vault");
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .expect("begin owner transaction");
        transaction
            .execute(
                "INSERT INTO agent_sessions (id, revision, payload) VALUES (?, ?, ?)",
                (session_id.to_string(), 0, session_payload),
            )
            .await
            .expect("write owner Session row");
        store
            .vault
            .insert_typed_agent_message_on(
                &transaction,
                session_id,
                Uuid::new_v4(),
                TypedAgentMessageProvenance::OwnerRecorded,
                &message,
            )
            .await
            .expect("write typed evidence in the same transaction");
        transaction
            .rollback()
            .await
            .expect("rollback owner transaction");

        let connection = store.vault.connection().expect("connect to Vault");
        let mut sessions = connection
            .query(
                "SELECT COUNT(*) FROM agent_sessions WHERE id = ?",
                [session_id.to_string()],
            )
            .await
            .expect("count owner Session rows");
        assert_eq!(
            sessions
                .next()
                .await
                .unwrap()
                .unwrap()
                .get::<i64>(0)
                .unwrap(),
            0
        );
        assert!(
            !crate::schema::typed_history_family_present(&connection)
                .await
                .expect("rolled-back family remains absent")
        );
    }
}
