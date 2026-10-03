use std::sync::Arc;

use floe_agent_contract::{AgentFailure, BoxFuture, ExecutionJournal, JournalAck, JournalEvent, OwnerActor, PersonId};
use floe_knowledge::{LearnerClaimJournal, LearnerClaimRef, LearnerJournalFactory};

use crate::{EncryptedAgentVault, VaultKeyProvider};

/// The adapter pins one verified host actor to the Vault generation it owns.
pub struct VaultLearnerJournalFactory<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    actor: OwnerActor,
}

impl<Keys: VaultKeyProvider> VaultLearnerJournalFactory<Keys> {
    pub fn new(
        vault: Arc<EncryptedAgentVault<Keys>>,
        actor: OwnerActor,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        vault.check_access()?;
        if actor.person_id != vault.person_id() {
            return Err(AgentFailure::NotFound);
        }
        if actor.device_id.len() > 128 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self { vault, actor })
    }

    fn authorize_person(&self, person_id: PersonId) -> Result<(), AgentFailure> {
        self.vault.check_access()?;
        if person_id != self.actor.person_id || person_id != self.vault.person_id() {
            return Err(AgentFailure::PolicyDenied);
        }
        self.actor.validate()?;
        if self.actor.device_id.len() > 128 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

impl<Keys: VaultKeyProvider + 'static> LearnerJournalFactory for VaultLearnerJournalFactory<Keys> {
    fn journal(
        &self,
        person_id: PersonId,
        claim: LearnerClaimRef,
    ) -> Result<Arc<dyn ExecutionJournal>, AgentFailure> {
        self.authorize_person(person_id)?;
        claim.validate()?;
        Ok(Arc::new(VaultLearnerJournal {
            vault: self.vault.clone(),
            actor: self.actor.clone(),
            claim,
        }))
    }

    fn load_journal<'a>(
        &'a self,
        person_id: PersonId,
        claim: LearnerClaimRef,
    ) -> BoxFuture<'a, Result<LearnerClaimJournal, AgentFailure>> {
        Box::pin(async move {
            self.authorize_person(person_id)?;
            claim.validate()?;
            self.vault.load_learner_journal(&self.actor, claim).await
        })
    }
}

struct VaultLearnerJournal<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    actor: OwnerActor,
    claim: LearnerClaimRef,
}

impl<Keys: VaultKeyProvider + 'static> VaultLearnerJournal<Keys> {
    fn record<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move {
            let revision = self
                .vault
                .append_learner_journal(&self.actor, self.claim, event)
                .await?;
            // The Vault returns only after the event and authenticated head
            // have committed together, so callers may release result bytes.
            Ok(JournalAck::Accepted { revision })
        })
    }
}

impl<Keys: VaultKeyProvider + 'static> ExecutionJournal for VaultLearnerJournal<Keys> {
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
            JournalEvent::Checkpoint { .. }
                | JournalEvent::ValidatedBatch { .. }
                | JournalEvent::BatchProgress { .. }
        ) {
            return Box::pin(async { Err(AgentFailure::CapabilityDenied) });
        }
        self.record(event)
    }
}
