use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

impl RoleSpec {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.role_id.trim().is_empty()
            || self.instructions.trim().is_empty()
            || self.output_contract.trim().is_empty()
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
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
    pub principal: String,
    pub role_spec: RoleSpec,
    pub scope: ExecutionScope,
    pub conversation: ModelConversation,
    pub allowed_catalog: AllowedCatalog,
    pub purpose: String,
    pub consumer: String,
    pub preferred_profile_id: Option<String>,
    pub max_iterations: u32,
    pub max_output_bytes: usize,
    pub replay: Vec<crate::ReplayReceipt>,
    pub resume: Option<EngineResumeState>,
}

impl EngineRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.role_spec.validate()?;
        if self.principal.trim().is_empty()
            || self.purpose.trim().is_empty()
            || self.purpose.len() > 512
            || self.consumer.trim().is_empty()
            || self.consumer.len() > 256
            || self
                .preferred_profile_id
                .as_ref()
                .is_some_and(|profile| profile.trim().is_empty() || profile.len() > 128)
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
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRequest {
    pub attempt_id: Uuid,
    pub principal: String,
    pub projection: AuthorizedModelProjection,
    pub catalog: AllowedCatalog,
    pub purpose: String,
    pub consumer: String,
    pub preferred_profile_id: Option<String>,
    pub replay: Vec<crate::ReplayReceipt>,
}

impl ModelRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.attempt_id.is_nil()
            || self.principal.trim().is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.purpose.trim().is_empty()
            || self.purpose.len() > 512
            || self.consumer.trim().is_empty()
            || self.consumer.len() > 256
            || self
                .preferred_profile_id
                .as_ref()
                .is_some_and(|profile| profile.trim().is_empty() || profile.len() > 128)
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
