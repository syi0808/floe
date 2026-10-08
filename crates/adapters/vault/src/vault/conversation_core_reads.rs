//! Bounded, read-only queries over neutral Conversation recorder custody.
//!
//! Page byte budgets count the serialized `TranscriptEntry` returned to the
//! caller: transcript reference, message, recording metadata, Task evidence
//! reference, and prefix digest. They do not count page/cursor metadata or
//! hydrate owner evidence payloads. Generated Task entries still pass the
//! existing exact producing Run, Task, and execution-receipt validation,
//! including the producing Run's Delegation journal, inside this read
//! transaction. Those owner reads have their own 512-entry journal bounds and
//! per-record limits (including `MAX_TASK_RECORD_BYTES` and the Task output
//! cap); one point read validates at most one entry and a reverse page at most
//! 128. This separate owner-validation I/O is not included in `encoded_bytes`
//! and is not a claim that historical Task evidence payloads are fully
//! hydrated under the transcript byte budget.

use super::conversation_core::{
    Scope, database_error, entry_on, integer, load_head_on, positive_integer, require_core_v3_on,
    start_error,
};
use super::{EncryptedAgentVault, VaultKeyProvider};
use floe_conversation_contract::{ConversationFailure, MessageId, TranscriptReference};
use floe_conversation_core::{
    ConversationReadTarget, ConversationStoreFailure, MAX_TRANSCRIPT_PAGE_BYTES,
    MAX_TRANSCRIPT_PAGE_ENTRIES, TranscriptEntry, TranscriptEntryLookup, TranscriptPageBudget,
    TranscriptReadBoundary, TranscriptReadCursor, TranscriptReversePage,
};
use floe_kernel::PersonId;
use turso::transaction::{Transaction, TransactionBehavior};

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Return the current exact transcript boundary without initializing or
    /// repairing Conversation Core storage.
    pub(super) async fn read_conversation_head(
        &self,
        target: ConversationReadTarget,
    ) -> Result<TranscriptReadBoundary, ConversationStoreFailure> {
        validate_target_person(self.person_id, &target)?;
        let mut connection = self.connection().map_err(start_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| start_error(super::database_failure(error)))?;
        let result = async {
            require_core_v3_on(&transaction).await?;
            let scope = scope_for(self.person_id, &target);
            let head = load_head_on(&transaction, scope)
                .await?
                .ok_or(ConversationStoreFailure::InvalidTranscriptReadTarget)?;
            if head.state.identity != target.identity {
                return Err(ConversationStoreFailure::InvalidTranscriptReadTarget);
            }
            let through = if head.state.head_revision == 0 {
                None
            } else {
                let entry = entry_on(&transaction, scope, head.state.head_revision)
                    .await?
                    .ok_or(ConversationStoreFailure::Unavailable)?;
                Some(entry.reference)
            };
            Ok(TranscriptReadBoundary {
                target,
                head_revision: head.state.head_revision,
                through,
            })
        }
        .await;
        self.finish_conversation_core_read(transaction, result)
            .await
    }

    /// Look up one exact transcript record in a pinned boundary. MessageId
    /// absence has its own typed result; a reference never falls back to a
    /// nearby sequence or a different MessageId.
    pub(super) async fn read_transcript_entry(
        &self,
        target: ConversationReadTarget,
        boundary: TranscriptReadBoundary,
        lookup: TranscriptEntryLookup,
        max_bytes: usize,
    ) -> Result<TranscriptEntry, ConversationStoreFailure> {
        validate_target_person(self.person_id, &target)?;
        if max_bytes == 0 {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::InvalidInput,
            ));
        }
        let mut connection = self.connection().map_err(start_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| start_error(super::database_failure(error)))?;
        let result = async {
            require_core_v3_on(&transaction).await?;
            let scope =
                validate_boundary_on(&transaction, self.person_id, &target, &boundary).await?;
            let sequence = match lookup {
                TranscriptEntryLookup::MessageId(message_id) => {
                    if !message_id.is_valid() {
                        return Err(ConversationStoreFailure::Transition(
                            ConversationFailure::InvalidInput,
                        ));
                    }
                    let Some(sequence) =
                        message_sequence_on(&transaction, scope, message_id).await?
                    else {
                        return Err(ConversationStoreFailure::TranscriptMessageNotFound);
                    };
                    if sequence > boundary.head_revision {
                        return Err(ConversationStoreFailure::InvalidTranscriptBoundary);
                    }
                    sequence
                }
                TranscriptEntryLookup::Reference(reference) => {
                    validate_lookup_reference(&target, &boundary, reference)?;
                    reference.sequence
                }
            };
            let entry = entry_on(&transaction, scope, sequence)
                .await?
                .ok_or(ConversationStoreFailure::TranscriptReferenceMismatch)?;
            if let TranscriptEntryLookup::Reference(reference) = lookup
                && entry.reference != reference
            {
                return Err(ConversationStoreFailure::TranscriptReferenceMismatch);
            }
            self.validate_producing_task_reference_on(&transaction, &entry)
                .await?;
            let encoded_bytes = serde_json::to_vec(&entry)
                .map_err(|_| ConversationStoreFailure::Unavailable)?
                .len();
            if encoded_bytes > max_bytes {
                return Err(ConversationStoreFailure::PageItemExceedsBudget);
            }
            Ok(entry)
        }
        .await;
        self.finish_conversation_core_read(transaction, result)
            .await
    }

    /// Read one bounded reverse page. The SQL range query returns only
    /// sequence keys for at most `max_entries + 1` rows; exact entries are
    /// loaded and validated only until the entry or byte budget stops the
    /// page. Results are chronological within the page.
    pub(super) async fn read_previous_conversation_page(
        &self,
        target: ConversationReadTarget,
        cursor: TranscriptReadCursor,
        budget: TranscriptPageBudget,
    ) -> Result<TranscriptReversePage, ConversationStoreFailure> {
        validate_target_person(self.person_id, &target)?;
        if budget.max_entries == 0 || budget.max_bytes == 0 {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::InvalidInput,
            ));
        }
        if cursor.boundary.target != target {
            return Err(ConversationStoreFailure::InvalidTranscriptCursor);
        }
        let mut connection = self.connection().map_err(start_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| start_error(super::database_failure(error)))?;
        let result = async {
            require_core_v3_on(&transaction).await?;
            let scope =
                validate_boundary_on(&transaction, self.person_id, &target, &cursor.boundary)
                    .await?;
            let before_sequence = if let Some(before) = cursor.before {
                if before.validate().is_err()
                    || before.conversation_id != target.conversation_id
                    || before.branch_id != target.branch_id
                    || before.sequence > cursor.boundary.head_revision
                {
                    return Err(ConversationStoreFailure::InvalidTranscriptCursor);
                }
                let exact = entry_on(&transaction, scope, before.sequence)
                    .await?
                    .is_some_and(|entry| entry.reference == before);
                if !exact {
                    return Err(ConversationStoreFailure::InvalidTranscriptCursor);
                }
                Some(before.sequence)
            } else {
                None
            };
            let max_entries = budget.max_entries.min(MAX_TRANSCRIPT_PAGE_ENTRIES);
            let max_bytes = budget.max_bytes.min(MAX_TRANSCRIPT_PAGE_BYTES);
            let sequences = reverse_page_sequences_on(
                &transaction,
                scope,
                cursor.boundary.head_revision,
                before_sequence,
                max_entries.saturating_add(1),
            )
            .await?;
            let mut entries = Vec::with_capacity(max_entries.min(32));
            let mut encoded_bytes = 0usize;
            let mut has_more = false;
            for sequence in sequences {
                if entries.len() == max_entries {
                    has_more = true;
                    break;
                }
                let entry = entry_on(&transaction, scope, sequence)
                    .await?
                    .ok_or(ConversationStoreFailure::Unavailable)?;
                self.validate_producing_task_reference_on(&transaction, &entry)
                    .await?;
                let entry_bytes = serde_json::to_vec(&entry)
                    .map_err(|_| ConversationStoreFailure::Unavailable)?
                    .len();
                if entry_bytes > max_bytes.saturating_sub(encoded_bytes) {
                    if entries.is_empty() {
                        return Err(ConversationStoreFailure::PageItemExceedsBudget);
                    }
                    has_more = true;
                    break;
                }
                encoded_bytes = encoded_bytes.checked_add(entry_bytes).ok_or(
                    ConversationStoreFailure::Transition(ConversationFailure::InvalidInput),
                )?;
                entries.push(entry);
            }
            entries.reverse();
            let next_cursor = if has_more {
                entries.first().map(|entry| TranscriptReadCursor {
                    boundary: cursor.boundary,
                    before: Some(entry.reference),
                })
            } else {
                None
            };
            Ok(TranscriptReversePage {
                entries,
                next_cursor,
                has_more,
                encoded_bytes,
            })
        }
        .await;
        self.finish_conversation_core_read(transaction, result)
            .await
    }
}

fn validate_target_person(
    person_id: PersonId,
    target: &ConversationReadTarget,
) -> Result<(), ConversationStoreFailure> {
    target
        .validate()
        .map_err(ConversationStoreFailure::Transition)?;
    if target.identity.person_id != person_id {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::AgentMismatch,
        ));
    }
    Ok(())
}

fn scope_for(person_id: PersonId, target: &ConversationReadTarget) -> Scope {
    Scope::from_identity(
        person_id,
        &target.identity,
        target.conversation_id,
        target.branch_id,
    )
}

async fn validate_boundary_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    target: &ConversationReadTarget,
    boundary: &TranscriptReadBoundary,
) -> Result<Scope, ConversationStoreFailure> {
    if &boundary.target != target {
        return Err(ConversationStoreFailure::InvalidTranscriptBoundary);
    }
    let scope = scope_for(person_id, target);
    let head = load_head_on(transaction, scope)
        .await?
        .ok_or(ConversationStoreFailure::InvalidTranscriptReadTarget)?;
    if head.state.identity != target.identity || boundary.head_revision > head.state.head_revision {
        return Err(ConversationStoreFailure::InvalidTranscriptBoundary);
    }
    match (boundary.head_revision, boundary.through) {
        (0, None) => {}
        (sequence, Some(reference)) if sequence > 0 => {
            if reference.sequence != sequence
                || reference.conversation_id != target.conversation_id
                || reference.branch_id != target.branch_id
            {
                return Err(ConversationStoreFailure::InvalidTranscriptBoundary);
            }
            let exact = entry_on(transaction, scope, sequence)
                .await?
                .is_some_and(|entry| entry.reference == reference);
            if !exact {
                return Err(ConversationStoreFailure::InvalidTranscriptBoundary);
            }
        }
        _ => return Err(ConversationStoreFailure::InvalidTranscriptBoundary),
    }
    Ok(scope)
}

fn validate_lookup_reference(
    target: &ConversationReadTarget,
    boundary: &TranscriptReadBoundary,
    reference: TranscriptReference,
) -> Result<(), ConversationStoreFailure> {
    if reference.validate().is_err()
        || reference.conversation_id != target.conversation_id
        || reference.branch_id != target.branch_id
    {
        return Err(ConversationStoreFailure::TranscriptReferenceMismatch);
    }
    if reference.sequence > boundary.head_revision {
        return Err(ConversationStoreFailure::InvalidTranscriptBoundary);
    }
    Ok(())
}

async fn message_sequence_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    message_id: MessageId,
) -> Result<Option<u64>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT sequence FROM agent_conversation_core_v3_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND message_id = ?",
            (
                person,
                conversation,
                branch,
                message_id.as_uuid().to_string(),
            ),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let sequence = positive_integer(
        row.get::<i64>(0)
            .map_err(|_| ConversationStoreFailure::Unavailable)?,
    )?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(ConversationStoreFailure::Unavailable);
    }
    Ok(Some(sequence))
}

async fn reverse_page_sequences_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    through: u64,
    before: Option<u64>,
    limit: usize,
) -> Result<Vec<u64>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let limit =
        integer(u64::try_from(limit).map_err(|_| {
            ConversationStoreFailure::Transition(ConversationFailure::InvalidInput)
        })?)?;
    let mut rows = if let Some(before) = before {
        transaction
            .query(
                "SELECT sequence FROM agent_conversation_core_v3_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence <= ? AND sequence < ? ORDER BY sequence DESC LIMIT ?",
                (
                    person,
                    conversation,
                    branch,
                    integer(through)?,
                    integer(before)?,
                    limit,
                ),
            )
            .await
            .map_err(database_error)?
    } else {
        transaction
            .query(
                "SELECT sequence FROM agent_conversation_core_v3_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence <= ? ORDER BY sequence DESC LIMIT ?",
                (
                    person,
                    conversation,
                    branch,
                    integer(through)?,
                    limit,
                ),
            )
            .await
            .map_err(database_error)?
    };
    let mut sequences = Vec::new();
    while let Some(row) = rows.next().await.map_err(database_error)? {
        sequences.push(positive_integer(
            row.get::<i64>(0)
                .map_err(|_| ConversationStoreFailure::Unavailable)?,
        )?);
    }
    Ok(sequences)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use super::*;
    use crate::{RootKey, VaultKeyProvider};
    use floe_conversation_contract::{
        AdmissionTarget, AgentInstanceId, AssignmentId, ConversationMessage, ConversationReference,
        MessageAdmissionRequest, MessageOrigin,
    };
    use floe_kernel::{AgentFailure, CommandId};
    use uuid::Uuid;

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
            let path = std::env::temp_dir().join(format!("floe-core-reads-{}", Uuid::new_v4()));
            std::fs::create_dir(&path).expect("create isolated encrypted Vault test root");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("restrict encrypted Vault test root");
            Self(path)
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct Fixture {
        vault: EncryptedAgentVault<TestKeys>,
        _root: TestRoot,
        target: ConversationReadTarget,
        head_revision: u64,
    }

    impl Fixture {
        async fn new() -> Self {
            let root = TestRoot::new();
            let person_id = PersonId::new();
            let vault = EncryptedAgentVault::create(&root.0, person_id, TestKeys::default())
                .await
                .expect("create encrypted test Vault");
            Self {
                vault,
                _root: root,
                target: ConversationReadTarget {
                    identity: floe_conversation_contract::AgentIdentity {
                        person_id,
                        agent_instance_id: AgentInstanceId::new(),
                        assignment_id: AssignmentId::new(),
                        definition_id: "test.reads".into(),
                        definition_revision: 1,
                    },
                    conversation_id: floe_conversation_contract::ConversationId::new(),
                    branch_id: floe_conversation_contract::ConversationBranchId::new(),
                },
                head_revision: 0,
            }
        }

        async fn append(&mut self, text: &str) -> TranscriptReference {
            let target = if self.head_revision == 0 {
                AdmissionTarget::New {
                    identity: self.target.identity.clone(),
                    conversation_id: self.target.conversation_id,
                    branch_id: self.target.branch_id,
                }
            } else {
                AdmissionTarget::AppendToExisting {
                    reference: ConversationReference {
                        identity: self.target.identity.clone(),
                        conversation_id: self.target.conversation_id,
                        branch_id: self.target.branch_id,
                        head_revision: self.head_revision,
                    },
                }
            };
            let result = self
                .vault
                .append_conversation_input(MessageAdmissionRequest {
                    target,
                    message: ConversationMessage {
                        message_id: MessageId::new(),
                        command_id: CommandId::new(),
                        origin: MessageOrigin::Person {
                            person_id: self.target.identity.person_id,
                        },
                        text: text.to_owned(),
                        evidence: None,
                        task_id: None,
                    },
                })
                .await
                .expect("append encrypted Core input");
            self.head_revision = result.receipt.transcript.sequence;
            result.receipt.transcript
        }

        async fn read_boundary(&self) -> TranscriptReadBoundary {
            self.vault
                .read_conversation_head(self.target.clone())
                .await
                .expect("read encrypted Core head")
        }

        async fn ensure_empty_head(&self) {
            let mut connection = self.vault.connection().expect("connect test Vault");
            let transaction = connection
                .transaction()
                .await
                .expect("begin empty Core setup transaction");
            super::super::conversation_core::ensure_core_v3_on(&transaction)
                .await
                .expect("initialize Core only in encrypted test setup");
            transaction
                .execute(
                    "INSERT INTO agent_conversation_core_v3_heads (person_id, conversation_id, branch_id, identity_json, head_revision, state_revision, settled_prefix, recorder_epoch) VALUES (?, ?, ?, ?, 0, 0, 0, 0)",
                    (
                        self.target.identity.person_id.to_string(),
                        self.target.conversation_id.as_uuid().to_string(),
                        self.target.branch_id.as_uuid().to_string(),
                        serde_json::to_string(&self.target.identity).expect("encode test identity"),
                    ),
                )
                .await
                .expect("insert empty Core head in test setup");
            transaction.commit().await.expect("commit empty Core setup");
        }
    }

    #[tokio::test]
    async fn encrypted_head_read_does_not_create_a_missing_conversation_or_change_family() {
        let fixture = Fixture::new().await;
        let connection = fixture.vault.connection().expect("connect test Vault");
        let family_before = crate::schema::conversation_core_family_version(&connection)
            .await
            .expect("inspect Core family before read");
        drop(connection);
        assert_eq!(
            fixture
                .vault
                .read_conversation_head(fixture.target.clone())
                .await,
            Err(ConversationStoreFailure::InvalidTranscriptReadTarget)
        );
        let connection = fixture.vault.connection().expect("connect test Vault");
        assert_eq!(
            crate::schema::conversation_core_family_version(&connection)
                .await
                .expect("inspect Core family after read"),
            family_before
        );
        let mut rows = connection
            .query("SELECT COUNT(*) FROM agent_conversation_core_v3_heads", ())
            .await
            .expect("query Core head count");
        let row = rows
            .next()
            .await
            .expect("read head count row")
            .expect("head count");
        assert_eq!(row.get::<i64>(0).expect("head count value"), 0);
    }

    #[tokio::test]
    async fn encrypted_empty_transcript_has_a_pinned_empty_boundary_and_zero_budgets_fail() {
        let fixture = Fixture::new().await;
        fixture.ensure_empty_head().await;
        let boundary = fixture.read_boundary().await;
        assert_eq!(boundary.head_revision, 0);
        assert_eq!(boundary.through, None);
        let page = fixture
            .vault
            .read_previous_conversation_page(
                fixture.target.clone(),
                TranscriptReadCursor::start(boundary.clone()),
                TranscriptPageBudget {
                    max_entries: 2,
                    max_bytes: 1024,
                },
            )
            .await
            .expect("read empty encrypted transcript");
        assert!(page.entries.is_empty());
        assert!(!page.has_more);
        assert_eq!(page.next_cursor, None);
        for budget in [
            TranscriptPageBudget {
                max_entries: 0,
                max_bytes: 1024,
            },
            TranscriptPageBudget {
                max_entries: 2,
                max_bytes: 0,
            },
        ] {
            assert_eq!(
                fixture
                    .vault
                    .read_previous_conversation_page(
                        fixture.target.clone(),
                        TranscriptReadCursor::start(boundary.clone()),
                        budget,
                    )
                    .await,
                Err(ConversationStoreFailure::Transition(
                    ConversationFailure::InvalidInput
                ))
            );
        }
    }

    #[tokio::test]
    async fn encrypted_reverse_entry_budget_clamps_over_limit_requests() {
        let mut fixture = Fixture::new().await;
        for index in 0..=MAX_TRANSCRIPT_PAGE_ENTRIES {
            fixture.append(&format!("entry-{index}")).await;
        }
        let page = fixture
            .vault
            .read_previous_conversation_page(
                fixture.target.clone(),
                TranscriptReadCursor::start(fixture.read_boundary().await),
                TranscriptPageBudget {
                    max_entries: usize::MAX,
                    max_bytes: MAX_TRANSCRIPT_PAGE_BYTES + 1,
                },
            )
            .await
            .expect("clamp an over-limit reverse page request");
        assert_eq!(page.entries.len(), MAX_TRANSCRIPT_PAGE_ENTRIES);
        assert!(page.has_more);
        assert!(page.encoded_bytes < MAX_TRANSCRIPT_PAGE_BYTES);
        let final_entry = page
            .next_cursor
            .expect("the transcript has one older entry")
            .before
            .expect("the continuation has an exclusive cursor");
        assert_eq!(final_entry.sequence, 2);
        let older = fixture
            .vault
            .read_previous_conversation_page(
                fixture.target.clone(),
                TranscriptReadCursor {
                    boundary: fixture.read_boundary().await,
                    before: Some(final_entry),
                },
                TranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: 1024,
                },
            )
            .await
            .expect("read the one entry before the capped page");
        assert_eq!(older.entries.len(), 1);
        assert_eq!(older.entries[0].reference.sequence, 1);
        assert!(!older.has_more);
    }

    #[tokio::test]
    async fn encrypted_reverse_pages_are_chronological_pinned_and_byte_bounded() {
        let mut fixture = Fixture::new().await;
        let first = fixture.append("oldest λ🙂").await;
        let second = fixture.append("older item with more unicode 🧭🧭").await;
        let third = fixture.append("a longer middle item — λ🙂🧭").await;
        let fourth = fixture.append("newest").await;
        let boundary = fixture.read_boundary().await;
        assert_eq!(boundary.through, Some(fourth));

        let newest_entry = fixture
            .vault
            .read_transcript_entry(
                fixture.target.clone(),
                boundary.clone(),
                TranscriptEntryLookup::Reference(fourth),
                usize::MAX,
            )
            .await
            .expect("read exact newest reference");
        let newest_bytes = serde_json::to_vec(&newest_entry)
            .expect("encode returned record envelope")
            .len();
        let middle = fixture
            .vault
            .read_previous_conversation_page(
                fixture.target.clone(),
                TranscriptReadCursor::start(boundary.clone()),
                TranscriptPageBudget {
                    max_entries: usize::MAX,
                    max_bytes: newest_bytes,
                },
            )
            .await
            .expect("read exact byte-boundary page");
        assert_eq!(middle.entries.len(), 1);
        assert_eq!(middle.entries[0].reference, fourth);
        assert_eq!(middle.encoded_bytes, newest_bytes);
        assert!(middle.has_more);
        let retry_cursor = middle.next_cursor.clone().expect("older page remains");

        let third_entry = fixture
            .vault
            .read_transcript_entry(
                fixture.target.clone(),
                boundary.clone(),
                TranscriptEntryLookup::MessageId(third.message_id),
                usize::MAX,
            )
            .await
            .expect("read exact MessageId");
        let third_bytes = serde_json::to_vec(&third_entry)
            .expect("encode record envelope")
            .len();
        assert!(third_bytes > newest_bytes);
        assert_eq!(
            fixture
                .vault
                .read_previous_conversation_page(
                    fixture.target.clone(),
                    retry_cursor.clone(),
                    TranscriptPageBudget {
                        max_entries: 8,
                        max_bytes: third_bytes - 1,
                    },
                )
                .await,
            Err(ConversationStoreFailure::PageItemExceedsBudget),
            "an oversized first item fails without advancing the input cursor"
        );
        let exact_third = fixture
            .vault
            .read_previous_conversation_page(
                fixture.target.clone(),
                retry_cursor.clone(),
                TranscriptPageBudget {
                    max_entries: 8,
                    max_bytes: third_bytes,
                },
            )
            .await
            .expect("retry exact first older record");
        assert_eq!(exact_third.entries[0].reference, third);
        assert_eq!(exact_third.encoded_bytes, third_bytes);
        assert!(exact_third.has_more);

        let oldest_page = fixture
            .vault
            .read_previous_conversation_page(
                fixture.target.clone(),
                exact_third
                    .next_cursor
                    .clone()
                    .expect("older entries remain"),
                TranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: usize::MAX,
                },
            )
            .await
            .expect("read one older entry");
        assert_eq!(oldest_page.entries[0].reference, second);
        assert!(oldest_page.has_more);
        let final_page = fixture
            .vault
            .read_previous_conversation_page(
                fixture.target.clone(),
                oldest_page
                    .next_cursor
                    .clone()
                    .expect("oldest record remains"),
                TranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: usize::MAX,
                },
            )
            .await
            .expect("read oldest entry");
        assert_eq!(final_page.entries[0].reference, first);
        assert!(!final_page.has_more);
        assert_eq!(final_page.next_cursor, None);

        let forward = fixture
            .vault
            .read_conversation_page(
                ConversationReference {
                    identity: fixture.target.identity.clone(),
                    conversation_id: fixture.target.conversation_id,
                    branch_id: fixture.target.branch_id,
                    head_revision: boundary.head_revision,
                },
                None,
                TranscriptPageBudget {
                    max_entries: usize::MAX,
                    max_bytes: usize::MAX,
                },
            )
            .await
            .expect("read bounded forward comparison page");
        assert_eq!(
            forward
                .entries
                .iter()
                .map(|entry| entry.reference)
                .collect::<Vec<_>>(),
            [first, second, third, fourth]
        );
        let reverse_joined = final_page
            .entries
            .iter()
            .chain(oldest_page.entries.iter())
            .chain(exact_third.entries.iter())
            .chain(middle.entries.iter())
            .map(|entry| entry.reference)
            .collect::<Vec<_>>();
        assert_eq!(reverse_joined, [first, second, third, fourth]);

        let appended = fixture.append("outside the pinned boundary").await;
        let pinned_continuation = fixture
            .vault
            .read_previous_conversation_page(
                fixture.target.clone(),
                retry_cursor,
                TranscriptPageBudget {
                    max_entries: 8,
                    max_bytes: usize::MAX,
                },
            )
            .await
            .expect("continue within original boundary after append");
        assert_eq!(
            pinned_continuation
                .entries
                .iter()
                .map(|entry| entry.reference)
                .collect::<Vec<_>>(),
            [first, second, third]
        );
        assert!(
            !pinned_continuation
                .entries
                .iter()
                .any(|entry| entry.reference == appended)
        );
        assert!(!pinned_continuation.has_more);

        let unicode_entry_bytes = serde_json::to_vec(&forward.entries[0])
            .expect("encode unicode envelope")
            .len();
        assert!(unicode_entry_bytes > forward.entries[0].message.text.chars().count());
    }

    #[tokio::test]
    async fn encrypted_point_reads_distinguish_absence_stale_and_reference_mismatch() {
        let mut fixture = Fixture::new().await;
        let first = fixture.append("exact point").await;
        let boundary = fixture.read_boundary().await;
        let by_id = fixture
            .vault
            .read_transcript_entry(
                fixture.target.clone(),
                boundary.clone(),
                TranscriptEntryLookup::MessageId(first.message_id),
                usize::MAX,
            )
            .await
            .expect("lookup by exact MessageId");
        let by_reference = fixture
            .vault
            .read_transcript_entry(
                fixture.target.clone(),
                boundary.clone(),
                TranscriptEntryLookup::Reference(first),
                usize::MAX,
            )
            .await
            .expect("lookup by exact transcript reference");
        assert_eq!(by_id, by_reference);

        assert_eq!(
            fixture
                .vault
                .read_transcript_entry(
                    fixture.target.clone(),
                    boundary.clone(),
                    TranscriptEntryLookup::MessageId(MessageId::new()),
                    usize::MAX,
                )
                .await,
            Err(ConversationStoreFailure::TranscriptMessageNotFound)
        );
        let mismatched = TranscriptReference {
            message_id: MessageId::new(),
            ..first
        };
        assert_eq!(
            fixture
                .vault
                .read_transcript_entry(
                    fixture.target.clone(),
                    boundary.clone(),
                    TranscriptEntryLookup::Reference(mismatched),
                    usize::MAX,
                )
                .await,
            Err(ConversationStoreFailure::TranscriptReferenceMismatch)
        );
        assert_eq!(
            fixture
                .vault
                .read_transcript_entry(
                    fixture.target.clone(),
                    boundary.clone(),
                    TranscriptEntryLookup::Reference(first),
                    1,
                )
                .await,
            Err(ConversationStoreFailure::PageItemExceedsBudget)
        );

        let appended = fixture.append("later than snapshot").await;
        assert_eq!(
            fixture
                .vault
                .read_transcript_entry(
                    fixture.target.clone(),
                    boundary.clone(),
                    TranscriptEntryLookup::MessageId(appended.message_id),
                    usize::MAX,
                )
                .await,
            Err(ConversationStoreFailure::InvalidTranscriptBoundary),
            "an exact MessageId beyond the pinned boundary is stale, not absent"
        );
    }

    #[tokio::test]
    async fn encrypted_reverse_cursors_reject_unknown_mismatched_and_cross_scope_values() {
        let mut fixture = Fixture::new().await;
        let first = fixture.append("first").await;
        fixture.append("second").await;
        let boundary = fixture.read_boundary().await;
        let base_cursor = TranscriptReadCursor::start(boundary.clone());
        let budget = TranscriptPageBudget {
            max_entries: 1,
            max_bytes: usize::MAX,
        };

        let mut unknown_boundary = base_cursor.clone();
        unknown_boundary.boundary.head_revision += 1;
        unknown_boundary.boundary.through = Some(TranscriptReference {
            sequence: unknown_boundary.boundary.head_revision,
            message_id: MessageId::new(),
            conversation_id: fixture.target.conversation_id,
            branch_id: fixture.target.branch_id,
        });
        assert_eq!(
            fixture
                .vault
                .read_previous_conversation_page(fixture.target.clone(), unknown_boundary, budget,)
                .await,
            Err(ConversationStoreFailure::InvalidTranscriptBoundary)
        );

        let mut mismatched_before = base_cursor.clone();
        mismatched_before.before = Some(TranscriptReference {
            message_id: MessageId::new(),
            ..first
        });
        assert_eq!(
            fixture
                .vault
                .read_previous_conversation_page(fixture.target.clone(), mismatched_before, budget,)
                .await,
            Err(ConversationStoreFailure::InvalidTranscriptCursor)
        );

        for change_scope in ["person", "agent", "branch"] {
            let mut cursor = base_cursor.clone();
            let mut requested_target = fixture.target.clone();
            match change_scope {
                "person" => {
                    cursor.boundary.target.identity.person_id = PersonId::new();
                }
                "agent" => {
                    cursor.boundary.target.identity.agent_instance_id = AgentInstanceId::new();
                }
                "branch" => {
                    cursor.boundary.target.branch_id =
                        floe_conversation_contract::ConversationBranchId::new();
                }
                _ => unreachable!(),
            }
            assert_eq!(
                fixture
                    .vault
                    .read_previous_conversation_page(requested_target.clone(), cursor, budget)
                    .await,
                Err(ConversationStoreFailure::InvalidTranscriptCursor),
                "reject cursor crossing {change_scope} scope"
            );
            if change_scope == "person" {
                requested_target.identity.person_id = PersonId::new();
                assert_eq!(
                    fixture.vault.read_conversation_head(requested_target).await,
                    Err(ConversationStoreFailure::Transition(
                        ConversationFailure::AgentMismatch
                    ))
                );
            }
        }
    }
}
