//! The Schedule Expert's own registration: the card it publishes and the
//! definition revision a delegating Run must have selected.

use floe_agent_contract::{AgentCard, AgentDefinition};

use crate::BuiltinExpertKind;

pub const SCHEDULE_DEFINITION_REVISION: u64 = 1;

pub fn schedule_definition() -> AgentDefinition {
    AgentDefinition {
        card: AgentCard {
            schema_version: floe_agent_contract::AGENT_SCHEMA_VERSION,
            protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
            id: BuiltinExpertKind::Schedule.package_id().into(),
            version: "1.0.0".into(),
            name: "Schedule Expert".into(),
            description: "Reviews the currently authorized calendar view".into(),
            domain_tags: vec!["schedule".into(), "calendar".into()],
            skills: vec!["Analyze an authorized calendar assignment".into()],
        },
        definition_revision: SCHEDULE_DEFINITION_REVISION,
    }
}
