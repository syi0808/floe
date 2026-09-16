//! Prompt assembly: the role-neutral kernel, capability protocol, persona and
//! the structural rules every assembled prompt must satisfy.
//!
//! A role's own text belongs to that role's owner.

use floe_agent_contract::AgentFailure;
use serde::{Deserialize, Serialize};

pub const AGENT_VERSION: u32 = 1;

pub const BEHAVIOR_KERNEL: &str = include_str!("../prompts/behavior_kernel.txt");
pub const CAPABILITY_PROTOCOL: &str = include_str!("../prompts/capability_protocol.txt");
pub const MODEL_CORRECTION: &str = include_str!("../prompts/model_correction.txt");
const DEFAULT_PERSONA: &str = include_str!("../prompts/default_persona.txt");
pub const BEHAVIOR_KERNEL_REVISION: u64 = 3;
pub const CAPABILITY_PROTOCOL_REVISION: u64 = 3;

pub const LEARNER_ROLE: &str = include_str!("../prompts/learner_role.txt");
pub const LEARNER_PROTOCOL: &str = include_str!("../prompts/learner_protocol.txt");
pub const LEARNER_ROLE_REVISION: u64 = 2;
pub const LEARNER_PROTOCOL_REVISION: u64 = 3;
pub const LEARNER_EXTRACTOR_VERSION: &str = "memory-extractor-v3";
pub const LEARNER_PROMPT_VERSION: &str = "memory-review-v3";


#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptRole {
    Manager,
    ScheduleExpert,
    CommitmentsExpert,
    CommunicationExpert,
    RelationshipsExpert,
    FocusAttentionExpert,
    WellbeingExpert,
    WorkContextExpert,
    LifeLogisticsExpert,
    Learner,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptComponentKind {
    BehaviorKernel,
    Role,
    Persona,
    CapabilityProtocol,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PersonaProfile {
    pub revision: u64,
    pub source: String,
    pub instructions: String,
}

impl Default for PersonaProfile {
    fn default() -> Self {
        Self {
            revision: 1,
            source: "floe.default".into(),
            instructions: DEFAULT_PERSONA.trim().into(),
        }
    }
}

impl PersonaProfile {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        validate_persona(self)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromptComponent {
    pub kind: PromptComponentKind,
    pub source: String,
    pub revision: u64,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PromptAssembly {
    pub schema_version: u32,
    pub role: PromptRole,
    pub components: Vec<PromptComponent>,
}

impl PromptAssembly {
    pub fn render(&self) -> String {
        self.components
            .iter()
            .map(|component| component.content.trim())
            .filter(|content| !content.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.schema_version != AGENT_VERSION
            || self.components.is_empty()
            || self.components.len() > 8
            || self.components.iter().any(|component| {
                component.source.trim().is_empty()
                    || component.source.len() > 128
                    || component.revision == 0
                    || component.content.trim().is_empty()
                    || component.content.len() > 4096
            })
            || self.render().len() > 8192
        {
            return Err(AgentFailure::InvalidInput);
        }
        for required in [
            PromptComponentKind::BehaviorKernel,
            PromptComponentKind::Role,
            PromptComponentKind::CapabilityProtocol,
        ] {
            if self
                .components
                .iter()
                .filter(|component| component.kind == required)
                .count()
                != 1
            {
                return Err(AgentFailure::InvalidInput);
            }
        }
        let persona_count = self
            .components
            .iter()
            .filter(|component| component.kind == PromptComponentKind::Persona)
            .count();
        if persona_count > 1 || (self.role != PromptRole::Manager && persona_count != 0) {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}


/// Assemble a role prompt from the shared kernel, the role text and the protocol.
pub fn expert_prompt(
    role: PromptRole,
    source: &str,
    revision: u64,
    content: &str,
) -> PromptAssembly {
    PromptAssembly {
        schema_version: AGENT_VERSION,
        role,
        components: vec![
            product_component(
                PromptComponentKind::BehaviorKernel,
                "behavior-kernel",
                BEHAVIOR_KERNEL_REVISION,
                BEHAVIOR_KERNEL,
            ),
            product_component(PromptComponentKind::Role, source, revision, content),
            product_component(
                PromptComponentKind::CapabilityProtocol,
                "capability-protocol",
                CAPABILITY_PROTOCOL_REVISION,
                CAPABILITY_PROTOCOL,
            ),
        ],
    }
}
pub fn learner_prompt() -> PromptAssembly {
    PromptAssembly {
        schema_version: AGENT_VERSION,
        role: PromptRole::Learner,
        components: vec![
            product_component(
                PromptComponentKind::BehaviorKernel,
                "behavior-kernel",
                BEHAVIOR_KERNEL_REVISION,
                BEHAVIOR_KERNEL,
            ),
            product_component(
                PromptComponentKind::Role,
                "learner-role",
                LEARNER_ROLE_REVISION,
                LEARNER_ROLE,
            ),
            product_component(
                PromptComponentKind::CapabilityProtocol,
                "learner-protocol",
                LEARNER_PROTOCOL_REVISION,
                LEARNER_PROTOCOL,
            ),
        ],
    }
}

pub fn product_component(
    kind: PromptComponentKind,
    source: &str,
    revision: u64,
    content: &str,
) -> PromptComponent {
    PromptComponent {
        kind,
        source: source.into(),
        revision,
        content: content.trim().into(),
    }
}

fn validate_persona(persona: &PersonaProfile) -> Result<(), AgentFailure> {
    if persona.revision == 0
        || persona.source.trim().is_empty()
        || persona.source.len() > 128
        || persona.instructions.trim().is_empty()
        || persona.instructions.len() > 4096
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}
