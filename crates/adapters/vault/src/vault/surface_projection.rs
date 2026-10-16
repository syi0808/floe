//! One read snapshot for verified Surface custody and safe original bodies.
use super::*;
use floe_conversation::{SurfaceEntryReceipt, SurfaceMessage, SurfaceReadRequest, SurfaceSnapshot};
use floe_kernel::OwnerActor;
use turso::transaction::TransactionBehavior;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn surface_snapshot(
        &self,
        actor: &OwnerActor,
        request: SurfaceReadRequest,
    ) -> Result<SurfaceSnapshot, AgentFailure> {
        request.validate()?;
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        self.surface_session_on(&tx, actor, request.session_id)
            .await?;
        let through = match request.through {
            Some(anchor) => anchor,
            None => self.surface_head_on(&tx, request.session_id).await?,
        };
        self.surface_anchor_on(&tx, through).await?;
        let after = through.sequence.saturating_sub(request.limit as u64);
        let entries = self
            .surface_entries_on(&tx, through, after, request.limit)
            .await?;
        let mut messages = Vec::with_capacity(entries.len());
        for entry in entries {
            let message = match entry {
                SurfaceEntryReceipt::Input(receipt) => SurfaceMessage::input(&receipt)?,
                SurfaceEntryReceipt::PublishedResult(receipt) => {
                    let run = self
                        .conversation_run_on(&tx, receipt.result.run_id)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    SurfaceMessage::result(&receipt, &run)?
                }
                SurfaceEntryReceipt::RunOutcome(receipt) => {
                    let run = self
                        .conversation_run_on(&tx, receipt.outcome.run_id)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    SurfaceMessage::outcome(&receipt, &run)?
                }
                SurfaceEntryReceipt::PendingWorkCancelled(receipt) => {
                    let work = self
                        .surface_work_command_on(&tx, receipt.cancellation.work_command_id)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    SurfaceMessage::cancellation(&receipt, &work)?
                }
            };
            messages.push(message);
        }
        let snapshot = SurfaceSnapshot::from_suffix(through, messages)?;
        tx.commit().await.map_err(database_failure)?;
        self.check_access()?;
        Ok(snapshot)
    }
}
