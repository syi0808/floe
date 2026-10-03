//! Claim identity and the authenticated head of the sole Learner execution journal.
use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, JournalEntry, JournalEvent, OwnerActor, PersonId, RunId};
use floe_agent_runtime::{JournalExecutionBinding, JournalProjection, JournalProjectionMode,
    journal_digest, project_execution_journal};
use serde::{Deserialize, Serialize};
use crate::*;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerJournalHead {
    pub claim: LearnerClaimRef,
    pub person_id: PersonId,
    pub device_id: String,
    pub budget: LearnerBudget,
    pub journal_revision: u64,
    pub journal_digest: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct LearnerClaimJournal {
    pub head: LearnerJournalHead,
    pub entries: Vec<JournalEntry>,
}
impl LearnerJournalHead {
    pub fn new(job: &LearnerReviewJob, budget: LearnerBudget) -> Result<Self, AgentFailure> {
        validate_learner_budget(&budget)?;
        validate_learner_job_lifecycle(&job.lifecycle())?;
        if job.state != LearnerJobState::Running || job.input.run_id != job.id {
            return Err(AgentFailure::Conflict);
        }
        Ok(Self { claim: LearnerClaimRef { job_id: job.id, claim_attempt: job.attempts },
            person_id: job.input.person_id,
            device_id: job.claimed_device_id.clone().ok_or(AgentFailure::Conflict)?, budget,
            journal_revision: 0, journal_digest: journal_digest(&[])? })
    }
    fn binding(&self) -> Result<JournalExecutionBinding, AgentFailure> {
        self.claim.validate()?;
        validate_learner_budget(&self.budget)?;
        if !self.person_id.is_valid() || self.device_id.trim().is_empty() || self.device_id.len() > 128
            || self.journal_revision > 64 || self.journal_digest == [0; 32]
        { return Err(AgentFailure::StorageUnavailable); }
        Ok(JournalExecutionBinding { principal: self.person_id.to_string(), device_id: self.device_id.clone(),
            execution_id: self.claim.execution_id(), catalog_revision: 1,
            root_run_id: RunId::from_uuid(self.claim.job_id), owning_task_id: None })
    }
}

pub fn validate_learner_budget(budget: &LearnerBudget) -> Result<(), AgentFailure> {
    if budget.max_input_bytes == 0 || budget.max_input_bytes > 16 * 1024
        || budget.max_output_bytes == 0 || budget.max_output_bytes > 4 * 1024
        || budget.max_model_tokens == 0 || budget.max_model_cost_micros == 0
        || budget.deadline_ms == 0 || budget.deadline_ms > 30_000
    { return Err(AgentFailure::InvalidInput); }
    Ok(())
}

fn project(head: &LearnerJournalHead, entries: &[JournalEntry]) -> Result<JournalProjection, AgentFailure> {
    if entries.len() > 64 || serde_json::to_vec(entries).map_err(|_| AgentFailure::StorageUnavailable)?.len() > 512 * 1024 {
        return Err(AgentFailure::BudgetExceeded);
    }
    let binding = head.binding()?;
    for (index, entry) in entries.iter().enumerate() {
        match &entry.event {
            JournalEvent::ModelIntent { parent_task_id, plan, reservation_ceiling, .. } => {
                if parent_task_id.is_some() || plan.consumer != LEARNER_INFERENCE_CONSUMER
                    || plan.purpose != LEARNER_INFERENCE_PURPOSE
                { return Err(AgentFailure::PolicyDenied); }
                let prior = project_execution_journal(&binding, &entries[..index], JournalProjectionMode::DurablePrefix)?;
                if prior.usage.tokens.checked_add(reservation_ceiling.tokens)
                    .is_none_or(|total| total > head.budget.max_model_tokens)
                    || prior.usage.cost_micros.checked_add(reservation_ceiling.cost_micros)
                        .is_none_or(|total| total > head.budget.max_model_cost_micros)
                { return Err(AgentFailure::BudgetExceeded); }
            }
            JournalEvent::ModelResult { .. } | JournalEvent::BatchProgress { .. } | JournalEvent::Checkpoint { .. } => {}
            JournalEvent::ValidatedBatch { batch } => {
                if batch.steps.iter().any(|step| !matches!(step,
                    floe_agent_contract::ModelStep::Preamble { .. } | floe_agent_contract::ModelStep::Answer { .. }))
                    || batch.steps.iter().any(|step| matches!(step, floe_agent_contract::ModelStep::Answer { artifacts, .. } if !artifacts.is_empty()))
                { return Err(AgentFailure::CapabilityDenied); }
            }
            JournalEvent::Output { text, artifacts } => {
                if !artifacts.is_empty() || text.len() > head.budget.max_output_bytes {
                    return Err(AgentFailure::InvalidModelOutput);
                }
                parse_learner_review_output(text)?;
            }
            _ => return Err(AgentFailure::CapabilityDenied),
        }
    }
    let projection = project_execution_journal(&binding, entries, JournalProjectionMode::DurablePrefix)?;
    if entries.len() + projection.unresolved_attempts.len() > 64 {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(projection)
}

pub fn validate_learner_journal(head: &LearnerJournalHead, entries: &[JournalEntry])
    -> Result<JournalProjection, AgentFailure>
{
    let projection = project(head, entries)?;
    if projection.journal_revision != head.journal_revision || projection.journal_digest != head.journal_digest {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(projection)
}

/// Call with the entire next prefix; storage atomically persists this head and its one new event.
pub fn advance_learner_journal(head: &LearnerJournalHead, entries: &[JournalEntry])
    -> Result<LearnerJournalHead, AgentFailure>
{
    let (_, previous) = entries.split_last().ok_or(AgentFailure::InvalidInput)?;
    validate_learner_journal(head, previous)?;
    let projection = project(head, entries)?;
    let mut next = head.clone();
    next.journal_revision = projection.journal_revision;
    next.journal_digest = projection.journal_digest;
    Ok(next)
}

pub fn recover_learner_claim(job: &LearnerReviewJob, journal: &LearnerClaimJournal,
    device_id: &str, now: DateTime<Utc>) -> Result<LearnerJobLifecycle, AgentFailure>
{
    validate_claim_identity(job, &journal.head)?;
    if job.state != LearnerJobState::Running || job.available_at > now { return Err(AgentFailure::Conflict); }
    let projection = validate_learner_journal(&journal.head, &journal.entries)?;
    let reusable = device_id == journal.head.device_id && !uncertain(&journal.entries, &projection)
        && (journal.entries.is_empty() || projection.output.is_some());
    if !reusable {
        return settle_learner_job(&job.lifecycle(), job.attempts,
            LearnerJobSettlement::Failed { failure: AgentFailure::Interrupted }, now);
    }
    let mut resumed = job.lifecycle();
    resumed.available_at = now + chrono::Duration::seconds(LEARNER_JOB_LEASE_SECONDS);
    // Same immutable claim, original claim time, device and allowance.
    Ok(resumed)
}

pub fn validate_learner_stage(actor: &OwnerActor, job: &LearnerReviewJob, journal: &LearnerClaimJournal,
    expected_revision: u64, expected_digest: [u8; 32], request: &StageMemoryCandidate)
    -> Result<(), AgentFailure>
{
    actor.validate()?;
    validate_claim_identity(job, &journal.head)?;
    let projection = validate_learner_journal(&journal.head, &journal.entries)?;
    if job.state != LearnerJobState::Running || actor.person_id != journal.head.person_id
        || actor.device_id != journal.head.device_id || expected_revision != journal.head.journal_revision
        || expected_digest != journal.head.journal_digest || uncertain(&journal.entries, &projection)
    { return Err(AgentFailure::PolicyDenied); }
    let (text, _) = projection.output.ok_or(AgentFailure::Conflict)?;
    let proposal = parse_learner_review_output(&text)?.ok_or(AgentFailure::InvalidInput)?;
    let expected = super::learner_service::stage_request(&job.input, proposal);
    if &expected != request { return Err(AgentFailure::Conflict); }
    Ok(())
}

fn validate_claim_identity(job: &LearnerReviewJob, head: &LearnerJournalHead) -> Result<(), AgentFailure> {
    validate_learner_input(&job.input, head.person_id)?;
    validate_learner_job_lifecycle(&job.lifecycle())?;
    if job.id != head.claim.job_id || job.input.run_id != job.id || job.attempts != head.claim.claim_attempt
        || job.claimed_device_id.as_deref() != Some(head.device_id.as_str())
    { return Err(AgentFailure::Conflict); }
    Ok(())
}
fn uncertain(entries: &[JournalEntry], projection: &JournalProjection) -> bool {
    !projection.unresolved_attempts.is_empty() || entries.iter().any(|entry| matches!(
        &entry.event, JournalEvent::ModelResult { accounting, .. } if accounting.unknown_tokens || accounting.unknown_cost))
}
