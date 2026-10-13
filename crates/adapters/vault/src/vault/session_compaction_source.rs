//! Bounded source selection for Conversation-owned semantic compaction.
use super::context_dependencies::{
    hydrate_context_dependency_coverage_on, preflight_context_dependency_coverage_on,
};
use super::conversation_core::{Scope, entry_on, owner_store_failure};
use super::owner_custody::manager_session_binding_for_session_on;
use super::*;
use floe_conversation::{
    COMPACTION_MIN_RETAIN_ENTRIES, COMPACTION_PREFERRED_RETAIN_ENTRIES, COMPACTION_TRIGGER_BYTES,
    COMPACTION_TRIGGER_MESSAGES, CompactionSource,
};
use floe_conversation_core::ConversationReadTarget;
use turso::transaction::TransactionBehavior;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn prepare_manager_compaction_source(
        &self,
        session_id: Uuid,
        expected_revision: u64,
        byte_limit: usize,
    ) -> Result<Option<CompactionSource>, AgentFailure> {
        if session_id.is_nil()
            || expected_revision == 0
            || byte_limit < 2
            || byte_limit > COMPACTION_TRIGGER_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let result = self
            .prepare_manager_compaction_source_on(
                &transaction,
                session_id,
                expected_revision,
                byte_limit,
            )
            .await;
        match result {
            Ok(source) => {
                transaction.commit().await.map_err(database_failure)?;
                self.check_access()?;
                Ok(source)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }

    /// The caller owns the transaction, so durable job admission can pin this
    /// exact source and its execution journal atomically. No model or external
    /// authorization work may run inside this transaction.
    pub(super) async fn prepare_manager_compaction_source_on(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        session_id: Uuid,
        expected_revision: u64,
        byte_limit: usize,
    ) -> Result<Option<CompactionSource>, AgentFailure> {
        if session_id.is_nil()
            || expected_revision == 0
            || byte_limit < 2
            || byte_limit > COMPACTION_TRIGGER_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        let session = self
            .manager_session_shell_on(transaction, session_id)
            .await
            .map_err(owner_store_failure)?
            .ok_or(AgentFailure::NotFound)?;
        let binding =
            manager_session_binding_for_session_on(transaction, self.person_id, session_id)
                .await
                .map_err(owner_store_failure)?
                .ok_or(AgentFailure::NotFound)?;
        let (head, checkpoint, previous) = self
            .manager_compaction_eligibility_on(transaction, &session, &binding, expected_revision)
            .await?;
        let start = checkpoint
            .as_ref()
            .map_or(0, |checkpoint| checkpoint.through.sequence);
        let available = head.state.head_revision.saturating_sub(start);
        if available <= COMPACTION_MIN_RETAIN_ENTRIES {
            return Ok(None);
        }
        let mut pressure = available.saturating_add(u64::from(previous.is_some()))
            >= COMPACTION_TRIGGER_MESSAGES as u64;
        let limit = head
            .state
            .settled_prefix
            .min(start.saturating_add(COMPACTION_TRIGGER_MESSAGES as u64));
        let scope = Scope::from_identity(
            self.person_id,
            &binding.identity,
            binding.conversation_id,
            binding.branch_id,
        );
        let target = ConversationReadTarget {
            identity: binding.identity.clone(),
            conversation_id: binding.conversation_id,
            branch_id: binding.branch_id,
        };
        let mut messages = Vec::new();
        let mut source_bytes = 2usize;
        if let Some(previous) = &previous {
            let coverage_header = preflight_context_dependency_coverage_on(
                transaction,
                self.person_id,
                session_id,
                previous.through_turn_id,
            )
            .await?;
            let summary = floe_conversation::AgentMessage::Compaction {
                turn_id: previous.through_turn_id,
                summary: previous.summary.clone(),
                recovery: previous.recovery(),
            };
            source_bytes = source_bytes
                .checked_add(
                    serde_json::to_vec(&summary)
                        .map_err(|_| AgentFailure::StorageUnavailable)?
                        .len(),
                )
                .and_then(|bytes| bytes.checked_add(coverage_header.accounted_bytes().ok()?))
                .filter(|bytes| *bytes <= byte_limit)
                .ok_or(AgentFailure::BudgetExceeded)?;
            let coverage = hydrate_context_dependency_coverage_on(
                transaction,
                self.person_id,
                session_id,
                previous.through_turn_id,
                coverage_header,
                #[cfg(test)]
                &self.context_coverage_payload_hydrations,
            )
            .await?;
            messages.push(floe_conversation::contract_message(
                &summary,
                previous.summary_alias_id,
                coverage,
            )?);
        }
        let mut byte_pressure = false;
        let mut candidate = None;
        let mut compact_tail_candidate = None;
        for sequence in (start + 1)..=limit {
            if messages.len() >= COMPACTION_TRIGGER_MESSAGES {
                pressure = true;
                break;
            }
            let entry = entry_on(transaction, scope, sequence)
                .await
                .map_err(owner_store_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?;
            self.validate_producing_task_reference_on(transaction, &entry)
                .await
                .map_err(owner_store_failure)?;
            let preflight = self
                .preflight_owner_entry_on(transaction, &target, session_id, &entry, scope)
                .await
                .map_err(owner_store_failure)?;
            let Some(next_bytes) = source_bytes.checked_add(preflight.encoded_bytes()) else {
                return Err(AgentFailure::BudgetExceeded);
            };
            if next_bytes > byte_limit {
                pressure = true;
                byte_pressure = true;
                break;
            }
            let through = entry.reference;
            let prefix_digest = entry.prefix_digest;
            let resolved = self
                .resolve_manager_transcript_entry_preflighted_on(
                    transaction,
                    session_id,
                    entry,
                    preflight,
                )
                .await
                .map_err(owner_store_failure)?;
            let turn_id = resolved.message.turn_id();
            let message = floe_conversation::contract_message(
                &resolved.message,
                resolved.alias_id,
                resolved.coverage,
            )?;
            messages.push(message);
            if serde_json::to_vec(&messages)
                .map_err(|_| AgentFailure::StorageUnavailable)?
                .len()
                > byte_limit
            {
                pressure = true;
                byte_pressure = true;
                break;
            }
            source_bytes = next_bytes;
            byte_pressure |= source_bytes >= byte_limit;
            pressure |= byte_pressure;
            // A cutoff must include the final normalized contribution of its
            // owner turn. An arbitrary page boundary may split that turn.
            let mut rows = transaction.query(
                "SELECT MAX(sequence) FROM agent_conversation_owner_transcript_evidence_v1 WHERE person_id = ? AND session_id = ? AND first_recording_run_id = ?",
                (self.person_id.to_string(), session_id.to_string(), turn_id.to_string()),
            ).await.map_err(database_failure)?;
            let end = rows
                .next()
                .await
                .map_err(database_failure)?
                .ok_or(AgentFailure::StorageUnavailable)?
                .get::<Option<i64>>(0)
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            drop(rows);
            // Input-only terminal turns have no generated evidence. Their
            // exact original input is the whole turn, never a made-up cut.
            let end = if let Some(end) = end {
                u64::try_from(end).map_err(|_| AgentFailure::StorageUnavailable)?
            } else {
                let id = floe_kernel::RunId::from_uuid(turn_id)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                super::owner_custody::owner_input_binding_on(transaction, self.person_id, id)
                    .await
                    .map_err(owner_store_failure)?
                    .ok_or(AgentFailure::StorageUnavailable)?
                    .input
                    .sequence
            };
            if end != sequence
                || head.state.head_revision - sequence < COMPACTION_MIN_RETAIN_ENTRIES
            {
                continue;
            }
            let selected = CompactionSource {
                session_id,
                session_revision: expected_revision,
                head: head.state.reference(),
                previous_checkpoint: checkpoint.clone(),
                through,
                prefix_digest,
                through_turn_id: turn_id,
                messages: messages.clone(),
            };
            selected.validate()?;
            if head.state.head_revision - sequence >= COMPACTION_PREFERRED_RETAIN_ENTRIES {
                candidate = Some(selected);
            } else {
                compact_tail_candidate = Some(selected);
            }
        }
        if pressure {
            let selected = if byte_pressure {
                compact_tail_candidate.or(candidate)
            } else {
                candidate.or(compact_tail_candidate)
            };
            selected.map(Some).ok_or(AgentFailure::BudgetExceeded)
        } else {
            Ok(None)
        }
    }
}
