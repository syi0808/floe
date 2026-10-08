//! Transactional owner resolution for bounded Core transcript reads.
//!
//! Every read composes Core, owner mapping/link, typed payload metadata and
//! live coverage inside one Deferred transaction. Only entries included in a
//! page hydrate typed and coverage bodies. Task-journal and owner-link checks
//! remain separately bounded validation I/O outside the encoded payload
//! counter.

use super::context_dependencies::{
    ContextDependencyCoverageHeader, hydrate_context_dependency_coverage_on,
    preflight_context_dependency_coverage_on,
};
use super::conversation_core::{
    Scope, entry_on, owner_error, require_core_v3_on, start_error, unavailable,
};
use super::conversation_core_reads::{
    message_sequence_on, reverse_page_sequences_on, scope_for, validate_boundary_on,
    validate_lookup_reference, validate_target_person,
};
use super::owner_custody::{
    OwnerTranscriptInputMapping, TypedTranscriptEvidenceLink,
    owner_transcript_input_for_reference_on, typed_transcript_link_for_entry_on,
};
use super::typed_history::StoredHeader;
use super::{EncryptedAgentVault, VaultKeyProvider};
use floe_access::DependencyCoverage;
use floe_agent_contract::TaskExecutionReceiptRef;
use floe_conversation::{AgentMessage, TypedAgentMessageProvenance, TypedAgentMessageReference};
use floe_conversation_contract::{AgentIdentity, ConversationFailure, TranscriptReference};
use floe_conversation_core::{
    ConversationReadTarget, ConversationStoreFailure, MAX_TRANSCRIPT_PAGE_BYTES,
    MAX_TRANSCRIPT_PAGE_ENTRIES, TranscriptEntry, TranscriptEntryKind, TranscriptEntryLookup,
    TranscriptReadBoundary, TranscriptReadCursor,
};
use floe_kernel::{PersonId, RunId};
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

/// Owner-level paging limits count transcript envelopes, typed evidence and
/// the current live coverage projection together.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
pub(super) struct OwnerResolvedTranscriptPage {
    /// Entries are chronological within this page.
    pub entries: Vec<OwnerResolvedTranscriptEntry>,
    pub next_cursor: Option<TranscriptReadCursor>,
    pub has_more: bool,
    pub encoded_bytes: usize,
}

enum OwnerEntryPreflight {
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

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Resolve one exact Core record and all of its owner evidence under one
    /// Deferred snapshot. The owner Session is identified explicitly; the
    /// stored Session aggregate is never hydrated.
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

    async fn preflight_owner_entry_on(
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

    async fn hydrate_owner_entry_on(
        &self,
        transaction: &Transaction<'_>,
        entry: TranscriptEntry,
        preflight: OwnerEntryPreflight,
    ) -> Result<OwnerResolvedTranscriptEntry, ConversationStoreFailure> {
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
    fn encoded_bytes(&self) -> usize {
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
