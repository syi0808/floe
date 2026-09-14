use serde::Deserialize;

use crate::ModelPlacement;
use crate::{
    AGENT_VERSION, AgentContext, AgentFailure, AgentMessage, AgentUsage, DataClass,
    InferencePolicyDecision, ModelRequest, ModelRunner, ModelStep, TransferConsent, UsageLedger,
    generate_with_recovery, learner_prompt,
};
pub use floe_knowledge::{
    LearnerMemoryProposal, LearnerModel, LearnerModelRequest, LearnerReviewOutput,
};

pub use floe_knowledge::{
    LearnerBudget, LearnerJobClaim, LearnerJobLifecycle, LearnerJobSettlement, LearnerJobState,
    LearnerReviewInput, LearnerReviewJob, LearnerRuntime, MemoryCandidateSink,
    explicit_learning_signal, retryable_learner_failure, settlement_for_learner_result,
};

pub struct StructuredLearnerModel<Model> {
    model: Model,
}

impl<Model> StructuredLearnerModel<Model> {
    pub const fn new(model: Model) -> Self {
        Self { model }
    }
}

impl<Model: ModelRunner + Sync> LearnerModel for StructuredLearnerModel<Model> {
    fn placement(&self) -> ModelPlacement {
        self.model.placement()
    }

    async fn review(
        &self,
        request: LearnerModelRequest,
    ) -> Result<LearnerReviewOutput, AgentFailure> {
        if self.model.placement() != ModelPlacement::DeviceLocal {
            return Err(AgentFailure::PolicyDenied);
        }
        let turn_id = request
            .input
            .turn_ids
            .last()
            .copied()
            .ok_or(AgentFailure::InvalidInput)?;
        let response = generate_with_recovery(
            &self.model,
            ModelRequest {
                usage: UsageLedger::new(
                    request.remaining_tokens,
                    request.remaining_cost_micros,
                    AgentUsage::default(),
                ),
                replay: vec![],
                schema_version: AGENT_VERSION,
                prompt: learner_prompt(),
                person_id: request.input.person_id,
                session_id: request.input.session_id,
                turn_id,
                policy: InferencePolicyDecision {
                    purpose: "governed-memory-review".into(),
                    data_classes: vec![DataClass::Personal],
                    allowed_placements: vec![ModelPlacement::DeviceLocal],
                    performance_class: "background".into(),
                    projection_version: 1,
                    external_transfer_consent: TransferConsent::NotGranted,
                    bounded_sensitive_projection: false,
                },
                context: AgentContext {
                    projection_version: 1,
                    persona: None,
                    memories: request.input.current_memories,
                    optional_context_issues: vec![],
                    evidence: vec![],
                },
                messages: vec![AgentMessage::User {
                    turn_id,
                    text: request.input.digest,
                }],
                capabilities: vec![],
                active_agents: vec![],
                remaining_tokens: request.remaining_tokens,
                remaining_cost_micros: request.remaining_cost_micros,
                max_output_bytes: request.max_output_bytes,
                deadline: request.deadline,
                cancellation: request.cancellation,
            },
        )
        .await?;
        if response.replay.is_some() || response.output.len() != 1 {
            return Err(AgentFailure::InvalidModelOutput);
        }
        let ModelStep::Answer { text } = &response.output[0] else {
            return Err(AgentFailure::InvalidModelOutput);
        };
        let answer: StructuredLearnerAnswer =
            serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
        if answer.schema_version != floe_knowledge::KNOWLEDGE_VERSION {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(LearnerReviewOutput {
            schema_version: answer.schema_version,
            proposal: answer.proposal,
            used_tokens: response.used_tokens,
            cost_micros: response.cost_micros,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StructuredLearnerAnswer {
    schema_version: u32,
    proposal: Option<LearnerMemoryProposal>,
}

#[cfg(test)]
mod tests {
    use crate::{
        Cancellation, KNOWLEDGE_VERSION, KnowledgeActor, KnowledgeCandidate,
        LearningObservationKind, StageMemoryCandidate,
    };
    use chrono::{DateTime, Utc};
    use floe_domain::PersonId;
    use floe_knowledge::PersonalMemoryValue;
    use tokio::time::{Duration, Instant};
    use uuid::Uuid;

    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use crate::{
        EpistemicStatus, KnowledgeCandidateState, KnowledgeKind, KnowledgeOperation,
        KnowledgePayload, LearningEvidenceRef, ModelResponse, PersonalMemoryKind,
    };

    use super::*;

    struct Model {
        placement: ModelPlacement,
        output: LearnerReviewOutput,
        calls: Arc<AtomicUsize>,
    }

    struct PendingModel;

    struct Runner {
        placement: ModelPlacement,
        response: ModelResponse,
        requests: Mutex<Vec<ModelRequest>>,
    }

    impl ModelRunner for Runner {
        fn placement(&self) -> ModelPlacement {
            self.placement
        }

        async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
            self.requests.lock().unwrap().push(request);
            Ok(self.response.clone())
        }
    }

    impl LearnerModel for PendingModel {
        fn placement(&self) -> ModelPlacement {
            ModelPlacement::DeviceLocal
        }

        async fn review(
            &self,
            _: LearnerModelRequest,
        ) -> Result<LearnerReviewOutput, AgentFailure> {
            std::future::pending().await
        }
    }

    impl LearnerModel for Model {
        fn placement(&self) -> ModelPlacement {
            self.placement
        }

        async fn review(
            &self,
            _: LearnerModelRequest,
        ) -> Result<LearnerReviewOutput, AgentFailure> {
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
            outcome: floe_knowledge::LearningOutcome::Completed,
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
            response: ModelResponse {
                replay: None,
                schema_version: AGENT_VERSION,
                output: vec![ModelStep::Answer { text: answer }],
                used_tokens: 120,
                cost_micros: 10,
            },
            requests: Mutex::new(vec![]),
        };
        let model = StructuredLearnerModel::new(runner);

        let output = model.review(model_request(input.clone())).await.unwrap();

        assert_eq!(output, expected);
        let requests = model.model.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].prompt.role, crate::PromptRole::Learner);
        assert_eq!(requests[0].person_id, input.person_id);
        assert_eq!(requests[0].session_id, input.session_id);
        assert_eq!(requests[0].turn_id, input.turn_ids[0]);
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
        assert!(matches!(
            requests[0].messages.as_slice(),
            [AgentMessage::User { text, .. }] if text == &input.digest
        ));
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
                response: ModelResponse {
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
            let model = StructuredLearnerModel::new(runner);

            assert_eq!(
                model.review(model_request(input())).await,
                Err(if placement == ModelPlacement::Remote {
                    AgentFailure::PolicyDenied
                } else {
                    AgentFailure::InvalidModelOutput
                })
            );
            assert_eq!(model.model.requests.lock().unwrap().len(), expected_calls);
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
        halted.outcome = floe_knowledge::LearningOutcome::Halted {
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
}
