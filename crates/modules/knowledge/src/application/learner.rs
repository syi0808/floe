use std::collections::HashSet;

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

const MAX_LEARNER_VERSION_BYTES: usize = 128;
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
        || input.person_id != person_id
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
}

#[derive(Clone)]
pub struct LearnerModelRequest {
    pub input: LearnerReviewInput,
    pub remaining_tokens: u64,
    pub remaining_cost_micros: u64,
    pub max_output_bytes: usize,
    pub deadline: tokio::time::Instant,
    pub cancellation: floe_execution::Cancellation,
}

pub trait LearnerModel {
    fn placement(&self) -> floe_context_contract::ModelPlacement;

    fn review(
        &self,
        request: LearnerModelRequest,
    ) -> impl Future<Output = Result<LearnerReviewOutput, AgentFailure>> + Send;
}

pub trait MemoryCandidateSink {
    fn person_id(&self) -> floe_kernel::PersonId;

    fn stage_memory_candidate(
        &self,
        request: crate::StageMemoryCandidate,
    ) -> impl Future<Output = Result<crate::KnowledgeCandidate, AgentFailure>> + Send;
}

pub struct LearnerRuntime<'runtime, Model, Sink> {
    pub model: &'runtime Model,
    pub candidates: &'runtime Sink,
    pub budget: LearnerBudget,
    pub extractor_version: &'runtime str,
    pub prompt_version: &'runtime str,
}

impl<Model: LearnerModel + Sync, Sink: MemoryCandidateSink + Sync> LearnerRuntime<'_, Model, Sink> {
    pub async fn review(
        &self,
        input: LearnerReviewInput,
        cancellation: floe_execution::Cancellation,
    ) -> Result<Option<crate::KnowledgeCandidate>, AgentFailure> {
        self.validate_input(&input)?;
        if self.model.placement() != floe_context_contract::ModelPlacement::DeviceLocal {
            return Err(AgentFailure::PolicyDenied);
        }
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let deadline = tokio::time::Instant::now()
            + tokio::time::Duration::from_millis(self.budget.deadline_ms);
        let output = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(AgentFailure::DeadlineExceeded),
            output = self.model.review(LearnerModelRequest {
                input: input.clone(),
                remaining_tokens: self.budget.max_model_tokens,
                remaining_cost_micros: self.budget.max_model_cost_micros,
                max_output_bytes: self.budget.max_output_bytes,
                deadline,
                cancellation: cancellation.clone(),
            }) => output?,
        };
        self.validate_output(&output)?;
        let Some(mut proposal) = output.proposal else {
            return Ok(None);
        };
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        proposal.value.observed_at = input.observed_at;
        self.candidates
            .stage_memory_candidate(crate::StageMemoryCandidate {
                session_id: input.session_id,
                expected_session_revision: input.session_revision,
                turn_ids: input.turn_ids,
                observation_kind: proposal.observation_kind,
                digest: input.digest,
                value: proposal.value,
                target_id: proposal.target_id,
                base_revision: proposal.base_revision,
                extractor_version: self.extractor_version.to_owned(),
                prompt_version: self.prompt_version.to_owned(),
                actor: crate::KnowledgeActor::Learner {
                    run_id: input.run_id,
                },
                created_at: input.observed_at,
            })
            .await
            .map(Some)
    }

    fn validate_input(&self, input: &LearnerReviewInput) -> Result<(), AgentFailure> {
        if input.schema_version != crate::KNOWLEDGE_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        validate_learner_input(input, self.candidates.person_id())?;
        if self.extractor_version.trim().is_empty()
            || self.extractor_version.len() > MAX_LEARNER_VERSION_BYTES
            || self.prompt_version.trim().is_empty()
            || self.prompt_version.len() > MAX_LEARNER_VERSION_BYTES
            || self.budget.deadline_ms == 0
            || self.budget.deadline_ms > 30_000
            || self.budget.max_model_tokens == 0
            || self.budget.max_model_cost_micros == 0
            || serde_json::to_vec(input)
                .map_err(|_| AgentFailure::InvalidInput)?
                .len()
                > self.budget.max_input_bytes
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    fn validate_output(&self, output: &LearnerReviewOutput) -> Result<(), AgentFailure> {
        if output.schema_version != crate::KNOWLEDGE_VERSION
            || output.used_tokens > self.budget.max_model_tokens
            || output.cost_micros > self.budget.max_model_cost_micros
            || serde_json::to_vec(output)
                .map_err(|_| AgentFailure::InvalidModelOutput)?
                .len()
                > self.budget.max_output_bytes
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    struct ReviewModel {
        placement: floe_context_contract::ModelPlacement,
        output: LearnerReviewOutput,
    }

    impl LearnerModel for ReviewModel {
        fn placement(&self) -> floe_context_contract::ModelPlacement {
            self.placement
        }

        async fn review(
            &self,
            _request: LearnerModelRequest,
        ) -> Result<LearnerReviewOutput, AgentFailure> {
            Ok(self.output.clone())
        }
    }

    struct ReviewSink {
        person_id: floe_kernel::PersonId,
    }

    impl MemoryCandidateSink for ReviewSink {
        fn person_id(&self) -> floe_kernel::PersonId {
            self.person_id
        }

        async fn stage_memory_candidate(
            &self,
            _request: crate::StageMemoryCandidate,
        ) -> Result<crate::KnowledgeCandidate, AgentFailure> {
            Err(AgentFailure::VaultUnavailable)
        }
    }

    fn review_input(person_id: floe_kernel::PersonId) -> LearnerReviewInput {
        LearnerReviewInput {
            schema_version: crate::KNOWLEDGE_VERSION,
            run_id: Uuid::new_v4(),
            person_id,
            session_id: Uuid::new_v4(),
            session_revision: 1,
            turn_ids: vec![Uuid::new_v4()],
            outcome: crate::LearningOutcome::Completed,
            digest: "remember focused mornings".into(),
            current_memories: vec![],
            observed_at: Utc::now(),
        }
    }

    #[test]
    fn learning_outcome_keeps_agent_wire_shape() {
        let encoded = serde_json::to_string(&crate::LearningOutcome::Halted {
            reason: AgentFailure::Cancelled,
        })
        .unwrap();
        assert_eq!(encoded, r#"{"status":"halted","reason":"cancelled"}"#);
    }

    #[tokio::test]
    async fn runtime_owns_placement_and_cancellation_admission() {
        let person_id = floe_kernel::PersonId::new();
        let sink = ReviewSink { person_id };
        let output = LearnerReviewOutput {
            schema_version: crate::KNOWLEDGE_VERSION,
            proposal: None,
            used_tokens: 1,
            cost_micros: 1,
        };
        let remote = ReviewModel {
            placement: floe_context_contract::ModelPlacement::Remote,
            output: output.clone(),
        };
        let runtime = LearnerRuntime {
            model: &remote,
            candidates: &sink,
            budget: LearnerBudget::default(),
            extractor_version: "extractor.v1",
            prompt_version: "prompt.v1",
        };
        assert_eq!(
            runtime
                .review(
                    review_input(person_id),
                    floe_execution::Cancellation::default()
                )
                .await,
            Err(AgentFailure::PolicyDenied)
        );

        let local = ReviewModel {
            placement: floe_context_contract::ModelPlacement::DeviceLocal,
            output,
        };
        let runtime = LearnerRuntime {
            model: &local,
            candidates: &sink,
            budget: LearnerBudget::default(),
            extractor_version: "extractor.v1",
            prompt_version: "prompt.v1",
        };
        let cancellation = floe_execution::Cancellation::default();
        cancellation.cancel();
        assert_eq!(
            runtime.review(review_input(person_id), cancellation).await,
            Err(AgentFailure::Cancelled)
        );
    }

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
