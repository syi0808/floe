use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use floe_agent_contract::{
    AgentContext, AgentFailure, AllowedCatalog, DataClass, JournalAck, JournalEvent,
    ModelCapabilities, ModelConversation, ModelConversationEntry, ModelPlanRequest, ModelPort,
    ModelProjectionOutcome, ModelRequest, ModelStep,
};
use floe_conversation::{AgentMessage, AgentOutcome, SessionStore};
use floe_execution::Cancellation;
use floe_execution::budget::{BudgetConfig, BudgetLedger, ModelUsage};
use floe_kernel::{PersonId, RunId, TraceContext};
use floe_knowledge::{
    KNOWLEDGE_VERSION, LEARNER_INFERENCE_CONSUMER, LEARNER_INFERENCE_PURPOSE,
    LearnerJournalFactory, LearnerModel, LearnerModelRequest, LearnerReviewOutput, LearnerService,
    parse_learner_review_output,
};
use floe_vault::{EncryptedAgentVault, VaultKey, VaultKeyProvider, VaultLearnerJournalFactory};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Default)]
struct SmokeKeys(Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>);

struct SmokeLearnerModel<'a> {
    model: &'a dyn ModelPort,
    journals: &'a dyn LearnerJournalFactory,
    device_id: &'a str,
}

impl LearnerModel for SmokeLearnerModel<'_> {
    async fn review(
        &self,
        request: LearnerModelRequest,
    ) -> Result<LearnerReviewOutput, AgentFailure> {
        validate_review_request(&request)?;
        let journal = self.journals.journal(
            request.input.person_id,
            request.input.run_id,
            request.claim_attempt,
        )?;
        let ledger = BudgetLedger::new(
            BudgetConfig::new(request.remaining_tokens, request.remaining_cost_micros),
            ModelUsage::default(),
        );
        let trace = TraceContext::new(request.input.run_id)
            .with_run_id(RunId::from_uuid(request.input.run_id).ok_or(AgentFailure::InvalidInput)?);
        let scope = floe_execution::ExecutionScope::root(
            request.cancellation.clone(),
            request.deadline,
            ledger.work_lease(),
            trace,
        );
        let plan_request = ModelPlanRequest {
            principal: request.input.person_id.to_string(),
            device_id: self.device_id.to_owned(),
            purpose: LEARNER_INFERENCE_PURPOSE.into(),
            consumer: LEARNER_INFERENCE_CONSUMER.into(),
            required_capabilities: ModelCapabilities::chat(),
        };
        let prepared = scope
            .run(self.model.prepare(plan_request.clone(), &scope))
            .await?;
        let plan = prepared.plan().clone();
        plan.validate()?;
        if plan.principal != plan_request.principal
            || plan.device_id != plan_request.device_id
            || plan.purpose != plan_request.purpose
            || plan.consumer != plan_request.consumer
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let origin_turn = request
            .input
            .turn_ids
            .last()
            .copied()
            .ok_or(AgentFailure::InvalidInput)?;
        let agent_context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: request.input.current_memories.clone(),
            optional_context_issues: vec![],
            evidence: vec![],
        };
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
        let projection =
            floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
                role: floe_context::ContextProjectionRole::Learner,
                plan: &plan,
                projection_operation_id: Uuid::new_v4(),
                purpose: LEARNER_INFERENCE_PURPOSE,
                response_contract: "One structured memory review answer.",
                correction: None,
                prompt: floe_knowledge::prompts::learner_prompt(),
                conversation,
                agent_context: &agent_context,
                catalog: &catalog,
                expert_environment: None,
                authorized_history_dependencies: &[],
                input_data_classes: vec![DataClass::Personal],
                max_output_bytes: request.max_output_bytes,
            })?;
        let projection = match projection {
            ModelProjectionOutcome::Ready(projection) => projection,
            ModelProjectionOutcome::NeedsSourceReview(_) => {
                return Err(AgentFailure::AccessReviewRequired);
            }
        };
        let attempt_id = Uuid::new_v4();
        let reservation_ceiling =
            floe_execution::budget::ModelReservationCeiling::for_lease(scope.budget());
        let intent = scope
            .run(journal.record_intent(JournalEvent::ModelIntent {
                reservation_ceiling,
                parent_task_id: None,
                attempt_id,
                projection_ref: projection.projection_ref,
                plan,
            }))
            .await?;
        if !matches!(intent, JournalAck::Accepted { .. }) {
            return Err(AgentFailure::Conflict);
        }
        let response = scope
            .run(prepared.generate(
                ModelRequest {
                    attempt_id,
                    reservation_ceiling,
                    principal: plan_request.principal,
                    device_id: plan_request.device_id,
                    purpose: plan_request.purpose,
                    consumer: plan_request.consumer,
                    projection,
                    catalog,
                    replay: vec![],
                },
                &scope,
            ))
            .await;
        let receipt = ledger.model_attempt_receipt(attempt_id);
        if receipt.is_none() && (response.is_ok() || ledger.model_attempt_admitted(attempt_id)) {
            return Err(AgentFailure::StorageUnavailable);
        }
        let (usage, accounting) = receipt.map_or_else(
            || {
                (
                    floe_agent_contract::ModelUsage::default(),
                    floe_agent_contract::ModelAccounting::default(),
                )
            },
            |receipt| {
                (
                    floe_agent_contract::ModelUsage {
                        tokens: receipt.charged_tokens,
                        cost_micros: receipt.charged_cost_micros,
                    },
                    receipt.accounting,
                )
            },
        );
        let acknowledgment = journal
            .record_result(JournalEvent::ModelResult {
                attempt_id,
                usage,
                accounting,
            })
            .await?;
        if !matches!(acknowledgment, JournalAck::Accepted { .. }) {
            return Err(AgentFailure::Conflict);
        }
        if receipt.is_some() {
            ledger.acknowledge_model_attempt(attempt_id)?;
        }
        let response = response?;
        if response.attempt_id != attempt_id
            || response.usage != usage
            || response.accounting != accounting
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
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
    if request.input.turn_ids.is_empty()
        || request.input.run_id.is_nil()
        || !(1..=floe_knowledge::MAX_LEARNER_JOB_ATTEMPTS).contains(&request.claim_attempt)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
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
    let vault =
        Arc::new(EncryptedAgentVault::create(root.path(), person, SmokeKeys::default()).await?);
    let mut session = vault.create_session().await?;
    let turn_id = Uuid::new_v4();
    session.messages = vec![
        AgentMessage::User {
            message_id: Uuid::new_v4(),
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
    let service = super::support::synthetic_model();
    let journals = VaultLearnerJournalFactory::new(Arc::clone(&vault));
    let model = SmokeLearnerModel {
        model: &service,
        journals: &journals,
        device_id: "synthetic-smoke-device",
    };
    let processed = LearnerService {
        model: &model,
        repository: vault.as_ref(),
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
        "boundary": "device",
        "personal_data": false,
        "disposable_encrypted_vault": true,
        "pending_candidates": pending.candidates.len(),
        "auto_approved": false,
        "explicit_expiry": with_expiry,
    }))
}
