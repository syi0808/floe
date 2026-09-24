//! Canonical background Learner model host.
//!
//! The Learner reviews one digest through shared Inference: Knowledge owns
//! the prompt, the budget and the structured answer it accepts; Context
//! projects the authorized input; Inference selects the device profile and
//! owns the attempt; Access fences the dispatch. No route, raw transport,
//! or second usage ledger exists here.

use floe_agent_contract::{
    AgentContext, AgentFailure, AllowedCatalog, DataClass, ModelConversation,
    ModelConversationEntry, ModelPlacement, ModelRequest, ModelStep,
};
use floe_execution::Cancellation;
use floe_execution::budget::{BudgetConfig, BudgetLedger, ModelUsage};
use floe_inference::{InferenceExecutionConstraint, InferenceExecutor};
use floe_kernel::{RunId, TraceContext};
use floe_knowledge::{
    KNOWLEDGE_VERSION, LEARNER_INFERENCE_CONSUMER, LEARNER_INFERENCE_PURPOSE, LearnerModel,
    LearnerModelRequest, LearnerReviewOutput, LearnerService, parse_learner_review_output,
};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

pub(super) async fn run<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    cancellation: Cancellation,
) -> Result<bool, AgentFailure> {
    // Canonical Learner composition, prepared here because background work
    // actually runs: the device Foundation model observes the Learner scope,
    // and shared Inference selects it under a DeviceOnly constraint. There
    // is no server leg to admit; a background review never leaves the device.
    let provider = floe_provider_adapters::models::FoundationModelProvider::scoped(
        floe_agent_contract::SessionProtection::Encrypted,
        LEARNER_INFERENCE_PURPOSE,
        LEARNER_INFERENCE_CONSUMER,
    )?;
    let service = floe_inference::InferenceService::new(
        provider,
        LearnerDependencyResolver,
        LearnerRecipientAuthority,
    );
    let model = LearnerModelHost { executor: &service };
    LearnerService {
        model: &model,
        repository: vault,
    }
    .run_next(cancellation)
    .await
}

/// The model one Learner review runs on, as Knowledge's own contract states it.
///
/// A review asks one question and is owed one structured answer. Projecting
/// the authorized input is Context's work; selecting the device profile,
/// fencing the dispatch, and settling the attempt is Inference's. This host
/// only binds the two: the review becomes a Context projection plus a
/// canonical model request under a background root scope carrying the
/// Learner budget.
struct LearnerModelHost<'executor> {
    executor: &'executor dyn InferenceExecutor,
}

impl LearnerModel for LearnerModelHost<'_> {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn review(
        &self,
        request: LearnerModelRequest,
    ) -> Result<LearnerReviewOutput, AgentFailure> {
        validate_review_request(&request)?;
        // The review must name the turn it came from, even though the turn
        // itself does not cross to the model.
        let Some(origin_turn) = request.input.turn_ids.last().copied() else {
            return Err(AgentFailure::InvalidInput);
        };
        let agent_context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: request.input.current_memories.clone(),
            optional_context_issues: vec![],
            evidence: vec![],
        };
        agent_context.validate()?;
        // The Learner reviews one digest; there is no conversation to project.
        let conversation = ModelConversation {
            history: vec![],
            current_turn: vec![ModelConversationEntry::User {
                message_id: origin_turn,
                text: request.input.digest.clone(),
            }],
        };
        // No domain Tools, no delegates: the catalog is what the Engine and
        // Inference validate model output against.
        let catalog = AllowedCatalog {
            cards: vec![],
            tools: vec![],
            revision: 1,
        };
        let projection =
            floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
                role: floe_context::ContextProjectionRole::Learner,
                purpose: LEARNER_INFERENCE_PURPOSE,
                response_contract: "One structured memory review answer.",
                correction: None,
                prompt: floe_knowledge::prompts::learner_prompt(),
                conversation,
                agent_context: &agent_context,
                catalog: &catalog,
                active_experts: &[],
                authorized_history_dependencies: &[],
                input_data_classes: vec![DataClass::Personal],
                max_output_bytes: request.max_output_bytes,
            })?;
        // Background root scope: the Learner is not a child of the foreground
        // turn. The Learner budget becomes the scope allowance under the
        // stable Learner run identity; Inference settles the attempt against
        // it exactly once, with no second usage counter.
        let ledger = BudgetLedger::new(
            BudgetConfig::new(request.remaining_tokens, request.remaining_cost_micros),
            ModelUsage::default(),
        );
        let trace = RunId::from_uuid(request.input.run_id)
            .map(|run_id| TraceContext::new(request.input.run_id).with_run_id(run_id))
            .unwrap_or_else(|| TraceContext::new(request.input.run_id));
        let scope = floe_execution::ExecutionScope::root(
            request.cancellation.clone(),
            request.deadline,
            ledger.work_lease(),
            trace,
        );
        let response = self
            .executor
            .execute(
                ModelRequest {
                    attempt_id: Uuid::new_v4(),
                    principal: request.input.person_id.to_string(),
                    projection,
                    catalog,
                    purpose: LEARNER_INFERENCE_PURPOSE.into(),
                    consumer: LEARNER_INFERENCE_CONSUMER.into(),
                    preferred_profile_id: None,
                    replay: vec![],
                },
                &scope,
                InferenceExecutionConstraint::DeviceOnly,
            )
            .await?;
        // One question, one reply: a preamble, a tool call, a delegation or
        // a second step is not the structured answer this role accepts.
        let [ModelStep::Answer { text, .. }] = response.steps.as_slice() else {
            return Err(AgentFailure::InvalidModelOutput);
        };
        if response.usage.tokens > request.remaining_tokens
            || response.usage.cost_micros > request.remaining_cost_micros
            || text.len() > request.max_output_bytes
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(LearnerReviewOutput {
            schema_version: KNOWLEDGE_VERSION,
            proposal: parse_learner_review_output(text)?,
            used_tokens: response.usage.tokens,
            cost_micros: response.usage.cost_micros,
        })
    }
}

fn validate_review_request(request: &LearnerModelRequest) -> Result<(), AgentFailure> {
    if request.remaining_tokens == 0
        || request.remaining_cost_micros == 0
        || request.max_output_bytes == 0
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    if request.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if request.deadline <= tokio::time::Instant::now() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    if request.input.turn_ids.is_empty() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

/// Dependency resolver for the background Learner dispatch.
///
/// The Learner projection carries vault-local memories under independent
/// coverage to a device target, which Access admits without consulting this
/// resolver. Any dependency that ever appears fails closed here.
struct LearnerDependencyResolver;

impl floe_access::DependencyResolver for LearnerDependencyResolver {
    fn authorize<'a>(
        &'a self,
        _dependency: &'a floe_context_contract::ContextDependency,
        _request: &'a floe_access::DependencyAuthorization,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>>
    {
        Box::pin(async move { Err(AgentFailure::PolicyDenied) })
    }
}

/// Recipient authority for the background Learner dispatch.
///
/// Device dispatch never consults it, and the DeviceOnly execution
/// constraint admits no external target, so any recipient check fails closed.
struct LearnerRecipientAuthority;

impl floe_access::ModelDispatchRecipientAuthority for LearnerRecipientAuthority {
    fn check_recipient(&self, _recipient: &str) -> Result<(), AgentFailure> {
        Err(AgentFailure::PolicyDenied)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use chrono::Utc;
    use floe_agent_contract::{ModelResponse, ModelUsage as ContractUsage};
    use floe_kernel::PersonId;
    use floe_knowledge::{KNOWLEDGE_VERSION, LearnerReviewInput, LearningOutcome};
    use tokio::time::{Duration, Instant};

    use super::*;

    struct FakeExecutor {
        response: ModelResponse,
        calls: AtomicUsize,
        seen: Mutex<Vec<(ModelRequest, InferenceExecutionConstraint)>>,
        seen_scopes: Mutex<Vec<ScopeFacts>>,
    }

    struct ScopeFacts {
        max_tokens: u64,
        max_cost_micros: u64,
        deadline: Instant,
        run_id: Option<RunId>,
    }

    impl FakeExecutor {
        fn answering(text: &str, tokens: u64, cost_micros: u64) -> Self {
            Self::responding(
                vec![ModelStep::Answer {
                    text: text.into(),
                    artifacts: vec![],
                }],
                tokens,
                cost_micros,
            )
        }

        fn responding(steps: Vec<ModelStep>, tokens: u64, cost_micros: u64) -> Self {
            Self {
                response: ModelResponse {
                    attempt_id: Uuid::new_v4(),
                    steps,
                    usage: ContractUsage {
                        tokens,
                        cost_micros,
                    },
                },
                calls: AtomicUsize::new(0),
                seen: Mutex::new(vec![]),
                seen_scopes: Mutex::new(vec![]),
            }
        }
    }

    impl InferenceExecutor for FakeExecutor {
        fn execute<'a>(
            &'a self,
            request: ModelRequest,
            scope: &'a floe_execution::ExecutionScope,
            constraint: InferenceExecutionConstraint,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.seen.lock().unwrap().push((request, constraint));
            self.seen_scopes.lock().unwrap().push(ScopeFacts {
                max_tokens: scope.budget().max_tokens(),
                max_cost_micros: scope.budget().max_cost_micros(),
                deadline: scope.deadline(),
                run_id: scope.root_run_id(),
            });
            Box::pin(async move { Ok(self.response.clone()) })
        }
    }

    fn review_request() -> LearnerModelRequest {
        LearnerModelRequest {
            input: LearnerReviewInput {
                schema_version: KNOWLEDGE_VERSION,
                run_id: Uuid::new_v4(),
                person_id: PersonId::new(),
                session_id: Uuid::new_v4(),
                session_revision: 1,
                turn_ids: vec![Uuid::new_v4()],
                outcome: LearningOutcome::Completed,
                digest: "The user asked Floe to remember a preference.".into(),
                current_memories: vec![],
                observed_at: Utc::now(),
            },
            remaining_tokens: 8_192,
            remaining_cost_micros: 50_000,
            max_output_bytes: 4 * 1024,
            deadline: Instant::now() + Duration::from_secs(1),
            cancellation: Cancellation::default(),
        }
    }

    #[tokio::test]
    async fn review_runs_device_only_under_a_background_learner_scope() {
        let request = review_request();
        let executor = FakeExecutor::answering(
            &format!(r#"{{"schema_version":{KNOWLEDGE_VERSION},"proposal":null}}"#),
            120,
            10,
        );
        let model = LearnerModelHost {
            executor: &executor,
        };

        let output = model.review(request.clone()).await.unwrap();

        assert_eq!(output.schema_version, KNOWLEDGE_VERSION);
        assert_eq!(output.proposal, None);
        // Inference owns usage: the host reports exactly what it settled.
        assert_eq!(output.used_tokens, 120);
        assert_eq!(output.cost_micros, 10);
        assert_eq!(executor.calls.load(Ordering::Relaxed), 1);
        let seen = executor.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let (dispatched, constraint) = &seen[0];
        assert_eq!(*constraint, InferenceExecutionConstraint::DeviceOnly);
        assert_eq!(dispatched.purpose, LEARNER_INFERENCE_PURPOSE);
        assert_eq!(dispatched.consumer, LEARNER_INFERENCE_CONSUMER);
        assert_eq!(dispatched.principal, request.input.person_id.to_string());
        assert!(dispatched.preferred_profile_id.is_none());
        assert!(dispatched.catalog.tools.is_empty());
        assert!(dispatched.catalog.cards.is_empty());
        assert_eq!(
            dispatched.projection.envelope.scoped_instructions.purpose,
            LEARNER_INFERENCE_PURPOSE
        );
        assert!(
            dispatched
                .projection
                .envelope
                .conversation
                .history
                .is_empty()
        );
        assert!(matches!(
            dispatched
                .projection
                .envelope
                .conversation
                .current_turn
                .as_slice(),
            [ModelConversationEntry::User { text, .. }] if text == &request.input.digest
        ));
        assert_eq!(
            dispatched.projection.envelope.contextual_data.memories,
            request.input.current_memories
        );
        let scopes = executor.seen_scopes.lock().unwrap();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].max_tokens, request.remaining_tokens);
        assert_eq!(scopes[0].max_cost_micros, request.remaining_cost_micros);
        assert_eq!(scopes[0].deadline, request.deadline);
        assert_eq!(
            scopes[0].run_id.map(RunId::as_uuid),
            Some(request.input.run_id)
        );
    }

    #[tokio::test]
    async fn review_accepts_only_one_structured_answer() {
        for steps in [
            vec![ModelStep::Preamble {
                text: "reviewing".into(),
            }],
            vec![ModelStep::CallTool {
                tool_id: "memory.write".into(),
                definition_revision: 1,
                input: "{}".into(),
            }],
            vec![ModelStep::Delegate {
                agent_id: "memory".into(),
                definition_revision: 1,
                message: "remember".into(),
                context_refs: vec![],
            }],
            vec![
                ModelStep::Answer {
                    text: format!(r#"{{"schema_version":{KNOWLEDGE_VERSION},"proposal":null}}"#),
                    artifacts: vec![],
                },
                ModelStep::Answer {
                    text: format!(r#"{{"schema_version":{KNOWLEDGE_VERSION},"proposal":null}}"#),
                    artifacts: vec![],
                },
            ],
        ] {
            let executor = FakeExecutor::responding(steps, 1, 0);
            let model = LearnerModelHost {
                executor: &executor,
            };
            assert_eq!(
                model.review(review_request()).await,
                Err(AgentFailure::InvalidModelOutput)
            );
            assert_eq!(executor.calls.load(Ordering::Relaxed), 1);
        }
    }

    #[tokio::test]
    async fn review_rejects_malformed_answers_and_usage_overruns() {
        for (text, tokens, cost, expected) in [
            ("not json", 1, 0, AgentFailure::InvalidModelOutput),
            (
                r#"{"schema_version":1}"#,
                1,
                0,
                AgentFailure::InvalidModelOutput,
            ),
            (
                r#"{"schema_version":1,"proposal":null,"extra":true}"#,
                1,
                0,
                AgentFailure::InvalidModelOutput,
            ),
            (
                &format!(r#"{{"schema_version":{KNOWLEDGE_VERSION},"proposal":null}}"#),
                8_193,
                0,
                AgentFailure::BudgetExceeded,
            ),
            (
                &format!(r#"{{"schema_version":{KNOWLEDGE_VERSION},"proposal":null}}"#),
                1,
                50_001,
                AgentFailure::BudgetExceeded,
            ),
        ] {
            let executor = FakeExecutor::answering(text, tokens, cost);
            let model = LearnerModelHost {
                executor: &executor,
            };
            assert_eq!(model.review(review_request()).await, Err(expected));
            assert_eq!(executor.calls.load(Ordering::Relaxed), 1);
        }
        let oversized = format!(
            r#"{{"schema_version":{KNOWLEDGE_VERSION},"proposal":null,"pad":"{}"}}"#,
            "x".repeat(8 * 1024)
        );
        let executor = FakeExecutor::answering(&oversized, 1, 0);
        let model = LearnerModelHost {
            executor: &executor,
        };
        assert_eq!(
            model.review(review_request()).await,
            Err(AgentFailure::BudgetExceeded)
        );
    }

    #[tokio::test]
    async fn review_admits_budget_cancellation_and_input_before_dispatch() {
        let mut exhausted = review_request();
        exhausted.remaining_tokens = 0;
        let cancelled = review_request();
        cancelled.cancellation.cancel();
        let mut expired = review_request();
        expired.deadline = Instant::now() - Duration::from_secs(1);
        let mut nameless = review_request();
        nameless.input.turn_ids.clear();
        for (request, expected) in [
            (exhausted, AgentFailure::BudgetExceeded),
            (cancelled, AgentFailure::Cancelled),
            (expired, AgentFailure::DeadlineExceeded),
            (nameless, AgentFailure::InvalidInput),
        ] {
            let executor = FakeExecutor::answering(
                &format!(r#"{{"schema_version":{KNOWLEDGE_VERSION},"proposal":null}}"#),
                1,
                0,
            );
            let model = LearnerModelHost {
                executor: &executor,
            };
            assert_eq!(model.review(request).await, Err(expected));
            assert_eq!(executor.calls.load(Ordering::Relaxed), 0);
        }
    }

    #[tokio::test]
    async fn background_dispatch_ports_fail_closed_when_consulted() {
        use floe_access::{DependencyResolver, ModelDispatchRecipientAuthority};
        use floe_context_contract::{
            ConnectionId, ConnectorId, ConsumerPolicyAuthority, ContextDependency,
            ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId,
            GrantOperation, GrantPurpose, GrantSourceBinding, ProcessingRestriction,
            ResourceHandle, SourceAuthority,
        };

        let person_id = PersonId::new();
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("connection").unwrap(),
            ConnectorId::try_new("connector").unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let now = Utc::now();
        let dependency = ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"fingerprint".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - chrono::Duration::minutes(1),
            now + chrono::Duration::minutes(5),
        )
        .unwrap();
        let authorization = floe_access::DependencyAuthorization {
            deadline: Instant::now() + Duration::from_secs(1),
            cancellation: Cancellation::default(),
        };
        assert_eq!(
            LearnerDependencyResolver
                .authorize(&dependency, &authorization)
                .await,
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(
            LearnerRecipientAuthority.check_recipient("external"),
            Err(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn only_transient_background_failures_are_retried() {
        use floe_knowledge::retryable_learner_failure;

        for failure in [
            AgentFailure::Cancelled,
            AgentFailure::DeadlineExceeded,
            AgentFailure::ModelUnavailable,
            AgentFailure::LocalModelUnavailable,
            AgentFailure::QuotaExceeded,
            AgentFailure::Interrupted,
        ] {
            assert!(retryable_learner_failure(failure));
        }
        for failure in [
            AgentFailure::InvalidModelOutput,
            AgentFailure::PolicyDenied,
            AgentFailure::BudgetExceeded,
            AgentFailure::StaleContext,
        ] {
            assert!(!retryable_learner_failure(failure));
        }
    }
}
