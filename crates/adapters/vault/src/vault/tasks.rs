use super::database_failure;
use floe_agent_contract::{
    AgentFailure, DependencyCoverage, JournalEntry, JournalEvent, MAX_AGENT_MESSAGES,
    MAX_MODEL_CONVERSATION_BYTES, MAX_OUTPUT_BYTES, ModelConversation, ModelConversationEntry,
    TaskExecutionKey, TaskExecutionReceipt, TaskExecutionReceiptRef, TaskId, TaskSnapshot,
    TaskState,
};
use floe_conversation_contract::{
    AgentIdentity, ConversationBranchId, ConversationId, ConversationMessage,
    MessageEvidenceReference, MessageId, MessageOrigin, TaskEvidenceReference, TranscriptReference,
};
use floe_conversation_core::{
    EMPTY_PREFIX_DIGEST, ExecutorDomain, OwnerRunEvidence, OwnerRunState, RecorderStartRequest,
    RecordingRequest, TranscriptEntryKind, advance_core_prefix_digest,
};
use floe_experts::{
    ExpertConversationEvidence, ExpertConversationKey, ExpertSettlement,
    ExpertTaskAdmissionReference, ExpertTaskConversation, ExpertTaskConversationDraft,
    ExpertTaskConversationInput, MAX_TASK_RECORD_BYTES, TaskActivation, TaskAdmission,
    TaskExecutionCommit, TaskRecord, advance_task_journal, interrupt_task_execution,
    settle_task_execution as settle_task_record_execution, validate_task_journal,
};
use floe_kernel::{CommandId, RunId};
use sha2::{Digest, Sha256};
use turso::transaction::{Transaction, TransactionBehavior};

use super::*;

const MAX_TASK_JOURNAL_ENTRY_BYTES: usize = 128 * 1024;
const MAX_TASK_JOURNAL_ENTRIES: usize = 512;
const MAX_TASK_ROWS: i64 = 4_096;

fn commitment_hex(commitment: [u8; 32]) -> String {
    commitment
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_commitment(value: &str) -> Result<[u8; 32], AgentFailure> {
    if value.len() != 64 {
        return Err(AgentFailure::StorageUnavailable);
    }
    let mut digest = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|_| AgentFailure::StorageUnavailable)?;
        digest[index] =
            u8::from_str_radix(text, 16).map_err(|_| AgentFailure::StorageUnavailable)?;
    }
    if commitment_hex(digest) != value {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(digest)
}

fn map_constraint(error: turso::Error) -> AgentFailure {
    match error {
        turso::Error::Constraint(_) => AgentFailure::Conflict,
        other => database_failure(other),
    }
}

fn nonnegative_integer(value: i64) -> Result<u64, AgentFailure> {
    u64::try_from(value).map_err(|_| AgentFailure::StorageUnavailable)
}

async fn ensure_expert_task_conversation_family_on(
    transaction: &Transaction<'_>,
) -> Result<(), AgentFailure> {
    if crate::schema::expert_task_conversations_family_v1_present(transaction)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)?
    {
        return Ok(());
    }
    let mut rows = transaction
        .query("SELECT count(*) FROM agent_tasks", ())
        .await
        .map_err(database_failure)?;
    let existing = rows
        .next()
        .await
        .map_err(database_failure)?
        .ok_or(AgentFailure::StorageUnavailable)?
        .get::<i64>(0)
        .map_err(storage)?;
    drop(rows);
    if existing != 0 {
        return Err(AgentFailure::UnsupportedVersion);
    }
    crate::schema::ensure_expert_task_conversations_family_v1(transaction)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)
}

fn expert_binding_key(key: &ExpertConversationKey) -> Result<String, AgentFailure> {
    key.validate()?;
    let encoded = serde_json::to_vec(key).map_err(storage)?;
    let mut digest = Sha256::new();
    digest.update(b"floe-expert-conversation-binding-v1\0");
    digest.update((encoded.len() as u64).to_be_bytes());
    digest.update(encoded);
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

async fn load_expert_binding_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    binding_key: &str,
) -> Result<
    Option<(
        ExpertConversationKey,
        AgentIdentity,
        ConversationId,
        ConversationBranchId,
    )>,
    AgentFailure,
> {
    let mut rows = transaction
        .query(
            "SELECT key_json, identity_json, conversation_id, branch_id FROM agent_expert_task_conversation_bindings_v1 WHERE person_id = ? AND binding_key = ?",
            (person_id.to_string(), binding_key),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    let key_json = row.get::<String>(0).map_err(storage)?;
    let identity_json = row.get::<String>(1).map_err(storage)?;
    let conversation_id = ConversationId::from_uuid(
        Uuid::parse_str(&row.get::<String>(2).map_err(storage)?).map_err(storage)?,
    )
    .ok_or(AgentFailure::StorageUnavailable)?;
    let branch_id = ConversationBranchId::from_uuid(
        Uuid::parse_str(&row.get::<String>(3).map_err(storage)?).map_err(storage)?,
    )
    .ok_or(AgentFailure::StorageUnavailable)?;
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    let key: ExpertConversationKey = serde_json::from_str(&key_json).map_err(storage)?;
    let identity: AgentIdentity = serde_json::from_str(&identity_json).map_err(storage)?;
    key.validate()?;
    identity
        .validate()
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    if expert_binding_key(&key)? != binding_key
        || identity != key.core_identity()?
        || key.person_id != person_id
        || serde_json::to_string(&key).map_err(storage)? != key_json
        || serde_json::to_string(&identity).map_err(storage)? != identity_json
        || !conversation_id.is_valid()
        || !branch_id.is_valid()
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some((key, identity, conversation_id, branch_id)))
}

async fn task_conversation_input_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    execution: TaskExecutionKey,
) -> Result<Option<ExpertTaskConversationInput>, AgentFailure> {
    execution.validate()?;
    let mut rows = transaction
        .query(
            "SELECT binding_key, conversation_id, branch_id, run_id, input_json, input_commitment, input_sequence, input_message_id, input_reference_json FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ?",
            (
                person_id.to_string(),
                execution.task_id.as_uuid().to_string(),
                execution.execution_id.to_string(),
                integer(execution.executor_generation)?,
            ),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    let binding_key = row.get::<String>(0).map_err(storage)?;
    let conversation_id = ConversationId::from_uuid(
        Uuid::parse_str(&row.get::<String>(1).map_err(storage)?).map_err(storage)?,
    )
    .ok_or(AgentFailure::StorageUnavailable)?;
    let branch_id = ConversationBranchId::from_uuid(
        Uuid::parse_str(&row.get::<String>(2).map_err(storage)?).map_err(storage)?,
    )
    .ok_or(AgentFailure::StorageUnavailable)?;
    let run_id = RunId::from_uuid(
        Uuid::parse_str(&row.get::<String>(3).map_err(storage)?).map_err(storage)?,
    )
    .ok_or(AgentFailure::StorageUnavailable)?;
    let payload = row.get::<String>(4).map_err(storage)?;
    let input_commitment = parse_commitment(&row.get::<String>(5).map_err(storage)?)?;
    if payload.is_empty() || payload.len() > 262_144 {
        return Err(AgentFailure::StorageUnavailable);
    }
    let mut input: ExpertTaskConversationInput = serde_json::from_str(&payload).map_err(storage)?;
    let sequence = row.get::<Option<i64>>(6).map_err(storage)?;
    let message_id = row.get::<Option<String>>(7).map_err(storage)?;
    let input_reference_json = row.get::<Option<String>>(8).map_err(storage)?;
    input.input_reference = match (sequence, message_id, input_reference_json) {
        (None, None, None) => None,
        (Some(sequence), Some(message_id), Some(reference_json)) if sequence > 0 => {
            let reference: TranscriptReference =
                serde_json::from_str(&reference_json).map_err(storage)?;
            if reference.sequence != sequence as u64
                || reference.message_id
                    != MessageId::from_uuid(Uuid::parse_str(&message_id).map_err(storage)?)
                        .ok_or(AgentFailure::StorageUnavailable)?
                || serde_json::to_string(&reference).map_err(storage)? != reference_json
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            Some(reference)
        }
        _ => return Err(AgentFailure::StorageUnavailable),
    };
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    drop(rows);
    let (bound_key, bound_identity, bound_conversation, bound_branch) =
        load_expert_binding_on(transaction, person_id, &binding_key)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
    let owner_reference = load_expert_admission_reference_on(transaction, person_id, execution)
        .await?
        .ok_or(AgentFailure::StorageUnavailable)?;
    input.validate()?;
    if input.execution != execution
        || input.request_digest != owner_reference.request_digest
        || input.run_id != run_id
        || input.conversation_id != conversation_id
        || input.branch_id != branch_id
        || input.key.person_id != person_id
        || input.key != bound_key
        || input.identity != bound_identity
        || input.conversation_id != bound_conversation
        || input.branch_id != bound_branch
        || input.commitment()? != input_commitment
        || owner_reference.execution != input.execution
        || owner_reference.run_id != input.run_id
        || owner_reference.conversation_id != input.conversation_id
        || owner_reference.branch_id != input.branch_id
        || owner_reference.history_pin != input.history_pin
        || owner_reference.host_input_commitment != input_commitment
        || serde_json::to_string(&input).map_err(storage)? != payload
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some(input))
}

/// Read the current canonical Task input's byte bound without hydrating its
/// JSON. History resolution calls this before decoding current owner coverage
/// or any pinned typed history payload.
async fn bounded_expert_task_input_bytes_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    execution: TaskExecutionKey,
) -> Result<usize, AgentFailure> {
    execution.validate()?;
    let mut rows = transaction
        .query(
            "SELECT length(CAST(input_json AS BLOB)) FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ?",
            (
                person_id.to_string(),
                execution.task_id.as_uuid().to_string(),
                execution.execution_id.to_string(),
                integer(execution.executor_generation)?,
            ),
        )
        .await
        .map_err(database_failure)?;
    let row = rows
        .next()
        .await
        .map_err(database_failure)?
        .ok_or(AgentFailure::StorageUnavailable)?;
    let bytes = nonnegative_integer(row.get::<i64>(0).map_err(storage)?)?;
    if bytes == 0 || rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    if bytes > MAX_MODEL_CONVERSATION_BYTES as u64 {
        return Err(AgentFailure::BudgetExceeded);
    }
    usize::try_from(bytes).map_err(|_| AgentFailure::BudgetExceeded)
}

async fn load_expert_admission_reference_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    execution: TaskExecutionKey,
) -> Result<Option<ExpertTaskAdmissionReference>, AgentFailure> {
    execution.validate()?;
    let mut rows = transaction
        .query(
            "SELECT request_digest, input_commitment, reference_json FROM agent_expert_task_conversation_admissions_v1 WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ?",
            (
                person_id.to_string(),
                execution.task_id.as_uuid().to_string(),
                execution.execution_id.to_string(),
                integer(execution.executor_generation)?,
            ),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    let request_digest = parse_commitment(&row.get::<String>(0).map_err(storage)?)?;
    let input_commitment = parse_commitment(&row.get::<String>(1).map_err(storage)?)?;
    let payload = row.get::<String>(2).map_err(storage)?;
    if payload.is_empty() || payload.len() > 8192 {
        return Err(AgentFailure::StorageUnavailable);
    }
    let reference: ExpertTaskAdmissionReference =
        serde_json::from_str(&payload).map_err(storage)?;
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    reference.validate()?;
    if reference.execution != execution
        || reference.request_digest != request_digest
        || reference.host_input_commitment != input_commitment
        || serde_json::to_string(&reference).map_err(storage)? != payload
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some(reference))
}

async fn load_expert_task_open_receipt_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    record: &TaskRecord,
) -> Result<Option<String>, AgentFailure> {
    let mut rows = transaction
        .query(
            "SELECT open_receipt_json FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ?",
            (
                person_id.to_string(),
                record.snapshot.task_id.as_uuid().to_string(),
                record.execution_id.to_string(),
                integer(record.executor_generation)?,
            ),
        )
        .await
        .map_err(database_failure)?;
    let row = rows
        .next()
        .await
        .map_err(database_failure)?
        .ok_or(AgentFailure::StorageUnavailable)?;
    let receipt = row.get::<Option<String>>(0).map_err(storage)?;
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(receipt)
}

async fn store_expert_task_close_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    record: &TaskRecord,
    receipt: &floe_conversation_core::RecorderCloseReceipt,
) -> Result<(), AgentFailure> {
    if receipt.fence.validate().is_err()
        || receipt.fence.executor_domain != ExecutorDomain::TaskExecution
        || receipt.fence.execution_task != Some(record.snapshot.task_id)
        || receipt.fence.executor_generation != record.executor_generation
        || receipt.terminal_receipt_digest == [0; 32]
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    let payload = serde_json::to_string(receipt).map_err(storage)?;
    let changed = transaction
        .execute(
            "UPDATE agent_expert_task_conversation_runs_v1 SET close_receipt_json = ? WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ? AND close_receipt_json IS NULL AND retirement_receipt_json IS NULL",
            (
                payload,
                person_id.to_string(),
                record.snapshot.task_id.as_uuid().to_string(),
                record.execution_id.to_string(),
                integer(record.executor_generation)?,
            ),
        )
        .await
        .map_err(database_failure)?;
    if changed != 1 {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}

async fn store_expert_task_retirement_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    record: &TaskRecord,
    receipt: &floe_conversation_core::RecorderRetirementReceipt,
) -> Result<(), AgentFailure> {
    if receipt.fence.validate().is_err()
        || receipt.fence.executor_domain != ExecutorDomain::TaskExecution
        || receipt.fence.execution_task != Some(record.snapshot.task_id)
        || receipt.fence.executor_generation != record.executor_generation
        || receipt.generation_fence.domain != ExecutorDomain::TaskExecution
        || receipt.generation_fence.old_generation != record.executor_generation
        || receipt.generation_fence.current_generation <= record.executor_generation
        || receipt.generation_fence_digest != receipt.generation_fence.evidence_digest
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    let payload = serde_json::to_string(receipt).map_err(storage)?;
    let changed = transaction
        .execute(
            "UPDATE agent_expert_task_conversation_runs_v1 SET retirement_receipt_json = ? WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ? AND close_receipt_json IS NULL AND retirement_receipt_json IS NULL",
            (
                payload,
                person_id.to_string(),
                record.snapshot.task_id.as_uuid().to_string(),
                record.execution_id.to_string(),
                integer(record.executor_generation)?,
            ),
        )
        .await
        .map_err(database_failure)?;
    if changed != 1 {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}

async fn expert_input_for_reference_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    conversation_id: ConversationId,
    branch_id: ConversationBranchId,
    reference: TranscriptReference,
) -> Result<Option<ExpertTaskConversationInput>, AgentFailure> {
    let mut rows = transaction
        .query(
            "SELECT task_id, execution_id, executor_generation FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND input_sequence = ? AND input_message_id = ?",
            (
                person_id.to_string(),
                conversation_id.as_uuid().to_string(),
                branch_id.as_uuid().to_string(),
                integer(reference.sequence)?,
                reference.message_id.as_uuid().to_string(),
            ),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    let task_id = TaskId::from_uuid(
        Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(storage)?,
    )
    .ok_or(AgentFailure::StorageUnavailable)?;
    let execution_id = Uuid::parse_str(&row.get::<String>(1).map_err(storage)?).map_err(storage)?;
    let generation = nonnegative_integer(row.get::<i64>(2).map_err(storage)?)?;
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    drop(rows);
    let input = task_conversation_input_on(
        transaction,
        person_id,
        TaskExecutionKey {
            task_id,
            execution_id,
            executor_generation: generation,
        },
    )
    .await?
    .ok_or(AgentFailure::StorageUnavailable)?;
    if input.input_reference != Some(reference) {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some(input))
}

async fn expert_typed_entry_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    conversation_id: ConversationId,
    branch_id: ConversationBranchId,
    sequence: u64,
) -> Result<
    Option<(
        ExpertConversationEvidence,
        DependencyCoverage,
        TaskId,
        u64,
        u64,
        [u8; 32],
    )>,
    AgentFailure,
> {
    let mut rows = transaction
        .query(
            "SELECT task_id, executor_generation, journal_revision, event_digest, coverage_json, evidence_json, evidence_bytes FROM agent_expert_task_conversation_entries_v1 WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ?",
            (
                person_id.to_string(),
                conversation_id.as_uuid().to_string(),
                branch_id.as_uuid().to_string(),
                integer(sequence)?,
            ),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    let task_id = TaskId::from_uuid(
        Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(storage)?,
    )
    .ok_or(AgentFailure::StorageUnavailable)?;
    let generation = nonnegative_integer(row.get::<i64>(1).map_err(storage)?)?;
    let journal_revision = nonnegative_integer(row.get::<i64>(2).map_err(storage)?)?;
    let event_digest = parse_hex_digest(&row.get::<String>(3).map_err(storage)?)?;
    let coverage_json = row.get::<String>(4).map_err(storage)?;
    let evidence_json = row.get::<String>(5).map_err(storage)?;
    let evidence_bytes = usize::try_from(row.get::<i64>(6).map_err(storage)?)
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    drop(rows);
    if evidence_json.is_empty()
        || evidence_json.len() != evidence_bytes
        || evidence_bytes > MAX_MODEL_CONVERSATION_BYTES
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    let evidence: ExpertConversationEvidence =
        serde_json::from_str(&evidence_json).map_err(storage)?;
    let coverage: DependencyCoverage = serde_json::from_str(&coverage_json).map_err(storage)?;
    evidence.validate()?;
    coverage
        .validate()
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    let evidence_coverage = match &evidence {
        ExpertConversationEvidence::ModelEntry { coverage, .. }
        | ExpertConversationEvidence::Blocked { coverage, .. } => coverage,
    };
    if evidence_coverage != &coverage
        || serde_json::to_string(&evidence).map_err(storage)? != evidence_json
        || serde_json::to_string(&coverage).map_err(storage)? != coverage_json
        || generation == 0
        || (journal_revision == 0
            && !matches!(evidence, ExpertConversationEvidence::Blocked { .. }))
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some((
        evidence,
        coverage,
        task_id,
        generation,
        journal_revision,
        event_digest,
    )))
}

fn parse_hex_digest(value: &str) -> Result<[u8; 32], AgentFailure> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AgentFailure::StorageUnavailable);
    }
    let mut digest = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or(AgentFailure::StorageUnavailable)?;
        let low = hex_nibble(pair[1]).ok_or(AgentFailure::StorageUnavailable)?;
        digest[index] = (high << 4) | low;
    }
    Ok(digest)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn same_draft(input: &ExpertTaskConversationInput, draft: &ExpertTaskConversationDraft) -> bool {
    input.key == draft.key
        && input.run_id == draft.run_id
        && input.input_message_id == draft.input_message_id
        && input.input_command_id == draft.input_command_id
        && input.delegated_message == draft.delegated_message
        && input.input_coverage == draft.input_coverage
}

fn expert_task_evidence_reference_at(
    record: &TaskRecord,
    journal: &[JournalEntry],
    revision: u64,
) -> Result<(TaskEvidenceReference, [u8; 32]), AgentFailure> {
    if revision == 0 || revision > journal.len() as u64 {
        return Err(AgentFailure::StorageUnavailable);
    }
    let event = &journal[usize::try_from(revision - 1).map_err(storage)?].event;
    let encoded = serde_json::to_vec(event).map_err(storage)?;
    let event_digest: [u8; 32] = Sha256::digest(&encoded).into();
    let proof = serde_json::to_vec(&(
        "floe-expert-task-event-evidence-v1",
        record.execution(),
        revision,
        event_digest,
    ))
    .map_err(storage)?;
    let digest = Sha256::digest(proof).into();
    Ok((
        TaskEvidenceReference::from_digest(record.snapshot.task_id, digest),
        event_digest,
    ))
}

fn expert_task_terminal_reference(
    record: &TaskRecord,
) -> Result<(TaskEvidenceReference, [u8; 32]), AgentFailure> {
    if !terminal_task_state(record.snapshot.state) {
        return Err(AgentFailure::Conflict);
    }
    let receipt = record
        .receipt
        .as_ref()
        .ok_or(AgentFailure::StorageUnavailable)?;
    receipt.validate(floe_agent_contract::MAX_TASK_EXECUTION_RECEIPT_BYTES)?;
    let proof = serde_json::to_vec(&(
        "floe-expert-task-terminal-evidence-v1",
        record.execution(),
        &receipt.reference,
    ))
    .map_err(storage)?;
    let digest = Sha256::digest(proof).into();
    Ok((
        TaskEvidenceReference::from_digest(record.snapshot.task_id, digest),
        receipt.reference.digest,
    ))
}

fn expert_task_model_entry_at(
    run_id: RunId,
    journal: &[JournalEntry],
    revision: u64,
) -> Result<(ModelConversationEntry, String), AgentFailure> {
    if revision == 0 || revision > journal.len() as u64 {
        return Err(AgentFailure::StorageUnavailable);
    }
    let event = &journal[usize::try_from(revision - 1).map_err(storage)?].event;
    match event {
        JournalEvent::Output { text, .. } => Ok((
            ModelConversationEntry::Assistant {
                message_id: expert_task_message_id(run_id, revision, "assistant"),
                text: text.clone(),
            },
            text.clone(),
        )),
        JournalEvent::ToolResult { result } => {
            let call = journal[..usize::try_from(revision - 1).map_err(storage)?]
                .iter()
                .filter_map(|entry| match &entry.event {
                    JournalEvent::ToolIntent { call } if call.call_id == result.call_id => {
                        Some(call.clone())
                    }
                    _ => None,
                })
                .next()
                .ok_or(AgentFailure::StorageUnavailable)?;
            Ok((
                ModelConversationEntry::ToolExchange {
                    call,
                    result: result.clone(),
                },
                result.text.clone(),
            ))
        }
        _ => Err(AgentFailure::StorageUnavailable),
    }
}

fn terminal_task_state(state: TaskState) -> bool {
    matches!(
        state,
        TaskState::Completed
            | TaskState::Blocked
            | TaskState::Failed
            | TaskState::Rejected
            | TaskState::Cancelled
            | TaskState::TimedOut
            | TaskState::Interrupted
    )
}

fn existing_task_event(
    journal: &[JournalEntry],
    proposed: &JournalEvent,
) -> Result<Option<u64>, AgentFailure> {
    let encoded = serde_json::to_vec(proposed).map_err(storage)?;
    let mut matching = journal
        .iter()
        .filter(|entry| match (&entry.event, proposed) {
            (
                JournalEvent::ModelIntent {
                    attempt_id: left, ..
                },
                JournalEvent::ModelIntent {
                    attempt_id: right, ..
                },
            )
            | (
                JournalEvent::ModelResult {
                    attempt_id: left, ..
                },
                JournalEvent::ModelResult {
                    attempt_id: right, ..
                },
            ) => left == right,
            (JournalEvent::ToolIntent { call: left }, JournalEvent::ToolIntent { call: right }) => {
                left.call_id == right.call_id
            }
            (
                JournalEvent::ToolResult { result: left },
                JournalEvent::ToolResult { result: right },
            ) => left.call_id == right.call_id,
            (
                JournalEvent::ToolReviewRequired { call_id: left, .. },
                JournalEvent::ToolReviewRequired { call_id: right, .. },
            ) => left == right,
            (
                JournalEvent::DelegationIntent { request: left },
                JournalEvent::DelegationIntent { request: right },
            ) => left.task_id == right.task_id,
            (
                JournalEvent::DelegationResult { receipt: left },
                JournalEvent::DelegationResult { receipt: right },
            ) => left.task_id == right.task_id,
            (JournalEvent::Output { .. }, JournalEvent::Output { .. }) => true,
            (
                JournalEvent::Checkpoint { iteration: left },
                JournalEvent::Checkpoint { iteration: right },
            ) => left == right,
            (
                JournalEvent::FinalizationStarted {
                    prior_execution_id: left,
                    ..
                },
                JournalEvent::FinalizationStarted {
                    prior_execution_id: right,
                    ..
                },
            ) => left == right,
            (
                JournalEvent::ValidatedBatch { batch: left },
                JournalEvent::ValidatedBatch { batch: right },
            ) => left.batch_id == right.batch_id,
            (
                JournalEvent::BatchProgress { cursor: left },
                JournalEvent::BatchProgress { cursor: right },
            ) => left == right,
            _ => false,
        });
    let Some(entry) = matching.next() else {
        return Ok(None);
    };
    if matching.next().is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    if serde_json::to_vec(&entry.event).map_err(storage)? != encoded {
        return Err(AgentFailure::Conflict);
    }
    Ok(Some(entry.revision))
}

fn expert_task_message_id(run_id: RunId, revision: u64, kind: &str) -> Uuid {
    Uuid::new_v5(
        &run_id.as_uuid(),
        format!("floe.expert.message:{revision}:{kind}").as_bytes(),
    )
}

fn expert_task_command_id(run_id: RunId, revision: u64, kind: &str) -> Uuid {
    Uuid::new_v5(
        &run_id.as_uuid(),
        format!("floe.expert.command:{revision}:{kind}").as_bytes(),
    )
}

fn expert_task_contribution_id(run_id: RunId, revision: u64, kind: &str) -> Uuid {
    Uuid::new_v5(
        &run_id.as_uuid(),
        format!("floe.expert.contribution:{revision}:{kind}").as_bytes(),
    )
}

fn hex_digest(value: [u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn expert_journal_coverage(journal: &[JournalEntry]) -> Result<DependencyCoverage, AgentFailure> {
    journal
        .iter()
        .try_fold(DependencyCoverage::Independent, |coverage, entry| {
            let observed = match &entry.event {
                JournalEvent::ToolResult { result } => Some(&result.coverage),
                JournalEvent::ValidatedBatch { batch } => Some(&batch.projection_coverage),
                _ => None,
            };
            match observed {
                Some(observed) => coverage
                    .merge(observed)
                    .map_err(|_| AgentFailure::StorageUnavailable),
                None => Ok(coverage),
            }
        })
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn expert_task_admission_reference(
        &self,
        execution: TaskExecutionKey,
    ) -> Result<ExpertTaskAdmissionReference, AgentFailure> {
        execution.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let record = self
                .task_on(&transaction, execution.task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if record.execution() != execution {
                return Err(AgentFailure::Conflict);
            }
            let input = task_conversation_input_on(&transaction, self.person_id, execution)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let reference =
                load_expert_admission_reference_on(&transaction, self.person_id, execution)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
            if record.request_digest != reference.request_digest
                || input.request_digest != reference.request_digest
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            Ok(reference)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn expert_task_conversation(
        &self,
        execution: TaskExecutionKey,
    ) -> Result<ExpertTaskConversation, AgentFailure> {
        execution.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            super::conversation_core::require_core_v3_on(&transaction)
                .await
                .map_err(super::conversation_core::owner_store_failure)?;
            if !crate::schema::expert_task_conversations_family_v1_present(&transaction)
                .await
                .map_err(crate::schema::SchemaFailure::into_agent)?
            {
                return Err(AgentFailure::UnsupportedVersion);
            }
            let record = self
                .task_on(&transaction, execution.task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if record.execution() != execution || record.snapshot.state != TaskState::Working {
                return Err(AgentFailure::Conflict);
            }
            let current_input_bytes =
                bounded_expert_task_input_bytes_on(&transaction, self.person_id, execution)
                    .await?;
            let input = task_conversation_input_on(&transaction, self.person_id, execution)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if input.request_digest != record.request_digest {
                return Err(AgentFailure::StorageUnavailable);
            }
            let current_input = input
                .input_reference
                .ok_or(AgentFailure::StorageUnavailable)?;
            if input.identity.person_id != self.person_id
                || input.history_pin.head_revision.saturating_add(1) != current_input.sequence
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            let reservation = transaction
                .query(
                    "SELECT task_id, execution_id, executor_generation FROM agent_expert_task_conversation_reservations_v1 WHERE person_id = ? AND binding_key = ?",
                    (
                        self.person_id.to_string(),
                        expert_binding_key(&input.key)?,
                    ),
                )
                .await
                .map_err(database_failure)?;
            let mut reservation = reservation;
            let row = reservation
                .next()
                .await
                .map_err(database_failure)?
                .ok_or(AgentFailure::Conflict)?;
            let reservation_task = TaskId::from_uuid(
                Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(storage)?,
            )
            .ok_or(AgentFailure::StorageUnavailable)?;
            let reservation_execution = Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
                .map_err(storage)?;
            let reservation_generation = nonnegative_integer(row.get::<i64>(2).map_err(storage)?)?;
            if reservation.next().await.map_err(database_failure)?.is_some()
                || reservation_task != execution.task_id
                || reservation_execution != execution.execution_id
                || reservation_generation != execution.executor_generation
            {
                return Err(AgentFailure::Conflict);
            }
            drop(reservation);
            let scope = super::conversation_core::Scope::from_identity(
                self.person_id,
                &input.identity,
                input.conversation_id,
                input.branch_id,
            );
            let head = super::conversation_core::load_head_on(&transaction, scope)
                .await
                .map_err(super::conversation_core::owner_store_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if head.state.identity != input.identity
                || head.state.head_revision != current_input.sequence
            {
                return Err(AgentFailure::Conflict);
            }
            let current_entry = super::conversation_core::entry_on(
                &transaction,
                scope,
                current_input.sequence,
            )
            .await
            .map_err(super::conversation_core::owner_store_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?;
            if current_entry.reference != current_input
                || current_entry.kind != TranscriptEntryKind::Inbound
                || current_entry.message.task_id != Some(execution.task_id)
                || current_entry.message.text != input.delegated_message
                || current_entry.message.origin != MessageOrigin::Host
                || current_entry
                    .message
                    .evidence
                    .as_ref()
                    .map(MessageEvidenceReference::digest)
                    != Some(input.commitment()?)
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            input.history_pin.validate(input.conversation_id, input.branch_id)?;
            if input.history_pin.head_revision > 0 {
                let pinned = super::conversation_core::entry_on(
                    &transaction,
                    scope,
                    input.history_pin.head_revision,
                )
                .await
                .map_err(super::conversation_core::owner_store_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?;
                if Some(pinned.reference) != input.history_pin.head_reference
                    || pinned.prefix_digest != input.history_pin.prefix_digest
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            let mut total_bytes = current_input_bytes;
            let candidate_count = usize::try_from(
                input
                    .history_pin
                    .head_revision
                    .min((MAX_AGENT_MESSAGES - 1) as u64),
            )
            .map_err(|_| AgentFailure::BudgetExceeded)?;
            let mut selected_sequences = Vec::with_capacity(candidate_count);
            if candidate_count > 0 {
                let mut bounds = transaction
                    .query(
                        "SELECT e.sequence, e.entry_kind, e.message_bytes, length(CAST(e.message_json AS BLOB)), t.sequence, t.evidence_bytes, length(CAST(t.evidence_json AS BLOB)), length(CAST(t.coverage_json AS BLOB)), r.task_id, length(CAST(r.input_json AS BLOB)) FROM agent_conversation_core_v3_entries e LEFT JOIN agent_expert_task_conversation_entries_v1 t ON t.person_id = e.person_id AND t.conversation_id = e.conversation_id AND t.branch_id = e.branch_id AND t.sequence = e.sequence LEFT JOIN agent_expert_task_conversation_runs_v1 r ON r.person_id = e.person_id AND r.conversation_id = e.conversation_id AND r.branch_id = e.branch_id AND r.input_sequence = e.sequence AND r.input_message_id = e.message_id WHERE e.person_id = ? AND e.conversation_id = ? AND e.branch_id = ? AND e.sequence <= ? ORDER BY e.sequence DESC LIMIT ?",
                        (
                            self.person_id.to_string(),
                            input.conversation_id.as_uuid().to_string(),
                            input.branch_id.as_uuid().to_string(),
                            integer(input.history_pin.head_revision)?,
                            candidate_count as i64,
                        ),
                    )
                    .await
                    .map_err(database_failure)?;
                let mut expected_sequence = input.history_pin.head_revision;
                for _ in 0..candidate_count {
                    let row = bounds
                        .next()
                        .await
                        .map_err(database_failure)?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    let sequence = nonnegative_integer(row.get::<i64>(0).map_err(storage)?)?;
                    let kind = row.get::<String>(1).map_err(storage)?;
                    let stored_message_bytes = nonnegative_integer(row.get::<i64>(2).map_err(storage)?)?;
                    let actual_message_bytes = nonnegative_integer(row.get::<i64>(3).map_err(storage)?)?;
                    if sequence != expected_sequence || stored_message_bytes != actual_message_bytes {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    let typed_sequence = row.get::<Option<i64>>(4).map_err(storage)?;
                    let typed_stored_bytes = row.get::<Option<i64>>(5).map_err(storage)?;
                    let typed_actual_bytes = row.get::<Option<i64>>(6).map_err(storage)?;
                    let typed_coverage_bytes = row.get::<Option<i64>>(7).map_err(storage)?;
                    let input_task_id = row.get::<Option<String>>(8).map_err(storage)?;
                    let input_json_bytes = row.get::<Option<i64>>(9).map_err(storage)?;
                    let mut entry_bytes = usize::try_from(actual_message_bytes)
                        .map_err(|_| AgentFailure::BudgetExceeded)?;
                    match kind.as_str() {
                        "inbound" => {
                            if typed_sequence.is_some()
                                || input_task_id.is_none()
                                || input_json_bytes.is_none()
                            {
                                return Err(AgentFailure::StorageUnavailable);
                            }
                            entry_bytes = entry_bytes
                                .checked_add(usize::try_from(input_json_bytes.unwrap())
                                    .map_err(|_| AgentFailure::BudgetExceeded)?)
                                .ok_or(AgentFailure::BudgetExceeded)?;
                        }
                        "generated_output" => {
                            let (Some(typed_sequence), Some(stored), Some(actual), Some(coverage)) =
                                (typed_sequence, typed_stored_bytes, typed_actual_bytes, typed_coverage_bytes)
                            else {
                                return Err(AgentFailure::StorageUnavailable);
                            };
                            if typed_sequence != sequence as i64
                                || stored != actual
                                || input_task_id.is_some()
                                || input_json_bytes.is_some()
                            {
                                return Err(AgentFailure::StorageUnavailable);
                            }
                            entry_bytes = entry_bytes
                                .checked_add(usize::try_from(actual).map_err(|_| AgentFailure::BudgetExceeded)?)
                                .and_then(|total| total.checked_add(usize::try_from(coverage).ok()?))
                                .ok_or(AgentFailure::BudgetExceeded)?;
                        }
                        _ => return Err(AgentFailure::StorageUnavailable),
                    }
                    if total_bytes.checked_add(entry_bytes)
                        .is_none_or(|total| total > MAX_MODEL_CONVERSATION_BYTES)
                    {
                        break;
                    }
                    total_bytes += entry_bytes;
                    selected_sequences.push(sequence);
                    expected_sequence = expected_sequence.saturating_sub(1);
                }
                drop(bounds);
                if selected_sequences.len() != 0 {
                    let expected_newest = input.history_pin.head_revision;
                    if selected_sequences[0] != expected_newest
                        || selected_sequences
                            .windows(2)
                            .any(|pair| pair[0] != pair[1].saturating_add(1))
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
            }
            let count = selected_sequences.len();
            if input.history_pin.head_revision > 0 && count == 0 {
                return Err(AgentFailure::BudgetExceeded);
            }
            let start_sequence = input
                .history_pin
                .head_revision
                .saturating_sub(count as u64);

            let mut history = Vec::with_capacity(count);
            let mut history_coverage = Vec::with_capacity(count);
            let mut previous = if start_sequence == 0 {
                EMPTY_PREFIX_DIGEST
            } else {
                super::conversation_core::entry_on(&transaction, scope, start_sequence)
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?
                    .prefix_digest
            };
            for sequence in start_sequence.saturating_add(1)..=input.history_pin.head_revision {
                let entry = super::conversation_core::entry_on(&transaction, scope, sequence)
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let chained = advance_core_prefix_digest(
                    previous,
                    entry.reference,
                    &entry.message,
                    entry.kind,
                    entry.producer_run,
                    entry.contribution_id,
                    entry.producing_task.as_ref(),
                )
                .map_err(|_| AgentFailure::StorageUnavailable)?;
                if chained != entry.prefix_digest {
                    return Err(AgentFailure::StorageUnavailable);
                }
                previous = entry.prefix_digest;
                match entry.kind {
                    TranscriptEntryKind::Inbound => {
                        let Some(task_id) = entry.message.task_id else {
                            return Err(AgentFailure::StorageUnavailable);
                        };
                        let prior_input = expert_input_for_reference_on(
                            &transaction,
                            self.person_id,
                            input.conversation_id,
                            input.branch_id,
                            entry.reference,
                        )
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                        if prior_input.execution.task_id != task_id
                            || prior_input.key != input.key
                            || prior_input.identity != input.identity
                            || entry.message.text != prior_input.delegated_message
                            || entry.message.origin != MessageOrigin::Host
                            || entry
                                .message
                                .evidence
                                .as_ref()
                                .map(MessageEvidenceReference::digest)
                                != Some(prior_input.commitment()?)
                        {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        let (owner, _) = self
                            .verified_expert_task_owner_evidence_on(
                                &transaction,
                                prior_input.run_id,
                                &prior_input.identity,
                                prior_input.conversation_id,
                                prior_input.branch_id,
                                prior_input.input_reference.ok_or(AgentFailure::StorageUnavailable)?,
                                prior_input.execution.executor_generation,
                            )
                            .await?;
                        if owner.state != OwnerRunState::Terminal
                            || !owner.unresolved_effects.is_empty()
                        {
                            return Err(AgentFailure::Conflict);
                        }
                        history.push(ModelConversationEntry::User {
                            message_id: entry.message.message_id.as_uuid(),
                            text: entry.message.text,
                        });
                        history_coverage.push(prior_input.input_coverage);
                    }
                    TranscriptEntryKind::GeneratedOutput => {
                        let (evidence, coverage, task_id, generation, journal_revision, event_digest) =
                            expert_typed_entry_on(
                                &transaction,
                                self.person_id,
                                input.conversation_id,
                                input.branch_id,
                                sequence,
                            )
                            .await?
                            .ok_or(AgentFailure::StorageUnavailable)?;
                        if entry.producer_run.is_none()
                            || entry.producing_task.as_ref().is_none_or(|reference| reference.task_id() != task_id)
                        {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        let run_input = self
                            .expert_task_owner_for_run_on(&transaction, entry.producer_run.ok_or(AgentFailure::StorageUnavailable)?)
                            .await?;
                        if run_input.execution.task_id != task_id
                            || run_input.execution.executor_generation != generation
                            || run_input.key != input.key
                        {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        let producing_record = self
                            .task_on(&transaction, task_id)
                            .await?
                            .ok_or(AgentFailure::StorageUnavailable)?;
                        if producing_record.execution() != run_input.execution
                            || !terminal_task_state(producing_record.snapshot.state)
                        {
                            return Err(AgentFailure::Conflict);
                        }
                        let producing_journal = self
                            .task_journal_on(&transaction, &producing_record)
                            .await?;
                        let expected_reference = match &evidence {
                            ExpertConversationEvidence::ModelEntry { entry: typed, .. } => {
                                let (expected, expected_message) = expert_task_model_entry_at(
                                    run_input.run_id,
                                    &producing_journal,
                                    journal_revision,
                                )?;
                                let expected_coverage = match &expected {
                                    ModelConversationEntry::Assistant { .. }
                                        if producing_record.snapshot.state == TaskState::Completed =>
                                    {
                                        producing_record.snapshot.coverage.clone()
                                    }
                                    ModelConversationEntry::Assistant { .. } => {
                                        DependencyCoverage::Unknown
                                    }
                                    ModelConversationEntry::ToolExchange { result, .. } => {
                                        result.coverage.clone()
                                    }
                                    _ => return Err(AgentFailure::StorageUnavailable),
                                };
                                if typed != &expected
                                    || entry.message.text != expected_message
                                    || coverage != expected_coverage
                                {
                                    return Err(AgentFailure::StorageUnavailable);
                                }
                                let (reference, digest) = expert_task_evidence_reference_at(
                                    &producing_record,
                                    &producing_journal,
                                    journal_revision,
                                )?;
                                if digest != event_digest {
                                    return Err(AgentFailure::StorageUnavailable);
                                }
                                reference
                            }
                            ExpertConversationEvidence::Blocked {
                                blockage,
                                coverage: evidence_coverage,
                            } => {
                                let (reference, digest) =
                                    expert_task_terminal_reference(&producing_record)?;
                                if producing_record.snapshot.state != TaskState::Blocked
                                    || producing_record.snapshot.blockage.as_ref() != Some(blockage)
                                    || producing_record.snapshot.coverage != *evidence_coverage
                                    || coverage != *evidence_coverage
                                    || journal_revision != producing_record.journal_revision
                                    || digest != event_digest
                                    || entry.message.text != "This Expert Task is blocked."
                                {
                                    return Err(AgentFailure::StorageUnavailable);
                                }
                                reference
                            }
                        };
                        let expected_message_id = expert_task_message_id(
                            run_input.run_id,
                            match &evidence {
                                ExpertConversationEvidence::Blocked { .. } => producing_record
                                    .receipt
                                    .as_ref()
                                    .ok_or(AgentFailure::StorageUnavailable)?
                                    .reference
                                    .task_revision,
                                ExpertConversationEvidence::ModelEntry { .. } => journal_revision,
                            },
                            match &evidence {
                                ExpertConversationEvidence::Blocked { .. } => "blocked",
                                ExpertConversationEvidence::ModelEntry {
                                    entry: ModelConversationEntry::Assistant { .. },
                                    ..
                                } => "assistant",
                                ExpertConversationEvidence::ModelEntry { .. } => "tool",
                            },
                        );
                        let expected_command_id = expert_task_command_id(
                            run_input.run_id,
                            match &evidence {
                                ExpertConversationEvidence::Blocked { .. } => producing_record
                                    .receipt
                                    .as_ref()
                                    .ok_or(AgentFailure::StorageUnavailable)?
                                    .reference
                                    .task_revision,
                                ExpertConversationEvidence::ModelEntry { .. } => journal_revision,
                            },
                            match &evidence {
                                ExpertConversationEvidence::Blocked { .. } => "blocked",
                                ExpertConversationEvidence::ModelEntry {
                                    entry: ModelConversationEntry::Assistant { .. },
                                    ..
                                } => "assistant",
                                ExpertConversationEvidence::ModelEntry { .. } => "tool",
                            },
                        );
                        let expected_contribution_id = expert_task_contribution_id(
                            run_input.run_id,
                            match &evidence {
                                ExpertConversationEvidence::Blocked { .. } => producing_record
                                    .receipt
                                    .as_ref()
                                    .ok_or(AgentFailure::StorageUnavailable)?
                                    .reference
                                    .task_revision,
                                ExpertConversationEvidence::ModelEntry { .. } => journal_revision,
                            },
                            match &evidence {
                                ExpertConversationEvidence::Blocked { .. } => "blocked",
                                ExpertConversationEvidence::ModelEntry {
                                    entry: ModelConversationEntry::Assistant { .. },
                                    ..
                                } => "assistant",
                                ExpertConversationEvidence::ModelEntry { .. } => "tool",
                            },
                        );
                        let expected_origin = MessageOrigin::Agent {
                            agent_instance_id: floe_conversation_contract::AgentInstanceId::from_uuid(
                                run_input.key.installation_id,
                            )
                            .ok_or(AgentFailure::StorageUnavailable)?,
                        };
                        if entry.producing_task.as_ref() != Some(&expected_reference)
                            || entry.message.message_id.as_uuid() != expected_message_id
                            || entry.message.command_id.as_uuid() != expected_command_id
                            || entry.message.origin != expected_origin
                            || entry.message.task_id != Some(task_id)
                            || entry.contribution_id
                                != Some(
                                    floe_conversation_contract::LogicalContributionId::from_uuid(
                                        expected_contribution_id,
                                    )
                                    .ok_or(AgentFailure::StorageUnavailable)?,
                                )
                        {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        let (owner, _) = self
                            .verified_expert_task_owner_evidence_on(
                                &transaction,
                                run_input.run_id,
                                &run_input.identity,
                                run_input.conversation_id,
                                run_input.branch_id,
                                run_input.input_reference.ok_or(AgentFailure::StorageUnavailable)?,
                                generation,
                            )
                            .await?;
                        if owner.state != OwnerRunState::Terminal
                            || !owner.unresolved_effects.is_empty()
                        {
                            return Err(AgentFailure::Conflict);
                        }
                        match evidence {
                            ExpertConversationEvidence::ModelEntry { entry, .. } => {
                                history.push(entry);
                                history_coverage.push(coverage);
                            }
                            ExpertConversationEvidence::Blocked { .. } => {}
                        }
                    }
                }
            }
            if input.history_pin.head_revision > 0 && previous != input.history_pin.prefix_digest {
                return Err(AgentFailure::StorageUnavailable);
            }
            let conversation = ModelConversation {
                history,
                current_turn: vec![ModelConversationEntry::User {
                    message_id: input.input_message_id.as_uuid(),
                    text: input.delegated_message.clone(),
                }],
            };
            conversation.validate()?;
            if history_coverage.len() != conversation.history.len() {
                return Err(AgentFailure::StorageUnavailable);
            }
            self.check_access()?;
            Ok(ExpertTaskConversation {
                conversation,
                history_coverage,
            })
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn activate_task_executor(&self) -> Result<TaskActivation, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            validate_schema(&transaction).await?;
            let expert_conversations = crate::schema::expert_task_conversations_family_v1_present(
                &transaction,
            )
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?;
            if !expert_conversations {
                let mut rows = transaction
                    .query("SELECT count(*) FROM agent_tasks", ())
                    .await
                    .map_err(database_failure)?;
                let task_count = rows
                    .next()
                    .await
                    .map_err(database_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?
                    .get::<i64>(0)
                    .map_err(storage)?;
                if task_count != 0 {
                    return Err(AgentFailure::UnsupportedVersion);
                }
            }
            let current_generation = executor_generation(&transaction).await?;
            let next_generation = current_generation
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;

            // The immediate transaction fences concurrent old-executor writes;
            // advance the durable fence before deriving any interruption.
            let changed = transaction
                .execute(
                    "UPDATE agent_task_executor SET generation = ? WHERE id = 1 AND generation = ?",
                    (integer(next_generation)?, integer(current_generation)?),
                )
                .await
                .map_err(database_failure)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }

            let mut rows = transaction
                .query(
                    "SELECT task_id FROM agent_tasks WHERE state IN ('submitted', 'working') ORDER BY task_id LIMIT 4097",
                    (),
                )
                .await
                .map_err(database_failure)?;
            let mut task_ids = Vec::new();
            while let Some(row) = rows.next().await.map_err(database_failure)? {
                task_ids.push(parse_task_id(&row.get::<String>(0).map_err(storage)?)?);
            }
            drop(rows);
            if task_ids.len() > usize::try_from(MAX_TASK_ROWS).unwrap_or(usize::MAX) {
                return Err(AgentFailure::BudgetExceeded);
            }

            let mut interrupted = Vec::with_capacity(task_ids.len());
            for task_id in task_ids {
                let current = self
                    .task_on(&transaction, task_id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let journal = self.task_journal_on(&transaction, &current).await?;
                let was_submitted = current.snapshot.state == TaskState::Submitted;
                let next = interrupt_task_execution(
                    &current,
                    next_generation,
                    &journal,
                    MAX_OUTPUT_BYTES,
                )?
                .ok_or(AgentFailure::Conflict)?;
                if write_task(
                    &transaction,
                    &next,
                    current.aggregate_revision,
                    current.executor_generation,
                )
                .await?
                    != 1
                {
                    return Err(AgentFailure::Conflict);
                }
                if expert_conversations {
                    if was_submitted {
                        self.release_unstarted_expert_task_reservation_on(
                            &transaction,
                            &next,
                        )
                        .await?;
                    } else {
                        self.settle_expert_task_conversation_on(&transaction, &next, true)
                            .await?;
                    }
                }
                interrupted.push(next);
            }
            self.check_access()?;
            Ok(TaskActivation {
                executor_generation: next_generation,
                interrupted,
            })
        }
        .await;
        let activation = self
            .finish_registry_transaction_checked(transaction, result)
            .await?;
        self.task_executor_generation
            .store(activation.executor_generation, Ordering::Release);
        Ok(activation)
    }

    pub async fn admit_task(
        &self,
        proposed: TaskRecord,
        draft: ExpertTaskConversationDraft,
    ) -> Result<TaskAdmission, AgentFailure> {
        proposed.validate_initial(MAX_OUTPUT_BYTES)?;
        draft.validate()?;
        if proposed.snapshot.principal != self.person_id.to_string()
            || draft.key.person_id != self.person_id
            || !draft.key.matches_admission(&proposed.admission)
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let payload = encode(&proposed)?;
        let task_id = proposed.snapshot.task_id;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            if let Some(existing) = self.task_on(&transaction, task_id).await? {
                return if exact_admission(&existing, &proposed) {
                    let stored = task_conversation_input_on(
                        &transaction,
                        self.person_id,
                        existing.execution(),
                    )
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    if same_draft(&stored, &draft) {
                        let expert_input = load_expert_admission_reference_on(
                            &transaction,
                            self.person_id,
                            existing.execution(),
                        )
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                        if expert_input.request_digest != existing.request_digest {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        Ok(TaskAdmission::Existing {
                            record: existing,
                            expert_input,
                        })
                    } else {
                        Err(AgentFailure::Conflict)
                    }
                } else {
                    Err(AgentFailure::Conflict)
                };
            }
            if self.task_executor_generation.load(Ordering::Acquire) == 0 {
                return Err(AgentFailure::Conflict);
            }
            if proposed.executor_generation != self.active_executor_generation(&transaction).await? {
                return Err(AgentFailure::Conflict);
            }
            super::conversation_core::ensure_core_v3_on(&transaction)
                .await
                .map_err(super::conversation_core::owner_store_failure)?;
            ensure_expert_task_conversation_family_on(&transaction).await?;
            let mut count = transaction
                .query("SELECT count(*) FROM agent_tasks", ())
                .await
                .map_err(database_failure)?;
            let rows = count
                .next()
                .await
                .map_err(database_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?;
            if rows >= MAX_TASK_ROWS {
                return Err(AgentFailure::BudgetExceeded);
            }
            let binding_key = expert_binding_key(&draft.key)?;
            let identity = draft.key.core_identity()?;
            let (conversation_id, branch_id) = match load_expert_binding_on(
                &transaction,
                self.person_id,
                &binding_key,
            )
            .await?
            {
                Some((stored_key, stored_identity, conversation_id, branch_id)) => {
                    if stored_key != draft.key || stored_identity != identity {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    (conversation_id, branch_id)
                }
                None => {
                    let conversation_id = ConversationId::new();
                    let branch_id = ConversationBranchId::new();
                    transaction
                        .execute(
                            "INSERT INTO agent_expert_task_conversation_bindings_v1 (person_id, binding_key, key_json, identity_json, conversation_id, branch_id) VALUES (?, ?, ?, ?, ?, ?)",
                            (
                                self.person_id.to_string(),
                                binding_key.clone(),
                                serde_json::to_string(&draft.key).map_err(storage)?,
                                serde_json::to_string(&identity).map_err(storage)?,
                                conversation_id.as_uuid().to_string(),
                                branch_id.as_uuid().to_string(),
                            ),
                        )
                        .await
                        .map_err(map_constraint)?;
                    (conversation_id, branch_id)
                }
            };
            let history_pin = super::conversation_core::pin_expert_history_on(
                &transaction,
                &identity,
                conversation_id,
                branch_id,
            )
            .await?;
            let input = ExpertTaskConversationInput {
                execution: proposed.execution(),
                request_digest: proposed.request_digest,
                key: draft.key.clone(),
                identity,
                conversation_id,
                branch_id,
                run_id: draft.run_id,
                input_message_id: draft.input_message_id,
                input_command_id: draft.input_command_id,
                delegated_message: draft.delegated_message.clone(),
                input_coverage: draft.input_coverage.clone(),
                input_reference: None,
                history_pin,
            };
            input.validate()?;
            let input_commitment = input.commitment()?;
            let expert_input = input.issue_admission_reference()?;
            let input_payload = serde_json::to_string(&input).map_err(storage)?;
            if input_payload.len() > MAX_MODEL_CONVERSATION_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            transaction
                .execute(
                    "INSERT INTO agent_expert_task_conversation_runs_v1 (person_id, task_id, execution_id, executor_generation, binding_key, conversation_id, branch_id, run_id, input_json, input_commitment, input_sequence, input_message_id, input_reference_json, open_receipt_json, close_receipt_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, NULL, NULL)",
                    (
                        self.person_id.to_string(),
                        task_id.as_uuid().to_string(),
                        proposed.execution_id.to_string(),
                        integer(proposed.executor_generation)?,
                        binding_key.clone(),
                        conversation_id.as_uuid().to_string(),
                        branch_id.as_uuid().to_string(),
                        draft.run_id.as_uuid().to_string(),
                        input_payload,
                        commitment_hex(input_commitment),
                    ),
                )
                .await
                .map_err(map_constraint)?;
            transaction
                .execute(
                    "INSERT INTO agent_expert_task_conversation_admissions_v1 (person_id, task_id, execution_id, executor_generation, request_digest, input_commitment, reference_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (
                        self.person_id.to_string(),
                        task_id.as_uuid().to_string(),
                        proposed.execution_id.to_string(),
                        integer(proposed.executor_generation)?,
                        commitment_hex(proposed.request_digest),
                        commitment_hex(input_commitment),
                        serde_json::to_string(&expert_input).map_err(storage)?,
                    ),
                )
                .await
                .map_err(map_constraint)?;
            transaction
                .execute(
                    "INSERT INTO agent_expert_task_conversation_reservations_v1 (person_id, binding_key, task_id, execution_id, executor_generation) VALUES (?, ?, ?, ?, ?)",
                    (
                        self.person_id.to_string(),
                        binding_key,
                        task_id.as_uuid().to_string(),
                        proposed.execution_id.to_string(),
                        integer(proposed.executor_generation)?,
                    ),
                )
                .await
                .map_err(map_constraint)?;
            transaction
                .execute(
                    "INSERT INTO agent_tasks (task_id, invocation_key, person_id, state, aggregate_revision, executor_generation, payload) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (
                        task_id.as_uuid().to_string(),
                        proposed.invocation_key.as_uuid().to_string(),
                        self.person_id.to_string(),
                        state_name(proposed.snapshot.state),
                        integer(proposed.aggregate_revision)?,
                        integer(proposed.executor_generation)?,
                        payload,
                    ),
                )
                .await
                .map_err(|error| match error {
                    turso::Error::Constraint(_) => AgentFailure::Conflict,
                    other => storage(other),
                })?;
            self.check_access()?;
            Ok(TaskAdmission::Created {
                record: proposed,
                expert_input,
            })
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn compare_and_swap_task(
        &self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
        expert_input: ExpertTaskAdmissionReference,
    ) -> Result<TaskRecord, AgentFailure> {
        expert_input.validate()?;
        if snapshot.task_id != task_id || expert_input.execution.task_id != task_id {
            return Err(AgentFailure::Conflict);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            if executor_generation != self.active_executor_generation(&transaction).await? {
                return Err(AgentFailure::Conflict);
            }
            let current = self
                .task_on(&transaction, task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let next = current.transition(
                expected_aggregate_revision,
                executor_generation,
                snapshot,
                MAX_OUTPUT_BYTES,
            )?;
            if current.snapshot.state == TaskState::Submitted
                && next.snapshot.state == TaskState::Working
            {
                let stored_input =
                    task_conversation_input_on(&transaction, self.person_id, current.execution())
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                let stored_reference = load_expert_admission_reference_on(
                    &transaction,
                    self.person_id,
                    current.execution(),
                )
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
                if current.request_digest != stored_reference.request_digest
                    || stored_input.request_digest != current.request_digest
                    || stored_reference != expert_input
                {
                    return Err(AgentFailure::Conflict);
                }
                self.start_expert_task_run_on(&transaction, &next).await?;
            }
            if write_task(
                &transaction,
                &next,
                expected_aggregate_revision,
                current.executor_generation,
            )
            .await?
                != 1
            {
                return Err(AgentFailure::Conflict);
            }
            if current.snapshot.state == TaskState::Submitted
                && next.snapshot.state == TaskState::Working
            {
                self.open_expert_task_recorder_on(&transaction, &next)
                    .await?;
            }
            self.check_access()?;
            Ok(next)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    async fn start_expert_task_run_on(
        &self,
        transaction: &Transaction<'_>,
        record: &TaskRecord,
    ) -> Result<(), AgentFailure> {
        ensure_expert_task_conversation_family_on(transaction).await?;
        let mut input = task_conversation_input_on(transaction, self.person_id, record.execution())
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if input.input_reference.is_some() || record.snapshot.state != TaskState::Working {
            return Err(AgentFailure::Conflict);
        }
        let current_pin = super::conversation_core::pin_expert_history_on(
            transaction,
            &input.identity,
            input.conversation_id,
            input.branch_id,
        )
        .await?;
        if current_pin != input.history_pin {
            return Err(AgentFailure::Conflict);
        }
        let target = match input.history_pin.head_reference {
            Some(_) => floe_conversation_contract::AdmissionTarget::AppendToExisting {
                reference: floe_conversation_contract::ConversationReference {
                    conversation_id: input.conversation_id,
                    branch_id: input.branch_id,
                    identity: input.identity.clone(),
                    head_revision: input.history_pin.head_revision,
                },
            },
            None => floe_conversation_contract::AdmissionTarget::New {
                conversation_id: input.conversation_id,
                branch_id: input.branch_id,
                identity: input.identity.clone(),
            },
        };
        let admitted = self
            .append_conversation_input_on(
                transaction,
                floe_conversation_contract::MessageAdmissionRequest {
                    target,
                    message: ConversationMessage {
                        message_id: input.input_message_id,
                        command_id: input.input_command_id,
                        origin: MessageOrigin::Host,
                        text: input.delegated_message.clone(),
                        evidence: Some(MessageEvidenceReference::from_digest(input.commitment()?)),
                        task_id: Some(record.snapshot.task_id),
                    },
                },
            )
            .await
            .map_err(super::conversation_core::owner_store_failure)?;
        let reference = admitted.receipt.transcript;
        if reference.sequence != input.history_pin.head_revision.saturating_add(1)
            || reference.message_id != input.input_message_id
            || admitted.receipt.task_id != Some(record.snapshot.task_id)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        input.input_reference = Some(reference);
        input.validate()?;
        let payload = serde_json::to_string(&input).map_err(storage)?;
        let changed = transaction
            .execute(
                "UPDATE agent_expert_task_conversation_runs_v1 SET input_json = ?, input_sequence = ?, input_message_id = ?, input_reference_json = ? WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ? AND input_sequence IS NULL AND input_reference_json IS NULL",
                (
                    payload,
                    integer(reference.sequence)?,
                    reference.message_id.as_uuid().to_string(),
                    serde_json::to_string(&reference).map_err(storage)?,
                    self.person_id.to_string(),
                    record.snapshot.task_id.as_uuid().to_string(),
                    record.execution_id.to_string(),
                    integer(record.executor_generation)?,
                ),
            )
            .await
            .map_err(database_failure)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }

    async fn open_expert_task_recorder_on(
        &self,
        transaction: &Transaction<'_>,
        record: &TaskRecord,
    ) -> Result<(), AgentFailure> {
        let input = task_conversation_input_on(transaction, self.person_id, record.execution())
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if input.request_digest != record.request_digest {
            return Err(AgentFailure::StorageUnavailable);
        }
        let reference = input
            .input_reference
            .ok_or(AgentFailure::StorageUnavailable)?;
        let receipt = self
            .open_conversation_recorder_on(
                transaction,
                RecorderStartRequest {
                    identity: input.identity,
                    conversation_id: input.conversation_id,
                    branch_id: input.branch_id,
                    run_id: input.run_id,
                    input: reference,
                    executor_domain: ExecutorDomain::TaskExecution,
                    executor_generation: record.executor_generation,
                    execution_task: Some(record.snapshot.task_id),
                },
            )
            .await
            .map_err(super::conversation_core::owner_store_failure)?;
        let changed = transaction
            .execute(
                "UPDATE agent_expert_task_conversation_runs_v1 SET open_receipt_json = ? WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ? AND open_receipt_json IS NULL",
                (
                    serde_json::to_string(&receipt).map_err(storage)?,
                    self.person_id.to_string(),
                    record.snapshot.task_id.as_uuid().to_string(),
                    record.execution_id.to_string(),
                    integer(record.executor_generation)?,
                ),
            )
            .await
            .map_err(database_failure)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }

    async fn release_unstarted_expert_task_reservation_on(
        &self,
        transaction: &Transaction<'_>,
        record: &TaskRecord,
    ) -> Result<(), AgentFailure> {
        let input = task_conversation_input_on(transaction, self.person_id, record.execution())
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if input.request_digest != record.request_digest {
            return Err(AgentFailure::StorageUnavailable);
        }
        if input.input_reference.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        let mut rows = transaction
            .query(
                "SELECT open_receipt_json, close_receipt_json, retirement_receipt_json FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ?",
                (
                    self.person_id.to_string(),
                    record.snapshot.task_id.as_uuid().to_string(),
                    record.execution_id.to_string(),
                    integer(record.executor_generation)?,
                ),
            )
            .await
            .map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let open = row.get::<Option<String>>(0).map_err(storage)?;
        let close = row.get::<Option<String>>(1).map_err(storage)?;
        let retirement = row.get::<Option<String>>(2).map_err(storage)?;
        if rows.next().await.map_err(database_failure)?.is_some()
            || open.is_some()
            || close.is_some()
            || retirement.is_some()
            || record.snapshot.state != TaskState::Interrupted
            || record.journal_revision != 0
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(rows);
        self.release_expert_task_reservation_on(transaction, &input)
            .await
    }

    async fn release_expert_task_reservation_on(
        &self,
        transaction: &Transaction<'_>,
        input: &ExpertTaskConversationInput,
    ) -> Result<(), AgentFailure> {
        let changed = transaction
            .execute(
                "DELETE FROM agent_expert_task_conversation_reservations_v1 WHERE person_id = ? AND binding_key = ? AND task_id = ? AND execution_id = ? AND executor_generation = ?",
                (
                    self.person_id.to_string(),
                    expert_binding_key(&input.key)?,
                    input.execution.task_id.as_uuid().to_string(),
                    input.execution.execution_id.to_string(),
                    integer(input.execution.executor_generation)?,
                ),
            )
            .await
            .map_err(database_failure)?;
        if changed != 1 {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }

    async fn settle_expert_task_conversation_on(
        &self,
        transaction: &Transaction<'_>,
        record: &TaskRecord,
        stale_generation: bool,
    ) -> Result<(), AgentFailure> {
        if !terminal_task_state(record.snapshot.state) {
            return Err(AgentFailure::Conflict);
        }
        let input = task_conversation_input_on(transaction, self.person_id, record.execution())
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let input_reference = input
            .input_reference
            .ok_or(AgentFailure::StorageUnavailable)?;
        let open_json = load_expert_task_open_receipt_on(transaction, self.person_id, record)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let open: floe_conversation_core::RecorderOpenReceipt =
            serde_json::from_str(&open_json).map_err(storage)?;
        if open.fence.run_id != input.run_id || open.fence.input != input_reference {
            return Err(AgentFailure::StorageUnavailable);
        }
        let output_coverage = if record.snapshot.state == TaskState::Completed {
            record.snapshot.coverage.clone()
        } else {
            DependencyCoverage::Unknown
        };
        self.update_expert_task_output_coverage_on(
            transaction,
            record,
            input.conversation_id,
            input.branch_id,
            output_coverage,
        )
        .await?;

        let (owner, _) = self
            .verified_expert_task_owner_evidence_on(
                transaction,
                input.run_id,
                &input.identity,
                input.conversation_id,
                input.branch_id,
                input_reference,
                input.execution.executor_generation,
            )
            .await?;
        if owner.state != OwnerRunState::Terminal || !owner.unresolved_effects.is_empty() {
            // The receipt is terminal, but uncertainty still owns this exact
            // recorder and assignment reservation until later proof exists.
            return Ok(());
        }

        if record.snapshot.state == TaskState::Blocked {
            self.record_expert_task_blocked_entry_on(
                transaction,
                record,
                &input,
                open.fence.clone(),
            )
            .await?;
        }

        if stale_generation {
            let retirement = self
                .retire_conversation_recorder_on(transaction, open.fence.clone())
                .await
                .map_err(super::conversation_core::owner_store_failure)?;
            store_expert_task_retirement_on(transaction, self.person_id, record, &retirement)
                .await?;
        } else {
            let close = self
                .close_conversation_recorder_on(transaction, open.fence.clone())
                .await
                .map_err(super::conversation_core::owner_store_failure)?;
            store_expert_task_close_on(transaction, self.person_id, record, &close).await?;
        }
        self.release_expert_task_reservation_on(transaction, &input)
            .await
    }

    async fn update_expert_task_output_coverage_on(
        &self,
        transaction: &Transaction<'_>,
        record: &TaskRecord,
        conversation_id: ConversationId,
        branch_id: ConversationBranchId,
        coverage: DependencyCoverage,
    ) -> Result<(), AgentFailure> {
        coverage
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let mut rows = transaction
            .query(
                "SELECT sequence, evidence_json FROM agent_expert_task_conversation_entries_v1 WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ? ORDER BY sequence",
                (
                    self.person_id.to_string(),
                    conversation_id.as_uuid().to_string(),
                    branch_id.as_uuid().to_string(),
                    record.snapshot.task_id.as_uuid().to_string(),
                    record.execution_id.to_string(),
                    integer(record.executor_generation)?,
                ),
            )
            .await
            .map_err(database_failure)?;
        let mut updates = Vec::new();
        while let Some(row) = rows.next().await.map_err(database_failure)? {
            let sequence = nonnegative_integer(row.get::<i64>(0).map_err(storage)?)?;
            let payload = row.get::<String>(1).map_err(storage)?;
            let mut evidence: ExpertConversationEvidence =
                serde_json::from_str(&payload).map_err(storage)?;
            if let ExpertConversationEvidence::ModelEntry {
                entry: ModelConversationEntry::Assistant { .. },
                coverage: existing,
            } = &mut evidence
            {
                *existing = coverage.clone();
                let evidence_json = serde_json::to_string(&evidence).map_err(storage)?;
                if evidence_json.len() > MAX_MODEL_CONVERSATION_BYTES {
                    return Err(AgentFailure::BudgetExceeded);
                }
                updates.push((sequence, evidence_json));
            }
        }
        drop(rows);
        let coverage_json = serde_json::to_string(&coverage).map_err(storage)?;
        for (sequence, evidence_json) in updates {
            let changed = transaction
                .execute(
                    "UPDATE agent_expert_task_conversation_entries_v1 SET coverage_json = ?, evidence_json = ?, evidence_bytes = ? WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ? AND sequence = ?",
                    (
                        coverage_json.clone(),
                        evidence_json.clone(),
                        integer(evidence_json.len() as u64)?,
                        self.person_id.to_string(),
                        conversation_id.as_uuid().to_string(),
                        branch_id.as_uuid().to_string(),
                        record.snapshot.task_id.as_uuid().to_string(),
                        record.execution_id.to_string(),
                        integer(record.executor_generation)?,
                        integer(sequence)?,
                    ),
                )
                .await
                .map_err(database_failure)?;
            if changed != 1 {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
        Ok(())
    }

    async fn record_expert_task_blocked_entry_on(
        &self,
        transaction: &Transaction<'_>,
        record: &TaskRecord,
        input: &ExpertTaskConversationInput,
        recorder: floe_conversation_core::RecorderFence,
    ) -> Result<(), AgentFailure> {
        let blockage = record
            .snapshot
            .blockage
            .clone()
            .ok_or(AgentFailure::StorageUnavailable)?;
        let coverage = record.snapshot.coverage.clone();
        let (producing_task, event_digest) = expert_task_terminal_reference(record)?;
        let receipt = record
            .receipt
            .as_ref()
            .ok_or(AgentFailure::StorageUnavailable)?;
        let revision = receipt.reference.task_revision;
        let message_id =
            MessageId::from_uuid(expert_task_message_id(input.run_id, revision, "blocked"))
                .ok_or(AgentFailure::StorageUnavailable)?;
        let command_id =
            CommandId::from_uuid(expert_task_command_id(input.run_id, revision, "blocked"))
                .ok_or(AgentFailure::StorageUnavailable)?;
        let contribution_id = floe_conversation_contract::LogicalContributionId::from_uuid(
            expert_task_contribution_id(input.run_id, revision, "blocked"),
        )
        .ok_or(AgentFailure::StorageUnavailable)?;
        let evidence = ExpertConversationEvidence::Blocked { blockage, coverage };
        evidence.validate()?;
        let typed_json = serde_json::to_string(&evidence).map_err(storage)?;
        let message_text = "This Expert Task is blocked.".to_owned();
        let recording = self
            .record_terminal_conversation_entry_on(
                transaction,
                RecordingRequest {
                    recorder: recorder.clone(),
                    message: ConversationMessage {
                        message_id,
                        command_id,
                        origin: MessageOrigin::Agent {
                            agent_instance_id:
                                floe_conversation_contract::AgentInstanceId::from_uuid(
                                    input.key.installation_id,
                                )
                                .ok_or(AgentFailure::StorageUnavailable)?,
                        },
                        text: message_text,
                        evidence: None,
                        task_id: Some(record.snapshot.task_id),
                    },
                    contribution_id,
                    producing_task: Some(producing_task),
                },
            )
            .await
            .map_err(super::conversation_core::owner_store_failure)?;
        let coverage_json = serde_json::to_string(&record.snapshot.coverage).map_err(storage)?;
        let inserted = transaction
            .execute(
                "INSERT INTO agent_expert_task_conversation_entries_v1 (person_id, conversation_id, branch_id, sequence, task_id, execution_id, executor_generation, journal_revision, event_digest, coverage_json, evidence_json, evidence_bytes) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    self.person_id.to_string(),
                    input.conversation_id.as_uuid().to_string(),
                    input.branch_id.as_uuid().to_string(),
                    integer(recording.transcript.sequence)?,
                    record.snapshot.task_id.as_uuid().to_string(),
                    record.execution_id.to_string(),
                    integer(record.executor_generation)?,
                    integer(record.journal_revision)?,
                    hex_digest(event_digest),
                    coverage_json,
                    typed_json.clone(),
                    integer(typed_json.len() as u64)?,
                ),
            )
            .await
            .map_err(map_constraint)?;
        if inserted != 1 {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }

    pub async fn settle_task_execution(
        &self,
        commit: TaskExecutionCommit,
    ) -> Result<TaskExecutionReceipt, AgentFailure> {
        commit.execution.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let current = self
                .task_on(&transaction, commit.execution.task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.execution() != commit.execution {
                return Err(AgentFailure::Conflict);
            }
            let replay = terminal(current.snapshot.state);
            if !replay {
                let active = self.active_executor_generation(&transaction).await?;
                if commit.execution.executor_generation != active {
                    return Err(AgentFailure::Conflict);
                }
            }

            let journal = self.task_journal_on(&transaction, &current).await?;
            let next = settle_task_record_execution(&current, &commit, &journal, MAX_OUTPUT_BYTES)?;
            if replay {
                if next != current {
                    return Err(AgentFailure::Conflict);
                }
                return current.receipt.ok_or(AgentFailure::StorageUnavailable);
            }

            self.validate_context_dependency_coverage_in_transaction(
                &transaction,
                &commit.terminal.coverage,
            )
            .await?;

            if let Some(endpoint_settlement) = commit.settlement.as_ref() {
                let settlement = ExpertSettlement::from_endpoint_settlement(
                    endpoint_settlement,
                    &current.snapshot.agent_id,
                )?;
                let coverage = if settlement.dependencies.is_empty() {
                    DependencyCoverage::Independent
                } else {
                    DependencyCoverage::Dependent {
                        dependencies: settlement.dependencies.clone(),
                    }
                };
                coverage
                    .validate()
                    .map_err(|_| AgentFailure::PolicyDenied)?;
                if settlement.admission.assignment_id.is_nil()
                    || settlement.invocation_id.is_nil()
                    || settlement.owner() != commit.terminal.agent_id
                    || commit.terminal.task_id != commit.execution.task_id
                    || commit.terminal.principal != self.person_id.to_string()
                    || commit.terminal.state != TaskState::Completed
                    || commit.terminal.coverage != coverage
                    || commit.terminal.result.as_deref() != Some(settlement.task_result.as_str())
                    || settlement.next_private_state.schema_version != 1
                    || settlement.next_private_state.last_invocation_id
                        != Some(settlement.invocation_id)
                    || settlement.expected_private_state_revision.checked_add(1)
                        != Some(settlement.next_private_state.revision)
                    || settlement.next_private_state.completed_invocations
                        != settlement.next_private_state.revision
                {
                    return Err(AgentFailure::Conflict);
                }

                let mut registry = self
                    .registry_on(&transaction)
                    .await?
                    .ok_or(AgentFailure::NotFound)?;
                if registry.instance_id != settlement.admission.registry_instance_id {
                    return Err(AgentFailure::Conflict);
                }
                let assignment = registry
                    .assignments
                    .iter_mut()
                    .find(|assignment| {
                        assignment.id == settlement.admission.assignment_id
                            && assignment.person_id == self.person_id
                    })
                    .ok_or(AgentFailure::Conflict)?;
                if assignment.installation_id != settlement.admission.installation_id
                    || assignment.private_state.revision
                        != settlement.expected_private_state_revision
                    || assignment.private_state.completed_invocations
                        != settlement.expected_private_state_revision
                    || assignment.private_state.last_invocation_id == Some(settlement.invocation_id)
                    || !registry.installations.iter().any(|installation| {
                        installation.id == assignment.installation_id
                            && installation.package == settlement.admission.package
                    })
                    || settlement.admission.definition_revision
                        != commit.terminal.definition_revision
                    || current.admission != settlement.admission
                    || current.invocation_key.as_uuid() != settlement.invocation_id
                {
                    return Err(AgentFailure::Conflict);
                }
                assignment.private_state = settlement.next_private_state;
                let previous_revision = registry.revision;
                registry.revision = previous_revision
                    .checked_add(1)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                let payload = self.registry_payload(&registry)?;
                self.update_registry(&transaction, previous_revision, registry.revision, payload)
                    .await?;
            }

            if write_task(
                &transaction,
                &next,
                current.aggregate_revision,
                current.executor_generation,
            )
            .await?
                != 1
            {
                return Err(AgentFailure::Conflict);
            }
            self.settle_expert_task_conversation_on(&transaction, &next, false)
                .await?;
            self.check_access()?;
            next.receipt.ok_or(AgentFailure::StorageUnavailable)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn task(&self, task_id: TaskId) -> Result<Option<TaskRecord>, AgentFailure> {
        if !task_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = self.task_on(&transaction, task_id).await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn load_task_journal(
        &self,
        execution: TaskExecutionKey,
    ) -> Result<Vec<JournalEntry>, AgentFailure> {
        execution.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let record = self
                .task_on(&transaction, execution.task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if record.execution() != execution {
                return Err(AgentFailure::Conflict);
            }
            let entries = self.task_journal_on(&transaction, &record).await?;
            self.check_access()?;
            Ok(entries)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn read_task_execution_receipt(
        &self,
        reference: TaskExecutionReceiptRef,
    ) -> Result<TaskExecutionReceipt, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = self
            .read_execution_receipt_on(&transaction, &reference)
            .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn append_task_journal(
        &self,
        execution: TaskExecutionKey,
        phase: &str,
        event: JournalEvent,
    ) -> Result<u64, AgentFailure> {
        execution.validate()?;
        let kind = task_journal_kind(&event).ok_or(AgentFailure::CapabilityDenied)?;
        if phase != kind {
            return Err(AgentFailure::InvalidInput);
        }
        let payload =
            serde_json::to_string(&event).map_err(|_| AgentFailure::StorageUnavailable)?;
        if payload.is_empty() || payload.len() > MAX_TASK_JOURNAL_ENTRY_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let current = self
                .task_on(&transaction, execution.task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.execution() != execution {
                return Err(AgentFailure::Conflict);
            }
            let admission = load_expert_admission_reference_on(
                &transaction,
                self.person_id,
                execution,
            )
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
            if admission.request_digest != current.request_digest {
                return Err(AgentFailure::StorageUnavailable);
            }
            let mut entries = self.task_journal_on(&transaction, &current).await?;
            if let Some(existing) = existing_task_event(&entries, &event)? {
                self.verify_expert_task_journal_projection_on(
                    &transaction,
                    &current,
                    &entries,
                    existing,
                    &event,
                )
                .await?;
                self.check_access()?;
                return Ok(existing);
            }
            if current.snapshot.state != TaskState::Working
                || current.executor_generation
                    != self.active_executor_generation(&transaction).await?
            {
                return Err(AgentFailure::Conflict);
            }
            if entries.len() >= MAX_TASK_JOURNAL_ENTRIES {
                return Err(AgentFailure::BudgetExceeded);
            }
            let revision = (entries.len() as u64)
                .checked_add(1)
                .ok_or(AgentFailure::BudgetExceeded)?;
            entries.push(JournalEntry {
                revision,
                event,
            });
            let next = advance_task_journal(&current, &entries)?;
            transaction
                .execute(
                    "INSERT INTO agent_task_journal (task_id, execution_id, executor_generation, revision, kind, payload) VALUES (?, ?, ?, ?, ?, ?)",
                    (
                        execution.task_id.as_uuid().to_string(),
                        execution.execution_id.to_string(),
                        integer(execution.executor_generation)?,
                        integer(revision)?,
                        kind,
                        payload,
                    ),
                )
                .await
                .map_err(|error| match error {
                    turso::Error::Constraint(_) => AgentFailure::Conflict,
                    other => storage(other),
                })?;
            if write_task(&transaction, &next, current.aggregate_revision, current.executor_generation).await? != 1 {
                return Err(AgentFailure::Conflict);
            }
            self.record_expert_task_journal_event_on(
                &transaction,
                &next,
                &entries,
                &entries.last().ok_or(AgentFailure::StorageUnavailable)?.event,
            )
            .await?;
            self.check_access()?;
            Ok(revision)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    async fn verify_expert_task_journal_projection_on(
        &self,
        transaction: &Transaction<'_>,
        record: &TaskRecord,
        journal: &[JournalEntry],
        revision: u64,
        event: &JournalEvent,
    ) -> Result<(), AgentFailure> {
        if !matches!(
            event,
            JournalEvent::Output { .. } | JournalEvent::ToolResult { .. }
        ) {
            return Ok(());
        }
        let input = task_conversation_input_on(transaction, self.person_id, record.execution())
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let input_reference = input
            .input_reference
            .ok_or(AgentFailure::StorageUnavailable)?;
        let (expected_entry, message_text) =
            expert_task_model_entry_at(input.run_id, journal, revision)?;
        let expected_coverage = match (&expected_entry, record.snapshot.state) {
            (ModelConversationEntry::Assistant { .. }, TaskState::Working) => {
                expert_journal_coverage(&journal[..usize::try_from(revision).map_err(storage)?])?
            }
            (ModelConversationEntry::Assistant { .. }, TaskState::Completed) => {
                record.snapshot.coverage.clone()
            }
            (ModelConversationEntry::Assistant { .. }, _) => DependencyCoverage::Unknown,
            (ModelConversationEntry::ToolExchange { result, .. }, _) => result.coverage.clone(),
            _ => return Err(AgentFailure::StorageUnavailable),
        };
        let (expected_reference, event_digest) =
            expert_task_evidence_reference_at(record, journal, revision)?;
        let kind = if matches!(expected_entry, ModelConversationEntry::Assistant { .. }) {
            "assistant"
        } else {
            "tool"
        };
        let expected_message_id =
            MessageId::from_uuid(expert_task_message_id(input.run_id, revision, kind))
                .ok_or(AgentFailure::StorageUnavailable)?;
        let expected_command_id =
            CommandId::from_uuid(expert_task_command_id(input.run_id, revision, kind))
                .ok_or(AgentFailure::StorageUnavailable)?;
        let expected_contribution = floe_conversation_contract::LogicalContributionId::from_uuid(
            expert_task_contribution_id(input.run_id, revision, kind),
        )
        .ok_or(AgentFailure::StorageUnavailable)?;
        let mut rows = transaction
            .query(
                "SELECT sequence FROM agent_expert_task_conversation_entries_v1 WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ? AND journal_revision = ?",
                (
                    self.person_id.to_string(),
                    record.snapshot.task_id.as_uuid().to_string(),
                    record.execution_id.to_string(),
                    integer(record.executor_generation)?,
                    integer(revision)?,
                ),
            )
            .await
            .map_err(database_failure)?;
        let sequence = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?
            .get::<i64>(0)
            .map_err(storage)?;
        if sequence <= 0 || rows.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(rows);
        let sequence = nonnegative_integer(sequence)?;
        let Some((typed, typed_coverage, task_id, generation, typed_revision, typed_digest)) =
            expert_typed_entry_on(
                transaction,
                self.person_id,
                input.conversation_id,
                input.branch_id,
                sequence,
            )
            .await?
        else {
            return Err(AgentFailure::StorageUnavailable);
        };
        let expected_typed = ExpertConversationEvidence::ModelEntry {
            entry: expected_entry,
            coverage: expected_coverage.clone(),
        };
        if typed != expected_typed
            || typed_coverage != expected_coverage
            || task_id != record.snapshot.task_id
            || generation != record.executor_generation
            || typed_revision != revision
            || typed_digest != event_digest
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let scope = super::conversation_core::Scope::from_identity(
            self.person_id,
            &input.identity,
            input.conversation_id,
            input.branch_id,
        );
        let core_entry = super::conversation_core::entry_on(transaction, scope, sequence)
            .await
            .map_err(super::conversation_core::owner_store_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let expected_origin = MessageOrigin::Agent {
            agent_instance_id: floe_conversation_contract::AgentInstanceId::from_uuid(
                input.key.installation_id,
            )
            .ok_or(AgentFailure::StorageUnavailable)?,
        };
        if core_entry.kind != TranscriptEntryKind::GeneratedOutput
            || core_entry.producer_run != Some(input.run_id)
            || core_entry.reference.sequence <= input_reference.sequence
            || core_entry.reference.message_id != expected_message_id
            || core_entry.message.command_id != expected_command_id
            || core_entry.message.origin != expected_origin
            || core_entry.message.task_id != Some(record.snapshot.task_id)
            || core_entry.message.text != message_text
            || core_entry.producing_task.as_ref() != Some(&expected_reference)
            || core_entry.contribution_id != Some(expected_contribution)
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }

    async fn record_expert_task_journal_event_on(
        &self,
        transaction: &Transaction<'_>,
        record: &TaskRecord,
        journal: &[JournalEntry],
        event: &JournalEvent,
    ) -> Result<(), AgentFailure> {
        let (typed_entry, coverage, message_text, kind) = match event {
            JournalEvent::Output { text, .. } => (
                ModelConversationEntry::Assistant {
                    message_id: Uuid::nil(),
                    text: text.clone(),
                },
                expert_journal_coverage(journal)?,
                text.clone(),
                "assistant",
            ),
            JournalEvent::ToolResult { result } => {
                let mut calls = journal[..journal.len().saturating_sub(1)]
                    .iter()
                    .filter_map(|entry| match &entry.event {
                        JournalEvent::ToolIntent { call } if call.call_id == result.call_id => {
                            Some(call.clone())
                        }
                        _ => None,
                    });
                let call = calls.next().ok_or(AgentFailure::StorageUnavailable)?;
                if calls.next().is_some() {
                    return Err(AgentFailure::StorageUnavailable);
                }
                (
                    ModelConversationEntry::ToolExchange {
                        call,
                        result: result.clone(),
                    },
                    result.coverage.clone(),
                    result.text.clone(),
                    "tool",
                )
            }
            _ => return Ok(()),
        };
        if message_text.len() > floe_conversation_contract::MAX_CONVERSATION_MESSAGE_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        let input = task_conversation_input_on(transaction, self.person_id, record.execution())
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let input_reference = input
            .input_reference
            .ok_or(AgentFailure::StorageUnavailable)?;
        let mut open_rows = transaction
            .query(
                "SELECT open_receipt_json FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND task_id = ? AND execution_id = ? AND executor_generation = ?",
                (
                    self.person_id.to_string(),
                    record.snapshot.task_id.as_uuid().to_string(),
                    record.execution_id.to_string(),
                    integer(record.executor_generation)?,
                ),
            )
            .await
            .map_err(database_failure)?;
        let open_json = open_rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?
            .get::<Option<String>>(0)
            .map_err(storage)?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if open_rows.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(open_rows);
        let open: floe_conversation_core::RecorderOpenReceipt =
            serde_json::from_str(&open_json).map_err(storage)?;
        if open.fence.run_id != input.run_id || open.fence.input != input_reference {
            return Err(AgentFailure::StorageUnavailable);
        }
        let revision = journal
            .last()
            .map(|entry| entry.revision)
            .ok_or(AgentFailure::StorageUnavailable)?;
        let (reference, event_digest) =
            expert_task_evidence_reference_at(record, journal, revision)?;
        let message_id = MessageId::from_uuid(expert_task_message_id(input.run_id, revision, kind))
            .ok_or(AgentFailure::StorageUnavailable)?;
        let command_id = CommandId::from_uuid(expert_task_command_id(input.run_id, revision, kind))
            .ok_or(AgentFailure::StorageUnavailable)?;
        let contribution_id = floe_conversation_contract::LogicalContributionId::from_uuid(
            expert_task_contribution_id(input.run_id, revision, kind),
        )
        .ok_or(AgentFailure::StorageUnavailable)?;
        let typed_entry = match typed_entry {
            ModelConversationEntry::Assistant { text, .. } => ModelConversationEntry::Assistant {
                message_id: message_id.as_uuid(),
                text,
            },
            entry => entry,
        };
        let evidence = ExpertConversationEvidence::ModelEntry {
            entry: typed_entry,
            coverage: coverage.clone(),
        };
        evidence.validate()?;
        let typed_json = serde_json::to_string(&evidence).map_err(storage)?;
        if typed_json.len() > MAX_MODEL_CONVERSATION_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        let recording = self
            .record_conversation_entry_on(
                transaction,
                RecordingRequest {
                    recorder: open.fence,
                    message: ConversationMessage {
                        message_id,
                        command_id,
                        origin: MessageOrigin::Agent {
                            agent_instance_id:
                                floe_conversation_contract::AgentInstanceId::from_uuid(
                                    input.key.installation_id,
                                )
                                .ok_or(AgentFailure::StorageUnavailable)?,
                        },
                        text: message_text,
                        evidence: None,
                        task_id: Some(record.snapshot.task_id),
                    },
                    contribution_id,
                    producing_task: Some(reference),
                },
            )
            .await
            .map_err(super::conversation_core::owner_store_failure)?;
        let coverage_json = serde_json::to_string(&coverage).map_err(storage)?;
        let inserted = transaction
            .execute(
                "INSERT INTO agent_expert_task_conversation_entries_v1 (person_id, conversation_id, branch_id, sequence, task_id, execution_id, executor_generation, journal_revision, event_digest, coverage_json, evidence_json, evidence_bytes) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    self.person_id.to_string(),
                    input.conversation_id.as_uuid().to_string(),
                    input.branch_id.as_uuid().to_string(),
                    integer(recording.transcript.sequence)?,
                    record.snapshot.task_id.as_uuid().to_string(),
                    record.execution_id.to_string(),
                    integer(record.executor_generation)?,
                    integer(revision)?,
                    hex_digest(event_digest),
                    coverage_json,
                    typed_json.clone(),
                    integer(typed_json.len() as u64)?,
                ),
            )
            .await
            .map_err(map_constraint)?;
        if inserted != 1 {
            return Err(AgentFailure::StorageUnavailable);
        }
        self.check_access()?;
        Ok(())
    }

    pub(super) async fn task_on(
        &self,
        connection: &turso::Connection,
        task_id: TaskId,
    ) -> Result<Option<TaskRecord>, AgentFailure> {
        let mut rows = connection
            .query(
                "SELECT invocation_key, person_id, state, aggregate_revision, executor_generation, payload FROM agent_tasks WHERE task_id = ?",
                [task_id.as_uuid().to_string()],
            )
            .await
            .map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
            drop(rows);
            let mut orphan = connection
                .query(
                    "SELECT 1 FROM agent_task_journal WHERE task_id = ? LIMIT 1",
                    [task_id.as_uuid().to_string()],
                )
                .await
                .map_err(database_failure)?;
            if orphan.next().await.map_err(database_failure)?.is_some() {
                return Err(AgentFailure::StorageUnavailable);
            }
            return Ok(None);
        };
        let payload = row.get::<String>(5).map_err(storage)?;
        if payload.is_empty() || payload.len() > MAX_TASK_RECORD_BYTES {
            return Err(AgentFailure::StorageUnavailable);
        }
        let record: TaskRecord = serde_json::from_str(&payload).map_err(unavailable)?;
        record.validate(MAX_OUTPUT_BYTES).map_err(unavailable)?;
        if record.snapshot.task_id != task_id
            || record.snapshot.principal != self.person_id.to_string()
            || row.get::<String>(0).map_err(storage)? != record.invocation_key.as_uuid().to_string()
            || row.get::<String>(1).map_err(storage)? != self.person_id.to_string()
            || row.get::<String>(2).map_err(storage)? != state_name(record.snapshot.state)
            || row.get::<i64>(3).map_err(storage)? != integer(record.aggregate_revision)?
            || row.get::<i64>(4).map_err(storage)? != integer(record.executor_generation)?
            || rows.next().await.map_err(database_failure)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(rows);
        self.task_journal_on(connection, &record).await?;
        Ok(Some(record))
    }

    pub(super) async fn read_execution_receipt_on(
        &self,
        transaction: &Transaction<'_>,
        reference: &TaskExecutionReceiptRef,
    ) -> Result<TaskExecutionReceipt, AgentFailure> {
        self.check_access()?;
        reference.validate()?;
        let record = self
            .task_on(transaction, reference.execution.task_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if record.snapshot.principal != self.person_id.to_string()
            || record.execution() != reference.execution
        {
            return Err(AgentFailure::Conflict);
        }
        let input = task_conversation_input_on(transaction, self.person_id, record.execution())
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let admission =
            load_expert_admission_reference_on(transaction, self.person_id, record.execution())
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
        if input.request_digest != record.request_digest
            || admission.request_digest != record.request_digest
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let stored = record.receipt.as_ref().ok_or(AgentFailure::Conflict)?;
        if stored.reference != *reference {
            return Err(AgentFailure::Conflict);
        }
        let journal = self.task_journal_on(transaction, &record).await?;
        let expected_task_revision = record
            .aggregate_revision
            .checked_sub(1)
            .ok_or(AgentFailure::StorageUnavailable)?;
        let identical_replay = TaskExecutionCommit {
            execution: reference.execution,
            expected_task_revision,
            expected_journal_revision: reference.journal_revision,
            terminal: record.snapshot.clone(),
            settlement: None,
        };
        let verified =
            settle_task_record_execution(&record, &identical_replay, &journal, MAX_OUTPUT_BYTES)?;
        if verified != record {
            return Err(AgentFailure::StorageUnavailable);
        }
        self.check_access()?;
        Ok(stored.clone())
    }

    async fn task_journal_on(
        &self,
        connection: &turso::Connection,
        record: &TaskRecord,
    ) -> Result<Vec<JournalEntry>, AgentFailure> {
        let mut rows = connection
            .query(
                "SELECT task_id, execution_id, executor_generation, revision, kind, payload FROM agent_task_journal WHERE task_id = ? ORDER BY revision LIMIT 513",
                [record.snapshot.task_id.as_uuid().to_string()],
            )
            .await
            .map_err(database_failure)?;
        let execution = record.execution();
        let mut entries = Vec::new();
        while let Some(row) = rows.next().await.map_err(database_failure)? {
            let task_id = row.get::<String>(0).map_err(storage)?;
            let execution_id = row.get::<String>(1).map_err(storage)?;
            let executor_generation = row.get::<i64>(2).map_err(storage)?;
            let revision = u64::try_from(row.get::<i64>(3).map_err(storage)?)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            let kind = row.get::<String>(4).map_err(storage)?;
            let payload = row.get::<String>(5).map_err(storage)?;
            if task_id != execution.task_id.as_uuid().to_string()
                || execution_id != execution.execution_id.to_string()
                || executor_generation != integer(execution.executor_generation)?
                || revision != entries.len() as u64 + 1
                || kind.is_empty()
                || kind.len() > 64
                || payload.is_empty()
                || payload.len() > MAX_TASK_JOURNAL_ENTRY_BYTES
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            let event: JournalEvent =
                serde_json::from_str(&payload).map_err(|_| AgentFailure::StorageUnavailable)?;
            if task_journal_kind(&event) != Some(kind.as_str()) {
                return Err(AgentFailure::StorageUnavailable);
            }
            entries.push(JournalEntry { revision, event });
        }
        if entries.len() > MAX_TASK_JOURNAL_ENTRIES {
            return Err(AgentFailure::StorageUnavailable);
        }
        validate_task_journal(record, &entries)?;
        validate_terminal_journal(record, &entries)?;
        Ok(entries)
    }

    pub(super) async fn active_executor_generation(
        &self,
        connection: &turso::Connection,
    ) -> Result<u64, AgentFailure> {
        let active = self.task_executor_generation.load(Ordering::Acquire);
        if active == 0 || active != executor_generation_on(connection).await? {
            return Err(AgentFailure::Conflict);
        }
        Ok(active)
    }

    pub(super) async fn verified_expert_task_owner_evidence_on(
        &self,
        transaction: &Transaction<'_>,
        run_id: RunId,
        identity: &AgentIdentity,
        conversation_id: ConversationId,
        branch_id: ConversationBranchId,
        input_reference: TranscriptReference,
        executor_generation: u64,
    ) -> Result<(OwnerRunEvidence, Option<[u8; 32]>), AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT task_id, execution_id, executor_generation, open_receipt_json FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND run_id = ?",
                (self.person_id.to_string(), run_id.as_uuid().to_string()),
            )
            .await
            .map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
            return Err(AgentFailure::Conflict);
        };
        let task_id = TaskId::from_uuid(
            Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(storage)?,
        )
        .ok_or(AgentFailure::StorageUnavailable)?;
        let execution_id =
            Uuid::parse_str(&row.get::<String>(1).map_err(storage)?).map_err(storage)?;
        let generation = nonnegative_integer(row.get::<i64>(2).map_err(storage)?)?;
        let open_json = row.get::<Option<String>>(3).map_err(storage)?;
        if rows.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(rows);
        let execution = TaskExecutionKey {
            task_id,
            execution_id,
            executor_generation: generation,
        };
        let input = task_conversation_input_on(transaction, self.person_id, execution)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if input.run_id != run_id
            || input.identity != *identity
            || input.conversation_id != conversation_id
            || input.branch_id != branch_id
            || input.input_reference != Some(input_reference)
            || input.execution.executor_generation != executor_generation
        {
            return Err(AgentFailure::Conflict);
        }
        if let Some(open_json) = open_json {
            let open: floe_conversation_core::RecorderOpenReceipt =
                serde_json::from_str(&open_json).map_err(storage)?;
            if open.fence.run_id != run_id
                || open.fence.input != input_reference
                || open.fence.identity != *identity
                || open.fence.executor_generation != executor_generation
                || open.fence.executor_domain != ExecutorDomain::TaskExecution
                || serde_json::to_string(&open).map_err(storage)? != open_json
            {
                return Err(AgentFailure::StorageUnavailable);
            }
        }
        let record = self
            .task_on(transaction, task_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if record.execution() != execution {
            return Err(AgentFailure::Conflict);
        }
        let owner_reference =
            load_expert_admission_reference_on(transaction, self.person_id, execution)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
        if record.request_digest != owner_reference.request_digest
            || input.request_digest != owner_reference.request_digest
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let journal = self.task_journal_on(transaction, &record).await?;
        validate_task_journal(&record, &journal)?;
        let mut unresolved = std::collections::BTreeSet::new();
        let mut unresolved_models = std::collections::BTreeSet::new();
        let mut tools = std::collections::HashSet::new();
        let mut models = std::collections::HashSet::new();
        let mut delegations = std::collections::HashSet::new();
        for entry in &journal {
            match &entry.event {
                JournalEvent::ModelIntent { attempt_id, .. } => {
                    models.insert(*attempt_id);
                }
                JournalEvent::ModelResult { attempt_id, .. } => {
                    models.remove(attempt_id);
                }
                JournalEvent::ToolIntent { call } => {
                    tools.insert(call.call_id);
                }
                JournalEvent::ToolResult { result } => {
                    tools.remove(&result.call_id);
                }
                JournalEvent::DelegationIntent { request } => {
                    delegations.insert(request.task_id.as_uuid());
                }
                JournalEvent::DelegationResult { receipt } => {
                    delegations.remove(&receipt.task_id.as_uuid());
                }
                _ => {}
            }
        }
        unresolved.extend(tools);
        unresolved.extend(delegations);
        unresolved.extend(models.iter().copied());
        unresolved_models.extend(models);
        if let Some(receipt) = record.receipt.as_ref() {
            for attempt in &receipt.accounting.unresolved_attempts {
                unresolved.insert(attempt.attempt_id);
                unresolved_models.insert(attempt.attempt_id);
            }
        }
        let unresolved_effects = unresolved.into_iter().collect::<Vec<_>>();
        let unresolved_model_attempts = unresolved_models.into_iter().collect::<Vec<_>>();
        let state = if record.snapshot.state == TaskState::Working {
            OwnerRunState::Working
        } else if terminal_task_state(record.snapshot.state) {
            if unresolved_effects.is_empty() {
                OwnerRunState::Terminal
            } else {
                OwnerRunState::PendingTerminal
            }
        } else {
            return Err(AgentFailure::Conflict);
        };
        let encoded = serde_json::to_vec(&(
            "floe-expert-task-owner-record-v1",
            &record,
            &input,
            &journal,
        ))
        .map_err(storage)?;
        let record_digest: [u8; 32] = Sha256::digest(encoded).into();
        let owner = OwnerRunEvidence {
            domain: ExecutorDomain::TaskExecution,
            run_id,
            person_id: self.person_id,
            input: input_reference,
            executor_generation,
            aggregate_revision: record.aggregate_revision,
            journal_revision: record.journal_revision,
            state,
            record_digest,
            unresolved_effects,
            unresolved_model_attempts,
        };
        owner
            .validate()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let terminal_digest = record
            .receipt
            .as_ref()
            .map(|receipt| receipt.reference.digest);
        Ok((owner, terminal_digest))
    }

    pub(super) async fn verified_expert_task_reference_on(
        &self,
        transaction: &Transaction<'_>,
        run_id: RunId,
        reference: &TaskEvidenceReference,
    ) -> Result<TaskEvidenceReference, AgentFailure> {
        reference.validate().map_err(|_| AgentFailure::Conflict)?;
        let owner = self
            .expert_task_owner_for_run_on(transaction, run_id)
            .await?;
        let record = self
            .task_on(transaction, owner.execution.task_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let journal = self.task_journal_on(transaction, &record).await?;
        // Transcript custody pins the exact Task journal event that produced
        // an entry. Later Task events and final settlement must not retarget
        // that reference, so verify it against every admitted journal revision.
        // The stored journal is bounded by MAX_TASK_JOURNAL_ENTRIES.
        for entry in &journal {
            let (expected, _) =
                expert_task_evidence_reference_at(&record, &journal, entry.revision)?;
            if reference == &expected {
                return Ok(expected);
            }
        }
        if terminal_task_state(record.snapshot.state) {
            let (expected, _) = expert_task_terminal_reference(&record)?;
            if reference == &expected {
                return Ok(expected);
            }
        }
        Err(AgentFailure::Conflict)
    }

    pub(super) async fn expert_task_owner_for_run_on(
        &self,
        transaction: &Transaction<'_>,
        run_id: RunId,
    ) -> Result<ExpertTaskConversationInput, AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT task_id, execution_id, executor_generation FROM agent_expert_task_conversation_runs_v1 WHERE person_id = ? AND run_id = ?",
                (self.person_id.to_string(), run_id.as_uuid().to_string()),
            )
            .await
            .map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::Conflict)?;
        let task_id = TaskId::from_uuid(
            Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(storage)?,
        )
        .ok_or(AgentFailure::StorageUnavailable)?;
        let execution_id =
            Uuid::parse_str(&row.get::<String>(1).map_err(storage)?).map_err(storage)?;
        let executor_generation = nonnegative_integer(row.get::<i64>(2).map_err(storage)?)?;
        if rows.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        task_conversation_input_on(
            transaction,
            self.person_id,
            TaskExecutionKey {
                task_id,
                execution_id,
                executor_generation,
            },
        )
        .await?
        .ok_or(AgentFailure::StorageUnavailable)
    }
}

pub(super) async fn validate_schema(transaction: &turso::Connection) -> Result<(), AgentFailure> {
    crate::schema::inspect_family(transaction, crate::schema::Family::Tasks)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)?;
    executor_generation(transaction).await?;
    Ok(())
}

pub(super) async fn executor_generation(
    connection: &turso::Connection,
) -> Result<u64, AgentFailure> {
    let generation = executor_generation_on(connection).await?;
    if generation > i64::MAX as u64 {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(generation)
}

async fn executor_generation_on(connection: &turso::Connection) -> Result<u64, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT generation FROM agent_task_executor WHERE id = 1",
            (),
        )
        .await
        .map_err(database_failure)?;
    let value = rows
        .next()
        .await
        .map_err(database_failure)?
        .ok_or(AgentFailure::VaultUnavailable)?
        .get::<i64>(0)
        .map_err(storage)?;
    if value < 0 || rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(value as u64)
}

async fn write_task(
    transaction: &Transaction<'_>,
    next: &TaskRecord,
    expected_revision: u64,
    expected_generation: u64,
) -> Result<u64, AgentFailure> {
    let payload = encode(next)?;
    transaction
        .execute(
            "UPDATE agent_tasks SET state = ?, aggregate_revision = ?, executor_generation = ?, payload = ? WHERE task_id = ? AND aggregate_revision = ? AND executor_generation = ?",
            (
                state_name(next.snapshot.state),
                integer(next.aggregate_revision)?,
                integer(next.executor_generation)?,
                payload,
                next.snapshot.task_id.as_uuid().to_string(),
                integer(expected_revision)?,
                integer(expected_generation)?,
            ),
        )
        .await
        .map_err(database_failure)
}

fn encode(record: &TaskRecord) -> Result<String, AgentFailure> {
    record.validate(MAX_OUTPUT_BYTES)?;
    let payload = serde_json::to_string(record).map_err(storage)?;
    if payload.len() > MAX_TASK_RECORD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}

fn exact_admission(existing: &TaskRecord, proposed: &TaskRecord) -> bool {
    existing.snapshot.task_id == proposed.snapshot.task_id
        && existing.snapshot.parent_run_id == proposed.snapshot.parent_run_id
        && existing.snapshot.principal == proposed.snapshot.principal
        && existing.snapshot.agent_id == proposed.snapshot.agent_id
        && existing.snapshot.definition_revision == proposed.snapshot.definition_revision
        && existing.admission == proposed.admission
        && existing.selection == proposed.selection
        && existing.invocation_key == proposed.invocation_key
        && existing.request_digest == proposed.request_digest
        && existing.execution_id == proposed.execution_id
        && existing.device_id == proposed.device_id
        && existing.catalog_revision == proposed.catalog_revision
        && existing.model_allowance == proposed.model_allowance
        && existing.maximum_output_bytes == proposed.maximum_output_bytes
}

fn validate_terminal_journal(
    record: &TaskRecord,
    entries: &[JournalEntry],
) -> Result<(), AgentFailure> {
    if !terminal(record.snapshot.state) {
        return Ok(());
    }
    let receipt = record
        .receipt
        .as_ref()
        .ok_or(AgentFailure::StorageUnavailable)?;
    let expected_task_revision = record
        .aggregate_revision
        .checked_sub(1)
        .ok_or(AgentFailure::StorageUnavailable)?;
    let replay = TaskExecutionCommit {
        execution: record.execution(),
        expected_task_revision,
        expected_journal_revision: receipt.reference.journal_revision,
        terminal: record.snapshot.clone(),
        settlement: None,
    };
    let verified = settle_task_record_execution(record, &replay, entries, MAX_OUTPUT_BYTES)
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    if verified != *record {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(())
}

fn task_journal_kind(event: &JournalEvent) -> Option<&'static str> {
    match event {
        JournalEvent::ModelIntent { .. } | JournalEvent::ToolIntent { .. } => Some("intent"),
        JournalEvent::ModelResult { .. }
        | JournalEvent::ToolResult { .. }
        | JournalEvent::ToolReviewRequired { .. } => Some("result"),
        JournalEvent::Output { .. } => Some("output"),
        JournalEvent::Checkpoint { .. }
        | JournalEvent::ValidatedBatch { .. }
        | JournalEvent::BatchProgress { .. } => Some("checkpoint"),
        JournalEvent::DelegationIntent { .. }
        | JournalEvent::DelegationResult { .. }
        | JournalEvent::FinalizationStarted { .. } => None,
    }
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::InvalidInput)
}

fn parse_task_id(value: &str) -> Result<TaskId, AgentFailure> {
    TaskId::from_uuid(uuid::Uuid::parse_str(value).map_err(unavailable)?)
        .ok_or(AgentFailure::StorageUnavailable)
}

fn terminal(state: TaskState) -> bool {
    !matches!(state, TaskState::Submitted | TaskState::Working)
}

fn state_name(state: TaskState) -> &'static str {
    match state {
        TaskState::Submitted => "submitted",
        TaskState::Working => "working",
        TaskState::Completed => "completed",
        TaskState::Blocked => "blocked",
        TaskState::Failed => "failed",
        TaskState::Rejected => "rejected",
        TaskState::Cancelled => "cancelled",
        TaskState::TimedOut => "timed_out",
        TaskState::Interrupted => "interrupted",
    }
}
