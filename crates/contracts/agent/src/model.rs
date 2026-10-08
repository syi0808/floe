use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ValidatedFinalPayload {
    pub text: String,
    pub artifacts: Vec<crate::Artifact>,
}

use floe_execution::ExecutionScope;

use crate::{
    AgentCard, AgentFailure, Artifact, AuthorizedModelProjection, InvocationKey, ModelConversation,
    ToolResult,
};

/// Role-specific instructions only: never the rendered behavior kernel, persona,
/// or capability protocol. The provider's system instruction source is
/// [`ContextEnvelope`](crate::ContextEnvelope) `stable_instructions`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSpec {
    pub role_id: String,
    pub instructions: String,
    pub output_contract: String,
    pub output_format: crate::ModelOutputFormat,
}

impl RoleSpec {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.role_id.trim().is_empty()
            || self.instructions.trim().is_empty()
            || self.output_contract.trim().is_empty()
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.output_format
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedCatalog {
    pub cards: Vec<AgentDefinition>,
    pub tools: Vec<ToolDescriptor>,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDefinition {
    pub card: AgentCard,
    pub definition_revision: u64,
}

impl AgentDefinition {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        (self.definition_revision > 0)
            .then_some(())
            .ok_or(AgentFailure::InvalidInput)?;
        self.card.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDescriptor {
    pub id: String,
    pub definition_revision: u64,
    pub description: String,
    pub input_schema: String,
    pub output_data_class: String,
}

impl ToolDescriptor {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.id.trim().is_empty()
            || self.definition_revision == 0
            || self.description.trim().is_empty()
            || self.input_schema.trim().is_empty()
        {
            return Err(AgentFailure::InvalidInput);
        }
        let schema = serde_json::from_str::<serde_json::Value>(&self.input_schema)
            .map_err(|_| AgentFailure::InvalidInput)?;
        if !schema.is_object() {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

pub fn validate_tool_input(input: &str) -> Result<(), AgentFailure> {
    let value = serde_json::from_str::<serde_json::Value>(input)
        .map_err(|_| AgentFailure::InvalidModelOutput)?;
    value
        .is_object()
        .then_some(())
        .ok_or(AgentFailure::InvalidModelOutput)
}

/// A validated batch restored for replay: the Engine executes the stored steps
/// from the cursor instead of calling the model again. The batch keeps its
/// original execution id, so step identity never depends on the resuming run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineResumeState {
    pub validated_batch: crate::ValidatedModelBatch,
    pub cursor: crate::BatchCursor,
}

impl EngineResumeState {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        self.validated_batch.validate(maximum_bytes)?;
        self.cursor.validate()?;
        if self.cursor.batch_id != self.validated_batch.batch_id
            || self.cursor.next_step_index as usize >= self.validated_batch.steps.len()
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct EngineRequest {
    pub execution_id: Uuid,
    pub principal: String,
    pub device_id: String,
    pub role_spec: RoleSpec,
    pub scope: ExecutionScope,
    pub conversation: ModelConversation,
    pub allowed_catalog: AllowedCatalog,
    pub purpose: String,
    pub consumer: String,
    pub max_iterations: u32,
    pub max_output_bytes: usize,
    pub replay: Vec<crate::ReplayReceipt>,
    pub resume: Option<EngineResumeState>,
    /// The selected provider/profile state for this logical Agent execution.
    pub model_selection: crate::ModelSelectionState,
    /// The explicit host context delegate steps execute under, supplied by
    /// the Conversation owner. Required when a validated batch contains a
    /// Delegate step; absent otherwise.
    pub delegation_context: Option<crate::DelegationContextInput>,
}

impl EngineRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.role_spec.validate()?;
        crate::ModelPlanRequest {
            principal: self.principal.clone(),
            device_id: self.device_id.clone(),
            purpose: self.purpose.clone(),
            consumer: self.consumer.clone(),
            required_capabilities: crate::ModelCapabilities::for_request(
                &self.role_spec.output_format,
                &self.allowed_catalog,
            )?,
        }
        .validate()?;
        self.model_selection.validate()?;
        let required_capabilities = crate::ModelCapabilities::for_request(
            &self.role_spec.output_format,
            &self.allowed_catalog,
        )?;
        match &self.model_selection {
            crate::ModelSelectionState::Fresh if self.resume.is_some() => {
                return Err(AgentFailure::InvalidInput);
            }
            crate::ModelSelectionState::Pinned(selection)
                if selection.principal != self.principal
                    || selection.device_id != self.device_id
                    || selection.purpose != self.purpose
                    || selection.consumer != self.consumer
                    || !selection.capabilities.includes(&required_capabilities) =>
            {
                return Err(AgentFailure::PolicyDenied);
            }
            // A historical batch without a complete selection pin may still
            // replay its already validated steps. The Engine rejects the
            // following model attempt before projection or provider handoff.
            crate::ModelSelectionState::Unproven if self.resume.is_none() => {
                return Err(AgentFailure::PolicyDenied);
            }
            _ => {}
        }
        if self.execution_id.is_nil()
            || self.principal.trim().is_empty()
            || self.purpose.trim().is_empty()
            || self.purpose.len() > 512
            || self.consumer.trim().is_empty()
            || self.consumer.len() > 256
            || self.max_iterations == 0
            || self.max_iterations > 64
            || self.max_output_bytes == 0
            || self.max_output_bytes > crate::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self.replay.len() > 128 {
            return Err(AgentFailure::BudgetExceeded);
        }
        self.conversation.validate()?;
        self.allowed_catalog
            .tools
            .iter()
            .try_for_each(ToolDescriptor::validate)?;
        self.allowed_catalog
            .cards
            .iter()
            .try_for_each(AgentDefinition::validate)?;
        if let Some(resume) = &self.resume {
            resume.validate(self.max_output_bytes)?;
            if resume.validated_batch.execution_id != self.execution_id {
                return Err(AgentFailure::Conflict);
            }
        }
        if let Some(context) = &self.delegation_context {
            context.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRequest {
    pub attempt_id: Uuid,
    pub reservation_ceiling: floe_execution::budget::ModelReservationCeiling,
    pub principal: String,
    pub device_id: String,
    pub projection: AuthorizedModelProjection,
    pub catalog: AllowedCatalog,
    pub purpose: String,
    pub consumer: String,
    pub replay: Vec<crate::ReplayReceipt>,
}

impl ModelRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.reservation_ceiling.validate()?;
        crate::ModelPlanRequest {
            principal: self.principal.clone(),
            device_id: self.device_id.clone(),
            purpose: self.purpose.clone(),
            consumer: self.consumer.clone(),
            required_capabilities: crate::ModelCapabilities::for_request(
                &self.projection.envelope.run_instructions.output_format,
                &self.catalog,
            )?,
        }
        .validate()?;
        if self.attempt_id.is_nil()
            || self.principal.trim().is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.purpose.trim().is_empty()
            || self.purpose.len() > 512
            || self.consumer.trim().is_empty()
            || self.consumer.len() > 256
            || self.replay.len() > 128
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.projection.validate()?;
        self.catalog
            .tools
            .iter()
            .try_for_each(ToolDescriptor::validate)?;
        self.catalog
            .cards
            .iter()
            .try_for_each(AgentDefinition::validate)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelStep {
    Preamble {
        text: String,
    },
    Answer {
        text: String,
        artifacts: Vec<Artifact>,
    },
    CallTool {
        tool_id: String,
        definition_revision: u64,
        input: String,
    },
    Delegate {
        agent_id: String,
        definition_revision: u64,
        message: String,
        context_refs: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCall {
    pub call_id: Uuid,
    pub invocation_key: InvocationKey,
    pub tool_id: String,
    pub definition_revision: u64,
    pub input: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelResponse {
    pub attempt_id: Uuid,
    pub steps: Vec<ModelStep>,
    pub usage: ModelUsage,
    pub accounting: floe_execution::budget::ModelAccounting,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelUsage {
    pub tokens: u64,
    pub cost_micros: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineStep {
    Answer {
        text: String,
        artifacts: Vec<Artifact>,
    },
    Tool(ToolResult),
    Delegation(Box<crate::TaskReceipt>),
}

#[cfg(test)]
mod model_selection_tests {
    use super::*;
    use crate::{BatchCursor, ModelConversationEntry, ModelSelectionState, ProjectionRef};
    use floe_execution::{Cancellation, budget::BudgetLedger};
    use floe_kernel::TraceContext;
    use std::time::Duration;

    fn request(
        model_selection: ModelSelectionState,
        resume: Option<EngineResumeState>,
    ) -> EngineRequest {
        let execution_id = resume
            .as_ref()
            .map_or_else(Uuid::new_v4, |resume| resume.validated_batch.execution_id);
        let ledger = BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            floe_execution::budget::ModelUsage::default(),
        );
        EngineRequest {
            execution_id,
            principal: "00000000-0000-4000-8000-000000000001".into(),
            device_id: "device-1".into(),
            role_spec: RoleSpec {
                role_id: "manager".into(),
                instructions: "Help the person.".into(),
                output_contract: "Return a concise answer.".into(),
                output_format: crate::ModelOutputFormat::Text,
            },
            scope: floe_execution::ExecutionScope::root(
                Cancellation::new(),
                tokio::time::Instant::now() + Duration::from_secs(30),
                ledger.work_lease(),
                TraceContext::new(execution_id),
            ),
            conversation: ModelConversation {
                history: vec![],
                current_turn: vec![ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "hello".into(),
                }],
            },
            allowed_catalog: AllowedCatalog::default(),
            purpose: "everyday_assistance".into(),
            consumer: "floe.conversation.manager".into(),
            max_iterations: 1,
            max_output_bytes: crate::MAX_OUTPUT_BYTES,
            replay: vec![],
            resume,
            model_selection,
            delegation_context: None,
        }
    }

    fn pending_resume() -> EngineResumeState {
        let execution_id = Uuid::new_v4();
        let batch_id = Uuid::new_v4();
        EngineResumeState {
            validated_batch: crate::ValidatedModelBatch {
                execution_id,
                attempt_id: Uuid::new_v4(),
                projection_ref: ProjectionRef::new(),
                batch_id,
                steps: vec![crate::ModelStep::Answer {
                    text: "already validated".into(),
                    artifacts: vec![],
                }],
                catalog_revision: 0,
                tool_revisions: vec![],
                agent_revisions: vec![],
                projection_coverage: crate::DependencyCoverage::Independent,
                delegation_context: None,
            },
            cursor: BatchCursor {
                batch_id,
                next_step_index: 0,
            },
        }
    }

    #[test]
    fn unproven_legacy_selection_allows_only_a_pending_batch_resume() {
        assert_eq!(
            request(ModelSelectionState::Unproven, None).validate(),
            Err(AgentFailure::PolicyDenied),
            "historical evidence without a complete pin cannot start a model dispatch"
        );
        assert!(
            request(ModelSelectionState::Unproven, Some(pending_resume()))
                .validate()
                .is_ok(),
            "an already validated pending batch remains replayable"
        );
    }
}
