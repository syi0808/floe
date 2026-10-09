//! Transactional owner resolution for bounded Core transcript reads.
//!
//! Every read composes Core, owner mapping/link, typed payload metadata and
//! live coverage inside one Deferred transaction. Only entries included in a
//! page hydrate typed and coverage bodies. Task-journal and owner-link checks
//! remain separately bounded validation I/O outside the encoded payload
//! counter.

use std::collections::HashSet;

use super::context_dependencies::{
    ContextDependencyCoverageHeader, hydrate_context_dependency_coverage_on,
    preflight_context_dependency_coverage_on,
};
use super::conversation_core::{
    Scope, checkpoint_on, database_error, entry_on, integer, load_head_on, nonnegative_integer,
    owner_error, owner_store_failure, require_core_v3_on, start_error, unavailable,
};
use super::conversation_core_reads::{
    message_sequence_on, reverse_page_sequences_on, scope_for, validate_boundary_on,
    validate_lookup_reference, validate_target_person,
};
use super::owner_custody::{
    OwnerTranscriptInputMapping, TypedTranscriptEvidenceLink,
    manager_session_alias_for_reference_on, manager_session_alias_on,
    manager_session_binding_for_session_on, manager_session_message_alias,
    owner_transcript_input_for_reference_on, typed_transcript_link_for_entry_on,
    validate_manager_pristine_session_on,
};
use super::session_archive::{ManagerArchiveLookup, manager_archive_manifest_on};
use super::typed_history::StoredHeader;
use super::{EncryptedAgentVault, VaultKeyProvider};
use floe_access::DependencyCoverage;
use floe_agent_contract::{AgentFailure, TaskExecutionReceiptRef};
use floe_conversation::{
    AgentMessage, AgentSession, MAX_SESSION_BYTES, SessionHistoryMessage, SessionHistoryPage,
    SessionRecoveryPointer, TypedAgentMessageProvenance, TypedAgentMessageReference,
};
use floe_conversation_contract::{AgentIdentity, ConversationFailure, TranscriptReference};
#[cfg(test)]
use floe_conversation_core::TranscriptReadCursor;
use floe_conversation_core::{
    ConversationReadTarget, ConversationStoreFailure, MAX_TRANSCRIPT_PAGE_BYTES,
    MAX_TRANSCRIPT_PAGE_ENTRIES, TranscriptEntry, TranscriptEntryKind, TranscriptEntryLookup,
    TranscriptReadBoundary,
};
use floe_kernel::{PersonId, RunId};
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

/// Owner-level paging limits count transcript envelopes, typed evidence and
/// the current live coverage projection together.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(super) struct OwnerTranscriptPageBudget {
    pub max_entries: usize,
    pub max_bytes: usize,
}

/// Typed history is either present and exactly owner-linked, or explicitly
/// absent for an inbound entry which has no typed owner row. Generated output
/// without a valid typed link is an error and is never projected from Core
/// text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum OwnerTranscriptTypedEvidence {
    Absent,
    Present {
        reference: TypedAgentMessageReference,
        message: AgentMessage,
        coverage: DependencyCoverage,
        first_recording_run_id: RunId,
        original_task_receipt: Option<TaskExecutionReceiptRef>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OwnerResolvedTranscriptEntry {
    pub transcript_entry: TranscriptEntry,
    pub session_id: Uuid,
    pub owner_user_message_id: Uuid,
    pub owner_input: TranscriptReference,
    pub original_owner_run_id: RunId,
    pub owner_identity: AgentIdentity,
    pub typed_evidence: OwnerTranscriptTypedEvidence,
    pub encoded_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(super) struct OwnerResolvedTranscriptPage {
    /// Entries are chronological within this page.
    pub entries: Vec<OwnerResolvedTranscriptEntry>,
    pub next_cursor: Option<TranscriptReadCursor>,
    pub has_more: bool,
    pub encoded_bytes: usize,
}

pub(super) enum OwnerEntryPreflight {
    Absent {
        mapping: OwnerTranscriptInputMapping,
        core_bytes: usize,
    },
    Present {
        mapping: OwnerTranscriptInputMapping,
        link: TypedTranscriptEvidenceLink,
        header: StoredHeader,
        coverage_header: ContextDependencyCoverageHeader,
        encoded_bytes: usize,
    },
}

struct ManagerSummaryPreflight {
    alias_id: Uuid,
    transcript_sequence: u64,
    turn_id: Uuid,
    message: AgentMessage,
    coverage_header: ContextDependencyCoverageHeader,
    encoded_bytes: usize,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Load only the bounded owner Session shell for an unscoped personal
    /// Manager Session. SQLite replaces the serialized message vector before
    /// it crosses into Rust; normalized Core remains the only transcript
    /// authority. Scoped Expert Sessions continue through their own owner
    /// route until the next milestone.
    pub(crate) async fn read_manager_session_shell(
        &self,
        session_id: Uuid,
    ) -> Result<Option<AgentSession>, AgentFailure> {
        if session_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| owner_store_failure(start_error(super::database_failure(error))))?;
        let result = async {
            self.manager_session_shell_on(&transaction, session_id)
                .await
        }
        .await;
        self.finish_conversation_core_read(transaction, result)
            .await
            .map_err(owner_store_failure)
    }

    pub(super) async fn manager_session_shell_on(
        &self,
        transaction: &Transaction<'_>,
        session_id: Uuid,
    ) -> Result<Option<AgentSession>, ConversationStoreFailure> {
        let mut size_rows = transaction
            .query(
                "SELECT length(CAST(payload AS BLOB)) FROM agent_sessions WHERE id = ? LIMIT 2",
                [session_id.to_string()],
            )
            .await
            .map_err(database_error)?;
        let Some(size_row) = size_rows.next().await.map_err(database_error)? else {
            return Ok(None);
        };
        if size_rows.next().await.map_err(database_error)?.is_some() {
            return Err(unavailable());
        }
        let encoded_size = size_row.get::<i64>(0).map_err(|_| unavailable())?;
        if encoded_size < 0
            || usize::try_from(encoded_size).map_err(|_| unavailable())? > MAX_SESSION_BYTES
        {
            return Err(ConversationStoreFailure::PageItemExceedsBudget);
        }
        drop(size_rows);
        let mut rows = transaction
            .query(
                "SELECT revision, json_type(payload, '$.scope'), json_extract(payload, '$.data_classes[0]'), json_array_length(payload, '$.messages') FROM agent_sessions WHERE id = ? LIMIT 2",
                [session_id.to_string()],
            )
            .await
            .map_err(database_error)?;
        let header = rows
            .next()
            .await
            .map_err(database_error)?
            .ok_or_else(unavailable)?;
        if rows.next().await.map_err(database_error)?.is_some() {
            return Err(unavailable());
        }
        let scope_kind = header.get::<String>(1).map_err(|_| unavailable())?;
        let data_class = header.get::<String>(2).map_err(|_| unavailable())?;
        if scope_kind != "null" || data_class != "personal" {
            return Ok(None);
        }
        if header.get::<i64>(3).map_err(|_| unavailable())? != 0 {
            return Err(ConversationStoreFailure::UnsupportedStoredMeaning);
        }
        let revision = nonnegative_integer(header.get::<i64>(0).map_err(|_| unavailable())?)?;
        drop(rows);
        let mut payload_rows = transaction
            .query(
                "SELECT json_set(payload, '$.messages', json('[]')) FROM agent_sessions WHERE id = ? AND revision = ? LIMIT 2",
                (session_id.to_string(), integer(revision)?),
            )
            .await
            .map_err(database_error)?;
        let payload = payload_rows
            .next()
            .await
            .map_err(database_error)?
            .ok_or_else(unavailable)?
            .get::<String>(0)
            .map_err(|_| unavailable())?;
        if payload_rows.next().await.map_err(database_error)?.is_some() {
            return Err(unavailable());
        }
        drop(payload_rows);
        let session: AgentSession = serde_json::from_str(&payload).map_err(|_| unavailable())?;
        session
            .validate_owner_snapshot(self.person_id)
            .map_err(|_| ConversationStoreFailure::UnsupportedStoredMeaning)?;
        if session.id != session_id || session.revision != revision || !session.messages.is_empty()
        {
            return Err(unavailable());
        }
        if manager_session_binding_for_session_on(transaction, self.person_id, session_id)
            .await?
            .is_none()
        {
            validate_manager_pristine_session_on(transaction, self.person_id, session_id).await?;
        }
        Ok(Some(session))
    }

    /// Bounded product history read over normalized Manager custody. Cursor
    /// aliases resolve to exact immutable Core references, then owner evidence
    /// and live coverage are preflighted and hydrated under the same snapshot.
    pub(crate) async fn read_manager_session_history_page(
        &self,
        session_id: Uuid,
        before_message_id: Option<Uuid>,
        required_user_id: Option<Uuid>,
        limit: usize,
        byte_limit: usize,
    ) -> Result<SessionHistoryPage, AgentFailure> {
        if session_id.is_nil() || limit == 0 || byte_limit == 0 {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| owner_store_failure(start_error(super::database_failure(error))))?;
        let result = async {
            let Some(binding) =
                manager_session_binding_for_session_on(&transaction, self.person_id, session_id)
                    .await?
            else {
                if before_message_id.is_some() {
                    return Err(ConversationStoreFailure::TranscriptMessageNotFound);
                }
                validate_manager_pristine_session_on(&transaction, self.person_id, session_id)
                    .await?;
                if required_user_id.is_some() {
                    return Err(ConversationStoreFailure::TranscriptMessageNotFound);
                }
                return Ok(SessionHistoryPage {
                    messages: Vec::new(),
                    has_earlier_messages: false,
                    encoded_bytes: 0,
                });
            };
            let target = ConversationReadTarget {
                identity: binding.identity,
                conversation_id: binding.conversation_id,
                branch_id: binding.branch_id,
            };
            validate_target_person(self.person_id, &target)?;
            require_core_v3_on(&transaction).await?;
            let scope = scope_for(self.person_id, &target);
            let head = load_head_on(&transaction, scope)
                .await?
                .ok_or(ConversationStoreFailure::InvalidTranscriptReadTarget)?;
            let through = if head.state.head_revision == 0 {
                None
            } else {
                Some(
                    entry_on(&transaction, scope, head.state.head_revision)
                        .await?
                        .ok_or(ConversationStoreFailure::Unavailable)?
                        .reference,
                )
            };
            let boundary = TranscriptReadBoundary {
                target: target.clone(),
                head_revision: head.state.head_revision,
                through,
            };
            let checkpoint = checkpoint_on(&transaction, scope).await?;
            let checkpoint_sequence = checkpoint
                .as_ref()
                .map_or(0, |checkpoint| checkpoint.through.sequence);
            let limit = limit.min(256);
            let byte_limit = byte_limit.min(MAX_TRANSCRIPT_PAGE_BYTES);
            let compaction_summary = if let Some(checkpoint) = &checkpoint {
                let manifest = manager_archive_manifest_on(
                    &transaction,
                    self.person_id,
                    session_id,
                    ManagerArchiveLookup::Checkpoint(checkpoint.through.sequence),
                )
                .await
                .map_err(owner_error)?
                .ok_or(ConversationStoreFailure::Unavailable)?;
                if manifest.checkpoint().map_err(owner_error)? != *checkpoint
                    || manifest.conversation_id != binding.conversation_id
                    || manifest.branch_id != binding.branch_id
                {
                    return Err(ConversationStoreFailure::Unavailable);
                }
                let coverage_header = preflight_context_dependency_coverage_on(
                    &transaction,
                    self.person_id,
                    session_id,
                    manifest.through_turn_id,
                )
                .await
                .map_err(owner_error)?;
                let coverage_bytes = coverage_header.accounted_bytes().map_err(owner_error)?;
                let message = AgentMessage::Compaction {
                    turn_id: manifest.through_turn_id,
                    summary: manifest.summary,
                    recovery: SessionRecoveryPointer {
                        archive_id: manifest.archive_id,
                        source_revision: manifest.source_revision,
                        through_turn_id: manifest.through_turn_id,
                        archived_message_count: manifest.message_count,
                    },
                };
                if manager_session_message_alias(&message) != manifest.summary_alias_id {
                    return Err(ConversationStoreFailure::Unavailable);
                }
                let encoded_bytes = serde_json::to_vec(&message)
                    .map_err(|_| unavailable())?
                    .len()
                    .checked_add(coverage_bytes)
                    .ok_or(ConversationStoreFailure::Unavailable)?;
                Some(ManagerSummaryPreflight {
                    alias_id: manifest.summary_alias_id,
                    transcript_sequence: checkpoint.through.sequence,
                    turn_id: manifest.through_turn_id,
                    message,
                    coverage_header,
                    encoded_bytes,
                })
            } else {
                None
            };
            let mut cursor_points_to_summary = false;
            let mut before_sequence = if let Some(alias_id) = before_message_id {
                if compaction_summary
                    .as_ref()
                    .is_some_and(|summary| summary.alias_id == alias_id)
                {
                    cursor_points_to_summary = true;
                    Some(checkpoint_sequence)
                } else {
                    let reference = manager_session_alias_on(
                        &transaction,
                        self.person_id,
                        session_id,
                        alias_id,
                    )
                    .await?
                    .ok_or(ConversationStoreFailure::TranscriptMessageNotFound)?;
                    validate_lookup_reference(&target, &boundary, reference)?;
                    if reference.sequence <= checkpoint_sequence {
                        return Err(ConversationStoreFailure::InvalidTranscriptCursor);
                    }
                    let exact = entry_on(&transaction, scope, reference.sequence)
                        .await?
                        .is_some_and(|entry| entry.reference == reference);
                    if !exact {
                        return Err(ConversationStoreFailure::InvalidTranscriptCursor);
                    }
                    Some(reference.sequence)
                }
            } else {
                None
            };
            let mut messages = Vec::with_capacity(limit.min(128));
            let required_user = if let Some(required_user_id) = required_user_id {
                let reference = manager_session_alias_on(
                    &transaction,
                    self.person_id,
                    session_id,
                    required_user_id,
                )
                .await?
                .ok_or(ConversationStoreFailure::TranscriptMessageNotFound)?;
                validate_lookup_reference(&target, &boundary, reference)?;
                let exact_entry = entry_on(&transaction, scope, reference.sequence)
                    .await?
                    .ok_or(ConversationStoreFailure::Unavailable)?;
                if exact_entry.reference != reference {
                    return Err(ConversationStoreFailure::InvalidTranscriptCursor);
                }
                self.validate_producing_task_reference_on(&transaction, &exact_entry)
                    .await?;
                let preflight = self
                    .preflight_owner_entry_on(
                        &transaction,
                        &target,
                        session_id,
                        &exact_entry,
                        scope,
                    )
                    .await?;
                let required_bytes = preflight.encoded_bytes();
                if required_bytes > byte_limit {
                    return Err(ConversationStoreFailure::PageItemExceedsBudget);
                }
                let exact = self
                    .hydrate_owner_entry_on(&transaction, exact_entry, preflight)
                    .await?;
                if exact.transcript_entry.kind != TranscriptEntryKind::Inbound
                    || exact.owner_user_message_id != required_user_id
                    || exact.typed_evidence != OwnerTranscriptTypedEvidence::Absent
                    || manager_session_alias_for_reference_on(
                        &transaction,
                        self.person_id,
                        session_id,
                        exact.transcript_entry.reference,
                    )
                    .await?
                        != Some(required_user_id)
                {
                    return Err(owner_mismatch());
                }
                Some(SessionHistoryMessage {
                    alias_id: required_user_id,
                    transcript_sequence: exact.transcript_entry.reference.sequence,
                    encoded_bytes: exact.encoded_bytes,
                    coverage: DependencyCoverage::Independent,
                    message: AgentMessage::User {
                        turn_id: exact.original_owner_run_id.as_uuid(),
                        message_id: required_user_id,
                        text: exact.transcript_entry.message.text,
                    },
                })
            } else {
                None
            };
            let mut encoded_bytes = required_user
                .as_ref()
                .map_or(0, |message| message.encoded_bytes);
            let mut has_earlier_messages = false;
            loop {
                if messages.len() >= limit || head.state.head_revision == 0 {
                    break;
                }
                let take = (limit - messages.len()).min(MAX_TRANSCRIPT_PAGE_ENTRIES);
                let mut sequences = reverse_page_sequences_on(
                    &transaction,
                    scope,
                    boundary.head_revision,
                    before_sequence,
                    take.saturating_add(1),
                )
                .await?;
                let batch_has_more = sequences.len() > take;
                let extra_has_live_entry = sequences
                    .get(take)
                    .is_some_and(|sequence| *sequence > checkpoint_sequence);
                if batch_has_more {
                    sequences.truncate(take);
                }
                sequences.retain(|sequence| *sequence > checkpoint_sequence);
                if sequences.is_empty() {
                    break;
                }
                let batch_before = sequences.last().copied();
                let mut stopped_for_bytes = false;
                for sequence in sequences {
                    if required_user
                        .as_ref()
                        .is_some_and(|message| message.transcript_sequence == sequence)
                    {
                        continue;
                    }
                    let entry = entry_on(&transaction, scope, sequence)
                        .await?
                        .ok_or(ConversationStoreFailure::Unavailable)?;
                    self.validate_producing_task_reference_on(&transaction, &entry)
                        .await?;
                    let preflight = self
                        .preflight_owner_entry_on(&transaction, &target, session_id, &entry, scope)
                        .await?;
                    let entry_bytes = preflight.encoded_bytes();
                    if entry_bytes > byte_limit.saturating_sub(encoded_bytes) {
                        if messages.is_empty() && required_user.is_none() {
                            return Err(ConversationStoreFailure::PageItemExceedsBudget);
                        }
                        has_earlier_messages = true;
                        stopped_for_bytes = true;
                        break;
                    }
                    let resolved = self
                        .hydrate_owner_entry_on(&transaction, entry, preflight)
                        .await?;
                    let public_alias = manager_session_alias_for_reference_on(
                        &transaction,
                        self.person_id,
                        session_id,
                        resolved.transcript_entry.reference,
                    )
                    .await?;
                    let alias_id = public_alias.ok_or_else(owner_mismatch)?;
                    let (message, coverage) = match resolved.transcript_entry.kind {
                        TranscriptEntryKind::Inbound => {
                            if resolved.typed_evidence != OwnerTranscriptTypedEvidence::Absent
                                || alias_id != resolved.owner_user_message_id
                            {
                                return Err(owner_mismatch());
                            }
                            (
                                AgentMessage::User {
                                    turn_id: resolved.original_owner_run_id.as_uuid(),
                                    message_id: resolved.owner_user_message_id,
                                    text: resolved.transcript_entry.message.text,
                                },
                                DependencyCoverage::Independent,
                            )
                        }
                        TranscriptEntryKind::GeneratedOutput => match resolved.typed_evidence {
                            OwnerTranscriptTypedEvidence::Present {
                                message, coverage, ..
                            } => (message, coverage),
                            OwnerTranscriptTypedEvidence::Absent => return Err(owner_mismatch()),
                        },
                    };
                    encoded_bytes = encoded_bytes
                        .checked_add(resolved.encoded_bytes)
                        .ok_or(ConversationStoreFailure::Unavailable)?;
                    messages.push(SessionHistoryMessage {
                        alias_id,
                        transcript_sequence: resolved.transcript_entry.reference.sequence,
                        encoded_bytes: resolved.encoded_bytes,
                        coverage,
                        message,
                    });
                }
                if stopped_for_bytes {
                    break;
                }
                if batch_has_more {
                    if messages.len() >= limit {
                        has_earlier_messages |= extra_has_live_entry;
                        break;
                    }
                    let Some(batch_before) = batch_before else {
                        break;
                    };
                    before_sequence = Some(batch_before);
                } else {
                    break;
                }
            }
            messages.reverse();
            if let Some(required_user) = required_user {
                let required_user_alias = required_user.alias_id;
                if !messages
                    .iter()
                    .any(|message| message.alias_id == required_user_alias)
                {
                    messages.push(required_user);
                }
                messages.sort_by_key(|message| message.transcript_sequence);
                while messages.len() > limit || encoded_bytes > byte_limit {
                    let Some(index) = messages
                        .iter()
                        .position(|message| message.alias_id != required_user_alias)
                    else {
                        return Err(ConversationStoreFailure::PageItemExceedsBudget);
                    };
                    let removed = messages.remove(index);
                    encoded_bytes = encoded_bytes
                        .checked_sub(removed.encoded_bytes)
                        .ok_or(ConversationStoreFailure::Unavailable)?;
                    has_earlier_messages = true;
                }
            }
            if !cursor_points_to_summary
                && !has_earlier_messages
                && let Some(summary) = compaction_summary
            {
                if messages.len() >= limit {
                    has_earlier_messages = true;
                } else if summary.encoded_bytes > byte_limit.saturating_sub(encoded_bytes) {
                    if messages.is_empty() && required_user_id.is_none() {
                        return Err(ConversationStoreFailure::PageItemExceedsBudget);
                    }
                    has_earlier_messages = true;
                } else {
                    let coverage = hydrate_context_dependency_coverage_on(
                        &transaction,
                        self.person_id,
                        session_id,
                        summary.turn_id,
                        summary.coverage_header,
                        #[cfg(test)]
                        &self.context_coverage_payload_hydrations,
                    )
                    .await
                    .map_err(owner_error)?;
                    encoded_bytes = encoded_bytes
                        .checked_add(summary.encoded_bytes)
                        .ok_or(ConversationStoreFailure::Unavailable)?;
                    messages.push(SessionHistoryMessage {
                        alias_id: summary.alias_id,
                        transcript_sequence: summary.transcript_sequence,
                        encoded_bytes: summary.encoded_bytes,
                        coverage,
                        message: summary.message,
                    });
                    messages.sort_by_key(|message| message.transcript_sequence);
                }
            }
            Ok(SessionHistoryPage {
                messages,
                has_earlier_messages,
                encoded_bytes,
            })
        }
        .await;
        self.finish_conversation_core_read(transaction, result)
            .await
            .map_err(owner_store_failure)
    }

    /// Read the bounded newest Manager history suffix used by explicit
    /// learning discovery without loading the owner Session vector. The
    /// caller owns the same Deferred transaction used to read owner state and
    /// learning evidence. It never expands a compacted prefix from the
    /// archive; only the bounded live suffix after the checkpoint is read.
    pub(super) async fn read_manager_learning_history_on(
        &self,
        transaction: &Transaction<'_>,
        session_id: Uuid,
        max_messages: usize,
        max_bytes: usize,
    ) -> Result<Vec<SessionHistoryMessage>, ConversationStoreFailure> {
        if session_id.is_nil() || max_messages == 0 || max_bytes == 0 {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::InvalidInput,
            ));
        }
        let Some(binding) =
            manager_session_binding_for_session_on(transaction, self.person_id, session_id).await?
        else {
            validate_manager_pristine_session_on(transaction, self.person_id, session_id).await?;
            return Ok(Vec::new());
        };
        let target = ConversationReadTarget {
            identity: binding.identity,
            conversation_id: binding.conversation_id,
            branch_id: binding.branch_id,
        };
        validate_target_person(self.person_id, &target)?;
        require_core_v3_on(transaction).await?;
        let scope = scope_for(self.person_id, &target);
        let head = load_head_on(transaction, scope)
            .await?
            .ok_or(ConversationStoreFailure::InvalidTranscriptReadTarget)?;
        if head.state.identity != target.identity {
            return Err(owner_mismatch());
        }
        let checkpoint_sequence = checkpoint_on(transaction, scope)
            .await?
            .map_or(0, |checkpoint| checkpoint.through.sequence);
        let limit = max_messages.min(256);
        let byte_limit = max_bytes.min(MAX_TRANSCRIPT_PAGE_BYTES);
        let mut messages = Vec::with_capacity(limit.min(64));
        let mut encoded_bytes = 0usize;
        let mut before_sequence = None;
        let mut stop_suffix = false;

        while messages.len() < limit && !stop_suffix {
            let take = (limit - messages.len()).min(MAX_TRANSCRIPT_PAGE_ENTRIES);
            let mut sequences = reverse_page_sequences_on(
                transaction,
                scope,
                head.state.head_revision,
                before_sequence,
                take.saturating_add(1),
            )
            .await?;
            let batch_has_more = sequences.len() > take;
            if batch_has_more {
                sequences.truncate(take);
            }
            if sequences.is_empty() {
                break;
            }
            for sequence in sequences.iter().copied() {
                if sequence <= checkpoint_sequence {
                    stop_suffix = true;
                    break;
                }
                let entry = entry_on(transaction, scope, sequence)
                    .await?
                    .ok_or_else(unavailable)?;
                self.validate_producing_task_reference_on(transaction, &entry)
                    .await?;
                let preflight = self
                    .preflight_owner_entry_on(transaction, &target, session_id, &entry, scope)
                    .await?;
                let item_bytes = preflight.encoded_bytes();
                if item_bytes > byte_limit.saturating_sub(encoded_bytes) {
                    if messages.is_empty() {
                        return Err(ConversationStoreFailure::PageItemExceedsBudget);
                    }
                    stop_suffix = true;
                    break;
                }
                let message = self
                    .resolve_manager_transcript_entry_preflighted_on(
                        transaction,
                        session_id,
                        entry,
                        preflight,
                    )
                    .await?;
                encoded_bytes = encoded_bytes
                    .checked_add(message.encoded_bytes)
                    .ok_or_else(unavailable)?;
                messages.push(message);
                if messages.len() >= limit {
                    break;
                }
            }
            if messages.len() >= limit || stop_suffix || !batch_has_more {
                break;
            }
            before_sequence = sequences.last().copied();
        }
        messages.reverse();
        let mut aliases = HashSet::with_capacity(messages.len());
        if messages
            .iter()
            .any(|message| !aliases.insert(message.alias_id))
        {
            return Err(unavailable());
        }
        Ok(messages)
    }

    /// Point-read one original Manager User contribution without hydrating a
    /// Session vector. The returned sequence allows a bounded context page to
    /// include an old Continue/Resume origin exactly once and in order.
    pub(crate) async fn read_manager_session_user_message(
        &self,
        session_id: Uuid,
        message_id: Uuid,
    ) -> Result<Option<SessionHistoryMessage>, AgentFailure> {
        if session_id.is_nil() || message_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| owner_store_failure(start_error(super::database_failure(error))))?;
        let result = async {
            let Some(binding) =
                manager_session_binding_for_session_on(&transaction, self.person_id, session_id)
                    .await?
            else {
                validate_manager_pristine_session_on(&transaction, self.person_id, session_id)
                    .await?;
                return Ok(None);
            };
            let target = ConversationReadTarget {
                identity: binding.identity,
                conversation_id: binding.conversation_id,
                branch_id: binding.branch_id,
            };
            validate_target_person(self.person_id, &target)?;
            require_core_v3_on(&transaction).await?;
            let scope = scope_for(self.person_id, &target);
            let head = load_head_on(&transaction, scope)
                .await?
                .ok_or(ConversationStoreFailure::InvalidTranscriptReadTarget)?;
            let through = if head.state.head_revision == 0 {
                None
            } else {
                Some(
                    entry_on(&transaction, scope, head.state.head_revision)
                        .await?
                        .ok_or(ConversationStoreFailure::Unavailable)?
                        .reference,
                )
            };
            let boundary = TranscriptReadBoundary {
                target: target.clone(),
                head_revision: head.state.head_revision,
                through,
            };
            let Some(reference) =
                manager_session_alias_on(&transaction, self.person_id, session_id, message_id)
                    .await?
            else {
                return Ok(None);
            };
            let (scope, entry) = self
                .exact_core_entry_on(
                    &transaction,
                    &target,
                    &boundary,
                    TranscriptEntryLookup::Reference(reference),
                )
                .await?;
            let preflight = self
                .preflight_owner_entry_on(&transaction, &target, session_id, &entry, scope)
                .await?;
            let resolved = self
                .hydrate_owner_entry_on(&transaction, entry, preflight)
                .await?;
            if resolved.transcript_entry.kind != TranscriptEntryKind::Inbound
                || resolved.owner_user_message_id != message_id
                || resolved.typed_evidence != OwnerTranscriptTypedEvidence::Absent
            {
                return Err(owner_mismatch());
            }
            Ok(Some(SessionHistoryMessage {
                alias_id: message_id,
                transcript_sequence: resolved.transcript_entry.reference.sequence,
                encoded_bytes: resolved.encoded_bytes,
                coverage: DependencyCoverage::Independent,
                message: AgentMessage::User {
                    turn_id: resolved.original_owner_run_id.as_uuid(),
                    message_id,
                    text: resolved.transcript_entry.message.text,
                },
            }))
        }
        .await;
        self.finish_conversation_core_read(transaction, result)
            .await
            .map_err(owner_store_failure)
    }

    /// Resolve one Manager transcript entry and its stable product alias in a
    /// caller-owned Deferred or Immediate transaction. Archive reads use the
    /// same owner and byte-accounting checks as product pages.
    pub(super) async fn resolve_manager_transcript_entry_preflighted_on(
        &self,
        transaction: &Transaction<'_>,
        session_id: Uuid,
        entry: TranscriptEntry,
        preflight: OwnerEntryPreflight,
    ) -> Result<SessionHistoryMessage, ConversationStoreFailure> {
        let resolved = self
            .hydrate_owner_entry_on(transaction, entry, preflight)
            .await?;
        let alias_id = manager_session_alias_for_reference_on(
            transaction,
            self.person_id,
            session_id,
            resolved.transcript_entry.reference,
        )
        .await?
        .ok_or(ConversationStoreFailure::TranscriptMessageNotFound)?;
        let (message, coverage) = match resolved.transcript_entry.kind {
            TranscriptEntryKind::Inbound => {
                if resolved.typed_evidence != OwnerTranscriptTypedEvidence::Absent
                    || alias_id != resolved.owner_user_message_id
                {
                    return Err(owner_mismatch());
                }
                (
                    AgentMessage::User {
                        turn_id: resolved.original_owner_run_id.as_uuid(),
                        message_id: resolved.owner_user_message_id,
                        text: resolved.transcript_entry.message.text,
                    },
                    DependencyCoverage::Independent,
                )
            }
            TranscriptEntryKind::GeneratedOutput => match resolved.typed_evidence {
                OwnerTranscriptTypedEvidence::Present {
                    message, coverage, ..
                } => (message, coverage),
                OwnerTranscriptTypedEvidence::Absent => return Err(owner_mismatch()),
            },
        };
        Ok(SessionHistoryMessage {
            alias_id,
            transcript_sequence: resolved.transcript_entry.reference.sequence,
            encoded_bytes: resolved.encoded_bytes,
            coverage,
            message,
        })
    }

    /// Resolve one exact Core record and all of its owner evidence under one
    /// Deferred snapshot. The owner Session is identified explicitly; the
    /// stored Session aggregate is never hydrated.
    #[cfg(test)]
    pub(super) async fn read_owner_transcript_entry(
        &self,
        target: ConversationReadTarget,
        owner_session_id: Uuid,
        boundary: TranscriptReadBoundary,
        lookup: TranscriptEntryLookup,
        max_bytes: usize,
    ) -> Result<OwnerResolvedTranscriptEntry, ConversationStoreFailure> {
        validate_target_person(self.person_id, &target)?;
        if owner_session_id.is_nil() || max_bytes == 0 {
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
            let (scope, entry) = self
                .exact_core_entry_on(&transaction, &target, &boundary, lookup)
                .await?;
            let preflight = self
                .preflight_owner_entry_on(&transaction, &target, owner_session_id, &entry, scope)
                .await?;
            let max_bytes = max_bytes.min(MAX_TRANSCRIPT_PAGE_BYTES);
            let encoded_bytes = preflight.encoded_bytes();
            if encoded_bytes > max_bytes {
                return Err(ConversationStoreFailure::PageItemExceedsBudget);
            }
            self.hydrate_owner_entry_on(&transaction, entry, preflight)
                .await
        }
        .await;
        self.finish_conversation_core_read(transaction, result)
            .await
    }

    /// Resolve one pinned reverse page under one Deferred transaction. Each
    /// candidate is metadata-preflighted before either body is fetched; an
    /// oversized first item is an error, while a later oversized item stops
    /// the page without moving the cursor past it.
    #[cfg(test)]
    pub(super) async fn read_previous_owner_transcript_page(
        &self,
        target: ConversationReadTarget,
        owner_session_id: Uuid,
        cursor: TranscriptReadCursor,
        budget: OwnerTranscriptPageBudget,
    ) -> Result<OwnerResolvedTranscriptPage, ConversationStoreFailure> {
        validate_target_person(self.person_id, &target)?;
        if owner_session_id.is_nil() || budget.max_entries == 0 || budget.max_bytes == 0 {
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
                let preflight = self
                    .preflight_owner_entry_on(
                        &transaction,
                        &target,
                        owner_session_id,
                        &entry,
                        scope,
                    )
                    .await?;
                let entry_bytes = preflight.encoded_bytes();
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
                entries.push(
                    self.hydrate_owner_entry_on(&transaction, entry, preflight)
                        .await?,
                );
            }
            entries.reverse();
            let next_cursor = if has_more {
                entries.first().map(|entry| TranscriptReadCursor {
                    boundary: cursor.boundary,
                    before: Some(entry.transcript_entry.reference),
                })
            } else {
                None
            };
            Ok(OwnerResolvedTranscriptPage {
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

    async fn exact_core_entry_on(
        &self,
        transaction: &Transaction<'_>,
        target: &ConversationReadTarget,
        boundary: &TranscriptReadBoundary,
        lookup: TranscriptEntryLookup,
    ) -> Result<(Scope, TranscriptEntry), ConversationStoreFailure> {
        let scope = validate_boundary_on(transaction, self.person_id, target, boundary).await?;
        let sequence = match lookup {
            TranscriptEntryLookup::MessageId(message_id) => {
                if !message_id.is_valid() {
                    return Err(ConversationStoreFailure::Transition(
                        ConversationFailure::InvalidInput,
                    ));
                }
                let Some(sequence) = message_sequence_on(transaction, scope, message_id).await?
                else {
                    return Err(ConversationStoreFailure::TranscriptMessageNotFound);
                };
                if sequence > boundary.head_revision {
                    return Err(ConversationStoreFailure::InvalidTranscriptBoundary);
                }
                sequence
            }
            TranscriptEntryLookup::Reference(reference) => {
                validate_lookup_reference(target, boundary, reference)?;
                reference.sequence
            }
        };
        let entry = entry_on(transaction, scope, sequence)
            .await?
            .ok_or(ConversationStoreFailure::TranscriptReferenceMismatch)?;
        if let TranscriptEntryLookup::Reference(reference) = lookup
            && entry.reference != reference
        {
            return Err(ConversationStoreFailure::TranscriptReferenceMismatch);
        }
        self.validate_producing_task_reference_on(transaction, &entry)
            .await?;
        Ok((scope, entry))
    }

    pub(super) async fn preflight_owner_entry_on(
        &self,
        transaction: &Transaction<'_>,
        target: &ConversationReadTarget,
        owner_session_id: Uuid,
        entry: &TranscriptEntry,
        scope: Scope,
    ) -> Result<OwnerEntryPreflight, ConversationStoreFailure> {
        let core_bytes = serialized_entry_bytes(entry)?;
        let link = typed_transcript_link_for_entry_on(transaction, self.person_id, entry.reference)
            .await?;
        match entry.kind {
            TranscriptEntryKind::Inbound => {
                if link.is_some() {
                    return Err(owner_mismatch());
                }
                let mapping = owner_transcript_input_for_reference_on(
                    transaction,
                    self.person_id,
                    entry.reference,
                )
                .await?
                .ok_or_else(owner_mismatch)?;
                validate_owner_mapping(&mapping, self.person_id, owner_session_id, target)?;
                if mapping.input != entry.reference
                    || mapping.original_binding.identity != target.identity
                    || scope != scope_for(self.person_id, target)
                {
                    return Err(owner_mismatch());
                }
                Ok(OwnerEntryPreflight::Absent {
                    mapping,
                    core_bytes,
                })
            }
            TranscriptEntryKind::GeneratedOutput => {
                let link = link.ok_or_else(owner_mismatch)?;
                if link.transcript_entry != *entry {
                    return Err(owner_mismatch());
                }
                let mapping = owner_transcript_input_for_reference_on(
                    transaction,
                    self.person_id,
                    link.owner_input,
                )
                .await?
                .ok_or_else(owner_mismatch)?;
                validate_owner_mapping(&mapping, self.person_id, owner_session_id, target)?;
                if mapping.input != link.owner_input
                    || mapping.session_id != link.owner_session_id
                    || mapping.input.conversation_id != entry.reference.conversation_id
                    || mapping.input.branch_id != entry.reference.branch_id
                    || link.first_recording_run_id
                        != entry.producer_run.ok_or_else(owner_mismatch)?
                    || link.typed_reference.person_id() != self.person_id
                    || link.typed_reference.session_id() != owner_session_id
                    || link.typed_reference.provenance()
                        != TypedAgentMessageProvenance::OwnerRecorded
                    || link.typed_reference.turn_id() != link.first_recording_run_id.as_uuid()
                    || link.transcript_entry.reference != entry.reference
                {
                    return Err(owner_mismatch());
                }
                let header = self
                    .preflight_typed_agent_message_on(transaction, &link.typed_reference)
                    .await
                    .map_err(owner_error)?
                    .ok_or_else(owner_mismatch)?;
                if header.reference != link.typed_reference {
                    return Err(unavailable());
                }
                let coverage_header = preflight_context_dependency_coverage_on(
                    transaction,
                    self.person_id,
                    owner_session_id,
                    link.typed_reference.turn_id(),
                )
                .await
                .map_err(owner_error)?;
                let evidence_bytes = header.encoded_bytes().map_err(owner_error)?;
                let coverage_bytes = coverage_header.accounted_bytes().map_err(owner_error)?;
                let encoded_bytes = core_bytes
                    .checked_add(evidence_bytes)
                    .and_then(|bytes| bytes.checked_add(coverage_bytes))
                    .ok_or(ConversationStoreFailure::Transition(
                        ConversationFailure::InvalidInput,
                    ))?;
                Ok(OwnerEntryPreflight::Present {
                    mapping,
                    link,
                    header,
                    coverage_header,
                    encoded_bytes,
                })
            }
        }
    }

    pub(super) async fn hydrate_owner_entry_on(
        &self,
        transaction: &Transaction<'_>,
        entry: TranscriptEntry,
        preflight: OwnerEntryPreflight,
    ) -> Result<OwnerResolvedTranscriptEntry, ConversationStoreFailure> {
        #[cfg(test)]
        self.owner_transcript_entry_hydrations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match preflight {
            OwnerEntryPreflight::Absent {
                mapping,
                core_bytes,
            } => Ok(OwnerResolvedTranscriptEntry {
                transcript_entry: entry,
                session_id: mapping.session_id,
                owner_user_message_id: mapping.owner_user_message_id,
                owner_input: mapping.input,
                original_owner_run_id: mapping.original_owner_run_id,
                owner_identity: mapping.original_binding.identity,
                typed_evidence: OwnerTranscriptTypedEvidence::Absent,
                encoded_bytes: core_bytes,
            }),
            OwnerEntryPreflight::Present {
                mapping,
                link,
                header,
                coverage_header,
                encoded_bytes,
            } => {
                let message = self
                    .hydrate_typed_agent_message_on(transaction, &header)
                    .await
                    .map_err(owner_error)?;
                if message.turn_id() != link.typed_reference.turn_id() {
                    return Err(unavailable());
                }
                let coverage = hydrate_context_dependency_coverage_on(
                    transaction,
                    self.person_id,
                    mapping.session_id,
                    link.typed_reference.turn_id(),
                    coverage_header,
                    #[cfg(test)]
                    &self.context_coverage_payload_hydrations,
                )
                .await
                .map_err(owner_error)?;
                let coverage_bytes = coverage
                    .as_persisted_bytes()
                    .map_err(|_| unavailable())?
                    .len();
                if coverage_bytes > coverage_header.accounted_bytes().map_err(owner_error)? {
                    return Err(unavailable());
                }
                Ok(OwnerResolvedTranscriptEntry {
                    transcript_entry: entry,
                    session_id: mapping.session_id,
                    owner_user_message_id: mapping.owner_user_message_id,
                    owner_input: mapping.input,
                    original_owner_run_id: mapping.original_owner_run_id,
                    owner_identity: mapping.original_binding.identity,
                    typed_evidence: OwnerTranscriptTypedEvidence::Present {
                        reference: link.typed_reference,
                        message,
                        coverage,
                        first_recording_run_id: link.first_recording_run_id,
                        original_task_receipt: link.original_task_receipt,
                    },
                    encoded_bytes,
                })
            }
        }
    }
}

impl OwnerEntryPreflight {
    pub(super) fn encoded_bytes(&self) -> usize {
        match self {
            Self::Absent { core_bytes, .. } => *core_bytes,
            Self::Present { encoded_bytes, .. } => *encoded_bytes,
        }
    }
}

fn validate_owner_mapping(
    mapping: &OwnerTranscriptInputMapping,
    person_id: PersonId,
    owner_session_id: Uuid,
    target: &ConversationReadTarget,
) -> Result<(), ConversationStoreFailure> {
    mapping.validate(person_id)?;
    if mapping.person_id != person_id
        || mapping.session_id != owner_session_id
        || mapping.original_binding.identity != target.identity
        || mapping.original_binding.identity.person_id != person_id
        || mapping.original_binding.conversation_id != target.conversation_id
        || mapping.original_binding.branch_id != target.branch_id
        || mapping.original_binding.input != mapping.input
        || mapping.original_owner_run_id != mapping.original_binding.run_id
    {
        return Err(owner_mismatch());
    }
    Ok(())
}

fn serialized_entry_bytes(entry: &TranscriptEntry) -> Result<usize, ConversationStoreFailure> {
    serde_json::to_vec(entry)
        .map(|bytes| bytes.len())
        .map_err(|_| unavailable())
}

fn owner_mismatch() -> ConversationStoreFailure {
    ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
}
