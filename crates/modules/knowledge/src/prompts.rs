//! The Learner's own prompt.
//!
//! The kernel, the capability protocol, the persona and the assembly rules are
//! role-neutral and belong to the agent contract; what is Knowledge's own is the
//! Learner role it plays.

pub use floe_agent_contract::prompts::*;

pub const LEARNER_ROLE: &str = include_str!("../prompts/learner_role.txt");
pub const LEARNER_PROTOCOL: &str = include_str!("../prompts/learner_protocol.txt");
pub const LEARNER_ROLE_REVISION: u64 = 2;
pub const LEARNER_PROTOCOL_REVISION: u64 = 3;
pub const LEARNER_EXTRACTOR_VERSION: &str = "memory-extractor-v3";
pub const LEARNER_PROMPT_VERSION: &str = "memory-review-v3";

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
