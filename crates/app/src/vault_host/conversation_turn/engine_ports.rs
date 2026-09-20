use floe_agent_contract::{AgentDefinition, AgentFailure, Artifact};

pub(super) struct ManagerPayloadValidator;

impl floe_conversation::FinalPayloadValidator for ManagerPayloadValidator {
    fn validate(&self, role: &str, text: &str, artifacts: &[Artifact]) -> Result<(), AgentFailure> {
        if (role != "manager" && role != floe_conversation::FINALIZATION_ROLE_ID)
            || text.trim().is_empty()
            || text.len() > floe_agent_contract::MAX_OUTPUT_BYTES
            || artifacts.iter().any(|artifact| {
                artifact
                    .validate(floe_agent_contract::MAX_OUTPUT_BYTES)
                    .is_err()
            })
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

const DEFINITION_REVISION: u64 = 1;

pub(crate) fn contract_definition(card: &floe_experts::AgentCard) -> AgentDefinition {
    AgentDefinition {
        card: floe_agent_contract::AgentCard {
            schema_version: floe_agent_contract::AGENT_SCHEMA_VERSION,
            protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
            id: card.id.clone(),
            version: card.version.clone(),
            name: card.name.clone(),
            description: card.description.clone(),
            supported_placements: card.supported_placements.clone(),
            domain_tags: card.domain_tags.clone(),
            skills: card.skills.clone(),
        },
        definition_revision: DEFINITION_REVISION,
    }
}
