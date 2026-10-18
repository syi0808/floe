//! Point-read a published result without following the mutable active Run.
use super::*;
use floe_context::{LiveResultReader, LiveResultSnapshot};
use floe_context_contract::LiveProjectionSource;
use floe_conversation::{
    SurfaceEntryReceipt, SurfaceMessage, SurfaceMessageContent, SurfaceResultCompleteness,
};
use floe_execution::BoxFuture;
use floe_kernel::OwnerActor;
use turso::transaction::TransactionBehavior;

impl<Keys: VaultKeyProvider> LiveResultReader for EncryptedAgentVault<Keys> {
    fn read_live_result<'a>(
        &'a self,
        actor: &'a OwnerActor,
        source: &'a LiveProjectionSource,
    ) -> BoxFuture<'a, Result<LiveResultSnapshot, AgentFailure>> {
        Box::pin(async move {
            actor.validate()?;
            source.validate()?;
            let mut connection = self.connection()?;
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .await
                .map_err(database_failure)?;
            let (result, _) = self.read_live_result_on(&tx, actor, source).await?;
            tx.commit().await.map_err(database_failure)?;
            self.check_access()?;
            Ok(result)
        })
    }
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn read_live_result_on(
        &self,
        c: &turso::Connection,
        actor: &OwnerActor,
        source: &LiveProjectionSource,
    ) -> Result<(LiveResultSnapshot, floe_kernel::RunId), AgentFailure> {
        actor.validate()?;
        source.validate()?;
        let LiveProjectionSource::SurfaceResult {
            through,
            surface_message_id,
        } = source
        else {
            return Err(AgentFailure::InvalidInput);
        };
        self.surface_session_on(c, actor, through.session_id)
            .await?;
        self.surface_anchor_on(c, *through).await?;
        let mut rows = c.query(
                "SELECT sequence FROM agent_surface_entries WHERE session_id = ? AND message_id = ? AND kind = 'published_result' AND sequence <= ?",
                (through.session_id.to_string(), surface_message_id.as_uuid().to_string(), through.sequence as i64),
            ).await.map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::NotFound)?;
        let sequence: i64 = row.get(0).map_err(storage)?;
        if sequence <= 0 || rows.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(rows);
        let entries = self
            .surface_entries_on(c, *through, sequence as u64 - 1, 1)
            .await?;
        let [SurfaceEntryReceipt::PublishedResult(receipt)] = entries.as_slice() else {
            return Err(AgentFailure::StorageUnavailable);
        };
        if receipt.result.message_id() != *surface_message_id {
            return Err(AgentFailure::StorageUnavailable);
        }
        let run = self
            .conversation_run_on(c, receipt.result.run_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        // Reuse Conversation's exact publication/output/revision verification.
        let message = SurfaceMessage::result(receipt, &run)?;
        let SurfaceMessageContent::Result {
            text, completeness, ..
        } = message.content
        else {
            return Err(AgentFailure::StorageUnavailable);
        };
        let result = LiveResultSnapshot {
            person_id: actor.person_id,
            source: source.clone(),
            text,
            partial: completeness == SurfaceResultCompleteness::Partial,
            coverage: run.coverage,
        };
        result.validate_for(actor, source)?;
        Ok((result, receipt.result.run_id))
    }
}
