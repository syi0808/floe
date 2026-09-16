use crate::{AgentMessage as LegacyMessage, CapabilityDescriptor, CapabilityHost, CapabilityInvocation, ModelRequest as LegacyModelRequest, ModelResponse as LegacyModelResponse, ModelRunner as LegacyModelRunner, ModelStep as LegacyModelStep};
use crate::turn::UsageLedger;
use floe_agent_contract::{
    AgentDefinition, AllowedCatalog, Artifact, BoxFuture, DelegationPort, DelegationRequest,
    DependencyCoverage, MessageRole, ModelPort, ModelRequest, ModelResponse, ModelStep,
    TaskReceipt, ToolCall, ToolDescriptor, ToolPort, ToolResult,
};
use floe_vault::{GovernedAgentSessionStore, VaultKeyProvider};
use floe_kernel::PersonId;
use floe_execution::ExecutionScope;
use uuid::Uuid;

use super::{
    AgentContext, AgentFailure, BuiltinExpertKind, GovernedModel, InferencePolicyDecision,
};

const DEFINITION_REVISION: u64 = 1;
const MAX_ATTEMPT_TOKENS: u64 = 4_096;
const MAX_ATTEMPT_COST_MICROS: u64 = 1_000_000;

pub(super) struct LegacyModelPort<'a, Keys, Runner: LegacyModelRunner> {
    pub model: &'a Runner,
    pub store: &'a GovernedAgentSessionStore<'a, Keys>,
    pub resolver: &'a dyn floe_vault::GovernedDependencyResolver,
    pub policy: &'a InferencePolicyDecision,
    pub context: &'a AgentContext,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub active_agents: Vec<floe_experts::AgentCard>,
    pub max_output_bytes: usize,
}

impl<Keys, Runner> ModelPort for LegacyModelPort<'_, Keys, Runner>
where
    Keys: VaultKeyProvider,
    Runner: LegacyModelRunner + Sync,
{
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        Box::pin(async move {
            if request.role.role_id != "manager" || request.attempt_id.is_nil() {
                return Err(AgentFailure::InvalidInput);
            }
            let finalization = request.role.prompt == crate::FINALIZATION_ROLE_PROMPT
                && request.role.output_contract == crate::FINALIZATION_OUTPUT_CONTRACT;
            let run_id = scope
                .root_run_id()
                .ok_or(AgentFailure::InvalidInput)?
                .as_uuid();
            let remaining_tokens = scope.budget().max_tokens().min(MAX_ATTEMPT_TOKENS);
            let remaining_cost_micros = scope
                .budget()
                .max_cost_micros()
                .min(MAX_ATTEMPT_COST_MICROS);
            let usage =
                UsageLedger::new(remaining_tokens, remaining_cost_micros, Default::default());
            let mut prompt = crate::manager_prompt(self.context.persona.as_ref())?;
            if finalization {
                let role = prompt
                    .components
                    .iter_mut()
                    .find(|component| component.kind == floe_knowledge::PromptComponentKind::Role)
                    .ok_or(AgentFailure::InvalidInput)?;
                role.content = format!("{}\n{}", request.role.prompt, request.role.output_contract);
            }
            let context = if finalization {
                AgentContext {
                    projection_version: self.context.projection_version,
                    persona: None,
                    memories: vec![],
                    optional_context_issues: vec![],
                    evidence: vec![],
                }
            } else {
                self.context.clone()
            };
            let capabilities = self
                .capabilities
                .iter()
                .filter(|capability| {
                    request
                        .catalog
                        .tools
                        .iter()
                        .any(|tool| tool.id == capability.id)
                })
                .cloned()
                .collect();
            let active_agents = self
                .active_agents
                .iter()
                .filter(|card| {
                    request
                        .catalog
                        .cards
                        .iter()
                        .any(|definition| definition.card.id == card.id)
                })
                .cloned()
                .collect();
            let legacy_request = LegacyModelRequest {
                usage,
                replay: vec![],
                schema_version: floe_kernel::AGENT_VERSION,
                prompt,
                person_id: self.person_id,
                session_id: self.session_id,
                turn_id: run_id,
                policy: self.policy.clone(),
                context,
                messages: legacy_messages(&request, run_id)?,
                capabilities,
                active_agents,
                remaining_tokens,
                remaining_cost_micros,
                max_output_bytes: if finalization {
                    self.max_output_bytes.min(4_096)
                } else {
                    self.max_output_bytes
                },
                deadline: scope.deadline(),
                cancellation: scope.cancellation().clone(),
            };
            let governed = GovernedModel {
                model: self.model,
                store: self.store,
                resolver: self.resolver,
            };
            let response = crate::turn::generate_with_recovery(&governed, legacy_request).await?;
            contract_response(request.attempt_id, response, &request.catalog)
        })
    }
}

pub(super) struct LegacyToolPort<'a, Keys, Host: CapabilityHost> {
    pub host: &'a Host,
    pub store: &'a GovernedAgentSessionStore<'a, Keys>,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub max_output_bytes: usize,
}

impl<Keys, Host> ToolPort for LegacyToolPort<'_, Keys, Host>
where
    Keys: VaultKeyProvider,
    Host: CapabilityHost + Sync,
{
    fn invoke<'a>(
        &'a self,
        call: ToolCall,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ToolResult, AgentFailure>> {
        Box::pin(async move {
            if call.definition_revision != DEFINITION_REVISION {
                return Err(AgentFailure::Conflict);
            }
            if !self
                .host
                .descriptors(self.person_id)
                .iter()
                .any(|descriptor| descriptor.id == call.tool_id && descriptor.read_only)
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let turn_id = scope
                .root_run_id()
                .ok_or(AgentFailure::InvalidInput)?
                .as_uuid();
            let text = self
                .host
                .invoke(CapabilityInvocation {
                    usage: UsageLedger::default(),
                    schema_version: floe_kernel::AGENT_VERSION,
                    call_id: call.call_id,
                    person_id: self.person_id,
                    session_id: self.session_id,
                    turn_id,
                    capability_id: call.tool_id,
                    input: call.input,
                    max_output_bytes: self.max_output_bytes,
                    deadline: scope.deadline(),
                    cancellation: scope.cancellation().clone(),
                })
                .await?;
            Ok(ToolResult {
                call_id: call.call_id,
                text,
                artifacts: vec![],
                coverage: self
                    .store
                    .result_coverage(turn_id, call.call_id)?
                    .unwrap_or(DependencyCoverage::Unknown),
                issue: None,
            })
        })
    }
}

pub(super) struct LegacyDelegationPort<'a, Keys: VaultKeyProvider> {
    pub task_coordinator: &'a floe_experts::TaskCoordinator<
        crate::vault_host::task_repository::VaultTaskRepository<Keys>,
    >,
    pub schedule_endpoint: &'a super::expert_dispatch::schedule::ScheduleEndpoint<Keys>,
    pub legacy_expert_endpoint: &'a super::expert_dispatch::LegacyExpertEndpoint<Keys>,
    pub turn_request: &'a floe_protocol::AgentConversationTurnRequestDto,
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
            self.legacy_expert_endpoint.stage(
                run_id,
                super::expert_dispatch::LegacyExpertEndpointContext {
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
            self.legacy_expert_endpoint.clear(run_id)?;
            result
        })
    }
}

pub(super) struct ManagerPayloadValidator;

impl crate::FinalPayloadValidator for ManagerPayloadValidator {
    fn validate(&self, role: &str, text: &str, artifacts: &[Artifact]) -> Result<(), AgentFailure> {
        if role != "manager"
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

pub(super) fn contract_tools(descriptors: &[CapabilityDescriptor]) -> Vec<ToolDescriptor> {
    descriptors
        .iter()
        .filter(|descriptor| descriptor.read_only)
        .map(|descriptor| ToolDescriptor {
            id: descriptor.id.clone(),
            definition_revision: DEFINITION_REVISION,
            description: format!("Read-only Floe capability {}", descriptor.id),
            input_schema: descriptor
                .input_schema
                .clone()
                .unwrap_or_else(|| serde_json::json!({"type": "object"}))
                .to_string(),
            output_data_class: format!("{:?}", descriptor.output_data_class).to_lowercase(),
        })
        .collect()
}

pub(crate) fn contract_definition(card: &floe_experts::AgentCard) -> AgentDefinition {
    AgentDefinition {
        card: floe_agent_contract::AgentCard {
            schema_version: floe_agent_contract::AGENT_SCHEMA_VERSION,
            protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
            id: card.id.clone(),
            version: card.version.clone(),
            name: card.name.clone(),
            description: card.description.clone(),
            domain_tags: card.domain_tags.clone(),
            skills: card.skills.clone(),
        },
        definition_revision: DEFINITION_REVISION,
    }
}

fn legacy_messages(
    request: &ModelRequest,
    current_turn_id: Uuid,
) -> Result<Vec<LegacyMessage>, AgentFailure> {
    let current_start = request
        .messages
        .iter()
        .rposition(|message| message.role == MessageRole::User && message.text == request.prompt)
        .ok_or(AgentFailure::InvalidInput)?;
    request
        .messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let turn_id = if index >= current_start {
                current_turn_id
            } else {
                message.message_id
            };
            let legacy = match message.role {
                MessageRole::User => LegacyMessage::User {
                    turn_id,
                    text: message.text.clone(),
                },
                MessageRole::Preamble => LegacyMessage::Preamble {
                    turn_id,
                    text: message.text.clone(),
                },
                MessageRole::Assistant | MessageRole::Delegation => LegacyMessage::Assistant {
                    turn_id,
                    text: message.text.clone(),
                },
                MessageRole::Tool => {
                    let call_id = message.call_id.unwrap_or(message.message_id);
                    let capability_id = request
                        .replay
                        .iter()
                        .find(|receipt| receipt.call_id == call_id)
                        .and_then(|receipt| receipt.tool_id.clone())
                        .unwrap_or_else(|| "floe.observation".into());
                    LegacyMessage::Capability {
                        turn_id,
                        call_id,
                        capability_id,
                        input: "{}".into(),
                        result: Ok(message.text.clone()),
                    }
                }
            };
            Ok(legacy)
        })
        .collect()
}

fn contract_response(
    attempt_id: Uuid,
    response: LegacyModelResponse,
    catalog: &AllowedCatalog,
) -> Result<ModelResponse, AgentFailure> {
    let steps = response
        .output
        .into_iter()
        .map(|step| match step {
            LegacyModelStep::Preamble { text } => Ok(ModelStep::Preamble { text }),
            LegacyModelStep::Answer { text } => Ok(ModelStep::Answer {
                text,
                artifacts: vec![],
            }),
            LegacyModelStep::Call {
                capability_id,
                input,
            } => {
                let revision = catalog
                    .tools
                    .iter()
                    .find(|descriptor| descriptor.id == capability_id)
                    .map(|descriptor| descriptor.definition_revision)
                    .ok_or(AgentFailure::CapabilityDenied)?;
                Ok(ModelStep::CallTool {
                    tool_id: capability_id,
                    definition_revision: revision,
                    input,
                })
            }
            LegacyModelStep::Delegate { agent_id, message } => {
                let revision = catalog
                    .cards
                    .iter()
                    .find(|definition| definition.card.id == agent_id)
                    .map(|definition| definition.definition_revision)
                    .ok_or(AgentFailure::CapabilityDenied)?;
                Ok(ModelStep::Delegate {
                    agent_id,
                    definition_revision: revision,
                    message,
                    context_refs: vec![],
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ModelResponse {
        attempt_id,
        steps,
        usage: floe_agent_contract::ModelUsage {
            tokens: response.used_tokens,
            cost_micros: response.cost_micros,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use floe_agent_contract::AgentMessage;

    #[test]
    fn conversion_preserves_current_turn_and_catalog_revisions() {
        let run_id = Uuid::new_v4();
        let old_id = Uuid::new_v4();
        let call_id = Uuid::new_v4();
        let request = ModelRequest {
            attempt_id: Uuid::new_v4(),
            role: floe_agent_contract::RoleSpec {
                role_id: "manager".into(),
                prompt: "manager".into(),
                output_contract: "text".into(),
            },
            prompt: "current".into(),
            bounded_context: floe_agent_contract::BoundedContext {
                text: String::new(),
                coverage: DependencyCoverage::Independent,
            },
            messages: vec![
                AgentMessage {
                    message_id: old_id,
                    role: MessageRole::Assistant,
                    text: "old".into(),
                    call_id: None,
                    coverage: DependencyCoverage::Independent,
                },
                AgentMessage {
                    message_id: Uuid::new_v4(),
                    role: MessageRole::User,
                    text: "current".into(),
                    call_id: None,
                    coverage: DependencyCoverage::Independent,
                },
                AgentMessage {
                    message_id: Uuid::new_v4(),
                    role: MessageRole::Tool,
                    text: "result".into(),
                    call_id: Some(call_id),
                    coverage: DependencyCoverage::Independent,
                },
            ],
            catalog: AllowedCatalog {
                cards: vec![],
                tools: vec![ToolDescriptor {
                    id: "lookup".into(),
                    definition_revision: 7,
                    description: "lookup".into(),
                    input_schema: "{\"type\":\"object\"}".into(),
                    output_data_class: "personal".into(),
                }],
                revision: 1,
            },
            replay: vec![floe_agent_contract::ReplayReceipt {
                principal: "person".into(),
                run_id: None,
                task_id: None,
                agent_id: None,
                tool_id: Some("lookup".into()),
                definition_revision: 7,
                input_digest: [1; 32],
                invocation_key: floe_agent_contract::InvocationKey::new(),
                call_id,
                result: "result".into(),
                task_result: None,
                task_state: None,
                task_artifacts: vec![],
                task_coverage: DependencyCoverage::Unknown,
                task_issue: None,
                tool_artifacts: vec![],
                tool_coverage: DependencyCoverage::Independent,
                tool_issue: None,
            }],
        };
        let messages = legacy_messages(&request, run_id).unwrap();
        assert!(matches!(
            &messages[0],
            LegacyMessage::Assistant { turn_id, .. } if *turn_id == old_id
        ));
        assert!(matches!(
            &messages[1],
            LegacyMessage::User { turn_id, .. } if *turn_id == run_id
        ));
        assert!(matches!(
            &messages[2],
            LegacyMessage::Capability { turn_id, capability_id, .. }
                if *turn_id == run_id && capability_id == "lookup"
        ));

        let converted = contract_response(
            request.attempt_id,
            LegacyModelResponse {
                replay: None,
                schema_version: floe_kernel::AGENT_VERSION,
                output: vec![LegacyModelStep::Call {
                    capability_id: "lookup".into(),
                    input: "{}".into(),
                }],
                used_tokens: 5,
                cost_micros: 2,
            },
            &request.catalog,
        )
        .unwrap();
        assert!(matches!(
            converted.steps.as_slice(),
            [ModelStep::CallTool {
                tool_id,
                definition_revision: 7,
                ..
            }] if tool_id == "lookup"
        ));
        assert_eq!(converted.usage.tokens, 5);
    }

    #[test]
    fn answer_only_catalog_rejects_tool_and_delegation_steps() {
        for output in [
            LegacyModelStep::Call {
                capability_id: "lookup".into(),
                input: "{}".into(),
            },
            LegacyModelStep::Delegate {
                agent_id: "expert".into(),
                message: "finish".into(),
            },
        ] {
            assert_eq!(
                contract_response(
                    Uuid::new_v4(),
                    LegacyModelResponse {
                        replay: None,
                        schema_version: floe_kernel::AGENT_VERSION,
                        output: vec![output],
                        used_tokens: 1,
                        cost_micros: 1,
                    },
                    &AllowedCatalog {
                        cards: vec![],
                        tools: vec![],
                        revision: 1,
                    },
                ),
                Err(AgentFailure::CapabilityDenied)
            );
        }
    }
}
