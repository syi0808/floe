//! Prompt assembly: the role-neutral kernel, capability protocol, persona and
//! the structural rules every assembled prompt must satisfy.
//!
//! A role's own text belongs to that role's owner: this crate holds what every
//! role shares and the shape they all assemble into, and each owner supplies
//! its own role text.

use floe_kernel::AgentFailure;

pub use floe_kernel::AGENT_VERSION;
use serde::{Deserialize, Serialize};

pub const BEHAVIOR_KERNEL: &str = include_str!("../prompts/behavior_kernel.txt");
pub const CAPABILITY_PROTOCOL: &str = include_str!("../prompts/capability_protocol.txt");
pub const MODEL_CORRECTION: &str = include_str!("../prompts/model_correction.txt");
const DEFAULT_PERSONA: &str = include_str!("../prompts/default_persona.txt");
pub const BEHAVIOR_KERNEL_REVISION: u64 = 3;
pub const CAPABILITY_PROTOCOL_REVISION: u64 = 3;
pub const MAX_STABLE_INSTRUCTIONS_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptRole {
    Manager,
    Expert,
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
            || self.render().len() > MAX_STABLE_INSTRUCTIONS_BYTES
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
    source: &str,
    revision: u64,
    content: &str,
) -> PromptAssembly {
    PromptAssembly {
        schema_version: AGENT_VERSION,
        role: PromptRole::Expert,
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

pub fn validate_persona(persona: &PersonaProfile) -> Result<(), AgentFailure> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_instructions_use_utf8_byte_limits_with_bounded_components() {
        for bytes in [4097, 8192, 8193] {
            let first_bytes = (bytes - 6).min(4096);
            let sizes = [first_bytes, bytes - first_bytes - 5, 1];
            let mut prompt = expert_prompt("fixture-role", 1, "fixture");
            for (component, size) in prompt.components.iter_mut().zip(sizes) {
                component.content = "가".repeat(size / 3) + &"x".repeat(size % 3);
                assert!(component.content.len() <= 4096);
            }
            assert_eq!(prompt.render().len(), bytes);
            assert_eq!(
                prompt.validate(),
                if bytes <= MAX_STABLE_INSTRUCTIONS_BYTES {
                    Ok(())
                } else {
                    Err(AgentFailure::InvalidInput)
                }
            );
        }
    }

    #[test]
    fn component_and_persona_limits_remain_4096_bytes() {
        let mut prompt = expert_prompt("fixture-role", 1, &"x".repeat(4096));
        assert_eq!(prompt.validate(), Ok(()));
        prompt.components[1].content.push('x');
        assert_eq!(prompt.validate(), Err(AgentFailure::InvalidInput));
        let mut persona = PersonaProfile {
            instructions: "x".repeat(4096),
            ..PersonaProfile::default()
        };
        assert_eq!(persona.validate(), Ok(()));
        persona.instructions.push('x');
        assert_eq!(persona.validate(), Err(AgentFailure::InvalidInput));
    }
}
