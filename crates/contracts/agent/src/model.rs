use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_execution::ExecutionScope;

use crate::message::bounded_messages;
use crate::{
    AgentCard, AgentFailure, AgentMessage, Artifact, DependencyCoverage, InvocationKey, ToolResult,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSpec {
    pub role_id: String,
    pub prompt: String,
    pub output_contract: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoundedContext {
    pub text: String,
    pub coverage: DependencyCoverage,
}

impl RoleSpec {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.role_id.trim().is_empty()
            || self.prompt.trim().is_empty()
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

#[derive(Clone, Debug)]
pub struct EngineRequest {
    pub principal: String,
    pub role_spec: RoleSpec,
    pub prompt: String,
    pub scope: ExecutionScope,
    pub bounded_context: BoundedContext,
    pub messages: Vec<AgentMessage>,
    pub allowed_catalog: AllowedCatalog,
    pub max_iterations: u32,
    pub max_output_bytes: usize,
    pub replay: Vec<crate::ReplayReceipt>,
}

impl EngineRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.role_spec.validate()?;
        if self.principal.trim().is_empty()
            || self.prompt.trim().is_empty()
            || self.bounded_context.text.len() > 128 * 1024
            || self.max_iterations == 0
            || self.max_iterations > 64
            || self.max_output_bytes == 0
            || self.max_output_bytes > crate::MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.bounded_context
            .coverage
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if self.replay.len() > 128 {
            return Err(AgentFailure::BudgetExceeded);
        }
        bounded_messages(&self.messages)?;
        self.allowed_catalog
            .tools
            .iter()
            .try_for_each(ToolDescriptor::validate)?;
        self.allowed_catalog
            .cards
            .iter()
            .try_for_each(AgentDefinition::validate)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRequest {
    pub attempt_id: Uuid,
    pub role: RoleSpec,
    pub prompt: String,
    pub bounded_context: BoundedContext,
    pub messages: Vec<AgentMessage>,
    pub catalog: AllowedCatalog,
    pub replay: Vec<crate::ReplayReceipt>,
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
