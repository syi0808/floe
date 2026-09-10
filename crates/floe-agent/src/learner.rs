use std::collections::HashSet;

use chrono::{DateTime, Utc};
use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, Instant};
use uuid::Uuid;

use crate::{
    AgentFailure, AgentOutcome, Cancellation, ContextMemory, KNOWLEDGE_VERSION, KnowledgeActor,
    KnowledgeCandidate, LearningObservationKind, ModelPlacement, PersonalMemoryValue,
    StageMemoryCandidate,
};

const MAX_LEARNER_VERSION_BYTES: usize = 128;

pub fn explicit_learning_signal(text: &str) -> Option<LearningObservationKind> {
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
        return Some(LearningObservationKind::ExplicitRemember);
    }
    if normalized.starts_with("forget ")
        || ["please forget", "잊어줘", "잊어 줘", "기억에서 지워"]
            .iter()
            .any(|signal| normalized.contains(signal))
    {
        return Some(LearningObservationKind::UserCorrection);
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
        return Some(LearningObservationKind::UserCorrection);
    }
    None
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerReviewInput {
    pub schema_version: u32,
    pub run_id: Uuid,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub session_revision: u64,
    pub turn_ids: Vec<Uuid>,
    pub outcome: AgentOutcome,
    pub digest: String,
    pub current_memories: Vec<ContextMemory>,
    pub observed_at: DateTime<Utc>,
}

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LearnerJobState {
    Queued,
    Running,
    Deferred,
    Completed,
    Failed,
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
            max_model_tokens: 2_048,
            max_model_cost_micros: 50_000,
            deadline_ms: 15_000,
        }
    }
}

pub trait LearnerModel {
    fn placement(&self) -> ModelPlacement;

    fn review(
        &self,
        input: LearnerReviewInput,
    ) -> impl Future<Output = Result<LearnerReviewOutput, AgentFailure>> + Send;
}

pub trait MemoryCandidateSink {
    fn person_id(&self) -> PersonId;

    fn stage_memory_candidate(
        &self,
        request: StageMemoryCandidate,
    ) -> impl Future<Output = Result<KnowledgeCandidate, AgentFailure>> + Send;
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
        cancellation: Cancellation,
    ) -> Result<Option<KnowledgeCandidate>, AgentFailure> {
        self.validate_input(&input)?;
        if self.model.placement() != ModelPlacement::DeviceLocal {
            return Err(AgentFailure::PolicyDenied);
        }
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let deadline = Instant::now() + Duration::from_millis(self.budget.deadline_ms);
        let output = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(AgentFailure::DeadlineExceeded),
            output = self.model.review(input.clone()) => output?,
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
            .stage_memory_candidate(StageMemoryCandidate {
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
                actor: KnowledgeActor::Learner {
                    run_id: input.run_id,
                },
                created_at: input.observed_at,
            })
            .await
            .map(Some)
    }

    fn validate_input(&self, input: &LearnerReviewInput) -> Result<(), AgentFailure> {
        let unique_turns = input.turn_ids.iter().copied().collect::<HashSet<_>>();
        if input.schema_version != KNOWLEDGE_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if input.person_id != self.candidates.person_id()
            || input.outcome != AgentOutcome::Completed
            || input.session_revision == 0
            || input.turn_ids.is_empty()
            || unique_turns.len() != input.turn_ids.len()
            || input.digest.trim().is_empty()
            || self.extractor_version.trim().is_empty()
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
        if output.schema_version != KNOWLEDGE_VERSION
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use crate::{
        EpistemicStatus, KnowledgeCandidateState, KnowledgeKind, KnowledgeOperation,
        KnowledgePayload, LearningEvidenceRef, PersonalMemoryKind,
    };

    use super::*;

    struct Model {
        placement: ModelPlacement,
        output: LearnerReviewOutput,
        calls: Arc<AtomicUsize>,
    }

    struct PendingModel;

    impl LearnerModel for PendingModel {
        fn placement(&self) -> ModelPlacement {
            ModelPlacement::DeviceLocal
        }

        async fn review(&self, _: LearnerReviewInput) -> Result<LearnerReviewOutput, AgentFailure> {
            std::future::pending().await
        }
    }

    impl LearnerModel for Model {
        fn placement(&self) -> ModelPlacement {
            self.placement
        }

        async fn review(&self, _: LearnerReviewInput) -> Result<LearnerReviewOutput, AgentFailure> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.output.clone())
        }
    }

    struct Sink {
        person_id: PersonId,
        requests: Mutex<Vec<StageMemoryCandidate>>,
    }

    impl Sink {
        fn new(person_id: PersonId) -> Self {
            Self {
                person_id,
                requests: Mutex::new(vec![]),
            }
        }
    }

    impl MemoryCandidateSink for Sink {
        fn person_id(&self) -> PersonId {
            self.person_id
        }

        async fn stage_memory_candidate(
            &self,
            request: StageMemoryCandidate,
        ) -> Result<KnowledgeCandidate, AgentFailure> {
            self.requests.lock().unwrap().push(request.clone());
            Ok(KnowledgeCandidate {
                schema_version: KNOWLEDGE_VERSION,
                id: Uuid::new_v4(),
                person_id: PersonId::new(),
                observation_id: Uuid::new_v4(),
                idempotency_key: "fixture-key".into(),
                kind: KnowledgeKind::Memory,
                operation: KnowledgeOperation::Create,
                target_id: request.target_id,
                base_revision: request.base_revision,
                payload: KnowledgePayload::Memory {
                    value: request.value,
                },
                before_hash: None,
                after_hash: "fixture-hash".into(),
                source_refs: request
                    .turn_ids
                    .into_iter()
                    .map(|turn_id| LearningEvidenceRef {
                        session_id: request.session_id,
                        turn_id,
                    })
                    .collect(),
                extractor_version: request.extractor_version,
                prompt_version: request.prompt_version,
                actor: request.actor,
                state: KnowledgeCandidateState::Pending,
                created_at: request.created_at,
            })
        }
    }

    fn input() -> LearnerReviewInput {
        LearnerReviewInput {
            schema_version: KNOWLEDGE_VERSION,
            run_id: Uuid::new_v4(),
            person_id: PersonId::new(),
            session_id: Uuid::new_v4(),
            session_revision: 1,
            turn_ids: vec![Uuid::new_v4()],
            outcome: AgentOutcome::Completed,
            digest: "The user explicitly asked Floe to remember a preference.".into(),
            current_memories: vec![],
            observed_at: Utc::now(),
        }
    }

    fn proposal(observed_at: DateTime<Utc>) -> LearnerReviewOutput {
        LearnerReviewOutput {
            schema_version: KNOWLEDGE_VERSION,
            proposal: Some(LearnerMemoryProposal {
                observation_kind: LearningObservationKind::ExplicitRemember,
                value: PersonalMemoryValue {
                    kind: PersonalMemoryKind::Preference,
                    statement: "Prefers focused mornings".into(),
                    epistemic_status: EpistemicStatus::Fact,
                    confidence_millis: 900,
                    valid_from: None,
                    valid_until: None,
                    observed_at,
                },
                target_id: None,
                base_revision: None,
            }),
            used_tokens: 120,
            cost_micros: 10,
        }
    }

    #[test]
    fn explicit_signal_detection_is_narrow_and_multilingual() {
        assert_eq!(
            explicit_learning_signal("회의는 오후가 좋다고 기억해 줘"),
            Some(LearningObservationKind::ExplicitRemember)
        );
        assert_eq!(
            explicit_learning_signal("Actually, I prefer meetings after 2 PM"),
            Some(LearningObservationKind::UserCorrection)
        );
        assert_eq!(
            explicit_learning_signal("Please forget my old office preference"),
            Some(LearningObservationKind::UserCorrection)
        );
        assert_eq!(
            explicit_learning_signal("I remember that meeting from last year"),
            None
        );
        assert_eq!(explicit_learning_signal("오늘 일정 알려줘"), None);
    }

    #[tokio::test]
    async fn learner_stages_one_candidate_with_runtime_owned_provenance() {
        let input = input();
        let calls = Arc::new(AtomicUsize::new(0));
        let model = Model {
            placement: ModelPlacement::DeviceLocal,
            output: proposal(input.observed_at - chrono::Duration::days(1)),
            calls: calls.clone(),
        };
        let sink = Sink::new(input.person_id);
        let runtime = LearnerRuntime {
            model: &model,
            candidates: &sink,
            budget: LearnerBudget::default(),
            extractor_version: "memory-extractor-v1",
            prompt_version: "memory-review-v1",
        };

        let candidate = runtime
            .review(input.clone(), Cancellation::default())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(candidate.source_refs[0].session_id, input.session_id);
        assert_eq!(
            sink.requests.lock().unwrap()[0].actor,
            KnowledgeActor::Learner {
                run_id: input.run_id
            }
        );
        assert_eq!(
            sink.requests.lock().unwrap()[0].extractor_version,
            "memory-extractor-v1"
        );
        assert_eq!(
            sink.requests.lock().unwrap()[0].value.observed_at,
            input.observed_at
        );
    }

    #[tokio::test]
    async fn learner_cancellation_and_placement_fail_before_model_or_storage() {
        for (placement, cancellation, expected) in [
            (
                ModelPlacement::Remote,
                Cancellation::default(),
                AgentFailure::PolicyDenied,
            ),
            (
                ModelPlacement::DeviceLocal,
                {
                    let cancellation = Cancellation::default();
                    cancellation.cancel();
                    cancellation
                },
                AgentFailure::Cancelled,
            ),
        ] {
            let input = input();
            let calls = Arc::new(AtomicUsize::new(0));
            let model = Model {
                placement,
                output: proposal(input.observed_at),
                calls: calls.clone(),
            };
            let sink = Sink::new(input.person_id);
            let runtime = LearnerRuntime {
                model: &model,
                candidates: &sink,
                budget: LearnerBudget::default(),
                extractor_version: "memory-extractor-v1",
                prompt_version: "memory-review-v1",
            };

            assert_eq!(runtime.review(input, cancellation).await, Err(expected));
            assert_eq!(calls.load(Ordering::Relaxed), 0);
            assert!(sink.requests.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn learner_rejects_over_budget_or_halted_reviews() {
        let mut halted = input();
        halted.outcome = AgentOutcome::Halted {
            reason: AgentFailure::Stalled,
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let model = Model {
            placement: ModelPlacement::DeviceLocal,
            output: proposal(halted.observed_at),
            calls: calls.clone(),
        };
        let sink = Sink::new(halted.person_id);
        let runtime = LearnerRuntime {
            model: &model,
            candidates: &sink,
            budget: LearnerBudget::default(),
            extractor_version: "memory-extractor-v1",
            prompt_version: "memory-review-v1",
        };
        assert_eq!(
            runtime.review(halted, Cancellation::default()).await,
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);

        let mut input = input();
        input.person_id = sink.person_id;
        let model = Model {
            placement: ModelPlacement::DeviceLocal,
            output: LearnerReviewOutput {
                used_tokens: LearnerBudget::default().max_model_tokens + 1,
                ..proposal(input.observed_at)
            },
            calls,
        };
        let runtime = LearnerRuntime {
            model: &model,
            candidates: &sink,
            budget: LearnerBudget::default(),
            extractor_version: "memory-extractor-v1",
            prompt_version: "memory-review-v1",
        };
        assert_eq!(
            runtime.review(input, Cancellation::default()).await,
            Err(AgentFailure::InvalidModelOutput)
        );
        assert!(sink.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn learner_preempts_in_flight_review_on_cancellation_or_deadline() {
        let first_input = input();
        let sink = Sink::new(first_input.person_id);
        let mut budget = LearnerBudget::default();
        budget.deadline_ms = 1;
        let runtime = LearnerRuntime {
            model: &PendingModel,
            candidates: &sink,
            budget,
            extractor_version: "memory-extractor-v1",
            prompt_version: "memory-review-v1",
        };
        assert_eq!(
            runtime.review(first_input, Cancellation::default()).await,
            Err(AgentFailure::DeadlineExceeded)
        );

        let cancellation = Cancellation::default();
        let cancel = cancellation.clone();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            cancel.cancel();
        });
        let runtime = LearnerRuntime {
            model: &PendingModel,
            candidates: &sink,
            budget: LearnerBudget::default(),
            extractor_version: "memory-extractor-v1",
            prompt_version: "memory-review-v1",
        };
        let mut cancellation_input = input();
        cancellation_input.person_id = sink.person_id;
        assert_eq!(
            runtime.review(cancellation_input, cancellation).await,
            Err(AgentFailure::Cancelled)
        );
        assert!(sink.requests.lock().unwrap().is_empty());
    }
}
