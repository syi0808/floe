//! Pending-work cancellation shares the queue claim's transaction boundary.
//! It never deletes input or changes an existing Run/Core recorder.
use super::*;
use floe_conversation::{CancelPendingWork, PendingWorkCancellationReceipt};
use floe_kernel::{CommandFailure, CommandId, OwnerActor};
use turso::{Connection, transaction::TransactionBehavior};

const MAX_RECEIPT_BYTES: usize = 4096;
fn command_id(value: String) -> Result<CommandId, AgentFailure> {
    Uuid::parse_str(&value)
        .ok()
        .and_then(CommandId::from_uuid)
        .ok_or(AgentFailure::StorageUnavailable)
}
impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    // Deliberately validates only the receipt row. Work validation calls this
    // reader for its cancellation proof; following Work here would recurse.
    pub(super) async fn pending_work_cancellation_on(
        &self,
        connection: &Connection,
        command: CommandId,
    ) -> Result<Option<PendingWorkCancellationReceipt>, AgentFailure> {
        let mut rows = connection.query("SELECT work_command_id, person_id FROM agent_surface_work_cancellations WHERE command_id = ?", [command.as_uuid().to_string()]).await.map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
            return Ok(None);
        };
        let entry = self
            .surface_entry_command_on(connection, command)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let floe_conversation::SurfaceEntryReceipt::PendingWorkCancelled(entry) = entry else {
            return Err(AgentFailure::StorageUnavailable);
        };
        let receipt = entry.cancellation;
        if receipt.command_id != command
            || receipt.person_id != self.person_id
            || row.get::<String>(0).map_err(storage)?
                != receipt.work_command_id.as_uuid().to_string()
            || row.get::<String>(1).map_err(storage)? != receipt.person_id.to_string()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(Some(receipt))
    }
    async fn verify_pending_cancellation_on(
        &self,
        connection: &Connection,
        actor: &OwnerActor,
        receipt: &PendingWorkCancellationReceipt,
    ) -> Result<(), AgentFailure> {
        let work = self
            .surface_work_command_on(connection, receipt.work_command_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        self.surface_session_on(connection, actor, work.anchor.session_id)
            .await?;
        if work.work_id != receipt.work_id {
            return Err(AgentFailure::StorageUnavailable);
        }
        let mut rows = connection
            .query(
                "SELECT state, run_id FROM agent_surface_work WHERE command_id = ?",
                [receipt.work_command_id.as_uuid().to_string()],
            )
            .await
            .map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::StorageUnavailable)?;
        if row.get::<String>(0).map_err(storage)? != "cancelled"
            || row.get::<Option<String>>(1).map_err(storage)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
    pub(super) async fn cancel_pending_surface_work(
        &self,
        actor: &OwnerActor,
        request: CancelPendingWork,
    ) -> Result<PendingWorkCancellationReceipt, CommandFailure<AgentFailure>> {
        let expected = request
            .receipt(actor)
            .map_err(CommandFailure::NotAdmitted)?;
        if actor.person_id != self.person_id {
            return Err(CommandFailure::NotAdmitted(AgentFailure::PolicyDenied));
        }
        let mut connection = self.connection().map_err(CommandFailure::Indeterminate)?;
        let read = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        if let Some(receipt) = self
            .pending_work_cancellation_on(&read, expected.command_id)
            .await
            .map_err(CommandFailure::Indeterminate)?
        {
            if receipt != expected {
                return Err(CommandFailure::Indeterminate(AgentFailure::Conflict));
            }
            self.verify_pending_cancellation_on(&read, actor, &receipt)
                .await
                .map_err(CommandFailure::Indeterminate)?;
            read.commit()
                .await
                .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            self.check_access().map_err(CommandFailure::Indeterminate)?;
            return Ok(receipt);
        }
        read.commit()
            .await
            .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
        let result = async {
            if let Some(receipt) = self.pending_work_cancellation_on(&tx, expected.command_id).await.map_err(CommandFailure::Indeterminate)? {
                if receipt != expected { return Err(CommandFailure::Indeterminate(AgentFailure::Conflict)); }
                self.verify_pending_cancellation_on(&tx, actor, &receipt).await.map_err(CommandFailure::Indeterminate)?;
                return Ok(receipt);
            }
            self.surface_command_free_on(&tx, expected.command_id).await.map_err(CommandFailure::Indeterminate)?;
            let work = self.surface_work_command_on(&tx, expected.work_command_id).await.map_err(CommandFailure::Indeterminate)?.ok_or(CommandFailure::NotApplied(AgentFailure::NotFound))?;
            self.surface_session_on(&tx, actor, work.anchor.session_id).await.map_err(CommandFailure::NotAdmitted)?;
            let mut rows = tx.query("SELECT state FROM agent_surface_work WHERE command_id = ?", [expected.work_command_id.as_uuid().to_string()]).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            let state: String = rows.next().await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?.ok_or(CommandFailure::Indeterminate(AgentFailure::StorageUnavailable))?.get(0).map_err(|_| CommandFailure::Indeterminate(AgentFailure::StorageUnavailable))?;
            drop(rows);
            match state.as_str() {
                "linked" => return Err(CommandFailure::NotApplied(AgentFailure::Conflict)),
                "queued" => {
                    let changed = tx.execute("UPDATE agent_surface_work SET state = 'cancelled' WHERE command_id = ? AND state = 'queued' AND run_id IS NULL", [expected.work_command_id.as_uuid().to_string()]).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
                    if changed != 1 { return Err(CommandFailure::NotApplied(AgentFailure::Conflict)); }
                }
                "cancelled" => {},
                _ => return Err(CommandFailure::Indeterminate(AgentFailure::StorageUnavailable)),
            }
            let head = self.surface_head_on(&tx, work.anchor.session_id).await.map_err(CommandFailure::Indeterminate)?;
            self.surface_anchor_on(&tx, head).await.map_err(CommandFailure::Indeterminate)?;
            let entry = floe_conversation::PendingWorkCancellationEntry::new(head, expected.clone()).map_err(CommandFailure::NotApplied)?;
            let payload = serde_json::to_string(&floe_conversation::SurfaceEntryReceipt::PendingWorkCancelled(entry.clone())).map_err(|_| CommandFailure::NotApplied(AgentFailure::InvalidInput))?;
            if payload.len() > MAX_RECEIPT_BYTES { return Err(CommandFailure::NotApplied(AgentFailure::BudgetExceeded)); }
            tx.execute("INSERT INTO agent_surface_entries (command_id, session_id, person_id, sequence, message_id, origin_key, kind, payload) VALUES (?, ?, ?, ?, ?, ?, 'work_cancelled', ?)", (expected.command_id.as_uuid().to_string(), head.session_id.to_string(), self.person_id.to_string(), entry.through.sequence as i64, entry.message_id().as_uuid().to_string(), format!("cancel:{}", expected.command_id.as_uuid()), payload)).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            tx.execute("INSERT INTO agent_surface_work_cancellations (command_id, work_command_id, person_id) VALUES (?, ?, ?)", (expected.command_id.as_uuid().to_string(), expected.work_command_id.as_uuid().to_string(), self.person_id.to_string())).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            tx.execute("UPDATE agent_surface_heads SET sequence = ?, digest = ? WHERE session_id = ?", (entry.through.sequence as i64, entry.through.digest.to_vec(), head.session_id.to_string())).await.map_err(|e| CommandFailure::Indeterminate(database_failure(e)))?;
            self.check_access().map_err(CommandFailure::Indeterminate)?;
            Ok(expected)
        }.await;
        self.finish_registry_command_transaction(tx, result).await
    }
    pub(super) async fn validate_pending_work_cancellation_store(
        &self,
    ) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let mut cursor = String::new();
        loop {
            let mut rows = tx.query("SELECT command_id FROM agent_surface_work_cancellations WHERE command_id > ? ORDER BY command_id LIMIT 128", [cursor.clone()]).await.map_err(database_failure)?;
            let mut count = 0;
            while let Some(row) = rows.next().await.map_err(database_failure)? {
                cursor = row.get(0).map_err(storage)?;
                count += 1;
                let id = command_id(cursor.clone())?;
                let receipt = self
                    .pending_work_cancellation_on(&tx, id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                self.verify_pending_cancellation_on(
                    &tx,
                    &OwnerActor {
                        person_id: receipt.person_id,
                        device_id: receipt.device_id.clone(),
                        runtime_epoch: 1,
                    },
                    &receipt,
                )
                .await?;
                if super::conversations::command_occupant(&tx, id.as_uuid()).await?
                    != Some(floe_conversation::ConversationCommandKind::SurfaceEntry)
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            if count < 128 {
                break;
            }
        }
        tx.commit().await.map_err(database_failure)?;
        self.check_access()
    }
}
