use std::collections::{HashMap, HashSet};

use floe_agent_contract::{
    AgentFailure, AgentMessage, DelegationPort, DelegationRequest, EngineRequest, EngineStep,
    ExecutionJournal, InvocationKey, JournalAck, JournalEvent, MessageRole, ModelPort,
    ModelRequest, ModelStep, ReplayReceipt, TaskId, TaskReceipt, ToolCall, ToolPort, ToolResult,
};
use uuid::Uuid;

pub trait FinalPayloadValidator: Sync {
    fn validate(
        &self,
        role: &str,
        text: &str,
        artifacts: &[floe_agent_contract::Artifact],
    ) -> Result<(), AgentFailure>;
}

struct ContractValidator;
impl FinalPayloadValidator for ContractValidator {
    fn validate(
        &self,
        _: &str,
        text: &str,
        artifacts: &[floe_agent_contract::Artifact],
    ) -> Result<(), AgentFailure> {
        if text.trim().is_empty() || text.len() > floe_agent_contract::MAX_OUTPUT_BYTES {
            return Err(AgentFailure::InvalidModelOutput);
        }
        artifacts.iter().try_for_each(|artifact| {
            artifact
                .coverage
                .validate()
                .map_err(|_| AgentFailure::InvalidModelOutput)
        })
    }
}

pub struct EnginePorts<'a> {
    pub model: &'a dyn ModelPort,
    pub tools: &'a dyn ToolPort,
    pub delegation: &'a dyn DelegationPort,
    pub journal: &'a dyn ExecutionJournal,
    pub validator: &'a dyn FinalPayloadValidator,
}

#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    pub max_attempt_tokens: u64,
    pub max_attempt_cost_micros: u64,
    pub max_tool_calls: u32,
    pub max_delegations: u32,
}
impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_attempt_tokens: 4_096,
            max_attempt_cost_micros: 1_000_000,
            max_tool_calls: 32,
            max_delegations: 16,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EngineReport {
    pub steps: Vec<EngineStep>,
    pub output: Option<String>,
    pub iterations: u32,
    pub attempt_ids: Vec<Uuid>,
}

pub struct Engine {
    config: EngineConfig,
    default_validator: ContractValidator,
}
impl Default for Engine {
    fn default() -> Self {
        Self::new(EngineConfig::default())
    }
}
impl Engine {
    pub const fn new(config: EngineConfig) -> Self {
        Self {
            config,
            default_validator: ContractValidator,
        }
    }

    pub async fn drive(
        &self,
        request: EngineRequest,
        ports: EnginePorts<'_>,
    ) -> Result<EngineReport, AgentFailure> {
        request.validate()?;
        let mut messages = request.messages.clone();
        let mut model_replay = request.replay.clone();
        let mut steps = Vec::new();
        let mut attempts = Vec::new();
        let mut tool_calls = 0;
        let mut delegations = 0;
        let mut unavailable = HashMap::<String, u8>::new();
        let mut seen_invocations = HashSet::<InvocationKey>::new();

        for iteration in 0..request.max_iterations {
            if request.scope.cancellation().is_cancelled() {
                return Err(floe_execution::tasks::cancellation_failure(
                    request.scope.cancellation(),
                ));
            }
            let attempt_id = Uuid::new_v4();
            attempts.push(attempt_id);
            let mut tokens = self.config.max_attempt_tokens;
            let mut cost = self.config.max_attempt_cost_micros;
            let mut reservation = request.scope.budget().begin(&mut tokens, &mut cost)?;
            let intent = request
                .scope
                .run(async {
                    ports
                        .journal
                        .record_intent(JournalEvent::ModelIntent { attempt_id })
                        .await
                })
                .await?;
            if let JournalAck::Replayed(receipt) = intent {
                return Err(if receipt.tool_id.is_some() || receipt.agent_id.is_some() {
                    AgentFailure::InvalidInput
                } else {
                    AgentFailure::Conflict
                });
            }
            if tokio::time::Instant::now() >= request.scope.deadline() {
                request
                    .scope
                    .cancellation()
                    .cancel_with_reason(floe_execution::CancelReason::Deadline);
                return Err(AgentFailure::DeadlineExceeded);
            }
            if request.scope.cancellation().is_cancelled() {
                return Err(floe_execution::tasks::cancellation_failure(
                    request.scope.cancellation(),
                ));
            }
            let model_request = ModelRequest {
                attempt_id,
                role: request.role_spec.clone(),
                prompt: request.prompt.clone(),
                bounded_context: request.bounded_context.clone(),
                messages: messages.clone(),
                catalog: request.allowed_catalog.clone(),
                replay: model_replay.clone(),
            };
            let model_scope = request.scope.child_scope(
                request.scope.deadline(),
                tokens.max(1),
                cost.max(1),
                None,
            );
            let response = model_scope
                .run(async {
                    reservation.mark_dispatched();
                    ports.model.generate(model_request, &model_scope).await
                })
                .await?;
            let settlement = reservation.settle(response.usage.tokens, response.usage.cost_micros);
            let journal_result = request
                .scope
                .run(async {
                    ports
                        .journal
                        .record_result(JournalEvent::ModelResult {
                            attempt_id,
                            usage: response.usage,
                        })
                        .await
                })
                .await;
            settlement?;
            journal_result?;
            if response.attempt_id != attempt_id || response.steps.is_empty() {
                return Err(AgentFailure::InvalidModelOutput);
            }
            let encoded_steps = serde_json::to_vec(&response.steps)
                .map_err(|_| AgentFailure::InvalidModelOutput)?;
            if encoded_steps.len() > request.max_output_bytes {
                return Err(AgentFailure::BudgetExceeded);
            }
            let corrections = validate_model_steps(&response.steps, &request.allowed_catalog)?;
            for step in &response.steps {
                if let ModelStep::Answer { text, artifacts } = step {
                    ports
                        .validator
                        .validate(&request.role_spec.role_id, text, artifacts)?;
                    if artifacts
                        .iter()
                        .any(|artifact| artifact.validate(request.max_output_bytes).is_err())
                    {
                        return Err(AgentFailure::InvalidModelOutput);
                    }
                }
            }

            for (step_index, step) in response.steps.into_iter().enumerate() {
                match step {
                    ModelStep::Preamble { text } => {
                        let message = AgentMessage {
                            message_id: Uuid::new_v4(),
                            role: MessageRole::Preamble,
                            text,
                            call_id: None,
                            coverage: floe_agent_contract::DependencyCoverage::Unknown,
                        };
                        message.validate()?;
                        messages.push(message);
                    }
                    ModelStep::Answer { text, artifacts } => {
                        ports
                            .validator
                            .validate(&request.role_spec.role_id, &text, &artifacts)?;
                        request
                            .scope
                            .run(async {
                                ports
                                    .journal
                                    .record_output(JournalEvent::Output {
                                        text: text.clone(),
                                        artifacts: artifacts.clone(),
                                    })
                                    .await
                            })
                            .await?;
                        steps.push(EngineStep::Answer {
                            text: text.clone(),
                            artifacts,
                        });
                        return Ok(EngineReport {
                            steps,
                            output: Some(text),
                            iterations: iteration + 1,
                            attempt_ids: attempts,
                        });
                    }
                    ModelStep::CallTool {
                        tool_id,
                        definition_revision,
                        input,
                    } => {
                        if tool_calls >= self.config.max_tool_calls {
                            return Err(AgentFailure::BudgetExceeded);
                        }
                        tool_calls += 1;
                        if let Some(correction) =
                            corrections.get(step_index).and_then(Option::as_ref)
                        {
                            let mut result = unavailable_tool(Uuid::new_v4(), correction);
                            result.issue = Some(floe_agent_contract::OutcomeIssue {
                                failure: AgentFailure::InvalidModelOutput,
                                retryable: true,
                            });
                            messages.push(observation(&result));
                            steps.push(EngineStep::Tool(result));
                            continue;
                        }
                        let Some(descriptor) = request
                            .allowed_catalog
                            .tools
                            .iter()
                            .find(|item| item.id == tool_id)
                        else {
                            let result = unavailable_tool(Uuid::new_v4(), "tool is not registered");
                            messages.push(observation(&result));
                            steps.push(EngineStep::Tool(result));
                            continue;
                        };
                        if descriptor.definition_revision != definition_revision {
                            let result =
                                unavailable_tool(Uuid::new_v4(), "tool descriptor is stale");
                            messages.push(observation(&result));
                            steps.push(EngineStep::Tool(result));
                            continue;
                        }
                        let invocation_key = stable_invocation_key(&request, iteration, tool_calls);
                        let call_id = Uuid::new_v5(
                            &request.scope.trace_context().request_id(),
                            format!("call:{}", invocation_key.as_uuid()).as_bytes(),
                        );
                        floe_agent_contract::validate_tool_input(&input)?;
                        let call = ToolCall {
                            call_id,
                            invocation_key,
                            tool_id: tool_id.clone(),
                            definition_revision,
                            input,
                        };
                        if !seen_invocations.insert(invocation_key) {
                            return Err(AgentFailure::Conflict);
                        }
                        let intent = request
                            .scope
                            .run(async {
                                ports
                                    .journal
                                    .record_intent(JournalEvent::ToolIntent { call: call.clone() })
                                    .await
                            })
                            .await?;
                        let result = if let JournalAck::Replayed(receipt) = intent {
                            verify_tool_replay(&request, &call, &receipt)?;
                            ToolResult {
                                call_id,
                                text: receipt.result.clone(),
                                artifacts: receipt.tool_artifacts.clone(),
                                coverage: receipt.tool_coverage.clone(),
                                issue: receipt.tool_issue.map(|failure| {
                                    floe_agent_contract::OutcomeIssue {
                                        failure,
                                        retryable: false,
                                    }
                                }),
                            }
                        } else {
                            let child = request.scope.child_scope(
                                request.scope.deadline(),
                                tokens.max(1),
                                cost.max(1),
                                None,
                            );
                            match child
                                .run(async { ports.tools.invoke(call.clone(), &child).await })
                                .await
                            {
                                Ok(result) => result,
                                Err(
                                    error @ (AgentFailure::CapabilityDenied
                                    | AgentFailure::CapabilityUnavailable
                                    | AgentFailure::PolicyDenied
                                    | AgentFailure::ConsentRequired),
                                ) => {
                                    let count = unavailable.entry(tool_id.clone()).or_default();
                                    *count += 1;
                                    if *count > 2 {
                                        return Err(AgentFailure::Stalled);
                                    }
                                    let mut result = unavailable_tool(call_id, "tool unavailable");
                                    result.issue = Some(floe_agent_contract::OutcomeIssue {
                                        failure: error,
                                        retryable: true,
                                    });
                                    result
                                }
                                Err(error) => return Err(error),
                            }
                        };
                        result.validate(call.call_id, request.max_output_bytes)?;
                        request
                            .scope
                            .run(async {
                                ports
                                    .journal
                                    .record_result(JournalEvent::ToolResult {
                                        result: result.clone(),
                                    })
                                    .await
                            })
                            .await?;
                        model_replay.push(tool_replay(&request, &call, &result));
                        messages.push(observation(&result));
                        steps.push(EngineStep::Tool(result));
                    }
                    ModelStep::Delegate {
                        agent_id,
                        definition_revision,
                        message,
                        context_refs,
                    } => {
                        if delegations >= self.config.max_delegations {
                            return Err(AgentFailure::BudgetExceeded);
                        }
                        delegations += 1;
                        let Some(card) = request
                            .allowed_catalog
                            .cards
                            .iter()
                            .find(|item| item.card.id == agent_id)
                        else {
                            messages.push(observation_text("delegation agent is not registered"));
                            continue;
                        };
                        if card.definition_revision != definition_revision {
                            messages.push(observation_text("delegation descriptor is stale"));
                            continue;
                        }
                        let invocation_key = stable_invocation_key(
                            &request,
                            iteration,
                            self.config.max_tool_calls + delegations,
                        );
                        let task_id = TaskId::from_uuid(Uuid::new_v5(
                            &request.scope.trace_context().request_id(),
                            format!("task:{}", invocation_key.as_uuid()).as_bytes(),
                        ))
                        .expect("uuid v5 is non-nil");
                        let delegation = DelegationRequest {
                            task_id,
                            parent_run_id: request.scope.root_run_id().map(|id| id.as_uuid()),
                            principal: request.principal.clone(),
                            invocation_key,
                            selected_agent_id: agent_id,
                            selected_definition_revision: definition_revision,
                            message,
                            context_refs,
                        };
                        if !seen_invocations.insert(delegation.invocation_key) {
                            return Err(AgentFailure::Conflict);
                        }
                        let intent = request
                            .scope
                            .run(async {
                                ports
                                    .journal
                                    .record_intent(JournalEvent::DelegationIntent {
                                        request: delegation.clone(),
                                    })
                                    .await
                            })
                            .await?;
                        let child = request.scope.child_scope(
                            request.scope.deadline(),
                            tokens.max(1),
                            cost.max(1),
                            Some(delegation.task_id),
                        );
                        let receipt = if let JournalAck::Replayed(replay_receipt) = intent {
                            replay_task(&delegation, &replay_receipt, request.max_output_bytes)?
                        } else {
                            child
                                .run(async {
                                    ports.delegation.delegate(delegation.clone(), &child).await
                                })
                                .await?
                        };
                        verify_receipt(&delegation, &receipt, request.max_output_bytes)?;
                        request
                            .scope
                            .run(async {
                                ports
                                    .journal
                                    .record_result(JournalEvent::DelegationResult {
                                        receipt: Box::new(receipt.clone()),
                                    })
                                    .await
                            })
                            .await?;
                        if let Some(replayed) = receipt.replay.clone() {
                            model_replay.push(replayed);
                        }
                        let text = receipt
                            .snapshot
                            .result
                            .clone()
                            .unwrap_or_else(|| format!("task {:?}", receipt.snapshot.state));
                        messages.push(observation_text(&text));
                        steps.push(EngineStep::Delegation(Box::new(receipt)));
                    }
                }
                if messages.len() > floe_agent_contract::MAX_AGENT_MESSAGES {
                    return Err(AgentFailure::BudgetExceeded);
                }
            }
            request
                .scope
                .run(async {
                    ports
                        .journal
                        .checkpoint(JournalEvent::Checkpoint {
                            iteration: iteration + 1,
                        })
                        .await
                })
                .await?;
        }
        Ok(EngineReport {
            steps,
            output: None,
            iterations: request.max_iterations,
            attempt_ids: attempts,
        })
    }

    pub async fn drive_with_default_validator(
        &self,
        request: EngineRequest,
        model: &dyn ModelPort,
        tools: &dyn ToolPort,
        delegation: &dyn DelegationPort,
        journal: &dyn ExecutionJournal,
    ) -> Result<EngineReport, AgentFailure> {
        self.drive(
            request,
            EnginePorts {
                model,
                tools,
                delegation,
                journal,
                validator: &self.default_validator,
            },
        )
        .await
    }
}

fn observation(result: &ToolResult) -> AgentMessage {
    AgentMessage {
        message_id: Uuid::new_v4(),
        role: MessageRole::Tool,
        text: result.text.clone(),
        call_id: Some(result.call_id),
        coverage: result.coverage.clone(),
    }
}
fn observation_text(text: &str) -> AgentMessage {
    observation_text_with_call(text.to_owned(), None)
}
fn observation_text_with_call(text: String, call_id: Option<Uuid>) -> AgentMessage {
    AgentMessage {
        message_id: Uuid::new_v4(),
        role: MessageRole::Tool,
        text,
        call_id,
        coverage: floe_agent_contract::DependencyCoverage::Unknown,
    }
}
fn unavailable_tool(call_id: Uuid, text: &str) -> ToolResult {
    ToolResult {
        call_id,
        text: text.to_owned(),
        artifacts: vec![],
        coverage: floe_agent_contract::DependencyCoverage::Unknown,
        issue: None,
    }
}

fn input_digest(input: &str) -> [u8; 32] {
    floe_agent_contract::input_digest(input)
}
fn tool_replay(request: &EngineRequest, call: &ToolCall, result: &ToolResult) -> ReplayReceipt {
    ReplayReceipt {
        principal: request.principal.clone(),
        run_id: request.scope.root_run_id(),
        task_id: request.scope.task_id(),
        agent_id: None,
        tool_id: Some(call.tool_id.clone()),
        definition_revision: call.definition_revision,
        input_digest: input_digest(&call.input),
        invocation_key: call.invocation_key,
        call_id: call.call_id,
        result: result.text.clone(),
        task_result: None,
        task_state: None,
        task_artifacts: vec![],
        task_issue: None,
        tool_artifacts: result.artifacts.clone(),
        tool_coverage: result.coverage.clone(),
        tool_issue: result.issue.as_ref().map(|issue| issue.failure),
    }
}
fn stable_invocation_key(request: &EngineRequest, iteration: u32, ordinal: u32) -> InvocationKey {
    let identity = format!(
        "{}:{:?}:{:?}:{}:{}:{}",
        request.principal,
        request.scope.root_run_id(),
        request.scope.task_id(),
        request.scope.trace_context().request_id(),
        iteration,
        ordinal
    );
    InvocationKey::from_uuid(Uuid::new_v5(
        &request.scope.trace_context().request_id(),
        identity.as_bytes(),
    ))
    .expect("uuid v5 is non-nil")
}
fn replay_task(
    request: &DelegationRequest,
    receipt: &ReplayReceipt,
    maximum_bytes: usize,
) -> Result<TaskReceipt, AgentFailure> {
    verify_task_replay(request, receipt)?;
    let task = TaskReceipt {
        task_id: request.task_id,
        snapshot: floe_agent_contract::TaskSnapshot {
            task_id: request.task_id,
            parent_run_id: request.parent_run_id,
            principal: request.principal.clone(),
            agent_id: request.selected_agent_id.clone(),
            definition_revision: request.selected_definition_revision,
            state: receipt.task_state.ok_or(AgentFailure::InvalidInput)?,
            result: receipt.task_result.clone(),
            artifacts: receipt.task_artifacts.clone(),
            issue: receipt.task_issue,
        },
        replay: Some(receipt.clone()),
    };
    task.snapshot.validate(maximum_bytes)?;
    if serde_json::to_vec(&task)
        .map(|encoded| encoded.len() > maximum_bytes)
        .unwrap_or(true)
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    Ok(task)
}
fn verify_receipt(
    request: &DelegationRequest,
    receipt: &TaskReceipt,
    maximum_bytes: usize,
) -> Result<(), AgentFailure> {
    receipt.snapshot.validate(maximum_bytes)?;
    if serde_json::to_vec(receipt)
        .map(|encoded| encoded.len() > maximum_bytes)
        .unwrap_or(true)
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    (receipt.task_id == request.task_id
        && receipt.snapshot.task_id == request.task_id
        && receipt.snapshot.parent_run_id == request.parent_run_id
        && receipt.snapshot.principal == request.principal
        && receipt.snapshot.agent_id == request.selected_agent_id
        && receipt.snapshot.definition_revision == request.selected_definition_revision)
        .then_some(())
        .ok_or(AgentFailure::InvalidInput)
}
fn verify_tool_replay(
    request: &EngineRequest,
    call: &ToolCall,
    receipt: &ReplayReceipt,
) -> Result<(), AgentFailure> {
    (receipt.principal == request.principal
        && receipt.run_id == request.scope.root_run_id()
        && receipt.task_id == request.scope.task_id()
        && receipt.agent_id.is_none()
        && receipt.tool_id.as_deref() == Some(call.tool_id.as_str())
        && receipt.definition_revision == call.definition_revision
        && receipt.invocation_key == call.invocation_key
        && receipt.call_id == call.call_id
        && receipt.task_state.is_none()
        && receipt.task_result.is_none()
        && receipt.task_artifacts.is_empty()
        && receipt.task_issue.is_none()
        && receipt.input_digest == input_digest(&call.input))
    .then_some(())
    .ok_or(AgentFailure::InvalidInput)
}
fn verify_task_replay(
    request: &DelegationRequest,
    receipt: &ReplayReceipt,
) -> Result<(), AgentFailure> {
    (receipt.principal == request.principal
        && receipt.run_id
            == request
                .parent_run_id
                .and_then(floe_agent_contract::RunId::from_uuid)
        && receipt.task_id == Some(request.task_id)
        && receipt.call_id == request.task_id.as_uuid()
        && receipt.agent_id.as_deref() == Some(request.selected_agent_id.as_str())
        && receipt.tool_id.is_none()
        && receipt.definition_revision == request.selected_definition_revision
        && receipt.invocation_key == request.invocation_key
        && receipt.task_state.is_some()
        && receipt.tool_artifacts.is_empty()
        && receipt.tool_issue.is_none()
        && receipt
            .task_artifacts
            .iter()
            .all(|artifact| artifact.coverage.validate().is_ok())
        && receipt.input_digest == input_digest(&request.message))
    .then_some(())
    .ok_or(AgentFailure::InvalidInput)
}

fn validate_model_steps(
    steps: &[ModelStep],
    catalog: &floe_agent_contract::AllowedCatalog,
) -> Result<Vec<Option<String>>, AgentFailure> {
    let mut corrections = Vec::with_capacity(steps.len());
    for step in steps {
        match step {
            ModelStep::CallTool {
                tool_id,
                definition_revision,
                input,
            } => {
                if tool_id.trim().is_empty() || *definition_revision == 0 || input.len() > 64 * 1024
                {
                    return Err(AgentFailure::InvalidModelOutput);
                }
                let Some(descriptor) = catalog.tools.iter().find(|item| {
                    item.id == *tool_id && item.definition_revision == *definition_revision
                }) else {
                    corrections.push(None);
                    continue;
                };
                let value = serde_json::from_str::<serde_json::Value>(input)
                    .map_err(|_| AgentFailure::InvalidModelOutput)?;
                let schema = serde_json::from_str::<serde_json::Value>(&descriptor.input_schema)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let compiled =
                    jsonschema::validator_for(&schema).map_err(|_| AgentFailure::InvalidInput)?;
                if compiled.is_valid(&value) {
                    corrections.push(None);
                } else {
                    corrections.push(Some(
                        "tool arguments do not satisfy the registered input schema".into(),
                    ));
                }
            }
            ModelStep::Delegate {
                agent_id,
                definition_revision,
                message,
                ..
            } => {
                if agent_id.trim().is_empty()
                    || *definition_revision == 0
                    || message.trim().is_empty()
                {
                    return Err(AgentFailure::InvalidModelOutput);
                }
                corrections.push(None);
            }
            ModelStep::Preamble { text } | ModelStep::Answer { text, .. } => {
                if text.trim().is_empty() || text.len() > floe_agent_contract::MAX_OUTPUT_BYTES {
                    return Err(AgentFailure::InvalidModelOutput);
                }
                corrections.push(None);
            }
        }
    }
    Ok(corrections)
}

#[cfg(test)]
mod tests {
    use floe_agent_contract::{
        AllowedCatalog, BoundedContext, DependencyCoverage, EngineRequest, ModelResponse,
        ModelUsage, RoleSpec, ToolDescriptor,
    };
    use floe_execution::budget::{BudgetConfig, BudgetLedger};
    use floe_execution::{Cancellation, ExecutionScope};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    use uuid::Uuid;

    use super::*;

    struct Model;
    impl ModelPort for Model {
        fn generate<'a>(
            &'a self,
            request: ModelRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
            Box::pin(async move {
                let steps = if request
                    .messages
                    .iter()
                    .any(|message| message.role == MessageRole::Tool)
                {
                    vec![ModelStep::Answer {
                        text: "done".into(),
                        artifacts: vec![],
                    }]
                } else {
                    vec![ModelStep::CallTool {
                        tool_id: "lookup".into(),
                        definition_revision: 1,
                        input: "{}".into(),
                    }]
                };
                Ok(ModelResponse {
                    attempt_id: request.attempt_id,
                    steps,
                    usage: ModelUsage {
                        tokens: 2,
                        cost_micros: 1,
                    },
                })
            })
        }
    }

    struct MalformedBatchModel;
    impl ModelPort for MalformedBatchModel {
        fn generate<'a>(
            &'a self,
            request: ModelRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
            Box::pin(async move {
                Ok(ModelResponse {
                    attempt_id: request.attempt_id,
                    steps: vec![
                        ModelStep::CallTool {
                            tool_id: "lookup".into(),
                            definition_revision: 1,
                            input: "{}".into(),
                        },
                        ModelStep::Answer {
                            text: String::new(),
                            artifacts: vec![],
                        },
                    ],
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                })
            })
        }
    }

    struct Tools {
        calls: Arc<AtomicUsize>,
    }
    impl ToolPort for Tools {
        fn invoke<'a>(
            &'a self,
            call: ToolCall,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ToolResult, AgentFailure>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(ToolResult {
                    call_id: call.call_id,
                    text: "observed".into(),
                    artifacts: vec![],
                    coverage: DependencyCoverage::Independent,
                    issue: None,
                })
            })
        }
    }
    struct InvalidResultTools {
        oversized: bool,
    }
    impl ToolPort for InvalidResultTools {
        fn invoke<'a>(
            &'a self,
            call: ToolCall,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ToolResult, AgentFailure>> {
            let call_id = if self.oversized {
                call.call_id
            } else {
                Uuid::new_v4()
            };
            let text = if self.oversized {
                "x".repeat(floe_agent_contract::MAX_OUTPUT_BYTES + 1)
            } else {
                "observed".into()
            };
            Box::pin(async move {
                Ok(ToolResult {
                    call_id,
                    text,
                    artifacts: vec![],
                    coverage: DependencyCoverage::Independent,
                    issue: None,
                })
            })
        }
    }
    struct Delegations;
    impl DelegationPort for Delegations {
        fn delegate<'a>(
            &'a self,
            _: DelegationRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<TaskReceipt, AgentFailure>> {
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }
    }
    struct Journal {
        reject_tool: bool,
        model_tokens: Arc<AtomicUsize>,
    }
    impl ExecutionJournal for Journal {
        fn record_intent<'a>(
            &'a self,
            event: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            let rejected = self.reject_tool && matches!(event, JournalEvent::ToolIntent { .. });
            Box::pin(async move {
                if rejected {
                    Err(AgentFailure::StorageUnavailable)
                } else {
                    Ok(JournalAck::Accepted { revision: 1 })
                }
            })
        }
        fn record_result<'a>(
            &'a self,
            event: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            if let JournalEvent::ModelResult { usage, .. } = event {
                self.model_tokens
                    .fetch_add(usage.tokens as usize, Ordering::SeqCst);
            }
            Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
        }
        fn record_output<'a>(
            &'a self,
            _: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
        }
        fn checkpoint<'a>(
            &'a self,
            _: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
        }
    }
    struct DeadlineAfterAckJournal {
        cancellation: Cancellation,
    }
    impl ExecutionJournal for DeadlineAfterAckJournal {
        fn record_intent<'a>(
            &'a self,
            _: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            let cancellation = self.cancellation.clone();
            Box::pin(async move {
                cancellation.cancel_with_reason(floe_execution::CancelReason::Deadline);
                Ok(JournalAck::Accepted { revision: 1 })
            })
        }
        fn record_result<'a>(
            &'a self,
            _: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
        }
        fn record_output<'a>(
            &'a self,
            _: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
        }
        fn checkpoint<'a>(
            &'a self,
            _: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
        }
    }
    struct Validator;
    impl FinalPayloadValidator for Validator {
        fn validate(
            &self,
            _: &str,
            text: &str,
            _: &[floe_agent_contract::Artifact],
        ) -> Result<(), AgentFailure> {
            (!text.is_empty())
                .then_some(())
                .ok_or(AgentFailure::InvalidModelOutput)
        }
    }

    fn request(scope: ExecutionScope) -> EngineRequest {
        EngineRequest {
            principal: "person:test".into(),
            role_spec: RoleSpec {
                role_id: "neutral".into(),
                prompt: "answer".into(),
                output_contract: "text".into(),
            },
            prompt: "question".into(),
            scope,
            bounded_context: BoundedContext {
                text: "context".into(),
                coverage: DependencyCoverage::Independent,
            },
            messages: vec![],
            allowed_catalog: AllowedCatalog {
                cards: vec![],
                tools: vec![ToolDescriptor {
                    id: "lookup".into(),
                    definition_revision: 1,
                    description: "lookup".into(),
                    input_schema: "{}".into(),
                    output_data_class: "derived".into(),
                }],
                revision: 1,
            },
            max_iterations: 3,
            max_output_bytes: 1024,
            replay: vec![],
        }
    }

    fn scope() -> ExecutionScope {
        let ledger = BudgetLedger::new(BudgetConfig::new(100, 100), Default::default());
        ExecutionScope::root(
            Cancellation::new(),
            tokio::time::Instant::now() + Duration::from_secs(5),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(Uuid::new_v4()),
        )
    }

    #[tokio::test]
    async fn drives_tool_then_answer_without_domain_ports() {
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let journal = Journal {
            reject_tool: false,
            model_tokens: Arc::new(AtomicUsize::new(0)),
        };
        let report = Engine::default()
            .drive(
                request(scope()),
                EnginePorts {
                    model: &Model,
                    tools: &tools,
                    delegation: &Delegations,
                    journal: &journal,
                    validator: &Validator,
                },
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 1);
        assert_eq!(report.iterations, 2);
    }

    #[tokio::test]
    async fn journal_rejection_prevents_tool_dispatch() {
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let result = Engine::default()
            .drive(
                request(scope()),
                EnginePorts {
                    model: &Model,
                    tools: &tools,
                    delegation: &Delegations,
                    journal: &Journal {
                        reject_tool: true,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    validator: &Validator,
                },
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::StorageUnavailable)));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn malformed_later_step_blocks_the_whole_model_batch() {
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let journal = Journal {
            reject_tool: false,
            model_tokens: Arc::new(AtomicUsize::new(0)),
        };
        let result = Engine::default()
            .drive(
                request(scope()),
                EnginePorts {
                    model: &MalformedBatchModel,
                    tools: &tools,
                    delegation: &Delegations,
                    journal: &journal,
                    validator: &Validator,
                },
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::InvalidModelOutput)));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        assert_eq!(journal.model_tokens.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn schema_mismatch_is_a_bounded_observation_without_provider_call() {
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let mut request = request(scope());
        request.allowed_catalog.tools[0].input_schema =
            r#"{"type":"object","required":["value"]}"#.into();
        let report = Engine::default()
            .drive(
                request,
                EnginePorts {
                    model: &Model,
                    tools: &tools,
                    delegation: &Delegations,
                    journal: &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    validator: &Validator,
                },
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn output_budget_is_checked_before_first_tool_in_batch() {
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let mut request = request(scope());
        request.max_output_bytes = 1;
        let result = Engine::default()
            .drive(
                request,
                EnginePorts {
                    model: &Model,
                    tools: &tools,
                    delegation: &Delegations,
                    journal: &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    validator: &Validator,
                },
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::BudgetExceeded)));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn provider_result_identity_and_bytes_are_fail_closed() {
        let wrong = InvalidResultTools { oversized: false };
        let result = Engine::default()
            .drive(
                request(scope()),
                EnginePorts {
                    model: &Model,
                    tools: &wrong,
                    delegation: &Delegations,
                    journal: &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    validator: &Validator,
                },
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::InvalidModelOutput)));

        let oversized = InvalidResultTools { oversized: true };
        let result = Engine::default()
            .drive(
                request(scope()),
                EnginePorts {
                    model: &Model,
                    tools: &oversized,
                    delegation: &Delegations,
                    journal: &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    validator: &Validator,
                },
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::InvalidModelOutput)));
    }

    #[tokio::test]
    async fn deadline_after_intent_ack_is_not_flattened_to_cancelled() {
        let cancellation = Cancellation::new();
        let mut request = request(scope());
        request.scope = ExecutionScope::root(
            cancellation.clone(),
            tokio::time::Instant::now() + Duration::from_secs(5),
            request.scope.budget().clone(),
            request.scope.trace_context(),
        );
        let result = Engine::default()
            .drive(
                request,
                EnginePorts {
                    model: &Model,
                    tools: &Tools {
                        calls: Arc::new(AtomicUsize::new(0)),
                    },
                    delegation: &Delegations,
                    journal: &DeadlineAfterAckJournal { cancellation },
                    validator: &Validator,
                },
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::DeadlineExceeded)));
    }
}
