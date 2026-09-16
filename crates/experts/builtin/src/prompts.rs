//! Role prompts for the builtin Experts. Each role's text is owned here.

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::prompts::{
    BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION, CAPABILITY_PROTOCOL, CAPABILITY_PROTOCOL_REVISION,
    PromptAssembly, PromptComponentKind, PromptRole, expert_prompt, product_component,
};

const SCHEDULE_EXPERT_ROLE: &str = include_str!("../prompts/schedule_expert_role.txt");
const COMMITMENTS_EXPERT_ROLE: &str = include_str!("../prompts/commitments_expert_role.txt");
const COMMUNICATION_EXPERT_ROLE: &str = include_str!("../prompts/communication_expert_role.txt");
const RELATIONSHIPS_EXPERT_ROLE: &str = include_str!("../prompts/relationships_expert_role.txt");
const FOCUS_EXPERT_ROLE: &str = include_str!("../prompts/focus_expert_role.txt");
const WELLBEING_EXPERT_ROLE: &str = include_str!("../prompts/wellbeing_expert_role.txt");
const WORK_CONTEXT_EXPERT_ROLE: &str = include_str!("../prompts/work_context_expert_role.txt");
const LIFE_LOGISTICS_EXPERT_ROLE: &str =
    include_str!("../prompts/life_logistics_expert_role.txt");
const SCHEDULE_EXPERT_ROLE_REVISION: u64 = 4;
const COMMITMENTS_EXPERT_ROLE_REVISION: u64 = 1;
const COMMUNICATION_EXPERT_ROLE_REVISION: u64 = 1;

pub fn schedule_expert_prompt() -> PromptAssembly {
    PromptAssembly {
        schema_version: AGENT_VERSION,
        role: PromptRole::ScheduleExpert,
        components: vec![
            product_component(
                PromptComponentKind::BehaviorKernel,
                "behavior-kernel",
                BEHAVIOR_KERNEL_REVISION,
                BEHAVIOR_KERNEL,
            ),
            product_component(
                PromptComponentKind::Role,
                "schedule-expert-role",
                SCHEDULE_EXPERT_ROLE_REVISION,
                SCHEDULE_EXPERT_ROLE,
            ),
            product_component(
                PromptComponentKind::CapabilityProtocol,
                "capability-protocol",
                CAPABILITY_PROTOCOL_REVISION,
                CAPABILITY_PROTOCOL,
            ),
        ],
    }
}

pub fn commitments_expert_prompt() -> PromptAssembly {
    expert_prompt(
        PromptRole::CommitmentsExpert,
        "commitments-expert-role",
        COMMITMENTS_EXPERT_ROLE_REVISION,
        COMMITMENTS_EXPERT_ROLE,
    )
}

pub fn communication_expert_prompt() -> PromptAssembly {
    expert_prompt(
        PromptRole::CommunicationExpert,
        "communication-expert-role",
        COMMUNICATION_EXPERT_ROLE_REVISION,
        COMMUNICATION_EXPERT_ROLE,
    )
}

pub fn relationships_expert_prompt() -> PromptAssembly {
    expert_prompt(
        PromptRole::RelationshipsExpert,
        "relationships-expert-role",
        1,
        RELATIONSHIPS_EXPERT_ROLE,
    )
}

pub fn focus_expert_prompt() -> PromptAssembly {
    expert_prompt(
        PromptRole::FocusAttentionExpert,
        "focus-expert-role",
        1,
        FOCUS_EXPERT_ROLE,
    )
}

pub fn wellbeing_expert_prompt() -> PromptAssembly {
    expert_prompt(
        PromptRole::WellbeingExpert,
        "wellbeing-expert-role",
        1,
        WELLBEING_EXPERT_ROLE,
    )
}

pub fn work_context_expert_prompt() -> PromptAssembly {
    expert_prompt(
        PromptRole::WorkContextExpert,
        "work-context-expert-role",
        1,
        WORK_CONTEXT_EXPERT_ROLE,
    )
}

pub fn life_logistics_expert_prompt() -> PromptAssembly {
    expert_prompt(
        PromptRole::LifeLogisticsExpert,
        "life-logistics-expert-role",
        1,
        LIFE_LOGISTICS_EXPERT_ROLE,
    )
}
