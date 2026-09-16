use floe_agent_contract::{AgentFailure, DataClass, ModelPlacement, TransferConsent};
use floe_context::{AgentContext, InferencePolicyDecision};
use floe_kernel::AGENT_VERSION;
use floe_conversation::{AgentMessage, AgentUsage, ModelRequest, ModelRunner, ModelStep};
use floe_conversation::{UsageLedger, generate_with_recovery};
use floe_knowledge::prompts::{learner_prompt};
use floe_inference::{
    DataRecipient, ExecutionLocation, ModelCapabilities, ModelConsumer, ModelProfile, ModelPurpose,
    PlannedRoute,
};
use floe_knowledge::{
    LEARNER_INFERENCE_CAPABILITY, LEARNER_INFERENCE_CONSUMER, LEARNER_INFERENCE_PURPOSE,
    LearnerInferenceResponse, LearnerInferenceTransport, LearnerModelRequest,
};

use crate::models::foundation::{FoundationModelRunner, LocalModelAvailability};

const FOUNDATION_LEARNER_PROFILE: &str = "apple-foundation-models-26-learner";

pub struct FoundationLearnerTransport;

fn foundation_profile(available: bool) -> Result<ModelProfile, AgentFailure> {
    Ok(ModelProfile {
        id: FOUNDATION_LEARNER_PROFILE.into(),
        purpose: ModelPurpose::new(LEARNER_INFERENCE_PURPOSE).ok_or(AgentFailure::InvalidInput)?,
        consumer: ModelConsumer::new(LEARNER_INFERENCE_CONSUMER)
            .ok_or(AgentFailure::InvalidInput)?,
        execution_location: ExecutionLocation::Device,
        data_recipient: DataRecipient::Device,
        capabilities: ModelCapabilities(vec![LEARNER_INFERENCE_CAPABILITY.into()]),
        available,
        external_transfer_consent: false,
    })
}

impl LearnerInferenceTransport for FoundationLearnerTransport {
    fn profile(&self) -> Result<ModelProfile, AgentFailure> {
        let availability = FoundationModelRunner::encrypted().availability()?;
        foundation_profile(availability == LocalModelAvailability::Available)
    }

    async fn generate(
        &self,
        route: PlannedRoute,
        request: LearnerModelRequest,
    ) -> Result<LearnerInferenceResponse, AgentFailure> {
        review_with_model(&FoundationModelRunner::encrypted(), route, request).await
    }
}

async fn review_with_model(
    model: &(impl ModelRunner + Sync),
    route: PlannedRoute,
    request: LearnerModelRequest,
) -> Result<LearnerInferenceResponse, AgentFailure> {
    if model.placement() != ModelPlacement::DeviceLocal
        || route.profile_id != FOUNDATION_LEARNER_PROFILE
        || route.purpose.as_str() != LEARNER_INFERENCE_PURPOSE
        || route.consumer.as_str() != LEARNER_INFERENCE_CONSUMER
        || route.execution_location != ExecutionLocation::Device
        || route.data_recipient != DataRecipient::Device
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let turn_id = request
        .input
        .turn_ids
        .last()
        .copied()
        .ok_or(AgentFailure::InvalidInput)?;
    let response = generate_with_recovery(
        model,
        ModelRequest {
            usage: UsageLedger::new(
                request.remaining_tokens,
                request.remaining_cost_micros,
                Default::default(),
            ),
            replay: vec![],
            schema_version: AGENT_VERSION,
            prompt: learner_prompt(),
            person_id: request.input.person_id,
            session_id: request.input.session_id,
            turn_id,
            policy: InferencePolicyDecision {
                purpose: LEARNER_INFERENCE_PURPOSE.into(),
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
    Ok(LearnerInferenceResponse {
        text: text.clone(),
        used_tokens: response.used_tokens,
        cost_micros: response.cost_micros,
    })
}

#[cfg(test)]
mod tests;
