use floe_agent_contract::{AgentFailure, Artifact};

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
