//! Non-secret planning values shared by all model consumers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AgentFailure;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingBoundary {
    Device,
    Gateway,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Chat,
    StructuredOutput,
    ToolProposals,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelCapabilities(pub Vec<ModelCapability>);

impl ModelCapabilities {
    pub fn chat() -> Self {
        Self(vec![ModelCapability::Chat])
    }

    pub fn for_request(format: &crate::ModelOutputFormat, catalog: &crate::AllowedCatalog) -> Result<Self, AgentFailure> {
        format.validate().map_err(|_| AgentFailure::InvalidInput)?;
        let has_tools = !catalog.tools.is_empty() || !catalog.cards.is_empty();
        if format.is_json() && has_tools { return Err(AgentFailure::InvalidInput); }
        let mut values = vec![ModelCapability::Chat];
        if format.is_json() { values.push(ModelCapability::StructuredOutput); }
        if has_tools { values.push(ModelCapability::ToolProposals); }
        Ok(Self(values))
    }

    pub fn contains(&self, capability: ModelCapability) -> bool {
        self.0.contains(&capability)
    }

    pub fn includes(&self, required: &Self) -> bool {
        required
            .0
            .iter()
            .all(|capability| self.contains(*capability))
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.0.is_empty() || self.0.len() > 3 || !self.0.contains(&ModelCapability::Chat)
            || self.0.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Correlation only. Possession of a digest never authorizes a dispatch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelBindingDigest(pub [u8; 32]);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPlanRequest {
    pub principal: String,
    pub device_id: String,
    pub purpose: String,
    pub consumer: String,
    pub required_capabilities: ModelCapabilities,
}

impl ModelPlanRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let person = Uuid::parse_str(&self.principal).map_err(|_| AgentFailure::InvalidInput)?;
        if person.is_nil()
            || person.to_string() != self.principal
            || !valid_identifier(&self.device_id, 256)
            || !matches!(
                self.purpose.as_str(),
                "quick_response" | "everyday_assistance" | "deep_work"
            )
            || !valid_identifier(&self.consumer, 128)
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.required_capabilities.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedModelPlan {
    pub operation_id: Uuid,
    pub principal: String,
    pub device_id: String,
    pub purpose: String,
    pub consumer: String,
    pub capabilities: ModelCapabilities,
    pub boundary: ProcessingBoundary,
    pub binding_digest: ModelBindingDigest,
}

impl PreparedModelPlan {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.operation_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        ModelPlanRequest {
            principal: self.principal.clone(),
            device_id: self.device_id.clone(),
            purpose: self.purpose.clone(),
            consumer: self.consumer.clone(),
            required_capabilities: self.capabilities.clone(),
        }
        .validate()
    }
}

fn valid_identifier(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
