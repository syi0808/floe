use std::sync::Arc;
use floe_agent_contract::{AgentFailure, AllowedCatalog, Artifact, BoxFuture, DelegationPort,
    DelegationRequest, EngineRequest, ExecutionScope, ModelConversation, ModelPort,
    ModelProjectionOutcome, ModelProjectionPort, ModelProjectionRequest, OwnerActor,
    RoleSpec, RunId, TaskReceipt, ToolCall, ToolInvocationOutcome, ToolPort, TraceContext,
    ValidatedFinalPayload};
use floe_agent_runtime::{Engine, EngineBlockage, EngineConfig, EngineOutcome, EnginePorts,
    FinalPayloadValidator};
use floe_execution::{Cancellation, budget::{BudgetConfig, BudgetLedger, ModelUsage}};
use crate::{KnowledgeRepository, LearnerBudget, LearnerClaimRef, LearnerJobRepository,
    LearnerJournalFactory, LearnerProjectionBounds, LearnerProjectionPort, LearnerProjectionRequest,
    LearnerReviewJob, MemoryStageOrigin, MemoryStageRequest};

pub struct LearnerService {
    actor: OwnerActor,
    knowledge: Arc<dyn KnowledgeRepository>,
    jobs: Arc<dyn LearnerJobRepository>,
    journals: Arc<dyn LearnerJournalFactory>,
    model: Arc<dyn ModelPort>,
    projection: Arc<dyn LearnerProjectionPort>,
    budget: LearnerBudget,
    clock: Arc<dyn crate::KnowledgeClock>,
    generation: Cancellation,
    closing: std::sync::atomic::AtomicBool,
    active: tokio::sync::Mutex<()>,
}
impl LearnerService {
    pub fn new(actor: OwnerActor, knowledge: Arc<dyn KnowledgeRepository>, jobs: Arc<dyn LearnerJobRepository>,
        journals: Arc<dyn LearnerJournalFactory>, model: Arc<dyn ModelPort>,
        projection: Arc<dyn LearnerProjectionPort>, clock: Arc<dyn crate::KnowledgeClock>, budget: LearnerBudget) -> Result<Self, AgentFailure>
    {
        actor.validate()?;
        if budget.max_input_bytes == 0 || budget.max_input_bytes > 16 * 1024
            || budget.max_output_bytes == 0 || budget.max_output_bytes > 4 * 1024
            || budget.max_model_tokens == 0 || budget.max_model_cost_micros == 0
            || budget.deadline_ms == 0 || budget.deadline_ms > 30_000
        { return Err(AgentFailure::InvalidInput); }
        Ok(Self { actor, knowledge, jobs, journals, model, projection, budget, clock,
            generation: Cancellation::default(), closing: std::sync::atomic::AtomicBool::new(false),
            active: tokio::sync::Mutex::new(()) })
    }

    pub async fn run_next(&self, cancellation: Cancellation) -> Result<bool, AgentFailure> {
        let _active = self.active.lock().await;
        if self.closing.load(std::sync::atomic::Ordering::Acquire) { return Err(AgentFailure::Interrupted); }
        let child = self.generation.child_scope();
        let future = self.execute_next(child.clone());
        tokio::pin!(future);
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                child.cancel_with_reason(cancellation.reason().unwrap_or(floe_execution::CancelReason::User));
                future.await
            }
            result = &mut future => result,
        }
    }

    pub fn close_admission(&self) {
        self.closing.store(true, std::sync::atomic::Ordering::Release);
        self.generation.cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
    }

    pub async fn shutdown(&self) -> Result<(), AgentFailure> {
        self.close_admission();
        tokio::time::timeout(std::time::Duration::from_secs(5), self.active.lock()).await
            .map(|_| ()).map_err(|_| AgentFailure::DeadlineExceeded)
    }

    pub(crate) async fn drive_background(&self, scheduling: crate::LearnerScheduling) {
        let mut delay = std::time::Duration::from_millis(750);
        loop {
            tokio::select! {
                _ = self.generation.cancelled() => return,
                _ = tokio::time::sleep(delay) => {}
            }
            if self.closing.load(std::sync::atomic::Ordering::Acquire) { return; }
            let lease = match scheduling.try_start() {
                Ok(Some(lease)) => lease,
                Ok(None) => { delay = std::time::Duration::from_millis(750); continue; }
                Err(_) => { delay = std::time::Duration::from_secs(5); continue; }
            };
            delay = match self.run_next(lease.cancellation()).await {
                Ok(true) => std::time::Duration::from_millis(750),
                Ok(false) => std::time::Duration::from_secs(30),
                Err(_) => std::time::Duration::from_secs(5),
            };
            drop(lease);
        }
    }

    async fn execute_next(&self, cancellation: Cancellation) -> Result<bool, AgentFailure> {
        if cancellation.is_cancelled() { return Ok(false); }
        let discovery_id = uuid::Uuid::new_v4();
        let discovery_scope = self.scope(discovery_id, cancellation.clone())?;
        self.discover_explicit_reviews(&discovery_scope).await?;
        if cancellation.is_cancelled() { return Ok(false); }
        let Some(job) = self.jobs.claim_review(&self.actor, self.budget.clone(), self.clock.now(), &discovery_scope).await?
            else { return Ok(false); };
        self.review_claimed(&job, cancellation).await
    }

    pub async fn discover_explicit_reviews(&self, scope: &ExecutionScope) -> Result<(), AgentFailure> {
        let now = self.clock.now();
        let sessions = self.jobs.discovery_sessions(&self.actor, 64, scope).await?;
        if sessions.len() > 64 { return Err(AgentFailure::BudgetExceeded); }
        let memories = self.knowledge.read_context(&self.actor, now, scope).await?;
        let mut count = 0;
        for session in sessions {
            if session.evidence.person_id != self.actor.person_id { return Err(AgentFailure::PolicyDenied); }
            if let Some(input) = super::discovery::explicit_review_input(&session, &memories.memories, now)? {
                match self.jobs.enqueue(&self.actor, input, now, scope).await {
                    Ok(_) => count += 1,
                    Err(AgentFailure::Conflict | AgentFailure::StaleContext) => continue,
                    Err(error) => return Err(error),
                }
                if count == 8 { break; }
            }
        }
        Ok(())
    }

    fn scope(&self, id: uuid::Uuid, cancellation: Cancellation) -> Result<ExecutionScope, AgentFailure> {
        Self::scope_with_budget(&self.budget, id, cancellation)
    }
    fn scope_with_budget(budget: &LearnerBudget, id: uuid::Uuid, cancellation: Cancellation) -> Result<ExecutionScope, AgentFailure> {
        let ledger = BudgetLedger::new(BudgetConfig::new(budget.max_model_tokens,
            budget.max_model_cost_micros), ModelUsage::default());
        Ok(ExecutionScope::root(cancellation,
            tokio::time::Instant::now() + std::time::Duration::from_millis(budget.deadline_ms),
            ledger.work_lease(), TraceContext::new(id).with_run_id(RunId::from_uuid(id).ok_or(AgentFailure::InvalidInput)?)))
    }

    async fn review_claimed(&self, job: &LearnerReviewJob, cancellation: Cancellation)
        -> Result<bool, AgentFailure>
    {
        crate::validate_learner_input(&job.input, self.actor.person_id)?;
        let claim = LearnerClaimRef { job_id: job.id, claim_attempt: job.attempts };
        claim.validate()?;
        if job.input.run_id != job.id || job.state != crate::LearnerJobState::Running
            || job.claimed_device_id.as_deref() != Some(self.actor.device_id.as_str())
        { return Err(AgentFailure::Conflict); }
        let journal = self.journals.journal(self.actor.person_id, claim)?;
        let prior = self.journals.load_journal(self.actor.person_id, claim).await?;
        if prior.head.claim != claim || prior.head.person_id != self.actor.person_id
            || prior.head.device_id != self.actor.device_id
        { return Err(AgentFailure::Conflict); }
        crate::validate_learner_journal(&prior.head, &prior.entries)?;
        let budget = prior.head.budget.clone();
        let scope = Self::scope_with_budget(&budget, job.id, cancellation)?;
        let evidence_refs = job.input.turn_ids.iter().map(|turn_id| crate::LearningEvidenceRef {
            session_id: job.input.session_id, turn_id: *turn_id,
        }).collect();
        let projector = ClaimProjection { owner: self, claim, evidence_refs, budget: budget.clone(),
            expires_at: self.clock.now() + chrono::Duration::milliseconds(budget.deadline_ms as i64) };
        let result = if prior.entries.is_empty() { Some(Engine::new(EngineConfig {
            max_attempt_tokens: budget.max_model_tokens,
            max_attempt_cost_micros: budget.max_model_cost_micros,
            max_task_tokens: budget.max_model_tokens,
            max_task_cost_micros: budget.max_model_cost_micros,
            max_tool_calls: 0, max_delegations: 0,
        }).drive(EngineRequest {
            execution_id: claim.execution_id(), principal: self.actor.person_id.to_string(),
            device_id: self.actor.device_id.clone(), scope: scope.clone(),
            role_spec: RoleSpec { role_id: "learner".into(), instructions: crate::prompts::LEARNER_ROLE.into(),
                output_contract: "One bounded candidate-only structured memory review answer.".into() },
            conversation: ModelConversation { history: vec![], current_turn: vec![
                floe_agent_contract::ModelConversationEntry::User {
                    message_id: *job.input.turn_ids.last().ok_or(AgentFailure::InvalidInput)?,
                    text: job.input.digest.clone(),
                }
            ] },
            allowed_catalog: AllowedCatalog { cards: vec![], tools: vec![], revision: 1 },
            purpose: crate::LEARNER_INFERENCE_PURPOSE.into(), consumer: crate::LEARNER_INFERENCE_CONSUMER.into(),
            max_iterations: 1, max_output_bytes: budget.max_output_bytes,
            replay: vec![], resume: None, delegation_context: None,
        }, EnginePorts { model: self.model.as_ref(), projection: &projector, tools: &NoLearnerTools,
            delegation: &NoLearnerTools, journal: journal.as_ref(), validator: &LearnerValidator { input: &job.input } }).await) } else { None };
        let recorded = if prior.entries.is_empty() { self.journals.load_journal(self.actor.person_id, claim).await? } else { prior };
        let projection = crate::validate_learner_journal(&recorded.head, &recorded.entries)?;
        let entries = &recorded.entries;
        let uncertain = (result.is_none() && projection.output.is_none())
            || !projection.unresolved_attempts.is_empty() || entries.iter().any(|entry| matches!(
            &entry.event, floe_agent_contract::JournalEvent::ModelResult { accounting, .. }
                if accounting.unknown_tokens || accounting.unknown_cost));
        let settlement = if uncertain {
            crate::LearnerJobSettlement::Failed { failure: AgentFailure::Interrupted }
        } else {
            match result {
                Some(Ok(EngineOutcome::Blocked(block))) => {
                    let EngineBlockage::ModelProjection { plan, review } = block.blockage
                        else { return Err(AgentFailure::PolicyDenied); };
                    crate::LearnerJobSettlement::Blocked { blockage: crate::LearnerProjectionBlock { plan, review } }
                }
                completed => {
                    let answer = match completed {
                        Some(Ok(EngineOutcome::Completed(report))) => {
                            if projection.output.as_ref().map(|(text, _)| text) != report.output.as_ref() {
                                return Err(AgentFailure::StorageUnavailable);
                            }
                            report.output.ok_or(AgentFailure::Stalled)
                        }
                        None => projection.output.as_ref().map(|(text, _)| text.clone())
                            .ok_or(AgentFailure::Interrupted),
                        Some(Err(error)) => Err(error),
                        Some(Ok(EngineOutcome::Blocked(_))) => unreachable!(),
                    };
                    let staged = match answer {
                        Ok(text) => self.stage_answer(job, claim, &projection, &text, &scope).await,
                        Err(error) => Err(error),
                    };
                    // A validated Output is already durable. A transient staging
                    // interruption retains this same claim for output recovery;
                    // Deferred would allocate a new claim and repeat inference.
                    if projection.output.is_some() {
                        match staged {
                            Ok(candidate_id) => crate::LearnerJobSettlement::Completed { candidate_id },
                            Err(error) if crate::retryable_learner_failure(error)
                                || matches!(error, AgentFailure::StorageUnavailable | AgentFailure::VaultUnavailable) => return Err(error),
                            Err(failure) => crate::LearnerJobSettlement::Failed { failure },
                        }
                    } else {
                        crate::settlement_for_learner_result(staged, self.clock.now())?
                    }
                }
            }
        };
        self.jobs.settle_review(&self.actor, job.id, job.attempts, settlement, self.clock.now()).await?;
        Ok(true)
    }

    async fn stage_answer(&self, job: &LearnerReviewJob, claim: LearnerClaimRef,
        projection: &floe_agent_runtime::JournalProjection, text: &str, scope: &ExecutionScope)
        -> Result<Option<uuid::Uuid>, AgentFailure>
    {
        let Some(proposal) = crate::parse_learner_review_output(text)? else { return Ok(None); };
        let candidate = self.knowledge.stage(MemoryStageRequest {
            actor: self.actor.clone(), request: stage_request(&job.input, proposal),
            origin: MemoryStageOrigin::Learner { claim,
                journal_revision: projection.journal_revision, journal_digest: projection.journal_digest },
        }, scope).await?;
        Ok(Some(candidate.id))
    }
}
impl Drop for LearnerService {
    fn drop(&mut self) { self.close_admission(); }
}

struct ClaimProjection<'a> {
    owner: &'a LearnerService,
    claim: LearnerClaimRef,
    budget: LearnerBudget,
    evidence_refs: Vec<crate::LearningEvidenceRef>,
    expires_at: chrono::DateTime<Utc>,
}
impl ModelProjectionPort for ClaimProjection<'_> {
    fn project<'a>(&'a self, request: ModelProjectionRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ModelProjectionOutcome, AgentFailure>>
    {
        Box::pin(async move {
            request.validate()?;
            if request.role.role_id != "learner" || !request.catalog.cards.is_empty() || !request.catalog.tools.is_empty()
            { return Err(AgentFailure::PolicyDenied); }
            self.owner.projection.project(LearnerProjectionRequest {
                actor: self.owner.actor.clone(), claim: self.claim,
                projection_operation_id: request.projection_operation_id, plan: request.plan,
                correction: request.correction, evidence_refs: self.evidence_refs.clone(),
                bounds: LearnerProjectionBounds { max_input_bytes: self.budget.max_input_bytes,
                    max_output_bytes: self.budget.max_output_bytes }, expires_at: self.expires_at,
            }, scope).await
        })
    }
}

struct NoLearnerTools;
impl ToolPort for NoLearnerTools {
    fn invoke<'a>(&'a self, _: ToolCall, _: &'a ExecutionScope) -> BoxFuture<'a, Result<ToolInvocationOutcome, AgentFailure>>
    { Box::pin(async { Err(AgentFailure::CapabilityDenied) }) }
}
impl DelegationPort for NoLearnerTools {
    fn delegate<'a>(&'a self, _: DelegationRequest, _: &'a ExecutionScope) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>>
    { Box::pin(async { Err(AgentFailure::CapabilityDenied) }) }
}
struct LearnerValidator<'a> { input: &'a crate::LearnerReviewInput }
impl FinalPayloadValidator for LearnerValidator<'_> {
    fn validate(&self, role: &str, text: &str, artifacts: &[Artifact]) -> Result<ValidatedFinalPayload, AgentFailure> {
        if role != "learner" || !artifacts.is_empty() { return Err(AgentFailure::InvalidModelOutput); }
        if let Some(proposal) = crate::parse_learner_review_output(text)? {
            if let Some(target_id) = proposal.target_id {
                if !self.input.current_memories.iter().any(|memory| memory.target_id == target_id
                    && Some(memory.revision) == proposal.base_revision)
                { return Err(AgentFailure::InvalidModelOutput); }
            }
            crate::validate_stage_request(&stage_request(self.input, proposal))
                .map_err(|_| AgentFailure::InvalidModelOutput)?;
        }
        Ok(ValidatedFinalPayload { text: text.to_owned(), artifacts: vec![] })
    }
}
pub(crate) fn stage_request(input: &crate::LearnerReviewInput, mut proposal: crate::LearnerMemoryProposal) -> crate::StageMemoryCandidate {
    proposal.value.observed_at = input.observed_at;
    crate::StageMemoryCandidate { session_id: input.session_id, expected_session_revision: input.session_revision,
        turn_ids: input.turn_ids.clone(), observation_kind: proposal.observation_kind, digest: input.digest.clone(),
        value: proposal.value, target_id: proposal.target_id, base_revision: proposal.base_revision,
        extractor_version: crate::prompts::LEARNER_EXTRACTOR_VERSION.into(),
        prompt_version: crate::prompts::LEARNER_PROMPT_VERSION.into(),
        actor: crate::KnowledgeActor::Learner { run_id: input.run_id }, created_at: input.observed_at }
}
