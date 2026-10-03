use std::sync::Arc;

use floe_agent_contract::{AgentFailure, BoxFuture, ExecutionJournal, JournalAck, JournalEvent};
use floe_kernel::PersonId;
use floe_knowledge::LearnerJournalFactory;
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

pub struct VaultLearnerJournalFactory<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
}

impl<Keys> VaultLearnerJournalFactory<Keys> {
    pub fn new(vault: Arc<EncryptedAgentVault<Keys>>) -> Self {
        Self { vault }
    }
}

impl<Keys: VaultKeyProvider + 'static> LearnerJournalFactory for VaultLearnerJournalFactory<Keys> {
    fn journal(
        &self,
        person_id: PersonId,
        job_id: Uuid,
        claim_attempt: u8,
    ) -> Result<Arc<dyn ExecutionJournal>, AgentFailure> {
        if person_id != self.vault.person_id()
            || job_id.is_nil()
            || !(1..=floe_knowledge::MAX_LEARNER_JOB_ATTEMPTS).contains(&claim_attempt)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(Arc::new(VaultLearnerJournal {
            vault: self.vault.clone(),
            job_id,
            claim_attempt,
        }))
    }
}

struct VaultLearnerJournal<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    job_id: Uuid,
    claim_attempt: u8,
}

impl<Keys: VaultKeyProvider + 'static> VaultLearnerJournal<Keys> {
    fn record<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move {
            let revision = self
                .vault
                .append_learner_journal(self.job_id, self.claim_attempt, event)
                .await?;
            Ok(JournalAck::Accepted { revision })
        })
    }
}

impl<Keys: VaultKeyProvider + 'static> ExecutionJournal for VaultLearnerJournal<Keys> {
    fn record_intent<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        if !matches!(event, JournalEvent::ModelIntent { .. }) {
            return Box::pin(async { Err(AgentFailure::CapabilityDenied) });
        }
        self.record(event)
    }
    fn record_result<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        if !matches!(event, JournalEvent::ModelResult { .. }) {
            return Box::pin(async { Err(AgentFailure::CapabilityDenied) });
        }
        self.record(event)
    }
    fn record_output<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        if !matches!(event, JournalEvent::Output { .. }) {
            return Box::pin(async { Err(AgentFailure::CapabilityDenied) });
        }
        self.record(event)
    }
    fn checkpoint<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        if !matches!(
            event,
            JournalEvent::Checkpoint { .. }
                | JournalEvent::ValidatedBatch { .. }
                | JournalEvent::BatchProgress { .. }
        ) {
            return Box::pin(async { Err(AgentFailure::CapabilityDenied) });
        }
        self.record(event)
    }
}
