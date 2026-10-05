//! Rejoin actual Task evidence without dispatching work. Receipt attachment,
//! deferred terminal settlement and eligible Session accounting share one transaction.

use super::database_failure;
use floe_agent_contract::{
    DelegationRequest, DependencyCoverage, JournalEntry, JournalEvent, MAX_OUTPUT_BYTES,
    MAX_TASK_RECEIPT_BYTES, ReplayReceipt, RunId, TaskExecutionEvidence, TaskReceipt, TaskState,
    delegation_request_digest,
};
use floe_conversation::{RunRecord, RunState, project_run_receipt, validate_run_journal};
use turso::transaction::TransactionBehavior;

use super::conversations::{MAX_JOURNAL_ENTRIES, encode_record, integer, state_name};
use super::*;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Read the Run and its accounting from one physical snapshot, including
    /// while the recovery owner is attaching a late Task result.
    pub async fn accounted_conversation_receipt(
        &self,
        run_id: RunId,
    ) -> Result<Option<floe_conversation::RunReceipt>, AgentFailure> {
        if !run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let Some(record) = self.conversation_run_on(&transaction, run_id).await? else {
                return Ok(None);
            };
            let journal = self.conversation_journal_on(&transaction, &record).await?;
            let mut receipt = project_run_receipt(record)?;
            let accounting = floe_conversation::project_run_accounting(&receipt, &journal)?;
            receipt.attempt_refs = accounting.attempt_refs;
            receipt.task_refs = accounting.task_refs;
            receipt.unresolved_attempts = accounting.unresolved_attempts;
            receipt.unresolved_delegations = accounting.unresolved_delegations;
            receipt.validate()?;
            self.check_access()?;
            Ok(Some(receipt))
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn reconcile_conversation_delegation(
        &self,
        run_id: RunId,
        receipt: TaskReceipt,
    ) -> Result<(), AgentFailure> {
        if !run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        receipt.validate(MAX_OUTPUT_BYTES)?;
        let event = JournalEvent::DelegationResult {
            receipt: Box::new(receipt.clone()),
        };
        let payload = serde_json::to_string(&event).map_err(storage)?;
        if payload.len() > MAX_TASK_RECEIPT_BYTES + 4096 {
            return Err(AgentFailure::BudgetExceeded);
        }

        // connection() verifies the actual Person Vault key before the first
        // read. All Task authentication and both Run writes share this snapshot.
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let mut record = self.conversation_run_on(&transaction, run_id).await?
                .ok_or(AgentFailure::NotFound)?;
            let active = self.active_conversation_executor_generation(&transaction).await?;
            if matches!(record.state, RunState::Completed | RunState::Blocked)
                || (record.state == RunState::Working && record.pending_terminal.is_none()
                    && record.executor_generation == active) {
                return Err(AgentFailure::Conflict);
            }
            let previous_record = record.clone();
            let mut journal = self.conversation_journal_on(&transaction, &record).await?;
            let mut intents = journal.iter().filter_map(|entry| match &entry.event {
                JournalEvent::DelegationIntent { request } if request.task_id == receipt.task_id => Some(request),
                _ => None,
            });
            let request = intents.next().ok_or(AgentFailure::Conflict)?.clone();
            if intents.next().is_some() {
                return Err(AgentFailure::StorageUnavailable);
            }
            let mut results = journal.iter().filter_map(|entry| match &entry.event {
                JournalEvent::DelegationResult { receipt: stored } if stored.task_id == receipt.task_id => Some(stored.as_ref()),
                _ => None,
            });
            let replay = match results.next() {
                Some(stored) if stored == &receipt => true,
                Some(_) => return Err(AgentFailure::Conflict),
                None => false,
            };
            if results.next().is_some() {
                return Err(AgentFailure::StorageUnavailable);
            }
            if request.parent_run_id != Some(run_id.as_uuid())
                || request.principal != self.person_id.to_string()
                || request.execution_context.session_id != record.session_id
                || request.execution_context.device_id != record.device_id
                || receipt.snapshot.principal != request.principal
                || receipt.snapshot.agent_id != request.selected_agent_id
                || receipt.snapshot.definition_revision != request.selected_definition_revision
            {
                return Err(AgentFailure::Conflict);
            }
            receipt.validate(request.execution_context.max_output_bytes)?;

            // Keep the Task's original parent. The same-transaction lineage
            // check below proves adoption by this Run rather than accepting an
            // arbitrary same-session parent supplied in a receipt.
            let mut original = request.clone();
            original.parent_run_id = receipt.snapshot.parent_run_id;
            let input_digest = delegation_request_digest(&original);
            validate_replay(&original, &receipt)?;
            let task = self.task_on(&transaction, receipt.task_id).await?;
            match (&receipt.execution, task) {
                (TaskExecutionEvidence::Admitted(evidence), Some(task)) => {
                    self.validate_conversation_task_lineage_on(&transaction, &record, &task).await?;
                    if task.snapshot != receipt.snapshot
                        || task.snapshot.parent_run_id != original.parent_run_id
                        || task.snapshot.principal != original.principal
                        || task.device_id != original.execution_context.device_id
                        || task.device_id != record.device_id
                        || task.invocation_key != original.invocation_key
                        || task.request_digest != input_digest
                        || task.maximum_output_bytes > original.execution_context.max_output_bytes
                        || task.execution() != evidence.reference.execution
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    let actual = self.read_execution_receipt_on(&transaction, &evidence.reference).await?;
                    if actual != *evidence {
                        return Err(AgentFailure::Conflict);
                    }
                }
                (TaskExecutionEvidence::Unadmitted, None) => {
                    // task_on treats an orphan journal or failed read as an
                    // error. Only true absence permits the owner's interrupted
                    // pre-admission rejection, never an invented completion.
                    if receipt.snapshot.parent_run_id != request.parent_run_id
                        || receipt.snapshot.state != TaskState::Rejected
                        || receipt.snapshot.issue != Some(AgentFailure::Interrupted)
                        || receipt.snapshot.result.is_some()
                        || !receipt.snapshot.artifacts.is_empty()
                        || receipt.snapshot.coverage != DependencyCoverage::Independent
                        || receipt.snapshot.blockage.is_some()
                        || receipt.replay.is_some()
                    {
                        return Err(AgentFailure::Conflict);
                    }
                }
                _ => return Err(AgentFailure::Conflict),
            }
            if replay {
                self.check_access()?;
                return Ok(());
            }
            if record.journal_revision >= MAX_JOURNAL_ENTRIES {
                return Err(AgentFailure::BudgetExceeded);
            }
            let previous = record.journal_revision;
            let next = previous.checked_add(1).ok_or(AgentFailure::BudgetExceeded)?;
            journal.push(JournalEntry { revision: next, event });
            // The canonical validator checks the exact outstanding intent and
            // preserves one result slot for every other acknowledged intent.
            validate_run_journal(&project_run_receipt(record.clone())?, &journal)?;
            record.journal_revision = next;
            if record.state == RunState::Working && record.pending_terminal.is_none() {
                record = floe_conversation::defer_run_terminal(&record, AgentFailure::Interrupted)?;
            } else {
                record.aggregate_revision = record.aggregate_revision.checked_add(1).ok_or(AgentFailure::BudgetExceeded)?;
            }
            if record.state.is_terminal() {
                let accounting = self.conversation_lineage_accounting_on(&transaction, &record, &journal).await?;
                let mut session = self.session_on(&transaction, record.session_id).await?;
                // Usage belongs to the current turn, not whichever Task settles
                // last. Older Run evidence never rewrites a newer Session.
                if session.revision == previous_record.session_revision && session.active_turn.is_none() {
                    let old_revision = session.revision;
                    session.usage = accounting.usage;
                    session.continuation = if accounting.unresolved_attempts.is_empty()
                        && accounting.unresolved_delegations.is_empty() {
                        project_run_receipt(record.clone())?.continuation().map(|_| floe_conversation::AgentContinuation {
                            turn_id: record.run_id.as_uuid(), level: record.continuation_level, usage: accounting.usage })
                    } else { None };
                    session.revision = session.revision.checked_add(1).ok_or(AgentFailure::BudgetExceeded)?;
                    record.session_revision = session.revision;
                    let changed = transaction.execute("UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                        (integer(session.revision)?, self.payload(&session)?, session.id.to_string(), integer(old_revision)?)).await.map_err(database_failure)?;
                    if changed != 1 { return Err(AgentFailure::Conflict); }
                }
            }
            record.validate(self.person_id)?;
            let changed = transaction.execute(
                "UPDATE agent_conversation_runs SET journal_revision = ?, aggregate_revision = ?, payload = ? WHERE run_id = ? AND person_id = ? AND state = ? AND aggregate_revision = ? AND journal_revision = ? AND executor_generation = ?",
                (integer(next)?, integer(record.aggregate_revision)?, encode_record(&record)?, run_id.as_uuid().to_string(),
                    self.person_id.to_string(), state_name(record.state),
                    integer(previous_record.aggregate_revision)?, integer(previous)?,
                    integer(record.executor_generation)?),
            ).await.map_err(database_failure)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            transaction.execute(
                "INSERT INTO agent_conversation_journal (run_id, revision, kind, payload) VALUES (?, ?, 'result', ?)",
                (run_id.as_uuid().to_string(), integer(next)?, payload),
            ).await.map_err(database_failure)?;
            if record.pending_terminal.is_some() {
                self.settle_pending_conversation_terminal_on(&transaction, record).await?;
            }
            self.check_access()?;
            Ok(())
        }.await;
        // Includes another access check after commit; an uncertain commit is
        // retained for exact readback and can never trigger a second dispatch.
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn conversation_recovery_runs(
        &self,
        actor: &floe_kernel::OwnerActor,
        after: Option<RunId>,
        limit: usize,
    ) -> Result<floe_conversation::RecoveryPage<RunId, RunId>, AgentFailure> {
        actor.validate()?;
        if actor.person_id != self.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        if limit == 0 || limit > 64 || after.is_some_and(|id| !id.is_valid()) {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        self.active_conversation_executor_generation(&connection)
            .await?;
        let mut rows = connection.query(
            "SELECT r.run_id FROM agent_conversation_runs r WHERE r.person_id = ? AND json_extract(r.payload, '$.device_id') = ? AND r.run_id > ? AND (r.state = 'working' OR (r.state IN ('failed','cancelled','timed_out','interrupted') AND EXISTS (SELECT 1 FROM agent_conversation_journal i WHERE i.run_id = r.run_id AND json_extract(i.payload, '$.kind') = 'delegation_intent' AND NOT EXISTS (SELECT 1 FROM agent_conversation_journal o WHERE o.run_id = r.run_id AND json_extract(o.payload, '$.kind') = 'delegation_result' AND json_extract(o.payload, '$.receipt.task_id') = json_extract(i.payload, '$.request.task_id'))))) ORDER BY r.run_id LIMIT ?",
            (self.person_id.to_string(), actor.device_id.clone(), after.map_or_else(String::new, |id| id.as_uuid().to_string()), limit as i64 + 1)).await.map_err(database_failure)?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await.map_err(database_failure)? {
            let id = uuid::Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                .map_err(unavailable)?;
            ids.push(RunId::from_uuid(id).ok_or(AgentFailure::StorageUnavailable)?);
        }
        let more = ids.len() > limit;
        ids.truncate(limit);
        let next_cursor = if more { ids.last().copied() } else { None };
        self.check_access()?;
        Ok(floe_conversation::RecoveryPage {
            items: ids,
            next_cursor,
        })
    }

    pub async fn settle_pending_conversation_terminal(
        &self,
        actor: &floe_kernel::OwnerActor,
        run_id: RunId,
    ) -> Result<RunRecord, AgentFailure> {
        actor.validate()?;
        if actor.person_id != self.person_id || !run_id.is_valid() {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let active = self
                .active_conversation_executor_generation(&transaction)
                .await?;
            let mut record = self
                .conversation_run_on(&transaction, run_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if record.device_id != actor.device_id {
                return Err(AgentFailure::PolicyDenied);
            }
            if record.state.is_terminal() {
                return Ok(record);
            }
            if record.pending_terminal.is_none() {
                if record.executor_generation >= active {
                    return Err(AgentFailure::Conflict);
                }
                let next =
                    floe_conversation::defer_run_terminal(&record, AgentFailure::Interrupted)?;
                if super::conversations::write_run(
                    &transaction,
                    &next,
                    record.aggregate_revision,
                    record.executor_generation,
                )
                .await?
                    != 1
                {
                    return Err(AgentFailure::Conflict);
                }
                record = next;
            }
            self.settle_pending_conversation_terminal_on(&transaction, record)
                .await
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    async fn settle_pending_conversation_terminal_on(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        record: RunRecord,
    ) -> Result<RunRecord, AgentFailure> {
        let pending = record.pending_terminal.ok_or(AgentFailure::Conflict)?;
        let journal = self.conversation_journal_on(transaction, &record).await?;
        if !floe_conversation::unresolved_run_delegations(
            &project_run_receipt(record.clone())?,
            &journal,
        )?
        .is_empty()
        {
            return Ok(record);
        }
        let terminal = floe_conversation::RunTerminal::from_failure(pending.failure);
        let digest = super::conversations::terminal_digest(
            record.run_id,
            pending.requested_from_revision,
            &terminal,
        )?;
        self.apply_conversation_terminal_on(transaction, &record, terminal, digest)
            .await
    }

    pub(super) async fn conversation_lineage_accounting_on(
        &self,
        connection: &turso::Connection,
        record: &RunRecord,
        journal: &[JournalEntry],
    ) -> Result<floe_conversation::RunAccountingProjection, AgentFailure> {
        let mut chain = vec![(project_run_receipt(record.clone())?, journal.to_vec())];
        let mut seen = std::collections::HashSet::from([record.run_id]);
        while let Some(parent) = chain.last().and_then(|(run, _)| run.continuation_of) {
            if chain.len() >= 4 || !seen.insert(parent) {
                return Err(AgentFailure::StorageUnavailable);
            }
            let record = self
                .conversation_run_on(connection, parent)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            let journal = self.conversation_journal_on(connection, &record).await?;
            chain.push((project_run_receipt(record)?, journal));
        }
        chain.reverse();
        floe_conversation::project_lineage_accounting(&chain)
    }

    /// Authenticate an actual Task's parent through the persisted continuation
    /// chain. Blocked publication uses this same read-only transaction seam.
    pub(super) async fn validate_conversation_task_lineage_on(
        &self,
        connection: &turso::Connection,
        run: &RunRecord,
        task: &floe_experts::TaskRecord,
    ) -> Result<(), AgentFailure> {
        self.check_access()?;
        let mut current = self
            .conversation_run_on(connection, run.run_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        if current != *run {
            return Err(AgentFailure::Conflict);
        }
        let mut seen = std::collections::HashSet::new();
        let mut chain = Vec::new();
        loop {
            if chain.len() >= 4 || !seen.insert(current.run_id) {
                return Err(AgentFailure::Conflict);
            }
            let entries = self.conversation_journal_on(connection, &current).await?;
            let parent = current.continuation_of;
            chain.push((project_run_receipt(current)?, entries));
            let Some(parent) = parent else {
                break;
            };
            current = self
                .conversation_run_on(connection, parent)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
        }
        chain.reverse();
        floe_conversation::validate_task_delegation_lineage(&chain, task)?;
        self.check_access()
    }
}

fn validate_replay(request: &DelegationRequest, receipt: &TaskReceipt) -> Result<(), AgentFailure> {
    let Some(replay) = &receipt.replay else {
        return Ok(());
    };
    let expected = ReplayReceipt {
        principal: request.principal.clone(),
        run_id: request.parent_run_id.and_then(RunId::from_uuid),
        task_id: Some(request.task_id),
        agent_id: Some(request.selected_agent_id.clone()),
        tool_id: None,
        definition_revision: request.selected_definition_revision,
        input_digest: delegation_request_digest(request),
        invocation_key: request.invocation_key,
        call_id: request.task_id.as_uuid(),
        result: receipt
            .snapshot
            .result
            .clone()
            .unwrap_or_else(|| format!("task {:?}", receipt.snapshot.state)),
        task_result: receipt.snapshot.result.clone(),
        task_state: Some(receipt.snapshot.state),
        task_artifacts: receipt.snapshot.artifacts.clone(),
        task_coverage: receipt.snapshot.coverage.clone(),
        task_issue: receipt.snapshot.issue,
        task_execution: match &receipt.execution {
            TaskExecutionEvidence::Admitted(value) => Some(value.clone()),
            TaskExecutionEvidence::Unadmitted => None,
        },
        tool_artifacts: vec![],
        tool_coverage: DependencyCoverage::Unknown,
        tool_issue: None,
    };
    if replay != &expected {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}
