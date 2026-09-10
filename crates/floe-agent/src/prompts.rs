use serde::{Deserialize, Serialize};

use crate::{AGENT_VERSION, AgentFailure};

const BEHAVIOR_KERNEL: &str = include_str!("../prompts/behavior_kernel.txt");
const CAPABILITY_PROTOCOL: &str = include_str!("../prompts/capability_protocol.txt");
const MANAGER_ROLE: &str = include_str!("../prompts/manager_role.txt");
const SCHEDULE_EXPERT_ROLE: &str = include_str!("../prompts/schedule_expert_role.txt");
const COMMITMENTS_EXPERT_ROLE: &str = include_str!("../prompts/commitments_expert_role.txt");
const COMMUNICATION_EXPERT_ROLE: &str = include_str!("../prompts/communication_expert_role.txt");
const RELATIONSHIPS_EXPERT_ROLE: &str = include_str!("../prompts/relationships_expert_role.txt");
const FOCUS_EXPERT_ROLE: &str = include_str!("../prompts/focus_expert_role.txt");
const WELLBEING_EXPERT_ROLE: &str = include_str!("../prompts/wellbeing_expert_role.txt");
const WORK_CONTEXT_EXPERT_ROLE: &str = include_str!("../prompts/work_context_expert_role.txt");
const LIFE_LOGISTICS_EXPERT_ROLE: &str = include_str!("../prompts/life_logistics_expert_role.txt");
const LEARNER_ROLE: &str = include_str!("../prompts/learner_role.txt");
const LEARNER_PROTOCOL: &str = include_str!("../prompts/learner_protocol.txt");
const DEFAULT_PERSONA: &str = include_str!("../prompts/default_persona.txt");
const BEHAVIOR_KERNEL_REVISION: u64 = 2;
const CAPABILITY_PROTOCOL_REVISION: u64 = 3;
const MANAGER_ROLE_REVISION: u64 = 3;
const SCHEDULE_EXPERT_ROLE_REVISION: u64 = 3;
const COMMITMENTS_EXPERT_ROLE_REVISION: u64 = 1;
const COMMUNICATION_EXPERT_ROLE_REVISION: u64 = 1;

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

fn expert_prompt(role: PromptRole, source: &str, revision: u64, content: &str) -> PromptAssembly {
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
            product_component(PromptComponentKind::Role, "learner-role", 1, LEARNER_ROLE),
            product_component(
                PromptComponentKind::CapabilityProtocol,
                "learner-protocol",
                1,
                LEARNER_PROTOCOL,
            ),
        ],
    }
}

fn product_component(
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

pub fn calendar_briefing_prompt(focus_minutes: u16) -> String {
    render_focus_minutes(
        include_str!("../prompts/calendar_briefing.txt"),
        focus_minutes,
    )
}

pub fn calendar_focus_proposal_prompt(focus_minutes: u16) -> String {
    render_focus_minutes(
        include_str!("../prompts/calendar_focus_proposal.txt"),
        focus_minutes,
    )
}

pub fn fixture_today_prompt() -> &'static str {
    include_str!("../prompts/fixture_today.txt").trim()
}

pub fn fixture_follow_up_prompt() -> &'static str {
    include_str!("../prompts/fixture_follow_up.txt").trim()
}

pub fn fixture_repeated_call_prompt() -> &'static str {
    include_str!("../prompts/fixture_repeated_call.txt").trim()
}

pub fn fixture_unavailable_prompt() -> &'static str {
    include_str!("../prompts/fixture_unavailable.txt").trim()
}

fn render_focus_minutes(template: &str, focus_minutes: u16) -> String {
    template
        .trim()
        .replace("{{focus_minutes}}", &focus_minutes.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_calendar_templates_render_without_placeholders() {
        assert_eq!(
            calendar_briefing_prompt(60),
            "Brief today's calendar and find a 60-minute focus window."
        );
        assert_eq!(
            calendar_focus_proposal_prompt(45),
            "Propose a 45-minute focus block from today's calendar."
        );
    }

    #[test]
    fn prompt_assembly_separates_role_persona_and_protocol() {
        let manager = manager_prompt(None).unwrap();
        assert_eq!(manager.role, PromptRole::Manager);
        assert_eq!(manager.components.len(), 4);
        assert_eq!(
            manager.components[0].kind,
            PromptComponentKind::BehaviorKernel
        );
        assert_eq!(manager.components[1].kind, PromptComponentKind::Role);
        assert_eq!(manager.components[0].revision, BEHAVIOR_KERNEL_REVISION);
        assert_eq!(manager.components[1].revision, MANAGER_ROLE_REVISION);
        assert_eq!(manager.components[2].kind, PromptComponentKind::Persona);
        assert_eq!(
            manager.components[3].kind,
            PromptComponentKind::CapabilityProtocol
        );
        assert_eq!(manager.components[3].revision, CAPABILITY_PROTOCOL_REVISION);
        assert!(!manager.render().contains("focus window"));
        assert!(manager.render().contains("subject, scope, and time"));
        assert!(
            manager
                .render()
                .contains("user-visible work currently underway")
        );
        assert!(manager.render().contains("For results"));
        assert!(
            manager
                .render()
                .contains("Do not explain how the request was understood")
        );

        let expert = schedule_expert_prompt();
        assert_eq!(expert.role, PromptRole::ScheduleExpert);
        assert_eq!(expert.components.len(), 3);
        assert!(
            !expert
                .components
                .iter()
                .any(|component| component.kind == PromptComponentKind::Persona)
        );
        assert!(!expert.render().contains("find_free_windows"));
        assert_eq!(expert.components[0].revision, BEHAVIOR_KERNEL_REVISION);
        assert_eq!(expert.components[1].revision, SCHEDULE_EXPERT_ROLE_REVISION);
        assert!(expert.render().contains("Manager-ready summary"));
        assert!(expert.render().contains("add no useful meaning"));

        let commitments = commitments_expert_prompt();
        assert_eq!(commitments.role, PromptRole::CommitmentsExpert);
        assert_eq!(commitments.validate(), Ok(()));
        assert!(commitments.render().contains("observed commitment"));
        assert!(commitments.render().contains("Do not draft, send, archive"));

        let communication = communication_expert_prompt();
        assert_eq!(communication.role, PromptRole::CommunicationExpert);
        assert_eq!(communication.validate(), Ok(()));
        assert!(
            communication
                .render()
                .contains("proposal, never permission")
        );

        for prompt in [
            relationships_expert_prompt(),
            focus_expert_prompt(),
            wellbeing_expert_prompt(),
        ] {
            assert_eq!(prompt.validate(), Ok(()));
            assert_eq!(prompt.components.len(), 3);
            assert!(prompt.render().contains("untrusted evidence"));
        }

        let learner = learner_prompt();
        assert_eq!(learner.role, PromptRole::Learner);
        assert_eq!(learner.validate(), Ok(()));
        assert!(
            !learner
                .components
                .iter()
                .any(|component| component.kind == PromptComponentKind::Persona)
        );
        assert!(learner.render().contains("Return exactly one JSON object"));
        assert!(learner.render().contains("No capabilities"));
    }

    #[test]
    fn invalid_persona_is_rejected() {
        let persona = PersonaProfile {
            revision: 0,
            source: "user".into(),
            instructions: "friendly".into(),
        };
        assert_eq!(
            manager_prompt(Some(&persona)),
            Err(AgentFailure::InvalidInput)
        );
    }
}
