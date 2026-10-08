//! Optional owner-to-transcript mappings and typed output proofs.
//!
//! This module owns the v1 custody tables, exact-key reads/writes, and the
//! typed-message composer. Shared transcript transitions and owner verification
//! remain in `conversation_core` and are imported with Vault-scoped visibility.

use super::conversation_core::{
    Scope, active_recorder_on, database_error, decode, encode, ensure_core_v3_on, entry_on,
    hex_digest, integer, open_receipt_on, owner_error, parse_branch_id, parse_conversation_id,
    parse_digest, parse_logical_contribution_id, parse_message_id, parse_run_id, parse_uuid,
    positive_integer, recording_receipt_on, schema_error, task_evidence_reference, unavailable,
};
use super::{EncryptedAgentVault, VaultKeyProvider};
use floe_agent_contract::{TaskExecutionEvidence, TaskExecutionReceiptRef, TaskReceipt};
use floe_conversation::{
    AgentMessage, RunRecord, TYPED_AGENT_MESSAGE_OWNER_NAMESPACE, TypedAgentMessageProvenance,
    TypedAgentMessageReference,
};
use floe_conversation_contract::{
    AgentIdentity, ConversationBranchId, ConversationFailure, ConversationId, ConversationMessage,
    MessageEvidenceReference, MessageId, MessageOrigin, TaskEvidenceReference, TranscriptReference,
};
use floe_conversation_core::{
    ConversationStoreFailure, ExecutorDomain, RecorderFence, RecordingReceipt, RecordingRequest,
    TranscriptEntry, TranscriptEntryKind, recording_content_digest, replay_recording_entry,
};
use floe_kernel::{CommandId, PersonId, RunId};
use serde::{Deserialize, Serialize};
use turso::transaction::Transaction;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerInputBinding {
    pub(super) run_id: RunId,
    pub(super) session_id: Uuid,
    pub(super) owner_user_message_id: Uuid,
    pub(super) identity: AgentIdentity,
    pub(super) conversation_id: ConversationId,
    pub(super) branch_id: ConversationBranchId,
    pub(super) input: TranscriptReference,
    pub(super) executor_domain: ExecutorDomain,
    pub(super) executor_generation: u64,
}

/// Immutable first-admission mapping from an exact Core inbound record to
/// the owner Session message and original owner Run that admitted it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnerTranscriptInputMapping {
    pub(super) person_id: PersonId,
    pub(super) input: TranscriptReference,
    pub(super) session_id: Uuid,
    pub(super) owner_user_message_id: Uuid,
    pub(super) original_owner_run_id: RunId,
    pub(super) original_binding: OwnerInputBinding,
}

impl OwnerTranscriptInputMapping {
    pub(super) fn validate(&self, person_id: PersonId) -> Result<(), ConversationStoreFailure> {
        self.input
            .validate()
            .map_err(ConversationStoreFailure::Transition)?;
        self.original_binding.validate(person_id)?;
        if self.person_id != person_id
            || self.session_id.is_nil()
            || self.owner_user_message_id.is_nil()
            || self.original_owner_run_id != self.original_binding.run_id
            || self.session_id != self.original_binding.session_id
            || self.owner_user_message_id != self.original_binding.owner_user_message_id
            || self.input != self.original_binding.input
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        Ok(())
    }
}

/// Immutable owner proof joining one typed AgentMessage to the exact Core
/// transcript entry that was first recorded for its logical contribution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TypedTranscriptEvidenceLink {
    pub(super) person_id: PersonId,
    pub(super) typed_reference: TypedAgentMessageReference,
    pub(super) transcript_entry: TranscriptEntry,
    pub(super) owner_input: TranscriptReference,
    pub(super) owner_session_id: Uuid,
    pub(super) owner_turn_id: Uuid,
    pub(super) contribution_id: floe_conversation_contract::LogicalContributionId,
    pub(super) first_recording_run_id: RunId,
    pub(super) original_task_receipt: Option<TaskExecutionReceiptRef>,
}

impl TypedTranscriptEvidenceLink {
    pub(super) fn validate(&self, person_id: PersonId) -> Result<(), ConversationStoreFailure> {
        self.typed_reference
            .validate_for_storage()
            .map_err(owner_error)?;
        if self.person_id != person_id
            || self.typed_reference.person_id() != person_id
            || self.typed_reference.owner_namespace() != TYPED_AGENT_MESSAGE_OWNER_NAMESPACE
            || self.typed_reference.provenance() != TypedAgentMessageProvenance::OwnerRecorded
            || self.owner_session_id.is_nil()
            || self.owner_turn_id.is_nil()
            || self.first_recording_run_id.as_uuid() != self.owner_turn_id
            || self.typed_reference.session_id() != self.owner_session_id
            || self.typed_reference.turn_id() != self.owner_turn_id
            || self.transcript_entry.kind != TranscriptEntryKind::GeneratedOutput
            || self.transcript_entry.reference.validate().is_err()
            || self.transcript_entry.producer_run != Some(self.first_recording_run_id)
            || self.transcript_entry.contribution_id != Some(self.contribution_id)
            || self.transcript_entry.producing_task
                != self
                    .original_task_receipt
                    .as_ref()
                    .map(task_evidence_reference)
                    .transpose()?
            || self
                .transcript_entry
                .message
                .evidence
                .as_ref()
                .map(|evidence| evidence.digest())
                != Some(self.typed_reference.digest())
            || self.transcript_entry.message.task_id
                != self
                    .original_task_receipt
                    .as_ref()
                    .map(|receipt| receipt.execution.task_id)
            || self.owner_input.conversation_id != self.transcript_entry.reference.conversation_id
            || self.owner_input.branch_id != self.transcript_entry.reference.branch_id
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        if let Some(receipt) = self.original_task_receipt.as_ref() {
            receipt.validate().map_err(|_| {
                ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
            })?;
        }
        Ok(())
    }
}

/// Inputs to the owner composer deliberately omit an evidence digest and a
/// TaskEvidenceReference. The composer derives both from the typed owner row
/// and the verified Task owner receipt.
#[derive(Clone, Debug)]
pub(super) struct TypedConversationRecordingRequest {
    pub recorder: RecorderFence,
    pub message_id: MessageId,
    pub command_id: CommandId,
    pub contribution_id: floe_conversation_contract::LogicalContributionId,
    pub typed_entry_id: Uuid,
    pub typed_message: AgentMessage,
}

impl OwnerInputBinding {
    pub(super) fn validate(&self, person_id: PersonId) -> Result<(), ConversationStoreFailure> {
        self.identity
            .validate()
            .map_err(ConversationStoreFailure::Transition)?;
        self.input
            .validate()
            .map_err(ConversationStoreFailure::Transition)?;
        if !self.run_id.is_valid()
            || self.session_id.is_nil()
            || self.owner_user_message_id.is_nil()
            || self.identity.person_id != person_id
            || !self.conversation_id.is_valid()
            || !self.branch_id.is_valid()
            || self.input.conversation_id != self.conversation_id
            || self.input.branch_id != self.branch_id
            || self.executor_domain != ExecutorDomain::HostRun
            || self.executor_generation == 0
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        Ok(())
    }
}

pub(super) async fn owner_custody_v1_present_on(
    transaction: &Transaction<'_>,
) -> Result<bool, ConversationStoreFailure> {
    crate::schema::conversation_owner_custody_family_present(transaction)
        .await
        .map_err(schema_error)
}

pub(super) async fn ensure_owner_custody_v1_on(
    transaction: &Transaction<'_>,
) -> Result<(), ConversationStoreFailure> {
    crate::schema::ensure_conversation_owner_custody_family(transaction)
        .await
        .map_err(schema_error)
}

pub(super) async fn owner_input_binding_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    run_id: RunId,
) -> Result<Option<OwnerInputBinding>, ConversationStoreFailure> {
    let mut rows = transaction
        .query(
            "SELECT session_id, owner_user_message_id, conversation_id, branch_id, input_sequence, input_message_id, binding_json FROM agent_conversation_core_v3_owner_bindings WHERE person_id = ? AND run_id = ?",
            (person_id.to_string(), run_id.as_uuid().to_string()),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let session_id = parse_uuid(&row.get::<String>(0).map_err(|_| unavailable())?)?;
    let owner_user_message_id = parse_uuid(&row.get::<String>(1).map_err(|_| unavailable())?)?;
    let conversation_id = parse_conversation_id(&row.get::<String>(2).map_err(|_| unavailable())?)?;
    let branch_id = parse_branch_id(&row.get::<String>(3).map_err(|_| unavailable())?)?;
    let sequence = positive_integer(row.get::<i64>(4).map_err(|_| unavailable())?)?;
    let message_id = parse_message_id(&row.get::<String>(5).map_err(|_| unavailable())?)?;
    let binding_json = row.get::<String>(6).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let binding: OwnerInputBinding = decode(&binding_json)?;
    binding.validate(person_id)?;
    if encode(&binding)? != binding_json
        || binding.run_id != run_id
        || binding.session_id != session_id
        || binding.owner_user_message_id != owner_user_message_id
        || binding.conversation_id != conversation_id
        || binding.branch_id != branch_id
        || binding.input.sequence != sequence
        || binding.input.message_id != message_id
    {
        return Err(unavailable());
    }
    Ok(Some(binding))
}

fn input_mapping_from_row(
    person_id: PersonId,
    conversation_id: &str,
    branch_id: &str,
    sequence: i64,
    message_id: &str,
    session_id: &str,
    owner_user_message_id: &str,
    original_owner_run_id: &str,
    mapping_json: &str,
) -> Result<OwnerTranscriptInputMapping, ConversationStoreFailure> {
    let mapping: OwnerTranscriptInputMapping = decode(mapping_json)?;
    mapping.validate(person_id)?;
    if encode(&mapping)? != mapping_json
        || mapping.input.conversation_id != parse_conversation_id(conversation_id)?
        || mapping.input.branch_id != parse_branch_id(branch_id)?
        || mapping.input.sequence != positive_integer(sequence)?
        || mapping.input.message_id != parse_message_id(message_id)?
        || mapping.session_id != parse_uuid(session_id)?
        || mapping.owner_user_message_id != parse_uuid(owner_user_message_id)?
        || mapping.original_owner_run_id != parse_run_id(original_owner_run_id)?
    {
        return Err(unavailable());
    }
    Ok(mapping)
}

/// Point lookup through the exact Core input key. No owner Run history is
/// scanned and no Core/owner UUID equality is inferred.
pub(super) async fn owner_transcript_input_for_reference_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    input: TranscriptReference,
) -> Result<Option<OwnerTranscriptInputMapping>, ConversationStoreFailure> {
    input
        .validate()
        .map_err(ConversationStoreFailure::Transition)?;
    if !owner_custody_v1_present_on(transaction).await? {
        return Ok(None);
    }
    let mut rows = transaction
        .query(
            "SELECT conversation_id, branch_id, input_sequence, input_message_id, session_id, owner_user_message_id, original_owner_run_id, mapping_json FROM agent_conversation_owner_transcript_inputs_v1 INDEXED BY agent_conversation_owner_transcript_inputs_core_key_v1 WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND input_sequence = ? AND input_message_id = ? LIMIT 2",
            (
                person_id.to_string(),
                input.conversation_id.as_uuid().to_string(),
                input.branch_id.as_uuid().to_string(),
                integer(input.sequence)?,
                input.message_id.as_uuid().to_string(),
            ),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let mapping = input_mapping_from_row(
        person_id,
        &row.get::<String>(0).map_err(|_| unavailable())?,
        &row.get::<String>(1).map_err(|_| unavailable())?,
        row.get::<i64>(2).map_err(|_| unavailable())?,
        &row.get::<String>(3).map_err(|_| unavailable())?,
        &row.get::<String>(4).map_err(|_| unavailable())?,
        &row.get::<String>(5).map_err(|_| unavailable())?,
        &row.get::<String>(6).map_err(|_| unavailable())?,
        &row.get::<String>(7).map_err(|_| unavailable())?,
    )?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    if mapping.input != input {
        return Err(unavailable());
    }
    let scope = Scope::from_identity(
        person_id,
        &mapping.original_binding.identity,
        input.conversation_id,
        input.branch_id,
    );
    let entry = entry_on(transaction, scope, input.sequence)
        .await?
        .ok_or_else(unavailable)?;
    if entry.reference != input
        || entry.kind != TranscriptEntryKind::Inbound
        || entry.message.origin != (MessageOrigin::Person { person_id })
        || entry.message.task_id.is_some()
        || entry.message.evidence.is_some()
    {
        return Err(unavailable());
    }
    Ok(Some(mapping))
}

/// Unique owner-key lookup used for Continue. It is explicitly pinned to the
/// owner-key index, so unrelated retained Runs do not affect lookup work.
pub(super) async fn owner_transcript_input_for_owner_message_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    session_id: Uuid,
    owner_user_message_id: Uuid,
) -> Result<Option<OwnerTranscriptInputMapping>, ConversationStoreFailure> {
    if person_id.is_valid() == false || session_id.is_nil() || owner_user_message_id.is_nil() {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::InvalidInput,
        ));
    }
    if !owner_custody_v1_present_on(transaction).await? {
        return Ok(None);
    }
    let mut rows = transaction
        .query(
            "SELECT conversation_id, branch_id, input_sequence, input_message_id, session_id, owner_user_message_id, original_owner_run_id, mapping_json FROM agent_conversation_owner_transcript_inputs_v1 INDEXED BY agent_conversation_owner_transcript_inputs_owner_key_v1 WHERE person_id = ? AND session_id = ? AND owner_user_message_id = ? LIMIT 2",
            (
                person_id.to_string(),
                session_id.to_string(),
                owner_user_message_id.to_string(),
            ),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let mapping = input_mapping_from_row(
        person_id,
        &row.get::<String>(0).map_err(|_| unavailable())?,
        &row.get::<String>(1).map_err(|_| unavailable())?,
        row.get::<i64>(2).map_err(|_| unavailable())?,
        &row.get::<String>(3).map_err(|_| unavailable())?,
        &row.get::<String>(4).map_err(|_| unavailable())?,
        &row.get::<String>(5).map_err(|_| unavailable())?,
        &row.get::<String>(6).map_err(|_| unavailable())?,
        &row.get::<String>(7).map_err(|_| unavailable())?,
    )?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    if mapping.session_id != session_id || mapping.owner_user_message_id != owner_user_message_id {
        return Err(unavailable());
    }
    Ok(Some(mapping))
}

/// Resolve the mapping linked to one exact owner Run through a Person/Run
/// primary-key lookup, then validate the referenced immutable input mapping.
pub(super) async fn owner_transcript_input_for_run_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    run_id: RunId,
) -> Result<Option<OwnerTranscriptInputMapping>, ConversationStoreFailure> {
    if !owner_custody_v1_present_on(transaction).await? {
        return Ok(None);
    }
    let mut rows = transaction
        .query(
            "SELECT conversation_id, branch_id, input_sequence, input_message_id FROM agent_conversation_owner_transcript_run_inputs_v1 WHERE person_id = ? AND run_id = ? LIMIT 2",
            (person_id.to_string(), run_id.as_uuid().to_string()),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let input = TranscriptReference {
        conversation_id: parse_conversation_id(&row.get::<String>(0).map_err(|_| unavailable())?)?,
        branch_id: parse_branch_id(&row.get::<String>(1).map_err(|_| unavailable())?)?,
        sequence: positive_integer(row.get::<i64>(2).map_err(|_| unavailable())?)?,
        message_id: parse_message_id(&row.get::<String>(3).map_err(|_| unavailable())?)?,
    };
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let mapping = owner_transcript_input_for_reference_on(transaction, person_id, input)
        .await?
        .ok_or_else(unavailable)?;
    let run_binding = owner_input_binding_on(transaction, person_id, run_id)
        .await?
        .ok_or_else(unavailable)?;
    if run_binding.session_id != mapping.session_id
        || run_binding.owner_user_message_id != mapping.owner_user_message_id
        || run_binding.identity != mapping.original_binding.identity
        || run_binding.conversation_id != mapping.input.conversation_id
        || run_binding.branch_id != mapping.input.branch_id
        || run_binding.input != mapping.input
    {
        return Err(unavailable());
    }
    Ok(Some(mapping))
}

pub(super) async fn insert_owner_transcript_input_mapping_on(
    transaction: &Transaction<'_>,
    mapping: &OwnerTranscriptInputMapping,
) -> Result<(), ConversationStoreFailure> {
    mapping.validate(mapping.person_id)?;
    if owner_transcript_input_for_reference_on(transaction, mapping.person_id, mapping.input)
        .await?
        .is_some()
        || owner_transcript_input_for_owner_message_on(
            transaction,
            mapping.person_id,
            mapping.session_id,
            mapping.owner_user_message_id,
        )
        .await?
        .is_some()
    {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::RunAlreadyUsed,
        ));
    }
    transaction
        .execute(
            "INSERT INTO agent_conversation_owner_transcript_inputs_v1 (person_id, conversation_id, branch_id, input_sequence, input_message_id, session_id, owner_user_message_id, original_owner_run_id, mapping_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                mapping.person_id.to_string(),
                mapping.input.conversation_id.as_uuid().to_string(),
                mapping.input.branch_id.as_uuid().to_string(),
                integer(mapping.input.sequence)?,
                mapping.input.message_id.as_uuid().to_string(),
                mapping.session_id.to_string(),
                mapping.owner_user_message_id.to_string(),
                mapping.original_owner_run_id.as_uuid().to_string(),
                encode(mapping)?,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

pub(super) async fn insert_owner_transcript_run_input_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    run_id: RunId,
    mapping: &OwnerTranscriptInputMapping,
) -> Result<(), ConversationStoreFailure> {
    mapping.validate(person_id)?;
    if owner_transcript_input_for_reference_on(transaction, person_id, mapping.input)
        .await?
        .as_ref()
        != Some(mapping)
    {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch,
        ));
    }
    transaction
        .execute(
            "INSERT INTO agent_conversation_owner_transcript_run_inputs_v1 (person_id, run_id, conversation_id, branch_id, input_sequence, input_message_id) VALUES (?, ?, ?, ?, ?, ?)",
            (
                person_id.to_string(),
                run_id.as_uuid().to_string(),
                mapping.input.conversation_id.as_uuid().to_string(),
                mapping.input.branch_id.as_uuid().to_string(),
                integer(mapping.input.sequence)?,
                mapping.input.message_id.as_uuid().to_string(),
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

pub(super) async fn insert_owner_input_binding_on(
    transaction: &Transaction<'_>,
    binding: &OwnerInputBinding,
) -> Result<(), ConversationStoreFailure> {
    binding.validate(binding.identity.person_id)?;
    let binding_json = encode(binding)?;
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_v3_owner_bindings (person_id, run_id, session_id, owner_user_message_id, conversation_id, branch_id, input_sequence, input_message_id, binding_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                binding.identity.person_id.to_string(),
                binding.run_id.as_uuid().to_string(),
                binding.session_id.to_string(),
                binding.owner_user_message_id.to_string(),
                binding.conversation_id.as_uuid().to_string(),
                binding.branch_id.as_uuid().to_string(),
                integer(binding.input.sequence)?,
                binding.input.message_id.as_uuid().to_string(),
                binding_json,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

pub(super) async fn typed_transcript_link_for_contribution_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    contribution_id: floe_conversation_contract::LogicalContributionId,
) -> Result<Option<TypedTranscriptEvidenceLink>, ConversationStoreFailure> {
    if !owner_custody_v1_present_on(transaction).await? {
        return Ok(None);
    }
    let mut rows = transaction
        .query(
            "SELECT conversation_id, branch_id, sequence, message_id, owner_input_conversation_id, owner_input_branch_id, owner_input_sequence, owner_input_message_id, session_id, typed_digest, typed_reference_json, transcript_entry_json, first_recording_run_id, original_task_receipt_json FROM agent_conversation_owner_transcript_evidence_v1 WHERE person_id = ? AND contribution_id = ? LIMIT 2",
            (
                person_id.to_string(),
                contribution_id.as_uuid().to_string(),
            ),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let conversation_id = parse_conversation_id(&row.get::<String>(0).map_err(|_| unavailable())?)?;
    let branch_id = parse_branch_id(&row.get::<String>(1).map_err(|_| unavailable())?)?;
    let sequence = positive_integer(row.get::<i64>(2).map_err(|_| unavailable())?)?;
    let message_id = parse_message_id(&row.get::<String>(3).map_err(|_| unavailable())?)?;
    let owner_input = TranscriptReference {
        conversation_id: parse_conversation_id(&row.get::<String>(4).map_err(|_| unavailable())?)?,
        branch_id: parse_branch_id(&row.get::<String>(5).map_err(|_| unavailable())?)?,
        sequence: positive_integer(row.get::<i64>(6).map_err(|_| unavailable())?)?,
        message_id: parse_message_id(&row.get::<String>(7).map_err(|_| unavailable())?)?,
    };
    let session_id = parse_uuid(&row.get::<String>(8).map_err(|_| unavailable())?)?;
    let typed_digest = parse_digest(&row.get::<String>(9).map_err(|_| unavailable())?)?;
    let typed_reference_json = row.get::<String>(10).map_err(|_| unavailable())?;
    let transcript_entry_json = row.get::<String>(11).map_err(|_| unavailable())?;
    let first_recording_run_id = parse_run_id(&row.get::<String>(12).map_err(|_| unavailable())?)?;
    let task_json = row.get::<Option<String>>(13).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);

    let typed_reference: TypedAgentMessageReference = decode(&typed_reference_json)?;
    if encode(&typed_reference)? != typed_reference_json
        || typed_reference.session_id() != session_id
        || typed_reference.digest() != typed_digest
    {
        return Err(unavailable());
    }
    let mapping = owner_transcript_input_for_reference_on(transaction, person_id, owner_input)
        .await?
        .ok_or_else(unavailable)?;
    let producer_mapping =
        owner_transcript_input_for_run_on(transaction, person_id, first_recording_run_id)
            .await?
            .ok_or_else(unavailable)?;
    if mapping.session_id != session_id
        || mapping.input.conversation_id != conversation_id
        || mapping.input.branch_id != branch_id
        || producer_mapping != mapping
    {
        return Err(unavailable());
    }
    let scope = Scope::from_identity(
        person_id,
        &mapping.original_binding.identity,
        conversation_id,
        branch_id,
    );
    let transcript_entry = entry_on(transaction, scope, sequence)
        .await?
        .ok_or_else(unavailable)?;
    if transcript_entry.reference.message_id != message_id
        || encode(&transcript_entry)? != transcript_entry_json
    {
        return Err(unavailable());
    }
    let original_task_receipt: Option<TaskExecutionReceiptRef> = task_json
        .as_deref()
        .map(decode::<TaskExecutionReceiptRef>)
        .transpose()?;
    let owner_turn_id = typed_reference.turn_id();
    let link = TypedTranscriptEvidenceLink {
        person_id,
        typed_reference,
        transcript_entry,
        owner_input,
        owner_session_id: session_id,
        owner_turn_id,
        contribution_id,
        first_recording_run_id,
        original_task_receipt,
    };
    link.validate(person_id)?;
    let recording_receipt = recording_receipt_on(transaction, person_id, contribution_id)
        .await?
        .ok_or_else(unavailable)?;
    let linked_task_reference = link
        .original_task_receipt
        .as_ref()
        .map(task_evidence_reference)
        .transpose()?;
    let linked_digest = recording_content_digest(
        &link.transcript_entry.message,
        contribution_id,
        linked_task_reference.as_ref(),
    )
    .map_err(ConversationStoreFailure::Transition)?;
    if recording_receipt.transcript != link.transcript_entry.reference
        || recording_receipt.recorder.run_id != link.first_recording_run_id
        || recording_receipt.producing_task != linked_task_reference
        || recording_receipt.content_digest != linked_digest
    {
        return Err(unavailable());
    }
    Ok(Some(link))
}

/// Bounded exact transcript-to-owner proof lookup for the later hydrated
/// reader. It performs no schema initialization and never searches by digest.
pub(super) async fn typed_transcript_link_for_entry_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    reference: TranscriptReference,
) -> Result<Option<TypedTranscriptEvidenceLink>, ConversationStoreFailure> {
    reference
        .validate()
        .map_err(ConversationStoreFailure::Transition)?;
    if !owner_custody_v1_present_on(transaction).await? {
        return Ok(None);
    }
    let mut rows = transaction
        .query(
            "SELECT contribution_id FROM agent_conversation_owner_transcript_evidence_v1 WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ? AND message_id = ? LIMIT 2",
            (
                person_id.to_string(),
                reference.conversation_id.as_uuid().to_string(),
                reference.branch_id.as_uuid().to_string(),
                integer(reference.sequence)?,
                reference.message_id.as_uuid().to_string(),
            ),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let contribution_id =
        parse_logical_contribution_id(&row.get::<String>(0).map_err(|_| unavailable())?)?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let link = typed_transcript_link_for_contribution_on(transaction, person_id, contribution_id)
        .await?
        .ok_or_else(unavailable)?;
    if link.transcript_entry.reference != reference {
        return Err(unavailable());
    }
    Ok(Some(link))
}

async fn insert_typed_transcript_link_on(
    transaction: &Transaction<'_>,
    link: &TypedTranscriptEvidenceLink,
) -> Result<(), ConversationStoreFailure> {
    link.validate(link.person_id)?;
    if !owner_custody_v1_present_on(transaction).await? {
        return Err(unavailable());
    }
    if typed_transcript_link_for_contribution_on(transaction, link.person_id, link.contribution_id)
        .await?
        .is_some()
        || typed_transcript_link_for_entry_on(
            transaction,
            link.person_id,
            link.transcript_entry.reference,
        )
        .await?
        .is_some()
    {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::RunAlreadyUsed,
        ));
    }
    transaction
        .execute(
            "INSERT INTO agent_conversation_owner_transcript_evidence_v1 (person_id, contribution_id, conversation_id, branch_id, sequence, message_id, owner_input_conversation_id, owner_input_branch_id, owner_input_sequence, owner_input_message_id, session_id, typed_digest, typed_reference_json, transcript_entry_json, first_recording_run_id, original_task_receipt_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                link.person_id.to_string(),
                link.contribution_id.as_uuid().to_string(),
                link.transcript_entry.reference.conversation_id.as_uuid().to_string(),
                link.transcript_entry.reference.branch_id.as_uuid().to_string(),
                integer(link.transcript_entry.reference.sequence)?,
                link.transcript_entry.reference.message_id.as_uuid().to_string(),
                link.owner_input.conversation_id.as_uuid().to_string(),
                link.owner_input.branch_id.as_uuid().to_string(),
                integer(link.owner_input.sequence)?,
                link.owner_input.message_id.as_uuid().to_string(),
                link.owner_session_id.to_string(),
                hex_digest(link.typed_reference.digest()),
                encode(&link.typed_reference)?,
                encode(&link.transcript_entry)?,
                link.first_recording_run_id.as_uuid().to_string(),
                link.original_task_receipt.as_ref().map(encode).transpose()?,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

fn typed_output_message(
    typed_message: &AgentMessage,
    identity: &AgentIdentity,
    message_id: MessageId,
    command_id: CommandId,
    evidence_digest: [u8; 32],
    producing_task: Option<&TaskEvidenceReference>,
) -> Result<ConversationMessage, ConversationStoreFailure> {
    let (origin, text, task_id) = match typed_message {
        AgentMessage::Assistant { text, .. } if producing_task.is_none() => (
            MessageOrigin::Agent {
                agent_instance_id: identity.agent_instance_id,
            },
            text.clone(),
            None,
        ),
        AgentMessage::Capability {
            capability_id,
            result,
            ..
        } if producing_task.is_none() => (
            MessageOrigin::Tool {
                agent_instance_id: identity.agent_instance_id,
                tool_id: capability_id.clone(),
            },
            result
                .as_ref()
                .cloned()
                .unwrap_or_else(|failure| format!("unavailable: {failure:?}")),
            None,
        ),
        AgentMessage::Delegation { task, .. }
            if producing_task.is_some_and(|reference| reference.task_id() == task.task_id) =>
        {
            (
                MessageOrigin::Host,
                task.result
                    .clone()
                    .unwrap_or_else(|| format!("{}: {:?}", task.agent_id, task.state)),
                Some(task.task_id),
            )
        }
        AgentMessage::Delegation { task, .. } if producing_task.is_none() => (
            MessageOrigin::Host,
            task.result
                .clone()
                .unwrap_or_else(|| format!("{}: {:?}", task.agent_id, task.state)),
            None,
        ),
        AgentMessage::Interaction { .. } if producing_task.is_none() => {
            (MessageOrigin::Host, String::new(), None)
        }
        // User, Preamble and Compaction are outside this generated terminal
        // output slice.
        _ => {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
    };
    let message = ConversationMessage {
        message_id,
        command_id,
        origin,
        text,
        evidence: Some(MessageEvidenceReference::from_digest(evidence_digest)),
        task_id,
    };
    message
        .validate()
        .map_err(ConversationStoreFailure::Transition)?;
    Ok(message)
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    async fn verified_typed_recorder_on(
        &self,
        transaction: &Transaction<'_>,
        recorder: &RecorderFence,
    ) -> Result<(RunRecord, OwnerTranscriptInputMapping), ConversationStoreFailure> {
        recorder
            .validate()
            .map_err(ConversationStoreFailure::Transition)?;
        if recorder.identity.person_id != self.person_id
            || recorder.executor_domain != ExecutorDomain::HostRun
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let (_, run, _) = self
            .verified_owner_evidence_on(
                transaction,
                recorder.run_id,
                &recorder.identity,
                recorder.conversation_id,
                recorder.branch_id,
                recorder.input,
                recorder.executor_domain,
                recorder.executor_generation,
            )
            .await?;
        let mapping =
            owner_transcript_input_for_run_on(transaction, self.person_id, recorder.run_id)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?;
        if mapping.input != recorder.input
            || mapping.session_id != run.session_id
            || mapping.owner_user_message_id != run.user_message_id
            || mapping.original_binding.identity != recorder.identity
            || mapping.input.conversation_id != recorder.conversation_id
            || mapping.input.branch_id != recorder.branch_id
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let open = open_receipt_on(transaction, self.person_id, recorder.run_id)
            .await?
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
        let scope = Scope::from_identity(
            self.person_id,
            &recorder.identity,
            recorder.conversation_id,
            recorder.branch_id,
        );
        let active = active_recorder_on(transaction, scope).await?;
        let current_generation = self
            .active_conversation_executor_generation(transaction)
            .await
            .map_err(owner_error)?;
        if open.fence != *recorder
            || active.as_ref() != Some(recorder)
            || current_generation != recorder.executor_generation
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::WrongWriter,
            ));
        }
        Ok((run, mapping))
    }

    async fn verified_typed_task_on(
        &self,
        transaction: &Transaction<'_>,
        producer_run_id: RunId,
        typed_message: &AgentMessage,
    ) -> Result<
        (
            Option<TaskExecutionReceiptRef>,
            Option<TaskEvidenceReference>,
        ),
        ConversationStoreFailure,
    > {
        match typed_message {
            AgentMessage::Delegation {
                task,
                execution_receipt: Some(execution_receipt),
                ..
            } => {
                execution_receipt.validate().map_err(|_| {
                    ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
                })?;
                let actual = self
                    .task_on(transaction, task.task_id)
                    .await
                    .map_err(owner_error)?
                    .ok_or(ConversationStoreFailure::Transition(
                        ConversationFailure::OwnerEvidenceMismatch,
                    ))?;
                if actual.snapshot != *task
                    || actual.snapshot.parent_run_id != Some(producer_run_id.as_uuid())
                    || actual.receipt.as_ref().map(|receipt| &receipt.reference)
                        != Some(execution_receipt)
                {
                    return Err(ConversationStoreFailure::Transition(
                        ConversationFailure::OwnerEvidenceMismatch,
                    ));
                }
                let reference = task_evidence_reference(execution_receipt)?;
                let verified = self
                    .verified_task_reference_on(transaction, producer_run_id, &reference)
                    .await?;
                if verified != reference {
                    return Err(ConversationStoreFailure::Transition(
                        ConversationFailure::OwnerEvidenceMismatch,
                    ));
                }
                Ok((Some(execution_receipt.clone()), Some(verified)))
            }
            AgentMessage::Delegation {
                task,
                execution_receipt: None,
                ..
            } => {
                self.verified_unadmitted_task_on(transaction, producer_run_id, task)
                    .await?;
                Ok((None, None))
            }
            AgentMessage::Assistant { .. } | AgentMessage::Capability { .. } => Ok((None, None)),
            AgentMessage::Interaction { .. } => Ok((None, None)),
            AgentMessage::User { .. }
            | AgentMessage::Preamble { .. }
            | AgentMessage::Compaction { .. } => Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            )),
        }
    }

    async fn verified_unadmitted_task_on(
        &self,
        transaction: &Transaction<'_>,
        producer_run_id: RunId,
        typed_snapshot: &floe_agent_contract::TaskSnapshot,
    ) -> Result<(), ConversationStoreFailure> {
        let open = open_receipt_on(transaction, self.person_id, producer_run_id)
            .await?
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
        let (_, owner_run, _) = self
            .verified_owner_evidence_on(
                transaction,
                producer_run_id,
                &open.fence.identity,
                open.fence.conversation_id,
                open.fence.branch_id,
                open.fence.input,
                open.fence.executor_domain,
                open.fence.executor_generation,
            )
            .await?;
        if !typed_snapshot.task_id.is_valid()
            || typed_snapshot.parent_run_id != Some(producer_run_id.as_uuid())
            || typed_snapshot.principal != self.person_id.to_string()
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        // An Unadmitted receipt is meaningful only while the Task owner has
        // no durable record for this identity. The exact Run journal pair
        // below preserves the owner's positive absence observation.
        if self
            .task_on(transaction, typed_snapshot.task_id)
            .await
            .map_err(owner_error)?
            .is_some()
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let journal = self
            .conversation_journal_on(transaction, &owner_run)
            .await
            .map_err(owner_error)?;
        let mut intent = None;
        let mut intent_revision = 0;
        let mut result: Option<Box<TaskReceipt>> = None;
        let mut result_revision = 0;
        for entry in journal {
            match entry.event {
                floe_agent_contract::JournalEvent::DelegationIntent { request }
                    if request.task_id == typed_snapshot.task_id =>
                {
                    if intent.replace(request).is_some() {
                        return Err(ConversationStoreFailure::Transition(
                            ConversationFailure::OwnerEvidenceMismatch,
                        ));
                    }
                    intent_revision = entry.revision;
                }
                floe_agent_contract::JournalEvent::DelegationResult { receipt }
                    if receipt.task_id == typed_snapshot.task_id =>
                {
                    if result.replace(receipt).is_some() {
                        return Err(ConversationStoreFailure::Transition(
                            ConversationFailure::OwnerEvidenceMismatch,
                        ));
                    }
                    result_revision = entry.revision;
                }
                _ => {}
            }
        }
        let request = intent.ok_or(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch,
        ))?;
        let receipt = result.ok_or(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch,
        ))?;
        if receipt
            .validate(floe_agent_contract::MAX_OUTPUT_BYTES)
            .is_err()
            || !matches!(&receipt.execution, TaskExecutionEvidence::Unadmitted)
            || receipt.snapshot != *typed_snapshot
            || receipt.task_id != typed_snapshot.task_id
            || intent_revision == 0
            || result_revision <= intent_revision
            || request.task_id != receipt.task_id
            || request.parent_run_id != Some(producer_run_id.as_uuid())
            || request.principal != self.person_id.to_string()
            || request.selected_agent_id != typed_snapshot.agent_id
            || request.selected_definition_revision != typed_snapshot.definition_revision
            || request.execution_context.session_id != owner_run.session_id
            || request.execution_context.device_id != owner_run.device_id
            || request.execution_context.validate().is_err()
            || !floe_agent_contract::valid_context_refs(&request.context_refs)
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        Ok(())
    }

    async fn verified_typed_interaction_on(
        &self,
        transaction: &Transaction<'_>,
        producer_run_id: RunId,
        typed_message: &AgentMessage,
    ) -> Result<(), ConversationStoreFailure> {
        let AgentMessage::Interaction {
            turn_id,
            interaction_id,
            interaction_kind,
        } = typed_message
        else {
            return Ok(());
        };
        let open = open_receipt_on(transaction, self.person_id, producer_run_id)
            .await?
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
        let (_, owner_run, _) = self
            .verified_owner_evidence_on(
                transaction,
                producer_run_id,
                &open.fence.identity,
                open.fence.conversation_id,
                open.fence.branch_id,
                open.fence.input,
                open.fence.executor_domain,
                open.fence.executor_generation,
            )
            .await?;
        // read_interaction validates the serialized row, its duplicated
        // indexed columns, and its owner audit/evidence links. The current
        // status is deliberately not copied into the transcript.
        let interaction = super::conversation_interactions::read_interaction(
            transaction,
            self.person_id,
            *interaction_id,
        )
        .await
        .map_err(owner_error)?
        .ok_or(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch,
        ))?;
        interaction.validate().map_err(owner_error)?;
        if interaction.id != *interaction_id
            || interaction.person_id != self.person_id
            || interaction.session_id != owner_run.session_id
            || interaction.origin_run_id != producer_run_id
            || interaction.origin_turn_id != *turn_id
            || interaction.kind != *interaction_kind
            || *turn_id != producer_run_id.as_uuid()
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        Ok(())
    }

    /// Compose owner-typed evidence, the neutral Core output receipt, and its
    /// immutable transcript link in the caller's existing owner transaction.
    /// The caller supplies typed content and independent message identities;
    /// the composer derives the evidence digest and Task reference itself.
    pub(super) async fn record_typed_conversation_entry_on(
        &self,
        transaction: &Transaction<'_>,
        request: TypedConversationRecordingRequest,
    ) -> Result<RecordingReceipt, ConversationStoreFailure> {
        ensure_core_v3_on(transaction).await?;
        let stored_receipt =
            recording_receipt_on(transaction, self.person_id, request.contribution_id).await?;
        let stored_link = typed_transcript_link_for_contribution_on(
            transaction,
            self.person_id,
            request.contribution_id,
        )
        .await?;
        match (stored_receipt, stored_link) {
            (Some(receipt), Some(link)) => {
                return self
                    .replay_typed_conversation_entry_on(transaction, request, receipt, link)
                    .await;
            }
            (Some(_), None) | (None, Some(_)) => {
                // Existing Core output without a typed proof is not silently
                // retrofitted into the live owner-linked path.
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
            (None, None) => {}
        }
        if request.typed_entry_id.is_nil()
            || request.recorder.identity.person_id != self.person_id
            || request.recorder.executor_domain != ExecutorDomain::HostRun
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let (run, mapping) = self
            .verified_typed_recorder_on(transaction, &request.recorder)
            .await?;
        if request.typed_message.turn_id() != request.recorder.run_id.as_uuid()
            || mapping.input != request.recorder.input
            || mapping.session_id != run.session_id
            || mapping.owner_user_message_id != run.user_message_id
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        if self
            .typed_agent_message_exists_on(transaction, run.session_id, request.typed_entry_id)
            .await
            .map_err(owner_error)?
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let (original_task_receipt, producing_task) = self
            .verified_typed_task_on(transaction, request.recorder.run_id, &request.typed_message)
            .await?;
        self.verified_typed_interaction_on(
            transaction,
            request.recorder.run_id,
            &request.typed_message,
        )
        .await?;

        // A unique immutable link family is required for this composed write.
        // It is never created by any read/open path.
        ensure_owner_custody_v1_on(transaction).await?;
        let typed_reference = self
            .insert_typed_agent_message_on(
                transaction,
                run.session_id,
                request.typed_entry_id,
                TypedAgentMessageProvenance::OwnerRecorded,
                &request.typed_message,
            )
            .await
            .map_err(owner_error)?;
        if typed_reference.turn_id() != run.run_id.as_uuid()
            || typed_reference.session_id() != run.session_id
            || typed_reference.provenance() != TypedAgentMessageProvenance::OwnerRecorded
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        #[cfg(test)]
        if self
            .conversation_core_typed_write_fault
            .compare_exchange(
                1,
                0,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
        {
            return Err(ConversationStoreFailure::NotCommitted);
        }
        let message = typed_output_message(
            &request.typed_message,
            &request.recorder.identity,
            request.message_id,
            request.command_id,
            typed_reference.digest(),
            producing_task.as_ref(),
        )?;
        let core_request = RecordingRequest {
            recorder: request.recorder.clone(),
            message,
            contribution_id: request.contribution_id,
            producing_task: producing_task.clone(),
        };
        let receipt = self
            .record_conversation_entry_on(transaction, core_request)
            .await?;
        if receipt.recorder != request.recorder
            || receipt.contribution_id != request.contribution_id
            || receipt.producing_task != producing_task
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        #[cfg(test)]
        if self
            .conversation_core_typed_write_fault
            .compare_exchange(
                2,
                0,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
        {
            return Err(ConversationStoreFailure::NotCommitted);
        }
        let transcript_entry = entry_on(
            transaction,
            Scope::from_identity(
                self.person_id,
                &request.recorder.identity,
                request.recorder.conversation_id,
                request.recorder.branch_id,
            ),
            receipt.transcript.sequence,
        )
        .await?
        .ok_or_else(unavailable)?;
        let link = TypedTranscriptEvidenceLink {
            person_id: self.person_id,
            typed_reference,
            transcript_entry,
            owner_input: mapping.input,
            owner_session_id: run.session_id,
            owner_turn_id: run.run_id.as_uuid(),
            contribution_id: request.contribution_id,
            first_recording_run_id: run.run_id,
            original_task_receipt,
        };
        link.validate(self.person_id)?;
        insert_typed_transcript_link_on(transaction, &link).await?;
        #[cfg(test)]
        if self
            .conversation_core_typed_write_fault
            .compare_exchange(
                3,
                0,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
        {
            return Err(ConversationStoreFailure::NotCommitted);
        }
        Ok(receipt)
    }

    async fn replay_typed_conversation_entry_on(
        &self,
        transaction: &Transaction<'_>,
        request: TypedConversationRecordingRequest,
        receipt: RecordingReceipt,
        link: TypedTranscriptEvidenceLink,
    ) -> Result<RecordingReceipt, ConversationStoreFailure> {
        if request.contribution_id != link.contribution_id
            || request.typed_entry_id != link.typed_reference.entry_id()
            || request.typed_message.turn_id() != link.owner_turn_id
            || request.recorder.identity.person_id != self.person_id
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let mapping =
            owner_transcript_input_for_reference_on(transaction, self.person_id, link.owner_input)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?;
        let resolved = self
            .resolve_typed_agent_message_on(
                transaction,
                &link.typed_reference,
                floe_conversation::MAX_TYPED_AGENT_MESSAGE_ENVELOPE_BYTES,
            )
            .await
            .map_err(owner_error)?;
        if resolved.reference != link.typed_reference || resolved.message != request.typed_message {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::MessageIdConflict,
            ));
        }
        if mapping.session_id != link.owner_session_id
            || mapping.original_binding.identity != request.recorder.identity
            || mapping.input != link.owner_input
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let producing_task = link
            .original_task_receipt
            .as_ref()
            .map(task_evidence_reference)
            .transpose()?;
        self.verified_typed_interaction_on(
            transaction,
            link.first_recording_run_id,
            &request.typed_message,
        )
        .await?;
        let message = typed_output_message(
            &request.typed_message,
            &mapping.original_binding.identity,
            request.message_id,
            request.command_id,
            link.typed_reference.digest(),
            producing_task.as_ref(),
        )?;
        if message != link.transcript_entry.message
            || link.owner_session_id != mapping.session_id
            || link.owner_turn_id != link.typed_reference.turn_id()
            || link.first_recording_run_id.as_uuid() != link.owner_turn_id
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::MessageIdConflict,
            ));
        }
        if request.recorder != receipt.recorder {
            let (current_run, current_mapping) = self
                .verified_typed_recorder_on(transaction, &request.recorder)
                .await?;
            if current_mapping.input != link.owner_input
                || current_mapping.session_id != link.owner_session_id
                || current_mapping.owner_user_message_id != mapping.owner_user_message_id
                || current_run.session_id != link.owner_session_id
                || current_run.user_message_id != mapping.owner_user_message_id
                || request.recorder.identity != mapping.original_binding.identity
            {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
            if matches!(request.typed_message, AgentMessage::Delegation { .. }) {
                let (original_task_receipt, verified_task) = self
                    .verified_typed_task_on(
                        transaction,
                        link.first_recording_run_id,
                        &request.typed_message,
                    )
                    .await?;
                if original_task_receipt != link.original_task_receipt
                    || verified_task != producing_task
                {
                    return Err(ConversationStoreFailure::Transition(
                        ConversationFailure::OwnerEvidenceMismatch,
                    ));
                }
            }
        }
        let replay_request = RecordingRequest {
            recorder: receipt.recorder.clone(),
            message,
            contribution_id: request.contribution_id,
            producing_task,
        };
        replay_recording_entry(replay_request, receipt)
            .map(|transition| transition.receipt)
            .map_err(ConversationStoreFailure::Transition)
    }
}
