use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AGENT_SCHEMA_VERSION, AgentFailure, DependencyCoverage, MAX_AGENT_MESSAGES, MAX_OUTPUT_BYTES,
    ModelPlacement,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCard {
    pub schema_version: u32,
    pub protocol_version: String,
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub domain_tags: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    /// Where this agent's judgment can run.
    ///
    /// Eligibility is read from the card, not inferred from the agent's
    /// identity: a caller on the device model offers only the cards that say
    /// they run there.
    #[serde(default = "every_placement")]
    pub supported_placements: Vec<ModelPlacement>,
}

fn every_placement() -> Vec<ModelPlacement> {
    vec![ModelPlacement::DeviceLocal, ModelPlacement::Remote]
}

impl AgentCard {
    /// Whether this agent can answer on a caller running at `placement`.
    pub fn runs_at(&self, placement: ModelPlacement) -> bool {
        self.supported_placements.contains(&placement)
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != AGENT_SCHEMA_VERSION
            || self.protocol_version != crate::A2A_PROTOCOL_VERSION
            || !bounded(&self.id, 128)
            || !bounded(&self.version, 64)
            || !bounded(&self.name, 128)
            || !bounded(&self.description, 512)
            || self.domain_tags.len() > 8
            || self.skills.len() > 8
            || self.domain_tags.iter().any(|value| !bounded(value, 64))
            || self.skills.iter().any(|value| !bounded(value, 256))
            || self.supported_placements.is_empty()
            || self.supported_placements.len() > 2
            || self.supported_placements[1..].contains(&self.supported_placements[0])
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Preamble,
    Assistant,
    Tool,
    Delegation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentMessage {
    pub message_id: Uuid,
    pub role: MessageRole,
    pub text: String,
    pub call_id: Option<Uuid>,
    pub coverage: DependencyCoverage,
}

impl AgentMessage {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !bounded(&self.text, MAX_OUTPUT_BYTES) {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub artifact_id: Uuid,
    pub name: String,
    pub parts: Vec<ArtifactPart>,
    pub coverage: DependencyCoverage,
}

impl Artifact {
    pub fn validate(&self, maximum_bytes: usize) -> Result<(), AgentFailure> {
        if self.artifact_id.is_nil()
            || !bounded(&self.name, 256)
            || self.parts.iter().any(|part| match part {
                ArtifactPart::Text { text } => !bounded(text, maximum_bytes),
                ArtifactPart::Data { media_type, data } => {
                    !bounded(media_type, 256) || !bounded(data, maximum_bytes)
                }
            })
            || self.coverage.validate().is_err()
            || serde_json::to_vec(self)
                .map(|encoded| encoded.len() > maximum_bytes)
                .unwrap_or(true)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactPart {
    Text { text: String },
    Data { media_type: String, data: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolResult {
    pub call_id: Uuid,
    pub text: String,
    pub artifacts: Vec<Artifact>,
    pub coverage: DependencyCoverage,
    pub issue: Option<OutcomeIssue>,
}

impl ToolResult {
    pub fn validate(
        &self,
        expected_call_id: Uuid,
        maximum_bytes: usize,
    ) -> Result<(), AgentFailure> {
        if self.call_id != expected_call_id
            || !bounded(&self.text, maximum_bytes)
            || self.coverage.validate().is_err()
            || self
                .artifacts
                .iter()
                .any(|artifact| artifact.validate(maximum_bytes).is_err())
            || serde_json::to_vec(self)
                .map(|encoded| encoded.len() > maximum_bytes)
                .unwrap_or(true)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeIssue {
    pub failure: AgentFailure,
    pub retryable: bool,
}

pub(crate) fn bounded(value: &str, max: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max
        && !value
            .chars()
            .any(|character| character.is_control() && character != '\n')
}

pub(crate) fn bounded_messages(messages: &[AgentMessage]) -> Result<(), AgentFailure> {
    if messages.len() > MAX_AGENT_MESSAGES {
        return Err(AgentFailure::BudgetExceeded);
    }
    messages.iter().try_for_each(AgentMessage::validate)
}
