//! The Manager role prompt. Conversation owns the root Run's instructions.

use floe_agent_contract::{AgentFailure, RoleSpec};
use floe_knowledge::prompts::{
    AGENT_VERSION, BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION, CAPABILITY_PROTOCOL,
    CAPABILITY_PROTOCOL_REVISION, PersonaProfile, PromptAssembly, PromptComponent,
    PromptComponentKind, PromptRole, product_component, validate_persona,
};

use crate::MANAGER_OUTPUT_CONTRACT;

const MANAGER_ROLE: &str = include_str!("../prompts/manager_role.txt");
const MANAGER_ROLE_REVISION: u64 = 8;

/// The Manager role as role-only instructions: no behavior kernel, persona, or
/// capability protocol rendering. Canonical callers build their RoleSpec from
/// this instead of stuffing a rendered prompt into the role.
pub fn manager_role_spec() -> RoleSpec {
    RoleSpec {
        role_id: "manager".into(),
        instructions: MANAGER_ROLE.into(),
        output_contract: MANAGER_OUTPUT_CONTRACT.into(),
    }
}

pub fn manager_prompt(persona: Option<&PersonaProfile>) -> Result<PromptAssembly, AgentFailure> {
    let persona = persona.cloned().unwrap_or_default();
    validate_persona(&persona)?;
    let prompt = PromptAssembly {
        schema_version: AGENT_VERSION,
        role: PromptRole::Manager,
        components: vec![
            product_component(
                PromptComponentKind::BehaviorKernel,
                "behavior-kernel",
                BEHAVIOR_KERNEL_REVISION,
                BEHAVIOR_KERNEL,
            ),
            product_component(
                PromptComponentKind::Role,
                "manager-role",
                MANAGER_ROLE_REVISION,
                MANAGER_ROLE,
            ),
            PromptComponent {
                kind: PromptComponentKind::Persona,
                source: persona.source,
                revision: persona.revision,
                content: persona.instructions,
            },
            product_component(
                PromptComponentKind::CapabilityProtocol,
                "capability-protocol",
                CAPABILITY_PROTOCOL_REVISION,
                CAPABILITY_PROTOCOL,
            ),
        ],
    };
    prompt.validate()?;
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_role_spec_carries_role_only_instructions() {
        let spec = manager_role_spec();
        spec.validate().unwrap();
        assert_eq!(spec.role_id, "manager");
        assert_eq!(spec.instructions, MANAGER_ROLE);
        assert_eq!(spec.output_contract, MANAGER_OUTPUT_CONTRACT);
    }

    #[test]
    fn manager_prompt_preserves_component_identity_and_persona() {
        for persona in [
            PersonaProfile::default(),
            PersonaProfile {
                source: "example.eval.persona".into(),
                revision: 2,
                instructions: "Use a calm, concise tone.".into(),
            },
        ] {
            let prompt = manager_prompt(Some(&persona)).unwrap();
            assert_eq!(prompt.role, PromptRole::Manager);
            assert_eq!(
                prompt.components,
                vec![
                    product_component(
                        PromptComponentKind::BehaviorKernel,
                        "behavior-kernel",
                        BEHAVIOR_KERNEL_REVISION,
                        BEHAVIOR_KERNEL,
                    ),
                    product_component(PromptComponentKind::Role, "manager-role", 8, MANAGER_ROLE),
                    PromptComponent {
                        kind: PromptComponentKind::Persona,
                        source: persona.source,
                        revision: persona.revision,
                        content: persona.instructions,
                    },
                    product_component(
                        PromptComponentKind::CapabilityProtocol,
                        "capability-protocol",
                        CAPABILITY_PROTOCOL_REVISION,
                        CAPABILITY_PROTOCOL,
                    ),
                ]
            );
            assert_eq!(
                prompt.render(),
                prompt
                    .components
                    .iter()
                    .map(|component| component.content.trim())
                    .collect::<Vec<_>>()
                    .join("\n\n")
            );
        }
        assert_eq!(
            manager_prompt(None).unwrap(),
            manager_prompt(Some(&PersonaProfile::default())).unwrap()
        );
    }

    #[test]
    fn manager_maximum_persona_fits_stable_instruction_budget() {
        let persona = PersonaProfile {
            instructions: "p".repeat(4096),
            ..Default::default()
        };
        let prompt = manager_prompt(Some(&persona)).unwrap();
        assert!(
            prompt.render().len() <= floe_agent_contract::prompts::MAX_STABLE_INSTRUCTIONS_BYTES
        );
        assert_eq!(
            prompt
                .components
                .iter()
                .find(|component| component.kind == PromptComponentKind::Persona)
                .unwrap()
                .content,
            persona.instructions
        );
    }
}
