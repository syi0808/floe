use super::*;
use floe_conversation::{
    CompactionAdmission, CompactionClaim, CompactionExecution, CompactionRepository, RecoveryPage,
};
use floe_kernel::OwnerActor;

impl<Keys: VaultKeyProvider + 'static> CompactionRepository for VaultConversationRepository<Keys> {
    fn admit_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: CompactionAdmission,
    ) -> BoxFuture<'a, Result<Option<CompactionExecution>, AgentFailure>> {
        Box::pin(async move { self.vault.admit_manager_compaction(actor, request).await })
    }
    fn load_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        claim: CompactionClaim,
    ) -> BoxFuture<'a, Result<CompactionExecution, AgentFailure>> {
        Box::pin(async move { self.vault.load_manager_compaction(actor, claim).await })
    }
    fn pending_compactions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        after: Option<Uuid>,
        limit: usize,
    ) -> BoxFuture<'a, Result<RecoveryPage<Uuid, Uuid>, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .pending_manager_compactions(actor, after, limit)
                .await
        })
    }
    fn recover_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        execution_id: Uuid,
    ) -> BoxFuture<'a, Result<CompactionExecution, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .recover_manager_compaction(actor, execution_id)
                .await
        })
    }
    fn retire_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        claim: CompactionClaim,
        failure: AgentFailure,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.vault
                .retire_manager_compaction(actor, claim, failure)
                .await
        })
    }
    fn complete_compaction<'a>(
        &'a self,
        actor: &'a OwnerActor,
        claim: CompactionClaim,
    ) -> BoxFuture<'a, Result<floe_conversation::CompactionReceipt, AgentFailure>> {
        Box::pin(async move { self.vault.complete_manager_compaction(actor, claim).await })
    }
    fn compaction_journal(
        &self,
        actor: OwnerActor,
        claim: CompactionClaim,
    ) -> Result<Arc<dyn ExecutionJournal>, AgentFailure> {
        actor.validate()?;
        claim.validate()?;
        self.verify_principal(&actor.person_id.to_string())?;
        self.vault.check_access()?;
        Ok(Arc::new(VaultCompactionJournal {
            vault: self.vault.clone(),
            actor,
            claim,
        }))
    }
}

struct VaultCompactionJournal<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    actor: OwnerActor,
    claim: CompactionClaim,
}
impl<Keys: VaultKeyProvider + 'static> VaultCompactionJournal<Keys> {
    fn record<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move {
            let revision = self
                .vault
                .append_manager_compaction(&self.actor, self.claim, event)
                .await?;
            Ok(JournalAck::Accepted { revision })
        })
    }
}
impl<Keys: VaultKeyProvider + 'static> ExecutionJournal for VaultCompactionJournal<Keys> {
    fn record_intent<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        if !matches!(&event, JournalEvent::ModelIntent { .. }) {
            return Box::pin(async { Err(AgentFailure::CapabilityDenied) });
        }
        self.record(event)
    }
    fn record_result<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        if !matches!(&event, JournalEvent::ModelResult { .. }) {
            return Box::pin(async { Err(AgentFailure::CapabilityDenied) });
        }
        self.record(event)
    }
    fn record_output<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        if !matches!(&event, JournalEvent::Output { .. }) {
            return Box::pin(async { Err(AgentFailure::CapabilityDenied) });
        }
        self.record(event)
    }
    fn checkpoint<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        if !matches!(
            &event,
            JournalEvent::ValidatedBatch { .. }
                | JournalEvent::BatchProgress { .. }
                | JournalEvent::Checkpoint { .. }
        ) {
            return Box::pin(async { Err(AgentFailure::CapabilityDenied) });
        }
        self.record(event)
    }
}
