//! Background Learner input translation onto the shared prepared model port.

use std::sync::Arc;

use floe_agent_contract::{
    AgentContext, AgentFailure, AllowedCatalog, DataClass, JournalAck, JournalEvent,
    ModelCapabilities, ModelConversation, ModelConversationEntry, ModelPlanRequest, ModelPort,
    ModelProjectionOutcome, ModelRequest, ModelStep,
};
use floe_execution::Cancellation;
use floe_execution::budget::{BudgetConfig, BudgetLedger, ModelUsage};
use floe_kernel::{RunId, TraceContext};
use floe_knowledge::{
    KNOWLEDGE_VERSION, LEARNER_INFERENCE_CONSUMER, LEARNER_INFERENCE_PURPOSE,
    LearnerJournalFactory, LearnerModel, LearnerModelRequest, LearnerReviewOutput, LearnerService,
    parse_learner_review_output,
};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider, VaultLearnerJournalFactory};
use uuid::Uuid;

pub(super) async fn run<Keys: VaultKeyProvider + 'static>(
    vault: &Arc<EncryptedAgentVault<Keys>>,
    model: &dyn ModelPort,
    device_id: &str,
    cancellation: Cancellation,
) -> Result<bool, AgentFailure> {
    let journals = VaultLearnerJournalFactory::new(Arc::clone(vault));
    let model = LearnerModelHost {
        model,
        journals: &journals,
        device_id,
    };
    LearnerService {
        model: &model,
        repository: vault.as_ref(),
    }
    .run_next(cancellation)
    .await
}

struct LearnerModelHost<'a> {
    model: &'a dyn ModelPort,
    journals: &'a dyn LearnerJournalFactory,
    device_id: &'a str,
}

impl LearnerModel for LearnerModelHost<'_> {
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
