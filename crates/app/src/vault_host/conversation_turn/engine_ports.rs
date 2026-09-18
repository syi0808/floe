use floe_agent_contract::{
    AgentDefinition, AllowedCatalog, Artifact, AuthorizedModelProjection, BoxFuture,
    ContextEnvelope, ContextualData, DelegationPort, DelegationRequest, DependencyCoverage,
    ModelConversation, ModelConversationEntry, ModelPort, ModelProjectionPort,
    ModelProjectionRequest, ModelRequest, ModelResponse, ModelStep, ProjectionRef, RuntimeContext,
    ScopedInstructions, TaskReceipt, ToolCall, ToolDescriptor, ToolPort, ToolResult,
};
use floe_conversation::GovernedSessionStore;
use floe_conversation::turn::UsageLedger;
use floe_conversation::{
    AgentMessage as LegacyMessage, CapabilityDescriptor, CapabilityHost, CapabilityInvocation,
    ModelRequest as LegacyModelRequest, ModelResponse as LegacyModelResponse,
    ModelRunner as LegacyModelRunner, ModelStep as LegacyModelStep,
};
use floe_execution::ExecutionScope;
use floe_kernel::PersonId;
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use uuid::Uuid;

use floe_agent_contract::AgentFailure;
use floe_context::{AgentContext, InferencePolicyDecision};
use floe_experts_builtin::BuiltinExpertKind;

use super::GovernedModel;

const DEFINITION_REVISION: u64 = 1;
const MAX_ATTEMPT_TOKENS: u64 = 4_096;
const MAX_ATTEMPT_COST_MICROS: u64 = 1_000_000;

/// Transitional model projection: assembles the authorized envelope from the
/// typed conversation while the legacy model/tool bridges still own execution.
/// History authorization still runs exactly once, inside GovernedModel behind
/// LegacyModelPort; this adapter never filters history and never calls the
/// model. Deleted with the legacy bridges at B9.
pub(super) struct TransitionalModelProjection<'a, Keys> {
    pub store: &'a GovernedSessionStore<'a, EncryptedAgentVault<Keys>>,
    pub policy: &'a InferencePolicyDecision,
    pub context: &'a AgentContext,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub active_agents: Vec<floe_experts::AgentCard>,
}

impl<Keys> ModelProjectionPort for TransitionalModelProjection<'_, Keys>
where
    Keys: VaultKeyProvider,
{
    fn project<'a>(
        &'a self,
        request: ModelProjectionRequest,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<AuthorizedModelProjection, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            let finalization = match request.role.role_id.as_str() {
                "manager" => false,
                role if role == floe_conversation::FINALIZATION_ROLE_ID => true,
                _ => return Err(AgentFailure::InvalidInput),
            };
            let mut prompt =
                floe_conversation::prompts::manager_prompt(self.context.persona.as_ref())?;
            if finalization {
                let role = prompt
                    .components
                    .iter_mut()
                    .find(|component| {
                        component.kind == floe_knowledge::prompts::PromptComponentKind::Role
                    })
                    .ok_or(AgentFailure::InvalidInput)?;
                role.content = format!(
                    "{}\n{}",
                    floe_conversation::FINALIZATION_ROLE_PROMPT,
                    floe_conversation::FINALIZATION_OUTPUT_CONTRACT
                );
            }
            prompt.validate()?;
            let empty_context = AgentContext {
                projection_version: self.context.projection_version,
                persona: None,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            };
            let context = if finalization {
                &empty_context
            } else {
                self.context
            };
            let capabilities = filter_capabilities(&self.capabilities, &request.catalog);
            let active_agents = filter_agents(&self.active_agents, &request.catalog);
            let envelope = ContextEnvelope {
                schema_version: floe_agent_contract::AGENT_SCHEMA_VERSION,
                stable_instructions: prompt.clone(),
                scoped_instructions: ScopedInstructions {
                    purpose: self.policy.purpose.clone(),
                    response_contract: request.role.output_contract.clone(),
                    available_capabilities: capabilities,
                    active_experts: active_agents.clone(),
                    correction: request.correction.clone(),
                },
                contextual_data: ContextualData {
                    projection_version: context.projection_version,
                    memories: context.memories.clone(),
                    optional_context_issues: context.optional_context_issues.clone(),
                    evidence: context.evidence.clone(),
                },
                conversation: request.conversation,
                runtime: RuntimeContext {
                    max_output_bytes: request.max_output_bytes.min(16384),
                },
                manifest: floe_conversation::turn::context_manifest(
                    &prompt,
                    context,
                    &active_agents,
                ),
            };
            // Quoted history stands on its committed turn coverages; the live
            // exchanges carry their own. Turns GovernedModel later filters out
            // only overstate this input coverage, never understate it.
            let mut coverage = DependencyCoverage::Independent;
            for entry in &envelope.conversation.history {
                let message_id = match entry {
                    ModelConversationEntry::User { message_id, .. }
                    | ModelConversationEntry::Preamble { message_id, .. }
                    | ModelConversationEntry::Assistant { message_id, .. } => *message_id,
                    ModelConversationEntry::ToolExchange { .. }
                    | ModelConversationEntry::DelegationExchange { .. } => {
                        return Err(AgentFailure::InvalidInput);
                    }
                };
                let turn_coverage = self.store.committed_turn_coverage(message_id).await?;
                coverage = coverage
                    .merge(&turn_coverage)
                    .map_err(|_| AgentFailure::InvalidInput)?;
            }
            for entry in &envelope.conversation.current_turn {
                let (exchange_coverage, artifacts) = match entry {
                    ModelConversationEntry::ToolExchange { result, .. } => {
                        (&result.coverage, result.artifacts.as_slice())
                    }
                    ModelConversationEntry::DelegationExchange { receipt, .. } => (
                        &receipt.snapshot.coverage,
                        receipt.snapshot.artifacts.as_slice(),
                    ),
                    ModelConversationEntry::User { .. }
                    | ModelConversationEntry::Preamble { .. }
                    | ModelConversationEntry::Assistant { .. } => continue,
                };
                coverage = coverage
                    .merge(exchange_coverage)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                for artifact in artifacts {
                    coverage = coverage
                        .merge(&artifact.coverage)
                        .map_err(|_| AgentFailure::InvalidInput)?;
                }
            }
            let projection = AuthorizedModelProjection {
                projection_ref: ProjectionRef::new(),
                projection_revision: 1,
                envelope,
                coverage,
                input_data_classes: self.policy.data_classes.clone(),
            };
            projection.validate()?;
            Ok(projection)
        })
    }
}

fn filter_capabilities(
    capabilities: &[CapabilityDescriptor],
    catalog: &AllowedCatalog,
) -> Vec<CapabilityDescriptor> {
    capabilities
        .iter()
        .filter(|capability| {
            catalog
                .tools
                .iter()
                .any(|tool| tool.id == capability.id)
        })
        .cloned()
        .collect()
}

fn filter_agents(
    agents: &[floe_experts::AgentCard],
    catalog: &AllowedCatalog,
) -> Vec<floe_experts::AgentCard> {
    agents
        .iter()
        .filter(|card| {
            catalog
                .cards
                .iter()
                .any(|definition| definition.card.id == card.id)
        })
        .cloned()
        .collect()
}

pub(super) struct LegacyModelPort<'a, Keys, Runner: LegacyModelRunner> {
    pub model: &'a Runner,
    pub store: &'a GovernedSessionStore<'a, EncryptedAgentVault<Keys>>,
    pub resolver: &'a dyn floe_access::DependencyResolver,
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
            request.validate()?;
            // Purpose, consumer, and preferred profile ride along for the
            // canonical path; this legacy bridge keeps serving the route the
            // App already resolved. Deleted at B9.
            let envelope = &request.projection.envelope;
            let finalization = envelope.scoped_instructions.response_contract
                == floe_conversation::FINALIZATION_OUTPUT_CONTRACT;
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
            let legacy_request = LegacyModelRequest {
                usage,
                replay: vec![],
                schema_version: floe_kernel::AGENT_VERSION,
                prompt: envelope.stable_instructions.clone(),
                person_id: self.person_id,
                session_id: self.session_id,
                turn_id: run_id,
                policy: self.policy.clone(),
                context,
                messages: legacy_conversation_messages(&envelope.conversation, run_id)?,
                capabilities: filter_capabilities(&self.capabilities, &request.catalog),
                active_agents: filter_agents(&self.active_agents, &request.catalog),
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
            let response =
                floe_conversation::turn::generate_with_recovery(&governed, legacy_request).await?;
            contract_response(request.attempt_id, response, &request.catalog)
        })
    }
}

pub(super) struct LegacyToolPort<'a, Keys, Host: CapabilityHost> {
    pub host: &'a Host,
    pub store: &'a GovernedSessionStore<'a, EncryptedAgentVault<Keys>>,
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
            supported_placements: card.supported_placements.clone(),
            domain_tags: card.domain_tags.clone(),
            skills: card.skills.clone(),
        },
        definition_revision: DEFINITION_REVISION,
    }
}

/// Typed conversation back to legacy messages for the bridge's inner legacy
/// call. History keeps its turn association; the live turn groups under the
/// running run. Tool exchanges keep their exact call id, tool id, and input;
/// delegation exchanges stay lossy assistant text exactly as before.
fn legacy_conversation_messages(
    conversation: &ModelConversation,
    current_turn_id: Uuid,
) -> Result<Vec<LegacyMessage>, AgentFailure> {
    let mut messages = Vec::with_capacity(
        conversation
            .history
            .len()
            .saturating_add(conversation.current_turn.len()),
    );
    for entry in &conversation.history {
        match entry {
            ModelConversationEntry::User { message_id, text } => {
                messages.push(LegacyMessage::User {
                    turn_id: *message_id,
                    text: text.clone(),
                });
            }
            ModelConversationEntry::Preamble { message_id, text } => {
                messages.push(LegacyMessage::Preamble {
                    turn_id: *message_id,
                    text: text.clone(),
                });
            }
            ModelConversationEntry::Assistant { message_id, text } => {
                messages.push(LegacyMessage::Assistant {
                    turn_id: *message_id,
                    text: text.clone(),
                });
            }
            ModelConversationEntry::ToolExchange { .. }
            | ModelConversationEntry::DelegationExchange { .. } => {
                return Err(AgentFailure::InvalidInput);
            }
        }
    }
    for entry in &conversation.current_turn {
        match entry {
            ModelConversationEntry::User { text, .. } => {
                messages.push(LegacyMessage::User {
                    turn_id: current_turn_id,
                    text: text.clone(),
                });
            }
            ModelConversationEntry::Preamble { text, .. } => {
                messages.push(LegacyMessage::Preamble {
                    turn_id: current_turn_id,
                    text: text.clone(),
                });
            }
            ModelConversationEntry::Assistant { text, .. } => {
                messages.push(LegacyMessage::Assistant {
                    turn_id: current_turn_id,
                    text: text.clone(),
                });
            }
            ModelConversationEntry::ToolExchange { call, result } => {
                messages.push(LegacyMessage::Capability {
                    turn_id: current_turn_id,
                    call_id: call.call_id,
                    capability_id: call.tool_id.clone(),
                    input: call.input.clone(),
                    result: match &result.issue {
                        None => Ok(result.text.clone()),
                        Some(issue) => Err(issue.failure),
                    },
                });
            }
            ModelConversationEntry::DelegationExchange { receipt, .. } => {
                messages.push(LegacyMessage::Assistant {
                    turn_id: current_turn_id,
                    text: receipt
                        .snapshot
                        .result
                        .clone()
                        .unwrap_or_else(|| format!("task {:?}", receipt.snapshot.state)),
                });
            }
        }
    }
    Ok(messages)
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
            LegacyModelStep::Preamble { text } => {
                Ok::<_, AgentFailure>(ModelStep::Preamble { text })
            }
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
    use floe_agent_contract::{InvocationKey, ToolCall, ToolResult};

    #[test]
    fn conversion_preserves_current_turn_and_catalog_revisions() {
        let run_id = Uuid::new_v4();
        let old_id = Uuid::new_v4();
        let call_id = Uuid::new_v4();
        let conversation = ModelConversation {
            history: vec![ModelConversationEntry::Assistant {
                message_id: old_id,
                text: "old".into(),
            }],
            current_turn: vec![
                ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "current".into(),
                },
                ModelConversationEntry::ToolExchange {
                    call: ToolCall {
                        call_id,
                        invocation_key: InvocationKey::new(),
                        tool_id: "lookup".into(),
                        definition_revision: 7,
                        input: r#"{"day":"today"}"#.into(),
                    },
                    result: ToolResult {
                        call_id,
                        text: "result".into(),
                        artifacts: vec![],
                        coverage: DependencyCoverage::Independent,
                        issue: None,
                    },
                },
            ],
        };
        let catalog = AllowedCatalog {
            cards: vec![],
            tools: vec![ToolDescriptor {
                id: "lookup".into(),
                definition_revision: 7,
                description: "lookup".into(),
                input_schema: "{\"type\":\"object\"}".into(),
                output_data_class: "personal".into(),
            }],
            revision: 1,
        };
        let messages = legacy_conversation_messages(&conversation, run_id).unwrap();
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
            LegacyMessage::Capability {
                turn_id,
                capability_id,
                input,
                ..
            } if *turn_id == run_id && capability_id == "lookup" && input.contains("today")
        ));

        let converted = contract_response(
            Uuid::new_v4(),
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
            &catalog,
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
