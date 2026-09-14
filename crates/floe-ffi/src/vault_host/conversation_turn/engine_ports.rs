use floe_agent::{
    A2AMessage, A2AMessageRole, A2APart, A2ASendMessageRequest, A2ATask, A2ATaskState,
    AgentMessage as LegacyMessage, CapabilityDescriptor, CapabilityHost, CapabilityInvocation,
    InProcessAgent, ModelRequest as LegacyModelRequest, ModelResponse as LegacyModelResponse,
    ModelRunner as LegacyModelRunner, ModelStep as LegacyModelStep, UsageLedger,
};
use floe_agent_contract::{
    AgentDefinition, AllowedCatalog, Artifact, BoxFuture, DelegationPort, DelegationRequest,
    DependencyCoverage, MessageRole, ModelPort, ModelRequest, ModelResponse, ModelStep,
    TaskReceipt, TaskSnapshot, TaskState, ToolCall, ToolDescriptor, ToolPort, ToolResult,
};
use floe_core::{GovernedAgentSessionStore, VaultKeyProvider};
use floe_domain::PersonId;
use floe_execution::ExecutionScope;
use uuid::Uuid;

use super::{
    AgentContext, AgentFailure, BuiltinExpertKind, ConversationExperts, GovernedModel,
    InferencePolicyDecision,
};

const DEFINITION_REVISION: u64 = 1;
const MAX_ATTEMPT_TOKENS: u64 = 4_096;
const MAX_ATTEMPT_COST_MICROS: u64 = 1_000_000;

pub(super) struct LegacyModelPort<'a, Keys, Runner: LegacyModelRunner> {
    pub model: &'a Runner,
    pub store: &'a GovernedAgentSessionStore<'a, Keys>,
    pub resolver: &'a dyn floe_core::GovernedDependencyResolver,
    pub policy: &'a InferencePolicyDecision,
    pub context: &'a AgentContext,
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub active_agents: Vec<floe_agent::AgentCard>,
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
            let run_id = scope
                .root_run_id()
                .ok_or(AgentFailure::InvalidInput)?
                .as_uuid();
            let usage = UsageLedger::new(
                MAX_ATTEMPT_TOKENS,
                MAX_ATTEMPT_COST_MICROS,
                Default::default(),
            );
            let legacy_request = LegacyModelRequest {
                usage,
                replay: vec![],
                schema_version: floe_agent::AGENT_VERSION,
                prompt: floe_agent::manager_prompt(self.context.persona.as_ref())?,
                person_id: self.person_id,
                session_id: self.session_id,
                turn_id: run_id,
                policy: self.policy.clone(),
                context: self.context.clone(),
                messages: legacy_messages(&request, run_id)?,
                capabilities: self.capabilities.clone(),
                active_agents: self.active_agents.clone(),
                remaining_tokens: MAX_ATTEMPT_TOKENS,
                remaining_cost_micros: MAX_ATTEMPT_COST_MICROS,
                max_output_bytes: self.max_output_bytes,
                deadline: scope.deadline(),
                cancellation: scope.cancellation().clone(),
            };
            let governed = GovernedModel {
                model: self.model,
                store: self.store,
                resolver: self.resolver,
            };
            let response = floe_agent::generate_with_recovery(&governed, legacy_request).await?;
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
                    schema_version: floe_agent::AGENT_VERSION,
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
    pub experts: &'a ConversationExperts<'a>,
    pub task_coordinator: &'a floe_experts::TaskCoordinator<
        crate::vault_host::task_repository::VaultTaskRepository<Keys>,
    >,
    pub schedule_endpoint: &'a super::expert_dispatch::schedule::ScheduleEndpoint<Keys>,
    pub turn_request: &'a floe_protocol::AgentConversationTurnRequestDto,
    pub context: &'a AgentContext,
    pub store: &'a GovernedAgentSessionStore<'a, Keys>,
    pub person_id: PersonId,
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
            if request.selected_agent_id == BuiltinExpertKind::Schedule.package_id() {
                let run_id = scope
                    .root_run_id()
                    .ok_or(AgentFailure::InvalidInput)?
                    .as_uuid();
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
            if request.principal != self.person_id.to_string()
                || request.parent_run_id != scope.root_run_id().map(|run_id| run_id.as_uuid())
                || scope.task_id() != Some(request.task_id)
            {
                return Err(AgentFailure::CapabilityDenied);
            }
            let task_id = request.task_id.as_uuid();
            self.store.record_result_independent(task_id, task_id)?;
            let task = self
                .experts
                .handle_message(A2ASendMessageRequest {
                    usage: UsageLedger::default(),
                    schema_version: floe_agent::AGENT_VERSION,
                    person_id: self.person_id,
                    session_id: self.session_id,
                    parent_turn_id: request.parent_run_id.ok_or(AgentFailure::InvalidInput)?,
                    agent_id: request.selected_agent_id.clone(),
                    message: A2AMessage {
                        message_id: request.invocation_key.as_uuid(),
                        context_id: request.parent_run_id.ok_or(AgentFailure::InvalidInput)?,
                        task_id: Some(task_id),
                        role: A2AMessageRole::User,
                        parts: vec![A2APart::Text {
                            text: request.message.clone(),
                        }],
                    },
                    max_output_bytes: self.max_output_bytes,
                    deadline: scope.deadline(),
                    cancellation: scope.cancellation().clone(),
                })
                .await?;
            legacy_task_receipt(
                request,
                task,
                self.store
                    .result_coverage(task_id, task_id)?
                    .unwrap_or(DependencyCoverage::Independent),
                self.max_output_bytes,
            )
        })
    }
}

pub(super) struct ManagerPayloadValidator;

impl floe_conversation::FinalPayloadValidator for ManagerPayloadValidator {
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

pub(super) fn contract_definition(card: &floe_agent::AgentCard) -> AgentDefinition {
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

fn legacy_task_receipt(
    request: DelegationRequest,
    task: A2ATask,
    coverage: DependencyCoverage,
    maximum_bytes: usize,
) -> Result<TaskReceipt, AgentFailure> {
    if task.id != request.task_id.as_uuid()
        || task.context_id != request.parent_run_id.ok_or(AgentFailure::InvalidInput)?
        || task.agent_id != request.selected_agent_id
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    coverage
        .validate()
        .map_err(|_| AgentFailure::InvalidModelOutput)?;
    let state = match task.state {
        A2ATaskState::Submitted => TaskState::Submitted,
        A2ATaskState::Working => TaskState::Working,
        A2ATaskState::Completed => TaskState::Completed,
        A2ATaskState::Failed => TaskState::Failed,
        A2ATaskState::Cancelled => TaskState::Cancelled,
        A2ATaskState::Rejected => TaskState::Rejected,
    };
    let result = if state == TaskState::Completed {
        task.data_part(floe_agent::EXPERT_RESULT_MEDIA_TYPE)
            .or_else(|| task.result_text().ok())
            .map(str::to_owned)
            .ok_or(AgentFailure::InvalidModelOutput)?
            .into()
    } else {
        None
    };
    let issue = if state == TaskState::Completed {
        None
    } else {
        Some(task.failure.unwrap_or(match state {
            TaskState::Cancelled => AgentFailure::Cancelled,
            TaskState::Rejected => AgentFailure::CapabilityDenied,
            _ => AgentFailure::CapabilityUnavailable,
        }))
    };
    let snapshot = TaskSnapshot {
        task_id: request.task_id,
        parent_run_id: request.parent_run_id,
        principal: request.principal,
        agent_id: request.selected_agent_id,
        definition_revision: request.selected_definition_revision,
        state,
        result,
        artifacts: vec![],
        coverage: if state == TaskState::Completed {
            coverage
        } else {
            DependencyCoverage::Unknown
        },
        issue,
    };
    snapshot.validate(maximum_bytes)?;
    Ok(TaskReceipt {
        task_id: snapshot.task_id,
        snapshot,
        replay: None,
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
                schema_version: floe_agent::AGENT_VERSION,
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
}
