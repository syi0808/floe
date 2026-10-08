use std::sync::Arc;

use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, BoxFuture, ExecutionScope, OwnerActor};
use floe_kernel::CommandFailure;
use floe_knowledge::{
    KnowledgeRepository, LearnerBudget, LearnerClaimRef, LearnerEvidenceRepository,
    LearnerJobRepository, LearnerJobSettlement, LearnerReviewInput, LearnerReviewJob,
    LearningSessionSnapshot, MemoryDecisionRequest, MemoryStageRequest,
};
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

/// Owner-facing Knowledge ports over one exact encrypted Vault and host actor.
pub struct VaultKnowledgeRepository<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    actor: OwnerActor,
}

impl<Keys: VaultKeyProvider> VaultKnowledgeRepository<Keys> {
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

    fn authorize(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        self.vault.check_access()?;
        actor.validate()?;
        if actor != &self.actor {
            return Err(AgentFailure::CapabilityDenied);
        }
        if actor.person_id != self.vault.person_id() {
            return Err(AgentFailure::NotFound);
        }
        Ok(())
    }

    fn after_scoped<T>(
        &self,
        result: Result<T, AgentFailure>,
        scope: &ExecutionScope,
    ) -> Result<T, AgentFailure> {
        self.vault.check_access()?;
        check_scope(scope)?;
        result
    }

    fn after_durable<T>(&self, result: Result<T, AgentFailure>) -> Result<T, AgentFailure> {
        self.vault.check_access()?;
        result
    }
}

impl<Keys: VaultKeyProvider + 'static> KnowledgeRepository for VaultKnowledgeRepository<Keys> {
    fn read_context<'a>(
        &'a self,
        actor: &'a OwnerActor,
        now: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_knowledge::MemoryContextSnapshot, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            check_scope(scope)?;
            let result = scope
                .run(self.vault.knowledge_read_context(actor, now, scope))
                .await;
            self.after_scoped(result, scope)
        })
    }

    fn overview<'a>(
        &'a self,
        actor: &'a OwnerActor,
        limit: usize,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_knowledge::MemoryOverviewSnapshot, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            check_scope(scope)?;
            let result = scope
                .run(self.vault.knowledge_overview(actor, limit, scope))
                .await;
            self.after_scoped(result, scope)
        })
    }

    fn review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_knowledge::MemoryReviewSnapshot, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            check_scope(scope)?;
            let result = scope.run(self.vault.knowledge_review(actor, scope)).await;
            self.after_scoped(result, scope)
        })
    }

    fn decide<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: MemoryDecisionRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_knowledge::KnowledgeDecisionResult, CommandFailure<AgentFailure>>>
    {
        Box::pin(async move {
            self.authorize(actor).map_err(CommandFailure::NotAdmitted)?;
            check_scope(scope).map_err(CommandFailure::NotAdmitted)?;
            let result = scope
                .run(async { Ok(self.vault.knowledge_decide(actor, request, scope).await) })
                .await;
            match result {
                Err(failure) => Err(CommandFailure::Indeterminate(failure)),
                Ok(Err(failure)) => Err(failure),
                Ok(Ok(value)) => {
                    self.vault
                        .check_access()
                        .map_err(CommandFailure::Admitted)?;
                    check_scope(scope).map_err(CommandFailure::Admitted)?;
                    Ok(value)
                }
            }
        })
    }

    fn stage<'a>(
        &'a self,
        request: MemoryStageRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_knowledge::KnowledgeCandidate, AgentFailure>> {
        Box::pin(async move {
            self.authorize(&request.actor)?;
            check_scope(scope)?;
            let result = scope.run(self.vault.knowledge_stage(request, scope)).await;
            self.after_scoped(result, scope)
        })
    }
}

impl<Keys: VaultKeyProvider + 'static> LearnerJobRepository for VaultKnowledgeRepository<Keys> {
    fn discovery_sessions<'a>(
        &'a self,
        actor: &'a OwnerActor,
        limit: usize,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Vec<LearningSessionSnapshot>, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            check_scope(scope)?;
            let result = scope
                .run(self.vault.learner_discovery_sessions(actor, limit, scope))
                .await;
            self.after_scoped(result, scope)
        })
    }

    fn enqueue<'a>(
        &'a self,
        actor: &'a OwnerActor,
        input: LearnerReviewInput,
        available_at: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<LearnerReviewJob, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            check_scope(scope)?;
            let result = scope
                .run(
                    self.vault
                        .learner_enqueue(actor, input, available_at, scope),
                )
                .await;
            self.after_scoped(result, scope)
        })
    }

    fn claim_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        budget: LearnerBudget,
        now: DateTime<Utc>,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<LearnerReviewJob>, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            check_scope(scope)?;
            let result = scope
                .run(self.vault.learner_claim_review(actor, budget, now, scope))
                .await;
            self.after_scoped(result, scope)
        })
    }

    fn settle_review<'a>(
        &'a self,
        actor: &'a OwnerActor,
        job_id: Uuid,
        expected_attempt: u8,
        settlement: LearnerJobSettlement,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            // Settlement is a durable acknowledgement. It deliberately does
            // not inherit the cancelled execution's scope.
            let result = self
                .vault
                .learner_settle_review(actor, job_id, expected_attempt, settlement, now)
                .await;
            self.after_durable(result)
        })
    }
}

impl<Keys: VaultKeyProvider + 'static> LearnerEvidenceRepository
    for VaultKnowledgeRepository<Keys>
{
    fn read_claim<'a>(
        &'a self,
        actor: &'a OwnerActor,
        claim: LearnerClaimRef,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<LearnerReviewInput, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            check_scope(scope)?;
            let result = scope
                .run(self.vault.learner_read_claim(actor, claim, scope))
                .await;
            self.after_scoped(result, scope)
        })
    }
}

fn check_scope(scope: &ExecutionScope) -> Result<(), AgentFailure> {
    if scope.cancellation().is_cancelled() {
        Err(AgentFailure::Cancelled)
    } else if tokio::time::Instant::now() >= scope.deadline() {
        Err(AgentFailure::DeadlineExceeded)
    } else {
        Ok(())
    }
}
