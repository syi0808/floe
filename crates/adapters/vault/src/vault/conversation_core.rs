//! Encrypted storage adapter for the role-neutral Conversation Core port.
//!
//! Transcript entries, receipts, FIFO scheduling, writer fences and checkpoint
//! commitments are separate bounded rows. Admission and writer/checkpoint state
//! changes run in short immediate transactions; dispatch remains owned by the
//! caller and may follow only a returned durable admission receipt.

use super::{EncryptedAgentVault, VaultKeyProvider, database_failure};
use crate::write_fence::JournalWriteGuard;
use floe_conversation_contract::{
    AdmissionResult, AgentIdentity, ConversationBranchId, ConversationCheckpoint,
    ConversationFailure, ConversationId, ConversationMessage, ConversationReference,
    MessageAdmissionRequest, MessageId, RunTaskLink, TranscriptReference,
};
use floe_conversation_core::{
    AdmissionFacts, CheckpointFacts, ClaimFacts, CompletionFacts, ConversationHead,
    ConversationStoreFailure, ConversationStorePort, EMPTY_PREFIX_DIGEST,
    MAX_TRANSCRIPT_PAGE_ENTRIES, PendingMessage, StoredAdmission, StoredCommand, TranscriptEntry,
    TranscriptPage, TranscriptPageBudget, WriterClaim, WriterRecoveryObservation,
    WriterRecoveryState,
};
use floe_execution::BoxFuture;
use floe_kernel::{AgentFailure, CommandId, PersonId, RunId, TaskId};
use serde::{Serialize, de::DeserializeOwned};
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

const MAX_TRANSCRIPT_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_STORED_MESSAGE_BYTES: usize = 132_096;

#[derive(Clone, Debug)]
struct LoadedHead {
    state: ConversationHead,
    identity_json: String,
}

#[derive(Clone, Copy, Debug)]
struct Scope {
    person_id: PersonId,
    conversation_id: ConversationId,
    branch_id: ConversationBranchId,
}

impl Scope {
    fn sql(self) -> (String, String, String) {
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

fn unavailable() -> ConversationStoreFailure {
    ConversationStoreFailure::Unavailable
}

fn database_error(error: turso::Error) -> ConversationStoreFailure {
    if database_failure(error) == AgentFailure::StorageBusy {
        ConversationStoreFailure::Busy
    } else {
        unavailable()
    }
}

fn start_error(error: AgentFailure) -> ConversationStoreFailure {
    if error == AgentFailure::StorageBusy {
        ConversationStoreFailure::Busy
    } else {
        unavailable()
    }
}

fn encode<T: Serialize>(value: &T) -> Result<String, ConversationStoreFailure> {
    serde_json::to_string(value).map_err(|_| unavailable())
}

fn decode<T: DeserializeOwned>(value: &str) -> Result<T, ConversationStoreFailure> {
    serde_json::from_str(value).map_err(|_| unavailable())
}

fn integer(value: u64) -> Result<i64, ConversationStoreFailure> {
    i64::try_from(value)
        .map_err(|_| ConversationStoreFailure::Transition(ConversationFailure::InvalidInput))
}

fn positive_integer(value: i64) -> Result<u64, ConversationStoreFailure> {
    u64::try_from(value).map_err(|_| unavailable())
}

fn parse_uuid(value: &str) -> Result<Uuid, ConversationStoreFailure> {
    Uuid::parse_str(value).map_err(|_| unavailable())
}

fn parse_message_id(value: &str) -> Result<MessageId, ConversationStoreFailure> {
    MessageId::from_uuid(parse_uuid(value)?).ok_or_else(unavailable)
}

fn parse_run_id(value: &str) -> Result<RunId, ConversationStoreFailure> {
    RunId::from_uuid(parse_uuid(value)?).ok_or_else(unavailable)
}

fn parse_conversation_id(value: &str) -> Result<ConversationId, ConversationStoreFailure> {
    ConversationId::from_uuid(parse_uuid(value)?).ok_or_else(unavailable)
}

fn parse_branch_id(value: &str) -> Result<ConversationBranchId, ConversationStoreFailure> {
    ConversationBranchId::from_uuid(parse_uuid(value)?).ok_or_else(unavailable)
}

fn parse_task_id(value: Option<String>) -> Result<Option<TaskId>, ConversationStoreFailure> {
    value
        .map(|value| TaskId::from_uuid(parse_uuid(&value)?).ok_or_else(unavailable))
        .transpose()
}

fn hex_digest(value: [u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in value {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn parse_digest(value: &str) -> Result<[u8; 32], ConversationStoreFailure> {
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

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn decode_stored_message(
    scope: Scope,
    sequence: u64,
    message_id: &str,
    payload: String,
    stored_bytes: i64,
    prefix_digest: &str,
) -> Result<TranscriptEntry, ConversationStoreFailure> {
    let actual_bytes = payload.as_bytes().len();
    if actual_bytes == 0
        || actual_bytes > MAX_STORED_MESSAGE_BYTES
        || usize::try_from(stored_bytes).ok() != Some(actual_bytes)
    {
        return Err(unavailable());
    }
    let message: ConversationMessage = decode(&payload)?;
    message.validate().map_err(|_| unavailable())?;
    if matches!(
        &message.origin,
        floe_conversation_contract::MessageOrigin::Person { person_id }
            if *person_id != scope.person_id
    ) {
        return Err(unavailable());
    }
    if encode(&message)? != payload {
        return Err(unavailable());
    }
    let message_id = parse_message_id(message_id)?;
    if message.message_id != message_id {
        return Err(unavailable());
    }
    Ok(TranscriptEntry {
        reference: scope.transcript_reference(message_id, sequence),
        message,
        prefix_digest: parse_digest(prefix_digest)?,
    })
}

async fn load_head_on(
    transaction: &Transaction<'_>,
    scope: Scope,
) -> Result<Option<LoadedHead>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT identity_json, head_revision, state_revision, completed_prefix FROM agent_conversation_core_heads WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
            (person, conversation, branch),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let identity_json = row.get::<String>(0).map_err(|_| unavailable())?;
    let head_revision = positive_integer(row.get::<i64>(1).map_err(|_| unavailable())?)?;
    let state_revision = positive_integer(row.get::<i64>(2).map_err(|_| unavailable())?)?;
    let completed_prefix =
        u64::try_from(row.get::<i64>(3).map_err(|_| unavailable())?).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let identity: AgentIdentity = decode(&identity_json)?;
    identity.validate().map_err(|_| unavailable())?;
    if encode(&identity)? != identity_json
        || identity.person_id != scope.person_id
        || completed_prefix > head_revision
    {
        return Err(unavailable());
    }
    Ok(Some(LoadedHead {
        state: ConversationHead {
            identity,
            conversation_id: scope.conversation_id,
            branch_id: scope.branch_id,
            head_revision,
            completed_prefix,
            state_revision,
        },
        identity_json,
    }))
}

async fn raw_entry_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    sequence: u64,
) -> Result<Option<TranscriptEntry>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT message_id, message_json, message_bytes, prefix_digest FROM agent_conversation_core_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ?",
            (person, conversation, branch, integer(sequence)?),
        )
        .await
        .map_err(database_error)?;
    let Some(row) = rows.next().await.map_err(database_error)? else {
        return Ok(None);
    };
    let message_id = row.get::<String>(0).map_err(|_| unavailable())?;
    let payload = row.get::<String>(1).map_err(|_| unavailable())?;
    let stored_bytes = row.get::<i64>(2).map_err(|_| unavailable())?;
    let prefix_digest = row.get::<String>(3).map_err(|_| unavailable())?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    Ok(Some(decode_stored_message(
        scope,
        sequence,
        &message_id,
        payload,
        stored_bytes,
        &prefix_digest,
    )?))
}

async fn entry_on(
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
                "SELECT prefix_digest FROM agent_conversation_core_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ?",
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
    let calculated =
        floe_conversation_core::advance_prefix_digest(previous, entry.reference, &entry.message)
            .map_err(|_| unavailable())?;
    if calculated != entry.prefix_digest {
        return Err(unavailable());
    }
    Ok(Some(entry))
}

async fn next_entry_sequence_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    after_sequence: u64,
) -> Result<Option<u64>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT sequence FROM agent_conversation_core_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence > ? ORDER BY sequence LIMIT 1",
            (person, conversation, branch, integer(after_sequence)?),
        )
        .await
        .map_err(database_error)?;
    rows.next()
        .await
        .map_err(database_error)?
        .map(|row| positive_integer(row.get::<i64>(0).map_err(|_| unavailable())?))
        .transpose()
}

async fn has_entry_after_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    after_sequence: u64,
) -> Result<bool, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT 1 FROM agent_conversation_core_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence > ? ORDER BY sequence LIMIT 1",
            (person, conversation, branch, integer(after_sequence)?),
        )
        .await
        .map_err(database_error)?;
    Ok(rows.next().await.map_err(database_error)?.is_some())
}

async fn message_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    message_id: MessageId,
) -> Result<Option<StoredAdmission>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let message_id_text = message_id.as_uuid().to_string();
    let mut rows = transaction
        .query(
            "SELECT sequence, receipt_json FROM agent_conversation_core_message_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND message_id = ?",
            (person.clone(), conversation.clone(), branch.clone(), message_id_text.clone()),
        )
        .await
        .map_err(database_error)?;
    let receipt_row = rows
        .next()
        .await
        .map_err(database_error)?
        .map(|row| {
            Ok::<_, ConversationStoreFailure>((
                positive_integer(row.get::<i64>(0).map_err(|_| unavailable())?)?,
                row.get::<String>(1).map_err(|_| unavailable())?,
            ))
        })
        .transpose()?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let Some((sequence, receipt_json)) = receipt_row else {
        let mut entries = transaction
            .query(
                "SELECT sequence FROM agent_conversation_core_entries WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND message_id = ? LIMIT 1",
                (person, conversation, branch, message_id_text),
            )
            .await
            .map_err(database_error)?;
        let orphan = entries.next().await.map_err(database_error)?.is_some();
        if orphan {
            return Err(unavailable());
        }
        return Ok(None);
    };
    let entry = entry_on(transaction, scope, sequence)
        .await?
        .ok_or_else(unavailable)?;
    if entry.reference.message_id != message_id {
        return Err(unavailable());
    }
    let receipt: AdmissionResultReceipt = decode(&receipt_json)?;
    if encode(&receipt)? != receipt_json
        || receipt.transcript != entry.reference
        || receipt.head_revision != sequence
        || receipt.task_id != entry.message.task_id
    {
        return Err(unavailable());
    }
    Ok(Some(StoredAdmission {
        message: entry.message,
        receipt,
    }))
}

// Alias kept local so the SQL loader's return type remains explicit.
type AdmissionResultReceipt = floe_conversation_contract::AdmissionReceipt;

async fn command_message_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    command_id: CommandId,
) -> Result<Option<StoredCommand>, ConversationStoreFailure> {
    let command_id = command_id.as_uuid().to_string();
    let mut rows = transaction
        .query(
            "SELECT conversation_id, branch_id, message_id FROM agent_conversation_core_command_receipts WHERE person_id = ? AND command_id = ?",
            (person_id.to_string(), command_id.clone()),
        )
        .await
        .map_err(database_error)?;
    let receipt_values = rows
        .next()
        .await
        .map_err(database_error)?
        .map(|row| {
            Ok::<_, ConversationStoreFailure>((
                row.get::<String>(0).map_err(|_| unavailable())?,
                row.get::<String>(1).map_err(|_| unavailable())?,
                row.get::<String>(2).map_err(|_| unavailable())?,
            ))
        })
        .transpose()?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let Some((conversation_id, branch_id, message_id)) = receipt_values else {
        return Ok(None);
    };
    let scope = Scope {
        person_id,
        conversation_id: parse_conversation_id(&conversation_id)?,
        branch_id: parse_branch_id(&branch_id)?,
    };
    let head = load_head_on(transaction, scope)
        .await?
        .ok_or_else(unavailable)?;
    let message_id = parse_message_id(&message_id)?;
    let admission = message_receipt_on(transaction, scope, message_id)
        .await?
        .ok_or_else(unavailable)?;
    Ok(Some(StoredCommand {
        identity: head.state.identity,
        conversation_id: scope.conversation_id,
        branch_id: scope.branch_id,
        admission,
    }))
}

async fn active_claim_on(
    transaction: &Transaction<'_>,
    head: &ConversationHead,
) -> Result<Option<WriterClaim>, ConversationStoreFailure> {
    let scope = Scope {
        person_id: head.identity.person_id,
        conversation_id: head.conversation_id,
        branch_id: head.branch_id,
    };
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT run_id, task_id, message_sequence, writer_epoch, executor_generation FROM agent_conversation_core_active_writers WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
            (person, conversation, branch),
        )
        .await
        .map_err(database_error)?;
    let claim_values = rows
        .next()
        .await
        .map_err(database_error)?
        .map(|row| {
            Ok::<_, ConversationStoreFailure>((
                row.get::<String>(0).map_err(|_| unavailable())?,
                parse_task_id(row.get::<Option<String>>(1).map_err(|_| unavailable())?)?,
                positive_integer(row.get::<i64>(2).map_err(|_| unavailable())?)?,
                positive_integer(row.get::<i64>(3).map_err(|_| unavailable())?)?,
                positive_integer(row.get::<i64>(4).map_err(|_| unavailable())?)?,
            ))
        })
        .transpose()?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let Some((run_id, task_id, sequence, writer_epoch, executor_generation)) = claim_values else {
        return Ok(None);
    };
    let entry = entry_on(transaction, scope, sequence)
        .await?
        .ok_or_else(unavailable)?;
    if entry.message.task_id != task_id {
        return Err(unavailable());
    }
    Ok(Some(WriterClaim {
        identity: head.identity.clone(),
        link: RunTaskLink {
            run_id: parse_run_id(&run_id)?,
            task_id,
        },
        message: entry.reference,
        writer_epoch,
        executor_generation,
    }))
}

async fn writer_receipt_on(
    transaction: &Transaction<'_>,
    head: &ConversationHead,
    run_id: RunId,
) -> Result<Option<(WriterClaim, bool)>, ConversationStoreFailure> {
    let scope = Scope {
        person_id: head.identity.person_id,
        conversation_id: head.conversation_id,
        branch_id: head.branch_id,
    };
    let (person, conversation, branch) = scope.sql();
    let run_id_text = run_id.as_uuid().to_string();
    let mut rows = transaction
        .query(
            "SELECT task_id, message_sequence, writer_epoch, executor_generation, state FROM agent_conversation_core_writer_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND run_id = ?",
            (person, conversation, branch, run_id_text.clone()),
        )
        .await
        .map_err(database_error)?;
    let values = rows
        .next()
        .await
        .map_err(database_error)?
        .map(|row| {
            Ok::<_, ConversationStoreFailure>((
                parse_task_id(row.get::<Option<String>>(0).map_err(|_| unavailable())?)?,
                positive_integer(row.get::<i64>(1).map_err(|_| unavailable())?)?,
                positive_integer(row.get::<i64>(2).map_err(|_| unavailable())?)?,
                positive_integer(row.get::<i64>(3).map_err(|_| unavailable())?)?,
                row.get::<String>(4).map_err(|_| unavailable())?,
            ))
        })
        .transpose()?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(rows);
    let Some((task_id, sequence, writer_epoch, executor_generation, state)) = values else {
        return Ok(None);
    };
    let entry = entry_on(transaction, scope, sequence)
        .await?
        .ok_or_else(unavailable)?;
    if entry.message.task_id != task_id {
        return Err(unavailable());
    }
    let completed = match state.as_str() {
        "active" => false,
        "completed" => true,
        _ => return Err(unavailable()),
    };
    Ok(Some((
        WriterClaim {
            identity: head.identity.clone(),
            link: RunTaskLink {
                run_id: parse_run_id(&run_id_text)?,
                task_id,
            },
            message: entry.reference,
            writer_epoch,
            executor_generation,
        },
        completed,
    )))
}

async fn pending_first_on(
    transaction: &Transaction<'_>,
    head: &ConversationHead,
) -> Result<Option<PendingMessage>, ConversationStoreFailure> {
    let scope = Scope {
        person_id: head.identity.person_id,
        conversation_id: head.conversation_id,
        branch_id: head.branch_id,
    };
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT sequence, message_id FROM agent_conversation_core_pending_inputs WHERE person_id = ? AND conversation_id = ? AND branch_id = ? ORDER BY sequence LIMIT 1",
            (person, conversation, branch),
        )
        .await
        .map_err(database_error)?;
    let pending = rows
        .next()
        .await
        .map_err(database_error)?
        .map(|row| {
            Ok::<_, ConversationStoreFailure>((
                positive_integer(row.get::<i64>(0).map_err(|_| unavailable())?)?,
                parse_message_id(&row.get::<String>(1).map_err(|_| unavailable())?)?,
            ))
        })
        .transpose()?;
    drop(rows);
    let Some((sequence, message_id)) = pending else {
        return Ok(None);
    };
    let entry = entry_on(transaction, scope, sequence)
        .await?
        .ok_or_else(unavailable)?;
    if entry.reference.message_id != message_id {
        return Err(unavailable());
    }
    Ok(Some(PendingMessage { entry }))
}

async fn run_was_used_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    run_id: RunId,
) -> Result<bool, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT 1 FROM agent_conversation_core_writer_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND run_id = ? LIMIT 1",
            (
                person,
                conversation,
                branch,
                run_id.as_uuid().to_string(),
            ),
        )
        .await
        .map_err(database_error)?;
    let found = rows.next().await.map_err(database_error)?.is_some();
    Ok(found)
}

async fn executor_generation_on(
    transaction: &Transaction<'_>,
) -> Result<u64, ConversationStoreFailure> {
    let mut rows = transaction
        .query(
            "SELECT generation FROM agent_conversation_executor WHERE id = 1",
            (),
        )
        .await
        .map_err(database_error)?;
    let generation = rows
        .next()
        .await
        .map_err(database_error)?
        .ok_or_else(unavailable)?
        .get::<i64>(0)
        .map_err(|_| unavailable())?;
    u64::try_from(generation).map_err(|_| unavailable())
}

async fn update_head_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    previous: Option<&LoadedHead>,
    next: &ConversationHead,
) -> Result<(), ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let identity_json = encode(&next.identity)?;
    let head_revision = integer(next.head_revision)?;
    let state_revision = integer(next.state_revision)?;
    let completed_prefix = integer(next.completed_prefix)?;
    let changed = if let Some(previous) = previous {
        transaction
            .execute(
                "UPDATE agent_conversation_core_heads SET head_revision = ?, state_revision = ?, completed_prefix = ? WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND identity_json = ? AND head_revision = ? AND state_revision = ? AND completed_prefix = ?",
                (
                    head_revision,
                    state_revision,
                    completed_prefix,
                    person,
                    conversation,
                    branch,
                    previous.identity_json.clone(),
                    integer(previous.state.head_revision)?,
                    integer(previous.state.state_revision)?,
                    integer(previous.state.completed_prefix)?,
                ),
            )
            .await
            .map_err(database_error)?
    } else {
        transaction
            .execute(
                "INSERT INTO agent_conversation_core_heads (person_id, conversation_id, branch_id, identity_json, head_revision, state_revision, completed_prefix) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (
                    person,
                    conversation,
                    branch,
                    identity_json,
                    head_revision,
                    state_revision,
                    completed_prefix,
                ),
            )
            .await
            .map_err(database_error)?
    };
    if changed != 1 {
        return Err(unavailable());
    }
    Ok(())
}

async fn insert_entry_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    entry: &TranscriptEntry,
) -> Result<(), ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let payload = encode(&entry.message)?;
    let payload_bytes = payload.as_bytes().len();
    if payload_bytes == 0 || payload_bytes > MAX_STORED_MESSAGE_BYTES {
        return Err(ConversationStoreFailure::Transition(
            ConversationFailure::InvalidInput,
        ));
    }
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_entries (person_id, conversation_id, branch_id, sequence, message_id, message_json, message_bytes, prefix_digest) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (
                person,
                conversation,
                branch,
                integer(entry.reference.sequence)?,
                entry.reference.message_id.as_uuid().to_string(),
                payload,
                i64::try_from(payload_bytes).map_err(|_| unavailable())?,
                hex_digest(entry.prefix_digest),
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn insert_message_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    receipt: &floe_conversation_contract::AdmissionReceipt,
) -> Result<(), ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let receipt_json = encode(receipt)?;
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_message_receipts (person_id, conversation_id, branch_id, message_id, sequence, receipt_json) VALUES (?, ?, ?, ?, ?, ?)",
            (
                person,
                conversation,
                branch,
                receipt.transcript.message_id.as_uuid().to_string(),
                integer(receipt.transcript.sequence)?,
                receipt_json,
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn insert_command_receipt_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    command_id: floe_kernel::CommandId,
    message_id: MessageId,
) -> Result<(), ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_command_receipts (person_id, command_id, conversation_id, branch_id, message_id) VALUES (?, ?, ?, ?, ?)",
            (
                person,
                command_id.as_uuid().to_string(),
                conversation,
                branch,
                message_id.as_uuid().to_string(),
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

async fn insert_pending_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    reference: TranscriptReference,
) -> Result<(), ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    transaction
        .execute(
            "INSERT INTO agent_conversation_core_pending_inputs (person_id, conversation_id, branch_id, sequence, message_id) VALUES (?, ?, ?, ?, ?)",
            (
                person,
                conversation,
                branch,
                integer(reference.sequence)?,
                reference.message_id.as_uuid().to_string(),
            ),
        )
        .await
        .map_err(database_error)?;
    Ok(())
}

#[derive(Clone, Debug)]
struct LoadedCheckpoint {
    sequence: u64,
    prefix_digest: [u8; 32],
    summary: String,
}

async fn checkpoint_on(
    transaction: &Transaction<'_>,
    scope: Scope,
) -> Result<Option<LoadedCheckpoint>, ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(
            "SELECT sequence, prefix_digest, summary FROM agent_conversation_core_checkpoints WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
            (person, conversation, branch),
        )
        .await
        .map_err(database_error)?;
    let checkpoint = rows
        .next()
        .await
        .map_err(database_error)?
        .map(|row| {
            Ok::<_, ConversationStoreFailure>(LoadedCheckpoint {
                sequence: positive_integer(row.get::<i64>(0).map_err(|_| unavailable())?)?,
                prefix_digest: parse_digest(&row.get::<String>(1).map_err(|_| unavailable())?)?,
                summary: row.get::<String>(2).map_err(|_| unavailable())?,
            })
        })
        .transpose()?;
    if rows.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    Ok(checkpoint)
}

async fn write_checkpoint_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    previous: Option<&LoadedCheckpoint>,
    checkpoint: &ConversationCheckpoint,
) -> Result<(), ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let sequence = integer(checkpoint.through.sequence)?;
    let digest = hex_digest(checkpoint.prefix_digest);
    match previous {
        Some(previous) => {
            let changed = transaction
                .execute(
                    "UPDATE agent_conversation_core_checkpoints SET sequence = ?, prefix_digest = ?, summary = ? WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ? AND prefix_digest = ? AND summary = ?",
                    (
                        sequence,
                        digest,
                        checkpoint.summary.clone(),
                        person,
                        conversation,
                        branch,
                        integer(previous.sequence)?,
                        hex_digest(previous.prefix_digest),
                        previous.summary.clone(),
                    ),
                )
                .await
                .map_err(database_error)?;
            if changed != 1 {
                return Err(unavailable());
            }
        }
        None => {
            transaction
                .execute(
                    "INSERT INTO agent_conversation_core_checkpoints (person_id, conversation_id, branch_id, sequence, prefix_digest, summary) VALUES (?, ?, ?, ?, ?, ?)",
                    (
                        person,
                        conversation,
                        branch,
                        sequence,
                        digest,
                        checkpoint.summary.clone(),
                    ),
                )
                .await
                .map_err(database_error)?;
        }
    }
    Ok(())
}

async fn count_min_max_on(
    transaction: &Transaction<'_>,
    query: &str,
    scope: Scope,
) -> Result<(u64, Option<u64>, Option<u64>), ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let mut rows = transaction
        .query(query, (person, conversation, branch))
        .await
        .map_err(database_error)?;
    let row = rows
        .next()
        .await
        .map_err(database_error)?
        .ok_or_else(unavailable)?;
    let count =
        u64::try_from(row.get::<i64>(0).map_err(|_| unavailable())?).map_err(|_| unavailable())?;
    let min = row
        .get::<Option<i64>>(1)
        .map_err(|_| unavailable())?
        .map(positive_integer)
        .transpose()?;
    let max = row
        .get::<Option<i64>>(2)
        .map_err(|_| unavailable())?
        .map(positive_integer)
        .transpose()?;
    Ok((count, min, max))
}

async fn validate_message_receipts_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    entry: &TranscriptEntry,
) -> Result<(), ConversationStoreFailure> {
    let (person, conversation, branch) = scope.sql();
    let message_id = entry.reference.message_id.as_uuid().to_string();
    let mut receipts = transaction
        .query(
            "SELECT sequence, receipt_json FROM agent_conversation_core_message_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND message_id = ?",
            (person.clone(), conversation.clone(), branch.clone(), message_id.clone()),
        )
        .await
        .map_err(database_error)?;
    let row = receipts
        .next()
        .await
        .map_err(database_error)?
        .ok_or_else(unavailable)?;
    let sequence = positive_integer(row.get::<i64>(0).map_err(|_| unavailable())?)?;
    let receipt_json = row.get::<String>(1).map_err(|_| unavailable())?;
    if receipts.next().await.map_err(database_error)?.is_some() {
        return Err(unavailable());
    }
    drop(receipts);
    let receipt: AdmissionResultReceipt = decode(&receipt_json)?;
    if encode(&receipt)? != receipt_json
        || sequence != entry.reference.sequence
        || receipt.transcript != entry.reference
        || receipt.head_revision != sequence
        || receipt.task_id != entry.message.task_id
    {
        return Err(unavailable());
    }
    let mut commands = transaction
        .query(
            "SELECT command_id FROM agent_conversation_core_command_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND message_id = ? ORDER BY command_id",
            (person, conversation, branch, message_id),
        )
        .await
        .map_err(database_error)?;
    let expected_command = entry.message.command_id;
    let mut found_expected = false;
    let mut found_any = false;
    while let Some(row) = commands.next().await.map_err(database_error)? {
        let command_id = parse_uuid(&row.get::<String>(0).map_err(|_| unavailable())?)?;
        let command_id = CommandId::from_uuid(command_id).ok_or_else(unavailable)?;
        found_any = true;
        found_expected |= command_id == expected_command;
    }
    if !found_any || !found_expected {
        drop(commands);
        return Err(unavailable());
    }
    Ok(())
}

async fn validate_writer_state_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    head: &ConversationHead,
) -> Result<Option<WriterClaim>, ConversationStoreFailure> {
    let active = active_claim_on(transaction, head).await?;
    if let Some(claim) = &active {
        if claim.message.sequence != head.completed_prefix.saturating_add(1) {
            return Err(unavailable());
        }
    }

    let (completed_count, completed_min, completed_max) = count_min_max_on(
        transaction,
        "SELECT COUNT(*), MIN(message_sequence), MAX(message_sequence) FROM agent_conversation_core_writer_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND state = 'completed'",
        scope,
    )
    .await?;
    let expected_completed = head.completed_prefix;
    if completed_count != expected_completed
        || (expected_completed == 0 && (completed_min.is_some() || completed_max.is_some()))
        || (expected_completed > 0
            && (completed_min != Some(1) || completed_max != Some(expected_completed)))
    {
        return Err(unavailable());
    }

    let (active_receipt_count, active_min, active_max) = count_min_max_on(
        transaction,
        "SELECT COUNT(*), MIN(message_sequence), MAX(message_sequence) FROM agent_conversation_core_writer_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND state = 'active'",
        scope,
    )
    .await?;
    if active_receipt_count != u64::from(active.is_some())
        || active_min != active.as_ref().map(|claim| claim.message.sequence)
        || active_max != active.as_ref().map(|claim| claim.message.sequence)
    {
        return Err(unavailable());
    }

    let mut next_completed = 1_u64;
    let mut after_sequence = 0_u64;
    loop {
        let (person, conversation, branch) = scope.sql();
        let mut rows = transaction
            .query(
                "SELECT run_id, task_id, message_sequence, writer_epoch, executor_generation, state FROM agent_conversation_core_writer_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND message_sequence > ? ORDER BY message_sequence LIMIT 1",
                (person, conversation, branch, integer(after_sequence)?),
            )
            .await
            .map_err(database_error)?;
        let receipt = rows
            .next()
            .await
            .map_err(database_error)?
            .map(|row| {
                Ok::<_, ConversationStoreFailure>((
                    parse_run_id(&row.get::<String>(0).map_err(|_| unavailable())?)?,
                    parse_task_id(row.get::<Option<String>>(1).map_err(|_| unavailable())?)?,
                    positive_integer(row.get::<i64>(2).map_err(|_| unavailable())?)?,
                    positive_integer(row.get::<i64>(3).map_err(|_| unavailable())?)?,
                    positive_integer(row.get::<i64>(4).map_err(|_| unavailable())?)?,
                    row.get::<String>(5).map_err(|_| unavailable())?,
                ))
            })
            .transpose()?;
        drop(rows);
        let Some((run_id, task_id, sequence, writer_epoch, generation, state)) = receipt else {
            break;
        };
        if writer_epoch > head.state_revision {
            return Err(unavailable());
        }
        let entry = entry_on(transaction, scope, sequence)
            .await?
            .ok_or_else(unavailable)?;
        if entry.message.task_id != task_id {
            return Err(unavailable());
        }
        match state.as_str() {
            "completed" => {
                if sequence != next_completed || sequence > head.completed_prefix {
                    return Err(unavailable());
                }
                next_completed = next_completed.checked_add(1).ok_or_else(unavailable)?;
            }
            "active" => {
                let claim = active.as_ref().ok_or_else(unavailable)?;
                if sequence != head.completed_prefix.saturating_add(1)
                    || claim.link.run_id != run_id
                    || claim.link.task_id != task_id
                    || claim.message.sequence != sequence
                    || claim.writer_epoch != writer_epoch
                    || claim.executor_generation != generation
                {
                    return Err(unavailable());
                }
            }
            _ => return Err(unavailable()),
        }
        after_sequence = sequence;
    }
    if next_completed.saturating_sub(1) != expected_completed {
        return Err(unavailable());
    }
    Ok(active)
}

async fn validate_pending_state_on(
    transaction: &Transaction<'_>,
    scope: Scope,
    head: &ConversationHead,
    active: Option<&WriterClaim>,
) -> Result<(), ConversationStoreFailure> {
    let (count, min, max) = count_min_max_on(
        transaction,
        "SELECT COUNT(*), MIN(sequence), MAX(sequence) FROM agent_conversation_core_pending_inputs WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
        scope,
    )
    .await?;
    let inflight = u64::from(active.is_some());
    let expected = head
        .head_revision
        .checked_sub(head.completed_prefix)
        .and_then(|tail| tail.checked_sub(inflight))
        .ok_or_else(unavailable)?;
    let expected_min = head
        .completed_prefix
        .checked_add(inflight)
        .and_then(|sequence| sequence.checked_add(1))
        .ok_or_else(unavailable)?;
    let expected_max = head.head_revision;
    if count != expected
        || (expected == 0 && (min.is_some() || max.is_some()))
        || (expected > 0 && (min != Some(expected_min) || max != Some(expected_max)))
    {
        return Err(unavailable());
    }
    let mut expected_sequence = expected_min;
    loop {
        let (person, conversation, branch) = scope.sql();
        let mut rows = transaction
            .query(
                "SELECT p.sequence, p.message_id, e.message_id FROM agent_conversation_core_pending_inputs p LEFT JOIN agent_conversation_core_entries e ON e.person_id = p.person_id AND e.conversation_id = p.conversation_id AND e.branch_id = p.branch_id AND e.sequence = p.sequence WHERE p.person_id = ? AND p.conversation_id = ? AND p.branch_id = ? AND p.sequence >= ? ORDER BY p.sequence LIMIT 1",
                (person, conversation, branch, integer(expected_sequence)?),
            )
            .await
            .map_err(database_error)?;
        let pending = rows
            .next()
            .await
            .map_err(database_error)?
            .map(|row| {
                Ok::<_, ConversationStoreFailure>((
                    positive_integer(row.get::<i64>(0).map_err(|_| unavailable())?)?,
                    parse_message_id(&row.get::<String>(1).map_err(|_| unavailable())?)?,
                    row.get::<Option<String>>(2).map_err(|_| unavailable())?,
                ))
            })
            .transpose()?;
        drop(rows);
        let Some((sequence, message_id, entry_message_id)) = pending else {
            break;
        };
        if sequence != expected_sequence {
            return Err(unavailable());
        }
        if entry_message_id
            .as_deref()
            .map(parse_message_id)
            .transpose()?
            != Some(message_id)
        {
            return Err(unavailable());
        }
        expected_sequence = expected_sequence.checked_add(1).ok_or_else(unavailable)?;
    }
    if expected_sequence != expected_max.saturating_add(1) {
        return Err(unavailable());
    }
    Ok(())
}

async fn validate_conversation_core_row_scope_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
) -> Result<(), ConversationStoreFailure> {
    let mut rows = transaction
        .query(
            "SELECT (SELECT COUNT(*) FROM agent_conversation_core_heads WHERE person_id <> ?1) + (SELECT COUNT(*) FROM agent_conversation_core_entries WHERE person_id <> ?1) + (SELECT COUNT(*) FROM agent_conversation_core_message_receipts WHERE person_id <> ?1) + (SELECT COUNT(*) FROM agent_conversation_core_command_receipts WHERE person_id <> ?1) + (SELECT COUNT(*) FROM agent_conversation_core_pending_inputs WHERE person_id <> ?1) + (SELECT COUNT(*) FROM agent_conversation_core_active_writers WHERE person_id <> ?1) + (SELECT COUNT(*) FROM agent_conversation_core_writer_receipts WHERE person_id <> ?1) + (SELECT COUNT(*) FROM agent_conversation_core_checkpoints WHERE person_id <> ?1)",
            [person_id.to_string()],
        )
        .await
        .map_err(database_error)?;
    let foreign_person_rows = rows
        .next()
        .await
        .map_err(database_error)?
        .ok_or_else(unavailable)?
        .get::<i64>(0)
        .map_err(|_| unavailable())?;
    if foreign_person_rows != 0 {
        return Err(unavailable());
    }

    let mut rows = transaction
        .query(
            "SELECT (SELECT COUNT(*) FROM agent_conversation_core_entries e LEFT JOIN agent_conversation_core_heads h ON h.person_id = e.person_id AND h.conversation_id = e.conversation_id AND h.branch_id = e.branch_id WHERE h.person_id IS NULL) + (SELECT COUNT(*) FROM agent_conversation_core_message_receipts r LEFT JOIN agent_conversation_core_heads h ON h.person_id = r.person_id AND h.conversation_id = r.conversation_id AND h.branch_id = r.branch_id WHERE h.person_id IS NULL) + (SELECT COUNT(*) FROM agent_conversation_core_command_receipts c LEFT JOIN agent_conversation_core_heads h ON h.person_id = c.person_id AND h.conversation_id = c.conversation_id AND h.branch_id = c.branch_id WHERE h.person_id IS NULL) + (SELECT COUNT(*) FROM agent_conversation_core_pending_inputs p LEFT JOIN agent_conversation_core_heads h ON h.person_id = p.person_id AND h.conversation_id = p.conversation_id AND h.branch_id = p.branch_id WHERE h.person_id IS NULL) + (SELECT COUNT(*) FROM agent_conversation_core_active_writers w LEFT JOIN agent_conversation_core_heads h ON h.person_id = w.person_id AND h.conversation_id = w.conversation_id AND h.branch_id = w.branch_id WHERE h.person_id IS NULL) + (SELECT COUNT(*) FROM agent_conversation_core_writer_receipts w LEFT JOIN agent_conversation_core_heads h ON h.person_id = w.person_id AND h.conversation_id = w.conversation_id AND h.branch_id = w.branch_id WHERE h.person_id IS NULL) + (SELECT COUNT(*) FROM agent_conversation_core_checkpoints c LEFT JOIN agent_conversation_core_heads h ON h.person_id = c.person_id AND h.conversation_id = c.conversation_id AND h.branch_id = c.branch_id WHERE h.person_id IS NULL)",
            (),
        )
        .await
        .map_err(database_error)?;
    let orphan_rows = rows
        .next()
        .await
        .map_err(database_error)?
        .ok_or_else(unavailable)?
        .get::<i64>(0)
        .map_err(|_| unavailable())?;
    if orphan_rows != 0 {
        return Err(unavailable());
    }
    Ok(())
}

async fn validate_scope_on(
    transaction: &Transaction<'_>,
    loaded_head: &LoadedHead,
) -> Result<(), ConversationStoreFailure> {
    let head = &loaded_head.state;
    let scope = Scope {
        person_id: head.identity.person_id,
        conversation_id: head.conversation_id,
        branch_id: head.branch_id,
    };
    if head.head_revision == 0
        || head.state_revision < head.head_revision
        || head.completed_prefix > head.head_revision
    {
        return Err(unavailable());
    }
    let checkpoint = checkpoint_on(transaction, scope).await?;
    if checkpoint
        .as_ref()
        .is_some_and(|value| value.sequence > head.completed_prefix)
    {
        return Err(unavailable());
    }

    let mut prefix = EMPTY_PREFIX_DIGEST;
    let mut checkpoint_digest = None;
    let mut sequence = 1;
    while sequence <= head.head_revision {
        let entry = raw_entry_on(transaction, scope, sequence)
            .await?
            .ok_or_else(unavailable)?;
        if let floe_conversation_contract::MessageOrigin::Person { person_id } =
            &entry.message.origin
        {
            if *person_id != head.identity.person_id {
                return Err(unavailable());
            }
        }
        let calculated =
            floe_conversation_core::advance_prefix_digest(prefix, entry.reference, &entry.message)
                .map_err(|_| unavailable())?;
        if calculated != entry.prefix_digest {
            return Err(unavailable());
        }
        prefix = calculated;
        if checkpoint
            .as_ref()
            .is_some_and(|value| value.sequence == sequence)
        {
            checkpoint_digest = Some(prefix);
        }
        validate_message_receipts_on(transaction, scope, &entry).await?;
        sequence = sequence.checked_add(1).ok_or_else(unavailable)?;
    }
    if has_entry_after_on(transaction, scope, head.head_revision).await? {
        return Err(unavailable());
    }
    let (receipt_count, _, _) = count_min_max_on(
        transaction,
        "SELECT COUNT(*), MIN(sequence), MAX(sequence) FROM agent_conversation_core_message_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
        scope,
    )
    .await?;
    if receipt_count != head.head_revision {
        return Err(unavailable());
    }
    let (orphan_command_count, _, _) = count_min_max_on(
        transaction,
        "SELECT COUNT(*), MIN(1), MAX(1) FROM agent_conversation_core_command_receipts c LEFT JOIN agent_conversation_core_message_receipts m ON m.person_id = c.person_id AND m.conversation_id = c.conversation_id AND m.branch_id = c.branch_id AND m.message_id = c.message_id WHERE c.person_id = ? AND c.conversation_id = ? AND c.branch_id = ? AND m.message_id IS NULL",
        scope,
    )
    .await?;
    if orphan_command_count != 0 {
        return Err(unavailable());
    }
    let (person, conversation, branch) = scope.sql();
    let mut commands = transaction
        .query(
            "SELECT command_id, message_id FROM agent_conversation_core_command_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ? ORDER BY command_id",
            (person, conversation, branch),
        )
        .await
        .map_err(database_error)?;
    while let Some(row) = commands.next().await.map_err(database_error)? {
        let command_id = parse_uuid(&row.get::<String>(0).map_err(|_| unavailable())?)?;
        CommandId::from_uuid(command_id).ok_or_else(unavailable)?;
        parse_message_id(&row.get::<String>(1).map_err(|_| unavailable())?)?;
    }
    drop(commands);
    let active = validate_writer_state_on(transaction, scope, head).await?;
    validate_pending_state_on(transaction, scope, head, active.as_ref()).await?;

    if let Some(checkpoint) = checkpoint {
        let Some(prefix_digest) = checkpoint_digest else {
            return Err(unavailable());
        };
        if prefix_digest != checkpoint.prefix_digest {
            return Err(unavailable());
        }
        let entry = raw_entry_on(transaction, scope, checkpoint.sequence)
            .await?
            .ok_or_else(unavailable)?;
        let value = ConversationCheckpoint {
            through: entry.reference,
            prefix_digest,
            summary: checkpoint.summary,
        };
        value.validate().map_err(|_| unavailable())?;
    }
    Ok(())
}

async fn validate_all_conversations(
    transaction: &Transaction<'_>,
    person_id: PersonId,
) -> Result<(), ConversationStoreFailure> {
    let mut after_conversation = String::new();
    let mut after_branch = String::new();
    loop {
        let mut rows = transaction
            .query(
                "SELECT conversation_id, branch_id FROM agent_conversation_core_heads WHERE person_id = ? AND (conversation_id > ? OR (conversation_id = ? AND branch_id > ?)) ORDER BY conversation_id, branch_id LIMIT 1",
                (
                    person_id.to_string(),
                    after_conversation.clone(),
                    after_conversation.clone(),
                    after_branch.clone(),
                ),
            )
            .await
            .map_err(database_error)?;
        let row = rows.next().await.map_err(database_error)?;
        let Some(row) = row else {
            break;
        };
        let conversation_raw = row.get::<String>(0).map_err(|_| unavailable())?;
        let branch_raw = row.get::<String>(1).map_err(|_| unavailable())?;
        drop(rows);
        let conversation_id =
            ConversationId::from_uuid(parse_uuid(&conversation_raw)?).ok_or_else(unavailable)?;
        let branch_id =
            ConversationBranchId::from_uuid(parse_uuid(&branch_raw)?).ok_or_else(unavailable)?;
        let scope = Scope {
            person_id,
            conversation_id,
            branch_id,
        };
        let head = load_head_on(transaction, scope)
            .await?
            .ok_or_else(unavailable)?;
        validate_scope_on(transaction, &head).await?;
        after_conversation = conversation_raw;
        after_branch = branch_raw;
    }
    validate_conversation_core_row_scope_on(transaction, person_id).await
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    fn conversation_core_scope(
        &self,
        target: &ConversationReference,
    ) -> Result<Scope, ConversationStoreFailure> {
        if target.identity.person_id != self.person_id {
            return Err(ConversationFailure::AgentMismatch.into());
        }
        Ok(Scope {
            person_id: self.person_id,
            conversation_id: target.conversation_id,
            branch_id: target.branch_id,
        })
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
                    ConversationStoreFailure::Transition(_) => Err(failure),
                    _ => Err(ConversationStoreFailure::NotCommitted),
                }
            }
        }
    }

    async fn finish_conversation_core_read<T>(
        &self,
        mut guard: JournalWriteGuard<'_>,
        transaction: Transaction<'_>,
        result: Result<T, ConversationStoreFailure>,
    ) -> Result<T, ConversationStoreFailure> {
        match result {
            Ok(value) => {
                transaction.commit().await.map_err(|_| unavailable())?;
                guard.settled();
                if self.check_access().is_err() {
                    self.unavailable
                        .store(true, std::sync::atomic::Ordering::Release);
                    return Err(unavailable());
                }
                Ok(value)
            }
            Err(failure) => {
                if transaction.rollback().await.is_err() {
                    self.unavailable
                        .store(true, std::sync::atomic::Ordering::Release);
                    return Err(unavailable());
                }
                guard.settled();
                Err(failure)
            }
        }
    }

    pub(super) async fn validate_conversation_core_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let result = validate_all_conversations(&transaction, self.person_id)
            .await
            .map_err(|_| AgentFailure::VaultUnavailable);
        self.finish_access_grant_transaction(transaction, result)
            .await
    }
}

impl<Keys: VaultKeyProvider> ConversationStorePort for EncryptedAgentVault<Keys> {
    fn admit<'a>(
        &'a self,
        request: MessageAdmissionRequest,
    ) -> BoxFuture<'a, Result<AdmissionResult, ConversationStoreFailure>> {
        Box::pin(async move {
            let (conversation_id, branch_id, identity) = match &request.target {
                floe_conversation_contract::AdmissionTarget::New {
                    conversation_id,
                    branch_id,
                    identity,
                } => (*conversation_id, *branch_id, identity),
                floe_conversation_contract::AdmissionTarget::Continue { reference } => (
                    reference.conversation_id,
                    reference.branch_id,
                    &reference.identity,
                ),
            };
            if identity.person_id != self.person_id {
                return Err(ConversationFailure::AgentMismatch.into());
            }
            let scope = Scope {
                person_id: self.person_id,
                conversation_id,
                branch_id,
            };
            let mut connection = self.connection().map_err(start_error)?;
            let (guard, transaction) = self
                .journal_transaction(&mut connection)
                .await
                .map_err(start_error)?;
            let result = async {
                crate::schema::ensure_conversation_core_family(&transaction)
                    .await
                    .map_err(|_| unavailable())?;
                let previous_head = load_head_on(&transaction, scope).await?;
                let message =
                    message_receipt_on(&transaction, scope, request.message.message_id).await?;
                let command =
                    command_message_on(&transaction, self.person_id, request.message.command_id)
                        .await?;
                if previous_head.is_none() && message.is_some() {
                    return Err(unavailable());
                }
                let active_writer = match &previous_head {
                    Some(head) => active_claim_on(&transaction, &head.state).await?,
                    None => None,
                };
                let previous_prefix_digest = match &previous_head {
                    Some(head) => {
                        let entry = entry_on(&transaction, scope, head.state.head_revision)
                            .await?
                            .ok_or_else(unavailable)?;
                        entry.prefix_digest
                    }
                    None => EMPTY_PREFIX_DIGEST,
                };
                let transition = floe_conversation_core::admit_transition(
                    AdmissionFacts {
                        head: previous_head.as_ref().map(|head| head.state.clone()),
                        active_writer,
                        message,
                        command,
                        previous_prefix_digest,
                    },
                    request.clone(),
                )
                .map_err(ConversationStoreFailure::Transition)?;

                if transition.head_changed {
                    update_head_on(
                        &transaction,
                        scope,
                        previous_head.as_ref(),
                        &transition.head,
                    )
                    .await?;
                }
                if let Some(entry) = &transition.appended {
                    insert_entry_on(&transaction, scope, entry).await?;
                    insert_message_receipt_on(&transaction, scope, &transition.result.receipt)
                        .await?;
                    #[cfg(test)]
                    if self
                        .conversation_core_failure_before_pending
                        .swap(false, std::sync::atomic::Ordering::AcqRel)
                    {
                        // Execute a real failing SQLite statement after the
                        // head, transcript and message receipt have changed.
                        transaction
                            .execute(
                                "INSERT INTO floe_conversation_core_fault_injection (id) VALUES (1)",
                                (),
                            )
                            .await
                            .map_err(database_error)?;
                    }
                    insert_pending_on(&transaction, scope, entry.reference).await?;
                }
                if transition.command_receipt.is_some() {
                    insert_command_receipt_on(
                        &transaction,
                        scope,
                        request.message.command_id,
                        request.message.message_id,
                    )
                    .await?;
                }
                Ok(transition.result)
            }
            .await;
            self.finish_conversation_core_transaction(guard, transaction, result)
                .await
        })
    }

    fn read_transcript_page<'a>(
        &'a self,
        target: ConversationReference,
        after: Option<TranscriptReference>,
        budget: TranscriptPageBudget,
    ) -> BoxFuture<'a, Result<TranscriptPage, ConversationStoreFailure>> {
        Box::pin(async move {
            let scope = self.conversation_core_scope(&target)?;
            if budget.max_entries == 0 || budget.max_bytes == 0 {
                return Err(ConversationFailure::InvalidInput.into());
            }
            let max_entries = budget.max_entries.min(MAX_TRANSCRIPT_PAGE_ENTRIES);
            let max_bytes = budget.max_bytes.min(MAX_TRANSCRIPT_PAGE_BYTES);
            let mut connection = self.connection().map_err(start_error)?;
            let (guard, transaction) = self
                .journal_transaction(&mut connection)
                .await
                .map_err(start_error)?;
            let result = async {
                crate::schema::ensure_conversation_core_family(&transaction)
                    .await
                    .map_err(|_| unavailable())?;
                let head = load_head_on(&transaction, scope).await?.ok_or(
                    ConversationStoreFailure::Transition(ConversationFailure::ConversationMismatch),
                )?;
                floe_conversation_core::validate_reference_target(&head.state, &target)
                    .map_err(ConversationStoreFailure::Transition)?;
                let mut cursor_sequence = 0;
                if let Some(cursor) = after {
                    cursor
                        .validate()
                        .map_err(ConversationStoreFailure::Transition)?;
                    if cursor.conversation_id != scope.conversation_id
                        || cursor.branch_id != scope.branch_id
                        || cursor.sequence > head.state.head_revision
                    {
                        return Err(ConversationFailure::ConversationMismatch.into());
                    }
                    let stored = entry_on(&transaction, scope, cursor.sequence)
                        .await?
                        .ok_or(ConversationStoreFailure::Transition(
                            ConversationFailure::ConversationMismatch,
                        ))?;
                    if stored.reference != cursor {
                        return Err(ConversationFailure::ConversationMismatch.into());
                    }
                    cursor_sequence = cursor.sequence;
                }

                let mut entries = Vec::with_capacity(max_entries.min(32));
                let mut encoded_bytes = 0usize;
                while entries.len() < max_entries {
                    let Some(sequence) =
                        next_entry_sequence_on(&transaction, scope, cursor_sequence).await?
                    else {
                        break;
                    };
                    if sequence != cursor_sequence.saturating_add(1)
                        || sequence > head.state.head_revision
                    {
                        return Err(unavailable());
                    }
                    let entry = entry_on(&transaction, scope, sequence)
                        .await?
                        .ok_or_else(unavailable)?;
                    let entry_bytes = encode(&entry.message)?.as_bytes().len();
                    if entry_bytes > max_bytes.saturating_sub(encoded_bytes) {
                        if entries.is_empty() {
                            return Err(ConversationStoreFailure::PageItemExceedsBudget);
                        }
                        break;
                    }
                    encoded_bytes = encoded_bytes.checked_add(entry_bytes).ok_or(
                        ConversationStoreFailure::Transition(ConversationFailure::InvalidInput),
                    )?;
                    cursor_sequence = sequence;
                    entries.push(entry);
                }
                let has_more = has_entry_after_on(&transaction, scope, cursor_sequence).await?;
                let next_cursor = entries.last().map(|entry| entry.reference);
                Ok(TranscriptPage {
                    entries,
                    next_cursor,
                    has_more,
                    encoded_bytes,
                })
            }
            .await;
            self.finish_conversation_core_read(guard, transaction, result)
                .await
        })
    }

    fn claim_writer<'a>(
        &'a self,
        target: ConversationReference,
        link: RunTaskLink,
        executor_generation: u64,
    ) -> BoxFuture<'a, Result<WriterClaim, ConversationStoreFailure>> {
        Box::pin(async move {
            let scope = self.conversation_core_scope(&target)?;
            let mut connection = self.connection().map_err(start_error)?;
            let (guard, transaction) = self
                .journal_transaction(&mut connection)
                .await
                .map_err(start_error)?;
            let result = async {
                crate::schema::ensure_conversation_core_family(&transaction)
                    .await
                    .map_err(|_| unavailable())?;
                let loaded_head = load_head_on(&transaction, scope)
                    .await?
                    .ok_or(ConversationStoreFailure::Transition(
                        ConversationFailure::ConversationMismatch,
                    ))?;
                let active_writer = active_claim_on(&transaction, &loaded_head.state).await?;
                let earliest_pending = pending_first_on(&transaction, &loaded_head.state).await?;
                if active_writer.is_none()
                    && earliest_pending.is_none()
                    && loaded_head.state.head_revision > loaded_head.state.completed_prefix
                {
                    return Err(unavailable());
                }
                let actual_generation = executor_generation_on(&transaction).await?;
                let used = run_was_used_on(&transaction, scope, link.run_id).await?;
                let transition = floe_conversation_core::claim_writer_transition(
                    &target,
                    link,
                    ClaimFacts {
                        head: loaded_head.state.clone(),
                        active_writer,
                        earliest_pending,
                        run_already_used: used,
                        requested_executor_generation: executor_generation,
                        executor_generation: actual_generation,
                    },
                )
                .map_err(ConversationStoreFailure::Transition)?;
                let (person, conversation, branch) = scope.sql();
                let removed = transaction
                    .execute(
                        "DELETE FROM agent_conversation_core_pending_inputs WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = ? AND message_id = ?",
                        (
                            person.clone(),
                            conversation.clone(),
                            branch.clone(),
                            integer(transition.claim.message.sequence)?,
                            transition.claim.message.message_id.as_uuid().to_string(),
                        ),
                    )
                    .await
                    .map_err(database_error)?;
                if removed != 1 {
                    return Err(unavailable());
                }
                update_head_on(
                    &transaction,
                    scope,
                    Some(&loaded_head),
                    &transition.head,
                )
                .await?;
                let run_id = transition.claim.link.run_id.as_uuid().to_string();
                let task_id = transition
                    .claim
                    .link
                    .task_id
                    .map(|id| id.as_uuid().to_string());
                let sequence = integer(transition.claim.message.sequence)?;
                let epoch = integer(transition.claim.writer_epoch)?;
                let generation = integer(transition.claim.executor_generation)?;
                transaction
                    .execute(
                        "INSERT INTO agent_conversation_core_active_writers (person_id, conversation_id, branch_id, run_id, task_id, message_sequence, writer_epoch, executor_generation) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                        (
                            person.clone(),
                            conversation.clone(),
                            branch.clone(),
                            run_id.clone(),
                            task_id.clone(),
                            sequence,
                            epoch,
                            generation,
                        ),
                    )
                    .await
                    .map_err(database_error)?;
                transaction
                    .execute(
                        "INSERT INTO agent_conversation_core_writer_receipts (person_id, conversation_id, branch_id, run_id, task_id, message_sequence, writer_epoch, executor_generation, state) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'active')",
                        (
                            person,
                            conversation,
                            branch,
                            run_id,
                            task_id,
                            sequence,
                            epoch,
                            generation,
                        ),
                    )
                    .await
                    .map_err(database_error)?;
                Ok(transition.claim)
            }
            .await;
            self.finish_conversation_core_transaction(guard, transaction, result)
                .await
        })
    }

    fn complete_writer<'a>(
        &'a self,
        claim: WriterClaim,
    ) -> BoxFuture<'a, Result<(), ConversationStoreFailure>> {
        Box::pin(async move {
            let scope = Scope {
                person_id: self.person_id,
                conversation_id: claim.message.conversation_id,
                branch_id: claim.message.branch_id,
            };
            if claim.identity.person_id != self.person_id {
                return Err(ConversationFailure::AgentMismatch.into());
            }
            let mut connection = self.connection().map_err(start_error)?;
            let (guard, transaction) = self
                .journal_transaction(&mut connection)
                .await
                .map_err(start_error)?;
            let result = async {
                crate::schema::ensure_conversation_core_family(&transaction)
                    .await
                    .map_err(|_| unavailable())?;
                let loaded_head = load_head_on(&transaction, scope)
                    .await?
                    .ok_or(ConversationStoreFailure::Transition(
                        ConversationFailure::WrongWriter,
                    ))?;
                let active_writer = active_claim_on(&transaction, &loaded_head.state).await?;
                let actual_generation = executor_generation_on(&transaction).await?;
                let next_head = floe_conversation_core::complete_writer_transition(
                    claim.clone(),
                    CompletionFacts {
                        head: loaded_head.state.clone(),
                        active_writer,
                        executor_generation: actual_generation,
                    },
                )
                .map_err(ConversationStoreFailure::Transition)?;
                update_head_on(
                    &transaction,
                    scope,
                    Some(&loaded_head),
                    &next_head,
                )
                .await?;
                let (person, conversation, branch) = scope.sql();
                let run_id = claim.link.run_id.as_uuid().to_string();
                let task_id = claim.link.task_id.map(|id| id.as_uuid().to_string());
                let sequence = integer(claim.message.sequence)?;
                let epoch = integer(claim.writer_epoch)?;
                let generation = integer(claim.executor_generation)?;
                let completed = transaction
                    .execute(
                        "UPDATE agent_conversation_core_writer_receipts SET state = 'completed' WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND run_id = ? AND task_id IS ? AND message_sequence = ? AND writer_epoch = ? AND executor_generation = ? AND state = 'active'",
                        (
                            person.clone(),
                            conversation.clone(),
                            branch.clone(),
                            run_id.clone(),
                            task_id.clone(),
                            sequence,
                            epoch,
                            generation,
                        ),
                    )
                    .await
                    .map_err(database_error)?;
                if completed != 1 {
                    return Err(unavailable());
                }
                let removed = transaction
                    .execute(
                        "DELETE FROM agent_conversation_core_active_writers WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND run_id = ? AND task_id IS ? AND message_sequence = ? AND writer_epoch = ? AND executor_generation = ?",
                        (
                            person,
                            conversation,
                            branch,
                            run_id,
                            task_id,
                            sequence,
                            epoch,
                            generation,
                        ),
                    )
                    .await
                    .map_err(database_error)?;
                if removed != 1 {
                    return Err(unavailable());
                }
                Ok(())
            }
            .await;
            self.finish_conversation_core_transaction(guard, transaction, result)
                .await
        })
    }

    fn observe_writer<'a>(
        &'a self,
        target: ConversationReference,
        run_id: RunId,
    ) -> BoxFuture<'a, Result<WriterRecoveryObservation, ConversationStoreFailure>> {
        Box::pin(async move {
            let scope = self.conversation_core_scope(&target)?;
            if !run_id.is_valid() {
                return Err(ConversationFailure::InvalidInput.into());
            }
            target
                .validate()
                .map_err(ConversationStoreFailure::Transition)?;
            let mut connection = self.connection().map_err(start_error)?;
            let (guard, transaction) = self
                .journal_transaction(&mut connection)
                .await
                .map_err(start_error)?;
            let result = async {
                crate::schema::ensure_conversation_core_family(&transaction)
                    .await
                    .map_err(|_| unavailable())?;
                let loaded_head = load_head_on(&transaction, scope).await?.ok_or(
                    ConversationStoreFailure::Transition(ConversationFailure::ConversationMismatch),
                )?;
                floe_conversation_core::validate_reference_target(&loaded_head.state, &target)
                    .map_err(ConversationStoreFailure::Transition)?;
                let active_writer = active_claim_on(&transaction, &loaded_head.state).await?;
                let current_executor_generation = executor_generation_on(&transaction).await?;
                let exact_receipt =
                    writer_receipt_on(&transaction, &loaded_head.state, run_id).await?;
                let state = match exact_receipt.as_ref() {
                    Some((claim, true)) => {
                        if active_writer.as_ref() == Some(claim)
                            || claim.message.sequence > loaded_head.state.completed_prefix
                        {
                            return Err(unavailable());
                        }
                        WriterRecoveryState::Completed
                    }
                    Some((claim, false)) => {
                        if active_writer.as_ref() != Some(claim)
                            || claim.message.sequence
                                != loaded_head.state.completed_prefix.saturating_add(1)
                        {
                            return Err(unavailable());
                        }
                        if claim.executor_generation == current_executor_generation {
                            WriterRecoveryState::ActiveCurrentGeneration
                        } else {
                            WriterRecoveryState::Interrupted
                        }
                    }
                    None => {
                        if active_writer
                            .as_ref()
                            .is_some_and(|claim| claim.link.run_id == run_id)
                        {
                            return Err(unavailable());
                        }
                        WriterRecoveryState::Absent
                    }
                };
                Ok(WriterRecoveryObservation {
                    state,
                    head: loaded_head.state,
                    writer: exact_receipt.map(|(claim, _)| claim),
                    active_writer,
                    current_executor_generation,
                })
            }
            .await;
            self.finish_conversation_core_read(guard, transaction, result)
                .await
        })
    }

    fn apply_checkpoint<'a>(
        &'a self,
        target: ConversationReference,
        checkpoint: ConversationCheckpoint,
    ) -> BoxFuture<'a, Result<(), ConversationStoreFailure>> {
        Box::pin(async move {
            let scope = self.conversation_core_scope(&target)?;
            let mut connection = self.connection().map_err(start_error)?;
            let (guard, transaction) = self
                .journal_transaction(&mut connection)
                .await
                .map_err(start_error)?;
            let result = async {
                crate::schema::ensure_conversation_core_family(&transaction)
                    .await
                    .map_err(|_| unavailable())?;
                let loaded_head = load_head_on(&transaction, scope).await?.ok_or(
                    ConversationStoreFailure::Transition(ConversationFailure::ConversationMismatch),
                )?;
                let previous_checkpoint = checkpoint_on(&transaction, scope).await?;
                let stored_prefix_digest = if checkpoint.through.sequence > 0
                    && checkpoint.through.sequence <= loaded_head.state.head_revision
                {
                    entry_on(&transaction, scope, checkpoint.through.sequence)
                        .await?
                        .filter(|entry| entry.reference == checkpoint.through)
                        .map(|entry| entry.prefix_digest)
                } else {
                    None
                };
                let next_head = floe_conversation_core::apply_checkpoint_transition(
                    &target,
                    &checkpoint,
                    CheckpointFacts {
                        head: loaded_head.state.clone(),
                        current_checkpoint_sequence: previous_checkpoint
                            .as_ref()
                            .map(|value| value.sequence),
                        stored_prefix_digest,
                    },
                )
                .map_err(ConversationStoreFailure::Transition)?;
                update_head_on(&transaction, scope, Some(&loaded_head), &next_head).await?;
                write_checkpoint_on(
                    &transaction,
                    scope,
                    previous_checkpoint.as_ref(),
                    &checkpoint,
                )
                .await
            }
            .await;
            self.finish_conversation_core_transaction(guard, transaction, result)
                .await
        })
    }
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
    use crate::RootKey;
    use floe_conversation_contract::{
        AdmissionDisposition, AdmissionTarget, AgentIdentity, AgentInstanceId, AssignmentId,
        MessageEvidenceReference, MessageOrigin,
    };
    use floe_kernel::CommandId;

    #[derive(Default)]
    struct KeyState {
        values: HashMap<(PersonId, Uuid), [u8; 32]>,
        unavailable: bool,
    }

    #[derive(Clone, Default)]
    struct TestKeys(Arc<Mutex<KeyState>>);

    impl TestKeys {
        fn set_unavailable(&self, unavailable: bool) {
            self.0.lock().expect("test key state").unavailable = unavailable;
        }
    }

    impl VaultKeyProvider for TestKeys {
        fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<RootKey, AgentFailure> {
            let state = self.0.lock().map_err(|_| AgentFailure::VaultUnavailable)?;
            if state.unavailable {
                return Err(AgentFailure::VaultUnavailable);
            }
            state
                .values
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
            let mut state = self.0.lock().map_err(|_| AgentFailure::VaultUnavailable)?;
            if state.unavailable {
                return Err(AgentFailure::VaultUnavailable);
            }
            state.values.insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("floe-core-vault-{}", Uuid::new_v4()));
            std::fs::create_dir(&path).expect("create private test root");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("restrict test root");
            Self(path)
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct Fixture {
        vault: Option<EncryptedAgentVault<TestKeys>>,
        root: TestRoot,
        keys: TestKeys,
        person_id: PersonId,
        identity: AgentIdentity,
        conversation_id: ConversationId,
        branch_id: ConversationBranchId,
        task_id: TaskId,
    }

    impl Fixture {
        async fn new() -> Self {
            let root = TestRoot::new();
            let person_id = PersonId::new();
            let keys = TestKeys::default();
            let vault = EncryptedAgentVault::create(&root.0, person_id, keys.clone())
                .await
                .expect("create encrypted test Vault");
            Self {
                vault: Some(vault),
                root,
                keys,
                person_id,
                identity: AgentIdentity {
                    person_id,
                    agent_instance_id: AgentInstanceId::new(),
                    assignment_id: AssignmentId::new(),
                    definition_id: "fixture-agent".to_owned(),
                    definition_revision: 1,
                },
                conversation_id: ConversationId::new(),
                branch_id: ConversationBranchId::new(),
                task_id: TaskId::new(),
            }
        }

        fn vault(&self) -> &EncryptedAgentVault<TestKeys> {
            self.vault.as_ref().expect("test Vault is open")
        }

        fn independent_handle(&self) -> EncryptedAgentVault<TestKeys> {
            let source = self.vault();
            EncryptedAgentVault {
                database: source.database.clone(),
                key: RootKey::from_bytes(*source.key.as_bytes()),
                keys: self.keys.clone(),
                person_id: self.person_id,
                vault_id: source.vault_id,
                unavailable: std::sync::atomic::AtomicBool::new(false),
                journal_writes: tokio::sync::Mutex::new(()),
                conversation_executor_generation: std::sync::atomic::AtomicU64::new(
                    source
                        .conversation_executor_generation
                        .load(std::sync::atomic::Ordering::Acquire),
                ),
                task_executor_generation: std::sync::atomic::AtomicU64::new(
                    source
                        .task_executor_generation
                        .load(std::sync::atomic::Ordering::Acquire),
                ),
                conversation_core_ack_loss: std::sync::atomic::AtomicBool::new(false),
                conversation_core_failure_before_pending: std::sync::atomic::AtomicBool::new(false),
                _host_lock: source._host_lock.try_clone().expect("clone test host lock"),
            }
        }

        async fn reopen(&mut self) -> Result<(), AgentFailure> {
            drop(self.vault.take());
            self.vault = Some(
                EncryptedAgentVault::open(&self.root.0, self.person_id, self.keys.clone()).await?,
            );
            Ok(())
        }

        fn new_request(&self, message: ConversationMessage) -> MessageAdmissionRequest {
            MessageAdmissionRequest {
                target: AdmissionTarget::New {
                    conversation_id: self.conversation_id,
                    branch_id: self.branch_id,
                    identity: self.identity.clone(),
                },
                message,
            }
        }

        fn continue_request(
            &self,
            head_revision: u64,
            message: ConversationMessage,
        ) -> MessageAdmissionRequest {
            MessageAdmissionRequest {
                target: AdmissionTarget::Continue {
                    reference: self.reference(head_revision),
                },
                message,
            }
        }

        fn reference(&self, head_revision: u64) -> ConversationReference {
            ConversationReference {
                conversation_id: self.conversation_id,
                branch_id: self.branch_id,
                identity: self.identity.clone(),
                head_revision,
            }
        }

        fn message(&self, text: impl Into<String>) -> ConversationMessage {
            ConversationMessage {
                message_id: MessageId::new(),
                command_id: CommandId::new(),
                origin: MessageOrigin::Person {
                    person_id: self.person_id,
                },
                text: text.into(),
                evidence: None,
                task_id: Some(self.task_id),
            }
        }

        async fn admit_first(&self, message: ConversationMessage) -> AdmissionResult {
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                self.vault(),
                self.new_request(message),
            )
            .await
            .expect("durably admit first message")
        }
    }

    fn failure_is<T: std::fmt::Debug + PartialEq>(
        result: Result<T, ConversationStoreFailure>,
        expected: ConversationFailure,
    ) {
        assert_eq!(result, Err(ConversationStoreFailure::Transition(expected)));
    }

    #[tokio::test]
    async fn encrypted_reopen_preserves_receipt_and_key_unavailability_does_not_infer_absence() {
        let mut fixture = Fixture::new().await;
        let request = fixture.new_request(fixture.message("persisted encrypted input"));
        let first = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            request.clone(),
        )
        .await
        .expect("admission commits");
        assert_eq!(first.disposition, AdmissionDisposition::Appended);

        fixture.keys.set_unavailable(true);
        drop(fixture.vault.take());
        assert_eq!(
            EncryptedAgentVault::open(&fixture.root.0, fixture.person_id, fixture.keys.clone())
                .await
                .err(),
            Some(AgentFailure::VaultUnavailable)
        );
        fixture.keys.set_unavailable(false);
        fixture
            .reopen()
            .await
            .expect("reopen with exact stored key");
        let replay = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            request,
        )
        .await
        .expect("read and replay encrypted receipt");
        assert_eq!(replay.disposition, AdmissionDisposition::Replayed);
        assert_eq!(replay.receipt, first.receipt);
    }

    #[tokio::test]
    async fn owner_command_identity_blocks_retargeting_and_changed_evidence() {
        let mut fixture = Fixture::new().await;
        let mut original_message = fixture.message("owner scoped command");
        original_message.evidence = Some(MessageEvidenceReference::from_digest([0x31; 32]));
        let original_request = fixture.new_request(original_message.clone());
        let first = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            original_request.clone(),
        )
        .await
        .expect("first command admission commits");
        assert_eq!(first.disposition, AdmissionDisposition::Appended);

        let exact_replay = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            original_request.clone(),
        )
        .await
        .expect("exact target and delivery replay before mutable revision checks");
        assert_eq!(exact_replay.disposition, AdmissionDisposition::Replayed);
        assert_eq!(exact_replay.receipt, first.receipt);

        let mut alias_message = original_message.clone();
        alias_message.command_id = CommandId::new();
        let alias_request = fixture.new_request(alias_message);
        let alias_first = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            alias_request.clone(),
        )
        .await
        .expect("same conversation-local MessageId may carry a fresh CommandId");
        assert_eq!(alias_first.disposition, AdmissionDisposition::Replayed);
        assert_eq!(alias_first.receipt, first.receipt);
        let alias_replay = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            alias_request,
        )
        .await
        .expect("each owner-wide CommandId retains its original message binding");
        assert_eq!(alias_replay.disposition, AdmissionDisposition::Replayed);
        assert_eq!(alias_replay.receipt, first.receipt);

        let retarget_conversation = MessageAdmissionRequest {
            target: AdmissionTarget::New {
                identity: fixture.identity.clone(),
                conversation_id: ConversationId::new(),
                branch_id: ConversationBranchId::new(),
            },
            message: original_message.clone(),
        };
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                retarget_conversation,
            )
            .await,
            ConversationFailure::CommandIdConflict,
        );

        let mut other_agent = fixture.identity.clone();
        other_agent.agent_instance_id = AgentInstanceId::new();
        let retarget_agent = MessageAdmissionRequest {
            target: AdmissionTarget::New {
                identity: other_agent,
                conversation_id: fixture.conversation_id,
                branch_id: fixture.branch_id,
            },
            message: original_message.clone(),
        };
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                retarget_agent,
            )
            .await,
            ConversationFailure::CommandIdConflict,
        );

        let retarget_branch = MessageAdmissionRequest {
            target: AdmissionTarget::New {
                identity: fixture.identity.clone(),
                conversation_id: fixture.conversation_id,
                branch_id: ConversationBranchId::new(),
            },
            message: original_message.clone(),
        };
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                retarget_branch,
            )
            .await,
            ConversationFailure::CommandIdConflict,
        );

        let mut changed_evidence = original_message.clone();
        changed_evidence.evidence = Some(MessageEvidenceReference::from_digest([0x32; 32]));
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                fixture.new_request(changed_evidence),
            )
            .await,
            ConversationFailure::CommandIdConflict,
        );

        fixture
            .reopen()
            .await
            .expect("owner binding survives reopen");
        let replay_after_reopen = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            original_request,
        )
        .await
        .expect("reopened Vault reads the same owner command receipt");
        assert_eq!(
            replay_after_reopen.disposition,
            AdmissionDisposition::Replayed
        );
        assert_eq!(replay_after_reopen.receipt, first.receipt);
        let page = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
            fixture.vault(),
            fixture.reference(first.receipt.head_revision),
            None,
            TranscriptPageBudget {
                max_entries: 8,
                max_bytes: 4096,
            },
        )
        .await
        .expect("retarget conflicts did not append effects");
        assert_eq!(page.entries.len(), 1);
    }

    #[tokio::test]
    async fn concurrent_owner_command_retargeting_has_one_effect_across_connections() {
        let mut fixture = Fixture::new().await;
        let second_handle = fixture.independent_handle();
        let message = fixture.message("competing owner command");
        let first_request = fixture.new_request(message.clone());
        let second_request = MessageAdmissionRequest {
            target: AdmissionTarget::New {
                identity: fixture.identity.clone(),
                conversation_id: ConversationId::new(),
                branch_id: ConversationBranchId::new(),
            },
            message: message.clone(),
        };
        let (first, second) = tokio::join!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                first_request.clone(),
            ),
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                &second_handle,
                second_request.clone(),
            ),
        );
        assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
        let (loser, retry_request) = if first.is_err() {
            (first, first_request)
        } else {
            (second, second_request)
        };
        assert!(matches!(
            loser,
            Err(ConversationStoreFailure::Busy)
                | Err(ConversationStoreFailure::Transition(
                    ConversationFailure::CommandIdConflict
                ))
        ));
        if loser == Err(ConversationStoreFailure::Busy) {
            assert_eq!(
                <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                    fixture.vault(),
                    retry_request,
                )
                .await,
                Err(ConversationStoreFailure::Transition(
                    ConversationFailure::CommandIdConflict
                ))
            );
        }

        drop(second_handle);
        fixture
            .reopen()
            .await
            .expect("validate single winning receipt");
        let connection = fixture.vault().connection().expect("test connection");
        let mut rows = connection
            .query(
                "SELECT COUNT(*) FROM agent_conversation_core_command_receipts WHERE person_id = ? AND command_id = ?",
                (
                    fixture.person_id.to_string(),
                    message.command_id.as_uuid().to_string(),
                ),
            )
            .await
            .expect("count the owner-wide receipt");
        assert_eq!(
            rows.next()
                .await
                .expect("read command count")
                .expect("command count row")
                .get::<i64>(0)
                .expect("count column"),
            1
        );
    }

    #[tokio::test]
    async fn legacy_layout_three_opens_unchanged_and_initializes_only_on_core_port_use() {
        let mut fixture = Fixture::new().await;
        let connection = fixture.vault().connection().expect("test connection");
        for table in [
            "agent_conversation_core_checkpoints",
            "agent_conversation_core_writer_receipts",
            "agent_conversation_core_active_writers",
            "agent_conversation_core_pending_inputs",
            "agent_conversation_core_command_receipts",
            "agent_conversation_core_message_receipts",
            "agent_conversation_core_entries",
            "agent_conversation_core_heads",
            "agent_conversation_core_schema",
        ] {
            connection
                .execute(&format!("DROP TABLE {table}"), ())
                .await
                .expect("remove only the additive Core family");
        }
        drop(connection);
        fixture
            .reopen()
            .await
            .expect("older layout opens without startup Core DDL");
        assert!(
            !crate::schema::conversation_core_family_present(
                &fixture.vault().connection().expect("test connection")
            )
            .await
            .expect("inspect optional family")
        );

        let result = fixture
            .admit_first(fixture.message("explicit Core-port use"))
            .await;
        assert_eq!(result.receipt.head_revision, 1);
        assert!(
            crate::schema::conversation_core_family_present(
                &fixture.vault().connection().expect("test connection")
            )
            .await
            .expect("inspect initialized family")
        );
    }

    #[tokio::test]
    async fn reopen_recomputes_the_stored_prefix_chain_without_repairing_tampering() {
        let mut fixture = Fixture::new().await;
        let admission = fixture
            .admit_first(fixture.message("prefix integrity"))
            .await;
        let connection = fixture.vault().connection().expect("test connection");
        connection
            .execute(
                "UPDATE agent_conversation_core_entries SET prefix_digest = ? WHERE person_id = ? AND conversation_id = ? AND branch_id = ? AND sequence = 1",
                (
                    "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                    fixture.person_id.to_string(),
                    fixture.conversation_id.as_uuid().to_string(),
                    fixture.branch_id.as_uuid().to_string(),
                ),
            )
            .await
            .expect("inject a validly shaped but incorrect row commitment");
        drop(connection);
        drop(fixture.vault.take());
        assert_eq!(
            EncryptedAgentVault::open(&fixture.root.0, fixture.person_id, fixture.keys.clone())
                .await
                .err(),
            Some(AgentFailure::VaultUnavailable)
        );
        assert!(
            fixture
                .root
                .0
                .join(fixture.person_id.to_string())
                .join("sessions.db")
                .exists()
        );
        assert_eq!(admission.receipt.head_revision, 1);
    }

    #[tokio::test]
    async fn encrypted_store_confines_person_agent_and_replay_evidence() {
        let fixture = Fixture::new().await;
        let initial = fixture.message("same bytes, exact delivery");
        let request = fixture.new_request(initial.clone());
        let first = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            request.clone(),
        )
        .await
        .expect("initial admission");
        assert_eq!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                request,
            )
            .await
            .expect("exact retry"),
            AdmissionResult {
                disposition: AdmissionDisposition::Replayed,
                receipt: first.receipt.clone(),
            }
        );

        let mut changed_evidence = initial.clone();
        changed_evidence.evidence = Some(MessageEvidenceReference::from_digest([0x11; 32]));
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                fixture.new_request(changed_evidence),
            )
            .await,
            ConversationFailure::CommandIdConflict,
        );
        let mut changed_origin = initial.clone();
        changed_origin.origin = MessageOrigin::Host;
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                fixture.new_request(changed_origin),
            )
            .await,
            ConversationFailure::CommandIdConflict,
        );
        let mut changed_task = initial;
        changed_task.task_id = Some(TaskId::new());
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                fixture.new_request(changed_task),
            )
            .await,
            ConversationFailure::CommandIdConflict,
        );

        let wrong_person = PersonId::new();
        let mut wrong_person_identity = fixture.identity.clone();
        wrong_person_identity.person_id = wrong_person;
        let wrong_person_request = MessageAdmissionRequest {
            target: AdmissionTarget::New {
                conversation_id: ConversationId::new(),
                branch_id: ConversationBranchId::new(),
                identity: wrong_person_identity,
            },
            message: ConversationMessage {
                message_id: MessageId::new(),
                command_id: CommandId::new(),
                origin: MessageOrigin::Person {
                    person_id: wrong_person,
                },
                text: "wrong person scope".to_owned(),
                evidence: None,
                task_id: None,
            },
        };
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                wrong_person_request,
            )
            .await,
            ConversationFailure::AgentMismatch,
        );

        let mut wrong_agent_reference = fixture.reference(first.receipt.head_revision);
        wrong_agent_reference.identity.agent_instance_id = AgentInstanceId::new();
        let wrong_agent_request = MessageAdmissionRequest {
            target: AdmissionTarget::Continue {
                reference: wrong_agent_reference,
            },
            message: fixture.message("wrong agent cannot continue"),
        };
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                wrong_agent_request,
            )
            .await,
            ConversationFailure::AgentMismatch,
        );

        let connection = fixture.vault().connection().expect("test connection");
        let mut rows = connection
            .query(
                "SELECT COUNT(*) FROM agent_conversation_core_message_receipts WHERE person_id = ? AND conversation_id = ? AND branch_id = ?",
                (
                    fixture.person_id.to_string(),
                    fixture.conversation_id.as_uuid().to_string(),
                    fixture.branch_id.as_uuid().to_string(),
                ),
            )
            .await
            .expect("count persisted receipts");
        assert_eq!(
            rows.next()
                .await
                .expect("read receipt count")
                .unwrap()
                .get::<i64>(0)
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn encrypted_pages_enforce_count_and_utf8_byte_budgets_without_skipping() {
        let fixture = Fixture::new().await;
        let mut revision = fixture.admit_first(fixture.message("π".repeat(2))).await;
        for index in 1..130 {
            let result = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                fixture.continue_request(
                    revision.receipt.head_revision,
                    fixture.message(format!("message-{index}-é")),
                ),
            )
            .await
            .expect("append independently stored transcript row");
            revision = result;
        }

        let page = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
            fixture.vault(),
            fixture.reference(revision.receipt.head_revision),
            None,
            TranscriptPageBudget {
                max_entries: 4096,
                max_bytes: MAX_TRANSCRIPT_PAGE_BYTES,
            },
        )
        .await
        .expect("first bounded transcript page");
        assert_eq!(page.entries.len(), MAX_TRANSCRIPT_PAGE_ENTRIES);
        assert!(page.has_more);
        assert_eq!(
            page.next_cursor,
            page.entries.last().map(|entry| entry.reference)
        );
        let second =
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
                fixture.vault(),
                fixture.reference(revision.receipt.head_revision),
                page.next_cursor,
                TranscriptPageBudget {
                    max_entries: MAX_TRANSCRIPT_PAGE_ENTRIES,
                    max_bytes: MAX_TRANSCRIPT_PAGE_BYTES,
                },
            )
            .await
            .expect("next transcript page");
        assert_eq!(second.entries.len(), 2);
        assert!(!second.has_more);

        let boundary = Fixture::new().await;
        let message = boundary.message("é".to_owned());
        let serialized_bytes = serde_json::to_vec(&message)
            .expect("serialize UTF-8 message")
            .len();
        let admitted = boundary.admit_first(message).await;
        let target = boundary.reference(admitted.receipt.head_revision);
        let exact = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
            boundary.vault(),
            target.clone(),
            None,
            TranscriptPageBudget {
                max_entries: 1,
                max_bytes: serialized_bytes,
            },
        )
        .await
        .expect("exact UTF-8 byte budget succeeds");
        assert_eq!(exact.encoded_bytes, serialized_bytes);
        assert_eq!(exact.entries.len(), 1);
        assert_eq!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
                boundary.vault(),
                target.clone(),
                None,
                TranscriptPageBudget {
                    max_entries: 1,
                    max_bytes: serialized_bytes - 1,
                },
            )
            .await,
            Err(ConversationStoreFailure::PageItemExceedsBudget)
        );
        let retry = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
            boundary.vault(),
            target,
            None,
            TranscriptPageBudget {
                max_entries: 1,
                max_bytes: serialized_bytes,
            },
        )
        .await
        .expect("failed item budget did not advance the cursor");
        assert_eq!(retry.next_cursor, exact.next_cursor);
    }

    #[tokio::test]
    async fn concurrent_admissions_and_claims_keep_one_writer_and_one_revision_winner() {
        let fixture = Fixture::new().await;
        let second_handle = fixture.independent_handle();
        let first = fixture.admit_first(fixture.message("first")).await;
        let expected = first.receipt.head_revision;
        let left = fixture.continue_request(expected, fixture.message("left"));
        let right = fixture.continue_request(expected, fixture.message("right"));
        let (left_result, right_result) = tokio::join!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                left.clone()
            ),
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                &second_handle,
                right.clone()
            ),
        );
        assert_eq!(
            usize::from(left_result.is_ok()) + usize::from(right_result.is_ok()),
            1
        );
        let loser = if left_result.is_err() { left } else { right };
        let winner = if left_result.is_ok() {
            left_result.unwrap()
        } else {
            right_result.unwrap()
        };
        let retried = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            &second_handle,
            fixture.continue_request(winner.receipt.head_revision, loser.message),
        )
        .await
        .expect("retry the revision loser against the committed head");
        assert_eq!(
            retried.receipt.head_revision,
            winner.receipt.head_revision + 1
        );

        let generation = fixture
            .vault()
            .activate_conversation_executor()
            .await
            .expect("activate fenced executor")
            .executor_generation;
        let target = fixture.reference(retried.receipt.head_revision);
        let run_a = RunTaskLink {
            run_id: RunId::new(),
            task_id: Some(fixture.task_id),
        };
        let run_b = RunTaskLink {
            run_id: RunId::new(),
            task_id: Some(fixture.task_id),
        };
        let (claim_a, claim_b) = tokio::join!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
                fixture.vault(),
                target.clone(),
                run_a,
                generation,
            ),
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
                &second_handle,
                target.clone(),
                run_b,
                generation,
            ),
        );
        assert_eq!(
            usize::from(claim_a.is_ok()) + usize::from(claim_b.is_ok()),
            1
        );
        let (rejected_link, rejected) = if claim_a.is_err() {
            (run_a, claim_a.unwrap_err())
        } else {
            (run_b, claim_b.unwrap_err())
        };
        assert!(matches!(
            rejected,
            ConversationStoreFailure::Busy
                | ConversationStoreFailure::Transition(ConversationFailure::WriterAlreadyActive)
        ));
        assert_eq!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
                &second_handle,
                target,
                rejected_link,
                generation,
            )
            .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::WriterAlreadyActive
            ))
        );
    }

    #[tokio::test]
    async fn rollback_and_acknowledgement_loss_are_distinguished_by_encrypted_readback() {
        let fixture = Fixture::new().await;
        let request = fixture.new_request(fixture.message("rollback boundary"));
        fixture
            .vault()
            .conversation_core_failure_before_pending
            .store(true, std::sync::atomic::Ordering::Release);
        let connection = fixture.vault().connection().expect("test connection");
        assert_eq!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                request,
            )
            .await,
            Err(ConversationStoreFailure::NotCommitted)
        );
        let mut rows = connection
            .query(
                "SELECT (SELECT COUNT(*) FROM agent_conversation_core_heads) + (SELECT COUNT(*) FROM agent_conversation_core_entries) + (SELECT COUNT(*) FROM agent_conversation_core_message_receipts)",
                (),
            )
            .await
            .expect("read rolled-back facts");
        assert_eq!(
            rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
            0
        );

        let mut fixture = Fixture::new().await;
        let request = fixture.new_request(fixture.message("commit then lose response"));
        fixture
            .vault()
            .conversation_core_ack_loss
            .store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
                fixture.vault(),
                request.clone(),
            )
            .await,
            Err(ConversationStoreFailure::OutcomeUnknown)
        );
        fixture
            .reopen()
            .await
            .expect("read back committed record after response loss");
        let replay = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            request,
        )
        .await
        .expect("retry discovers receipt rather than appending twice");
        assert_eq!(replay.disposition, AdmissionDisposition::Replayed);
        let page = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
            fixture.vault(),
            fixture.reference(replay.receipt.head_revision),
            None,
            TranscriptPageBudget {
                max_entries: 8,
                max_bytes: 4096,
            },
        )
        .await
        .expect("single committed entry remains readable");
        assert_eq!(page.entries.len(), 1);
    }

    #[tokio::test]
    async fn committed_claim_ack_loss_is_observable_after_reopen_without_reclaim() {
        let mut fixture = Fixture::new().await;
        let first = fixture.admit_first(fixture.message("claim ACK loss")).await;
        let generation = fixture
            .vault()
            .activate_conversation_executor()
            .await
            .expect("activate fenced executor")
            .executor_generation;
        let absent_run = RunId::new();
        let absent = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::observe_writer(
            fixture.vault(),
            fixture.reference(first.receipt.head_revision),
            absent_run,
        )
        .await
        .expect("successfully distinguish a missing exact receipt");
        assert_eq!(absent.state, WriterRecoveryState::Absent);
        assert_eq!(absent.writer, None);
        assert_eq!(absent.active_writer, None);
        let link = RunTaskLink {
            run_id: RunId::new(),
            task_id: Some(fixture.task_id),
        };
        fixture
            .vault()
            .conversation_core_ack_loss
            .store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
                fixture.vault(),
                fixture.reference(first.receipt.head_revision),
                link,
                generation,
            )
            .await,
            Err(ConversationStoreFailure::OutcomeUnknown)
        );

        fixture
            .reopen()
            .await
            .expect("read back a committed claim after response loss");
        let target = fixture.reference(first.receipt.head_revision);
        let observation = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::observe_writer(
            fixture.vault(),
            target.clone(),
            link.run_id,
        )
        .await
        .expect("observe exact persisted Run receipt");
        assert_eq!(
            observation.state,
            WriterRecoveryState::ActiveCurrentGeneration
        );
        assert_eq!(
            observation.writer.as_ref().map(|claim| claim.link),
            Some(link)
        );
        assert_eq!(observation.active_writer, observation.writer);
        assert_eq!(observation.head.completed_prefix, 0);
        assert_eq!(observation.current_executor_generation, generation);

        let mut wrong_agent_target = target.clone();
        wrong_agent_target.identity.agent_instance_id = AgentInstanceId::new();
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::observe_writer(
                fixture.vault(),
                wrong_agent_target,
                link.run_id,
            )
            .await,
            ConversationFailure::AgentMismatch,
        );
        let mut wrong_branch_target = target.clone();
        wrong_branch_target.branch_id = ConversationBranchId::new();
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::observe_writer(
                fixture.vault(),
                wrong_branch_target,
                link.run_id,
            )
            .await,
            ConversationFailure::ConversationMismatch,
        );
        let mut wrong_person_target = target.clone();
        wrong_person_target.identity.person_id = PersonId::new();
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::observe_writer(
                fixture.vault(),
                wrong_person_target,
                link.run_id,
            )
            .await,
            ConversationFailure::AgentMismatch,
        );

        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
                fixture.vault(),
                target.clone(),
                RunTaskLink {
                    run_id: RunId::new(),
                    task_id: Some(fixture.task_id),
                },
                generation,
            )
            .await,
            ConversationFailure::WriterAlreadyActive,
        );
        let tail = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            fixture.continue_request(
                first.receipt.head_revision,
                fixture.message("queued after readback"),
            ),
        )
        .await
        .expect("append tail behind the recovered active claim");
        assert_eq!(tail.disposition, AdmissionDisposition::Queued);
        fixture
            .reopen()
            .await
            .expect("validate the active claim and persisted queued tail");
        let page = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
            fixture.vault(),
            fixture.reference(tail.receipt.head_revision),
            None,
            TranscriptPageBudget {
                max_entries: 8,
                max_bytes: 4096,
            },
        )
        .await
        .expect("read transcript without replaying either message");
        assert_eq!(page.entries.len(), 2);
        let mut pending = fixture
            .vault()
            .connection()
            .expect("test connection")
            .query(
                "SELECT sequence FROM agent_conversation_core_pending_inputs WHERE person_id = ? AND conversation_id = ? AND branch_id = ? ORDER BY sequence",
                (
                    fixture.person_id.to_string(),
                    fixture.conversation_id.as_uuid().to_string(),
                    fixture.branch_id.as_uuid().to_string(),
                ),
            )
            .await
            .expect("read persisted queue tail");
        assert_eq!(
            pending
                .next()
                .await
                .expect("read queue row")
                .expect("tail remains queued")
                .get::<i64>(0)
                .expect("sequence column"),
            2
        );
        assert!(pending.next().await.expect("end queue").is_none());
    }

    #[tokio::test]
    async fn committed_completion_ack_loss_reads_completed_receipt_and_preserves_tail() {
        let mut fixture = Fixture::new().await;
        let first = fixture
            .admit_first(fixture.message("completion ACK loss"))
            .await;
        let generation = fixture
            .vault()
            .activate_conversation_executor()
            .await
            .expect("activate fenced executor")
            .executor_generation;
        let run_link = RunTaskLink {
            run_id: RunId::new(),
            task_id: Some(fixture.task_id),
        };
        let claim = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
            fixture.vault(),
            fixture.reference(first.receipt.head_revision),
            run_link,
            generation,
        )
        .await
        .expect("claim first input");
        let tail = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            fixture.continue_request(
                first.receipt.head_revision,
                fixture.message("tail survives completed ACK loss"),
            ),
        )
        .await
        .expect("queue work behind active writer");
        assert_eq!(tail.disposition, AdmissionDisposition::Queued);

        fixture
            .vault()
            .conversation_core_ack_loss
            .store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::complete_writer(
                fixture.vault(),
                claim.clone(),
            )
            .await,
            Err(ConversationStoreFailure::OutcomeUnknown)
        );
        fixture
            .reopen()
            .await
            .expect("completion and remaining input validate after reopen");
        let target = fixture.reference(tail.receipt.head_revision);
        let observation = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::observe_writer(
            fixture.vault(),
            target.clone(),
            run_link.run_id,
        )
        .await
        .expect("read back the exact completed receipt");
        assert_eq!(observation.state, WriterRecoveryState::Completed);
        assert_eq!(observation.writer, Some(claim.clone()));
        assert_eq!(observation.active_writer, None);
        assert_eq!(observation.head.completed_prefix, 1);

        assert_eq!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::complete_writer(
                fixture.vault(),
                claim.clone(),
            )
            .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::WrongWriter
            ))
        );
        let replay_observation =
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::observe_writer(
                fixture.vault(),
                target.clone(),
                run_link.run_id,
            )
            .await
            .expect("duplicate completion attempt leaves receipt unchanged");
        assert_eq!(replay_observation.state, WriterRecoveryState::Completed);
        assert_eq!(replay_observation.head.completed_prefix, 1);
        let mut pending = fixture
            .vault()
            .connection()
            .expect("test connection")
            .query(
                "SELECT sequence FROM agent_conversation_core_pending_inputs WHERE person_id = ? AND conversation_id = ? AND branch_id = ? ORDER BY sequence",
                (
                    fixture.person_id.to_string(),
                    fixture.conversation_id.as_uuid().to_string(),
                    fixture.branch_id.as_uuid().to_string(),
                ),
            )
            .await
            .expect("read persisted queue tail");
        assert_eq!(
            pending
                .next()
                .await
                .expect("read queue row")
                .expect("tail remains queued")
                .get::<i64>(0)
                .expect("sequence column"),
            2
        );
        assert!(pending.next().await.expect("end queue").is_none());
        let next_claim = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
            fixture.vault(),
            target,
            RunTaskLink {
                run_id: RunId::new(),
                task_id: Some(fixture.task_id),
            },
            generation,
        )
        .await
        .expect("next Run claims the preserved tail exactly once");
        assert_eq!(next_claim.message.sequence, 2);
    }

    #[tokio::test]
    async fn reopen_preserves_active_claim_and_queued_tail_rejects_stale_completion() {
        let mut fixture = Fixture::new().await;
        let first = fixture.admit_first(fixture.message("active message")).await;
        let generation = fixture
            .vault()
            .activate_conversation_executor()
            .await
            .expect("activate writer")
            .executor_generation;
        let old_claim = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
            fixture.vault(),
            fixture.reference(first.receipt.head_revision),
            RunTaskLink {
                run_id: RunId::new(),
                task_id: Some(fixture.task_id),
            },
            generation,
        )
        .await
        .expect("claim first queued message");
        let tail = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            fixture.continue_request(
                first.receipt.head_revision,
                fixture.message("durable queued tail"),
            ),
        )
        .await
        .expect("append tail while a writer is active");
        assert_eq!(tail.disposition, AdmissionDisposition::Queued);
        fixture
            .reopen()
            .await
            .expect("validate and reopen persisted claim and tail");
        let page = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
            fixture.vault(),
            fixture.reference(tail.receipt.head_revision),
            None,
            TranscriptPageBudget {
                max_entries: 8,
                max_bytes: 4096,
            },
        )
        .await
        .expect("reopened transcript includes active and queued messages");
        assert_eq!(page.entries.len(), 2);
        let next_generation = fixture
            .vault()
            .activate_conversation_executor()
            .await
            .expect("advance existing executor fence after reopen")
            .executor_generation;
        assert!(next_generation > old_claim.executor_generation);
        let interrupted = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::observe_writer(
            fixture.vault(),
            fixture.reference(tail.receipt.head_revision),
            old_claim.link.run_id,
        )
        .await
        .expect("observe stale persisted claim without releasing it");
        assert_eq!(interrupted.state, WriterRecoveryState::Interrupted);
        assert_eq!(interrupted.writer, Some(old_claim.clone()));
        assert_eq!(interrupted.active_writer, Some(old_claim.clone()));
        assert_eq!(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::complete_writer(
                fixture.vault(),
                old_claim.clone(),
            )
            .await,
            Err(ConversationStoreFailure::Transition(
                ConversationFailure::WrongWriter
            ))
        );
        assert_eq!(page.entries[1].message.text, "durable queued tail");
    }

    #[tokio::test]
    async fn completed_prefix_checkpoint_survives_reopen_and_keeps_tail_queued() {
        let mut fixture = Fixture::new().await;
        let first = fixture
            .admit_first(fixture.message("completed prefix"))
            .await;
        let generation = fixture
            .vault()
            .activate_conversation_executor()
            .await
            .expect("activate executor")
            .executor_generation;
        let claim = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
            fixture.vault(),
            fixture.reference(first.receipt.head_revision),
            RunTaskLink {
                run_id: RunId::new(),
                task_id: Some(fixture.task_id),
            },
            generation,
        )
        .await
        .expect("claim completed prefix");
        <EncryptedAgentVault<TestKeys> as ConversationStorePort>::complete_writer(
            fixture.vault(),
            claim,
        )
        .await
        .expect("complete input before checkpoint");
        let tail = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::admit(
            fixture.vault(),
            fixture.continue_request(
                first.receipt.head_revision,
                fixture.message("tail after completed prefix"),
            ),
        )
        .await
        .expect("append tail beyond completed boundary");
        assert_eq!(tail.disposition, AdmissionDisposition::Appended);
        fixture
            .reopen()
            .await
            .expect("reopen and verify stored commitment");
        let target = fixture.reference(tail.receipt.head_revision);
        let page = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::read_transcript_page(
            fixture.vault(),
            target.clone(),
            None,
            TranscriptPageBudget {
                max_entries: 8,
                max_bytes: 4096,
            },
        )
        .await
        .expect("read verified transcript prefix");
        let prefix = &page.entries[0];
        let checkpoint = ConversationCheckpoint {
            through: prefix.reference,
            prefix_digest: prefix.prefix_digest,
            summary: "completed prefix checkpoint".to_owned(),
        };
        <EncryptedAgentVault<TestKeys> as ConversationStorePort>::apply_checkpoint(
            fixture.vault(),
            target.clone(),
            checkpoint.clone(),
        )
        .await
        .expect("commit checkpoint over completed prefix");
        let mut wrong_agent_checkpoint = checkpoint.clone();
        wrong_agent_checkpoint.through.branch_id = ConversationBranchId::new();
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::apply_checkpoint(
                fixture.vault(),
                target.clone(),
                wrong_agent_checkpoint,
            )
            .await,
            ConversationFailure::CheckpointMismatch,
        );
        let mut wrong_agent_target = target.clone();
        wrong_agent_target.identity.agent_instance_id = AgentInstanceId::new();
        failure_is(
            <EncryptedAgentVault<TestKeys> as ConversationStorePort>::apply_checkpoint(
                fixture.vault(),
                wrong_agent_target,
                checkpoint,
            )
            .await,
            ConversationFailure::AgentMismatch,
        );
        fixture
            .reopen()
            .await
            .expect("checkpoint and queued tail survive reopen");
        let generation = fixture
            .vault()
            .activate_conversation_executor()
            .await
            .expect("activate next writer");
        let claim = <EncryptedAgentVault<TestKeys> as ConversationStorePort>::claim_writer(
            fixture.vault(),
            target,
            RunTaskLink {
                run_id: RunId::new(),
                task_id: Some(fixture.task_id),
            },
            generation.executor_generation,
        )
        .await
        .expect("checkpoint left queued tail claimable");
        assert_eq!(claim.message.sequence, 2);
    }
}
