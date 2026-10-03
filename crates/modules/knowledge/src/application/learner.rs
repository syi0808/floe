use std::collections::HashSet;

use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{LearningObservationKind, PersonalMemoryValue};

pub const LEARNER_JOB_LEASE_SECONDS: i64 = 30;
pub const MAX_LEARNER_JOB_ATTEMPTS: u8 = 3;
pub const LEARNER_JOB_RETRY_DELAY_SECONDS: i64 = 5;

/// The Inference purpose/consumer one Learner review runs under.
///
/// Knowledge owns the scope; Inference selects the device profile behind it
/// and Access fences the dispatch. The Learner never names a route.
pub const LEARNER_INFERENCE_PURPOSE: &str = "everyday_assistance";
pub const LEARNER_INFERENCE_CONSUMER: &str = "knowledge.learner";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerMemoryProposal {
    pub observation_kind: LearningObservationKind,
    pub value: PersonalMemoryValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<u64>,
}

/// Parse the single structured answer one Learner review accepts.
///
/// The answer must name the current Knowledge schema and carry an explicit
/// proposal, even when there is nothing to remember. Unknown fields,
/// duplicate fields and a missing proposal fail closed: the model did
/// something this role never asked for.
pub fn parse_learner_review_output(
    text: &str,
) -> Result<Option<LearnerMemoryProposal>, AgentFailure> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
    if !value
        .as_object()
        .is_some_and(|object| object.contains_key("proposal"))
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    let answer: StructuredLearnerAnswer =
        serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
    if answer.schema_version != crate::KNOWLEDGE_VERSION {
        return Err(AgentFailure::InvalidModelOutput);
    }
    Ok(answer.proposal)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StructuredLearnerAnswer {
    schema_version: u32,
    proposal: Option<LearnerMemoryProposal>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerBudget {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_model_tokens: u64,
    pub max_model_cost_micros: u64,
    pub deadline_ms: u64,
}

impl Default for LearnerBudget {
    fn default() -> Self {
        Self {
            max_input_bytes: 16 * 1024,
            max_output_bytes: 4 * 1024,
            max_model_tokens: 8_192,
            max_model_cost_micros: 50_000,
            deadline_ms: 15_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearnerJobState {
    Queued,
    Running,
    Deferred,
    Completed,
    Blocked,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LearnerJobSettlement {
    Blocked { blockage: LearnerProjectionBlock },
    Completed {
        candidate_id: Option<Uuid>,
    },
    Deferred {
        available_at: DateTime<Utc>,
        failure: AgentFailure,
    },
    Failed {
        failure: AgentFailure,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerProjectionBlock {
    pub plan: floe_agent_contract::PreparedModelPlan,
    pub review: floe_agent_contract::SourceProjectionReview,
}
impl LearnerProjectionBlock {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.plan.validate()?;
        self.review.validate()?;
        if self.plan.consumer != LEARNER_INFERENCE_CONSUMER || self.plan.purpose != LEARNER_INFERENCE_PURPOSE {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearnerJobLifecycle {
    pub state: LearnerJobState,
    pub attempts: u8,
    pub available_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub candidate_id: Option<Uuid>,
    pub last_failure: Option<AgentFailure>,
    pub blocked: Option<LearnerProjectionBlock>,
    pub claimed_device_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LearnerJobClaim {
    Claimed(LearnerJobLifecycle),
    Exhausted(LearnerJobLifecycle),
}

pub fn claim_learner_job(
    lifecycle: &LearnerJobLifecycle,
    now: DateTime<Utc>,
    device_id: &str,
) -> Result<LearnerJobClaim, AgentFailure> {
    validate_learner_job_lifecycle(lifecycle)?;
    if device_id.trim().is_empty() || device_id.len() > 128 { return Err(AgentFailure::InvalidInput); }
    if !matches!(
        lifecycle.state,
        LearnerJobState::Queued | LearnerJobState::Deferred
    ) || lifecycle.available_at > now
    {
        return Err(AgentFailure::Conflict);
    }
    if lifecycle.attempts >= MAX_LEARNER_JOB_ATTEMPTS {
        let mut exhausted = lifecycle.clone();
        exhausted.state = LearnerJobState::Failed;
        exhausted.finished_at = Some(now);
        exhausted.last_failure = Some(AgentFailure::Stalled);
        return Ok(LearnerJobClaim::Exhausted(exhausted));
    }
    let mut claimed = lifecycle.clone();
    claimed.state = LearnerJobState::Running;
    claimed.attempts = lifecycle
        .attempts
        .checked_add(1)
        .ok_or(AgentFailure::VaultUnavailable)?;
    claimed.claimed_at = Some(now);
    claimed.available_at = now + chrono::Duration::seconds(LEARNER_JOB_LEASE_SECONDS);
    claimed.finished_at = None;
    claimed.candidate_id = None;
    claimed.last_failure = None;
    claimed.blocked = None;
    claimed.claimed_device_id = Some(device_id.to_owned());
    Ok(LearnerJobClaim::Claimed(claimed))
}

pub fn settle_learner_job(
    lifecycle: &LearnerJobLifecycle,
    expected_attempt: u8,
    settlement: LearnerJobSettlement,
    settled_at: DateTime<Utc>,
) -> Result<LearnerJobLifecycle, AgentFailure> {
    validate_learner_job_lifecycle(lifecycle)?;
    if lifecycle.state != LearnerJobState::Running || lifecycle.attempts != expected_attempt {
        return Err(AgentFailure::Conflict);
    }
    let mut settled = lifecycle.clone();
    match settlement {
        LearnerJobSettlement::Blocked { blockage } => {
            blockage.validate()?;
            settled.state = LearnerJobState::Blocked;
            settled.finished_at = Some(settled_at);
            settled.candidate_id = None;
            settled.last_failure = None;
            settled.blocked = Some(blockage);
        }
        LearnerJobSettlement::Completed { candidate_id } => {
            settled.state = LearnerJobState::Completed;
            settled.finished_at = Some(settled_at);
            settled.candidate_id = candidate_id;
            settled.last_failure = None;
        }
        LearnerJobSettlement::Deferred {
            available_at,
            failure,
        } => {
            if available_at <= settled_at || !retryable_learner_failure(failure) {
                return Err(AgentFailure::InvalidInput);
            }
            if settled.attempts >= MAX_LEARNER_JOB_ATTEMPTS {
                settled.state = LearnerJobState::Failed;
                settled.finished_at = Some(settled_at);
                settled.last_failure = Some(failure);
            } else {
                settled.state = LearnerJobState::Deferred;
                settled.available_at = available_at;
                settled.claimed_at = None;
                settled.last_failure = Some(failure);
            }
        }
        LearnerJobSettlement::Failed { failure } => {
            settled.state = LearnerJobState::Failed;
            settled.finished_at = Some(settled_at);
            settled.last_failure = Some(failure);
        }
    }
    Ok(settled)
}

pub fn reject_learner_claim(
    lifecycle: &LearnerJobLifecycle,
    failure: AgentFailure,
    failed_at: DateTime<Utc>,
) -> Result<LearnerJobLifecycle, AgentFailure> {
    if !matches!(
        failure,
        AgentFailure::StaleContext | AgentFailure::NotFound | AgentFailure::PolicyDenied
    ) {
        return Err(failure);
    }
    validate_learner_job_lifecycle(lifecycle)?;
    if !matches!(
        lifecycle.state,
        LearnerJobState::Queued | LearnerJobState::Deferred | LearnerJobState::Running
    ) || lifecycle.available_at > failed_at
    {
        return Err(AgentFailure::Conflict);
    }
    let mut rejected = lifecycle.clone();
    rejected.state = LearnerJobState::Failed;
    rejected.finished_at = Some(failed_at);
    rejected.last_failure = Some(failure);
    Ok(rejected)
}

pub const fn retryable_learner_failure(failure: AgentFailure) -> bool {
    matches!(
        failure,
        AgentFailure::Cancelled
            | AgentFailure::DeadlineExceeded
            | AgentFailure::ModelUnavailable
            | AgentFailure::LocalModelUnavailable
            | AgentFailure::QuotaExceeded
            | AgentFailure::Interrupted
    )
}

pub fn validate_learner_job_lifecycle(lifecycle: &LearnerJobLifecycle) -> Result<(), AgentFailure> {
    if lifecycle.attempts > MAX_LEARNER_JOB_ATTEMPTS
        || (lifecycle.state == LearnerJobState::Blocked) != lifecycle.blocked.is_some()
        || lifecycle.blocked.as_ref().is_some_and(|blockage| blockage.validate().is_err())
        || (lifecycle.attempts > 0) != lifecycle.claimed_device_id.is_some()
        || lifecycle.claimed_device_id.as_ref().is_some_and(|device| device.trim().is_empty() || device.len() > 128)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let valid = match lifecycle.state {
        LearnerJobState::Queued => {
            lifecycle.attempts == 0
                && lifecycle.claimed_at.is_none()
                && lifecycle.finished_at.is_none()
                && lifecycle.candidate_id.is_none()
                && lifecycle.last_failure.is_none()
        }
        LearnerJobState::Running => {
            (1..=MAX_LEARNER_JOB_ATTEMPTS).contains(&lifecycle.attempts)
                && lifecycle.claimed_at.is_some()
                && lifecycle.finished_at.is_none()
                && lifecycle.candidate_id.is_none()
                && lifecycle.last_failure.is_none()
        }
        LearnerJobState::Deferred => {
            (1..MAX_LEARNER_JOB_ATTEMPTS).contains(&lifecycle.attempts)
                && lifecycle.claimed_at.is_none()
                && lifecycle.finished_at.is_none()
                && lifecycle.candidate_id.is_none()
                && lifecycle
                    .last_failure
                    .is_some_and(retryable_learner_failure)
        }
        LearnerJobState::Completed => {
            lifecycle.attempts > 0
                && lifecycle.finished_at.is_some()
                && lifecycle.last_failure.is_none()
        }
        LearnerJobState::Blocked => {
            lifecycle.attempts > 0 && lifecycle.finished_at.is_some()
                && lifecycle.last_failure.is_none() && lifecycle.candidate_id.is_none()
        }
        LearnerJobState::Failed => {
            lifecycle.finished_at.is_some()
                && lifecycle.candidate_id.is_none()
                && lifecycle.last_failure.is_some()
        }
    };
    valid.then_some(()).ok_or(AgentFailure::VaultUnavailable)
}

pub fn settlement_for_learner_result(
    result: Result<Option<Uuid>, AgentFailure>,
    settled_at: DateTime<Utc>,
) -> Result<LearnerJobSettlement, AgentFailure> {
    match result {
        Ok(candidate_id) => Ok(LearnerJobSettlement::Completed { candidate_id }),
        Err(failure @ (AgentFailure::VaultUnavailable | AgentFailure::StorageUnavailable)) => {
            Err(failure)
        }
        Err(failure) if retryable_learner_failure(failure) => Ok(LearnerJobSettlement::Deferred {
            available_at: settled_at + chrono::Duration::seconds(LEARNER_JOB_RETRY_DELAY_SECONDS),
            failure,
        }),
        Err(failure) => Ok(LearnerJobSettlement::Failed { failure }),
    }
}

const MAX_OBSERVATION_DIGEST_BYTES: usize = 4 * 1024;
const MAX_EVIDENCE_REFS: usize = 32;

pub fn validate_learner_input(
    input: &LearnerReviewInput,
    person_id: floe_kernel::PersonId,
) -> Result<(), AgentFailure> {
    let unique_turns = input.turn_ids.iter().collect::<HashSet<_>>();
    let unique_memories = input
        .current_memories
        .iter()
        .map(|memory| memory.target_id)
        .collect::<HashSet<_>>();
    if input.schema_version != crate::KNOWLEDGE_VERSION
        || input.person_id != person_id || !person_id.is_valid()
        || input.session_id.is_nil() || input.turn_ids.iter().any(Uuid::is_nil)
        || input.outcome != crate::LearningOutcome::Completed
        || input.session_revision == 0
        || input.turn_ids.is_empty()
        || input.turn_ids.len() > MAX_EVIDENCE_REFS
        || unique_turns.len() != input.turn_ids.len()
        || input.digest.trim().is_empty()
        || input.digest.len() > MAX_OBSERVATION_DIGEST_BYTES
        || input.current_memories.len() > crate::MAX_CONTEXT_MEMORIES
        || unique_memories.len() != input.current_memories.len()
        || input.current_memories.iter().any(|memory| {
            memory.revision == 0
                || memory.statement.trim().is_empty()
                || memory.confidence_millis > 1000
                || memory.source_refs.is_empty()
        })
        || serde_json::to_vec(input)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > 16 * 1024
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerReviewInput {
    pub schema_version: u32,
    pub run_id: Uuid,
    pub person_id: floe_kernel::PersonId,
    pub session_id: Uuid,
    pub session_revision: u64,
    pub turn_ids: Vec<Uuid>,
    pub outcome: crate::LearningOutcome,
    pub digest: String,
    pub current_memories: Vec<crate::ContextMemory>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerReviewJob {
    pub schema_version: u32,
    pub id: Uuid,
    pub idempotency_key: String,
    pub input: LearnerReviewInput,
    pub state: LearnerJobState,
    pub attempts: u8,
    pub available_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<AgentFailure>,
    pub blocked: Option<LearnerProjectionBlock>,
    pub claimed_device_id: Option<String>,
}

pub fn explicit_learning_signal(text: &str) -> Option<crate::LearningObservationKind> {
    let normalized = text.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }
    if normalized.starts_with("remember ")
        || [
            "please remember",
            "기억해줘",
            "기억해 줘",
            "기억해 주세요",
            "기억해둬",
            "기억해 둬",
        ]
        .iter()
        .any(|signal| normalized.contains(signal))
    {
        return Some(crate::LearningObservationKind::ExplicitRemember);
    }
    if normalized.starts_with("forget ")
        || ["please forget", "잊어줘", "잊어 줘", "기억에서 지워"]
            .iter()
            .any(|signal| normalized.contains(signal))
    {
        return Some(crate::LearningObservationKind::UserCorrection);
    }
    if [
        "actually,",
        "correction:",
        "that's not right",
        "that is not right",
        "정확히는",
        "정정할게",
        "정정할게요",
        "그게 아니라",
    ]
    .iter()
    .any(|signal| normalized.contains(signal))
    {
        return Some(crate::LearningObservationKind::UserCorrection);
    }
    None
}

impl LearnerReviewJob {
    pub fn lifecycle(&self) -> LearnerJobLifecycle {
        LearnerJobLifecycle { state: self.state, attempts: self.attempts, available_at: self.available_at,
            claimed_at: self.claimed_at, finished_at: self.finished_at, candidate_id: self.candidate_id,
            last_failure: self.last_failure, blocked: self.blocked.clone(), claimed_device_id: self.claimed_device_id.clone() }
    }
    pub fn apply_lifecycle(&self, lifecycle: LearnerJobLifecycle) -> Result<Self, AgentFailure> {
        validate_learner_job_lifecycle(&lifecycle)?;
        let mut next = self.clone();
        next.state = lifecycle.state; next.attempts = lifecycle.attempts;
        next.available_at = lifecycle.available_at; next.claimed_at = lifecycle.claimed_at;
        next.finished_at = lifecycle.finished_at; next.candidate_id = lifecycle.candidate_id;
        next.last_failure = lifecycle.last_failure; next.blocked = lifecycle.blocked;
        next.claimed_device_id = lifecycle.claimed_device_id;
        Ok(next)
    }
}
