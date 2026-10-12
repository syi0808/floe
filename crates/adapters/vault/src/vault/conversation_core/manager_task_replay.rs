//! Continue observers rejoin an existing Task contribution. They do not become
//! its producer, overwrite its typed row, or append a duplicate public alias.
use super::*;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn verify_inherited_manager_task_on(
        &self,
        transaction: &Transaction<'_>,
        run: &RunRecord,
        receipt: &TaskReceipt,
    ) -> Result<(), ConversationStoreFailure> {
        let floe_agent_contract::TaskExecutionEvidence::Admitted(execution) = &receipt.execution
        else {
            return Err(owner_evidence_mismatch());
        };
        let task = self
            .task_on(transaction, receipt.task_id)
            .await
            .map_err(owner_error)?
            .ok_or_else(owner_evidence_mismatch)?;
        let original_run_id = task
            .snapshot
            .parent_run_id
            .and_then(RunId::from_uuid)
            .ok_or_else(owner_evidence_mismatch)?;
        if original_run_id == run.run_id
            || task.snapshot != receipt.snapshot
            || task.receipt.as_ref() != Some(execution)
            || receipt.replay.is_none()
        {
            return Err(owner_evidence_mismatch());
        }
        receipt
            .validate(floe_agent_contract::MAX_OUTPUT_BYTES)
            .map_err(owner_error)?;
        self.validate_conversation_task_lineage_on(transaction, run, &task)
            .await
            .map_err(owner_error)?;
        let entries = self
            .conversation_journal_on(transaction, run)
            .await
            .map_err(owner_error)?;
        let mut intents = entries.iter().filter_map(|entry| match &entry.event {
            JournalEvent::DelegationIntent { request } if request.task_id == receipt.task_id => {
                Some(request.clone())
            }
            _ => None,
        });
        let mut original = intents.next().ok_or_else(owner_evidence_mismatch)?;
        if intents.next().is_some() || original.parent_run_id != Some(run.run_id.as_uuid()) {
            return Err(owner_evidence_mismatch());
        }
        original.parent_run_id = Some(original_run_id.as_uuid());
        super::super::conversation_delegation_recovery::validate_task_replay(&original, receipt)
            .map_err(owner_error)?;

        let reference = task_evidence_reference(&execution.reference)?;
        self.verified_task_reference_on(transaction, original_run_id, &reference)
            .await?;
        let (message_id, command_id, contribution_id, typed_entry_id) =
            manager_contribution_ids(original_run_id, "delegation", receipt.task_id.as_uuid())?;
        let stored = recording_receipt_on(transaction, self.person_id, contribution_id)
            .await?
            .ok_or_else(owner_evidence_mismatch)?;
        let link =
            typed_transcript_link_for_contribution_on(transaction, self.person_id, contribution_id)
                .await?
                .ok_or_else(owner_evidence_mismatch)?;
        if stored.recorder.run_id != original_run_id
            || link.first_recording_run_id != original_run_id
            || link.owner_session_id != run.session_id
            || link.original_task_receipt.as_ref() != Some(&execution.reference)
        {
            return Err(owner_evidence_mismatch());
        }
        let replayed = self
            .record_typed_conversation_entry_on(
                transaction,
                TypedConversationRecordingRequest {
                    recorder: stored.recorder.clone(),
                    message_id,
                    command_id,
                    contribution_id,
                    typed_entry_id,
                    typed_message: AgentMessage::Delegation {
                        turn_id: original_run_id.as_uuid(),
                        task: receipt.snapshot.clone(),
                        execution_receipt: Some(execution.reference.clone()),
                    },
                },
            )
            .await?;
        if replayed != stored {
            return Err(owner_evidence_mismatch());
        }
        Ok(())
    }

    pub(super) async fn record_inherited_manager_task_on(
        &self,
        transaction: &Transaction<'_>,
        run: &RunRecord,
        recorder: &RecorderFence,
        receipt: &TaskReceipt,
        payload: &str,
    ) -> Result<u64, ConversationStoreFailure> {
        self.verify_inherited_manager_task_on(transaction, run, receipt)
            .await?;
        let entries = self
            .conversation_journal_on(transaction, run)
            .await
            .map_err(owner_error)?;
        let mut results = entries.iter().filter(|entry| matches!(
            &entry.event, JournalEvent::DelegationResult { receipt: stored } if stored.task_id == receipt.task_id
        ));
        if let Some(stored) = results.next() {
            if results.next().is_some()
                || serde_json::to_string(&stored.event).map_err(|_| unavailable())? != payload
            {
                return Err(owner_evidence_mismatch());
            }
            // Exact acknowledgement remains available after terminal state or a
            // later executor generation; no new child contribution is created.
            return Ok(stored.revision);
        }
        let (current, _) = self
            .verified_typed_recorder_on(transaction, recorder, false)
            .await?;
        if current != *run
            || current.state != RunState::Working
            || current.pending_terminal.is_some()
        {
            return Err(owner_evidence_mismatch());
        }
        let revision = self
            .append_conversation_journal_on(transaction, run.run_id, "result", payload)
            .await
            .map_err(owner_error)?;
        super::super::context_dependencies::merge_context_dependency_coverage(
            transaction,
            self.person_id,
            run.session_id,
            run.run_id.as_uuid(),
            receipt.snapshot.coverage.clone(),
        )
        .await
        .map_err(owner_error)?;
        Ok(revision)
    }
}
