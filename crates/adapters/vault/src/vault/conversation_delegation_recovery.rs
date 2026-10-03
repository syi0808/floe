//! Rejoin acknowledged Task evidence after the parent Run has exhausted its
//! budget or deadline. This transaction never admits or dispatches work.

use floe_agent_contract::{
    delegation_request_digest, DelegationRequest, DependencyCoverage, JournalEntry,
    JournalEvent, ReplayReceipt, RunId, TaskExecutionEvidence, TaskReceipt, TaskState,
    MAX_OUTPUT_BYTES, MAX_TASK_RECEIPT_BYTES,
};
use floe_conversation::{project_run_receipt, validate_run_journal, RunRecord, RunState};
use turso::transaction::TransactionBehavior;

use super::conversations::{encode_record, integer, state_name, MAX_JOURNAL_ENTRIES};
use super::*;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn reconcile_conversation_delegation(
        &self,
        run_id: RunId,
        receipt: TaskReceipt,
    ) -> Result<(), AgentFailure> {
        if !run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        receipt.validate(MAX_OUTPUT_BYTES)?;
        let event = JournalEvent::DelegationResult { receipt: Box::new(receipt.clone()) };
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
            if !matches!((record.state, record.issue),
                (RunState::TimedOut, Some(AgentFailure::DeadlineExceeded))
                    | (RunState::Failed, Some(AgentFailure::BudgetExceeded)))
            {
                return Err(AgentFailure::Conflict);
            }
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
            record.validate(self.person_id)?;
            let changed = transaction.execute(
                "UPDATE agent_conversation_runs SET journal_revision = ?, payload = ? WHERE run_id = ? AND person_id = ? AND state = ? AND aggregate_revision = ? AND journal_revision = ? AND executor_generation = ?",
                (integer(next)?, encode_record(&record)?, run_id.as_uuid().to_string(),
                    self.person_id.to_string(), state_name(record.state),
                    integer(record.aggregate_revision)?, integer(previous)?,
                    integer(record.executor_generation)?),
            ).await.map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            transaction.execute(
                "INSERT INTO agent_conversation_journal (run_id, revision, kind, payload) VALUES (?, ?, 'result', ?)",
                (run_id.as_uuid().to_string(), integer(next)?, payload),
            ).await.map_err(storage)?;
            self.check_access()?;
            Ok(())
        }.await;
        // Includes another access check after commit; an uncertain commit is
        // retained for exact readback and can never trigger a second dispatch.
        self.finish_registry_transaction_checked(transaction, result).await
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
        let mut current = self.conversation_run_on(connection, run.run_id).await?
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
            let Some(parent) = parent else { break; };
            current = self.conversation_run_on(connection, parent).await?
                .ok_or(AgentFailure::StorageUnavailable)?;
        }
        chain.reverse();
        floe_conversation::validate_task_delegation_lineage(&chain, task)?;
        self.check_access()
    }
}

fn validate_replay(request: &DelegationRequest, receipt: &TaskReceipt) -> Result<(), AgentFailure> {
    let Some(replay) = &receipt.replay else { return Ok(()); };
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
        result: receipt.snapshot.result.clone().unwrap_or_else(|| format!("task {:?}", receipt.snapshot.state)),
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
    if replay != &expected { return Err(AgentFailure::Conflict); }
    Ok(())
}
