use chrono::{DateTime, Utc};
use floe_kernel::AgentFailure;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{LearningObservationKind, PersonalMemoryValue};

pub const LEARNER_JOB_LEASE_SECONDS: i64 = 30;
pub const MAX_LEARNER_JOB_ATTEMPTS: u8 = 3;
pub const LEARNER_JOB_RETRY_DELAY_SECONDS: i64 = 5;

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerReviewOutput {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<LearnerMemoryProposal>,
    pub used_tokens: u64,
    pub cost_micros: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LearnerJobSettlement {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LearnerJobLifecycle {
    pub state: LearnerJobState,
    pub attempts: u8,
    pub available_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub candidate_id: Option<Uuid>,
    pub last_failure: Option<AgentFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LearnerJobClaim {
    Claimed(LearnerJobLifecycle),
    Exhausted(LearnerJobLifecycle),
}

pub fn claim_learner_job(
    lifecycle: &LearnerJobLifecycle,
    now: DateTime<Utc>,
) -> Result<LearnerJobClaim, AgentFailure> {
    validate_learner_job_lifecycle(lifecycle)?;
    if !matches!(
        lifecycle.state,
        LearnerJobState::Queued | LearnerJobState::Deferred | LearnerJobState::Running
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
    if lifecycle.attempts > MAX_LEARNER_JOB_ATTEMPTS {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_enforces_lease_and_attempt_cap() {
        let now = Utc::now();
        let lifecycle = LearnerJobLifecycle {
            state: LearnerJobState::Queued,
            attempts: 0,
            available_at: now,
            claimed_at: None,
            finished_at: None,
            candidate_id: None,
            last_failure: None,
        };
        let LearnerJobClaim::Claimed(claimed) = claim_learner_job(&lifecycle, now).unwrap() else {
            panic!("queued job should be claimed")
        };
        assert_eq!(claimed.attempts, 1);
        assert_eq!(claimed.available_at, now + chrono::Duration::seconds(30));
        assert_eq!(
            claim_learner_job(&claimed, now),
            Err(AgentFailure::Conflict)
        );
        let mut future = lifecycle.clone();
        future.available_at = now + chrono::Duration::seconds(1);
        assert_eq!(claim_learner_job(&future, now), Err(AgentFailure::Conflict));
        let exhausted = LearnerJobLifecycle {
            state: LearnerJobState::Running,
            attempts: MAX_LEARNER_JOB_ATTEMPTS,
            claimed_at: Some(now),
            ..lifecycle
        };
        assert!(matches!(
            claim_learner_job(&exhausted, now),
            Ok(LearnerJobClaim::Exhausted(_))
        ));
    }

    #[test]
    fn settlement_rejects_corrupt_or_terminal_lifecycle_metadata() {
        let now = Utc::now();
        let mut lifecycle = LearnerJobLifecycle {
            state: LearnerJobState::Running,
            attempts: 0,
            available_at: now,
            claimed_at: None,
            finished_at: None,
            candidate_id: None,
            last_failure: None,
        };
        assert_eq!(
            settle_learner_job(
                &lifecycle,
                0,
                LearnerJobSettlement::Completed { candidate_id: None },
                now
            ),
            Err(AgentFailure::VaultUnavailable)
        );
        lifecycle.attempts = 1;
        lifecycle.claimed_at = Some(now);
        let completed = settle_learner_job(
            &lifecycle,
            1,
            LearnerJobSettlement::Completed { candidate_id: None },
            now,
        )
        .unwrap();
        assert_eq!(
            reject_learner_claim(&completed, AgentFailure::PolicyDenied, now),
            Err(AgentFailure::Conflict)
        );
    }

    #[test]
    fn settlement_classifies_fatal_storage_errors_without_terminal_write() {
        let now = Utc::now();
        for failure in [
            AgentFailure::VaultUnavailable,
            AgentFailure::StorageUnavailable,
        ] {
            assert_eq!(
                settlement_for_learner_result(Err(failure), now),
                Err(failure)
            );
        }
        assert!(matches!(
            settlement_for_learner_result(Err(AgentFailure::PolicyDenied), now),
            Ok(LearnerJobSettlement::Failed {
                failure: AgentFailure::PolicyDenied
            })
        ));
        let lifecycle = LearnerJobLifecycle {
            state: LearnerJobState::Queued,
            attempts: 0,
            available_at: now,
            claimed_at: None,
            finished_at: None,
            candidate_id: None,
            last_failure: None,
        };
        assert_eq!(
            reject_learner_claim(&lifecycle, AgentFailure::VaultUnavailable, now),
            Err(AgentFailure::VaultUnavailable)
        );
    }
}
