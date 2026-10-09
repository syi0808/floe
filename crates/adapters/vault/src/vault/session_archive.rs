use super::database_failure;
use floe_agent_contract::AgentFailure;
use floe_agent_contract::{ArchiveReadRequest, ArchiveSnapshot, ArchivedMessage};
use floe_conversation::{AgentMessage as ManagerMessage, AgentSession, SessionRecoveryPointer};
use floe_conversation_contract::{ConversationBranchId, ConversationCheckpoint, ConversationId};
use floe_conversation_core::{ConversationReadTarget, ConversationStoreFailure};
use serde::{Deserialize, Serialize};
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

use super::context_dependencies::{
    hydrate_context_dependency_coverage_on, merge_context_dependency_coverage,
    preflight_context_dependency_coverage_on, read_context_dependency_coverage,
};
use super::conversation_core::{
    Scope, checkpoint_on, entry_on, hex_digest, load_head_on, owner_error, require_core_v3_on,
};
use super::owner_custody::{
    manager_session_alias_on, manager_session_binding_for_session_on,
    manager_session_message_alias, owner_input_binding_on, table_present_on,
};
use super::*;
use floe_kernel::RunId;

const MAX_COMPACTION_SUMMARY_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ManagerArchiveManifest {
    pub(super) person_id: floe_kernel::PersonId,
    pub(super) session_id: Uuid,
    pub(super) archive_id: Uuid,
    pub(super) source_revision: u64,
    pub(super) through_turn_id: Uuid,
    pub(super) summary_alias_id: Uuid,
    pub(super) message_count: usize,
    pub(super) conversation_id: ConversationId,
    pub(super) branch_id: ConversationBranchId,
    pub(super) start_sequence: u64,
    pub(super) through_sequence: u64,
    pub(super) through_message_id: Uuid,
    pub(super) prefix_digest: [u8; 32],
    pub(super) previous_archive_id: Option<Uuid>,
    pub(super) summary: String,
}

impl ManagerArchiveManifest {
    pub(super) fn recovery(&self) -> SessionRecoveryPointer {
        SessionRecoveryPointer {
            archive_id: self.archive_id,
            source_revision: self.source_revision,
            through_turn_id: self.through_turn_id,
            archived_message_count: self.message_count,
        }
    }

    pub(super) fn checkpoint(&self) -> Result<ConversationCheckpoint, AgentFailure> {
        Ok(ConversationCheckpoint {
            through: floe_conversation_contract::TranscriptReference {
                conversation_id: self.conversation_id,
                branch_id: self.branch_id,
                message_id: floe_conversation_contract::MessageId::from_uuid(
                    self.through_message_id,
                )
                .ok_or(AgentFailure::StorageUnavailable)?,
                sequence: self.through_sequence,
            },
            prefix_digest: self.prefix_digest,
            summary: self.summary.clone(),
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum ManagerArchiveLookup {
    Id(Uuid),
    Checkpoint(u64),
    SourceRevision(u64),
}

pub(super) async fn manager_archive_manifest_on(
    transaction: &Transaction<'_>,
    person_id: floe_kernel::PersonId,
    session_id: Uuid,
    lookup: ManagerArchiveLookup,
) -> Result<Option<ManagerArchiveManifest>, AgentFailure> {
    let columns = "SELECT archive_id, source_revision, through_turn_id, summary_alias_id, message_count, conversation_id, branch_id, start_sequence, through_sequence, through_message_id, prefix_digest, previous_archive_id, summary FROM agent_conversation_manager_archives_v3 WHERE person_id = ? AND session_id = ? AND ";
    let (sql, selector) = match lookup {
        ManagerArchiveLookup::Id(id) => {
            (format!("{columns}archive_id = ? LIMIT 2"), id.to_string())
        }
        ManagerArchiveLookup::Checkpoint(sequence) => (
            format!("{columns}through_sequence = ? LIMIT 2"),
            i64::try_from(sequence)
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .to_string(),
        ),
        ManagerArchiveLookup::SourceRevision(revision) => (
            format!("{columns}source_revision = ? LIMIT 2"),
            i64::try_from(revision)
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .to_string(),
        ),
    };
    let mut rows = transaction
        .query(
            &sql,
            (person_id.to_string(), session_id.to_string(), selector),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    let manifest = parse_manager_archive_row(person_id, session_id, &row)?;
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(Some(manifest))
}

fn parse_manager_archive_row(
    person_id: floe_kernel::PersonId,
    session_id: Uuid,
    row: &turso::Row,
) -> Result<ManagerArchiveManifest, AgentFailure> {
    let parse_uuid = |index| {
        Uuid::parse_str(&row.get::<String>(index).map_err(storage)?)
            .map_err(|_| AgentFailure::StorageUnavailable)
    };
    let archive_id = parse_uuid(0)?;
    let source_revision = u64::try_from(row.get::<i64>(1).map_err(storage)?)
        .ok()
        .filter(|revision| *revision > 0)
        .ok_or(AgentFailure::StorageUnavailable)?;
    let through_turn_id = parse_uuid(2)?;
    let summary_alias_id = parse_uuid(3)?;
    let message_count = usize::try_from(row.get::<i64>(4).map_err(storage)?)
        .ok()
        .filter(|count| *count > 0 && *count <= floe_agent_contract::MAX_AGENT_MESSAGES)
        .ok_or(AgentFailure::StorageUnavailable)?;
    let conversation_id =
        ConversationId::from_uuid(parse_uuid(5)?).ok_or(AgentFailure::StorageUnavailable)?;
    let branch_id =
        ConversationBranchId::from_uuid(parse_uuid(6)?).ok_or(AgentFailure::StorageUnavailable)?;
    let start_sequence = u64::try_from(row.get::<i64>(7).map_err(storage)?)
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    let through_sequence = u64::try_from(row.get::<i64>(8).map_err(storage)?)
        .ok()
        .filter(|sequence| *sequence > start_sequence)
        .ok_or(AgentFailure::StorageUnavailable)?;
    let through_message_id = parse_uuid(9)?;
    let prefix_digest =
        super::conversation_core::parse_digest(&row.get::<String>(10).map_err(storage)?)
            .map_err(|_| AgentFailure::StorageUnavailable)?;
    let previous_archive_id = row
        .get::<Option<String>>(11)
        .map_err(storage)?
        .map(|value| Uuid::parse_str(&value).map_err(|_| AgentFailure::StorageUnavailable))
        .transpose()?;
    let summary = row.get::<String>(12).map_err(storage)?;
    let message_count_expected = usize::try_from(through_sequence - start_sequence)
        .ok()
        .and_then(|count| count.checked_add(usize::from(previous_archive_id.is_some())))
        .ok_or(AgentFailure::StorageUnavailable)?;
    if archive_id.is_nil()
        || through_turn_id.is_nil()
        || summary_alias_id.is_nil()
        || through_message_id.is_nil()
        || summary.is_empty()
        || summary.len() > MAX_COMPACTION_SUMMARY_BYTES
        || message_count != message_count_expected
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(ManagerArchiveManifest {
        person_id,
        session_id,
        archive_id,
        source_revision,
        through_turn_id,
        summary_alias_id,
        message_count,
        conversation_id,
        branch_id,
        start_sequence,
        through_sequence,
        through_message_id,
        prefix_digest,
        previous_archive_id,
        summary,
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCompactionResult {
    pub session: AgentSession,
    pub recovery: SessionRecoveryPointer,
    pub summary_coverage: floe_access::DependencyCoverage,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Check bounded Session headers without hydrating their transcript vector.
    pub(super) async fn validate_session_records(&self) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT id, revision, json_extract(payload, '$.id'), json_extract(payload, '$.person_id'), json_extract(payload, '$.revision'), json_extract(payload, '$.schema_version'), json_type(payload, '$.messages') FROM agent_sessions",
                (),
            )
            .await
            .map_err(database_failure)?;
        while let Some(row) = rows.next().await.map_err(database_failure)? {
            let id = row.get::<String>(0).map_err(storage)?;
            let revision = row.get::<i64>(1).map_err(storage)?;
            let payload_id = row.get::<String>(2).map_err(storage)?;
            let person_id = row.get::<String>(3).map_err(storage)?;
            let payload_revision = row.get::<i64>(4).map_err(storage)?;
            let schema_version = row.get::<i64>(5).map_err(storage)?;
            let messages_type = row.get::<String>(6).map_err(storage)?;
            if Uuid::parse_str(&id).is_err()
                || id != payload_id
                || Uuid::parse_str(&person_id).is_err()
                || revision < 0
                || revision != payload_revision
                || schema_version != i64::from(floe_agent_contract::AGENT_VERSION)
                || messages_type != "array"
            {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
        drop(rows);
        Ok(())
    }

    /// Compact a Manager history prefix by committing a Core checkpoint and
    /// an immutable bounded archive manifest alongside the Session CAS. The
    /// Core payloads remain physically retained in this milestone.
    pub async fn compact_manager_session(
        &self,
        session_id: Uuid,
        expected_revision: u64,
        through_turn_id: Uuid,
        summary: String,
    ) -> Result<SessionCompactionResult, AgentFailure> {
        let summary = summary.trim().to_owned();
        if summary.is_empty() || summary.len() > MAX_COMPACTION_SUMMARY_BYTES {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let (guard, transaction) = self.journal_transaction(&mut connection).await?;
        let result = async {
            let mut session = self
                .manager_session_shell_on(&transaction, session_id)
                .await
                .map_err(super::conversation_core::owner_store_failure)?
                .ok_or(AgentFailure::UnsupportedVersion)?;
            let binding = manager_session_binding_for_session_on(
                &transaction,
                self.person_id,
                session_id,
            )
            .await
            .map_err(super::conversation_core::owner_store_failure)?
            .ok_or(AgentFailure::UnsupportedVersion)?;

            if let Some(existing) = manager_archive_manifest_on(
                &transaction,
                self.person_id,
                session_id,
                ManagerArchiveLookup::SourceRevision(expected_revision),
            )
            .await?
            {
                if existing.through_turn_id != through_turn_id || existing.summary != summary {
                    return Err(AgentFailure::Conflict);
                }
                session.revision = expected_revision
                    .checked_add(1)
                    .ok_or(AgentFailure::Conflict)?;
                let summary_coverage = read_context_dependency_coverage(
                    &transaction,
                    self.person_id,
                    session_id,
                    existing.through_turn_id,
                )
                .await?;
                return Ok(SessionCompactionResult {
                    session,
                    recovery: existing.recovery(),
                    summary_coverage,
                });
            }
            if session.revision != expected_revision
                || session.active_turn.is_some()
                || session.pending_output.is_some()
            {
                return Err(AgentFailure::Conflict);
            }

            if let Some(marker) = session.continuation {
                let current_run = RunId::from_uuid(marker.turn_id)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let run = self
                    .conversation_run_on(&transaction, current_run)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if run.person_id != self.person_id
                    || run.session_id != session_id
                    || run.session_revision != session.revision
                    || run.continuation_level != marker.level
                    || !matches!(run.state, floe_conversation::RunState::Completed | floe_conversation::RunState::Blocked)
                    || floe_conversation::project_run_receipt(run)?.continuation().is_none()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                return Err(AgentFailure::Conflict);
            }

            let mut pending_resume = transaction
                .query(
                    "SELECT 1 FROM agent_conversation_resume_requests WHERE person_id = ? AND session_id = ? AND state = 'pending' LIMIT 1",
                    (self.person_id.to_string(), session_id.to_string()),
                )
                .await
                .map_err(database_failure)?;
            if pending_resume.next().await.map_err(database_failure)?.is_some() {
                return Err(AgentFailure::Conflict);
            }
            drop(pending_resume);

            if table_present_on(&transaction, "agent_conversation_interactions")
                .await
                .map_err(super::conversation_core::owner_store_failure)?
            {
                let mut origins = transaction
                    .query(
                        "SELECT DISTINCT origin_run_id FROM agent_conversation_interactions INDEXED BY agent_conversation_interactions_session WHERE person_id = ? AND session_id = ? AND state IN ('pending', 'resolving') LIMIT 65",
                        (self.person_id.to_string(), session_id.to_string()),
                    )
                    .await
                    .map_err(database_failure)?;
                let mut live_open_interaction = false;
                let mut checked = 0usize;
                while let Some(row) = origins.next().await.map_err(database_failure)? {
                    if checked == 64 {
                        return Err(AgentFailure::BudgetExceeded);
                    }
                    checked += 1;
                    let value = row.get::<String>(0).map_err(storage)?;
                    let id = Uuid::parse_str(&value).map_err(unavailable)?;
                    let origin_id = RunId::from_uuid(id).ok_or(AgentFailure::StorageUnavailable)?;
                    let origin = self
                        .conversation_run_on(&transaction, origin_id)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    if origin.person_id != self.person_id || origin.session_id != session_id {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    if origin.session_revision == session.revision {
                        if !matches!(origin.state, floe_conversation::RunState::Completed | floe_conversation::RunState::Blocked)
                            || floe_conversation::project_run_receipt(origin)?.resume().is_none()
                        {
                            return Err(AgentFailure::StorageUnavailable);
                        }
                        live_open_interaction = true;
                    }
                }
                if live_open_interaction {
                    return Err(AgentFailure::Conflict);
                }
            }

            let scope = Scope::from_identity(
                self.person_id,
                &binding.identity,
                binding.conversation_id,
                binding.branch_id,
            );
            let head = load_head_on(&transaction, scope)
                .await
                .map_err(super::conversation_core::owner_store_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if super::conversation_core::active_recorder_on(&transaction, scope)
                .await
                .map_err(super::conversation_core::owner_store_failure)?
                .is_some()
            {
                return Err(AgentFailure::Conflict);
            }
            let prior_checkpoint = checkpoint_on(&transaction, scope)
                .await
                .map_err(super::conversation_core::owner_store_failure)?;
            let previous_manifest = if let Some(checkpoint) = &prior_checkpoint {
                let manifest = manager_archive_manifest_on(
                    &transaction,
                    self.person_id,
                    session_id,
                    ManagerArchiveLookup::Checkpoint(checkpoint.through.sequence),
                )
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
                if manifest.checkpoint().map_err(|_| AgentFailure::StorageUnavailable)? != *checkpoint
                    || manifest.conversation_id != binding.conversation_id
                    || manifest.branch_id != binding.branch_id
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Some(manifest)
            } else {
                None
            };
            let start_sequence = prior_checkpoint
                .as_ref()
                .map_or(0, |checkpoint| checkpoint.through.sequence);

            let turn_id = RunId::from_uuid(through_turn_id)
                .ok_or(AgentFailure::NotFound)?;
            let run = self
                .conversation_run_on(&transaction, turn_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if run.person_id != self.person_id
                || run.session_id != session_id
                || !matches!(run.state, floe_conversation::RunState::Completed | floe_conversation::RunState::Blocked | floe_conversation::RunState::Failed | floe_conversation::RunState::Cancelled | floe_conversation::RunState::TimedOut | floe_conversation::RunState::Interrupted)
                || run.pending_terminal.is_some()
            {
                return Err(AgentFailure::Conflict);
            }
            let run_binding = owner_input_binding_on(&transaction, self.person_id, run.run_id)
                .await
                .map_err(super::conversation_core::owner_store_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?;
            let (owner, _, terminal_digest) = self
                .verified_owner_evidence_on(
                    &transaction,
                    run.run_id,
                    &binding.identity,
                    binding.conversation_id,
                    binding.branch_id,
                    run_binding.input,
                    run_binding.executor_domain,
                    run_binding.executor_generation,
                )
                .await
                .map_err(super::conversation_core::owner_store_failure)?;
            if owner.state != floe_conversation_core::OwnerRunState::Terminal
                || !owner.unresolved_effects.is_empty()
                || terminal_digest.is_none()
            {
                return Err(AgentFailure::Conflict);
            }

            let mut target_sequences = Vec::new();
            let mut output_rows = transaction
                .query(
                    "SELECT MAX(sequence) FROM agent_conversation_owner_transcript_evidence_v1 WHERE person_id = ? AND session_id = ? AND first_recording_run_id = ?",
                    (self.person_id.to_string(), session_id.to_string(), through_turn_id.to_string()),
                )
                .await
                .map_err(database_failure)?;
            if let Some(row) = output_rows.next().await.map_err(database_failure)?
                && let Some(sequence) = row.get::<Option<i64>>(0).map_err(storage)?
            {
                target_sequences.push(u64::try_from(sequence).map_err(unavailable)?);
            }
            drop(output_rows);
            let mut input_rows = transaction
                .query(
                    "SELECT input_sequence FROM agent_conversation_owner_transcript_inputs_v1 WHERE person_id = ? AND session_id = ? AND original_owner_run_id = ? LIMIT 2",
                    (self.person_id.to_string(), session_id.to_string(), through_turn_id.to_string()),
                )
                .await
                .map_err(database_failure)?;
            if let Some(row) = input_rows.next().await.map_err(database_failure)? {
                target_sequences.push(
                    u64::try_from(row.get::<i64>(0).map_err(storage)?)
                        .map_err(|_| AgentFailure::StorageUnavailable)?,
                );
                if input_rows.next().await.map_err(database_failure)?.is_some() {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            drop(input_rows);
            let through_sequence = target_sequences
                .into_iter()
                .max()
                .filter(|sequence| *sequence > start_sequence)
                .ok_or(AgentFailure::NotFound)?;
            if through_sequence > head.state.settled_prefix
                || through_sequence > head.state.head_revision
            {
                return Err(AgentFailure::Conflict);
            }
            let through_entry = entry_on(&transaction, scope, through_sequence)
                .await
                .map_err(super::conversation_core::owner_store_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?;
            let target = ConversationReadTarget {
                identity: binding.identity.clone(),
                conversation_id: binding.conversation_id,
                branch_id: binding.branch_id,
            };
            let mut newly_archived = Vec::new();
            let mut projected_bytes = 2usize;
            let mut summary_coverage = if let Some(manifest) = &previous_manifest {
                let coverage_header = preflight_context_dependency_coverage_on(
                    &transaction,
                    self.person_id,
                    session_id,
                    manifest.through_turn_id,
                )
                .await?;
                let summary = ManagerMessage::Compaction {
                    turn_id: manifest.through_turn_id,
                    summary: manifest.summary.clone(),
                    recovery: manifest.recovery(),
                };
                let summary_total_bytes = serde_json::to_vec(&summary)
                    .map_err(storage)?
                    .len()
                    .checked_add(coverage_header.accounted_bytes()?)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                projected_bytes = projected_bytes
                    .checked_add(summary_total_bytes)
                    .filter(|bytes| {
                        *bytes <= floe_agent_contract::MAX_ARCHIVE_PROJECTION_BYTES
                    })
                    .ok_or(AgentFailure::BudgetExceeded)?;
                Some(
                    hydrate_context_dependency_coverage_on(
                        &transaction,
                        self.person_id,
                        session_id,
                        manifest.through_turn_id,
                        coverage_header,
                        #[cfg(test)]
                        &self.context_coverage_payload_hydrations,
                    )
                    .await?,
                )
            } else {
                None
            };
            for sequence in (start_sequence + 1)..=through_sequence {
                let entry = entry_on(&transaction, scope, sequence)
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                self.validate_producing_task_reference_on(&transaction, &entry)
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?;
                let preflight = self
                    .preflight_owner_entry_on(
                        &transaction,
                        &target,
                        session_id,
                        &entry,
                        scope,
                    )
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?;
                let item_bytes = preflight.encoded_bytes();
                let next_projected_bytes = projected_bytes
                    .checked_add(item_bytes)
                    .filter(|bytes| {
                        item_bytes <= floe_agent_contract::MAX_ARCHIVE_PROJECTION_BYTES
                            && *bytes <= floe_agent_contract::MAX_ARCHIVE_PROJECTION_BYTES
                    })
                    .ok_or(AgentFailure::BudgetExceeded)?;
                let archived_so_far = newly_archived
                    .len()
                    .checked_add(usize::from(previous_manifest.is_some()))
                    .ok_or(AgentFailure::BudgetExceeded)?;
                if archived_so_far >= floe_agent_contract::MAX_ARCHIVE_PROJECTION_MESSAGES {
                    return Err(AgentFailure::BudgetExceeded);
                }
                let message = self
                    .resolve_manager_transcript_entry_preflighted_on(
                        &transaction,
                        session_id,
                        entry,
                        preflight,
                    )
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?;
                if message.encoded_bytes != item_bytes {
                    return Err(AgentFailure::StorageUnavailable);
                }
                projected_bytes = next_projected_bytes;
                summary_coverage = Some(match summary_coverage {
                    Some(existing) => existing
                        .merge(&message.coverage)
                        .map_err(|_| AgentFailure::StorageUnavailable)?,
                    None => message.coverage.clone(),
                });
                newly_archived.push(message);
            }
            let through_message = newly_archived
                .last()
                .ok_or(AgentFailure::NotFound)?;
            if through_message.message.turn_id() != through_turn_id {
                return Err(AgentFailure::NotFound);
            }
            let archived_message_count = newly_archived
                .len()
                .checked_add(usize::from(previous_manifest.is_some()))
                .ok_or(AgentFailure::BudgetExceeded)?;
            if archived_message_count > floe_agent_contract::MAX_ARCHIVE_PROJECTION_MESSAGES {
                return Err(AgentFailure::BudgetExceeded);
            }
            let summary_coverage = summary_coverage.ok_or(AgentFailure::StorageUnavailable)?;
            let recovery = SessionRecoveryPointer {
                archive_id: Uuid::new_v5(
                    &session_id,
                    format!(
                        "floe.manager.archive.v3:{}:{}:{}",
                        expected_revision,
                        through_sequence,
                        hex_digest(through_entry.prefix_digest),
                    )
                    .as_bytes(),
                ),
                source_revision: expected_revision,
                through_turn_id,
                archived_message_count,
            };
            if recovery.archive_id.is_nil() {
                return Err(AgentFailure::StorageUnavailable);
            }
            let summary_message = ManagerMessage::Compaction {
                turn_id: through_turn_id,
                summary: summary.clone(),
                recovery: recovery.clone(),
            };
            let summary_alias_id = manager_session_message_alias(&summary_message);
            if summary_alias_id.is_nil()
                || manager_session_alias_on(
                    &transaction,
                    self.person_id,
                    session_id,
                    summary_alias_id,
                )
                .await
                .map_err(super::conversation_core::owner_store_failure)?
                .is_some()
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            let manifest = ManagerArchiveManifest {
                person_id: self.person_id,
                session_id,
                archive_id: recovery.archive_id,
                source_revision: expected_revision,
                through_turn_id,
                summary_alias_id,
                message_count: archived_message_count,
                conversation_id: binding.conversation_id,
                branch_id: binding.branch_id,
                start_sequence,
                through_sequence,
                through_message_id: through_entry.reference.message_id.as_uuid(),
                prefix_digest: through_entry.prefix_digest,
                previous_archive_id: previous_manifest.as_ref().map(|manifest| manifest.archive_id),
                summary: summary.clone(),
            };
            let checkpoint = ConversationCheckpoint {
                through: through_entry.reference,
                prefix_digest: through_entry.prefix_digest,
                summary: summary.clone(),
            };
            self.apply_conversation_checkpoint_on(
                &transaction,
                head.state.reference(),
                checkpoint,
            )
            .await
            .map_err(super::conversation_core::owner_store_failure)?;
            transaction
                .execute(
                    "INSERT INTO agent_conversation_manager_archives_v3 (person_id, session_id, archive_id, source_revision, through_turn_id, summary_alias_id, message_count, conversation_id, branch_id, start_sequence, through_sequence, through_message_id, prefix_digest, previous_archive_id, summary) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        self.person_id.to_string(),
                        session_id.to_string(),
                        manifest.archive_id.to_string(),
                        integer(manifest.source_revision)?,
                        manifest.through_turn_id.to_string(),
                        manifest.summary_alias_id.to_string(),
                        i64::try_from(manifest.message_count).map_err(|_| AgentFailure::BudgetExceeded)?,
                        manifest.conversation_id.as_uuid().to_string(),
                        manifest.branch_id.as_uuid().to_string(),
                        integer(manifest.start_sequence)?,
                        integer(manifest.through_sequence)?,
                        manifest.through_message_id.to_string(),
                        hex_digest(manifest.prefix_digest),
                        manifest.previous_archive_id.map(|id| id.to_string()),
                        manifest.summary.clone(),
                    ),
                )
                .await
                .map_err(database_failure)?;
            merge_context_dependency_coverage(
                &transaction,
                self.person_id,
                session_id,
                through_turn_id,
                summary_coverage.clone(),
            )
            .await?;
            session.revision = session.revision.checked_add(1).ok_or(AgentFailure::Conflict)?;
            let payload = self.payload(&session)?;
            let changed = transaction
                .execute(
                    "UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                    (
                        integer(session.revision)?,
                        payload,
                        session_id.to_string(),
                        integer(expected_revision)?,
                    ),
                )
                .await
                .map_err(database_failure)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            Ok(SessionCompactionResult {
                session,
                recovery,
                summary_coverage,
            })
        }
        .await;
        let result = self
            .finish_conversation_core_transaction(
                guard,
                transaction,
                result.map_err(|failure| match failure {
                    AgentFailure::BudgetExceeded => ConversationStoreFailure::PageItemExceedsBudget,
                    failure => owner_error(failure),
                }),
            )
            .await
            .map_err(super::conversation_core::owner_store_failure)?;
        self.check_access()?;
        Ok(result)
    }

    /// Read one immutable Manager archive by resolving its exact prefix from
    /// Core. No Session vector or SessionArchive payload is loaded.
    pub async fn read_manager_session_archive(
        &self,
        request: &ArchiveReadRequest,
    ) -> Result<ArchiveSnapshot, AgentFailure> {
        request.validate()?;
        if request.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let result = async {
            require_core_v3_on(&transaction)
                .await
                .map_err(super::conversation_core::owner_store_failure)?;
            let binding = manager_session_binding_for_session_on(
                &transaction,
                self.person_id,
                request.session_id,
            )
            .await
            .map_err(super::conversation_core::owner_store_failure)?
            .ok_or(AgentFailure::UnsupportedVersion)?;
            let manifest = manager_archive_manifest_on(
                &transaction,
                self.person_id,
                request.session_id,
                ManagerArchiveLookup::Id(request.pointer.archive_id),
            )
            .await?
            .ok_or(AgentFailure::NotFound)?;
            if manifest.recovery().source_revision != request.pointer.source_revision
                || manifest.recovery().through_turn_id != request.pointer.through_turn_id
                || manifest.recovery().archived_message_count
                    != request.pointer.archived_message_count
                || manifest.conversation_id != binding.conversation_id
                || manifest.branch_id != binding.branch_id
            {
                return Err(AgentFailure::Conflict);
            }
            if manifest.message_count > request.max_messages {
                return Err(AgentFailure::BudgetExceeded);
            }
            let target = ConversationReadTarget {
                identity: binding.identity.clone(),
                conversation_id: binding.conversation_id,
                branch_id: binding.branch_id,
            };
            let mut messages = Vec::with_capacity(manifest.message_count);
            let mut encoded_bytes = 2usize;
            let scope = Scope::from_identity(
                self.person_id,
                &binding.identity,
                binding.conversation_id,
                binding.branch_id,
            );
            if let Some(previous_archive_id) = manifest.previous_archive_id {
                let previous = manager_archive_manifest_on(
                    &transaction,
                    self.person_id,
                    request.session_id,
                    ManagerArchiveLookup::Id(previous_archive_id),
                )
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
                if previous.through_sequence != manifest.start_sequence
                    || previous.conversation_id != manifest.conversation_id
                    || previous.branch_id != manifest.branch_id
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let coverage_header = preflight_context_dependency_coverage_on(
                    &transaction,
                    self.person_id,
                    request.session_id,
                    previous.through_turn_id,
                )
                .await?;
                let summary = ManagerMessage::Compaction {
                    turn_id: previous.through_turn_id,
                    summary: previous.summary.clone(),
                    recovery: previous.recovery(),
                };
                let message_id = previous.summary_alias_id;
                let encoded = serde_json::to_vec(&summary).map_err(storage)?.len();
                let summary_bytes = encoded
                    .checked_add(coverage_header.accounted_bytes()?)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                let next_bytes = encoded_bytes
                    .checked_add(summary_bytes)
                    .filter(|bytes| *bytes <= request.max_bytes)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                let coverage = hydrate_context_dependency_coverage_on(
                    &transaction,
                    self.person_id,
                    request.session_id,
                    previous.through_turn_id,
                    coverage_header,
                    #[cfg(test)]
                    &self.context_coverage_payload_hydrations,
                )
                .await?;
                if coverage
                    .as_persisted_bytes()
                    .map_err(|_| AgentFailure::StorageUnavailable)?
                    .len()
                    > coverage_header.accounted_bytes()?
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                if next_bytes > request.max_bytes {
                    return Err(AgentFailure::BudgetExceeded);
                }
                encoded_bytes = next_bytes;
                messages.push(ArchivedMessage {
                    turn_id: previous.through_turn_id,
                    message: floe_conversation::contract_message(&summary, message_id, coverage)?,
                });
            }
            for sequence in (manifest.start_sequence + 1)..=manifest.through_sequence {
                if messages.len() >= request.max_messages {
                    return Err(AgentFailure::BudgetExceeded);
                }
                let entry = entry_on(&transaction, scope, sequence)
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                self.validate_producing_task_reference_on(&transaction, &entry)
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?;
                let preflight = self
                    .preflight_owner_entry_on(
                        &transaction,
                        &target,
                        request.session_id,
                        &entry,
                        scope,
                    )
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?;
                let item_bytes = preflight.encoded_bytes();
                let next_bytes = encoded_bytes
                    .checked_add(item_bytes)
                    .filter(|bytes| *bytes <= request.max_bytes)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                let resolved = self
                    .resolve_manager_transcript_entry_preflighted_on(
                        &transaction,
                        request.session_id,
                        entry,
                        preflight,
                    )
                    .await
                    .map_err(super::conversation_core::owner_store_failure)?;
                if resolved.encoded_bytes != item_bytes {
                    return Err(AgentFailure::StorageUnavailable);
                }
                encoded_bytes = next_bytes;
                let turn_id = resolved.message.turn_id();
                let message = floe_conversation::contract_message(
                    &resolved.message,
                    resolved.alias_id,
                    resolved.coverage,
                )?;
                messages.push(ArchivedMessage { turn_id, message });
            }
            if messages.len() != manifest.message_count
                || messages.last().map(|message| message.turn_id) != Some(manifest.through_turn_id)
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            let through = entry_on(&transaction, scope, manifest.through_sequence)
                .await
                .map_err(super::conversation_core::owner_store_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if through.reference.message_id.as_uuid() != manifest.through_message_id
                || through.prefix_digest != manifest.prefix_digest
            {
                return Err(AgentFailure::StorageUnavailable);
            }
            Ok(ArchiveSnapshot {
                person_id: self.person_id,
                session_id: request.session_id,
                pointer: request.pointer.clone(),
                messages,
            })
        }
        .await;
        match result {
            Ok(snapshot) => {
                transaction.commit().await.map_err(database_failure)?;
                self.check_access()?;
                snapshot.validate(request)?;
                Ok(snapshot)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::BudgetExceeded)
}
