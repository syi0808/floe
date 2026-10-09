//! Vault persistence for owner-driven neutral Conversation recorder custody.
//!
//! Core 3 is the only supported stored meaning for neutral recorder custody.
//! The internal `*_on` methods accept the caller's transaction so an
//! owner admission, journal, terminal write, and the matching Core receipt can
//! be composed atomically by a later product cutover.

use std::collections::HashSet;

use super::owner_custody::{
    OwnerInputBinding, OwnerTranscriptInputMapping, ensure_owner_custody_v1_on,
    insert_owner_input_binding_on, insert_owner_transcript_input_mapping_on,
    insert_owner_transcript_run_input_on, owner_input_binding_on,
    owner_transcript_input_for_owner_message_on, owner_transcript_input_for_reference_on,
    owner_transcript_input_for_run_on,
};
#[cfg(test)]
use super::owner_custody::{
    TypedConversationRecordingRequest, TypedTranscriptEvidenceLink,
    typed_transcript_link_for_contribution_on, typed_transcript_link_for_entry_on,
};
use super::{EncryptedAgentVault, VaultKeyProvider, database_failure};
use crate::write_fence::JournalWriteGuard;
use floe_agent_contract::TaskExecutionReceiptRef;
#[cfg(test)]
use floe_conversation::TypedAgentMessageProvenance;
use floe_conversation::{
    CanonicalTurnIntent, ResumeChildAdmission, RunRecord, RunState, TurnAdmissionRequest,
    TurnInput, TurnMode,
};
use floe_conversation_contract::{
    AdmissionDisposition, AdmissionResult, AdmissionTarget, AgentIdentity, ConversationBranchId,
    ConversationCheckpoint, ConversationFailure, ConversationId, ConversationMessage,
    ConversationReference, MessageAdmissionRequest, MessageId, TaskEvidenceReference,
    TranscriptReference,
};
use floe_conversation_core::{
    AppendFacts, CheckpointFacts, ConversationHead, ConversationStoreFailure, EMPTY_PREFIX_DIGEST,
    ExecutorDomain, MAX_TRANSCRIPT_PAGE_BYTES, MAX_TRANSCRIPT_PAGE_ENTRIES, OwnerGenerationFence,
    OwnerRunEvidence, OwnerRunState, OwnerSettlementEvidence, RecorderCloseFacts,
    RecorderCloseReceipt, RecorderFence, RecorderOpenFacts, RecorderOpenReceipt,
    RecorderRecoveryObservation, RecorderRecoveryState, RecorderRetirementFacts,
    RecorderRetirementReceipt, RecorderStartRequest, RecordingFacts, RecordingReceipt,
    RecordingRequest, TranscriptEntry, TranscriptEntryKind, TranscriptPage, TranscriptPageBudget,
    advance_core_prefix_digest, append_input_transition, apply_checkpoint_transition,
    close_recording_transition, open_recording_transition, record_entry_transition,
    recording_content_digest, replay_close_recording, replay_open_recording,
    replay_recording_entry, retire_stale_recording_transition, validate_reference_target,
};
use floe_kernel::{AgentFailure, PersonId, RunId};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

const MAX_STORED_MESSAGE_BYTES: usize = 132_096;

#[derive(Clone, Debug)]
pub(super) enum CoreComposedOwnerIntent {
    Turn(TurnAdmissionRequest),
    Resume(ResumeChildAdmission),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CoreComposedRunAdmission {
    Admitted {
        record: RunRecord,
        input: AdmissionResult,
        recorder: RecorderOpenReceipt,
    },
    /// The shared owner transition committed a stale request as superseded.
    /// The enclosing transaction must commit this outcome; report Conflict
    /// only after that commit succeeds.
    ResumeSuperseded,
}

#[derive(Clone, Debug)]
pub(super) struct LoadedHead {
    pub(super) state: ConversationHead,
    pub(super) identity_json: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Scope {
    pub(super) person_id: PersonId,
    pub(super) conversation_id: ConversationId,
    pub(super) branch_id: ConversationBranchId,
}

impl Scope {
    pub(super) fn from_identity(
        person_id: PersonId,
        _identity: &AgentIdentity,
        conversation_id: ConversationId,
        branch_id: ConversationBranchId,
    ) -> Self {
        Self {
            person_id,
            conversation_id,
            branch_id,
        }
    }

    pub(super) fn sql(self) -> (String, String, String) {
        (
            self.person_id.to_string(),
            self.conversation_id.as_uuid().to_string(),
            self.branch_id.as_uuid().to_string(),
        )
    }

    fn transcript_reference(self, message_id: MessageId, sequence: u64) -> TranscriptReference {
        TranscriptReference {
            conversation_id: self.conversation_id,
            branch_id: self.branch_id,
            message_id,
            sequence,
        }
    }
}

pub(super) fn unavailable() -> ConversationStoreFailure {
    ConversationStoreFailure::Unavailable
}

pub(super) fn database_error(error: turso::Error) -> ConversationStoreFailure {
    if database_failure(error) == AgentFailure::StorageBusy {
        ConversationStoreFailure::Busy
    } else {
        unavailable()
    }
}

pub(super) fn start_error(error: AgentFailure) -> ConversationStoreFailure {
    if error == AgentFailure::StorageBusy {
        ConversationStoreFailure::Busy
    } else {
        unavailable()
    }
}

pub(super) fn schema_error(error: crate::schema::SchemaFailure) -> ConversationStoreFailure {
    match error {
        crate::schema::SchemaFailure::Unsupported { .. } => {
            ConversationStoreFailure::UnsupportedStoredMeaning
        }
        crate::schema::SchemaFailure::Busy => ConversationStoreFailure::Busy,
        _ => unavailable(),
    }
}

pub(super) fn owner_error(error: AgentFailure) -> ConversationStoreFailure {
    match error {
        AgentFailure::StorageBusy => ConversationStoreFailure::Busy,
        AgentFailure::UnsupportedVersion => ConversationStoreFailure::UnsupportedStoredMeaning,
        AgentFailure::Conflict | AgentFailure::NotFound | AgentFailure::CapabilityDenied => {
            ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
        }
        _ => unavailable(),
    }
}

pub(super) fn encode<T: Serialize>(value: &T) -> Result<String, ConversationStoreFailure> {
    serde_json::to_string(value).map_err(|_| unavailable())
}

pub(super) fn decode<T: DeserializeOwned>(value: &str) -> Result<T, ConversationStoreFailure> {
    serde_json::from_str(value).map_err(|_| unavailable())
}

pub(super) fn integer(value: u64) -> Result<i64, ConversationStoreFailure> {
    i64::try_from(value)
        .map_err(|_| ConversationStoreFailure::Transition(ConversationFailure::InvalidInput))
}

fn nonnegative_integer(value: i64) -> Result<u64, ConversationStoreFailure> {
    u64::try_from(value).map_err(|_| unavailable())
}

pub(super) fn positive_integer(value: i64) -> Result<u64, ConversationStoreFailure> {
    let value = u64::try_from(value).map_err(|_| unavailable())?;
    (value > 0).then_some(value).ok_or_else(unavailable)
}

pub(super) fn parse_uuid(value: &str) -> Result<Uuid, ConversationStoreFailure> {
    Uuid::parse_str(value).map_err(|_| unavailable())
}

pub(super) fn parse_message_id(value: &str) -> Result<MessageId, ConversationStoreFailure> {
    MessageId::from_uuid(parse_uuid(value)?).ok_or_else(unavailable)
}

pub(super) fn parse_run_id(value: &str) -> Result<RunId, ConversationStoreFailure> {
    RunId::from_uuid(parse_uuid(value)?).ok_or_else(unavailable)
}

pub(super) fn parse_conversation_id(
    value: &str,
) -> Result<ConversationId, ConversationStoreFailure> {
    ConversationId::from_uuid(parse_uuid(value)?).ok_or_else(unavailable)
}

pub(super) fn parse_branch_id(
    value: &str,
) -> Result<ConversationBranchId, ConversationStoreFailure> {
    ConversationBranchId::from_uuid(parse_uuid(value)?).ok_or_else(unavailable)
}

pub(super) fn hex_digest(value: [u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in value {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

pub(super) fn parse_digest(value: &str) -> Result<[u8; 32], ConversationStoreFailure> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(unavailable());
    }
    let mut digest = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or_else(unavailable)?;
        let low = hex_nibble(pair[1]).ok_or_else(unavailable)?;
        digest[index] = (high << 4) | low;
    }
    Ok(digest)
}

pub(super) fn task_evidence_reference(
    reference: &TaskExecutionReceiptRef,
) -> Result<TaskEvidenceReference, ConversationStoreFailure> {
    reference.validate().map_err(|_| {
        ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
    })?;
    let encoded = serde_json::to_vec(&("floe-conversation-task-evidence-reference-v1", reference))
        .map_err(|_| unavailable())?;
    Ok(TaskEvidenceReference::from_digest(
        reference.execution.task_id,
        Sha256::digest(encoded).into(),
    ))
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn map_transition<T>(
    result: Result<T, ConversationFailure>,
) -> Result<T, ConversationStoreFailure> {
    result.map_err(ConversationStoreFailure::Transition)
}

fn request_scope(
    person_id: PersonId,
    request: &MessageAdmissionRequest,
) -> Result<(Scope, AgentIdentity, bool), ConversationStoreFailure> {
    let (identity, conversation_id, branch_id, is_new) = match &request.target {
        floe_conversation_contract::AdmissionTarget::New {
            identity,
            conversation_id,
            branch_id,
        } => (identity.clone(), *conversation_id, *branch_id, true),
        floe_conversation_contract::AdmissionTarget::AppendToExisting { reference } => (
            reference.identity.clone(),
            reference.conversation_id,
            reference.branch_id,
            false,
        ),
    };
    if identity.person_id != person_id {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::AgentMismatch,
        ));
    }
    Ok((
        Scope::from_identity(person_id, &identity, conversation_id, branch_id),
        identity,
        is_new,
    ))
}

pub(super) async fn ensure_core_v3_on(
    transaction: &Transaction<'_>,
) -> Result<(), ConversationStoreFailure> {
    crate::schema::ensure_conversation_core_v3_family(transaction)
        .await
        .map_err(schema_error)
}

pub(super) async fn require_core_v3_on(
    transaction: &Transaction<'_>,
) -> Result<(), ConversationStoreFailure> {
    match crate::schema::conversation_core_family_version(transaction)
        .await
        .map_err(schema_error)?
    {
        crate::schema::ConversationCoreFamilyVersion::RecorderV3 => Ok(()),
        crate::schema::ConversationCoreFamilyVersion::Absent => Err(unavailable()),
    }
}

pub(super) async fn load_head_on(
    transaction: &Transaction<'_>,
    scope: Scope,
) -> Result<Option<LoadedHead>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT identity_json, head_revision, state_revision, settled_prefix, recorder_epoch FROM agent_conversation_core_v3_heads WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
            (person, conversation, branch),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let identity_json = row.get::<String>(0).map_err(|_| unavailable())?;
    let head_revision = nonnegative_integer(row.get::<i64>(1).map_err(|_| unavailable())?)?;
    let state_revision = nonnegative_integer(row.get::<i64>(2).map_err(|_| unavailable())?)?;
    let settled_prefix = nonnegative_integer(row.get::<i64>(3).map_err(|_| unavailable())?)?;
    let recorder_epoch = nonnegative_integer(row.get::<i64>(4).map_err(|_| unavailable())?)?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let identity: AgentIdentity = decode(&identity_json)?;
    identity.validate().map_err(|_| unavailable())?;
    if encode(&identity)? != identity_json
        || identity.person_id != scope.person_id
        || settled_prefix > head_revision
        || head_revision > 4096
        || state_revision < head_revision
    {
        return Err(unavailable());
    }
    Ok(Some(LoadedHead {
        state: ConversationHead {
            identity,
            conversation_id: scope.conversation_id,
            branch_id: scope.branch_id,
            head_revision,
            settled_prefix,
            state_revision,
            recorder_epoch,
        },
        identity_json,
    }))
}

async fn save_head_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    previous: Option<&LoadedHead>,
    next: &ConversationHead,
) -> Result<(), ConversationStoreFailure> {
    let next_identity_json = encode(&next.identity)?;
    if let Some(previous) = previous {
        let changed = transaction
            .execute(
                "UPDATE agent_conversation_core_v3_heads SET head_revision = ?, state_revision = ?, settled_prefix = ?, recorder_epoch = ? WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND identity_json = ? AND head_revision = ? AND state_revision = ? AND settled_prefix = ? AND recorder_epoch = ?",
                (
                    integer(next.head_revision)?,
                    integer(next.state_revision)?,
                    integer(next.settled_prefix)?,
                    integer(next.recorder_epoch)?,
                    scope.person_id.to_string(),
                    scope.conversation_id.as_uuid().to_string(),
                    scope.branch_id.as_uuid().to_string(),
                    previous.identity_json.clone(),
                    integer(previous.state.head_revision)?,
                    integer(previous.state.state_revision)?,
                    integer(previous.state.settled_prefix)?,
                    integer(previous.state.recorder_epoch)?,
                ),
            )
            .await
            .map_err(database_error)?;
        if changed != 1 {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::RevisionConflict,
            ));
        }
    } else {
        transaction
            .execute(
                "INSERT INTO agent_conversation_core_v3_heads (person_id, conversation_id, branch_id, identity_json, head_revision, state_revision, settled_prefix, recorder_epoch) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    scope.person_id.to_string(),
                    scope.conversation_id.as_uuid().to_string(),
                    scope.branch_id.as_uuid().to_string(),
                    next_identity_json,
                    integer(next.head_revision)?,
                    integer(next.state_revision)?,
                    integer(next.settled_prefix)?,
                    integer(next.recorder_epoch)?,
                ),
            )
            .await
            .map_err(database_error)?;
    }
    Ok(())
}

async fn raw_entry_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    sequence: u64,
) -> Result<Option<TranscriptEntry>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT message_id, message_json, message_bytes, entry_kind, producer_run_id, contribution_id, producing_task_json, prefix_digest FROM agent_conversation_core_v3_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ?",
            (person, conversation, branch, integer(sequence)?),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let message_id_text = row.get::<String>(0).map_err(|_| unavailable())?;
    let payload = row.get::<String>(1).map_err(|_| unavailable())?;
    let stored_bytes = row.get::<i64>(2).map_err(|_| unavailable())?;
    let kind_text = row.get::<String>(3).map_err(|_| unavailable())?;
    let producer_run_text = row.get::<Option<String>>(4).map_err(|_| unavailable())?;
    let contribution_text = row.get::<Option<String>>(5).map_err(|_| unavailable())?;
    let task_json = row.get::<Option<String>>(6).map_err(|_| unavailable())?;
    let prefix_digest_text = row.get::<String>(7).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);

    let actual_bytes = payload.as_bytes().len();
    if actual_bytes == 0
        || actual_bytes > MAX_STORED_MESSAGE_BYTES
        || usize::try_from(stored_bytes).ok() != Some(actual_bytes)
    {
        return Err(unavailable());
    }
    let message: ConversationMessage = decode(&payload)?;
    message.validate().map_err(|_| unavailable())?;
    if encode(&message)? != payload {
        return Err(unavailable());
    }
    if matches!(
        &message.origin,
        floe_conversation_contract::MessageOrigin::Person { person_id }
            if *person_id != scope.person_id
    ) {
        return Err(unavailable());
    }
    let message_id = parse_message_id(&message_id_text)?;
    if message.message_id != message_id {
        return Err(unavailable());
    }
    let kind = match kind_text.as_str() {
        "inbound" => TranscriptEntryKind::Inbound,
        "generated_output" => TranscriptEntryKind::GeneratedOutput,
        _ => return Err(unavailable()),
    };
    let producer_run = producer_run_text.as_deref().map(parse_run_id).transpose()?;
    let contribution_id = contribution_text
        .as_deref()
        .map(parse_logical_contribution_id)
        .transpose()?;
    let producing_task = task_json
        .as_deref()
        .map(decode::<TaskEvidenceReference>)
        .transpose()?;
    if let Some(reference) = producing_task.as_ref() {
        reference.validate().map_err(|_| unavailable())?;
    }
    if task_json.as_deref().is_some_and(|json| {
        producing_task
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok())
            .as_deref()
            != Some(json)
    }) {
        return Err(unavailable());
    }
    let kind_is_valid = match kind {
        TranscriptEntryKind::Inbound => {
            producer_run.is_none() && contribution_id.is_none() && producing_task.is_none()
        }
        TranscriptEntryKind::GeneratedOutput => producer_run.is_some() && contribution_id.is_some(),
    };
    if !kind_is_valid {
        return Err(unavailable());
    }
    Ok(Some(TranscriptEntry {
        reference: scope.transcript_reference(message_id, sequence),
        message,
        kind,
        producer_run,
        contribution_id,
        producing_task,
        prefix_digest: parse_digest(&prefix_digest_text)?,
    }))
}

pub(super) async fn entry_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    sequence: u64,
) -> Result<Option<TranscriptEntry>, ConversationStoreFailure> {
    let Some(entry) = raw_entry_on(transaction, scope, sequence).await? else {
        return Ok(None);
    };
    let previous = if sequence == 1 {
        EMPTY_PREFIX_DIGEST
    } else {
        let (person, conversation, branch) = scope.sql();
        let mut rows = transaction
            .query(
                "SELECT prefix_digest FROM agent_conversation_core_v3_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ?",
                (person, conversation, branch, integer(sequence - 1)?),
            )
            .await
            .map_err(database_error)?;
        let row = rows
            .next()
            .await
            .map_err(database_error)?
            .ok_or_else(unavailable)?;
        let digest = parse_digest(&row.get::<String>(0).map_err(|_| unavailable())?)?;
        if rows.next().await.map_err(database_error)?.is_some() {
            return Err(unavailable());
        }
        digest
    };
    let calculated = advance_core_prefix_digest(
        previous,
        entry.reference,
        &entry.message,
        entry.kind,
        entry.producer_run,
        entry.contribution_id,
        entry.producing_task.as_ref(),
    )
    .map_err(|_| unavailable())?;
    if calculated != entry.prefix_digest {
        return Err(unavailable());
    }
    match entry.kind {
        TranscriptEntryKind::Inbound => {
            validate_input_receipt_on(transaction, scope, &entry).await?
        }
        TranscriptEntryKind::GeneratedOutput => {
            validate_output_receipt_on(transaction, scope, &entry).await?
        }
    }
    Ok(Some(entry))
}

async fn validate_input_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    entry: &TranscriptEntry,
) -> Result<(), ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT sequence, receipt_json FROM agent_conversation_core_v3_input_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND message_id = ?",
            (person, conversation, branch, entry.reference.message_id.as_uuid().to_string()),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Err(unavailable());
    };
    let sequence = positive_integer(row.get::<i64>(0).map_err(|_| unavailable())?)?;
    let receipt_json = row.get::<String>(1).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let receipt: AdmissionResult = decode(&receipt_json)?;
    if encode(&receipt)? != receipt_json
        || sequence != entry.reference.sequence
        || receipt.receipt.transcript != entry.reference
        || receipt.receipt.head_revision != entry.reference.sequence
        || receipt.receipt.task_id != entry.message.task_id
    {
        return Err(unavailable());
    }
    Ok(())
}

async fn validate_output_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    entry: &TranscriptEntry,
) -> Result<(), ConversationStoreFailure> {
    let contribution_id = entry.contribution_id.ok_or_else(unavailable)?;
    let receipt = recording_receipt_on(transaction, scope.person_id, contribution_id)
        .await?
        .ok_or_else(unavailable)?;
    let expected_digest = recording_content_digest(
        &entry.message,
        contribution_id,
        entry.producing_task.as_ref(),
    )
    .map_err(|_| unavailable())?;
    if receipt.transcript != entry.reference
        || receipt.recorder.run_id != entry.producer_run.ok_or_else(unavailable)?
        || receipt.contribution_id != contribution_id
        || receipt.producing_task != entry.producing_task
        || receipt.content_digest != expected_digest
    {
        return Err(unavailable());
    }
    Ok(())
}

async fn input_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    message_id: MessageId,
) -> Result<Option<(AdmissionResult, ConversationMessage)>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT sequence, receipt_json FROM agent_conversation_core_v3_input_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND message_id = ?",
            (person, conversation, branch, message_id.as_uuid().to_string()),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let sequence = positive_integer(row.get::<i64>(0).map_err(|_| unavailable())?)?;
    let receipt_json = row.get::<String>(1).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let receipt: AdmissionResult = decode(&receipt_json)?;
    let entry = entry_on(transaction, scope, sequence)
        .await?
        .ok_or_else(unavailable)?;
    if encode(&receipt)? != receipt_json
        || receipt.receipt.transcript != entry.reference
        || entry.reference.message_id != message_id
        || entry.kind != TranscriptEntryKind::Inbound
    {
        return Err(unavailable());
    }
    Ok(Some((receipt, entry.message)))
}

async fn transcript_has_message_id_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    message_id: MessageId,
) -> Result<bool, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT 1 FROM agent_conversation_core_v3_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND message_id = ? LIMIT 1",
            (person, conversation, branch, message_id.as_uuid().to_string()),
        )
        .await
        .map_err(database_error)?;
    Ok(rows.next().await.map_err(database_error)?.is_some())
}

async fn tail_prefix_digest_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    head: &ConversationHead,
) -> Result<[u8; 32], ConversationStoreFailure> {
    if head.head_revision == 0 {
        return Ok(EMPTY_PREFIX_DIGEST);
    }
    entry_on(transaction, scope, head.head_revision)
        .await?
        .map(|entry| entry.prefix_digest)
        .ok_or_else(unavailable)
}

async fn insert_entry_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    entry: &TranscriptEntry,
) -> Result<(), ConversationStoreFailure> {
    let payload = encode(&entry.message)?;
    if payload.is_empty() || payload.len() > MAX_STORED_MESSAGE_BYTES {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::InvalidInput,
        ));
    }
    let kind = match entry.kind {
        TranscriptEntryKind::Inbound => "inbound",
        TranscriptEntryKind::GeneratedOutput => "generated_output",
    };
    let producer_run = entry.producer_run.map(|value| value.as_uuid().to_string());
    let contribution = entry
        .contribution_id
        .map(|value| value.as_uuid().to_string());
    let task_json = entry.producing_task.as_ref().map(encode).transpose()?;
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_v3_entries (person_id, conversation_id, branch_id, sequence, message_id, message_json, message_bytes, entry_kind, producer_run_id, contribution_id, producing_task_json, prefix_digest) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                scope.person_id.to_string(),
                scope.conversation_id.as_uuid().to_string(),
                scope.branch_id.as_uuid().to_string(),
                integer(entry.reference.sequence)?,
                entry.reference.message_id.as_uuid().to_string(),
                payload.clone(),
                i64::try_from(payload.len()).map_err(|_| unavailable())?,
                kind,
                producer_run,
                contribution,
                task_json,
                hex_digest(entry.prefix_digest),
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn insert_input_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    result: &AdmissionResult,
) -> Result<(), ConversationStoreFailure> {
    let receipt_json = encode(result)?;
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_v3_input_receipts (person_id, conversation_id, branch_id, message_id, sequence, receipt_json) VALUES (?, ?, ?, ?, ?, ?)",
            (
                scope.person_id.to_string(),
                scope.conversation_id.as_uuid().to_string(),
                scope.branch_id.as_uuid().to_string(),
                result.receipt.transcript.message_id.as_uuid().to_string(),
                integer(result.receipt.transcript.sequence)?,
                receipt_json,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

pub(super) async fn open_receipt_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    run_id: RunId,
) -> Result<Option<RecorderOpenReceipt>, ConversationStoreFailure> {
    let mut rows = transaction
        .query(
            "SELECT conversation_id, branch_id, receipt_json FROM agent_conversation_core_v3_open_receipts WHERE person_id = ? AND run_id = ?",
            (person_id.to_string(), run_id.as_uuid().to_string()),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let conversation_id = parse_conversation_id(&row.get::<String>(0).map_err(|_| unavailable())?)?;
    let branch_id = parse_branch_id(&row.get::<String>(1).map_err(|_| unavailable())?)?;
    let receipt_json = row.get::<String>(2).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let receipt: RecorderOpenReceipt = decode(&receipt_json)?;
    if encode(&receipt)? != receipt_json
        || receipt.fence.run_id != run_id
        || receipt.fence.identity.person_id != person_id
        || receipt.fence.conversation_id != conversation_id
        || receipt.fence.branch_id != branch_id
        || receipt.fence.validate().is_err()
    {
        return Err(unavailable());
    }
    Ok(Some(receipt))
}

pub(super) async fn active_recorder_on(
    transaction: &Transaction<'_>,
    scope: Scope,
) -> Result<Option<RecorderFence>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT run_id, fence_json FROM agent_conversation_core_v3_active_recorders WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
            (person, conversation, branch),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let run_id = parse_run_id(&row.get::<String>(0).map_err(|_| unavailable())?)?;
    let fence_json = row.get::<String>(1).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let fence: RecorderFence = decode(&fence_json)?;
    if encode(&fence)? != fence_json
        || fence.run_id != run_id
        || fence.conversation_id != scope.conversation_id
        || fence.branch_id != scope.branch_id
        || fence.identity.person_id != scope.person_id
        || fence.validate().is_err()
    {
        return Err(unavailable());
    }
    Ok(Some(fence))
}

async fn close_receipt_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    run_id: RunId,
) -> Result<Option<RecorderCloseReceipt>, ConversationStoreFailure> {
    let mut rows = transaction
        .query(
            "SELECT conversation_id, branch_id, receipt_json FROM agent_conversation_core_v3_close_receipts WHERE person_id = ? AND run_id = ?",
            (person_id.to_string(), run_id.as_uuid().to_string()),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let conversation_id = parse_conversation_id(&row.get::<String>(0).map_err(|_| unavailable())?)?;
    let branch_id = parse_branch_id(&row.get::<String>(1).map_err(|_| unavailable())?)?;
    let receipt_json = row.get::<String>(2).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let receipt: RecorderCloseReceipt = decode(&receipt_json)?;
    if encode(&receipt)? != receipt_json
        || receipt.fence.run_id != run_id
        || receipt.fence.identity.person_id != person_id
        || receipt.fence.conversation_id != conversation_id
        || receipt.fence.branch_id != branch_id
        || receipt.fence.validate().is_err()
        || receipt.protection_roots.len() > 512
    {
        return Err(unavailable());
    }
    Ok(Some(receipt))
}

async fn retirement_receipt_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    run_id: RunId,
) -> Result<Option<RecorderRetirementReceipt>, ConversationStoreFailure> {
    let mut rows = transaction
        .query(
            "SELECT conversation_id, branch_id, receipt_json FROM agent_conversation_core_v3_retirement_receipts WHERE person_id = ? AND run_id = ?",
            (person_id.to_string(), run_id.as_uuid().to_string()),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let conversation_id = parse_conversation_id(&row.get::<String>(0).map_err(|_| unavailable())?)?;
    let branch_id = parse_branch_id(&row.get::<String>(1).map_err(|_| unavailable())?)?;
    let receipt_json = row.get::<String>(2).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let receipt: RecorderRetirementReceipt = decode(&receipt_json)?;
    if encode(&receipt)? != receipt_json
        || receipt.fence.run_id != run_id
        || receipt.fence.identity.person_id != person_id
        || receipt.fence.conversation_id != conversation_id
        || receipt.fence.branch_id != branch_id
        || receipt.fence.validate().is_err()
        || receipt.owner_evidence_digest == [0; 32]
        || receipt.generation_fence_digest == [0; 32]
    {
        return Err(unavailable());
    }
    Ok(Some(receipt))
}

pub(super) async fn recording_receipt_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    contribution_id: floe_conversation_contract::LogicalContributionId,
) -> Result<Option<RecordingReceipt>, ConversationStoreFailure> {
    let mut rows = transaction
        .query(
            "SELECT r.conversation_id, r.branch_id, r.run_id, r.sequence, r.message_id, r.receipt_json, o.agent_instance_id, o.message_id, o.conversation_id, o.branch_id, o.producer_run_id, o.sequence, o.content_digest, o.producing_task_json, o.receipt_json FROM agent_conversation_core_v3_recording_receipts r LEFT JOIN agent_conversation_core_output_receipts_v2 o ON o.person_id = r.person_id AND o.contribution_id = r.contribution_id WHERE r.person_id = ? AND r.contribution_id = ?",
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
    let run_id = parse_run_id(&row.get::<String>(2).map_err(|_| unavailable())?)?;
    let sequence = positive_integer(row.get::<i64>(3).map_err(|_| unavailable())?)?;
    let message_id = parse_message_id(&row.get::<String>(4).map_err(|_| unavailable())?)?;
    let receipt_json = row.get::<String>(5).map_err(|_| unavailable())?;
    let output_agent_id = row
        .get::<Option<String>>(6)
        .map_err(|_| unavailable())?
        .as_deref()
        .map(parse_uuid)
        .transpose()?;
    let output_message_id = row
        .get::<Option<String>>(7)
        .map_err(|_| unavailable())?
        .as_deref()
        .map(parse_message_id)
        .transpose()?;
    let output_conversation_id = row
        .get::<Option<String>>(8)
        .map_err(|_| unavailable())?
        .as_deref()
        .map(parse_conversation_id)
        .transpose()?;
    let output_branch_id = row
        .get::<Option<String>>(9)
        .map_err(|_| unavailable())?
        .as_deref()
        .map(parse_branch_id)
        .transpose()?;
    let output_run_id = row
        .get::<Option<String>>(10)
        .map_err(|_| unavailable())?
        .as_deref()
        .map(parse_run_id)
        .transpose()?;
    let output_sequence = row
        .get::<Option<i64>>(11)
        .map_err(|_| unavailable())?
        .map(positive_integer)
        .transpose()?;
    let output_digest = row
        .get::<Option<String>>(12)
        .map_err(|_| unavailable())?
        .as_deref()
        .map(parse_digest)
        .transpose()?;
    let output_task_json = row.get::<Option<String>>(13).map_err(|_| unavailable())?;
    let output_receipt_json = row.get::<Option<String>>(14).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let receipt: RecordingReceipt = decode(&receipt_json)?;
    if encode(&receipt)? != receipt_json {
        return Err(unavailable());
    }
    let task_json = receipt.producing_task.as_ref().map(encode).transpose()?;
    if receipt.recorder.run_id != run_id
        || receipt.recorder.identity.person_id != person_id
        || receipt.recorder.conversation_id != conversation_id
        || receipt.recorder.branch_id != branch_id
        || receipt.transcript.sequence != sequence
        || receipt.transcript.message_id != message_id
        || receipt.contribution_id != contribution_id
        || output_agent_id != Some(receipt.recorder.identity.agent_instance_id.as_uuid())
        || output_message_id != Some(message_id)
        || output_conversation_id != Some(conversation_id)
        || output_branch_id != Some(branch_id)
        || output_run_id != Some(run_id)
        || output_sequence != Some(sequence)
        || output_digest != Some(receipt.content_digest)
        || output_task_json != task_json
        || output_receipt_json.as_deref() != Some(receipt_json.as_str())
    {
        return Err(unavailable());
    }
    Ok(Some(receipt))
}

async fn insert_open_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    receipt: &RecorderOpenReceipt,
) -> Result<(), ConversationStoreFailure> {
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_v3_open_receipts (person_id, run_id, conversation_id, branch_id, receipt_json) VALUES (?, ?, ?, ?, ?)",
            (
                scope.person_id.to_string(),
                receipt.fence.run_id.as_uuid().to_string(),
                scope.conversation_id.as_uuid().to_string(),
                scope.branch_id.as_uuid().to_string(),
                encode(receipt)?,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn insert_active_recorder_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    fence: &RecorderFence,
) -> Result<(), ConversationStoreFailure> {
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_v3_active_recorders (person_id, conversation_id, branch_id, run_id, fence_json) VALUES (?, ?, ?, ?, ?)",
            (
                scope.person_id.to_string(),
                scope.conversation_id.as_uuid().to_string(),
                scope.branch_id.as_uuid().to_string(),
                fence.run_id.as_uuid().to_string(),
                encode(fence)?,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn insert_recording_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    receipt: &RecordingReceipt,
) -> Result<(), ConversationStoreFailure> {
    let receipt_json = encode(receipt)?;
    let contribution_id = receipt.contribution_id.as_uuid().to_string();
    let sequence = integer(receipt.transcript.sequence)?;
    let message_id = receipt.transcript.message_id.as_uuid().to_string();
    let task_json = receipt.producing_task.as_ref().map(encode).transpose()?;
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_v3_recording_receipts (person_id, contribution_id, conversation_id, branch_id, run_id, sequence, message_id, receipt_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                scope.person_id.to_string(),
                contribution_id.clone(),
                scope.conversation_id.as_uuid().to_string(),
                scope.branch_id.as_uuid().to_string(),
                receipt.recorder.run_id.as_uuid().to_string(),
                sequence,
                message_id.clone(),
                receipt_json.clone(),
            ),
        )
        .await
        .map_err(database_error)?;
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_output_receipts_v2 (person_id, agent_instance_id, message_id, contribution_id, conversation_id, branch_id, producer_run_id, sequence, content_digest, producing_task_json, receipt_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                scope.person_id.to_string(),
                receipt.recorder.identity.agent_instance_id.as_uuid().to_string(),
                message_id,
                contribution_id,
                scope.conversation_id.as_uuid().to_string(),
                scope.branch_id.as_uuid().to_string(),
                receipt.recorder.run_id.as_uuid().to_string(),
                sequence,
                hex_digest(receipt.content_digest),
                task_json,
                receipt_json,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn insert_close_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    receipt: &RecorderCloseReceipt,
) -> Result<(), ConversationStoreFailure> {
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_v3_close_receipts (person_id, run_id, conversation_id, branch_id, receipt_json) VALUES (?, ?, ?, ?, ?)",
            (
                scope.person_id.to_string(),
                receipt.fence.run_id.as_uuid().to_string(),
                scope.conversation_id.as_uuid().to_string(),
                scope.branch_id.as_uuid().to_string(),
                encode(receipt)?,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn insert_retirement_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    receipt: &RecorderRetirementReceipt,
) -> Result<(), ConversationStoreFailure> {
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_v3_retirement_receipts (person_id, run_id, conversation_id, branch_id, receipt_json) VALUES (?, ?, ?, ?, ?)",
            (
                scope.person_id.to_string(),
                receipt.fence.run_id.as_uuid().to_string(),
                scope.conversation_id.as_uuid().to_string(),
                scope.branch_id.as_uuid().to_string(),
                encode(receipt)?,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn delete_active_recorder_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    fence: &RecorderFence,
) -> Result<(), ConversationStoreFailure> {
    let changed = transaction
        .execute(
            "DELETE FROM agent_conversation_core_v3_active_recorders WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND run_id = ? AND fence_json = ?",
            (
                scope.person_id.to_string(),
                scope.conversation_id.as_uuid().to_string(),
                scope.branch_id.as_uuid().to_string(),
                fence.run_id.as_uuid().to_string(),
                encode(fence)?,
            ),
        )
        .await
        .map_err(database_error)?;
    if changed != 1 {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::WrongWriter,
        ));
    }
    Ok(())
}

pub(super) fn parse_logical_contribution_id(
    value: &str,
) -> Result<floe_conversation_contract::LogicalContributionId, ConversationStoreFailure> {
    floe_conversation_contract::LogicalContributionId::from_uuid(parse_uuid(value)?)
        .ok_or_else(unavailable)
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Host composition primitive: append neutral input custody inside the
    /// caller's owner-admission transaction.
    pub(super) async fn append_conversation_input_on(
        &self,
        transaction: &Transaction<'_>,
        request: MessageAdmissionRequest,
    ) -> Result<AdmissionResult, ConversationStoreFailure> {
        ensure_core_v3_on(transaction).await?;
        let (scope, identity, is_new) = request_scope(self.person_id, &request)?;
        let loaded_head = load_head_on(transaction, scope).await?;
        let head = match &loaded_head {
            Some(loaded) => loaded.state.clone(),
            None if is_new => ConversationHead {
                identity: identity.clone(),
                conversation_id: scope.conversation_id,
                branch_id: scope.branch_id,
                head_revision: 0,
                settled_prefix: 0,
                state_revision: 0,
                recorder_epoch: 0,
            },
            None => {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationMismatch,
                ));
            }
        };
        let message_id = request.message.message_id;
        let stored_input = input_receipt_on(transaction, scope, message_id).await?;
        let message_id_in_transcript = if stored_input.is_some() {
            true
        } else {
            transcript_has_message_id_on(transaction, scope, message_id).await?
        };
        let previous_prefix_digest = tail_prefix_digest_on(transaction, scope, &head).await?;
        let transition = map_transition(append_input_transition(
            request,
            AppendFacts {
                head: head.clone(),
                stored_receipt: stored_input
                    .as_ref()
                    .map(|(value, _)| value.receipt.clone()),
                stored_message: stored_input.map(|(_, message)| message),
                message_id_in_transcript,
                previous_prefix_digest,
            },
        ))?;
        if let Some(entry) = &transition.appended {
            save_head_on(transaction, scope, loaded_head.as_ref(), &transition.head).await?;
            insert_entry_on(transaction, scope, entry).await?;
            insert_input_receipt_on(transaction, scope, &transition.result).await?;
        }
        Ok(transition.result)
    }

    /// Admit a Host Run and bind its exact retained Core input in the same
    /// Vault transaction. New turns append one Person input. Continue and
    /// linked Resume resolve an earlier immutable owner binding and replay
    /// that exact Core input. Every admitted Run opens its fresh HostRun
    /// recorder before commit. Resume additionally claims the persisted group
    /// through the same transaction-scoped owner primitive as the public
    /// owner-only API.
    pub(super) async fn admit_conversation_run_with_core_input_on(
        &self,
        transaction: &Transaction<'_>,
        intent: CoreComposedOwnerIntent,
        core_request: MessageAdmissionRequest,
        replay_checked: &mut bool,
        prior_command: &mut bool,
    ) -> Result<CoreComposedRunAdmission, ConversationStoreFailure> {
        let (owner_request, resume_claim) = match intent {
            CoreComposedOwnerIntent::Turn(request) => {
                if matches!(&request.mode, TurnMode::Resume(_)) {
                    return Err(ConversationStoreFailure::UnsupportedOwnerIntent);
                }
                (request, None)
            }
            CoreComposedOwnerIntent::Resume(request) => {
                let child = request.child.clone();
                (child, Some(request))
            }
        };
        ensure_core_v3_on(transaction).await?;
        owner_request.validate().map_err(|_| {
            ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
        })?;
        if let Some(request) = resume_claim.as_ref() {
            request.validate().map_err(|_| {
                ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
            })?;
        }
        core_request
            .validate()
            .map_err(ConversationStoreFailure::Transition)?;
        let (owner_user_message_id, owner_message_text) =
            match (&owner_request.mode, &owner_request.input) {
                (TurnMode::New, TurnInput::NewMessage(message)) => {
                    (message.message_id, Some(message.text.as_str()))
                }
                (
                    TurnMode::Continue(_) | TurnMode::Resume(_),
                    TurnInput::ExistingMessage { message_id },
                ) => (*message_id, None),
                _ => {
                    return Err(ConversationStoreFailure::Transition(
                        ConversationFailure::OwnerEvidenceMismatch,
                    ));
                }
            };
        if owner_request.principal != self.person_id.to_string()
            || owner_message_text.is_some_and(|text| text != core_request.message.text)
            || core_request.message.evidence.is_some()
            || core_request.message.task_id.is_some()
            || core_request.message.origin
                != (floe_conversation_contract::MessageOrigin::Person {
                    person_id: self.person_id,
                })
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let source_mapping = match resume_claim.as_ref() {
            Some(request) => Some(
                owner_transcript_input_for_run_on(
                    transaction,
                    self.person_id,
                    request.request.origin_run_id,
                )
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?,
            ),
            None if owner_message_text.is_none() => Some(
                owner_transcript_input_for_owner_message_on(
                    transaction,
                    self.person_id,
                    owner_request.session_id,
                    owner_user_message_id,
                )
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?,
            ),
            None => None,
        };
        let source_binding = source_mapping
            .as_ref()
            .map(|mapping| mapping.original_binding.clone());
        let (identity, conversation_id, branch_id) = match &core_request.target {
            AdmissionTarget::New {
                identity,
                conversation_id,
                branch_id,
            } => (identity.clone(), *conversation_id, *branch_id),
            AdmissionTarget::AppendToExisting { reference } => (
                reference.identity.clone(),
                reference.conversation_id,
                reference.branch_id,
            ),
        };
        if identity.person_id != self.person_id {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::AgentMismatch,
            ));
        }
        if let Some(binding) = source_binding.as_ref()
            && (binding.identity != identity
                || binding.conversation_id != conversation_id
                || binding.branch_id != branch_id
                || binding.input.message_id != core_request.message.message_id
                || binding.session_id != owner_request.session_id
                || binding.owner_user_message_id != owner_user_message_id)
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        if let Some(binding) = source_binding.as_ref() {
            self.verified_owner_evidence_on(
                transaction,
                binding.run_id,
                &binding.identity,
                binding.conversation_id,
                binding.branch_id,
                binding.input,
                binding.executor_domain,
                binding.executor_generation,
            )
            .await?;
            let (stored_receipt, stored_message) = input_receipt_on(
                transaction,
                Scope::from_identity(
                    self.person_id,
                    &binding.identity,
                    binding.conversation_id,
                    binding.branch_id,
                ),
                binding.input.message_id,
            )
            .await?
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
            if stored_receipt.receipt.transcript != binding.input
                || stored_receipt.receipt.task_id.is_some()
                || stored_message != core_request.message
            {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
        }
        let scope = Scope::from_identity(self.person_id, &identity, conversation_id, branch_id);
        let admission = match resume_claim {
            Some(request) => match self
                .claim_conversation_resume_on(transaction, request)
                .await
                .map_err(owner_error)?
            {
                super::conversations::ResumeClaimOutcome::Admitted(admission) => admission,
                super::conversations::ResumeClaimOutcome::Superseded => {
                    return Ok(CoreComposedRunAdmission::ResumeSuperseded);
                }
            },
            None => self
                .admit_conversation_turn_on(
                    transaction,
                    owner_request.clone(),
                    replay_checked,
                    prior_command,
                )
                .await
                .map_err(owner_error)?,
        };
        #[cfg(test)]
        if matches!(
            &admission,
            super::VaultConversationAdmission::Created { .. }
        ) && matches!(&owner_request.mode, TurnMode::Resume(_))
            && self
                .conversation_core_resume_fault_after_owner_claim
                .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            return Err(ConversationStoreFailure::NotCommitted);
        }
        let (record, newly_created) = match admission {
            super::VaultConversationAdmission::Created { record, .. } => (record, true),
            super::VaultConversationAdmission::Existing(record) => (record, false),
            super::VaultConversationAdmission::Resumed(record) => (record, false),
        };
        if record.run_id != owner_request.run_id
            || record.expert_environment != owner_request.expert_environment
            || !record.matches_admission(&owner_request)
            || record.person_id != self.person_id
            || record.session_id != owner_request.session_id
            || record.user_message_id != owner_user_message_id
            || record.executor_generation == 0
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let existing_binding = if newly_created {
            None
        } else {
            let binding = owner_input_binding_on(transaction, self.person_id, record.run_id)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?;
            if binding.session_id != record.session_id
                || binding.owner_user_message_id != record.user_message_id
                || binding.identity != identity
                || binding.conversation_id != conversation_id
                || binding.branch_id != branch_id
                || binding.input.message_id != core_request.message.message_id
                || binding.executor_domain != ExecutorDomain::HostRun
                || binding.executor_generation != record.executor_generation
            {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
            let (stored_receipt, stored_message) =
                input_receipt_on(transaction, scope, binding.input.message_id)
                    .await?
                    .ok_or(ConversationStoreFailure::Transition(
                        ConversationFailure::OwnerEvidenceMismatch,
                    ))?;
            if stored_receipt.receipt.transcript != binding.input
                || stored_receipt.receipt.task_id.is_some()
                || stored_message != core_request.message
            {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
            Some(binding)
        };
        let existing_recorder = if newly_created {
            None
        } else {
            let binding = existing_binding
                .as_ref()
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?;
            let receipt = open_receipt_on(transaction, self.person_id, record.run_id)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?;
            replay_open_recording(
                RecorderStartRequest {
                    identity: identity.clone(),
                    conversation_id,
                    branch_id,
                    run_id: record.run_id,
                    input: binding.input,
                    executor_domain: ExecutorDomain::HostRun,
                    executor_generation: record.executor_generation,
                    execution_task: None,
                },
                receipt.clone(),
            )
            .map_err(ConversationStoreFailure::Transition)?;
            Some(receipt)
        };
        let core_admission = self
            .append_conversation_input_on(transaction, core_request)
            .await?;
        let expected_disposition = if newly_created && owner_message_text.is_some() {
            AdmissionDisposition::Appended
        } else {
            AdmissionDisposition::Replayed
        };
        if core_admission.disposition != expected_disposition
            || core_admission.receipt.task_id.is_some()
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let input_entry = entry_on(
            transaction,
            scope,
            core_admission.receipt.transcript.sequence,
        )
        .await?
        .ok_or_else(unavailable)?;
        if input_entry.reference != core_admission.receipt.transcript
            || input_entry.kind != TranscriptEntryKind::Inbound
            || input_entry.message.origin
                != (floe_conversation_contract::MessageOrigin::Person {
                    person_id: self.person_id,
                })
            || owner_message_text.is_some_and(|text| text != input_entry.message.text)
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        if owner_message_text.is_some() {
            let expected_owner_digest = CanonicalTurnIntent {
                session_id: record.session_id,
                expected_revision: record.initial_session_revision,
                text: input_entry.message.text,
                mode: TurnMode::New,
                retry_of: record.retry_of,
            }
            .digest(&self.person_id.to_string())
            .map_err(|_| {
                ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
            })?;
            if record.request_digest != expected_owner_digest {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
        }
        let binding = OwnerInputBinding {
            run_id: record.run_id,
            session_id: record.session_id,
            owner_user_message_id: record.user_message_id,
            identity: identity.clone(),
            conversation_id,
            branch_id,
            input: core_admission.receipt.transcript,
            executor_domain: ExecutorDomain::HostRun,
            executor_generation: record.executor_generation,
        };
        binding.validate(self.person_id)?;
        match existing_binding {
            Some(existing) if existing == binding => {}
            Some(_) => {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::RunAlreadyUsed,
                ));
            }
            None if newly_created => insert_owner_input_binding_on(transaction, &binding).await?,
            None => {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
        }
        let mapping = if let Some(mapping) = source_mapping {
            if mapping.input != binding.input
                || mapping.session_id != binding.session_id
                || mapping.owner_user_message_id != binding.owner_user_message_id
                || mapping.original_binding.identity != binding.identity
            {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
            mapping
        } else {
            let mapping = OwnerTranscriptInputMapping {
                person_id: self.person_id,
                input: binding.input,
                session_id: binding.session_id,
                owner_user_message_id: binding.owner_user_message_id,
                original_owner_run_id: binding.run_id,
                original_binding: binding.clone(),
            };
            mapping.validate(self.person_id)?;
            if newly_created {
                // This optional family is created only by the explicit
                // composed owner/Core admission write.
                ensure_owner_custody_v1_on(transaction).await?;
                insert_owner_transcript_input_mapping_on(transaction, &mapping).await?;
            } else if owner_transcript_input_for_reference_on(
                transaction,
                self.person_id,
                binding.input,
            )
            .await?
            .as_ref()
                != Some(&mapping)
            {
                // A legacy owner/Core row with no proof mapping stays
                // unbound. Replays never retrofit it.
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
            mapping
        };
        if newly_created {
            insert_owner_transcript_run_input_on(
                transaction,
                self.person_id,
                binding.run_id,
                &mapping,
            )
            .await?;
            #[cfg(test)]
            if self
                .conversation_core_fault_after_input_mapping
                .swap(false, std::sync::atomic::Ordering::AcqRel)
            {
                return Err(ConversationStoreFailure::NotCommitted);
            }
        } else if owner_transcript_input_for_run_on(transaction, self.person_id, binding.run_id)
            .await?
            .as_ref()
            != Some(&mapping)
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let recorder = match existing_recorder {
            Some(receipt) => receipt,
            None => {
                self.open_conversation_recorder_on(
                    transaction,
                    RecorderStartRequest {
                        identity,
                        conversation_id,
                        branch_id,
                        run_id: record.run_id,
                        input: core_admission.receipt.transcript,
                        executor_domain: ExecutorDomain::HostRun,
                        executor_generation: record.executor_generation,
                        execution_task: None,
                    },
                )
                .await?
            }
        };
        #[cfg(test)]
        if self
            .conversation_core_fault_after_recorder_open
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            return Err(ConversationStoreFailure::NotCommitted);
        }
        Ok(CoreComposedRunAdmission::Admitted {
            record,
            input: core_admission,
            recorder,
        })
    }

    pub(super) async fn append_conversation_input(
        &self,
        request: MessageAdmissionRequest,
    ) -> Result<AdmissionResult, ConversationStoreFailure> {
        let mut connection = self.connection().map_err(start_error)?;
        let (guard, transaction) = self
            .journal_transaction(&mut connection)
            .await
            .map_err(start_error)?;
        let result = self
            .append_conversation_input_on(&transaction, request)
            .await;
        self.finish_conversation_core_transaction(guard, transaction, result)
            .await
    }

    /// Host composition primitive: bind a freshly admitted, exact Run to a
    /// retained neutral input reference in the same transaction.
    pub(super) async fn open_conversation_recorder_on(
        &self,
        transaction: &Transaction<'_>,
        request: RecorderStartRequest,
    ) -> Result<RecorderOpenReceipt, ConversationStoreFailure> {
        ensure_core_v3_on(transaction).await?;
        request
            .validate()
            .map_err(ConversationStoreFailure::Transition)?;
        if request.identity.person_id != self.person_id {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::AgentMismatch,
            ));
        }
        if let Some(receipt) = open_receipt_on(transaction, self.person_id, request.run_id).await? {
            return replay_open_recording(request, receipt)
                .map(|transition| transition.receipt)
                .map_err(ConversationStoreFailure::Transition);
        }
        if request.executor_domain == ExecutorDomain::TaskExecution {
            return Err(ConversationStoreFailure::UnsupportedOwnerDomain);
        }
        let scope = Scope::from_identity(
            self.person_id,
            &request.identity,
            request.conversation_id,
            request.branch_id,
        );
        let loaded_head =
            load_head_on(transaction, scope)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationMismatch,
                ))?;
        let current_generation = self
            .active_conversation_executor_generation(transaction)
            .await
            .map_err(owner_error)?;
        if current_generation != request.executor_generation {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let (owner, _, _) = self
            .verified_owner_evidence_on(
                transaction,
                request.run_id,
                &request.identity,
                request.conversation_id,
                request.branch_id,
                request.input,
                request.executor_domain,
                request.executor_generation,
            )
            .await?;
        let active_recorder = active_recorder_on(transaction, scope).await?;
        let input_entry = entry_on(transaction, scope, request.input.sequence)
            .await?
            .filter(|entry| entry.reference == request.input);
        let transition = open_recording_transition(
            request,
            RecorderOpenFacts {
                owner: Some(owner),
                head: loaded_head.state.clone(),
                stored_receipt: None,
                active_recorder,
                input_entry,
            },
        )
        .map_err(ConversationStoreFailure::Transition)?;
        if let Some(next) = &transition.head {
            save_head_on(transaction, scope, Some(&loaded_head), next).await?;
            insert_open_receipt_on(transaction, scope, &transition.receipt).await?;
            insert_active_recorder_on(transaction, scope, &transition.receipt.fence).await?;
        }
        Ok(transition.receipt)
    }

    pub(super) async fn open_conversation_recorder(
        &self,
        request: RecorderStartRequest,
    ) -> Result<RecorderOpenReceipt, ConversationStoreFailure> {
        let mut connection = self.connection().map_err(start_error)?;
        let (guard, transaction) = self
            .journal_transaction(&mut connection)
            .await
            .map_err(start_error)?;
        let result = self
            .open_conversation_recorder_on(&transaction, request)
            .await;
        self.finish_conversation_core_transaction(guard, transaction, result)
            .await
    }

    /// Host composition primitive: append one immutable, owner-proven output
    /// contribution and its per-entry Task reference.
    pub(super) async fn record_conversation_entry_on(
        &self,
        transaction: &Transaction<'_>,
        request: RecordingRequest,
    ) -> Result<RecordingReceipt, ConversationStoreFailure> {
        ensure_core_v3_on(transaction).await?;
        if let Some(receipt) =
            recording_receipt_on(transaction, self.person_id, request.contribution_id).await?
        {
            return replay_recording_entry(request, receipt)
                .map(|transition| transition.receipt)
                .map_err(ConversationStoreFailure::Transition);
        }
        if request.recorder.identity.person_id != self.person_id {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::AgentMismatch,
            ));
        }
        if request.recorder.executor_domain == ExecutorDomain::TaskExecution {
            return Err(ConversationStoreFailure::UnsupportedOwnerDomain);
        }
        let scope = Scope::from_identity(
            self.person_id,
            &request.recorder.identity,
            request.recorder.conversation_id,
            request.recorder.branch_id,
        );
        let loaded_head =
            load_head_on(transaction, scope)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationMismatch,
                ))?;
        let active_recorder = active_recorder_on(transaction, scope).await?;
        let current_generation = self
            .active_conversation_executor_generation(transaction)
            .await
            .map_err(owner_error)?;
        if current_generation != request.recorder.executor_generation {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::WrongWriter,
            ));
        }
        let (owner, _, _) = self
            .verified_owner_evidence_on(
                transaction,
                request.recorder.run_id,
                &request.recorder.identity,
                request.recorder.conversation_id,
                request.recorder.branch_id,
                request.recorder.input,
                request.recorder.executor_domain,
                request.recorder.executor_generation,
            )
            .await?;
        let verified_task_reference = match request.producing_task.as_ref() {
            Some(reference) => Some(
                self.verified_task_reference_on(transaction, request.recorder.run_id, reference)
                    .await?,
            ),
            None => None,
        };
        let message_id_in_transcript =
            transcript_has_message_id_on(transaction, scope, request.message.message_id).await?;
        let previous_prefix_digest =
            tail_prefix_digest_on(transaction, scope, &loaded_head.state).await?;
        let transition = record_entry_transition(
            request,
            RecordingFacts {
                head: loaded_head.state.clone(),
                active_recorder,
                owner: Some(owner),
                verified_task_reference,
                stored_receipt: None,
                message_id_in_transcript,
                previous_prefix_digest,
            },
        )
        .map_err(ConversationStoreFailure::Transition)?;
        if let Some(entry) = &transition.appended {
            let next = transition.head.as_ref().ok_or_else(unavailable)?;
            save_head_on(transaction, scope, Some(&loaded_head), next).await?;
            insert_entry_on(transaction, scope, entry).await?;
            insert_recording_receipt_on(transaction, scope, &transition.receipt).await?;
        }
        Ok(transition.receipt)
    }

    pub(super) async fn record_conversation_entry(
        &self,
        request: RecordingRequest,
    ) -> Result<RecordingReceipt, ConversationStoreFailure> {
        let mut connection = self.connection().map_err(start_error)?;
        let (guard, transaction) = self
            .journal_transaction(&mut connection)
            .await
            .map_err(start_error)?;
        let result = self
            .record_conversation_entry_on(&transaction, request)
            .await;
        self.finish_conversation_core_transaction(guard, transaction, result)
            .await
    }

    /// Host composition primitive. Current Vault owner records do not yet
    /// expose verified transcript settlement/protection roots, so this close
    /// persists an unchanged settled prefix and never infers one from queue data.
    pub(super) async fn close_conversation_recorder_on(
        &self,
        transaction: &Transaction<'_>,
        fence: RecorderFence,
    ) -> Result<RecorderCloseReceipt, ConversationStoreFailure> {
        ensure_core_v3_on(transaction).await?;
        if let Some(receipt) = close_receipt_on(transaction, self.person_id, fence.run_id).await? {
            return replay_close_recording(fence, receipt)
                .map(|transition| transition.receipt)
                .map_err(ConversationStoreFailure::Transition);
        }
        if retirement_receipt_on(transaction, self.person_id, fence.run_id)
            .await?
            .is_some()
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::RunAlreadyUsed,
            ));
        }
        if fence.identity.person_id != self.person_id {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::AgentMismatch,
            ));
        }
        if fence.executor_domain == ExecutorDomain::TaskExecution {
            return Err(ConversationStoreFailure::UnsupportedOwnerDomain);
        }
        let scope = Scope::from_identity(
            self.person_id,
            &fence.identity,
            fence.conversation_id,
            fence.branch_id,
        );
        let loaded_head =
            load_head_on(transaction, scope)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationMismatch,
                ))?;
        let current_generation = self
            .active_conversation_executor_generation(transaction)
            .await
            .map_err(owner_error)?;
        if current_generation != fence.executor_generation {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::WrongWriter,
            ));
        }
        let active_recorder = active_recorder_on(transaction, scope).await?;
        let (owner, record, terminal_digest) = self
            .verified_owner_evidence_on(
                transaction,
                fence.run_id,
                &fence.identity,
                fence.conversation_id,
                fence.branch_id,
                fence.input,
                fence.executor_domain,
                fence.executor_generation,
            )
            .await?;
        if owner.state != OwnerRunState::Terminal || terminal_digest.is_none() {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let terminal_digest = terminal_digest.ok_or_else(unavailable)?;
        let settlement = OwnerSettlementEvidence {
            owner,
            terminal_receipt_digest: terminal_digest,
            settled_through: None,
            protection_roots: Vec::new(),
        };
        let transition = close_recording_transition(
            fence.clone(),
            RecorderCloseFacts {
                head: loaded_head.state.clone(),
                active_recorder,
                owner: Some(settlement),
                stored_receipt: None,
                settlement_prefix_verified: false,
            },
        )
        .map_err(ConversationStoreFailure::Transition)?;
        if let Some(next) = &transition.head {
            save_head_on(transaction, scope, Some(&loaded_head), next).await?;
            insert_close_receipt_on(transaction, scope, &transition.receipt).await?;
            delete_active_recorder_on(transaction, scope, &fence).await?;
        }
        let _ = record;
        Ok(transition.receipt)
    }

    pub(super) async fn close_conversation_recorder(
        &self,
        fence: RecorderFence,
    ) -> Result<RecorderCloseReceipt, ConversationStoreFailure> {
        let mut connection = self.connection().map_err(start_error)?;
        let (guard, transaction) = self
            .journal_transaction(&mut connection)
            .await
            .map_err(start_error)?;
        let result = self
            .close_conversation_recorder_on(&transaction, fence)
            .await;
        self.finish_conversation_core_transaction(guard, transaction, result)
            .await
    }

    /// Retire an abandoned recorder only after a durable, newer Host Run
    /// executor generation and terminal or pending-terminal owner evidence.
    pub(super) async fn retire_conversation_recorder_on(
        &self,
        transaction: &Transaction<'_>,
        fence: RecorderFence,
    ) -> Result<RecorderRetirementReceipt, ConversationStoreFailure> {
        ensure_core_v3_on(transaction).await?;
        if let Some(receipt) =
            retirement_receipt_on(transaction, self.person_id, fence.run_id).await?
        {
            if receipt.fence != fence {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::RunAlreadyUsed,
                ));
            }
            return Ok(receipt);
        }
        if close_receipt_on(transaction, self.person_id, fence.run_id)
            .await?
            .is_some()
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::RunAlreadyUsed,
            ));
        }
        if fence.identity.person_id != self.person_id {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::AgentMismatch,
            ));
        }
        if fence.executor_domain == ExecutorDomain::TaskExecution {
            return Err(ConversationStoreFailure::UnsupportedOwnerDomain);
        }
        let scope = Scope::from_identity(
            self.person_id,
            &fence.identity,
            fence.conversation_id,
            fence.branch_id,
        );
        let loaded_head =
            load_head_on(transaction, scope)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationMismatch,
                ))?;
        let active_recorder = active_recorder_on(transaction, scope).await?;
        let current_generation = self
            .active_conversation_executor_generation(transaction)
            .await
            .map_err(owner_error)?;
        let generation_fence = owner_generation_fence(
            fence.executor_domain,
            fence.run_id,
            fence.executor_generation,
            current_generation,
        )?;
        let (owner, _, terminal_digest) = self
            .verified_owner_evidence_on(
                transaction,
                fence.run_id,
                &fence.identity,
                fence.conversation_id,
                fence.branch_id,
                fence.input,
                fence.executor_domain,
                fence.executor_generation,
            )
            .await?;
        if owner.state == OwnerRunState::Terminal && terminal_digest.is_none() {
            return Err(unavailable());
        }
        let transition = retire_stale_recording_transition(
            fence.clone(),
            generation_fence,
            RecorderRetirementFacts {
                head: loaded_head.state.clone(),
                active_recorder,
                owner: Some(owner),
                stored_receipt: None,
            },
        )
        .map_err(ConversationStoreFailure::Transition)?;
        if let Some(next) = &transition.head {
            save_head_on(transaction, scope, Some(&loaded_head), next).await?;
            insert_retirement_receipt_on(transaction, scope, &transition.receipt).await?;
            delete_active_recorder_on(transaction, scope, &fence).await?;
        }
        Ok(transition.receipt)
    }

    pub(super) async fn retire_conversation_recorder(
        &self,
        fence: RecorderFence,
    ) -> Result<RecorderRetirementReceipt, ConversationStoreFailure> {
        let mut connection = self.connection().map_err(start_error)?;
        let (guard, transaction) = self
            .journal_transaction(&mut connection)
            .await
            .map_err(start_error)?;
        let result = self
            .retire_conversation_recorder_on(&transaction, fence)
            .await;
        self.finish_conversation_core_transaction(guard, transaction, result)
            .await
    }

    pub(super) async fn read_conversation_page(
        &self,
        target: ConversationReference,
        after: Option<TranscriptReference>,
        budget: TranscriptPageBudget,
    ) -> Result<TranscriptPage, ConversationStoreFailure> {
        if target.identity.person_id != self.person_id {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::AgentMismatch,
            ));
        }
        let mut connection = self.connection().map_err(start_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| start_error(database_failure(error)))?;
        let result = self
            .read_conversation_page_on(&transaction, &target, after, budget)
            .await;
        self.finish_conversation_core_read(transaction, result)
            .await
    }

    pub(super) async fn read_conversation_page_on(
        &self,
        transaction: &Transaction<'_>,
        target: &ConversationReference,
        after: Option<TranscriptReference>,
        budget: TranscriptPageBudget,
    ) -> Result<TranscriptPage, ConversationStoreFailure> {
        require_core_v3_on(transaction).await?;
        let scope = Scope::from_identity(
            self.person_id,
            &target.identity,
            target.conversation_id,
            target.branch_id,
        );
        let loaded =
            load_head_on(transaction, scope)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationMismatch,
                ))?;
        map_transition(validate_reference_target(&loaded.state, target))?;
        if budget.max_entries == 0 || budget.max_bytes == 0 {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::InvalidInput,
            ));
        }
        let mut cursor_sequence = 0;
        if let Some(cursor) = after {
            cursor
                .validate()
                .map_err(ConversationStoreFailure::Transition)?;
            if cursor.conversation_id != scope.conversation_id
                || cursor.branch_id != scope.branch_id
                || entry_on(transaction, scope, cursor.sequence)
                    .await?
                    .is_none_or(|entry| entry.reference != cursor)
            {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationMismatch,
                ));
            }
            cursor_sequence = cursor.sequence;
        }
        let max_entries = budget.max_entries.min(MAX_TRANSCRIPT_PAGE_ENTRIES);
        let max_bytes = budget.max_bytes.min(MAX_TRANSCRIPT_PAGE_BYTES);
        let mut entries = Vec::with_capacity(max_entries.min(32));
        let mut encoded_bytes = 0usize;
        let mut sequence = cursor_sequence.saturating_add(1);
        while sequence <= loaded.state.head_revision && entries.len() < max_entries {
            let entry = entry_on(transaction, scope, sequence)
                .await?
                .ok_or_else(unavailable)?;
            self.validate_producing_task_reference_on(transaction, &entry)
                .await?;
            let entry_bytes = encode(&entry)?.as_bytes().len();
            if entry_bytes > max_bytes.saturating_sub(encoded_bytes) {
                if entries.is_empty() {
                    return Err(ConversationStoreFailure::PageItemExceedsBudget);
                }
                break;
            }
            encoded_bytes = encoded_bytes.checked_add(entry_bytes).ok_or(
                ConversationStoreFailure::Transition(ConversationFailure::InvalidInput),
            )?;
            entries.push(entry);
            sequence = sequence.saturating_add(1);
        }
        let next_cursor = entries.last().map(|entry| entry.reference);
        let through = next_cursor.map_or(cursor_sequence, |cursor| cursor.sequence);
        Ok(TranscriptPage {
            entries,
            next_cursor,
            has_more: loaded.state.head_revision > through,
            encoded_bytes,
        })
    }

    pub(super) async fn observe_conversation_recorder(
        &self,
        target: ConversationReference,
        run_id: RunId,
    ) -> Result<RecorderRecoveryObservation, ConversationStoreFailure> {
        if target.identity.person_id != self.person_id {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::AgentMismatch,
            ));
        }
        let mut connection = self.connection().map_err(start_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| start_error(database_failure(error)))?;
        let scope = Scope::from_identity(
            self.person_id,
            &target.identity,
            target.conversation_id,
            target.branch_id,
        );
        let result = async {
            require_core_v3_on(&transaction).await?;
            let loaded = load_head_on(&transaction, scope).await?.ok_or(
                ConversationStoreFailure::Transition(ConversationFailure::ConversationMismatch),
            )?;
            map_transition(validate_reference_target(&loaded.state, &target))?;
            let open = open_receipt_on(&transaction, self.person_id, run_id).await?;
            let close = close_receipt_on(&transaction, self.person_id, run_id).await?;
            let retired = retirement_receipt_on(&transaction, self.person_id, run_id).await?;
            let active = active_recorder_on(&transaction, scope).await?;
            let current_generation = self
                .active_conversation_executor_generation(&transaction)
                .await
                .map_err(owner_error)?;
            let recorder = open.map(|receipt| receipt.fence);
            let state = if retired.is_some() {
                RecorderRecoveryState::Retired
            } else if close.is_some() {
                RecorderRecoveryState::Closed
            } else if let Some(fence) = &recorder {
                if active.as_ref() != Some(fence) || fence.executor_generation != current_generation
                {
                    RecorderRecoveryState::Interrupted
                } else {
                    RecorderRecoveryState::ActiveCurrentGeneration
                }
            } else {
                RecorderRecoveryState::Absent
            };
            Ok(RecorderRecoveryObservation {
                state,
                head: loaded.state,
                recorder,
                active_recorder: active,
                current_generation,
            })
        }
        .await;
        self.finish_conversation_core_read(transaction, result)
            .await
    }

    pub(super) async fn apply_conversation_checkpoint_on(
        &self,
        transaction: &Transaction<'_>,
        target: ConversationReference,
        checkpoint: ConversationCheckpoint,
    ) -> Result<(), ConversationStoreFailure> {
        ensure_core_v3_on(transaction).await?;
        if target.identity.person_id != self.person_id {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::AgentMismatch,
            ));
        }
        let scope = Scope::from_identity(
            self.person_id,
            &target.identity,
            target.conversation_id,
            target.branch_id,
        );
        let loaded_head =
            load_head_on(transaction, scope)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::ConversationMismatch,
                ))?;
        let current_checkpoint = checkpoint_on(transaction, scope).await?;
        let stored_prefix_digest = entry_on(transaction, scope, checkpoint.through.sequence)
            .await?
            .map(|entry| entry.prefix_digest);
        let next = apply_checkpoint_transition(
            &target,
            &checkpoint,
            CheckpointFacts {
                head: loaded_head.state.clone(),
                current_checkpoint_sequence: current_checkpoint
                    .as_ref()
                    .map(|value| value.through.sequence),
                stored_prefix_digest,
            },
        )
        .map_err(ConversationStoreFailure::Transition)?;
        save_head_on(transaction, scope, Some(&loaded_head), &next).await?;
        save_checkpoint_on(transaction, scope, current_checkpoint.as_ref(), &checkpoint).await
    }

    pub(super) async fn verified_owner_evidence_on(
        &self,
        transaction: &Transaction<'_>,
        run_id: RunId,
        identity: &AgentIdentity,
        conversation_id: ConversationId,
        branch_id: ConversationBranchId,
        input: TranscriptReference,
        executor_domain: ExecutorDomain,
        executor_generation: u64,
    ) -> Result<(OwnerRunEvidence, RunRecord, Option<[u8; 32]>), ConversationStoreFailure> {
        let record = self
            .conversation_run_on(transaction, run_id)
            .await
            .map_err(owner_error)?
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
        if record.run_id != run_id
            || record.person_id != self.person_id
            || identity.person_id != self.person_id
            || executor_domain != ExecutorDomain::HostRun
            || record.executor_generation != executor_generation
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let binding = owner_input_binding_on(transaction, self.person_id, run_id)
            .await?
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
        let scope = Scope::from_identity(self.person_id, identity, conversation_id, branch_id);
        let bound_input = entry_on(transaction, scope, binding.input.sequence)
            .await?
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
        let owner_request_matches_input = if let Some(source_run_id) =
            record.continuation_of.or(record.resume_of)
        {
            let source = self
                .conversation_run_on(transaction, source_run_id)
                .await
                .map_err(owner_error)?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?;
            let source_binding = owner_input_binding_on(transaction, self.person_id, source_run_id)
                .await?
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?;
            source.person_id == record.person_id
                && source.session_id == record.session_id
                && source.user_message_id == record.user_message_id
                && source_binding.session_id == binding.session_id
                && source_binding.owner_user_message_id == binding.owner_user_message_id
                && source_binding.identity == binding.identity
                && source_binding.conversation_id == binding.conversation_id
                && source_binding.branch_id == binding.branch_id
                && source_binding.input == binding.input
        } else {
            let expected_owner_digest = CanonicalTurnIntent {
                session_id: record.session_id,
                expected_revision: record.initial_session_revision,
                text: bound_input.message.text.clone(),
                mode: TurnMode::New,
                retry_of: record.retry_of,
            }
            .digest(&self.person_id.to_string())
            .map_err(|_| {
                ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
            })?;
            record.request_digest == expected_owner_digest
        };
        if binding.run_id != record.run_id
            || binding.session_id != record.session_id
            || binding.owner_user_message_id != record.user_message_id
            || binding.identity != *identity
            || binding.conversation_id != conversation_id
            || binding.branch_id != branch_id
            || binding.input != input
            || binding.executor_domain != executor_domain
            || binding.executor_generation != record.executor_generation
            || bound_input.reference != input
            || bound_input.kind != TranscriptEntryKind::Inbound
            || bound_input.message.origin
                != (floe_conversation_contract::MessageOrigin::Person {
                    person_id: self.person_id,
                })
            || bound_input.message.task_id.is_some()
            || bound_input.message.evidence.is_some()
            || !owner_request_matches_input
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let journal = self
            .conversation_journal_on(transaction, &record)
            .await
            .map_err(owner_error)?;
        let accounting = self
            .conversation_lineage_accounting_on(transaction, &record, &journal)
            .await
            .map_err(owner_error)?;
        let terminal_text = super::conversations::terminal_receipt_on(transaction, run_id)
            .await
            .map_err(owner_error)?;
        let terminal_digest = terminal_text.as_deref().map(parse_digest).transpose()?;
        let state = if record.pending_terminal.is_some() {
            if terminal_digest.is_some() {
                return Err(unavailable());
            }
            OwnerRunState::PendingTerminal
        } else if record.state.is_terminal() {
            if terminal_digest.is_none() {
                return Err(unavailable());
            }
            OwnerRunState::Terminal
        } else if record.state == RunState::Working {
            if terminal_digest.is_some() {
                return Err(unavailable());
            }
            OwnerRunState::Working
        } else {
            return Err(unavailable());
        };
        let mut unresolved = HashSet::new();
        for attempt in accounting.unresolved_attempts {
            unresolved.insert(attempt.attempt_id);
        }
        for task_id in accounting.unresolved_delegations {
            unresolved.insert(task_id.as_uuid());
        }
        let mut unresolved_effects = unresolved.into_iter().collect::<Vec<_>>();
        unresolved_effects.sort_unstable();
        let journal_events = journal.iter().map(|entry| &entry.event).collect::<Vec<_>>();
        let evidence_bytes = serde_json::to_vec(&(&record, journal_events, &terminal_text))
            .map_err(|_| unavailable())?;
        let record_digest = Sha256::digest(evidence_bytes).into();
        let evidence = OwnerRunEvidence {
            domain: ExecutorDomain::HostRun,
            run_id,
            person_id: self.person_id,
            input,
            executor_generation: record.executor_generation,
            aggregate_revision: record.aggregate_revision,
            journal_revision: record.journal_revision,
            state,
            record_digest,
            unresolved_effects,
        };
        evidence
            .validate()
            .map_err(ConversationStoreFailure::Transition)?;
        Ok((evidence, record, terminal_digest))
    }

    async fn verified_task_receipt_on(
        &self,
        transaction: &Transaction<'_>,
        reference: &TaskExecutionReceiptRef,
    ) -> Result<floe_agent_contract::TaskExecutionReceipt, ConversationStoreFailure> {
        reference.validate().map_err(|_| {
            ConversationStoreFailure::Transition(ConversationFailure::OwnerEvidenceMismatch)
        })?;
        let actual = self
            .read_execution_receipt_on(transaction, reference)
            .await
            .map_err(owner_error)?;
        if actual.reference != *reference {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        Ok(actual)
    }

    pub(super) async fn verified_task_reference_on(
        &self,
        transaction: &Transaction<'_>,
        run_id: RunId,
        reference: &TaskEvidenceReference,
    ) -> Result<TaskEvidenceReference, ConversationStoreFailure> {
        reference
            .validate()
            .map_err(ConversationStoreFailure::Transition)?;
        let open = open_receipt_on(transaction, self.person_id, run_id)
            .await?
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
        let fence = open.fence;
        let (_, owner_run, _) = self
            .verified_owner_evidence_on(
                transaction,
                run_id,
                &fence.identity,
                fence.conversation_id,
                fence.branch_id,
                fence.input,
                fence.executor_domain,
                fence.executor_generation,
            )
            .await?;
        let task = self
            .task_on(transaction, reference.task_id())
            .await
            .map_err(owner_error)?
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
        let receipt_reference = task
            .receipt
            .as_ref()
            .map(|receipt| receipt.reference.clone())
            .ok_or(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ))?;
        let verified = self
            .verified_task_receipt_on(transaction, &receipt_reference)
            .await?;
        if task.snapshot.parent_run_id != Some(run_id.as_uuid())
            || task.snapshot.principal != self.person_id.to_string()
            || matches!(
                task.snapshot.state,
                floe_agent_contract::TaskState::Submitted | floe_agent_contract::TaskState::Working
            )
            || verified.snapshot != task.snapshot
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let journal = self
            .conversation_journal_on(transaction, &owner_run)
            .await
            .map_err(owner_error)?;
        let mut intent: Option<floe_agent_contract::DelegationRequest> = None;
        let mut intent_revision = 0;
        let mut result: Option<Box<floe_agent_contract::TaskReceipt>> = None;
        let mut result_revision = 0;
        for entry in journal {
            match entry.event {
                floe_agent_contract::JournalEvent::DelegationIntent { request }
                    if request.task_id == reference.task_id() =>
                {
                    if intent.replace(request).is_some() {
                        return Err(ConversationStoreFailure::Transition(
                            ConversationFailure::OwnerEvidenceMismatch,
                        ));
                    }
                    intent_revision = entry.revision;
                }
                floe_agent_contract::JournalEvent::DelegationResult { receipt }
                    if receipt.task_id == reference.task_id() =>
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
        let expected_request_digest = floe_agent_contract::delegation_request_digest(&request);
        let expected_replay_input_digest: [u8; 32] =
            Sha256::digest(request.message.as_bytes()).into();
        let exact_receipt = matches!(
            &receipt.execution,
            floe_agent_contract::TaskExecutionEvidence::Admitted(stored) if stored == &verified
        );
        let replay_matches = receipt.replay.as_ref().is_none_or(|replay| {
            replay.task_id == Some(reference.task_id())
                && replay.run_id == Some(run_id)
                && replay.principal == request.principal
                && replay.agent_id.as_deref() == Some(request.selected_agent_id.as_str())
                && replay.definition_revision == request.selected_definition_revision
                && replay.invocation_key == request.invocation_key
                && replay.task_execution.as_ref() == Some(&verified)
                && replay.task_state == Some(task.snapshot.state)
                && replay.task_result == task.snapshot.result
                && replay.task_artifacts == task.snapshot.artifacts
                && replay.task_coverage == task.snapshot.coverage
                && replay.task_issue == task.snapshot.issue
                && replay.input_digest == expected_replay_input_digest
        });
        if intent_revision == 0
            || result_revision <= intent_revision
            || request.parent_run_id != Some(run_id.as_uuid())
            || request.principal != self.person_id.to_string()
            || request.invocation_key != task.invocation_key
            || request.selected_agent_id != task.snapshot.agent_id
            || request.selected_definition_revision != task.snapshot.definition_revision
            || request.execution_context.session_id != owner_run.session_id
            || request.execution_context.device_id != owner_run.device_id
            || task.device_id != owner_run.device_id
            || task.request_digest != expected_request_digest
            || !exact_receipt
            || receipt.snapshot != task.snapshot
            || receipt
                .validate(floe_agent_contract::MAX_OUTPUT_BYTES)
                .is_err()
            || !replay_matches
        {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        let resolved = task_evidence_reference(&verified.reference)?;
        if resolved != *reference {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch,
            ));
        }
        Ok(resolved)
    }

    pub(super) async fn validate_producing_task_reference_on(
        &self,
        transaction: &Transaction<'_>,
        entry: &TranscriptEntry,
    ) -> Result<(), ConversationStoreFailure> {
        if let Some(reference) = entry.producing_task.as_ref() {
            if entry.message.task_id != Some(reference.task_id()) {
                return Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ));
            }
            let run_id = entry
                .producer_run
                .ok_or(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch,
                ))?;
            self.verified_task_reference_on(transaction, run_id, reference)
                .await?;
        }
        Ok(())
    }

    async fn finish_conversation_core_transaction<'vault, 'transaction, T>(
        &'vault self,
        mut guard: JournalWriteGuard<'vault>,
        transaction: Transaction<'transaction>,
        result: Result<T, ConversationStoreFailure>,
    ) -> Result<T, ConversationStoreFailure> {
        match result {
            Ok(value) => {
                if transaction.commit().await.is_err() {
                    self.unavailable
                        .store(true, std::sync::atomic::Ordering::Release);
                    return Err(ConversationStoreFailure::OutcomeUnknown);
                }
                guard.settled();
                #[cfg(test)]
                if self
                    .conversation_core_ack_loss
                    .swap(false, std::sync::atomic::Ordering::AcqRel)
                {
                    self.unavailable
                        .store(true, std::sync::atomic::Ordering::Release);
                    return Err(ConversationStoreFailure::OutcomeUnknown);
                }
                if self.check_access().is_err() {
                    self.unavailable
                        .store(true, std::sync::atomic::Ordering::Release);
                    return Err(ConversationStoreFailure::OutcomeUnknown);
                }
                Ok(value)
            }
            Err(failure) => {
                if transaction.rollback().await.is_err() {
                    self.unavailable
                        .store(true, std::sync::atomic::Ordering::Release);
                    return Err(ConversationStoreFailure::OutcomeUnknown);
                }
                guard.settled();
                match failure {
                    ConversationStoreFailure::Transition(_)
                    | ConversationStoreFailure::UnsupportedOwnerDomain
                    | ConversationStoreFailure::UnsupportedOwnerIntent
                    | ConversationStoreFailure::UnsupportedStoredMeaning => Err(failure),
                    _ => Err(ConversationStoreFailure::NotCommitted),
                }
            }
        }
    }

    pub(super) async fn finish_conversation_core_read<T>(
        &self,
        transaction: Transaction<'_>,
        result: Result<T, ConversationStoreFailure>,
    ) -> Result<T, ConversationStoreFailure> {
        match result {
            Ok(value) => {
                transaction.commit().await.map_err(|_| unavailable())?;
                self.check_access().map_err(|_| unavailable())?;
                Ok(value)
            }
            Err(failure) => {
                if transaction.rollback().await.is_err() {
                    self.unavailable
                        .store(true, std::sync::atomic::Ordering::Release);
                    return Err(unavailable());
                }
                Err(failure)
            }
        }
    }

    pub(super) async fn validate_conversation_core_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        match crate::schema::conversation_core_family_version(&connection)
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?
        {
            crate::schema::ConversationCoreFamilyVersion::Absent => Ok(()),
            crate::schema::ConversationCoreFamilyVersion::RecorderV3 => {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Deferred)
                    .await
                    .map_err(database_failure)?;
                let result = self
                    .validate_all_recorder_v3_on(&transaction)
                    .await
                    .map_err(|_| AgentFailure::VaultUnavailable);
                self.finish_registry_transaction_checked(transaction, result)
                    .await
            }
        }
    }

    async fn validate_all_recorder_v3_on(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<(), ConversationStoreFailure> {
        let mut heads = transaction
            .query(
                "SELECT conversation_id, branch_id FROM agent_conversation_core_v3_heads WHERE person_id = ? ORDER BY conversation_id, branch_id LIMIT 4097",
                [self.person_id.to_string()],
            )
            .await
            .map_err(database_error)?;
        let mut scopes = Vec::new();
        while let Some(row) = heads.next().await.map_err(database_error)? {
            if scopes.len() == 4096 {
                return Err(unavailable());
            }
            scopes.push(Scope {
                person_id: self.person_id,
                conversation_id: parse_conversation_id(
                    &row.get::<String>(0).map_err(|_| unavailable())?,
                )?,
                branch_id: parse_branch_id(&row.get::<String>(1).map_err(|_| unavailable())?)?,
            });
        }
        drop(heads);
        for scope in &scopes {
            let loaded = load_head_on(transaction, *scope)
                .await?
                .ok_or_else(unavailable)?;
            for sequence in 1..=loaded.state.head_revision {
                let entry = entry_on(transaction, *scope, sequence)
                    .await?
                    .ok_or_else(unavailable)?;
                self.validate_producing_task_reference_on(transaction, &entry)
                    .await?;
            }
            let (person, conversation, branch) = scope.sql();
            let mut active = transaction
                .query(
                    "SELECT run_id FROM agent_conversation_core_v3_active_recorders WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
                    (person, conversation, branch),
                )
                .await
                .map_err(database_error)?;
            if let Some(row) = active.next().await.map_err(database_error)? {
                let run_id = parse_run_id(&row.get::<String>(0).map_err(|_| unavailable())?)?;
                if open_receipt_on(transaction, self.person_id, run_id)
                    .await?
                    .is_none()
                    || close_receipt_on(transaction, self.person_id, run_id)
                        .await?
                        .is_some()
                    || retirement_receipt_on(transaction, self.person_id, run_id)
                        .await?
                        .is_some()
                {
                    return Err(unavailable());
                }
            }
        }
        let (orphan_inputs, orphan_outputs) =
            v3_orphan_counts_on(transaction, self.person_id).await?;
        if orphan_inputs != 0 || orphan_outputs != 0 {
            return Err(unavailable());
        }
        self.validate_recorder_owner_receipts_on(transaction)
            .await?;
        Ok(())
    }

    async fn validate_recorder_owner_receipts_on(
        &self,
        transaction: &Transaction<'_>,
    ) -> Result<(), ConversationStoreFailure> {
        let mut bindings = transaction
            .query(
                "SELECT run_id FROM agent_conversation_core_v3_owner_bindings WHERE person_id = ? ORDER BY run_id LIMIT 4097",
                [self.person_id.to_string()],
            )
            .await
            .map_err(database_error)?;
        let mut binding_runs = Vec::new();
        while let Some(row) = bindings.next().await.map_err(database_error)? {
            if binding_runs.len() == 4096 {
                return Err(unavailable());
            }
            binding_runs.push(parse_run_id(
                &row.get::<String>(0).map_err(|_| unavailable())?,
            )?);
        }
        drop(bindings);
        for run_id in binding_runs {
            let binding = owner_input_binding_on(transaction, self.person_id, run_id)
                .await?
                .ok_or_else(unavailable)?;
            self.verified_owner_evidence_on(
                transaction,
                run_id,
                &binding.identity,
                binding.conversation_id,
                binding.branch_id,
                binding.input,
                binding.executor_domain,
                binding.executor_generation,
            )
            .await?;
        }

        let mut rows = transaction
            .query(
                "SELECT run_id FROM agent_conversation_core_v3_open_receipts WHERE person_id = ? ORDER BY run_id LIMIT 4097",
                [self.person_id.to_string()],
            )
            .await
            .map_err(database_error)?;
        let mut run_ids = Vec::new();
        while let Some(row) = rows.next().await.map_err(database_error)? {
            if run_ids.len() == 4096 {
                return Err(unavailable());
            }
            run_ids.push(parse_run_id(
                &row.get::<String>(0).map_err(|_| unavailable())?,
            )?);
        }
        drop(rows);
        for run_id in run_ids {
            let open = open_receipt_on(transaction, self.person_id, run_id)
                .await?
                .ok_or_else(unavailable)?;
            let fence = open.fence;
            if fence.executor_domain != ExecutorDomain::HostRun {
                return Err(unavailable());
            }
            let scope = Scope::from_identity(
                self.person_id,
                &fence.identity,
                fence.conversation_id,
                fence.branch_id,
            );
            let loaded = load_head_on(transaction, scope)
                .await?
                .ok_or_else(unavailable)?;
            let input = entry_on(transaction, scope, fence.input.sequence)
                .await?
                .ok_or_else(unavailable)?;
            if input.reference != fence.input || input.kind != TranscriptEntryKind::Inbound {
                return Err(unavailable());
            }
            let (owner, record, terminal_digest) = self
                .verified_owner_evidence_on(
                    transaction,
                    run_id,
                    &fence.identity,
                    fence.conversation_id,
                    fence.branch_id,
                    fence.input,
                    fence.executor_domain,
                    fence.executor_generation,
                )
                .await?;
            let active = active_recorder_on(transaction, scope).await?;
            let close = close_receipt_on(transaction, self.person_id, run_id).await?;
            let retired = retirement_receipt_on(transaction, self.person_id, run_id).await?;
            if u64::try_from(open.opened_head_revision)
                .ok()
                .is_none_or(|value| value > loaded.state.head_revision)
            {
                return Err(unavailable());
            }
            match (active.as_ref() == Some(&fence), close, retired) {
                (true, None, None) => {}
                (false, Some(receipt), None) => {
                    if owner.state != OwnerRunState::Terminal
                        || !owner.unresolved_effects.is_empty()
                        || terminal_digest != Some(receipt.terminal_receipt_digest)
                        || record.aggregate_revision != receipt.owner_aggregate_revision
                    {
                        return Err(unavailable());
                    }
                }
                (false, None, Some(receipt)) => {
                    if !matches!(
                        owner.state,
                        OwnerRunState::Terminal | OwnerRunState::PendingTerminal
                    ) || !owner.unresolved_effects.is_empty()
                        || owner.record_digest != receipt.owner_evidence_digest
                        || receipt.generation_fence.domain != fence.executor_domain
                        || receipt.generation_fence.old_generation != fence.executor_generation
                        || receipt.generation_fence.current_generation
                            <= receipt.generation_fence.old_generation
                        || receipt.generation_fence.evidence_digest
                            != receipt.generation_fence_digest
                    {
                        return Err(unavailable());
                    }
                    let expected = owner_generation_fence(
                        receipt.generation_fence.domain,
                        fence.run_id,
                        receipt.generation_fence.old_generation,
                        receipt.generation_fence.current_generation,
                    )?;
                    if expected != receipt.generation_fence {
                        return Err(unavailable());
                    }
                }
                _ => return Err(unavailable()),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        sync::{Arc, Mutex, atomic::Ordering},
    };

    use super::*;
    use crate::{RootKey, VaultKeyProvider};
    use floe_agent_contract::{
        AgentMessage, DependencyCoverage, MessageRole, TaskExecutionReceiptRef, TaskSnapshot,
        TaskState,
    };
    use floe_conversation::{
        CanonicalTurnIntent, ResumeChildAdmission, ResumeRequired, RunTerminal,
        SessionStartAdmission, SessionStore, StartSessionRequest, TurnAdmissionRequest, TurnInput,
        TurnMode,
    };
    use floe_conversation_contract::{
        AdmissionDisposition, AdmissionTarget, AgentIdentity, AgentInstanceId, AssignmentId,
        ConversationMessage, LogicalContributionId, MessageAdmissionRequest, MessageId,
        MessageOrigin,
    };
    use floe_experts::{
        ExpertAdmissionIdentity, ExpertExecutionSelection, PackageKind, PackageRef,
        TaskExecutionCommit, TaskRecord,
    };
    use floe_kernel::{AgentFailure, CommandId, TaskId};
    use uuid::Uuid;

    #[path = "conversation_core_owner_custody_tests.rs"]
    mod owner_custody;

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
            let path = std::env::temp_dir().join(format!("floe-core-custody-{}", Uuid::new_v4()));
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

    fn resolved_binding_interaction(
        record: &RunRecord,
    ) -> floe_conversation::ConversationInteraction {
        use floe_conversation::{
            BlockedReviewEvidence, ConversationInteraction, InteractionOrigin,
            InteractionRequirement, InteractionRequirementKind, InteractionResolutionCause,
            InteractionResolutionReceipt, InteractionState, OwnerResolutionReceipt,
            ReviewAuditRecord, ReviewedTarget, canonical_requirement_digest,
            canonical_target_digest, interaction_publication_id,
        };

        let review_ref = floe_experts::BindingReviewRef {
            id: Uuid::new_v4(),
            digest: [81; 32],
        };
        let execution = floe_agent_contract::TaskExecutionReceiptRef {
            execution: floe_agent_contract::TaskExecutionKey {
                task_id: TaskId::new(),
                execution_id: Uuid::new_v4(),
                executor_generation: 1,
            },
            task_revision: 2,
            journal_revision: 0,
            digest: [82; 32],
        };
        let origin = InteractionOrigin::Task {
            execution: execution.clone(),
            capability_call_id: None,
        };
        let target = ReviewedTarget::ExpertBinding(review_ref.clone());
        let requirement = InteractionRequirement {
            kind: InteractionRequirementKind::ConfigureExpertBinding,
            source_id: "floe.expert.binding".into(),
            connection_id: None,
            consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
            purpose: "configuration".into(),
            inline: false,
        };
        let owner_receipt = OwnerResolutionReceipt::ExpertBinding {
            receipt: floe_experts::BindingMutationReceipt {
                command_id: CommandId::new(),
                review_ref: review_ref.clone(),
                assignment_ref: Uuid::new_v4(),
                binding_revision: 1,
                registry_revision: 1,
                committed_at_unix_ms: 1,
            },
        };
        let receipt = InteractionResolutionReceipt {
            cause: InteractionResolutionCause::Refresh {
                command_id: Uuid::new_v4(),
            },
            owner_command_id: owner_receipt.command_id(),
            owner_operation_id: owner_receipt.operation_id(),
            owner_receipt,
            resolved_at_unix_ms: 1,
        };
        let audit = ReviewAuditRecord {
            person_id: record.person_id,
            device_id: record.device_id.clone(),
            session_id: record.session_id,
            run_id: record.run_id,
            executor_generation: record.executor_generation,
            operation_id: Uuid::new_v4(),
            evidence: BlockedReviewEvidence::ExpertBinding {
                execution,
                requirement_key: "calendar".into(),
                review: review_ref,
            },
        };
        let requirement_digest =
            canonical_requirement_digest(&requirement).expect("valid fixture requirement");
        let target_digest = canonical_target_digest(&target).expect("valid fixture target");
        let id =
            interaction_publication_id(record.run_id, &origin, &requirement_digest, &target_digest)
                .expect("valid fixture publication identity");
        let interaction = ConversationInteraction {
            id,
            person_id: record.person_id,
            session_id: record.session_id,
            origin_run_id: record.run_id,
            origin_turn_id: record.run_id.as_uuid(),
            origin,
            audit,
            kind: floe_agent_contract::UserInteractionKind::ExpertBinding,
            requirement,
            requirement_digest,
            target,
            target_digest,
            state: InteractionState::Resolved { receipt },
            revision: 2,
            created_at_unix_ms: 1,
            expires_at_unix_ms: 1 + floe_conversation::INTERACTION_PENDING_LIFETIME_MS,
        };
        interaction
            .validate()
            .expect("minimal resolved interaction fixture is valid");
        interaction
    }

    async fn compose_owner_core_run(
        vault: &EncryptedAgentVault<TestKeys>,
        intent: CoreComposedOwnerIntent,
        core_request: MessageAdmissionRequest,
    ) -> Result<CoreComposedRunAdmission, ConversationStoreFailure> {
        let mut connection = vault.connection().map_err(start_error)?;
        let (guard, transaction) = vault
            .journal_transaction(&mut connection)
            .await
            .map_err(start_error)?;
        let mut replay_checked = false;
        let mut prior_command = false;
        let result = vault
            .admit_conversation_run_with_core_input_on(
                &transaction,
                intent,
                core_request,
                &mut replay_checked,
                &mut prior_command,
            )
            .await;
        vault
            .finish_conversation_core_transaction(guard, transaction, result)
            .await
    }

    async fn compose_typed_recording(
        vault: &EncryptedAgentVault<TestKeys>,
        request: TypedConversationRecordingRequest,
    ) -> Result<RecordingReceipt, ConversationStoreFailure> {
        let mut connection = vault.connection().map_err(start_error)?;
        let (guard, transaction) = vault
            .journal_transaction(&mut connection)
            .await
            .map_err(start_error)?;
        let result = vault
            .record_typed_conversation_entry_on(&transaction, request)
            .await;
        vault
            .finish_conversation_core_transaction(guard, transaction, result)
            .await
    }

    fn typed_assistant_request(
        recorder: RecorderFence,
        text: &str,
    ) -> TypedConversationRecordingRequest {
        TypedConversationRecordingRequest {
            recorder: recorder.clone(),
            message_id: MessageId::new(),
            command_id: CommandId::new(),
            contribution_id: LogicalContributionId::new(),
            typed_entry_id: Uuid::new_v4(),
            typed_message: floe_conversation::AgentMessage::Assistant {
                turn_id: recorder.run_id.as_uuid(),
                text: text.to_owned(),
            },
        }
    }

    async fn stored_typed_link(
        vault: &EncryptedAgentVault<TestKeys>,
        person_id: PersonId,
        contribution_id: LogicalContributionId,
    ) -> Option<TypedTranscriptEvidenceLink> {
        let mut connection = vault.connection().expect("connect to typed link fixture");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .expect("start typed link read transaction");
        let result =
            typed_transcript_link_for_contribution_on(&transaction, person_id, contribution_id)
                .await
                .expect("resolve exact immutable typed link");
        transaction
            .commit()
            .await
            .expect("commit typed link read transaction");
        result
    }

    async fn stored_run_input_mapping(
        vault: &EncryptedAgentVault<TestKeys>,
        person_id: PersonId,
        run_id: RunId,
    ) -> Option<OwnerTranscriptInputMapping> {
        let mut connection = vault
            .connection()
            .expect("connect to owner input mapping fixture");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .expect("start owner input mapping read transaction");
        let result = owner_transcript_input_for_run_on(&transaction, person_id, run_id)
            .await
            .expect("resolve exact Run-to-input mapping");
        transaction
            .commit()
            .await
            .expect("commit owner input mapping read transaction");
        result
    }

    struct Scenario {
        // Keep the Vault before the root so its database and lock drop first.
        vault: Option<EncryptedAgentVault<TestKeys>>,
        keys: TestKeys,
        person_id: PersonId,
        identity: AgentIdentity,
        conversation_id: ConversationId,
        branch_id: ConversationBranchId,
        session_id: Uuid,
        session_revision: u64,
        executor_generation: u64,
        last_core_input_request: Option<(MessageAdmissionRequest, TranscriptReference)>,
        last_core_recorder: Option<RecorderOpenReceipt>,
        root: TestRoot,
    }

    impl Scenario {
        async fn new() -> Self {
            let root = TestRoot::new();
            let person_id = PersonId::new();
            let keys = TestKeys::default();
            let vault = EncryptedAgentVault::create(&root.0, person_id, keys.clone())
                .await
                .expect("create isolated encrypted Vault");
            let session = match vault
                .start_conversation_session(StartSessionRequest {
                    principal: person_id.to_string(),
                    command_id: CommandId::new(),
                })
                .await
                .expect("start owner Session")
            {
                SessionStartAdmission::Started(receipt) => receipt,
                other => panic!("unexpected Session admission: {other:?}"),
            };
            let activation = vault
                .activate_conversation_executor()
                .await
                .expect("activate owner executor fence");
            Self {
                vault: Some(vault),
                keys,
                person_id,
                identity: AgentIdentity {
                    person_id,
                    agent_instance_id: AgentInstanceId::new(),
                    assignment_id: AssignmentId::new(),
                    definition_id: "test.manager".into(),
                    definition_revision: 1,
                },
                conversation_id: ConversationId::new(),
                branch_id: ConversationBranchId::new(),
                session_id: session.session_id,
                session_revision: session.session_revision,
                executor_generation: activation.executor_generation,
                last_core_input_request: None,
                last_core_recorder: None,
                root,
            }
        }

        fn vault(&self) -> &EncryptedAgentVault<TestKeys> {
            self.vault.as_ref().expect("test Vault remains open")
        }

        async fn reopen(&mut self) {
            drop(self.vault.take());
            self.vault = Some(
                EncryptedAgentVault::open(&self.root.0, self.person_id, self.keys.clone())
                    .await
                    .expect("reopen isolated encrypted Vault"),
            );
        }

        fn owner_request(&self, text: &str) -> TurnAdmissionRequest {
            self.owner_request_for_session(self.session_id, self.session_revision, text)
        }

        fn owner_request_for_session(
            &self,
            session_id: Uuid,
            session_revision: u64,
            text: &str,
        ) -> TurnAdmissionRequest {
            let command_id = CommandId::new();
            let principal = self.person_id.to_string();
            let intent = CanonicalTurnIntent {
                session_id,
                expected_revision: session_revision,
                text: text.to_owned(),
                mode: TurnMode::New,
                retry_of: None,
            };
            TurnAdmissionRequest {
                expert_environment: floe_experts::RunExpertEnvironmentIdentity {
                    revision: 1,
                    digest: [41; 32],
                },
                run_id: RunId::new(),
                command_id,
                session_id,
                expected_session_revision: session_revision,
                principal: principal.clone(),
                device_id: "device-core-custody-test".into(),
                request_digest: intent.digest(&principal).expect("canonical owner digest"),
                mode: TurnMode::New,
                retry_of: None,
                input: TurnInput::NewMessage(AgentMessage {
                    message_id: command_id.as_uuid(),
                    role: MessageRole::User,
                    text: text.to_owned(),
                    call_id: None,
                    coverage: DependencyCoverage::Independent,
                }),
            }
        }

        fn core_input_request(&self, text: &str) -> MessageAdmissionRequest {
            MessageAdmissionRequest {
                target: AdmissionTarget::New {
                    conversation_id: self.conversation_id,
                    branch_id: self.branch_id,
                    identity: self.identity.clone(),
                },
                // MessageId and CommandId are independent transcript/owner keys.
                message: ConversationMessage {
                    message_id: MessageId::new(),
                    command_id: CommandId::new(),
                    origin: MessageOrigin::Person {
                        person_id: self.person_id,
                    },
                    text: text.to_owned(),
                    evidence: None,
                    task_id: None,
                },
            }
        }

        async fn admit_run_and_append_input(&mut self, text: &str) -> (RunRecord, AdmissionResult) {
            let owner_request = self.owner_request(text);
            let core_request = self.core_input_request(text);
            self.admit_run_and_bind_input(owner_request, core_request)
                .await
        }

        async fn admit_run_and_bind_input(
            &mut self,
            owner_request: TurnAdmissionRequest,
            core_request: MessageAdmissionRequest,
        ) -> (RunRecord, AdmissionResult) {
            let mut connection = self.vault().connection().expect("connect to test Vault");
            let (mut guard, transaction) = self
                .vault()
                .journal_transaction(&mut connection)
                .await
                .expect("start composed owner/Core transaction");
            let mut replay_checked = false;
            let mut prior_command = false;
            let admitted = self
                .vault()
                .admit_conversation_run_with_core_input_on(
                    &transaction,
                    CoreComposedOwnerIntent::Turn(owner_request),
                    core_request.clone(),
                    &mut replay_checked,
                    &mut prior_command,
                )
                .await
                .expect("admit owner Run and bind its Core input atomically");
            let CoreComposedRunAdmission::Admitted {
                record,
                input,
                recorder,
            } = admitted
            else {
                panic!("unexpected superseded result for ordinary Run admission");
            };
            assert_eq!(recorder.fence.run_id, record.run_id);
            assert_eq!(recorder.fence.input, input.receipt.transcript);
            let next_revision = record.session_revision;
            transaction
                .commit()
                .await
                .expect("commit owner and Core admission together");
            guard.settled();
            drop(guard);
            self.session_revision = next_revision;
            self.last_core_input_request = Some((core_request, input.receipt.transcript));
            self.last_core_recorder = Some(recorder);
            (record, input)
        }

        async fn admit_unbound_owner_run(&mut self, request: TurnAdmissionRequest) -> RunRecord {
            let mut connection = self.vault().connection().expect("connect to test Vault");
            let (mut guard, transaction) = self
                .vault()
                .journal_transaction(&mut connection)
                .await
                .expect("start deliberately unbound owner transaction");
            let mut replay_checked = false;
            let mut prior_command = false;
            let admitted = self
                .vault()
                .admit_conversation_turn_on(
                    &transaction,
                    request,
                    &mut replay_checked,
                    &mut prior_command,
                )
                .await
                .expect("admit owner Run without a Core binding");
            let super::super::VaultConversationAdmission::Created { record, .. } = admitted else {
                panic!("unbound fixture did not create a Run");
            };
            transaction
                .commit()
                .await
                .expect("commit deliberately unbound owner Run");
            guard.settled();
            drop(guard);
            self.session_revision = record.session_revision;
            record
        }

        async fn admit_owner_run_reusing_input(
            &mut self,
            parent: &RunRecord,
            text: &str,
        ) -> (RunRecord, AdmissionResult) {
            let continuation = floe_conversation::project_run_receipt(parent.clone())
                .expect("project terminal parent receipt")
                .continuation()
                .expect("BudgetExceeded parent provides a Continue reference");
            let command_id = CommandId::new();
            let mode = TurnMode::Continue(continuation);
            let request_digest = CanonicalTurnIntent {
                session_id: parent.session_id,
                expected_revision: parent.session_revision,
                text: text.to_owned(),
                mode: mode.clone(),
                retry_of: None,
            }
            .digest(&self.person_id.to_string())
            .expect("canonical Continue intent digest");
            let owner_request = TurnAdmissionRequest {
                expert_environment: parent.expert_environment,
                run_id: RunId::new(),
                command_id,
                session_id: parent.session_id,
                expected_session_revision: parent.session_revision,
                principal: self.person_id.to_string(),
                device_id: parent.device_id.clone(),
                request_digest,
                mode,
                retry_of: None,
                input: TurnInput::ExistingMessage {
                    message_id: parent.user_message_id,
                },
            };
            let (mut core_request, input_reference) = self
                .last_core_input_request
                .clone()
                .expect("a retained Core input was previously admitted");
            let AdmissionTarget::New {
                identity,
                conversation_id,
                branch_id,
            } = core_request.target.clone()
            else {
                panic!("fixture retains the original input admission target");
            };
            let previous = core_request.message.clone();
            core_request.target = AdmissionTarget::AppendToExisting {
                reference: ConversationReference {
                    identity,
                    conversation_id,
                    branch_id,
                    head_revision: input_reference.sequence,
                },
            };
            debug_assert_eq!(core_request.message, previous);
            self.admit_run_and_bind_input(owner_request, core_request)
                .await
        }

        fn start_request(
            &self,
            run: &RunRecord,
            input: TranscriptReference,
        ) -> RecorderStartRequest {
            RecorderStartRequest {
                identity: self.identity.clone(),
                conversation_id: self.conversation_id,
                branch_id: self.branch_id,
                run_id: run.run_id,
                input,
                executor_domain: ExecutorDomain::HostRun,
                executor_generation: self.executor_generation,
                execution_task: None,
            }
        }

        fn recording_request(&self, fence: RecorderFence, text: &str) -> RecordingRequest {
            RecordingRequest {
                recorder: fence,
                message: ConversationMessage {
                    message_id: MessageId::new(),
                    command_id: CommandId::new(),
                    origin: MessageOrigin::Agent {
                        agent_instance_id: self.identity.agent_instance_id,
                    },
                    text: text.to_owned(),
                    evidence: None,
                    task_id: None,
                },
                contribution_id: LogicalContributionId::new(),
                producing_task: None,
            }
        }

        async fn append_owner_event(
            &self,
            run_id: RunId,
            event: floe_agent_contract::JournalEvent,
        ) {
            let kind = super::super::conversations::journal_kind(&event);
            let payload = serde_json::to_string(&event).expect("encode owner journal fixture");
            self.vault()
                .append_conversation_journal(run_id, kind, &payload)
                .await
                .expect("append owner journal fixture event");
        }

        async fn complete_owner_run(&mut self, run: &RunRecord) -> RunRecord {
            use floe_agent_contract::{
                BatchCursor, EngineStep, JournalEvent, ModelBindingDigest, ModelBudgetProfile,
                ModelCapabilities, ModelSelectionCommitment, ModelStep, ModelUsage,
                PreparedModelPlan, ProcessingBoundary, ValidatedModelBatch,
            };

            let text = "Synthetic completed Run for Resume custody.".to_owned();
            let attempt_id = Uuid::new_v4();
            let projection_ref = floe_agent_contract::ProjectionRef::new();
            let batch_id = Uuid::new_v4();
            self.append_owner_event(
                run.run_id,
                JournalEvent::ModelIntent {
                    attempt_id,
                    parent_task_id: None,
                    reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                        tokens: 128,
                        cost_micros: 128,
                    },
                    projection_ref,
                    plan: PreparedModelPlan {
                        operation_id: Uuid::new_v4(),
                        principal: self.person_id.to_string(),
                        device_id: run.device_id.clone(),
                        purpose: "everyday_assistance".into(),
                        consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
                        capabilities: ModelCapabilities::chat(),
                        boundary: ProcessingBoundary::Device,
                        binding_digest: ModelBindingDigest([71; 32]),
                        selection_commitment: Some(ModelSelectionCommitment([72; 32])),
                        budget_profile: Some(ModelBudgetProfile::unknown()),
                    },
                },
            )
            .await;
            self.append_owner_event(
                run.run_id,
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: ModelUsage::default(),
                    accounting: floe_execution::budget::ModelAccounting {
                        observed_tokens: None,
                        observed_cost_micros: None,
                        unknown_tokens: true,
                        unknown_cost: true,
                    },
                },
            )
            .await;
            let batch = ValidatedModelBatch {
                execution_id: run.run_id.as_uuid(),
                attempt_id,
                projection_ref,
                batch_id,
                steps: vec![ModelStep::Answer {
                    text: text.clone(),
                    artifacts: Vec::new(),
                }],
                catalog_revision: run.expert_environment.revision,
                tool_revisions: vec![],
                agent_revisions: vec![],
                projection_coverage: DependencyCoverage::Independent,
                delegation_context: None,
            };
            self.append_owner_event(
                run.run_id,
                JournalEvent::ValidatedBatch {
                    batch: batch.clone(),
                },
            )
            .await;
            self.append_owner_event(
                run.run_id,
                JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id,
                        next_step_index: 0,
                    },
                },
            )
            .await;
            self.append_owner_event(
                run.run_id,
                JournalEvent::Output {
                    text: text.clone(),
                    artifacts: Vec::new(),
                },
            )
            .await;
            let completed = self
                .vault()
                .finish_conversation_run(
                    run.run_id,
                    run.aggregate_revision,
                    RunTerminal {
                        state: RunState::Completed,
                        output: Some(text.clone()),
                        steps: vec![EngineStep::Answer {
                            text,
                            artifacts: Vec::new(),
                        }],
                        coverage: DependencyCoverage::Independent,
                        issue: None,
                        blocked: None,
                        interactions: vec![],
                    },
                )
                .await
                .expect("finish synthetic owner Run through the Vault API");
            if let Some(recorder) = self
                .last_core_recorder
                .as_ref()
                .filter(|recorder| recorder.fence.run_id == run.run_id)
            {
                self.vault()
                    .close_conversation_recorder(recorder.fence.clone())
                    .await
                    .expect("close the completed origin recorder through Vault");
            }
            self.session_revision = completed.session_revision;
            completed
        }

        async fn create_pending_resume(&mut self, origin: &RunRecord) -> ResumeRequired {
            let interaction = resolved_binding_interaction(origin);
            let mut connection = self.vault().connection().expect("connect to test Vault");
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .expect("begin resolved interaction fixture transaction");
            super::super::conversation_interactions::insert_interaction(&transaction, &interaction)
                .await
                .expect("store minimal resolved interaction fixture");
            transaction
                .commit()
                .await
                .expect("commit resolved interaction fixture");
            drop(connection);
            let completed = self.complete_owner_run(origin).await;
            let actor = floe_kernel::OwnerActor {
                person_id: self.person_id,
                device_id: origin.device_id.clone(),
                runtime_epoch: 1,
            };
            self.vault()
                .pending_conversation_resume_request(&actor, completed.run_id)
                .await
                .expect("read pending Resume request through Vault")
                .expect("resolved interaction queues one pending Resume")
        }

        fn resume_child_admission(
            &self,
            pending: ResumeRequired,
            origin: &RunRecord,
            text: &str,
        ) -> ResumeChildAdmission {
            let mode = TurnMode::Resume(floe_conversation::InteractionResumeRef {
                origin_run_id: pending.origin_run_id,
                lineage: pending.lineage,
            });
            let principal = self.person_id.to_string();
            let child = TurnAdmissionRequest {
                expert_environment: origin.expert_environment,
                run_id: RunId::new(),
                command_id: CommandId::new(),
                session_id: pending.session_id,
                expected_session_revision: pending.expected_session_revision,
                principal: principal.clone(),
                device_id: pending.device_id.clone(),
                request_digest: CanonicalTurnIntent {
                    session_id: pending.session_id,
                    expected_revision: pending.expected_session_revision,
                    text: text.to_owned(),
                    mode: mode.clone(),
                    retry_of: None,
                }
                .digest(&principal)
                .expect("canonical Resume digest"),
                mode,
                retry_of: None,
                input: TurnInput::ExistingMessage {
                    message_id: pending.user_message_id,
                },
            };
            ResumeChildAdmission {
                request: pending,
                child,
            }
        }

        fn resume_core_request(&self) -> (MessageAdmissionRequest, TranscriptReference) {
            let (mut request, input) = self
                .last_core_input_request
                .clone()
                .expect("composed origin has a retained Core input binding");
            let (identity, conversation_id, branch_id) = match &request.target {
                AdmissionTarget::New {
                    identity,
                    conversation_id,
                    branch_id,
                } => (identity.clone(), *conversation_id, *branch_id),
                _ => panic!("origin input was admitted as a fresh New entry"),
            };
            request.target = AdmissionTarget::AppendToExisting {
                reference: ConversationReference {
                    identity,
                    conversation_id,
                    branch_id,
                    head_revision: input.sequence,
                },
            };
            (request, input)
        }

        async fn admit_resume_and_bind_input(
            &mut self,
            request: ResumeChildAdmission,
            core_request: MessageAdmissionRequest,
        ) -> CoreComposedRunAdmission {
            let mut connection = self.vault().connection().expect("connect to test Vault");
            let (mut guard, transaction) = self
                .vault()
                .journal_transaction(&mut connection)
                .await
                .expect("start composed Resume transaction");
            let mut replay_checked = false;
            let mut prior_command = false;
            let result = self
                .vault()
                .admit_conversation_run_with_core_input_on(
                    &transaction,
                    CoreComposedOwnerIntent::Resume(request),
                    core_request.clone(),
                    &mut replay_checked,
                    &mut prior_command,
                )
                .await;
            let admitted = self
                .vault()
                .finish_conversation_core_transaction(guard, transaction, result)
                .await
                .expect("compose and commit owner Resume, Core binding and recorder together");
            if let CoreComposedRunAdmission::Admitted {
                record,
                input,
                recorder,
            } = &admitted
            {
                self.session_revision = record.session_revision;
                self.last_core_input_request = Some((core_request, input.receipt.transcript));
                self.last_core_recorder = Some(recorder.clone());
            }
            admitted
        }

        async fn terminal_task_receipts(
            &self,
            run: &RunRecord,
            executor_generation: u64,
        ) -> Vec<TaskExecutionReceiptRef> {
            use floe_agent_contract::InvocationKey;
            use floe_agent_contract::{
                AgentContext, DelegationExecutionContext, DelegationRequest, JournalEvent,
                ModelBindingDigest, ModelBudgetProfile, ModelCapabilities,
                ModelSelectionCommitment, PinnedAgentRevision, PreparedModelPlan,
                ProcessingBoundary, ProjectionRef, TaskExecutionEvidence, ValidatedModelBatch,
            };
            use floe_kernel::TaskId;

            let agent_context = AgentContext {
                projection_version: 1,
                persona: None,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            };
            let execution_context = DelegationExecutionContext {
                session_id: run.session_id,
                device_id: run.device_id.clone(),
                agent_context,
                max_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
                projection_coverage: DependencyCoverage::Independent,
            };
            let execution_id = run.run_id.as_uuid();
            let attempt_id = Uuid::new_v4();
            let projection_ref = ProjectionRef::new();
            let batch_id = Uuid::new_v4();
            self.append_owner_event(
                run.run_id,
                JournalEvent::ModelIntent {
                    attempt_id,
                    parent_task_id: None,
                    reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                        tokens: 128,
                        cost_micros: 128,
                    },
                    projection_ref,
                    plan: PreparedModelPlan {
                        operation_id: Uuid::new_v4(),
                        principal: self.person_id.to_string(),
                        device_id: run.device_id.clone(),
                        purpose: "everyday_assistance".into(),
                        consumer: floe_conversation::CONVERSATION_CONSUMER.into(),
                        capabilities: ModelCapabilities::chat(),
                        boundary: ProcessingBoundary::Device,
                        binding_digest: ModelBindingDigest([61; 32]),
                        selection_commitment: Some(ModelSelectionCommitment([62; 32])),
                        budget_profile: Some(ModelBudgetProfile::unknown()),
                    },
                },
            )
            .await;
            self.append_owner_event(
                run.run_id,
                JournalEvent::ModelResult {
                    attempt_id,
                    usage: floe_agent_contract::ModelUsage::default(),
                    accounting: floe_execution::budget::ModelAccounting {
                        observed_tokens: None,
                        observed_cost_micros: None,
                        unknown_tokens: true,
                        unknown_cost: true,
                    },
                },
            )
            .await;
            let task_messages = ["Task A input", "Task B input"];
            let batch = ValidatedModelBatch {
                execution_id,
                attempt_id,
                projection_ref,
                batch_id,
                steps: task_messages
                    .iter()
                    .map(|message| floe_agent_contract::ModelStep::Delegate {
                        agent_id: "fixture.expert".into(),
                        definition_revision: 1,
                        message: (*message).into(),
                        context_refs: vec![],
                    })
                    .collect(),
                catalog_revision: run.expert_environment.revision,
                tool_revisions: vec![],
                agent_revisions: vec![PinnedAgentRevision {
                    agent_id: "fixture.expert".into(),
                    definition_revision: 1,
                }],
                projection_coverage: DependencyCoverage::Independent,
                delegation_context: Some(execution_context.clone()),
            };
            self.append_owner_event(
                run.run_id,
                JournalEvent::ValidatedBatch {
                    batch: batch.clone(),
                },
            )
            .await;
            self.append_owner_event(
                run.run_id,
                JournalEvent::BatchProgress {
                    cursor: floe_agent_contract::BatchCursor {
                        batch_id,
                        next_step_index: 0,
                    },
                },
            )
            .await;

            let mut references = Vec::new();
            for (ordinal, message) in task_messages.iter().enumerate() {
                let ordinal = ordinal as u32;
                let task_id = TaskId::from_uuid(Uuid::new_v5(
                    &execution_id,
                    format!("{execution_id}:{batch_id}:{ordinal}:task").as_bytes(),
                ))
                .expect("derive stable TaskId");
                let invocation_key = InvocationKey::from_uuid(Uuid::new_v5(
                    &execution_id,
                    format!("{execution_id}:{batch_id}:{ordinal}:delegation").as_bytes(),
                ))
                .expect("derive stable delegation invocation");
                let request = DelegationRequest {
                    task_id,
                    parent_run_id: Some(run.run_id.as_uuid()),
                    principal: self.person_id.to_string(),
                    invocation_key,
                    selected_agent_id: "fixture.expert".into(),
                    selected_definition_revision: 1,
                    message: (*message).into(),
                    context_refs: vec![],
                    execution_context: execution_context.clone(),
                };
                self.append_owner_event(
                    run.run_id,
                    JournalEvent::DelegationIntent {
                        request: request.clone(),
                    },
                )
                .await;
                let snapshot = TaskSnapshot {
                    task_id,
                    parent_run_id: request.parent_run_id,
                    principal: request.principal.clone(),
                    agent_id: request.selected_agent_id.clone(),
                    definition_revision: request.selected_definition_revision,
                    state: TaskState::Submitted,
                    result: None,
                    artifacts: vec![],
                    coverage: DependencyCoverage::Unknown,
                    issue: None,
                    blockage: None,
                };
                let empty_journal_digest = Sha256::digest(
                    serde_json::to_vec(&(
                        "floe.execution-journal.sha256.v1",
                        Vec::<floe_agent_contract::JournalEntry>::new(),
                    ))
                    .expect("encode empty Task journal"),
                )
                .into();
                let proposed = TaskRecord {
                    snapshot,
                    admission: ExpertAdmissionIdentity {
                        registry_instance_id: Uuid::new_v4(),
                        assignment_id: Uuid::new_v4(),
                        installation_id: Uuid::new_v4(),
                        package: PackageRef {
                            kind: PackageKind::Expert,
                            id: "fixture.expert".into(),
                            version: "1.0.0".into(),
                        },
                        definition_revision: 1,
                    },
                    selection: ExpertExecutionSelection::without_requirements(1)
                        .expect("build empty binding fixture"),
                    invocation_key,
                    request_digest: floe_agent_contract::delegation_request_digest(&request),
                    aggregate_revision: 1,
                    executor_generation,
                    execution_id: Uuid::new_v4(),
                    device_id: "device-core-custody-test".into(),
                    catalog_revision: 1,
                    model_allowance: floe_execution::budget::ModelReservationCeiling {
                        tokens: 128,
                        cost_micros: 128,
                    },
                    maximum_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
                    journal_revision: 0,
                    journal_digest: empty_journal_digest,
                    receipt: None,
                };
                self.vault()
                    .admit_task(proposed.clone())
                    .await
                    .expect("admit Task for evidence fixture");
                let working = self
                    .vault()
                    .compare_and_swap_task(
                        task_id,
                        proposed.aggregate_revision,
                        executor_generation,
                        TaskSnapshot {
                            state: TaskState::Working,
                            ..proposed.snapshot.clone()
                        },
                    )
                    .await
                    .expect("start Task execution fixture");
                let receipt = self
                    .vault()
                    .settle_task_execution(TaskExecutionCommit {
                        execution: working.execution(),
                        expected_task_revision: working.aggregate_revision,
                        expected_journal_revision: working.journal_revision,
                        terminal: TaskSnapshot {
                            state: TaskState::Interrupted,
                            issue: Some(AgentFailure::Interrupted),
                            ..working.snapshot
                        },
                        settlement: None,
                    })
                    .await
                    .expect("settle exact terminal Task execution receipt");
                let task_receipt = floe_agent_contract::TaskReceipt {
                    task_id,
                    snapshot: receipt.snapshot.clone(),
                    replay: None,
                    execution: TaskExecutionEvidence::Admitted(receipt.clone()),
                };
                self.append_owner_event(
                    run.run_id,
                    JournalEvent::DelegationResult {
                        receipt: Box::new(task_receipt),
                    },
                )
                .await;
                references.push(receipt.reference.clone());
                if ordinal == 0 {
                    self.append_owner_event(
                        run.run_id,
                        JournalEvent::BatchProgress {
                            cursor: floe_agent_contract::BatchCursor {
                                batch_id,
                                next_step_index: 1,
                            },
                        },
                    )
                    .await;
                }
            }
            self.append_owner_event(
                run.run_id,
                JournalEvent::BatchProgress {
                    cursor: floe_agent_contract::BatchCursor {
                        batch_id,
                        next_step_index: task_messages.len() as u32,
                    },
                },
            )
            .await;
            references
        }

        async fn cancel_owner_run(&mut self, run: &RunRecord) -> RunRecord {
            let next = self
                .vault()
                .finish_conversation_run(
                    run.run_id,
                    run.aggregate_revision,
                    RunTerminal::from_failure(AgentFailure::Cancelled),
                )
                .await
                .expect("write normal owner terminal receipt");
            self.session_revision = next.session_revision;
            if let Some(recorder) = self
                .last_core_recorder
                .as_ref()
                .filter(|recorder| recorder.fence.run_id == run.run_id)
            {
                self.vault()
                    .close_conversation_recorder(recorder.fence.clone())
                    .await
                    .expect("close the cancelled Run's composed recorder");
            }
            next
        }

        async fn budget_exceeded_owner_run(&mut self, run: &RunRecord) -> RunRecord {
            let next = self
                .vault()
                .finish_conversation_run(
                    run.run_id,
                    run.aggregate_revision,
                    RunTerminal::from_failure(AgentFailure::BudgetExceeded),
                )
                .await
                .expect("finish owner Run with an eligible Continue failure");
            self.session_revision = next.session_revision;
            next
        }
    }

    async fn table_count(connection: &turso::Connection, table: &str) -> i64 {
        let query = format!("SELECT count(*) FROM {table}");
        connection
            .query(&query, ())
            .await
            .expect("query fixture row count")
            .next()
            .await
            .expect("read fixture row count")
            .expect("fixture count row")
            .get::<i64>(0)
            .expect("integer fixture count")
    }

    async fn table_count_in_transaction(transaction: &Transaction<'_>, table: &str) -> i64 {
        let query = format!("SELECT count(*) FROM {table}");
        transaction
            .query(&query, ())
            .await
            .expect("query transactional fixture row count")
            .next()
            .await
            .expect("read transactional fixture row count")
            .expect("transactional fixture count row")
            .get::<i64>(0)
            .expect("integer transactional fixture count")
    }

    async fn live_catalog_snapshot(
        connection: &turso::Connection,
    ) -> Vec<(String, String, String)> {
        let mut rows = connection
            .query(
                "SELECT type, name, sql FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' ORDER BY name",
                (),
            )
            .await
            .expect("read live schema catalog");
        let mut snapshot = Vec::new();
        while let Some(row) = rows.next().await.expect("read live schema row") {
            snapshot.push((
                row.get::<String>(0).expect("schema type"),
                row.get::<String>(1).expect("schema name"),
                row.get::<String>(2).expect("schema DDL"),
            ));
        }
        snapshot
    }

    async fn session_storage_snapshot(
        connection: &turso::Connection,
        session_id: Uuid,
    ) -> (i64, String) {
        let mut rows = connection
            .query(
                "SELECT revision, payload FROM agent_sessions WHERE id = ?",
                [session_id.to_string()],
            )
            .await
            .expect("read owner Session snapshot");
        let row = rows
            .next()
            .await
            .expect("read owner Session row")
            .expect("owner Session exists");
        (
            row.get::<i64>(0).expect("Session revision type"),
            row.get::<String>(1).expect("Session payload type"),
        )
    }

    async fn stored_resume_state(
        connection: &turso::Connection,
        origin_run_id: RunId,
    ) -> Option<String> {
        let mut rows = connection
            .query(
                "SELECT state FROM agent_conversation_resume_requests WHERE origin_run_id = ?",
                [origin_run_id.as_uuid().to_string()],
            )
            .await
            .expect("read persisted Resume request state");
        rows.next()
            .await
            .expect("read Resume state row")
            .map(|row| row.get::<String>(0).expect("Resume state text"))
    }

    async fn catalog_snapshot(path: &Path, key: &RootKey) -> Vec<(String, String, String)> {
        let database = super::super::encrypted_database(path, key)
            .await
            .expect("open raw encrypted fixture database");
        let connection = database.connect().expect("connect to raw fixture database");
        let mut rows = connection
            .query(
                "SELECT type, name, sql FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' ORDER BY name",
                (),
            )
            .await
            .expect("read raw schema snapshot");
        let mut snapshot = Vec::new();
        while let Some(row) = rows.next().await.expect("read raw schema row") {
            snapshot.push((
                row.get::<String>(0).expect("schema type"),
                row.get::<String>(1).expect("schema name"),
                row.get::<String>(2).expect("schema DDL"),
            ));
        }
        snapshot
    }

    #[tokio::test]
    async fn resolved_interaction_resume_admits_one_child_input_and_recorder_and_replays_after_reopen()
     {
        let mut scenario = Scenario::new().await;
        let (origin, input) = scenario
            .admit_run_and_append_input("resume retained input")
            .await;
        let pending = scenario.create_pending_resume(&origin).await;
        assert_eq!(pending.origin_run_id, origin.run_id);
        assert_eq!(pending.user_message_id, origin.user_message_id);
        let next_generation = scenario
            .vault()
            .activate_conversation_executor()
            .await
            .expect("advance current owner generation before explicit Resume")
            .executor_generation;
        let request = scenario.resume_child_admission(pending, &origin, "resume retained input");
        let (core_request, retained_reference) = scenario.resume_core_request();
        assert_eq!(retained_reference, input.receipt.transcript);
        assert_eq!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Turn(request.child.clone()),
                core_request.clone(),
            )
            .await,
            Err(ConversationStoreFailure::UnsupportedOwnerIntent),
            "an unqualified Turn Resume intent still fails before owner writes"
        );

        let first = scenario
            .admit_resume_and_bind_input(request.clone(), core_request.clone())
            .await;
        let CoreComposedRunAdmission::Admitted {
            record: child,
            input: first_input,
            recorder: first_recorder,
        } = first
        else {
            panic!("fresh Resume was unexpectedly superseded");
        };
        assert_eq!(child.resume_of, Some(origin.run_id));
        assert_eq!(child.resume_lineage, 1);
        assert_eq!(child.executor_generation, next_generation);
        assert_eq!(child.continuation_of, None);
        assert_eq!(child.continuation_level, 0);
        assert_eq!(child.retry_of, None);
        assert_eq!(first_input.disposition, AdmissionDisposition::Replayed);
        assert_eq!(first_input.receipt.transcript, retained_reference);
        assert_eq!(first_recorder.fence.run_id, child.run_id);
        assert_eq!(first_recorder.fence.input, retained_reference);
        assert_eq!(
            first_recorder.fence.executor_generation,
            child.executor_generation
        );
        let original_mapping =
            stored_run_input_mapping(scenario.vault(), scenario.person_id, origin.run_id)
                .await
                .expect("origin Run keeps its immutable Core input mapping");
        assert_eq!(
            stored_run_input_mapping(scenario.vault(), scenario.person_id, child.run_id).await,
            Some(original_mapping),
            "Resume child points to the origin's retained input mapping"
        );

        let connection = scenario
            .vault()
            .connection()
            .expect("connect to test Vault");
        assert_eq!(table_count(&connection, "agent_conversation_runs").await, 2);
        assert_eq!(
            table_count(&connection, "agent_conversation_resume_slots").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_entries").await,
            1,
            "Resume references the retained source input without appending a duplicate"
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_input_receipts").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
            2
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_open_receipts").await,
            2,
            "the New origin and Resume child each have one immutable recorder receipt"
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_owner_transcript_inputs_v1").await,
            1,
            "Resume keeps the original owner-to-input mapping"
        );
        assert_eq!(
            table_count(
                &connection,
                "agent_conversation_owner_transcript_run_inputs_v1"
            )
            .await,
            2,
            "origin and Resume child both point to that mapping"
        );
        drop(connection);
        let session = SessionStore::load(scenario.vault(), scenario.person_id, scenario.session_id)
            .await
            .expect("reload Session after Resume");
        let user_messages = session
            .messages
            .iter()
            .filter(|message| matches!(message, floe_conversation::AgentMessage::User { .. }))
            .count();
        assert_eq!(user_messages, 1, "Resume never adds a duplicate User entry");
        assert!(
            scenario
                .vault()
                .conversation_journal(child.run_id)
                .await
                .expect("read newly admitted Resume journal")
                .is_empty(),
            "custody admission performs no model or effect dispatch"
        );

        let mut changed_identity = core_request.clone();
        let AdmissionTarget::AppendToExisting { reference } = &mut changed_identity.target else {
            panic!("Resume input points at the retained Core conversation");
        };
        reference.identity.definition_revision += 1;
        assert_eq!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Resume(request.clone()),
                changed_identity,
            )
            .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
            "a claimed child cannot be retargeted to another Core identity"
        );
        let mut changed_group = request.clone();
        changed_group.request.group_digest[0] ^= 0x80;
        assert_eq!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Resume(changed_group),
                core_request.clone(),
            )
            .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
            "a claimed slot rejects a conflicting persisted group digest"
        );

        scenario.reopen().await;
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to advance mutable Session after claim");
        let (mut guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start controlled post-claim Session change");
        let mut session = scenario
            .vault()
            .session_on(&transaction, scenario.session_id)
            .await
            .expect("load claimed child Session");
        let prior_revision = session.revision;
        session.revision += 1;
        let changed = transaction
            .execute(
                "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                (
                    integer(session.revision).expect("encode next Session revision"),
                    scenario
                        .vault()
                        .payload(&session)
                        .expect("encode changed Session"),
                    session.id.to_string(),
                    integer(prior_revision).expect("encode prior Session revision"),
                ),
            )
            .await
            .expect("persist post-claim Session change");
        assert_eq!(changed, 1);
        transaction
            .commit()
            .await
            .expect("commit post-claim Session change");
        guard.settled();
        drop(guard);
        drop(connection);
        let later_generation = scenario
            .vault()
            .activate_conversation_executor()
            .await
            .expect("advance current generation after Session and child recorder changed")
            .executor_generation;
        assert!(later_generation > child.executor_generation);
        let replay = compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Resume(request),
            core_request,
        )
        .await
        .expect("exact Resume replay recovers after reopen and generation change");
        let CoreComposedRunAdmission::Admitted {
            record: replayed_child,
            input: replayed_input,
            recorder: replayed_recorder,
        } = replay
        else {
            panic!("claimed Resume replay was unexpectedly superseded");
        };
        assert_eq!(replayed_child, child);
        assert_eq!(replayed_input.disposition, AdmissionDisposition::Replayed);
        assert_eq!(replayed_input.receipt, first_input.receipt);
        assert_eq!(replayed_recorder, first_recorder);
        let connection = scenario
            .vault()
            .connection()
            .expect("reconnect after replay");
        assert_eq!(table_count(&connection, "agent_conversation_runs").await, 2);
        assert_eq!(
            table_count(&connection, "agent_conversation_resume_slots").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_entries").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_open_receipts").await,
            2
        );
    }

    #[tokio::test]
    async fn composed_resume_commit_ack_loss_replays_one_committed_child_slot_and_recorder() {
        let mut scenario = Scenario::new().await;
        let (origin, input) = scenario
            .admit_run_and_append_input("Resume commit acknowledgement loss")
            .await;
        let pending = scenario.create_pending_resume(&origin).await;
        let generation = scenario
            .vault()
            .activate_conversation_executor()
            .await
            .expect("advance current generation before explicit Resume")
            .executor_generation;
        let request =
            scenario.resume_child_admission(pending, &origin, "Resume commit acknowledgement loss");
        let (core_request, retained_reference) = scenario.resume_core_request();
        assert_eq!(retained_reference, input.receipt.transcript);
        let child_run_id = request.child.run_id;

        scenario
            .vault()
            .conversation_core_ack_loss
            .store(true, Ordering::Release);
        assert_eq!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Resume(request.clone()),
                core_request.clone(),
            )
            .await,
            Err(ConversationStoreFailure::OutcomeUnknown),
            "the composed Resume committed, then lost its acknowledgement"
        );

        scenario.reopen().await;
        let child = scenario
            .vault()
            .conversation_run(child_run_id)
            .await
            .expect("read child after lost acknowledgement")
            .expect("the child committed with the Resume slot");
        assert_eq!(child.resume_of, Some(origin.run_id));
        assert_eq!(child.resume_lineage, 1);
        assert_eq!(child.executor_generation, generation);
        assert_eq!(child.continuation_of, None);
        assert_eq!(child.retry_of, None);

        let tables = [
            "agent_conversation_runs",
            "agent_conversation_resume_slots",
            "agent_conversation_core_v3_entries",
            "agent_conversation_core_v3_input_receipts",
            "agent_conversation_core_v3_owner_bindings",
            "agent_conversation_core_v3_open_receipts",
            "agent_conversation_core_v3_active_recorders",
        ];
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect after lost Resume acknowledgement");
        let mut committed_counts = Vec::new();
        for table in tables {
            committed_counts.push(table_count(&connection, table).await);
        }
        assert_eq!(committed_counts, [2, 1, 1, 1, 2, 2, 1]);
        assert_eq!(
            stored_resume_state(&connection, origin.run_id)
                .await
                .as_deref(),
            Some("claimed")
        );
        let mut slots = connection
            .query(
                "SELECT child_run_id FROM agent_conversation_resume_slots WHERE origin_run_id = ?",
                [origin.run_id.as_uuid().to_string()],
            )
            .await
            .expect("read committed Resume slot");
        let slot_child = slots
            .next()
            .await
            .expect("read Resume slot row")
            .expect("one Resume slot exists")
            .get::<String>(0)
            .expect("Resume slot child ID is text");
        assert_eq!(slot_child, child_run_id.as_uuid().to_string());
        assert!(
            slots
                .next()
                .await
                .expect("check for duplicate Resume slots")
                .is_none()
        );
        drop(slots);

        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .expect("begin read-only receipt verification transaction");
        let scope = Scope::from_identity(
            scenario.person_id,
            &scenario.identity,
            scenario.conversation_id,
            scenario.branch_id,
        );
        let (stored_input, stored_message) =
            input_receipt_on(&transaction, scope, core_request.message.message_id)
                .await
                .expect("read committed retained Core input")
                .expect("the source input receipt remains stored");
        assert_eq!(stored_input.receipt.transcript, retained_reference);
        assert_eq!(stored_message, core_request.message);
        let original_recorder = open_receipt_on(&transaction, scenario.person_id, child_run_id)
            .await
            .expect("read committed immutable recorder receipt")
            .expect("Resume recorder receipt committed with the child");
        assert_eq!(original_recorder.fence.run_id, child_run_id);
        assert_eq!(original_recorder.fence.input, retained_reference);
        assert_eq!(original_recorder.fence.executor_generation, generation);
        transaction
            .commit()
            .await
            .expect("finish receipt verification transaction");
        drop(connection);

        assert!(
            scenario
                .vault()
                .conversation_journal(child_run_id)
                .await
                .expect("read child journal after lost acknowledgement")
                .is_empty(),
            "ACK recovery does not create model or dispatch journal entries"
        );
        let session = SessionStore::load(scenario.vault(), scenario.person_id, scenario.session_id)
            .await
            .expect("reload Session after committed Resume");
        assert_eq!(
            session
                .messages
                .iter()
                .filter(|message| matches!(message, floe_conversation::AgentMessage::User { .. }))
                .count(),
            1,
            "Resume ACK recovery never appends a duplicate User entry"
        );

        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to change Session after committed Resume");
        let (mut guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start controlled post-claim Session change");
        let mut session = scenario
            .vault()
            .session_on(&transaction, scenario.session_id)
            .await
            .expect("load committed child Session");
        let prior_revision = session.revision;
        session.revision += 1;
        let changed = transaction
            .execute(
                "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                (
                    integer(session.revision).expect("encode next Session revision"),
                    scenario
                        .vault()
                        .payload(&session)
                        .expect("encode changed Session"),
                    session.id.to_string(),
                    integer(prior_revision).expect("encode prior Session revision"),
                ),
            )
            .await
            .expect("persist post-claim Session revision");
        assert_eq!(changed, 1);
        transaction
            .commit()
            .await
            .expect("commit controlled Session revision change");
        guard.settled();
        drop(guard);
        drop(connection);
        let later_generation = scenario
            .vault()
            .activate_conversation_executor()
            .await
            .expect("advance generation after Resume commit and reopen")
            .executor_generation;
        assert!(later_generation > generation);

        let replay = compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Resume(request),
            core_request,
        )
        .await
        .expect("exact Resume retry recovers after ACK loss, reopen and mutable changes");
        let CoreComposedRunAdmission::Admitted {
            record: replayed_child,
            input: replayed_input,
            recorder: replayed_recorder,
        } = replay
        else {
            panic!("claimed Resume replay was unexpectedly superseded");
        };
        assert_eq!(replayed_child, child);
        assert_eq!(replayed_input.disposition, AdmissionDisposition::Replayed);
        assert_eq!(replayed_input.receipt, stored_input.receipt);
        assert_eq!(replayed_recorder, original_recorder);

        let connection = scenario
            .vault()
            .connection()
            .expect("connect after exact Resume ACK replay");
        for (table, count) in tables.iter().zip(committed_counts) {
            assert_eq!(
                table_count(&connection, table).await,
                count,
                "exact replay does not add rows to {table}"
            );
        }
        assert_eq!(
            scenario
                .vault()
                .conversation_journal(child_run_id)
                .await
                .expect("check journal after exact ACK replay")
                .len(),
            0,
            "exact replay still performs no dispatch"
        );
    }

    #[tokio::test]
    async fn concurrent_resume_composition_replays_one_child_slot_and_recorder() {
        let mut scenario = Scenario::new().await;
        let (origin, _) = scenario
            .admit_run_and_append_input("concurrent Resume claim")
            .await;
        let pending = scenario.create_pending_resume(&origin).await;
        let request = scenario.resume_child_admission(pending, &origin, "concurrent Resume claim");
        let (core_request, _) = scenario.resume_core_request();

        let (first, second) = tokio::join!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Resume(request.clone()),
                core_request.clone(),
            ),
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Resume(request),
                core_request,
            ),
        );
        let (first, second) = (
            first.expect("first concurrent Resume transaction succeeds"),
            second.expect("second concurrent Resume transaction replays"),
        );
        let (
            CoreComposedRunAdmission::Admitted {
                record: first,
                recorder: first_recorder,
                ..
            },
            CoreComposedRunAdmission::Admitted {
                record: second,
                recorder: second_recorder,
                ..
            },
        ) = (first, second)
        else {
            panic!("exact concurrent Resume calls return the one committed child");
        };
        assert_eq!(first, second);
        assert_eq!(first_recorder, second_recorder);
        let connection = scenario
            .vault()
            .connection()
            .expect("connect after concurrent claims");
        assert_eq!(table_count(&connection, "agent_conversation_runs").await, 2);
        assert_eq!(
            table_count(&connection, "agent_conversation_resume_slots").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
            2
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_open_receipts").await,
            2
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_entries").await,
            1
        );
    }

    #[tokio::test]
    async fn resume_owner_slot_fault_before_core_binding_rolls_back_every_write() {
        let mut scenario = Scenario::new().await;
        let (origin, _) = scenario
            .admit_run_and_append_input("faulted Resume transaction")
            .await;
        let pending = scenario.create_pending_resume(&origin).await;
        let request =
            scenario.resume_child_admission(pending, &origin, "faulted Resume transaction");
        let (core_request, _) = scenario.resume_core_request();
        let connection = scenario
            .vault()
            .connection()
            .expect("connect before injected fault");
        let before_session = session_storage_snapshot(&connection, scenario.session_id).await;
        let tables = [
            "agent_conversation_runs",
            "agent_conversation_resume_slots",
            "agent_conversation_core_v3_entries",
            "agent_conversation_core_v3_input_receipts",
            "agent_conversation_core_v3_owner_bindings",
            "agent_conversation_core_v3_open_receipts",
            "agent_conversation_core_v3_active_recorders",
        ];
        let mut before = Vec::new();
        for table in tables {
            before.push(table_count(&connection, table).await);
        }
        assert_eq!(
            stored_resume_state(&connection, origin.run_id)
                .await
                .as_deref(),
            Some("pending")
        );
        drop(connection);

        scenario
            .vault()
            .conversation_core_resume_fault_after_owner_claim
            .store(true, Ordering::Release);
        assert_eq!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Resume(request),
                core_request,
            )
            .await,
            Err(ConversationStoreFailure::NotCommitted),
            "the injected post-slot failure is reported only after rollback"
        );

        let connection = scenario
            .vault()
            .connection()
            .expect("reconnect after rollback");
        for (table, count) in tables.iter().zip(before) {
            assert_eq!(
                table_count(&connection, table).await,
                count,
                "rollback restores {table}"
            );
        }
        assert_eq!(
            session_storage_snapshot(&connection, scenario.session_id).await,
            before_session,
            "owner Session CAS rolls back with the claimed slot"
        );
        assert_eq!(
            stored_resume_state(&connection, origin.run_id)
                .await
                .as_deref(),
            Some("pending")
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_resume_slots").await,
            0
        );
    }

    #[tokio::test]
    async fn resume_composition_denies_wrong_session_device_group_and_core_binding() {
        let mut scenario = Scenario::new().await;
        let (origin, _) = scenario
            .admit_run_and_append_input("Resume admission checks")
            .await;
        let pending = scenario.create_pending_resume(&origin).await;
        let request = scenario.resume_child_admission(pending, &origin, "Resume admission checks");
        let (core_request, _) = scenario.resume_core_request();
        let connection = scenario
            .vault()
            .connection()
            .expect("connect before denials");
        let before_session = session_storage_snapshot(&connection, scenario.session_id).await;
        let tables = [
            "agent_conversation_runs",
            "agent_conversation_resume_slots",
            "agent_conversation_core_v3_entries",
            "agent_conversation_core_v3_input_receipts",
            "agent_conversation_core_v3_owner_bindings",
            "agent_conversation_core_v3_open_receipts",
        ];
        let mut before = Vec::new();
        for table in tables {
            before.push(table_count(&connection, table).await);
        }
        drop(connection);

        let mut wrong_session = request.clone();
        let other_session = Uuid::new_v4();
        wrong_session.request.session_id = other_session;
        wrong_session.child.session_id = other_session;

        let mut wrong_device = request.clone();
        wrong_device.request.device_id = "other-device".into();
        wrong_device.child.device_id = "other-device".into();

        let mut wrong_group = request.clone();
        wrong_group.request.group_digest[0] ^= 1;

        let mut wrong_identity = core_request.clone();
        let AdmissionTarget::AppendToExisting { reference } = &mut wrong_identity.target else {
            panic!("Resume source must target the retained conversation");
        };
        reference.identity.assignment_id = AssignmentId::new();

        let mut wrong_binding = core_request.clone();
        wrong_binding.message.message_id = MessageId::new();

        for (request, binding) in [
            (wrong_session, core_request.clone()),
            (wrong_device, core_request.clone()),
            (wrong_group, core_request.clone()),
            (request.clone(), wrong_identity),
            (request, wrong_binding),
        ] {
            assert_eq!(
                compose_owner_core_run(
                    scenario.vault(),
                    CoreComposedOwnerIntent::Resume(request),
                    binding,
                )
                .await,
                Err(ConversationStoreFailure::Transition(
                    ConversationFailure::OwnerEvidenceMismatch
                )),
                "foreign Session/device/group/Core binding is denied"
            );
        }

        let connection = scenario
            .vault()
            .connection()
            .expect("reconnect after denials");
        for (table, count) in tables.iter().zip(before) {
            assert_eq!(
                table_count(&connection, table).await,
                count,
                "denial leaves {table} unchanged"
            );
        }
        assert_eq!(
            session_storage_snapshot(&connection, scenario.session_id).await,
            before_session
        );
        assert_eq!(
            stored_resume_state(&connection, origin.run_id)
                .await
                .as_deref(),
            Some("pending")
        );
    }

    #[tokio::test]
    async fn owner_only_resume_still_works_without_retrofitting_a_core_binding() {
        let mut scenario = Scenario::new().await;
        let origin_request = scenario.owner_request("owner-only Resume source");
        let origin = scenario.admit_unbound_owner_run(origin_request).await;
        let pending = scenario.create_pending_resume(&origin).await;
        let generation = scenario
            .vault()
            .activate_conversation_executor()
            .await
            .expect("advance generation before legacy owner-only Resume")
            .executor_generation;
        let request = scenario.resume_child_admission(pending, &origin, "owner-only Resume source");
        let missing_source = scenario.core_input_request("owner-only Resume source");
        let before = scenario
            .vault()
            .connection()
            .expect("connect before fail-closed composed attempt");
        let before_session = session_storage_snapshot(&before, scenario.session_id).await;
        drop(before);
        assert_eq!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Resume(request.clone()),
                missing_source,
            )
            .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
            "the composed path requires the source's actual Core binding"
        );
        let connection = scenario
            .vault()
            .connection()
            .expect("connect after fail-closed attempt");
        assert_eq!(
            session_storage_snapshot(&connection, scenario.session_id).await,
            before_session
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_resume_slots").await,
            0
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
            0,
            "a missing source binding is not silently imported"
        );
        drop(connection);

        let admission = scenario
            .vault()
            .claim_conversation_resume(request)
            .await
            .expect("public legacy owner-only Resume remains supported");
        let super::super::VaultConversationAdmission::Created { record: child, .. } = admission
        else {
            panic!("first owner-only Resume claim creates its child");
        };
        assert_eq!(child.resume_of, Some(origin.run_id));
        assert!(generation > origin.executor_generation);
        assert_eq!(child.executor_generation, generation);
        assert_eq!(child.continuation_of, None);
        let session = SessionStore::load(scenario.vault(), scenario.person_id, scenario.session_id)
            .await
            .expect("reload legacy owner-only Session");
        assert_eq!(
            session
                .messages
                .iter()
                .filter(|message| matches!(message, floe_conversation::AgentMessage::User { .. }))
                .count(),
            1,
            "owner-only Resume preserves the original user message"
        );
        assert!(matches!(
            session.messages.first(),
            Some(floe_conversation::AgentMessage::User { message_id, .. })
                if *message_id == origin.user_message_id
        ));
    }

    #[tokio::test]
    async fn a_real_new_supersedes_pending_resume_without_composer_revival() {
        let mut scenario = Scenario::new().await;
        let (origin, _) = scenario
            .admit_run_and_append_input("New supersedes Resume")
            .await;
        let pending = scenario.create_pending_resume(&origin).await;
        let request = scenario.resume_child_admission(pending, &origin, "New supersedes Resume");
        let (core_request, _) = scenario.resume_core_request();
        let new_request = scenario.owner_request("real New input");
        let super::super::VaultConversationAdmission::Created {
            record: new_run, ..
        } = scenario
            .vault()
            .admit_conversation_turn(new_request)
            .await
            .expect("real New admits through the Conversation owner")
        else {
            panic!("real New creates its owner Run");
        };
        assert_ne!(new_run.run_id, origin.run_id);
        assert_eq!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Resume(request),
                core_request,
            )
            .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
            "a superseded request remains a conflict, never a successful child"
        );
        let connection = scenario
            .vault()
            .connection()
            .expect("connect after New supersession");
        assert_eq!(
            stored_resume_state(&connection, origin.run_id)
                .await
                .as_deref(),
            Some("superseded")
        );
        assert_eq!(table_count(&connection, "agent_conversation_runs").await, 2);
        assert_eq!(
            table_count(&connection, "agent_conversation_resume_slots").await,
            0
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
            1
        );
    }

    #[tokio::test]
    async fn composed_stale_resume_commits_supersession_before_returning_conflict() {
        let mut scenario = Scenario::new().await;
        let (origin, _) = scenario
            .admit_run_and_append_input("stale Session Resume")
            .await;
        let pending = scenario.create_pending_resume(&origin).await;
        let request = scenario.resume_child_admission(pending, &origin, "stale Session Resume");
        let (core_request, _) = scenario.resume_core_request();

        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to advance Session");
        let (mut guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start controlled Session revision change");
        let mut session = scenario
            .vault()
            .session_on(&transaction, scenario.session_id)
            .await
            .expect("load current owner Session");
        let prior_revision = session.revision;
        session.revision += 1;
        let changed = transaction
            .execute(
                "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                (
                    integer(session.revision).expect("encode next Session revision"),
                    scenario
                        .vault()
                        .payload(&session)
                        .expect("encode changed Session"),
                    session.id.to_string(),
                    integer(prior_revision).expect("encode prior Session revision"),
                ),
            )
            .await
            .expect("persist changed Session revision");
        assert_eq!(changed, 1);
        transaction
            .commit()
            .await
            .expect("commit controlled Session revision change");
        guard.settled();
        drop(guard);
        drop(connection);

        assert_eq!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Resume(request),
                core_request,
            )
            .await
            .expect("superseded outcome commits through the Core transaction"),
            CoreComposedRunAdmission::ResumeSuperseded
        );
        let connection = scenario
            .vault()
            .connection()
            .expect("connect after committed supersession");
        assert_eq!(
            stored_resume_state(&connection, origin.run_id)
                .await
                .as_deref(),
            Some("superseded")
        );
        assert_eq!(table_count(&connection, "agent_conversation_runs").await, 1);
        assert_eq!(
            table_count(&connection, "agent_conversation_resume_slots").await,
            0
        );
    }

    #[tokio::test]
    async fn exact_new_owner_replay_recovers_its_input_and_open_receipts_after_terminal_reopen() {
        let mut scenario = Scenario::new().await;
        let owner_request = scenario.owner_request("exact New replay");
        let core_request = scenario.core_input_request("exact New replay");
        let (first_run, first_input) = scenario
            .admit_run_and_bind_input(owner_request.clone(), core_request.clone())
            .await;
        assert_eq!(first_input.disposition, AdmissionDisposition::Appended);
        let first_recorder = scenario
            .last_core_recorder
            .clone()
            .expect("New composition returns its open receipt");
        assert_eq!(first_recorder.fence.run_id, first_run.run_id);
        assert_eq!(first_recorder.fence.input, first_input.receipt.transcript);
        let first_mapping =
            stored_run_input_mapping(scenario.vault(), scenario.person_id, first_run.run_id)
                .await
                .expect("first New write stores the owner-to-Core mapping");
        assert_eq!(first_mapping.input, first_input.receipt.transcript);
        assert_eq!(first_mapping.original_owner_run_id, first_run.run_id);
        let terminal = scenario.cancel_owner_run(&first_run).await;
        assert!(terminal.state.is_terminal());
        scenario.reopen().await;
        let later_generation = scenario
            .vault()
            .activate_conversation_executor()
            .await
            .expect("advance executor generation after terminal Run reopen")
            .executor_generation;
        assert!(later_generation > first_recorder.fence.executor_generation);

        let before = scenario
            .vault()
            .connection()
            .expect("connect to test Vault");
        let before_session = session_storage_snapshot(&before, scenario.session_id).await;
        drop(before);
        let replay = compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Turn(owner_request.clone()),
            core_request.clone(),
        )
        .await
        .expect("exact New replay precedes terminal and generation fences");
        let CoreComposedRunAdmission::Admitted {
            record: replayed_run,
            input: replayed_input,
            recorder: replayed_recorder,
        } = replay
        else {
            panic!("exact New replay was unexpectedly superseded");
        };
        assert_eq!(replayed_run, terminal);
        assert_eq!(replayed_input.disposition, AdmissionDisposition::Replayed);
        assert_eq!(replayed_input.receipt, first_input.receipt);
        assert_eq!(replayed_recorder, first_recorder);
        assert_eq!(
            stored_run_input_mapping(scenario.vault(), scenario.person_id, first_run.run_id).await,
            Some(first_mapping),
            "exact New replay validates and preserves the original mapping"
        );

        let mut changed_command = owner_request.clone();
        changed_command.command_id = CommandId::new();
        assert!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Turn(changed_command),
                core_request.clone(),
            )
            .await
            .is_err(),
            "a changed owner command cannot claim the old Run"
        );
        let mut changed_body = core_request.clone();
        changed_body.message.text.push_str(" changed");
        assert!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Turn(owner_request.clone()),
                changed_body,
            )
            .await
            .is_err(),
            "a changed transcript body cannot replay the old admission"
        );
        let mut changed_identity = core_request.clone();
        let AdmissionTarget::New { identity, .. } = &mut changed_identity.target else {
            panic!("New admission keeps its original target");
        };
        identity.definition_revision += 1;
        assert!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Turn(owner_request),
                changed_identity,
            )
            .await
            .is_err(),
            "a changed Core identity cannot replay the old admission"
        );

        let after = scenario
            .vault()
            .connection()
            .expect("reconnect to test Vault");
        assert_eq!(table_count(&after, "agent_conversation_runs").await, 1);
        assert_eq!(
            table_count(&after, "agent_conversation_core_v3_entries").await,
            1
        );
        assert_eq!(
            table_count(&after, "agent_conversation_core_v3_owner_bindings").await,
            1
        );
        assert_eq!(
            table_count(&after, "agent_conversation_core_v3_open_receipts").await,
            1
        );
        assert_eq!(
            session_storage_snapshot(&after, scenario.session_id).await,
            before_session
        );
    }

    #[tokio::test]
    async fn distinct_new_commands_cannot_reuse_one_core_message_id() {
        let mut scenario = Scenario::new().await;
        let text = "different New commands, one Core MessageId";
        let first_owner = scenario.owner_request(text);
        let shared_core_input = scenario.core_input_request(text);
        let (first_run, first_input) = scenario
            .admit_run_and_bind_input(first_owner, shared_core_input.clone())
            .await;
        scenario.cancel_owner_run(&first_run).await;
        let second_owner = scenario.owner_request(text);
        assert_ne!(second_owner.command_id, first_run.command_id);
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to test Vault");
        let before_session = session_storage_snapshot(&connection, scenario.session_id).await;
        let before_runs = table_count(&connection, "agent_conversation_runs").await;
        let before_entries = table_count(&connection, "agent_conversation_core_v3_entries").await;
        let before_bindings =
            table_count(&connection, "agent_conversation_core_v3_owner_bindings").await;
        let (mut guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start duplicate Core MessageId transaction");
        let mut replay_checked = false;
        let mut prior_command = false;
        assert_eq!(
            scenario
                .vault()
                .admit_conversation_run_with_core_input_on(
                    &transaction,
                    CoreComposedOwnerIntent::Turn(second_owner),
                    shared_core_input,
                    &mut replay_checked,
                    &mut prior_command,
                )
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
            "a newly admitted owner command requires a fresh Core append"
        );
        assert!(replay_checked, "the second owner intent was new");
        transaction
            .rollback()
            .await
            .expect("roll back owner admission rejected by Core replay");
        guard.settled();
        drop(guard);
        drop(connection);

        let after = scenario
            .vault()
            .connection()
            .expect("reconnect to test Vault");
        assert_eq!(
            table_count(&after, "agent_conversation_runs").await,
            before_runs
        );
        assert_eq!(
            table_count(&after, "agent_conversation_core_v3_entries").await,
            before_entries
        );
        assert_eq!(
            table_count(&after, "agent_conversation_core_v3_owner_bindings").await,
            before_bindings
        );
        assert_eq!(
            session_storage_snapshot(&after, scenario.session_id).await,
            before_session,
            "owner Session append and revision rolled back with the Core conflict"
        );
        assert_eq!(first_input.receipt.transcript.sequence, 1);
    }

    #[tokio::test]
    async fn recorder_open_requires_the_exact_admission_binding() {
        let mut scenario = Scenario::new().await;
        let (run, input) = scenario
            .admit_run_and_append_input("bound owner input")
            .await;
        let start = scenario.start_request(&run, input.receipt.transcript);

        let mut changed_identity = start.clone();
        changed_identity.identity.agent_instance_id = AgentInstanceId::new();
        assert_eq!(
            scenario
                .vault()
                .open_conversation_recorder(changed_identity)
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::RunAlreadyUsed
            )),
        );

        let mut changed_input = start.clone();
        changed_input.input.message_id = MessageId::new();
        assert_eq!(
            scenario
                .vault()
                .open_conversation_recorder(changed_input)
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::RunAlreadyUsed
            )),
        );

        let mut changed_generation = start.clone();
        changed_generation.executor_generation += 1;
        assert_eq!(
            scenario
                .vault()
                .open_conversation_recorder(changed_generation)
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::RunAlreadyUsed
            )),
        );

        let mut changed_domain = start.clone();
        changed_domain.executor_domain = ExecutorDomain::TaskExecution;
        changed_domain.execution_task = Some(TaskId::new());
        assert_eq!(
            scenario
                .vault()
                .open_conversation_recorder(changed_domain)
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::RunAlreadyUsed
            )),
        );

        scenario
            .vault()
            .open_conversation_recorder(start)
            .await
            .expect("the exact bound owner Run opens");
    }

    #[tokio::test]
    async fn same_person_run_without_transactional_core_binding_cannot_open() {
        let mut scenario = Scenario::new().await;
        let (first_run, input) = scenario
            .admit_run_and_append_input("same person is insufficient")
            .await;
        scenario.cancel_owner_run(&first_run).await;
        let unbound_request = scenario.owner_request("same person is insufficient");
        let unbound = scenario
            .admit_unbound_owner_run(unbound_request.clone())
            .await;
        assert_eq!(unbound.person_id, first_run.person_id);

        let core_request = scenario.core_input_request("same person is insufficient");
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to test Vault");
        let before_session = session_storage_snapshot(&connection, scenario.session_id).await;
        let before_runs = table_count(&connection, "agent_conversation_runs").await;
        let before_entries = table_count(&connection, "agent_conversation_core_v3_entries").await;
        let before_bindings =
            table_count(&connection, "agent_conversation_core_v3_owner_bindings").await;
        let (mut guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start rejected legacy owner replay transaction");
        let mut replay_checked = false;
        let mut prior_command = false;
        assert_eq!(
            scenario
                .vault()
                .admit_conversation_run_with_core_input_on(
                    &transaction,
                    CoreComposedOwnerIntent::Turn(unbound_request),
                    core_request,
                    &mut replay_checked,
                    &mut prior_command,
                )
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
            "an existing owner Run without its immutable binding cannot be retrofitted"
        );
        assert_eq!(
            table_count_in_transaction(&transaction, "agent_conversation_runs").await,
            before_runs
        );
        assert_eq!(
            table_count_in_transaction(&transaction, "agent_conversation_core_v3_entries").await,
            before_entries
        );
        assert_eq!(
            table_count_in_transaction(&transaction, "agent_conversation_core_v3_owner_bindings",)
                .await,
            before_bindings
        );
        assert_eq!(
            session_storage_snapshot(&transaction, scenario.session_id).await,
            before_session
        );
        transaction
            .rollback()
            .await
            .expect("roll back rejected unbound owner replay");
        guard.settled();
        drop(guard);
        drop(connection);

        assert_eq!(
            scenario
                .vault()
                .open_conversation_recorder(
                    scenario.start_request(&unbound, input.receipt.transcript),
                )
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
        );
    }

    #[tokio::test]
    async fn lost_open_ack_replays_exact_receipt_after_restart_before_generation_fence() {
        let mut scenario = Scenario::new().await;
        let (run, input) = scenario.admit_run_and_append_input("open ack").await;
        assert_eq!(input.disposition, AdmissionDisposition::Appended);
        let start = scenario.start_request(&run, input.receipt.transcript);
        scenario
            .vault()
            .conversation_core_ack_loss
            .store(true, Ordering::Release);
        assert_eq!(
            scenario
                .vault()
                .open_conversation_recorder(start.clone())
                .await,
            Err(ConversationStoreFailure::OutcomeUnknown)
        );

        scenario.reopen().await;
        let activation = scenario
            .vault()
            .activate_conversation_executor()
            .await
            .expect("fence old executor after restart");
        assert!(activation.executor_generation > start.executor_generation);
        let replay = scenario
            .vault()
            .open_conversation_recorder(start.clone())
            .await
            .expect("exact lost-open replay survives stale generation");
        assert_eq!(replay.fence.run_id, run.run_id);
        assert_eq!(replay.fence.input, input.receipt.transcript);
        assert_eq!(replay.fence.executor_generation, start.executor_generation);

        let mut changed_identity = start.clone();
        changed_identity.identity.assignment_id = AssignmentId::new();
        assert_eq!(
            scenario
                .vault()
                .open_conversation_recorder(changed_identity)
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::RunAlreadyUsed
            ))
        );
        let mut changed_domain = start.clone();
        changed_domain.executor_domain = ExecutorDomain::TaskExecution;
        changed_domain.execution_task = Some(TaskId::new());
        assert_eq!(
            scenario
                .vault()
                .open_conversation_recorder(changed_domain)
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::RunAlreadyUsed
            ))
        );
        let mut changed_generation = start;
        changed_generation.executor_generation += 1;
        assert_eq!(
            scenario
                .vault()
                .open_conversation_recorder(changed_generation)
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::RunAlreadyUsed
            ))
        );
    }

    #[tokio::test]
    async fn lost_output_ack_replays_exact_contribution_after_restart() {
        let mut scenario = Scenario::new().await;
        let (run, input) = scenario.admit_run_and_append_input("output ack").await;
        let start = scenario.start_request(&run, input.receipt.transcript);
        let open = scenario
            .vault()
            .open_conversation_recorder(start)
            .await
            .expect("open recorder");
        let request = scenario.recording_request(open.fence.clone(), "record once");
        scenario
            .vault()
            .conversation_core_ack_loss
            .store(true, Ordering::Release);
        assert_eq!(
            scenario
                .vault()
                .record_conversation_entry(request.clone())
                .await,
            Err(ConversationStoreFailure::OutcomeUnknown)
        );

        scenario.reopen().await;
        let replay = scenario
            .vault()
            .record_conversation_entry(request.clone())
            .await
            .expect("exact output receipt replays after restart");
        assert_eq!(
            replay.transcript.sequence,
            input.receipt.transcript.sequence + 1
        );
        assert_eq!(replay.recorder, open.fence);
        assert_eq!(replay.contribution_id, request.contribution_id);
        let mut changed = request;
        changed.message.text.push_str(" changed");
        assert_eq!(
            scenario.vault().record_conversation_entry(changed).await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::MessageIdConflict
            ))
        );
    }

    #[tokio::test]
    async fn lost_close_ack_replays_exact_receipt_and_keeps_unproven_prefix() {
        let mut scenario = Scenario::new().await;
        let (run, input) = scenario.admit_run_and_append_input("close ack").await;
        let start = scenario.start_request(&run, input.receipt.transcript);
        let open = scenario
            .vault()
            .open_conversation_recorder(start)
            .await
            .expect("open recorder");
        let terminal = scenario.cancel_owner_run(&run).await;
        scenario
            .vault()
            .conversation_core_ack_loss
            .store(true, Ordering::Release);
        assert_eq!(
            scenario
                .vault()
                .close_conversation_recorder(open.fence.clone())
                .await,
            Err(ConversationStoreFailure::OutcomeUnknown)
        );

        scenario.reopen().await;
        let replay = scenario
            .vault()
            .close_conversation_recorder(open.fence.clone())
            .await
            .expect("exact close receipt replays after restart");
        assert_eq!(replay.fence, open.fence);
        assert_eq!(replay.owner_aggregate_revision, terminal.aggregate_revision);
        assert_eq!(replay.settled_prefix, 0);
        assert!(replay.protection_roots.is_empty());
    }

    #[tokio::test]
    async fn retained_input_can_bind_two_separate_owner_runs() {
        let mut scenario = Scenario::new().await;
        let (first_run, input) = scenario.admit_run_and_append_input("retained input").await;
        let first_open = scenario
            .vault()
            .open_conversation_recorder(
                scenario.start_request(&first_run, input.receipt.transcript),
            )
            .await
            .expect("open first recorder");
        let first_terminal = scenario.budget_exceeded_owner_run(&first_run).await;
        let first_close = scenario
            .vault()
            .close_conversation_recorder(first_open.fence.clone())
            .await
            .expect("close first recorder");
        assert_eq!(first_close.settled_prefix, 0);

        let (second_run, reused_input) = scenario
            .admit_owner_run_reusing_input(&first_terminal, "Continue retained input")
            .await;
        assert_eq!(reused_input.disposition, AdmissionDisposition::Replayed);
        let second_open = scenario
            .vault()
            .open_conversation_recorder(
                scenario.start_request(&second_run, input.receipt.transcript),
            )
            .await
            .expect("reuse exact retained Core input for another owner Run");
        assert_ne!(first_run.run_id, second_run.run_id);
        assert!(second_open.fence.recorder_epoch > first_open.fence.recorder_epoch);
        assert_eq!(second_open.fence.input, first_close.fence.input);
        assert!(first_terminal.state.is_terminal());
    }

    #[tokio::test]
    async fn recorder_can_open_a_fresh_input_beyond_the_unsettled_prefix() {
        let mut scenario = Scenario::new().await;
        let (first_run, first_input) = scenario.admit_run_and_append_input("first turn").await;
        let first_open = scenario
            .vault()
            .open_conversation_recorder(
                scenario.start_request(&first_run, first_input.receipt.transcript),
            )
            .await
            .expect("open first recorder");
        scenario
            .vault()
            .record_conversation_entry(
                scenario.recording_request(first_open.fence.clone(), "first output"),
            )
            .await
            .expect("append first generated output");
        scenario.cancel_owner_run(&first_run).await;
        let first_close = scenario
            .vault()
            .close_conversation_recorder(first_open.fence)
            .await
            .expect("close first recorder without settlement proof");
        assert_eq!(first_close.settled_prefix, 0);

        let owner_request = scenario.owner_request("second turn");
        let mut core_request = scenario.core_input_request("second turn");
        core_request.target = AdmissionTarget::AppendToExisting {
            reference: ConversationReference {
                identity: scenario.identity.clone(),
                conversation_id: scenario.conversation_id,
                branch_id: scenario.branch_id,
                head_revision: 2,
            },
        };
        let (second_run, second_input) = scenario
            .admit_run_and_bind_input(owner_request, core_request)
            .await;
        assert_eq!(second_input.disposition, AdmissionDisposition::Appended);
        assert_eq!(second_input.receipt.transcript.sequence, 3);

        let connection = scenario
            .vault()
            .connection()
            .expect("connect to test Vault");
        let mut rows = connection
            .query(
                "SELECT head_revision, settled_prefix FROM agent_conversation_core_v3_heads WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
                (
                    scenario.person_id.to_string(),
                    scenario.conversation_id.as_uuid().to_string(),
                    scenario.branch_id.as_uuid().to_string(),
                ),
            )
            .await
            .expect("query transcript head");
        let head = rows
            .next()
            .await
            .expect("read transcript head")
            .expect("head exists");
        assert_eq!(head.get::<i64>(0).expect("head revision"), 3);
        assert_eq!(head.get::<i64>(1).expect("settled prefix"), 0);
        drop(rows);
        drop(connection);

        let second_open = scenario
            .vault()
            .open_conversation_recorder(
                scenario.start_request(&second_run, second_input.receipt.transcript),
            )
            .await
            .expect("open recorder for input beyond the settled prefix");
        assert_eq!(second_open.fence.input.sequence, 3);
        assert_eq!(
            second_open.fence.recorder_epoch,
            first_close.fence.recorder_epoch + 1
        );
    }

    #[tokio::test]
    async fn continue_reuses_the_exact_owner_bound_input_without_appending() {
        let mut scenario = Scenario::new().await;
        let (parent, input) = scenario
            .admit_run_and_append_input("retained input for Continue")
            .await;
        let parent_recorder = scenario
            .last_core_recorder
            .clone()
            .expect("New composition opens the parent recorder");
        let failed_parent = scenario.budget_exceeded_owner_run(&parent).await;
        scenario
            .vault()
            .close_conversation_recorder(parent_recorder.fence.clone())
            .await
            .expect("close the failed parent before Continue admission");
        let continuation = floe_conversation::project_run_receipt(failed_parent.clone())
            .expect("project failed parent receipt")
            .continuation()
            .expect("BudgetExceeded parent has a continuation reference");
        let principal = scenario.person_id.to_string();
        let mode = TurnMode::Continue(continuation.clone());
        let intent = CanonicalTurnIntent {
            session_id: scenario.session_id,
            expected_revision: scenario.session_revision,
            text: "Continue the retained input".to_owned(),
            mode: mode.clone(),
            retry_of: None,
        };
        let owner_request = TurnAdmissionRequest {
            expert_environment: failed_parent.expert_environment,
            run_id: RunId::new(),
            command_id: CommandId::new(),
            session_id: scenario.session_id,
            expected_session_revision: scenario.session_revision,
            principal: principal.clone(),
            device_id: failed_parent.device_id.clone(),
            request_digest: intent.digest(&principal).expect("Continue request digest"),
            mode,
            retry_of: None,
            input: TurnInput::ExistingMessage {
                message_id: failed_parent.user_message_id,
            },
        };
        let (core_request, retained_reference) = scenario
            .last_core_input_request
            .clone()
            .expect("root admission persisted its exact Core request");
        assert_eq!(retained_reference, input.receipt.transcript);

        let mut wrong_session = owner_request.clone();
        wrong_session.session_id = Uuid::new_v4();
        wrong_session.expected_session_revision = scenario.session_revision;
        wrong_session.request_digest = CanonicalTurnIntent {
            session_id: wrong_session.session_id,
            expected_revision: wrong_session.expected_session_revision,
            text: intent.text.clone(),
            mode: wrong_session.mode.clone(),
            retry_of: None,
        }
        .digest(&principal)
        .expect("changed-session Continue digest");
        let mut connection = scenario
            .vault()
            .connection()
            .expect("connect to test Vault");
        let (mut guard, transaction) = scenario
            .vault()
            .journal_transaction(&mut connection)
            .await
            .expect("start wrong-session composition transaction");
        let mut replay_checked = false;
        let mut prior_command = false;
        let rejected = scenario
            .vault()
            .admit_conversation_run_with_core_input_on(
                &transaction,
                CoreComposedOwnerIntent::Turn(wrong_session),
                core_request.clone(),
                &mut replay_checked,
                &mut prior_command,
            )
            .await;
        assert_eq!(
            rejected,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            ))
        );
        transaction
            .rollback()
            .await
            .expect("roll back rejected changed-session admission");
        guard.settled();
        drop(guard);
        drop(connection);

        let (continued, reused) = scenario
            .admit_run_and_bind_input(owner_request.clone(), core_request.clone())
            .await;
        assert_eq!(reused.disposition, AdmissionDisposition::Replayed);
        assert_eq!(reused.receipt.transcript, retained_reference);
        assert_ne!(continued.run_id, failed_parent.run_id);
        let continue_recorder = scenario
            .last_core_recorder
            .clone()
            .expect("Continue composition opens the child recorder");
        assert_eq!(continue_recorder.fence.run_id, continued.run_id);
        assert_eq!(continue_recorder.fence.input, retained_reference);
        assert!(continue_recorder.fence.recorder_epoch > parent_recorder.fence.recorder_epoch);
        let original_mapping =
            stored_run_input_mapping(scenario.vault(), scenario.person_id, failed_parent.run_id)
                .await
                .expect("New Run keeps its immutable Core input mapping");
        assert_eq!(
            stored_run_input_mapping(scenario.vault(), scenario.person_id, continued.run_id).await,
            Some(original_mapping.clone()),
            "Continue Run points to the original New input mapping"
        );
        assert_eq!(original_mapping.original_owner_run_id, parent.run_id);
        let connection = scenario
            .vault()
            .connection()
            .expect("reconnect to test Vault");
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_entries").await,
            1,
            "Continue binds the retained input without appending a duplicate"
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_owner_transcript_inputs_v1").await,
            1,
            "Continue does not make a replacement owner-to-input mapping"
        );
        assert_eq!(
            table_count(
                &connection,
                "agent_conversation_owner_transcript_run_inputs_v1"
            )
            .await,
            2,
            "both New and Continue Runs point to that one mapping"
        );
        let opened = scenario
            .vault()
            .open_conversation_recorder(scenario.start_request(&continued, retained_reference))
            .await
            .expect("open a Continue Run against the retained input");
        assert_eq!(opened.fence.run_id, continued.run_id);
        assert_eq!(opened.fence.input, retained_reference);
        assert_eq!(opened, continue_recorder);

        let terminal_continue = scenario.cancel_owner_run(&continued).await;
        scenario.reopen().await;
        let later_generation = scenario
            .vault()
            .activate_conversation_executor()
            .await
            .expect("advance executor generation after terminal Continue reopen")
            .executor_generation;
        assert!(later_generation > continue_recorder.fence.executor_generation);
        let replay = compose_owner_core_run(
            scenario.vault(),
            CoreComposedOwnerIntent::Turn(owner_request),
            core_request,
        )
        .await
        .expect("exact Continue replay survives terminal state and generation movement");
        let CoreComposedRunAdmission::Admitted {
            record: replayed_run,
            input: replayed_input,
            recorder: replayed_recorder,
        } = replay
        else {
            panic!("exact Continue replay was unexpectedly superseded");
        };
        assert_eq!(replayed_run, terminal_continue);
        assert_eq!(replayed_input.disposition, AdmissionDisposition::Replayed);
        assert_eq!(replayed_input.receipt, reused.receipt);
        assert_eq!(replayed_recorder, continue_recorder);
    }

    #[tokio::test]
    async fn one_manager_recorder_stores_two_exact_task_owner_receipts() {
        let mut scenario = Scenario::new().await;
        let (run, input) = scenario
            .admit_run_and_append_input("manager coordinates two Tasks")
            .await;
        let open = scenario
            .vault()
            .open_conversation_recorder(scenario.start_request(&run, input.receipt.transcript))
            .await
            .expect("open Manager recorder");
        let task_generation = scenario
            .vault()
            .activate_task_executor()
            .await
            .expect("activate Task owner")
            .executor_generation;
        let task_receipts = scenario.terminal_task_receipts(&run, task_generation).await;
        let receipt_a = task_receipts[0].clone();
        let receipt_b = task_receipts[1].clone();
        assert_ne!(receipt_a.execution.task_id, receipt_b.execution.task_id);
        let task_reference_a =
            task_evidence_reference(&receipt_a).expect("bind exact Task A receipt reference");
        let task_reference_b =
            task_evidence_reference(&receipt_b).expect("bind exact Task B receipt reference");

        let mut request_a = scenario.recording_request(open.fence.clone(), "Task A interrupted");
        request_a.message.task_id = Some(receipt_a.execution.task_id);
        request_a.producing_task = Some(task_reference_a.clone());
        let mut request_b = scenario.recording_request(open.fence.clone(), "Task B interrupted");
        request_b.message.task_id = Some(receipt_b.execution.task_id);
        request_b.producing_task = Some(task_reference_b.clone());
        let recorded_a = scenario
            .vault()
            .record_conversation_entry(request_a.clone())
            .await
            .expect("record Task A contribution after owner verification");
        let recorded_b = scenario
            .vault()
            .record_conversation_entry(request_b)
            .await
            .expect("record Task B contribution after owner verification");
        assert_eq!(recorded_a.producing_task, Some(task_reference_a.clone()));
        assert_eq!(recorded_b.producing_task, Some(task_reference_b.clone()));
        assert_eq!(
            scenario
                .vault()
                .record_conversation_entry(request_a.clone())
                .await
                .expect("replay exact Task A contribution"),
            recorded_a,
        );

        let terminal = scenario.budget_exceeded_owner_run(&run).await;
        scenario
            .vault()
            .close_conversation_recorder(open.fence.clone())
            .await
            .expect("close completed owner before admitting another Run");
        let (other_run, _) = scenario
            .admit_owner_run_reusing_input(&terminal, "Continue after Task evidence")
            .await;
        let other_open = scenario
            .vault()
            .open_conversation_recorder(
                scenario.start_request(&other_run, input.receipt.transcript),
            )
            .await
            .expect("open second owner Run against the retained input");
        let mut cross_run_task =
            scenario.recording_request(other_open.fence, "cannot attach another Run Task");
        cross_run_task.message.task_id = Some(receipt_a.execution.task_id);
        cross_run_task.producing_task = Some(task_reference_a.clone());
        assert_eq!(
            scenario
                .vault()
                .record_conversation_entry(cross_run_task)
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::OwnerEvidenceMismatch
            )),
            "Task receipt must be linked through this recorder's own intent/result"
        );

        let page = scenario
            .vault()
            .read_conversation_page(
                ConversationReference {
                    identity: scenario.identity.clone(),
                    conversation_id: scenario.conversation_id,
                    branch_id: scenario.branch_id,
                    head_revision: recorded_b.transcript.sequence,
                },
                None,
                TranscriptPageBudget {
                    max_entries: 8,
                    max_bytes: 4096,
                },
            )
            .await
            .expect("read exact Task evidence links");
        assert_eq!(page.entries[1].producing_task, recorded_a.producing_task);
        assert_eq!(page.entries[2].producing_task, recorded_b.producing_task);

        let mut changed_task = request_a;
        changed_task.message.task_id = Some(receipt_b.execution.task_id);
        changed_task.producing_task = Some(task_reference_b);
        assert_eq!(
            scenario
                .vault()
                .record_conversation_entry(changed_task)
                .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::MessageIdConflict
            )),
        );
    }

    #[tokio::test]
    async fn encrypted_transcript_pages_preserve_count_byte_and_cursor_boundaries() {
        let scenario = Scenario::new().await;
        let first_request = scenario.core_input_request("λ");
        let first = scenario
            .vault()
            .append_conversation_input(first_request.clone())
            .await
            .expect("append first retained input");
        let mut second_request = scenario.core_input_request("second entry is longer 🙂");
        second_request.target = AdmissionTarget::AppendToExisting {
            reference: ConversationReference {
                identity: scenario.identity.clone(),
                conversation_id: scenario.conversation_id,
                branch_id: scenario.branch_id,
                head_revision: first.receipt.transcript.sequence,
            },
        };
        let second = scenario
            .vault()
            .append_conversation_input(second_request)
            .await
            .expect("append second retained input");
        let target = ConversationReference {
            identity: scenario.identity.clone(),
            conversation_id: scenario.conversation_id,
            branch_id: scenario.branch_id,
            head_revision: second.receipt.transcript.sequence,
        };

        let count_limited = scenario
            .vault()
            .read_conversation_page(
                target.clone(),
                None,
                TranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: 4096,
                },
            )
            .await
            .expect("read one bounded entry");
        assert_eq!(count_limited.entries.len(), 1);
        assert_eq!(count_limited.entries[0].reference, first.receipt.transcript);
        assert_eq!(count_limited.next_cursor, Some(first.receipt.transcript));
        assert!(count_limited.has_more);
        let utf8_record_bytes = serde_json::to_vec(&count_limited.entries[0])
            .expect("encode UTF-8 transcript record envelope")
            .len();
        assert_eq!(count_limited.encoded_bytes, utf8_record_bytes);
        assert!(utf8_record_bytes > first_request.message.text.chars().count());

        let byte_limited = scenario
            .vault()
            .read_conversation_page(
                target.clone(),
                None,
                TranscriptPageBudget {
                    max_entries: 8,
                    max_bytes: count_limited.encoded_bytes,
                },
            )
            .await
            .expect("stop before a next item that exceeds remaining bytes");
        assert_eq!(byte_limited.entries.len(), 1);
        assert_eq!(byte_limited.next_cursor, count_limited.next_cursor);
        assert!(byte_limited.has_more);

        assert_eq!(
            scenario
                .vault()
                .read_conversation_page(
                    target.clone(),
                    None,
                    TranscriptPageBudget {
                        max_entries: 8,
                        max_bytes: count_limited.encoded_bytes - 1,
                    },
                )
                .await,
            Err(ConversationStoreFailure::PageItemExceedsBudget),
            "an oversized first entry fails without advancing a cursor"
        );
        let retried_first = scenario
            .vault()
            .read_conversation_page(
                target.clone(),
                None,
                TranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: 4096,
                },
            )
            .await
            .expect("retry from the same first cursor position");
        assert_eq!(retried_first.entries[0].reference, first.receipt.transcript);

        let second_page = scenario
            .vault()
            .read_conversation_page(
                target,
                byte_limited.next_cursor,
                TranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: 4096,
                },
            )
            .await
            .expect("continue from the exact first-item cursor");
        assert_eq!(second_page.entries.len(), 1);
        assert_eq!(second_page.entries[0].reference, second.receipt.transcript);
        assert_eq!(second_page.next_cursor, Some(second.receipt.transcript));
        assert!(!second_page.has_more);
    }

    #[tokio::test]
    async fn active_recorder_excludes_another_composed_admission_atomically() {
        let mut scenario = Scenario::new().await;
        let (first_run, first_input) = scenario
            .admit_run_and_append_input("first active recorder")
            .await;
        let first_recorder = scenario
            .last_core_recorder
            .clone()
            .expect("first composition opened its recorder");
        let second_session = match scenario
            .vault()
            .start_conversation_session(StartSessionRequest {
                principal: scenario.person_id.to_string(),
                command_id: CommandId::new(),
            })
            .await
            .expect("start independent owner Session")
        {
            SessionStartAdmission::Started(receipt) => receipt,
            other => panic!("unexpected second Session admission: {other:?}"),
        };
        let second_text = "second active recorder";
        let second_owner = scenario.owner_request_for_session(
            second_session.session_id,
            second_session.session_revision,
            second_text,
        );
        let mut second_core = scenario.core_input_request(second_text);
        second_core.target = AdmissionTarget::AppendToExisting {
            reference: ConversationReference {
                identity: scenario.identity.clone(),
                conversation_id: scenario.conversation_id,
                branch_id: scenario.branch_id,
                head_revision: first_input.receipt.transcript.sequence,
            },
        };
        let target = ConversationReference {
            identity: scenario.identity.clone(),
            conversation_id: scenario.conversation_id,
            branch_id: scenario.branch_id,
            head_revision: first_input.receipt.transcript.sequence,
        };
        let before = scenario
            .vault()
            .observe_conversation_recorder(target.clone(), first_run.run_id)
            .await
            .expect("observe the recorder opened by admission");
        assert_eq!(before.state, RecorderRecoveryState::ActiveCurrentGeneration);
        assert_eq!(before.active_recorder, Some(first_recorder.fence.clone()));
        let connection = scenario
            .vault()
            .connection()
            .expect("connect to test Vault");
        let before_second_session =
            session_storage_snapshot(&connection, second_session.session_id).await;
        drop(connection);

        assert_eq!(
            compose_owner_core_run(
                scenario.vault(),
                CoreComposedOwnerIntent::Turn(second_owner.clone()),
                second_core.clone(),
            )
            .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::WriterAlreadyActive
            )),
            "the second Run, input and recorder cannot commit while the first recorder is active"
        );
        let connection = scenario
            .vault()
            .connection()
            .expect("reconnect after rejection");
        assert_eq!(table_count(&connection, "agent_conversation_runs").await, 1);
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_entries").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_owner_bindings").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_open_receipts").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_owner_transcript_inputs_v1").await,
            1
        );
        assert_eq!(
            table_count(
                &connection,
                "agent_conversation_owner_transcript_run_inputs_v1"
            )
            .await,
            1
        );
        assert_eq!(
            session_storage_snapshot(&connection, second_session.session_id).await,
            before_second_session
        );
        drop(connection);

        scenario.cancel_owner_run(&first_run).await;
        let (second_run, second_input) = scenario
            .admit_run_and_bind_input(second_owner, second_core)
            .await;
        assert_eq!(second_input.disposition, AdmissionDisposition::Appended);
        let second_recorder = scenario
            .last_core_recorder
            .clone()
            .expect("retry after the first Run closes opens its recorder");
        assert_eq!(second_recorder.fence.run_id, second_run.run_id);
        assert_eq!(second_recorder.fence.input, second_input.receipt.transcript);
        assert!(second_recorder.fence.recorder_epoch > first_recorder.fence.recorder_epoch);
        let target_after = ConversationReference {
            identity: scenario.identity.clone(),
            conversation_id: scenario.conversation_id,
            branch_id: scenario.branch_id,
            head_revision: second_input.receipt.transcript.sequence,
        };
        let after = scenario
            .vault()
            .observe_conversation_recorder(target_after, second_run.run_id)
            .await
            .expect("observe the recorder opened by the admitted retry");
        assert_eq!(after.active_recorder, Some(second_recorder.fence.clone()));
        assert_eq!(after.head.recorder_epoch, before.head.recorder_epoch + 1);
        assert!(after.head.state_revision > before.head.state_revision);
    }

    #[tokio::test]
    async fn reopen_rejects_a_tampered_prefix_without_repairing_it() {
        let mut scenario = Scenario::new().await;
        let request = scenario.core_input_request("tamper prefix commitment");
        let admitted = scenario
            .vault()
            .append_conversation_input(request)
            .await
            .expect("append transcript entry before tamper fixture");
        let connection = scenario
            .vault()
            .connection()
            .expect("connect to test Vault");
        let mut rows = connection
            .query(
                "SELECT prefix_digest FROM agent_conversation_core_v3_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ?",
                (
                    scenario.person_id.to_string(),
                    scenario.conversation_id.as_uuid().to_string(),
                    scenario.branch_id.as_uuid().to_string(),
                    admitted.receipt.transcript.sequence as i64,
                ),
            )
            .await
            .expect("read original prefix digest");
        let original = rows
            .next()
            .await
            .expect("read prefix row")
            .expect("prefix exists")
            .get::<String>(0)
            .expect("prefix digest type");
        let tampered = if original == "0".repeat(64) {
            "f".repeat(64)
        } else {
            "0".repeat(64)
        };
        connection
            .execute(
                "UPDATE agent_conversation_core_v3_entries SET prefix_digest = ? WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ?",
                (
                    tampered.clone(),
                    scenario.person_id.to_string(),
                    scenario.conversation_id.as_uuid().to_string(),
                    scenario.branch_id.as_uuid().to_string(),
                    admitted.receipt.transcript.sequence as i64,
                ),
            )
            .await
            .expect("tamper stored prefix commitment");
        drop(connection);

        let root_path = scenario.root.0.clone();
        let person_id = scenario.person_id;
        let keys = scenario.keys.clone();
        let vault_id = scenario.vault().vault_id;
        let key = keys.load(person_id, vault_id).expect("read fixture key");
        let path = root_path.join(person_id.to_string()).join("sessions.db");
        drop(scenario.vault.take());
        assert_eq!(
            EncryptedAgentVault::open(&root_path, person_id, keys)
                .await
                .err(),
            Some(AgentFailure::VaultUnavailable),
            "reopen verifies the commitment chain and fails closed"
        );

        let database = super::super::encrypted_database(&path, &key)
            .await
            .expect("read raw encrypted test database after failed reopen");
        let connection = database
            .connect()
            .expect("connect to raw encrypted database");
        let mut rows = connection
            .query(
                "SELECT prefix_digest FROM agent_conversation_core_v3_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ?",
                (
                    person_id.to_string(),
                    scenario.conversation_id.as_uuid().to_string(),
                    scenario.branch_id.as_uuid().to_string(),
                    admitted.receipt.transcript.sequence as i64,
                ),
            )
            .await
            .expect("reread stored prefix after failed reopen");
        assert_eq!(
            rows.next()
                .await
                .expect("read retained tampered prefix")
                .expect("tampered entry remains present")
                .get::<String>(0)
                .expect("tampered digest type"),
            tampered,
            "failed reopen does not repair or rewrite tampered data"
        );
    }

    #[tokio::test]
    async fn owner_fencing_and_terminal_evidence_allow_crash_retirement_then_reopen() {
        let mut scenario = Scenario::new().await;
        let (old_run, input) = scenario
            .admit_run_and_append_input("interrupted owner")
            .await;
        let old_open = scenario
            .vault()
            .open_conversation_recorder(scenario.start_request(&old_run, input.receipt.transcript))
            .await
            .expect("open old recorder");
        let terminal = scenario.budget_exceeded_owner_run(&old_run).await;
        let activation = scenario
            .vault()
            .activate_conversation_executor()
            .await
            .expect("persist a newer executor generation");
        scenario.executor_generation = activation.executor_generation;
        let retired = scenario
            .vault()
            .retire_conversation_recorder(old_open.fence.clone())
            .await
            .expect("retire only after generation fencing and terminal evidence");
        assert_eq!(retired.fence, old_open.fence);
        assert_eq!(
            retired.generation_fence.old_generation,
            old_open.fence.executor_generation
        );
        assert_eq!(
            retired.generation_fence.current_generation,
            activation.executor_generation
        );
        assert_eq!(retired.owner_evidence_digest.len(), 32);

        let (new_run, reused_input) = scenario
            .admit_owner_run_reusing_input(&terminal, "Continue interrupted owner")
            .await;
        assert_eq!(reused_input.disposition, AdmissionDisposition::Replayed);
        let new_open = scenario
            .vault()
            .open_conversation_recorder(scenario.start_request(&new_run, input.receipt.transcript))
            .await
            .expect("open new recorder after stale retirement");
        assert!(new_open.fence.recorder_epoch > old_open.fence.recorder_epoch);
        assert_eq!(new_open.fence.input, old_open.fence.input);
        assert!(terminal.state.is_terminal());
    }

    #[tokio::test]
    async fn legacy_core_markers_fail_closed_without_catalog_or_new_api_writes() {
        let mut scenario = Scenario::new().await;
        let vault = scenario.vault();
        let connection = vault.connection().expect("connect to test Vault");
        connection
            .execute("DROP TABLE agent_conversation_core_schema", ())
            .await
            .expect("remove the fresh-family marker for the legacy fixture");
        connection
            .execute(
                "CREATE TABLE agent_conversation_core_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 2))",
                (),
            )
            .await
            .expect("install a legacy stored-meaning marker fixture");
        connection
            .execute(
                "INSERT INTO agent_conversation_core_schema(id, version) VALUES (1, 2)",
                (),
            )
            .await
            .expect("seed the Core 2 marker");
        let before_core_heads = table_count(&connection, "agent_conversation_core_v3_heads").await;
        let before_core_entries =
            table_count(&connection, "agent_conversation_core_v3_entries").await;
        let rejected = scenario
            .vault()
            .append_conversation_input(scenario.core_input_request("must not append"))
            .await;
        assert_eq!(
            rejected,
            Err(ConversationStoreFailure::UnsupportedStoredMeaning)
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_schema").await,
            1
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_heads").await,
            before_core_heads
        );
        assert_eq!(
            table_count(&connection, "agent_conversation_core_v3_entries").await,
            before_core_entries
        );
        drop(connection);

        let root_path = scenario.root.0.clone();
        let person_id = scenario.person_id;
        let keys = scenario.keys.clone();
        let vault_id = scenario.vault().vault_id;
        let key = keys.load(person_id, vault_id).expect("read fixture key");
        let path = root_path.join(person_id.to_string()).join("sessions.db");
        scenario
            .vault()
            .checkpoint()
            .await
            .expect("checkpoint legacy marker fixture");
        let before_open = catalog_snapshot(&path, &key).await;
        drop(scenario.vault.take());
        let opened = EncryptedAgentVault::open(&root_path, person_id, keys.clone()).await;
        assert_eq!(opened.err(), Some(AgentFailure::UnsupportedVersion));
        let after_open = catalog_snapshot(&path, &key).await;
        assert_eq!(
            after_open, before_open,
            "open failure performs no schema writes"
        );
    }
}

async fn checkpoint_on(
    transaction: &Transaction<'_>,
    scope: Scope,
) -> Result<Option<ConversationCheckpoint>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT sequence, prefix_digest, summary FROM agent_conversation_core_v3_checkpoints WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
            (person, conversation, branch),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let sequence = positive_integer(row.get::<i64>(0).map_err(|_| unavailable())?)?;
    let prefix_digest = parse_digest(&row.get::<String>(1).map_err(|_| unavailable())?)?;
    let summary = row.get::<String>(2).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    let through = entry_on(transaction, scope, sequence)
        .await?
        .map(|entry| entry.reference)
        .ok_or_else(unavailable)?;
    let checkpoint = ConversationCheckpoint {
        through,
        prefix_digest,
        summary,
    };
    checkpoint.validate().map_err(|_| unavailable())?;
    Ok(Some(checkpoint))
}

async fn save_checkpoint_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    previous: Option<&ConversationCheckpoint>,
    next: &ConversationCheckpoint,
) -> Result<(), ConversationStoreFailure> {
    if let Some(previous) = previous {
        let changed = transaction
            .execute(
                "UPDATE agent_conversation_core_v3_checkpoints SET sequence = ?, prefix_digest = ?, summary = ? WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ? AND prefix_digest = ? AND summary = ?",
                (
                    integer(next.through.sequence)?,
                    hex_digest(next.prefix_digest),
                    next.summary.clone(),
                    scope.person_id.to_string(),
                    scope.conversation_id.as_uuid().to_string(),
                    scope.branch_id.as_uuid().to_string(),
                    integer(previous.through.sequence)?,
                    hex_digest(previous.prefix_digest),
                    previous.summary.clone(),
                ),
            )
            .await
            .map_err(database_error)?;
        if changed != 1 {
            return Err(ConversationStoreFailure::Transition(
                ConversationFailure::RevisionConflict,
            ));
        }
    } else {
        transaction
            .execute(
                "INSERT INTO agent_conversation_core_v3_checkpoints (person_id, conversation_id, branch_id, sequence, prefix_digest, summary) VALUES (?, ?, ?, ?, ?, ?)",
                (
                    scope.person_id.to_string(),
                    scope.conversation_id.as_uuid().to_string(),
                    scope.branch_id.as_uuid().to_string(),
                    integer(next.through.sequence)?,
                    hex_digest(next.prefix_digest),
                    next.summary.clone(),
                ),
            )
            .await
            .map_err(database_error)?;
    }
    Ok(())
}

async fn v3_orphan_counts_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
) -> Result<(u64, u64), ConversationStoreFailure> {
    let mut rows = transaction
        .query(
            "SELECT (SELECT COUNT(*) FROM agent_conversation_core_v3_input_receipts r LEFT JOIN agent_conversation_core_v3_entries e ON e.person_id = r.person_id AND e.conversation_id = r.conversation_id AND e.branch_id = r.branch_id AND e.sequence = r.sequence WHERE r.person_id = ?1 AND (e.sequence IS NULL OR e.entry_kind <> 'inbound' OR e.message_id <> r.message_id)) + (SELECT COUNT(*) FROM agent_conversation_core_v3_entries e LEFT JOIN agent_conversation_core_v3_heads h ON h.person_id = e.person_id AND h.conversation_id = e.conversation_id AND h.branch_id = e.branch_id WHERE e.person_id = ?1 AND h.person_id IS NULL), (SELECT COUNT(*) FROM agent_conversation_core_output_receipts_v2 o LEFT JOIN agent_conversation_core_v3_recording_receipts r ON r.person_id = o.person_id AND r.contribution_id = o.contribution_id LEFT JOIN agent_conversation_core_v3_entries e ON e.person_id = r.person_id AND e.conversation_id = r.conversation_id AND e.branch_id = r.branch_id AND e.sequence = r.sequence WHERE o.person_id = ?1 AND (r.contribution_id IS NULL OR e.sequence IS NULL OR e.entry_kind <> 'generated_output'))",
            [person_id.to_string()],
        )
        .await
        .map_err(database_error)?;
    let row = rows
        .next()
        .await
        .map_err(database_error)?
        .ok_or_else(unavailable)?;
    let inputs = nonnegative_integer(row.get::<i64>(0).map_err(|_| unavailable())?)?;
    let outputs = nonnegative_integer(row.get::<i64>(1).map_err(|_| unavailable())?)?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    Ok((inputs, outputs))
}

fn owner_generation_fence(
    domain: ExecutorDomain,
    run_id: RunId,
    old_generation: u64,
    current_generation: u64,
) -> Result<OwnerGenerationFence, ConversationStoreFailure> {
    if current_generation <= old_generation {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::OwnerEvidenceMismatch,
        ));
    }
    let bytes = serde_json::to_vec(&(
        "floe-host-run-generation-fence-v1",
        domain,
        run_id,
        old_generation,
        current_generation,
        current_generation,
    ))
    .map_err(|_| unavailable())?;
    Ok(OwnerGenerationFence {
        domain,
        old_generation,
        current_generation,
        fence_revision: current_generation,
        evidence_digest: Sha256::digest(bytes).into(),
    })
}
