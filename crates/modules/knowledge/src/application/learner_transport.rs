//! The Learner's own model call, expressed over a role-neutral transport.
//!
//! What the Learner sends — its prompt, the memories it is reviewing against,
//! the single Answer it will accept — is Knowledge's judgment. A transport only
//! carries it. Where the model actually runs is a route Inference plans, and
//! this insists it stays on the device.

use floe_agent_contract::{
    AGENT_VERSION, AgentCard, AgentContext, AgentFailure, ContextEnvelope, ContextManifest,
    ContextualData, ConversationContext, DataClass, EvidenceManifestEntry,
    InferencePolicyDecision, MemoryManifestEntry, ModelPlacement, PromptManifestEntry,
    RuntimeContext, ScopedInstructions, TransferConsent,
};
use floe_inference::{
    DataRecipient, ExecutionLocation, ModelCapabilities, ModelConsumer, ModelProfile, ModelPurpose,
    ModelStep, ModelTransport, ModelTransportRequest, PlannedRoute,
};
use uuid::Uuid;

use super::inference::{
    LEARNER_INFERENCE_CAPABILITY, LEARNER_INFERENCE_CONSUMER, LEARNER_INFERENCE_PURPOSE,
    LearnerInferenceResponse, LearnerInferenceTransport,
};
use super::learner::LearnerModelRequest;
use crate::prompts::learner_prompt;

/// The profile a device-local Learner model is offered under.
///
/// The role binding is Knowledge's; the adapter only reports whether the model
/// behind it is available.
pub const FOUNDATION_LEARNER_PROFILE: &str = "apple-foundation-models-26-learner";

pub fn learner_profile(profile_id: &str, available: bool) -> Result<ModelProfile, AgentFailure> {
    Ok(ModelProfile {
        id: profile_id.into(),
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

/// What a model must report about itself before the Learner will use it.
pub trait LearnerModelAvailability {
    fn profile_id(&self) -> &str;
    fn is_available(&self) -> Result<bool, AgentFailure>;
}

/// The Learner's transport, assembled from any device-local model transport.
pub struct TransportLearnerModel<Transport> {
    transport: Transport,
}

impl<Transport> TransportLearnerModel<Transport> {
    pub const fn new(transport: Transport) -> Self {
        Self { transport }
    }
}

impl<Transport: ModelTransport + LearnerModelAvailability + Sync> LearnerInferenceTransport
    for TransportLearnerModel<Transport>
{
    fn profile(&self) -> Result<ModelProfile, AgentFailure> {
        learner_profile(self.transport.profile_id(), self.transport.is_available()?)
    }

    async fn generate(
        &self,
        route: PlannedRoute,
        request: LearnerModelRequest,
    ) -> Result<LearnerInferenceResponse, AgentFailure> {
        review_with_model(&self.transport, self.transport.profile_id(), route, request).await
    }
}

/// Send one Learner review to a transport and read the single Answer back.
pub async fn review_with_model(
    transport: &(impl ModelTransport + Sync),
    profile_id: &str,
    route: PlannedRoute,
    request: LearnerModelRequest,
) -> Result<LearnerInferenceResponse, AgentFailure> {
    if transport.placement() != ModelPlacement::DeviceLocal
        || route.profile_id != profile_id
        || route.purpose.as_str() != LEARNER_INFERENCE_PURPOSE
        || route.consumer.as_str() != LEARNER_INFERENCE_CONSUMER
        || route.execution_location != ExecutionLocation::Device
        || route.data_recipient != DataRecipient::Device
    {
        return Err(AgentFailure::PolicyDenied);
    }
    // The review must name the turn it came from, even though the turn itself
    // does not cross to the transport.
    if request.input.turn_ids.last().is_none() {
        return Err(AgentFailure::InvalidInput);
    }
    let prompt = learner_prompt();
    prompt.validate()?;
    let policy = InferencePolicyDecision {
        purpose: LEARNER_INFERENCE_PURPOSE.into(),
        data_classes: vec![DataClass::Personal],
        allowed_placements: vec![ModelPlacement::DeviceLocal],
        performance_class: "background".into(),
        projection_version: 1,
        external_transfer_consent: TransferConsent::NotGranted,
        bounded_sensitive_projection: false,
    };
    let context = AgentContext {
        projection_version: 1,
        persona: None,
        memories: request.input.current_memories.clone(),
        optional_context_issues: vec![],
        evidence: vec![],
    };
    context.validate()?;
    let envelope = ContextEnvelope {
        schema_version: AGENT_VERSION,
        stable_instructions: prompt.clone(),
        scoped_instructions: ScopedInstructions {
            purpose: policy.purpose.clone(),
            available_capabilities: vec![],
            active_experts: Vec::<AgentCard>::new(),
        },
        contextual_data: ContextualData {
            projection_version: context.projection_version,
            memories: context.memories.clone(),
            optional_context_issues: vec![],
            evidence: vec![],
        },
        // The Learner reviews one digest; there is no conversation to project.
        conversation: ConversationContext {
            history: vec![],
            current_turn: vec![serde_json::json!({
                "role": "user",
                "content": request.input.digest,
            })],
        },
        runtime: RuntimeContext {
            max_output_bytes: request.max_output_bytes.min(16384),
        },
        manifest: ContextManifest {
            prompt_components: prompt
                .components
                .iter()
                .map(|component| PromptManifestEntry {
                    kind: component.kind,
                    source: component.source.clone(),
                    revision: component.revision,
                })
                .collect(),
            evidence: Vec::<EvidenceManifestEntry>::new(),
            memories: context
                .memories
                .iter()
                .map(|memory| MemoryManifestEntry {
                    target_id: memory.target_id,
                    revision: memory.revision,
                    source_refs: memory.source_refs.clone(),
                })
                .collect(),
            agent_cards: vec![],
        },
    };
    let response = transport
        .generate(ModelTransportRequest {
            schema_version: AGENT_VERSION,
            attempt_id: Uuid::new_v4(),
            prompt,
            policy,
            context,
            envelope,
            capabilities: vec![],
            active_agents: vec![],
            replay: vec![],
            remaining_tokens: request.remaining_tokens,
            remaining_cost_micros: request.remaining_cost_micros,
            max_output_bytes: request.max_output_bytes,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        })
        .await?;
    // A Learner review is one structured answer. A replay, a tool call or a
    // second step means the model did something this role never asked for.
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
