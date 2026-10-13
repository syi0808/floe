//! Capacity recovery is bounded maintenance, not a replay of a foreground Run.
use super::*;
use crate::{CompactionAdmission, CompactionAuthority, CompactionDriver, CompactionRepository};
use floe_context::{DependencyResolver, EvidenceReader};

pub(in crate::application) struct CompactionResources {
    pub model: Arc<dyn floe_agent_contract::ModelPort>,
    pub evidence: Arc<dyn EvidenceReader>,
    pub resolver: Arc<dyn DependencyResolver>,
}
impl<R: ConversationRepository + InteractionRepository + CompactionRepository + 'static>
    RunCoordinator<R>
{
    pub(super) async fn compact_before_first_model(
        &self,
        actor: &OwnerActor,
        receipt: &RunReceipt,
        scope: &ExecutionScope,
    ) -> Result<Option<crate::AdmittedTurn>, AgentFailure> {
        let _guard = scope
            .run(async { Ok(self.compaction_lock.lock().await) })
            .await?;
        if !scope
            .run(self.repository.load_journal(receipt.run_id))
            .await?
            .is_empty()
        {
            return Ok(None);
        }
        // Maintenance has its own durable journal/budget, like Learner work.
        // At most one summary execution (including its bounded correction) may
        // reach a provider here. Smaller prefixes are tried only with proof of
        // zero model intents, never after a provider error or unknown usage.
        let mut source_byte_limit = crate::COMPACTION_TRIGGER_BYTES;
        while source_byte_limit >= 1024 {
            let request = CompactionAdmission {
                authority: CompactionAuthority::BeforeFirstModel {
                    run_id: receipt.run_id,
                },
                session_id: receipt.session_id,
                expected_session_revision: receipt.session_revision,
                source_byte_limit,
                max_tokens: (self.config.budget.max_tokens / 4).clamp(1, 16_384),
                max_cost_micros: (self.config.budget.max_cost_micros / 5).min(10_000),
                max_output_bytes: 2048.min(self.config.max_output_bytes),
            };
            let job = match scope
                .run(self.repository.admit_compaction(actor, request))
                .await
            {
                Ok(Some(job)) => job,
                Ok(None) | Err(AgentFailure::BudgetExceeded) => return Ok(None),
                Err(failure) => return Err(failure),
            };
            let duration = scope
                .deadline()
                .checked_duration_since(tokio::time::Instant::now())
                .filter(|duration| !duration.is_zero())
                .ok_or(AgentFailure::DeadlineExceeded)?
                .min(Duration::from_secs(30));
            let mut driver = CompactionDriver::new(
                actor.clone(),
                self.repository.clone(),
                self.compaction.evidence.clone(),
                self.compaction.resolver.clone(),
                self.compaction.model.clone(),
                duration,
            )?;
            match driver
                .drive(job.claim, scope.cancellation().child_scope())
                .await
            {
                Ok(_) => return scope.run(self.repository.load_run(receipt.run_id)).await,
                Err(AgentFailure::ModelInputCapacityExceeded) => {
                    let recorded = scope
                        .run(self.repository.load_compaction(actor, job.claim))
                        .await?;
                    if !recorded.entries.is_empty() {
                        return Err(AgentFailure::ModelInputCapacityExceeded);
                    }
                    source_byte_limit /= 2;
                }
                Err(failure) => return Err(failure),
            }
        }
        Ok(None)
    }
}

impl<R: ConversationRepository + InteractionRepository + CompactionRepository + 'static>
    RunCoordinator<R>
{
    /// Recovery never introduces a new maintenance model call. A live driver
    /// owns the lock through settlement; orphaned claims are reconciled from
    /// their durable journal after ordinary Run recovery has been considered.
    pub(in crate::application) async fn recover_compaction_job(
        &self,
        actor: &OwnerActor,
        id: uuid::Uuid,
    ) -> Result<bool, AgentFailure> {
        let Ok(_guard) = self.compaction_lock.try_lock() else {
            return Ok(true);
        };
        let job = self.repository.recover_compaction(actor, id).await?;
        if job.disposition != crate::CompactionDisposition::Pending {
            return Ok(false);
        }
        match crate::recover_compaction_journal(&job.head, &job.source, &job.entries)? {
            crate::CompactionRecovery::Output(_) => {
                match self.repository.complete_compaction(actor, job.claim).await {
                    Ok(_) => {}
                    Err(
                        failure @ (AgentFailure::Conflict
                        | AgentFailure::StaleContext
                        | AgentFailure::NotFound),
                    ) => {
                        self.repository
                            .retire_compaction(actor, job.claim, failure)
                            .await?;
                    }
                    Err(failure) => return Err(failure),
                }
            }
            crate::CompactionRecovery::NotStarted | crate::CompactionRecovery::Interrupted => {
                self.repository
                    .retire_compaction(actor, job.claim, AgentFailure::Interrupted)
                    .await?;
            }
        }
        Ok(false)
    }
}
