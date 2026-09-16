//! The Manager role prompt. Conversation owns the root Run's instructions.

use floe_agent_contract::AgentFailure;
use floe_knowledge::prompts::{
    AGENT_VERSION, BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION, CAPABILITY_PROTOCOL,
    CAPABILITY_PROTOCOL_REVISION, PersonaProfile, PromptAssembly, PromptComponent,
    PromptComponentKind, PromptRole, product_component, validate_persona,
};

const MANAGER_ROLE: &str = include_str!("../prompts/manager_role.txt");
const MANAGER_ROLE_REVISION: u64 = 4;

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

pub fn fixture_today_prompt() -> &'static str {
    include_str!("../prompts/fixture_today.txt")
}

pub fn fixture_follow_up_prompt() -> &'static str {
    include_str!("../prompts/fixture_follow_up.txt")
}

pub fn fixture_repeated_call_prompt() -> &'static str {
    include_str!("../prompts/fixture_repeated_call.txt")
}

pub fn fixture_unavailable_prompt() -> &'static str {
    include_str!("../prompts/fixture_unavailable.txt")
}
