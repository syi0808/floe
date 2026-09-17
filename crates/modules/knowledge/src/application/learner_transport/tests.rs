use crate::{
    InferenceLearnerModel, LearnerBudget, LearnerMemoryProposal, LearnerModel, LearnerReviewInput,
    LearnerReviewOutput, LearnerRuntime, MemoryCandidateSink, explicit_learning_signal,
};

use crate::PersonalMemoryValue;
use crate::{
    KNOWLEDGE_VERSION, KnowledgeActor, KnowledgeCandidate, LearningObservationKind,
    StageMemoryCandidate,
};
use chrono::{DateTime, Utc};
use floe_agent_contract::PersonId;
use floe_execution::Cancellation;
use tokio::time::{Duration, Instant};
use uuid::Uuid;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use crate::{
    EpistemicStatus, KnowledgeCandidateState, KnowledgeKind, KnowledgeOperation, KnowledgePayload,
    LearningEvidenceRef, PersonalMemoryKind,
};
use floe_inference::{ModelStep, ModelTransport, ModelTransportRequest, ModelTransportResponse};

use floe_agent_contract::{AgentFailure, ModelPlacement};
use floe_inference::{DataRecipient, ExecutionLocation, ModelProfile, PlannedRoute};

use super::*;
use crate::application::inference::{LearnerInferenceResponse, LearnerInferenceTransport};
use crate::application::learner::LearnerModelRequest;

struct FixtureTransport<'runner>(&'runner Runner);

impl LearnerInferenceTransport for FixtureTransport<'_> {
    fn profile(&self) -> Result<ModelProfile, AgentFailure> {
        let mut profile = learner_profile(FOUNDATION_LEARNER_PROFILE, true)?;
        if self.0.placement() != ModelPlacement::DeviceLocal {
            profile.execution_location = ExecutionLocation::Remote;
            profile.data_recipient = DataRecipient::External("fixture".into());
        }
        Ok(profile)
    }

    async fn generate(
        &self,
        route: PlannedRoute,
        request: LearnerModelRequest,
    ) -> Result<LearnerInferenceResponse, AgentFailure> {
        review_with_model(self.0, FOUNDATION_LEARNER_PROFILE, route, request).await
    }
}

struct Model {
    placement: ModelPlacement,
    output: LearnerReviewOutput,
    calls: Arc<AtomicUsize>,
}

struct PendingModel;

struct Runner {
    placement: ModelPlacement,
    response: ModelTransportResponse,
    requests: Mutex<Vec<ModelTransportRequest>>,
}

impl ModelTransport for Runner {
    fn placement(&self) -> ModelPlacement {
        self.placement
    }

    async fn generate(
        &self,
        request: ModelTransportRequest,
    ) -> Result<ModelTransportResponse, AgentFailure> {
        self.requests.lock().unwrap().push(request);
        Ok(self.response.clone())
    }
}

impl LearnerModel for PendingModel {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn review(&self, _: LearnerModelRequest) -> Result<LearnerReviewOutput, AgentFailure> {
        std::future::pending().await
    }
}

impl LearnerModel for Model {
    fn placement(&self) -> ModelPlacement {
        self.placement
    }

    async fn review(&self, _: LearnerModelRequest) -> Result<LearnerReviewOutput, AgentFailure> {
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
        outcome: crate::LearningOutcome::Completed,
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

fn model_request(input: LearnerReviewInput) -> LearnerModelRequest {
    LearnerModelRequest {
        input,
        remaining_tokens: LearnerBudget::default().max_model_tokens,
        remaining_cost_micros: LearnerBudget::default().max_model_cost_micros,
        max_output_bytes: LearnerBudget::default().max_output_bytes,
        deadline: Instant::now() + Duration::from_secs(1),
        cancellation: Cancellation::default(),
    }
}

#[tokio::test]
async fn foundation_adapter_rejects_a_different_profile_before_generation() {
    let runner = Runner {
        placement: ModelPlacement::DeviceLocal,
        response: ModelTransportResponse {
            replay: None,
            schema_version: AGENT_VERSION,
            output: vec![ModelStep::Answer {
                text: "{\"schema_version\":1,\"proposal\":null}".into(),
            }],
            used_tokens: 1,
            cost_micros: 0,
        },
        requests: Mutex::new(vec![]),
    };
    let profile = learner_profile(FOUNDATION_LEARNER_PROFILE, true).unwrap();
    let route = PlannedRoute {
        profile_id: "different-device-model".into(),
        purpose: profile.purpose,
        consumer: profile.consumer,
        execution_location: profile.execution_location,
        data_recipient: profile.data_recipient,
    };
    assert_eq!(
        review_with_model(
            &runner,
            FOUNDATION_LEARNER_PROFILE,
            route,
            model_request(input())
        )
        .await,
        Err(AgentFailure::PolicyDenied)
    );
    assert!(runner.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn structured_learner_uses_a_local_restricted_request() {
    let input = input();
    let expected = proposal(input.observed_at);
    let answer = serde_json::json!({
        "schema_version": KNOWLEDGE_VERSION,
        "proposal": expected.proposal.clone(),
    })
    .to_string();
    let runner = Runner {
        placement: ModelPlacement::DeviceLocal,
        response: ModelTransportResponse {
            replay: None,
            schema_version: AGENT_VERSION,
            output: vec![ModelStep::Answer { text: answer }],
            used_tokens: 120,
            cost_micros: 10,
        },
        requests: Mutex::new(vec![]),
    };
    let model = InferenceLearnerModel::new(FixtureTransport(&runner));

    let output = model.review(model_request(input.clone())).await.unwrap();

    assert_eq!(output, expected);
    let requests = runner.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].prompt.role, crate::prompts::PromptRole::Learner);
    assert!(requests[0].prompt.render().len() <= 4096);
    assert!(
        requests[0]
            .prompt
            .render()
            .contains("Never use a person's name as target_id")
    );
    // The Session that produced the digest does not cross the transport; what
    // crosses is the digest itself, already projected as the turn to review.
    assert_eq!(requests[0].policy.purpose, "governed-memory-review");
    assert_eq!(
        requests[0].policy.allowed_placements,
        vec![ModelPlacement::DeviceLocal]
    );
    assert_eq!(requests[0].policy.data_classes, vec![DataClass::Personal]);
    assert_eq!(
        requests[0].policy.external_transfer_consent,
        TransferConsent::NotGranted
    );
    assert_eq!(requests[0].policy.performance_class, "background");
    assert!(requests[0].context.persona.is_none());
    assert!(requests[0].capabilities.is_empty());
    assert!(requests[0].active_agents.is_empty());
    assert!(requests[0].envelope.conversation.history.is_empty());
    assert_eq!(
        requests[0].envelope.conversation.current_turn,
        vec![serde_json::json!({"role": "user", "content": input.digest})]
    );
}

#[tokio::test]
async fn structured_learner_rejects_remote_and_unstructured_output() {
    for (placement, output, expected_calls) in [
        (
            ModelPlacement::Remote,
            "{\"schema_version\":1,\"proposal\":null}",
            0,
        ),
        (
            ModelPlacement::DeviceLocal,
            "```json\n{\"schema_version\":1,\"proposal\":null}\n```",
            1,
        ),
        (
            ModelPlacement::DeviceLocal,
            "{\"schema_version\":1,\"proposal\":null,\"reason\":\"no\"}",
            1,
        ),
    ] {
        let runner = Runner {
            placement,
            response: ModelTransportResponse {
                replay: None,
                schema_version: AGENT_VERSION,
                output: vec![ModelStep::Answer {
                    text: output.into(),
                }],
                used_tokens: 1,
                cost_micros: 0,
            },
            requests: Mutex::new(vec![]),
        };
        let model = InferenceLearnerModel::new(FixtureTransport(&runner));

        assert_eq!(
            model.review(model_request(input())).await,
            Err(if placement == ModelPlacement::Remote {
                AgentFailure::PolicyDenied
            } else {
                AgentFailure::InvalidModelOutput
            })
        );
        assert_eq!(runner.requests.lock().unwrap().len(), expected_calls);
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
    halted.outcome = crate::LearningOutcome::Halted {
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
    let budget = LearnerBudget {
        deadline_ms: 1,
        ..LearnerBudget::default()
    };
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
