use std::{collections::HashMap, os::unix::fs::PermissionsExt, sync::Mutex};

use chrono::Utc;
use floe_agent_contract::{
    AgentContext, AgentFailure, AllowedCatalog, DataClass, ModelConversation,
    ModelConversationEntry, ModelRequest, ModelStep,
};
use floe_conversation::{AgentMessage, AgentOutcome, SessionStore};
use floe_execution::Cancellation;
use floe_execution::budget::{BudgetConfig, BudgetLedger, ModelUsage};
use floe_inference::{InferenceExecutionConstraint, InferenceExecutor};
use floe_kernel::{PersonId, RunId, TraceContext};
use floe_knowledge::{
    KNOWLEDGE_VERSION, LEARNER_INFERENCE_CONSUMER, LEARNER_INFERENCE_PURPOSE, LearnerModel,
    LearnerModelRequest, LearnerReviewOutput, LearnerService, parse_learner_review_output,
};
use floe_vault::{EncryptedAgentVault, VaultKey, VaultKeyProvider};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Default)]
struct SmokeKeys(Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>);

/// The bundled on-device model behind shared Inference, as the host binds it.
///
/// Knowledge owns the review; Context projects the input; Inference selects
/// the device profile and settles the attempt. The smoke path exercises the
/// same canonical composition production uses.
struct SmokeLearnerModel<'executor> {
    executor: &'executor dyn InferenceExecutor,
}

impl LearnerModel for SmokeLearnerModel<'_> {
    fn placement(&self) -> floe_agent_contract::ModelPlacement {
        floe_agent_contract::ModelPlacement::DeviceLocal
    }

    async fn review(&self, request: LearnerModelRequest) -> Result<LearnerReviewOutput, AgentFailure> {
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
        let conversation = ModelConversation {
            history: vec![],
            current_turn: vec![ModelConversationEntry::User {
                message_id: origin_turn,
                text: request.input.digest.clone(),
            }],
        };
        let catalog = AllowedCatalog {
            cards: vec![],
            tools: vec![],
            revision: 1,
        };
        let projection = floe_context::assemble_context_projection(
            floe_context::ContextProjectionInput {
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
            },
        )?;
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

struct SmokeResolver;

impl floe_access::DependencyResolver for SmokeResolver {
    fn authorize<'a>(
        &'a self,
        _dependency: &'a floe_context_contract::ContextDependency,
        _request: &'a floe_access::DependencyAuthorization,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>,
    > {
        Box::pin(async move { Err(AgentFailure::PolicyDenied) })
    }
}

struct SmokeAuthority;

impl floe_access::ModelDispatchRecipientAuthority for SmokeAuthority {
    fn check_recipient(&self, _recipient: &str) -> Result<(), AgentFailure> {
        Err(AgentFailure::PolicyDenied)
    }
}

impl VaultKeyProvider for SmokeKeys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
        self.0
            .lock()
            .map_err(|_| AgentFailure::VaultUnavailable)?
            .get(&(person, vault))
            .copied()
            .map(VaultKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }

    fn insert(&self, person: PersonId, vault: Uuid, key: &VaultKey) -> Result<(), AgentFailure> {
        let mut keys = self.0.lock().map_err(|_| AgentFailure::VaultUnavailable)?;
        if keys.contains_key(&(person, vault)) {
            return Err(AgentFailure::VaultUnavailable);
        }
        keys.insert((person, vault), *key.as_bytes());
        Ok(())
    }
}

pub(super) async fn run(with_expiry: bool) -> Result<Value, AgentFailure> {
    let root = tempfile::tempdir().map_err(|_| AgentFailure::StorageUnavailable)?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    let person = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person, SmokeKeys::default()).await?;
    let mut session = vault.create_session().await?;
    let turn_id = Uuid::new_v4();
    session.messages = vec![
        AgentMessage::User {
            turn_id,
            text: if with_expiry {
                "Please remember for later: fictional Alex prefers afternoon meetings. This preference expires at 2027-01-01T00:00:00Z, with no start date."
            } else {
                "Please remember for later: fictional Alex prefers afternoon meetings."
            }.into(),
        },
        AgentMessage::Assistant {
            turn_id,
            text: "I will prepare the fictional preference for review.".into(),
        },
    ];
    session.revision = 1;
    session.last_outcome = Some(AgentOutcome::Completed);
    vault
        .governed_general_store(session.id)
        .compare_and_swap(&session, 0)
        .await?;
    let now = Utc::now();
    let queued = vault.discover_explicit_learner_reviews(now, 1).await?;
    if queued.len() != 1 {
        return Err(AgentFailure::Conflict);
    }
    let provider = floe_provider_adapters::models::FoundationModelProvider::scoped(
        floe_agent_contract::SessionProtection::Encrypted,
        LEARNER_INFERENCE_PURPOSE,
        LEARNER_INFERENCE_CONSUMER,
    )?;
    let service =
        floe_inference::InferenceService::new(provider, SmokeResolver, SmokeAuthority);
    let model = SmokeLearnerModel { executor: &service };
    let processed = LearnerService {
        model: &model,
        repository: &vault,
    }
    .run_next(Cancellation::default())
    .await?;
    let settled = vault
        .enqueue_learner_review(queued[0].input.clone(), Utc::now())
        .await?;
    if !processed || settled.state != floe_knowledge::LearnerJobState::Completed {
        return Err(settled.last_failure.unwrap_or(AgentFailure::Conflict));
    }
    let pending = vault.memory_review_snapshot().await?;
    if pending.candidates.len() != usize::from(settled.candidate_id.is_some())
        || pending.candidates.first().map(|candidate| candidate.id) != settled.candidate_id
        || (with_expiry && pending.candidates.is_empty())
    {
        return Err(AgentFailure::Conflict);
    }
    if let Some(candidate) = pending.candidates.first() {
        let floe_knowledge::KnowledgePayload::Memory { value } = &candidate.payload else {
            return Err(AgentFailure::InvalidModelOutput);
        };
        let statement = value.statement.to_lowercase();
        let expected_expiry = if with_expiry {
            Some(
                chrono::DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
                    .map_err(|_| AgentFailure::InvalidInput)?
                    .with_timezone(&Utc),
            )
        } else {
            None
        };
        if value.valid_from.is_some()
            || value.valid_until != expected_expiry
            || !statement.contains("alex")
            || !statement.contains("afternoon")
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    }
    Ok(json!({
        "schema_version": 1,
        "status": "passed",
        "profile": "foundation-device",
        "personal_data": false,
        "disposable_encrypted_vault": true,
        "pending_candidates": pending.candidates.len(),
        "auto_approved": false,
        "explicit_expiry": with_expiry,
    }))
}
