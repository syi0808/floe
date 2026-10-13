//! Bounded execution of an already admitted summary. Admission/trigger policy
//! belongs to the Conversation lifecycle; this driver never creates new work.
use std::{sync::Arc, time::Duration};

use floe_agent_contract::{
    AgentFailure, AllowedCatalog, Artifact, BoxFuture, DelegationPort, DelegationRequest,
    EngineRequest, ModelPort, ModelSelectionState, TaskReceipt, ToolCall, ToolInvocationOutcome,
    ToolPort,
};
use floe_agent_runtime::{
    Engine, EngineConfig, EngineOutcome, EnginePorts, FinalPayloadValidator, ValidatedFinalPayload,
};
use floe_context::{DependencyResolver, EvidenceReader};
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_kernel::{OwnerActor, TraceContext};

use crate::{
    COMPACTION_CONSUMER, COMPACTION_ROLE_ID, CompactionClaim, CompactionDisposition,
    CompactionModelProjection, CompactionReceipt, CompactionRecovery, CompactionRepository,
    CompactionSource, compaction_role_spec, compaction_seed, recover_compaction_journal,
    validate_compaction_summary,
};

/// The lifecycle owns one driver. Mutable access serializes starts and recovery
/// within that owner; durable generation fencing remains the cross-host guard.
pub struct CompactionDriver {
    actor: OwnerActor,
    repository: Arc<dyn CompactionRepository>,
    evidence: Arc<dyn EvidenceReader>,
    resolver: Arc<dyn DependencyResolver>,
    model: Arc<dyn ModelPort>,
    maximum_duration: Duration,
}
impl CompactionDriver {
    pub fn new(
        actor: OwnerActor,
        repository: Arc<dyn CompactionRepository>,
        evidence: Arc<dyn EvidenceReader>,
        resolver: Arc<dyn DependencyResolver>,
        model: Arc<dyn ModelPort>,
        maximum_duration: Duration,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        if maximum_duration.is_zero() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(Self {
            actor,
            repository,
            evidence,
            resolver,
            model,
            maximum_duration,
        })
    }

    /// Recorded output is installed without another model call. An interrupted
    /// attempt is retired, never inferred to be safe to repeat from its error.
    pub async fn drive(
        &mut self,
        claim: CompactionClaim,
        cancellation: Cancellation,
    ) -> Result<CompactionReceipt, AgentFailure> {
        claim.validate()?;
        let job = self.repository.load_compaction(&self.actor, claim).await?;
        job.authorize_actor(&self.actor)?;
        if job.claim != claim {
            return Err(AgentFailure::Conflict);
        }
        match &job.disposition {
            CompactionDisposition::Committed { .. } => {
                return self.complete(claim).await;
            }
            CompactionDisposition::Retired { failure } => return Err(*failure),
            CompactionDisposition::Pending => {}
        }
        match recover_compaction_journal(&job.head, &job.source, &job.entries)? {
            CompactionRecovery::Output(_) => {
                return self.complete(claim).await;
            }
            CompactionRecovery::Interrupted => {
                return self.retire(claim, AgentFailure::Interrupted).await;
            }
            CompactionRecovery::NotStarted => {}
        }
        let budget = BudgetLedger::new(
            BudgetConfig::new(job.head.max_tokens, job.head.max_cost_micros),
            ModelUsage::default(),
        );
        let deadline = tokio::time::Instant::now()
            .checked_add(self.maximum_duration)
            .ok_or(AgentFailure::InvalidInput)?;
        // A summary is not a product Run or Expert Task. Do not invent either
        // identity just to obtain a trace or a budget lease.
        let scope = ExecutionScope::root(
            cancellation,
            deadline,
            budget.work_lease(),
            TraceContext::new(claim.execution_id),
        );
        let projection = CompactionModelProjection::new(
            self.actor.clone(),
            claim,
            self.repository.clone(),
            self.evidence.clone(),
            self.resolver.clone(),
        )?;
        let journal = self
            .repository
            .compaction_journal(self.actor.clone(), claim)?;
        let result = Engine::new(EngineConfig {
            max_attempt_tokens: job.head.max_tokens,
            max_attempt_cost_micros: job.head.max_cost_micros,
            max_task_tokens: job.head.max_tokens,
            max_task_cost_micros: job.head.max_cost_micros,
            max_tool_calls: 0,
            max_delegations: 0,
        })
        .drive(
            EngineRequest {
                execution_id: claim.execution_id,
                principal: self.actor.person_id.to_string(),
                device_id: self.actor.device_id.clone(),
                scope,
                role_spec: compaction_role_spec(),
                conversation: compaction_seed(claim.execution_id),
                allowed_catalog: AllowedCatalog {
                    revision: 1,
                    tools: vec![],
                    cards: vec![],
                },
                purpose: crate::CONVERSATION_PURPOSE.into(),
                consumer: COMPACTION_CONSUMER.into(),
                max_iterations: 1,
                max_output_bytes: job.head.max_output_bytes,
                replay: vec![],
                resume: None,
                model_selection: ModelSelectionState::Fresh,
                delegation_context: None,
            },
            EnginePorts {
                projection: &projection,
                model: self.model.as_ref(),
                tools: &NoEffects,
                delegation: &NoEffects,
                journal: journal.as_ref(),
                validator: &SummaryValidator {
                    source: &job.source,
                    limit: job.head.max_output_bytes,
                },
            },
        )
        .await;
        // Read and settle durable accounting even after execution cancellation.
        // A failed read leaves custody pending for recovery, never re-dispatches.
        let recorded = self.repository.load_compaction(&self.actor, claim).await?;
        recorded.authorize_actor(&self.actor)?;
        if recorded.claim != claim || recorded.source != job.source {
            return Err(AgentFailure::Conflict);
        }
        match recover_compaction_journal(&recorded.head, &recorded.source, &recorded.entries)? {
            CompactionRecovery::Output(text) => {
                if let Ok(EngineOutcome::Completed(report)) = &result {
                    if report.output.as_ref() != Some(&text) {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
                // Lost output ACKs are recovered from custody, not by inference.
                self.complete(claim).await
            }
            CompactionRecovery::Interrupted => self.retire(claim, AgentFailure::Interrupted).await,
            CompactionRecovery::NotStarted => {
                let failure = match result {
                    Err(failure) => failure,
                    Ok(EngineOutcome::Blocked(_)) => AgentFailure::AccessReviewRequired,
                    Ok(EngineOutcome::Completed(_)) => AgentFailure::Stalled,
                };
                self.retire(claim, failure).await
            }
        }
    }

    async fn complete(&self, claim: CompactionClaim) -> Result<CompactionReceipt, AgentFailure> {
        match self
            .repository
            .complete_compaction(&self.actor, claim)
            .await
        {
            Ok(receipt) => Ok(receipt),
            Err(
                failure @ (AgentFailure::Conflict
                | AgentFailure::StaleContext
                | AgentFailure::NotFound),
            ) => {
                // A changed historical source cannot become current again by
                // waiting. Retire it so discovery cannot loop on the stale job.
                // A newer executor still fences this write in the repository.
                self.retire(claim, failure).await
            }
            Err(failure) => Err(failure),
        }
    }

    async fn retire(
        &self,
        claim: CompactionClaim,
        failure: AgentFailure,
    ) -> Result<CompactionReceipt, AgentFailure> {
        self.repository
            .retire_compaction(&self.actor, claim, failure)
            .await?;
        Err(failure)
    }
}

struct NoEffects;
impl ToolPort for NoEffects {
    fn invoke<'a>(
        &'a self,
        _: ToolCall,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolInvocationOutcome, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::CapabilityDenied) })
    }
}
impl DelegationPort for NoEffects {
    fn delegate<'a>(
        &'a self,
        _: DelegationRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::CapabilityDenied) })
    }
}
struct SummaryValidator<'a> {
    source: &'a CompactionSource,
    limit: usize,
}
impl FinalPayloadValidator for SummaryValidator<'_> {
    fn validate(
        &self,
        role: &str,
        text: &str,
        artifacts: &[Artifact],
    ) -> Result<ValidatedFinalPayload, AgentFailure> {
        if role != COMPACTION_ROLE_ID || !artifacts.is_empty() {
            return Err(AgentFailure::InvalidModelOutput);
        }
        validate_compaction_summary(self.source, text, self.limit)?;
        Ok(ValidatedFinalPayload {
            text: text.to_owned(),
            artifacts: vec![],
        })
    }
}
