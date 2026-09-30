use floe_agent_contract::{AgentFailure, Artifact};

pub(super) fn manager_catalog(
    cards: Vec<floe_agent_contract::AgentDefinition>,
    revision: u64,
) -> floe_agent_contract::AllowedCatalog {
    floe_agent_contract::AllowedCatalog {
        cards,
        tools: vec![],
        revision: revision.max(1),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent_contract::ToolPort;

    #[test]
    fn root_catalog_preserves_expert_cards_and_revisions_without_tools() {
        let cards = floe_experts_builtin::manifests()
            .into_iter()
            .map(|manifest| floe_agent_contract::AgentDefinition {
                card: manifest.definition.card,
                definition_revision: 17,
            })
            .collect::<Vec<_>>();
        let catalog = manager_catalog(cards.clone(), 23);
        assert_eq!(catalog.cards, cards);
        assert_eq!(catalog.revision, 23);
        assert!(catalog.tools.is_empty());
    }

    #[tokio::test]
    async fn unexpected_manager_tool_invocations_fail_closed() {
        let ledger = floe_execution::budget::BudgetLedger::new(
            floe_execution::budget::BudgetConfig::new(100, 100),
            Default::default(),
        );
        let scope = floe_execution::ExecutionScope::root(
            floe_execution::Cancellation::default(),
            tokio::time::Instant::now() + std::time::Duration::from_secs(5),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(uuid::Uuid::new_v4()),
        );
        for tool_id in [
            "people.identity.read",
            "schedule.feasibility.read",
            "attention.coarse.read",
            "wellbeing.derived.read",
            "mail.communication.read",
            "work.context.read",
            "life.logistics.read",
            "unexpected.tool",
        ] {
            let call = floe_agent_contract::ToolCall {
                call_id: uuid::Uuid::new_v4(),
                invocation_key: floe_agent_contract::InvocationKey::new(),
                tool_id: tool_id.into(),
                definition_revision: 1,
                input: "{}".into(),
            };
            assert_eq!(
                NoManagerTools.invoke(call, &scope).await,
                Err(AgentFailure::CapabilityDenied)
            );
        }
    }
}

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
