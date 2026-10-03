use floe_agent_contract::{AgentFailure, Artifact};

pub(super) struct NoManagerTools;

impl floe_agent_contract::ToolPort for NoManagerTools {
    fn invoke<'a>(
        &'a self,
        _: floe_agent_contract::ToolCall,
        _: &'a floe_execution::ExecutionScope,
    ) -> floe_agent_contract::BoxFuture<'a, Result<floe_agent_contract::ToolResult, AgentFailure>>
    {
        Box::pin(async { Err(AgentFailure::CapabilityDenied) })
    }
}

pub(super) struct ManagerPayloadValidator;

impl crate::FinalPayloadValidator for ManagerPayloadValidator {
    fn validate(&self, role: &str, text: &str, artifacts: &[Artifact]) -> Result<(), AgentFailure> {
        if (role != "manager" && role != crate::FINALIZATION_ROLE_ID)
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
