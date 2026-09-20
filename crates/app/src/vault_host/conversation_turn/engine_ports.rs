use floe_agent_contract::{
    AgentDefinition, AgentFailure, Artifact, BoxFuture, DelegationPort, DelegationRequest,
    TaskReceipt,
};
use floe_context::AgentContext;
use floe_execution::ExecutionScope;
use floe_experts_builtin::BuiltinExpertKind;
use floe_vault::VaultKeyProvider;
use uuid::Uuid;

pub(super) struct LegacyDelegationPort<'a, Keys: VaultKeyProvider> {
    pub task_coordinator: &'a floe_experts::TaskCoordinator<floe_vault::VaultTaskRepository<Keys>>,
    pub schedule_endpoint: &'a super::expert_dispatch::schedule::ScheduleEndpoint<Keys>,
    pub builtin_expert_endpoint: &'a super::expert_dispatch::BuiltinExpertEndpoint<Keys>,
    pub turn_request: &'a crate::ConversationTurnRequest,
    pub context: &'a AgentContext,
    pub session_id: Uuid,
    pub max_output_bytes: usize,
}

impl<Keys: VaultKeyProvider + 'static> DelegationPort for LegacyDelegationPort<'_, Keys> {
    fn delegate<'a>(
        &'a self,
        request: DelegationRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>> {
        Box::pin(async move {
            let run_id = scope
                .root_run_id()
                .ok_or(AgentFailure::InvalidInput)?
                .as_uuid();
            if request.selected_agent_id == BuiltinExpertKind::Schedule.package_id() {
                self.schedule_endpoint.stage(
                    run_id,
                    super::expert_dispatch::schedule::ScheduleEndpointContext {
                        request: self.turn_request.clone(),
                        context: self.context.clone(),
                        max_output_bytes: self.max_output_bytes,
                    },
                )?;
                let result = floe_agent_contract::DelegationPort::delegate(
                    self.task_coordinator,
                    request,
                    scope,
                )
                .await;
                self.schedule_endpoint.clear(run_id)?;
                return result;
            }
            self.builtin_expert_endpoint.stage(
                run_id,
                super::expert_dispatch::BuiltinExpertEndpointContext {
                    request: self.turn_request.clone(),
                    context: self.context.clone(),
                    session_id: self.session_id,
                    max_output_bytes: self.max_output_bytes,
                },
            )?;
            let result = floe_agent_contract::DelegationPort::delegate(
                self.task_coordinator,
                request,
                scope,
            )
            .await;
            self.builtin_expert_endpoint.clear(run_id)?;
            result
        })
    }
}

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
