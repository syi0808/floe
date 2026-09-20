use std::collections::{HashMap, HashSet};

use floe_agent_contract::{
    AgentFailure, AllowedCatalog, AuthorizedModelProjection, BatchCursor, DelegationPort,
    DelegationRequest, DependencyCoverage, EngineRequest, EngineStep, ExecutionJournal,
    InvocationKey, JournalAck, JournalEvent, MODEL_CORRECTION_TEXT, ModelConversation,
    ModelConversationEntry, ModelCorrection, ModelPort, ModelProjectionPort, ModelProjectionRequest,
    ModelRequest, ModelResponse, ModelStep, ModelUsage, PinnedAgentRevision, PinnedToolRevision,
    ReplayReceipt, TaskId, TaskReceipt, ToolCall, ToolPort, ToolResult, ValidatedModelBatch,
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

#[derive(Clone, Copy)]
pub struct EnginePorts<'a> {
    pub projection: &'a dyn ModelProjectionPort,
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
    /// The projection coverage of the validated batch that produced the
    /// final answer, carried durably through that batch. `None` when no
    /// answer committed.
    pub answering_projection_coverage: Option<DependencyCoverage>,
    pub iterations: u32,
    pub attempt_ids: Vec<Uuid>,
    pub execution_id: Uuid,
}

/// Which step kind a stable invocation identity belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvocationKind {
    Tool,
    Delegation,
}

/// Stable invocation identity: execution, batch, step ordinal, and kind only.
/// Replaying or skipping an earlier step never changes a later step's identity.
pub fn stable_invocation_key(
    execution_id: Uuid,
    batch_id: Uuid,
    step_ordinal: u32,
    kind: InvocationKind,
) -> InvocationKey {
    let tag = match kind {
        InvocationKind::Tool => "tool",
        InvocationKind::Delegation => "delegation",
    };
    InvocationKey::from_uuid(Uuid::new_v5(
        &execution_id,
        format!("{execution_id}:{batch_id}:{step_ordinal}:{tag}").as_bytes(),
    ))
    .expect("uuid v5 is non-nil")
}

/// Stable tool call id, derived from the same basis as the invocation key.
pub fn stable_call_id(execution_id: Uuid, batch_id: Uuid, step_ordinal: u32) -> Uuid {
    Uuid::new_v5(
        &execution_id,
        format!("{execution_id}:{batch_id}:{step_ordinal}:call").as_bytes(),
    )
}

/// Stable delegation task id, derived from the same basis as the invocation key.
pub fn stable_task_id(execution_id: Uuid, batch_id: Uuid, step_ordinal: u32) -> TaskId {
    TaskId::from_uuid(Uuid::new_v5(
        &execution_id,
        format!("{execution_id}:{batch_id}:{step_ordinal}:task").as_bytes(),
    ))
    .expect("uuid v5 is non-nil")
}

/// Stable preamble message id, derived from the same basis as the step
/// identities. Recovery rebuilds the same id so a preamble never depends on
/// the random id its first execution used.
pub fn stable_preamble_id(execution_id: Uuid, batch_id: Uuid, step_ordinal: u32) -> Uuid {
    Uuid::new_v5(
        &execution_id,
        format!("{execution_id}:{batch_id}:{step_ordinal}:preamble").as_bytes(),
    )
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
        Drive {
            config: &self.config,
            request,
            ports,
        }
        .run()
        .await
    }

    pub async fn drive_with_default_validator(
        &self,
        request: EngineRequest,
        projection: &dyn ModelProjectionPort,
        model: &dyn ModelPort,
        tools: &dyn ToolPort,
        delegation: &dyn DelegationPort,
        journal: &dyn ExecutionJournal,
    ) -> Result<EngineReport, AgentFailure> {
        self.drive(
            request,
            EnginePorts {
                projection,
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

struct Drive<'a> {
    config: &'a EngineConfig,
    request: EngineRequest,
    ports: EnginePorts<'a>,
}

impl Drive<'_> {
    async fn run(self) -> Result<EngineReport, AgentFailure> {
        let mut drive = ActiveDrive {
            config: self.config,
            request: self.request,
            ports: self.ports,
            execution_id: Uuid::new_v4(),
            conversation: ModelConversation {
                history: Vec::new(),
                current_turn: Vec::new(),
            },
            model_replay: Vec::new(),
            steps: Vec::new(),
            attempts: Vec::new(),
            completed_iterations: 0,
            tool_calls: 0,
            delegations: 0,
            unavailable: HashMap::new(),
            seen_invocations: HashSet::new(),
        };
        drive.conversation = drive.request.conversation.clone();
        drive.model_replay.clone_from(&drive.request.replay);
        if let Some(resume) = drive.request.resume.clone() {
            drive.execution_id = resume.validated_batch.execution_id;
            if !resume
                .validated_batch
                .pinned_revisions_hold(&drive.request.allowed_catalog)
            {
                return Err(AgentFailure::Conflict);
            }
            // Re-record the resumed batch so this run's journal is self-contained;
            // a crash mid-resume stays recoverable without the older journal.
            drive
                .checkpoint(JournalEvent::ValidatedBatch {
                    batch: resume.validated_batch.clone(),
                })
                .await?;
            drive
                .checkpoint(JournalEvent::BatchProgress {
                    cursor: resume.cursor.clone(),
                })
                .await?;
            // Corrections are deterministic given the batch and the catalog, so a
            // resume recomputes rather than stores them.
            let corrections = validate_model_steps(
                &resume.validated_batch.steps,
                &drive.request.allowed_catalog,
            )?;
            if let Some(report) = drive
                .execute_batch(
                    &resume.validated_batch,
                    resume.cursor.next_step_index,
                    &corrections,
                )
                .await?
            {
                return Ok(report);
            }
            drive.completed_iterations += 1;
            drive
                .checkpoint(JournalEvent::Checkpoint {
                    iteration: drive.completed_iterations,
                })
                .await?;
        }
        for _ in 0..drive.request.max_iterations {
            if drive.request.scope.cancellation().is_cancelled() {
                return Err(floe_execution::tasks::cancellation_failure(
                    drive.request.scope.cancellation(),
                ));
            }
            let (batch, corrections) = drive.validated_batch().await?;
            drive
                .checkpoint(JournalEvent::ValidatedBatch {
                    batch: batch.clone(),
                })
                .await?;
            drive
                .checkpoint(JournalEvent::BatchProgress {
                    cursor: BatchCursor {
                        batch_id: batch.batch_id,
                        next_step_index: 0,
                    },
                })
                .await?;
            if let Some(report) = drive.execute_batch(&batch, 0, &corrections).await? {
                return Ok(report);
            }
            drive.completed_iterations += 1;
            drive
                .checkpoint(JournalEvent::Checkpoint {
                    iteration: drive.completed_iterations,
                })
                .await?;
        }
        Ok(EngineReport {
            steps: drive.steps,
            output: None,
            answering_projection_coverage: None,
            iterations: drive.completed_iterations,
            attempt_ids: drive.attempts,
            execution_id: drive.execution_id,
        })
    }
}

struct ActiveDrive<'a> {
    config: &'a EngineConfig,
    request: EngineRequest,
    ports: EnginePorts<'a>,
    execution_id: Uuid,
    conversation: ModelConversation,
    model_replay: Vec<ReplayReceipt>,
    steps: Vec<EngineStep>,
    attempts: Vec<Uuid>,
    completed_iterations: u32,
    tool_calls: u32,
    delegations: u32,
    unavailable: HashMap<String, u8>,
    seen_invocations: HashSet<InvocationKey>,
}

impl ActiveDrive<'_> {
    fn report(
        &self,
        output: Option<String>,
        answering_projection_coverage: Option<DependencyCoverage>,
    ) -> EngineReport {
        EngineReport {
            steps: self.steps.clone(),
            output,
            answering_projection_coverage,
            iterations: self.completed_iterations + 1,
            attempt_ids: self.attempts.clone(),
            execution_id: self.execution_id,
        }
    }

    async fn checkpoint(&self, event: JournalEvent) -> Result<JournalAck, AgentFailure> {
        self.request.scope.run(self.ports.journal.checkpoint(event)).await
    }

    async fn record_model_result(
        &self,
        attempt_id: Uuid,
        usage: ModelUsage,
    ) -> Result<(), AgentFailure> {
        self.request
            .scope
            .run(self.ports.journal.record_result(JournalEvent::ModelResult {
                attempt_id,
                usage,
            }))
            .await?;
        Ok(())
    }

    /// One validated batch: project, attempt (plus at most one host correction
    /// on invalid structured output), then validate the whole batch before
    /// anything is dispatched.
    async fn validated_batch(
        &mut self,
    ) -> Result<(ValidatedModelBatch, Vec<Option<String>>), AgentFailure> {
        let mut correction: Option<ModelCorrection> = None;
        loop {
            let projection_request = ModelProjectionRequest {
                principal: self.request.principal.clone(),
                role: self.request.role_spec.clone(),
                conversation: self.conversation.clone(),
                catalog: self.request.allowed_catalog.clone(),
                max_output_bytes: self.request.max_output_bytes,
                correction: correction.clone(),
            };
            projection_request.validate()?;
            let projection = self
                .request
                .scope
                .run(
                    self.ports
                        .projection
                        .project(projection_request, &self.request.scope),
                )
                .await?;
            projection.validate()?;
            let attempt_id = Uuid::new_v4();
            self.attempts.push(attempt_id);
            let intent = self
                .request
                .scope
                .run(self.ports.journal.record_intent(JournalEvent::ModelIntent {
                    attempt_id,
                    projection_ref: projection.projection_ref,
                }))
                .await?;
            if let JournalAck::Replayed(receipt) = intent {
                return Err(if receipt.tool_id.is_some() || receipt.agent_id.is_some() {
                    AgentFailure::InvalidInput
                } else {
                    AgentFailure::Conflict
                });
            }
            if tokio::time::Instant::now() >= self.request.scope.deadline() {
                self.request
                    .scope
                    .cancellation()
                    .cancel_with_reason(floe_execution::CancelReason::Deadline);
                return Err(AgentFailure::DeadlineExceeded);
            }
            if self.request.scope.cancellation().is_cancelled() {
                return Err(floe_execution::tasks::cancellation_failure(
                    self.request.scope.cancellation(),
                ));
            }
            let model_projection = projection.clone();
            let model_request = ModelRequest {
                attempt_id,
                principal: self.request.principal.clone(),
                projection,
                catalog: self.request.allowed_catalog.clone(),
                purpose: self.request.purpose.clone(),
                consumer: self.request.consumer.clone(),
                preferred_profile_id: self.request.preferred_profile_id.clone(),
                replay: self.model_replay.clone(),
            };
            let model_scope = self.request.scope.child_scope(
                self.request.scope.deadline(),
                self.config.max_attempt_tokens.max(1),
                self.config.max_attempt_cost_micros.max(1),
                None,
            );
            let before = model_scope.budget().snapshot();
            let response = model_scope
                .run(self.ports.model.generate(model_request, &model_scope))
                .await;
            let response = match response {
                Ok(response) => response,
                Err(failure) => {
                    // Pair every intent with a result so recovery never sees a
                    // dangling attempt from a failed call. A dispatched failure
                    // already charged the scope budget's unknown estimate
                    // in-memory; journal that delta so the charge survives a
                    // restart instead of resurrecting budget. An undispatched
                    // failure leaves no charge, so the delta is zero.
                    let after = model_scope.budget().snapshot();
                    let usage = failed_attempt_usage(&before, &after);
                    self.record_model_result(attempt_id, usage).await?;
                    if is_correctable(&failure) && correction.is_none() {
                        correction = Some(ModelCorrection {
                            text: MODEL_CORRECTION_TEXT.into(),
                        });
                        continue;
                    }
                    return Err(failure);
                }
            };
            self.record_model_result(attempt_id, response.usage).await?;
            match self.validated_response(attempt_id, &model_projection, &response) {
                Ok(validated) => return Ok(validated),
                Err(failure) => {
                    if is_correctable(&failure) && correction.is_none() {
                        correction = Some(ModelCorrection {
                            text: MODEL_CORRECTION_TEXT.into(),
                        });
                        continue;
                    }
                    return Err(failure);
                }
            }
        }
    }

    fn validated_response(
        &self,
        attempt_id: Uuid,
        projection: &AuthorizedModelProjection,
        response: &ModelResponse,
    ) -> Result<(ValidatedModelBatch, Vec<Option<String>>), AgentFailure> {
        if response.attempt_id != attempt_id || response.steps.is_empty() {
            return Err(AgentFailure::InvalidModelOutput);
        }
        let encoded_steps = serde_json::to_vec(&response.steps)
            .map_err(|_| AgentFailure::InvalidModelOutput)?;
        if encoded_steps.len() > self.request.max_output_bytes {
            return Err(AgentFailure::BudgetExceeded);
        }
        validate_batch_shape(&response.steps)?;
        let corrections = validate_model_steps(&response.steps, &self.request.allowed_catalog)?;
        for step in &response.steps {
            if let ModelStep::Answer { text, artifacts } = step {
                self.ports
                    .validator
                    .validate(&self.request.role_spec.role_id, text, artifacts)?;
                if artifacts
                    .iter()
                    .any(|artifact| artifact.validate(self.request.max_output_bytes).is_err())
                {
                    return Err(AgentFailure::InvalidModelOutput);
                }
            }
        }
        let (tool_revisions, agent_revisions) =
            pin_revisions(&response.steps, &self.request.allowed_catalog);
        Ok((
            ValidatedModelBatch {
                execution_id: self.execution_id,
                attempt_id,
                projection_ref: projection.projection_ref,
                batch_id: Uuid::new_v4(),
                steps: response.steps.clone(),
                catalog_revision: self.request.allowed_catalog.revision,
                tool_revisions,
                agent_revisions,
                projection_coverage: projection.coverage.clone(),
            },
            corrections,
        ))
    }

    /// Execute stored steps from `start_index`. Returns a report when an answer
    /// commits; `None` means the batch ran out without one. Step ordinals are
    /// batch indexes, so identities never shift under replay or resume.
    async fn execute_batch(
        &mut self,
        batch: &ValidatedModelBatch,
        start_index: u32,
        corrections: &[Option<String>],
    ) -> Result<Option<EngineReport>, AgentFailure> {
        for (step_index, step) in batch.steps.iter().enumerate() {
            let ordinal = step_index as u32;
            if ordinal < start_index {
                continue;
            }
            match step {
                ModelStep::Preamble { text } => {
                    // A preamble consumes its validated step even though it
                    // has no side effect: the cursor is durable before the
                    // in-memory push, and recovery rebuilds the same stable
                    // id from the batch.
                    self.checkpoint(JournalEvent::BatchProgress {
                        cursor: BatchCursor {
                            batch_id: batch.batch_id,
                            next_step_index: ordinal + 1,
                        },
                    })
                    .await?;
                    self.push_current(ModelConversationEntry::Preamble {
                        message_id: stable_preamble_id(
                            batch.execution_id,
                            batch.batch_id,
                            ordinal,
                        ),
                        text: text.clone(),
                    })?;
                }
                ModelStep::Answer { text, artifacts } => {
                    // Canonical validation is authoritative; this only guards
                    // against a trailing step being silently ignored.
                    debug_assert_eq!(ordinal as usize + 1, batch.steps.len());
                    self.ports.validator.validate(
                        &self.request.role_spec.role_id,
                        text,
                        artifacts,
                    )?;
                    self.request
                        .scope
                        .run(self.ports.journal.record_output(JournalEvent::Output {
                            text: text.clone(),
                            artifacts: artifacts.clone(),
                        }))
                        .await?;
                    self.steps.push(EngineStep::Answer {
                        text: text.clone(),
                        artifacts: artifacts.clone(),
                    });
                    // The answering coverage is the persisted batch's own:
                    // an answer executed from a resumed batch commits the
                    // same coverage it was validated under.
                    return Ok(Some(self.report(
                        Some(text.clone()),
                        Some(batch.projection_coverage.clone()),
                    )));
                }
                ModelStep::CallTool {
                    tool_id,
                    definition_revision,
                    input,
                } => {
                    self.execute_tool(
                        batch,
                        ordinal,
                        corrections.get(step_index).and_then(Option::as_ref),
                        tool_id,
                        *definition_revision,
                        input,
                    )
                    .await?;
                }
                ModelStep::Delegate {
                    agent_id,
                    definition_revision,
                    message,
                    context_refs,
                } => {
                    self.execute_delegation(
                        batch,
                        ordinal,
                        agent_id,
                        *definition_revision,
                        message,
                        context_refs,
                    )
                    .await?;
                }
            }
        }
        Ok(None)
    }

    async fn execute_tool(
        &mut self,
        batch: &ValidatedModelBatch,
        ordinal: u32,
        correction: Option<&String>,
        tool_id: &str,
        definition_revision: u64,
        input: &str,
    ) -> Result<(), AgentFailure> {
        if self.tool_calls >= self.config.max_tool_calls {
            return Err(AgentFailure::BudgetExceeded);
        }
        self.tool_calls += 1;
        // Malformed arguments are invalid model output (host-correctable), even
        // when the tool itself is unknown; every path below keeps the input.
        floe_agent_contract::validate_tool_input(input)?;
        if tool_id.trim().is_empty() || definition_revision == 0 {
            return Err(AgentFailure::InvalidModelOutput);
        }
        // Stable identity first: a step that cannot dispatch still journals
        // its intent and host-generated result under the identity a dispatch
        // would use, so the observation survives a crash past a later step.
        let invocation_key =
            stable_invocation_key(self.execution_id, batch.batch_id, ordinal, InvocationKind::Tool);
        let call = ToolCall {
            call_id: stable_call_id(self.execution_id, batch.batch_id, ordinal),
            invocation_key,
            tool_id: tool_id.to_owned(),
            definition_revision,
            input: input.to_owned(),
        };
        if !self.seen_invocations.insert(invocation_key) {
            return Err(AgentFailure::Conflict);
        }
        let soft_failure = correction.cloned().or_else(|| {
            if !self
                .request
                .allowed_catalog
                .tools
                .iter()
                .any(|descriptor| descriptor.id == tool_id)
            {
                Some("tool is not registered".to_owned())
            } else if !self.request.allowed_catalog.tools.iter().any(|descriptor| {
                descriptor.id == tool_id && descriptor.definition_revision == definition_revision
            }) {
                Some("tool descriptor is stale".to_owned())
            } else {
                None
            }
        });
        let intent = self
            .request
            .scope
            .run(
                self.ports
                    .journal
                    .record_intent(JournalEvent::ToolIntent { call: call.clone() }),
            )
            .await?;
        let result = if let JournalAck::Replayed(receipt) = intent {
            verify_tool_replay(&self.request, &call, &receipt)?;
            ToolResult {
                call_id: call.call_id,
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
        } else if let Some(receipt) = find_tool_replay(&self.model_replay, &call) {
            // A settled result survived without its cursor ack: reuse it under
            // the same identity and only advance the cursor, never redispatch.
            verify_resumed_tool_replay(&self.request, &call, &receipt)?;
            ToolResult {
                call_id: call.call_id,
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
        } else if let Some(reason) = soft_failure {
            // Host-generated observation without dispatch: the model erred,
            // so the issue stays a retryable invalid-output signal rather
            // than a capability barrier, and the host-authored status carries
            // no source data.
            soft_tool_result(call.call_id, &reason)
        } else {
            let child = self.request.scope.child_scope(
                self.request.scope.deadline(),
                self.config.max_attempt_tokens.max(1),
                self.config.max_attempt_cost_micros.max(1),
                None,
            );
            match child
                .run(self.ports.tools.invoke(call.clone(), &child))
                .await
            {
                Ok(result) => {
                    self.model_replay
                        .push(tool_replay(&self.request, &call, &result));
                    result
                }
                Err(
                    error @ (AgentFailure::CapabilityDenied
                    | AgentFailure::CapabilityUnavailable
                    | AgentFailure::PolicyDenied
                    | AgentFailure::ConsentRequired),
                ) => {
                    let count = self.unavailable.entry(tool_id.to_owned()).or_default();
                    *count += 1;
                    if *count > 2 {
                        return Err(AgentFailure::Stalled);
                    }
                    let mut result = unavailable_tool(call.call_id, "tool unavailable");
                    result.issue = Some(floe_agent_contract::OutcomeIssue {
                        failure: error,
                        retryable: true,
                    });
                    result
                }
                Err(error) => return Err(error),
            }
        };
        result.validate(call.call_id, self.request.max_output_bytes)?;
        self.request
            .scope
            .run(
                self.ports
                    .journal
                    .record_result(JournalEvent::ToolResult {
                        result: result.clone(),
                    }),
            )
            .await?;
        self.checkpoint(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: batch.batch_id,
                next_step_index: ordinal + 1,
            },
        })
        .await?;
        self.push_current(ModelConversationEntry::ToolExchange {
            call,
            result: result.clone(),
        })?;
        self.steps.push(EngineStep::Tool(result));
        Ok(())
    }

    async fn execute_delegation(
        &mut self,
        batch: &ValidatedModelBatch,
        ordinal: u32,
        agent_id: &str,
        definition_revision: u64,
        message: &str,
        context_refs: &[String],
    ) -> Result<(), AgentFailure> {
        if self.delegations >= self.config.max_delegations {
            return Err(AgentFailure::BudgetExceeded);
        }
        self.delegations += 1;
        // A delegation the catalog cannot serve still journals its intent and
        // a host-generated terminal rejection, so the observation survives a
        // crash past a later step. The model erred, so the issue stays
        // invalid output rather than a capability barrier.
        let soft_failure = match self
            .request
            .allowed_catalog
            .cards
            .iter()
            .find(|item| item.card.id == agent_id)
        {
            None => Some(AgentFailure::InvalidModelOutput),
            Some(card) if card.definition_revision != definition_revision => {
                Some(AgentFailure::InvalidModelOutput)
            }
            Some(_) => None,
        };
        let invocation_key = stable_invocation_key(
            self.execution_id,
            batch.batch_id,
            ordinal,
            InvocationKind::Delegation,
        );
        let delegation = DelegationRequest {
            task_id: stable_task_id(self.execution_id, batch.batch_id, ordinal),
            parent_run_id: self.request.scope.root_run_id().map(|id| id.as_uuid()),
            principal: self.request.principal.clone(),
            invocation_key,
            selected_agent_id: agent_id.to_owned(),
            selected_definition_revision: definition_revision,
            message: message.to_owned(),
            context_refs: context_refs.to_owned(),
        };
        if !self.seen_invocations.insert(delegation.invocation_key) {
            return Err(AgentFailure::Conflict);
        }
        let intent = self
            .request
            .scope
            .run(self.ports.journal.record_intent(JournalEvent::DelegationIntent {
                request: delegation.clone(),
            }))
            .await?;
        let child = self.request.scope.child_scope(
            self.request.scope.deadline(),
            self.config.max_attempt_tokens.max(1),
            self.config.max_attempt_cost_micros.max(1),
            Some(delegation.task_id),
        );
        let receipt = if let JournalAck::Replayed(replay_receipt) = intent {
            replay_task(
                &delegation,
                &replay_receipt,
                self.request.max_output_bytes,
            )?
        } else if let Some(replay_receipt) = find_task_replay(&self.model_replay, &delegation) {
            // Same-identity result replay: the receipt is re-issued under this
            // run so the new journal pairs; outcome and coverage are preserved.
            let mut receipt = replay_resumed_task(
                &delegation,
                &replay_receipt,
                self.request.max_output_bytes,
            )?;
            receipt.snapshot.parent_run_id = delegation.parent_run_id;
            receipt.snapshot.validate(self.request.max_output_bytes)?;
            receipt
        } else if let Some(failure) = soft_failure {
            // Host-generated terminal rejection without dispatch: no
            // DelegationPort call, but the intent/result pair is durable and
            // carries the exact request linkage.
            TaskReceipt {
                task_id: delegation.task_id,
                snapshot: floe_agent_contract::TaskSnapshot {
                    task_id: delegation.task_id,
                    parent_run_id: delegation.parent_run_id,
                    principal: delegation.principal.clone(),
                    agent_id: delegation.selected_agent_id.clone(),
                    definition_revision: delegation.selected_definition_revision,
                    state: floe_agent_contract::TaskState::Rejected,
                    result: None,
                    artifacts: vec![],
                    coverage: floe_agent_contract::DependencyCoverage::Independent,
                    issue: Some(failure),
                },
                replay: None,
            }
        } else {
            let delegated = self
                .ports
                .delegation
                .delegate(delegation.clone(), &child);
            let receipt = child.run(delegated).await?;
            verify_receipt(&delegation, &receipt, self.request.max_output_bytes)?;
            if let Some(replayed) = receipt.replay.clone() {
                self.model_replay.push(replayed);
            }
            receipt
        };
        self.request
            .scope
            .run(self.ports.journal.record_result(JournalEvent::DelegationResult {
                receipt: Box::new(receipt.clone()),
            }))
            .await?;
        self.checkpoint(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: batch.batch_id,
                next_step_index: ordinal + 1,
            },
        })
        .await?;
        self.push_current(ModelConversationEntry::DelegationExchange {
            request: delegation,
            receipt: receipt.clone(),
        })?;
        self.steps.push(EngineStep::Delegation(Box::new(receipt)));
        Ok(())
    }

    fn push_current(&mut self, entry: ModelConversationEntry) -> Result<(), AgentFailure> {
        entry.validate()?;
        self.conversation.current_turn.push(entry);
        if self.conversation.len() > floe_agent_contract::MAX_AGENT_MESSAGES {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }
}

fn is_correctable(failure: &AgentFailure) -> bool {
    matches!(
        failure,
        AgentFailure::InvalidModelOutput
            | AgentFailure::LocalModelInvalidOutput
            | AgentFailure::ServerModelInvalidOutput
    )
}

/// Durable usage for a failed model attempt from the scope budget delta.
///
/// The transitional bridge charges a dispatched failure to the in-memory
/// ledger as an unknown estimate when its budget attempt drops; an
/// undispatched failure releases without charge. Success never uses this:
/// the response's actual usage stays authoritative so usage is not double
/// counted. The canonical journal usage carries only tokens and cost, so the
/// unknown estimate is folded conservatively into both.
fn failed_attempt_usage(
    before: &floe_execution::budget::BudgetSnapshot,
    after: &floe_execution::budget::BudgetSnapshot,
) -> ModelUsage {
    let tokens = after.usage.tokens.saturating_sub(before.usage.tokens);
    let settled_cost = after
        .settled
        .cost_micros
        .saturating_sub(before.settled.cost_micros);
    let unknown_cost = after
        .unknown_cost_micros
        .saturating_sub(before.unknown_cost_micros);
    ModelUsage {
        tokens,
        cost_micros: settled_cost.saturating_add(unknown_cost),
    }
}

/// Pin only references that matched the catalog at validation time. Steps that
/// reference unknown tools or agents soft-fail at execution both now and on
/// resume; pinning them would wrongly demand their presence later.
fn pin_revisions(
    steps: &[ModelStep],
    catalog: &AllowedCatalog,
) -> (Vec<PinnedToolRevision>, Vec<PinnedAgentRevision>) {
    let mut tools = Vec::new();
    let mut agents = Vec::new();
    for step in steps {
        match step {
            ModelStep::CallTool {
                tool_id,
                definition_revision,
                ..
            } => {
                if catalog.tools.iter().any(|descriptor| {
                    descriptor.id == *tool_id
                        && descriptor.definition_revision == *definition_revision
                }) && !tools.iter().any(|pinned: &PinnedToolRevision| {
                    pinned.tool_id == *tool_id
                }) {
                    tools.push(PinnedToolRevision {
                        tool_id: tool_id.clone(),
                        definition_revision: *definition_revision,
                    });
                }
            }
            ModelStep::Delegate {
                agent_id,
                definition_revision,
                ..
            } => {
                if catalog.cards.iter().any(|definition| {
                    definition.card.id == *agent_id
                        && definition.definition_revision == *definition_revision
                }) && !agents.iter().any(|pinned: &PinnedAgentRevision| {
                    pinned.agent_id == *agent_id
                }) {
                    agents.push(PinnedAgentRevision {
                        agent_id: agent_id.clone(),
                        definition_revision: *definition_revision,
                    });
                }
            }
            ModelStep::Preamble { .. } | ModelStep::Answer { .. } => {}
        }
    }
    tools.sort_by(|left, right| left.tool_id.cmp(&right.tool_id));
    agents.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    (tools, agents)
}

/// Host-generated result for a step that never dispatches: the model emitted
/// an unexecutable call, so the issue is retryable invalid output rather than
/// a capability barrier, and the host-authored status carries no source data.
fn soft_tool_result(call_id: Uuid, text: &str) -> ToolResult {
    ToolResult {
        call_id,
        text: text.to_owned(),
        artifacts: vec![],
        coverage: floe_agent_contract::DependencyCoverage::Independent,
        issue: Some(floe_agent_contract::OutcomeIssue {
            failure: AgentFailure::InvalidModelOutput,
            retryable: true,
        }),
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
        task_coverage: floe_agent_contract::DependencyCoverage::Unknown,
        task_issue: None,
        tool_artifacts: result.artifacts.clone(),
        tool_coverage: result.coverage.clone(),
        tool_issue: result.issue.as_ref().map(|issue| issue.failure),
    }
}

fn find_tool_replay(replay: &[ReplayReceipt], call: &ToolCall) -> Option<ReplayReceipt> {
    replay
        .iter()
        .find(|receipt| {
            receipt.agent_id.is_none()
                && receipt.invocation_key == call.invocation_key
                && receipt.call_id == call.call_id
        })
        .cloned()
}

fn find_task_replay(
    replay: &[ReplayReceipt],
    delegation: &DelegationRequest,
) -> Option<ReplayReceipt> {
    replay
        .iter()
        .find(|receipt| {
            receipt.tool_id.is_none()
                && receipt.invocation_key == delegation.invocation_key
                && receipt.call_id == delegation.task_id.as_uuid()
        })
        .cloned()
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
            coverage: receipt.task_coverage.clone(),
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

/// Cross-run result replay: same invocation identity and input, but the linkage
/// (run, parent) belongs to the run that recorded it, so linkage is not
/// compared here. The caller re-issues the receipt under this run.
fn replay_resumed_task(
    request: &DelegationRequest,
    receipt: &ReplayReceipt,
    maximum_bytes: usize,
) -> Result<TaskReceipt, AgentFailure> {
    verify_resumed_task_replay(request, receipt)?;
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
            coverage: receipt.task_coverage.clone(),
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
    if matches!(
        receipt.snapshot.state,
        floe_agent_contract::TaskState::Submitted | floe_agent_contract::TaskState::Working
    ) {
        return Err(AgentFailure::Conflict);
    }
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
        && receipt.task_coverage == floe_agent_contract::DependencyCoverage::Unknown
        && receipt.task_issue.is_none()
        && receipt.input_digest == input_digest(&call.input))
    .then_some(())
    .ok_or(AgentFailure::InvalidInput)
}

fn verify_resumed_tool_replay(
    request: &EngineRequest,
    call: &ToolCall,
    receipt: &ReplayReceipt,
) -> Result<(), AgentFailure> {
    (receipt.principal == request.principal
        && receipt.agent_id.is_none()
        && receipt.tool_id.as_deref() == Some(call.tool_id.as_str())
        && receipt.definition_revision == call.definition_revision
        && receipt.invocation_key == call.invocation_key
        && receipt.call_id == call.call_id
        && receipt.task_state.is_none()
        && receipt.task_result.is_none()
        && receipt.task_artifacts.is_empty()
        && receipt.task_coverage == floe_agent_contract::DependencyCoverage::Unknown
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
        && receipt.task_coverage.validate().is_ok()
        && receipt.input_digest == input_digest(&request.message))
    .then_some(())
    .ok_or(AgentFailure::InvalidInput)
}

fn verify_resumed_task_replay(
    request: &DelegationRequest,
    receipt: &ReplayReceipt,
) -> Result<(), AgentFailure> {
    (receipt.principal == request.principal
        && receipt.task_id == Some(request.task_id)
        && receipt.call_id == request.task_id.as_uuid()
        && receipt.agent_id.as_deref() == Some(request.selected_agent_id.as_str())
        && receipt.tool_id.is_none()
        && receipt.definition_revision == request.selected_definition_revision
        && receipt.invocation_key == request.invocation_key
        && receipt.task_state.is_some()
        && !matches!(
            receipt.task_state,
            Some(
                floe_agent_contract::TaskState::Submitted
                    | floe_agent_contract::TaskState::Working
            )
        )
        && receipt.tool_artifacts.is_empty()
        && receipt.tool_issue.is_none()
        && receipt
            .task_artifacts
            .iter()
            .all(|artifact| artifact.coverage.validate().is_ok())
        && receipt.task_coverage.validate().is_ok()
        && receipt.input_digest == input_digest(&request.message))
    .then_some(())
    .ok_or(AgentFailure::InvalidInput)
}

/// Maximum steps in one model batch, carried over from the legacy path.
const MAX_BATCH_STEPS: usize = 16;

/// Maximum tool calls in one tool batch, carried over from the legacy path.
const MAX_BATCH_TOOL_CALLS: usize = 8;

/// Whole-batch grammar, checked before any per-step validation or dispatch.
/// An answer batch is exactly one final answer with no tool or delegation;
/// a tool batch carries no answer or delegation; a delegation batch carries
/// exactly one delegation and nothing else executable. Every preamble leads
/// its batch: no preamble may follow an executable step.
fn validate_batch_shape(steps: &[ModelStep]) -> Result<(), AgentFailure> {
    if steps.is_empty() || steps.len() > MAX_BATCH_STEPS {
        return Err(AgentFailure::InvalidModelOutput);
    }
    if let Some(first_executable) = steps
        .iter()
        .position(|step| !matches!(step, ModelStep::Preamble { .. }))
    {
        if steps[first_executable..]
            .iter()
            .any(|step| matches!(step, ModelStep::Preamble { .. }))
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    } else {
        return Err(AgentFailure::InvalidModelOutput);
    }
    let mut answers = 0;
    let mut tools = 0;
    let mut delegations = 0;
    for step in steps {
        match step {
            ModelStep::Answer { .. } => answers += 1,
            ModelStep::CallTool { .. } => tools += 1,
            ModelStep::Delegate { .. } => delegations += 1,
            ModelStep::Preamble { .. } => {}
        }
    }
    if answers > 0 {
        if answers != 1
            || tools != 0
            || delegations != 0
            || !matches!(steps.last(), Some(ModelStep::Answer { .. }))
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    } else if tools > 0 {
        if delegations != 0 || tools > MAX_BATCH_TOOL_CALLS {
            return Err(AgentFailure::InvalidModelOutput);
        }
    } else if delegations > 0 {
        if delegations != 1 {
            return Err(AgentFailure::InvalidModelOutput);
        }
    } else {
        return Err(AgentFailure::InvalidModelOutput);
    }
    Ok(())
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
                context_refs,
            } => {
                // Oversized delegations fail here, before any dispatch: the
                // journal and the downstream coordinator share these bounds.
                if agent_id.trim().is_empty()
                    || *definition_revision == 0
                    || message.trim().is_empty()
                    || message.len() > floe_agent_contract::MAX_OUTPUT_BYTES
                    || !floe_agent_contract::valid_context_refs(context_refs)
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
        AgentCard, AgentDefinition, AllowedCatalog, AuthorizedModelProjection, ContextEnvelope,
        ContextManifest, ContextualData, DataClass, DependencyCoverage, EngineRequest,
        EngineResumeState, ModelConversation, ModelConversationEntry, ModelProjectionRequest,
        ModelResponse, ModelUsage, ProjectionRef, RoleSpec, RuntimeContext, ScopedInstructions,
        ToolDescriptor,
        prompts::{PromptAssembly, PromptComponent, PromptComponentKind, PromptRole},
    };
    use floe_execution::budget::{BudgetConfig, BudgetLedger};
    use floe_execution::{Cancellation, ExecutionScope};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    use uuid::Uuid;

    use super::*;

    fn test_envelope(
        conversation: ModelConversation,
        correction: Option<ModelCorrection>,
        max_output_bytes: usize,
    ) -> ContextEnvelope {
        ContextEnvelope {
            schema_version: floe_agent_contract::AGENT_VERSION,
            stable_instructions: PromptAssembly {
                schema_version: floe_agent_contract::AGENT_VERSION,
                role: PromptRole::Manager,
                components: vec![
                    PromptComponent {
                        kind: PromptComponentKind::BehaviorKernel,
                        source: "test-kernel".into(),
                        revision: 1,
                        content: "kernel".into(),
                    },
                    PromptComponent {
                        kind: PromptComponentKind::Role,
                        source: "test-role".into(),
                        revision: 1,
                        content: "role".into(),
                    },
                    PromptComponent {
                        kind: PromptComponentKind::CapabilityProtocol,
                        source: "test-protocol".into(),
                        revision: 1,
                        content: "protocol".into(),
                    },
                ],
            },
            scoped_instructions: ScopedInstructions {
                purpose: "test-purpose".into(),
                response_contract: "text".into(),
                available_capabilities: vec![],
                active_experts: vec![],
                correction,
            },
            contextual_data: ContextualData {
                projection_version: 1,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            },
            conversation,
            runtime: RuntimeContext { max_output_bytes },
            manifest: ContextManifest {
                prompt_components: vec![],
                evidence: vec![],
                memories: vec![],
                agent_cards: vec![],
            },
        }
    }

    struct Projector {
        corrections: Arc<Mutex<Vec<Option<ModelCorrection>>>>,
    }
    impl Projector {
        fn new() -> (Self, Arc<Mutex<Vec<Option<ModelCorrection>>>>) {
            let corrections = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    corrections: Arc::clone(&corrections),
                },
                corrections,
            )
        }
    }
    impl ModelProjectionPort for Projector {
        fn project<'a>(
            &'a self,
            request: ModelProjectionRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<AuthorizedModelProjection, AgentFailure>>
        {
            request.validate().unwrap();
            self.corrections
                .lock()
                .unwrap()
                .push(request.correction.clone());
            let envelope = test_envelope(
                request.conversation.clone(),
                request.correction.clone(),
                request.max_output_bytes,
            );
            Box::pin(async move {
                Ok(AuthorizedModelProjection {
                    projection_ref: ProjectionRef::new(),
                    projection_revision: 1,
                    envelope,
                    coverage: DependencyCoverage::Independent,
                    input_data_classes: vec![DataClass::Synthetic],
                })
            })
        }
    }

    fn has_tool_exchange(request: &ModelRequest) -> bool {
        let conversation = &request.projection.envelope.conversation;
        conversation
            .history
            .iter()
            .chain(&conversation.current_turn)
            .any(|entry| matches!(entry, ModelConversationEntry::ToolExchange { .. }))
    }

    struct Model;
    impl ModelPort for Model {
        fn generate<'a>(
            &'a self,
            request: ModelRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
            Box::pin(async move {
                let steps = if has_tool_exchange(&request) {
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

    struct InvalidOnceModel {
        calls: Arc<AtomicUsize>,
    }
    impl ModelPort for InvalidOnceModel {
        fn generate<'a>(
            &'a self,
            request: ModelRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                let steps = if call == 0 {
                    vec![ModelStep::Answer {
                        text: String::new(),
                        artifacts: vec![],
                    }]
                } else {
                    vec![ModelStep::Answer {
                        text: "fixed".into(),
                        artifacts: vec![],
                    }]
                };
                Ok(ModelResponse {
                    attempt_id: request.attempt_id,
                    steps,
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

    struct RecordingJournal {
        events: Arc<Mutex<Vec<JournalEvent>>>,
        fail_batch: bool,
        fail_cursor_from: Option<u32>,
    }
    impl RecordingJournal {
        fn new() -> (Self, Arc<Mutex<Vec<JournalEvent>>>) {
            let events = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    events: Arc::clone(&events),
                    fail_batch: false,
                    fail_cursor_from: None,
                },
                events,
            )
        }
    }
    impl ExecutionJournal for RecordingJournal {
        fn record_intent<'a>(
            &'a self,
            event: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            self.events.lock().unwrap().push(event);
            Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
        }
        fn record_result<'a>(
            &'a self,
            event: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            self.events.lock().unwrap().push(event);
            Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
        }
        fn record_output<'a>(
            &'a self,
            event: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            self.events.lock().unwrap().push(event);
            Box::pin(async { Ok(JournalAck::Accepted { revision: 1 }) })
        }
        fn checkpoint<'a>(
            &'a self,
            event: JournalEvent,
        ) -> floe_agent_contract::BoxFuture<'a, Result<JournalAck, AgentFailure>> {
            let fail = self.fail_batch && matches!(event, JournalEvent::ValidatedBatch { .. })
                || matches!(&event, JournalEvent::BatchProgress { cursor }
                    if self
                        .fail_cursor_from
                        .is_some_and(|from| cursor.next_step_index >= from));
            if !fail {
                self.events.lock().unwrap().push(event);
            }
            Box::pin(async move {
                if fail {
                    Err(AgentFailure::StorageUnavailable)
                } else {
                    Ok(JournalAck::Accepted { revision: 1 })
                }
            })
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
                instructions: "answer".into(),
                output_contract: "text".into(),
            },
            scope,
            conversation: ModelConversation {
                history: vec![],
                current_turn: vec![ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "question".into(),
                }],
            },
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
            purpose: "test-purpose".into(),
            consumer: "test-consumer".into(),
            preferred_profile_id: None,
            max_iterations: 3,
            max_output_bytes: 1024,
            replay: vec![],
            resume: None,
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

    fn ports<'a>(
        projection: &'a Projector,
        model: &'a dyn ModelPort,
        tools: &'a dyn ToolPort,
        journal: &'a dyn ExecutionJournal,
        validator: &'a dyn FinalPayloadValidator,
    ) -> EnginePorts<'a> {
        EnginePorts {
            projection,
            model,
            tools,
            delegation: &Delegations,
            journal,
            validator,
        }
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
        let (projection, _) = Projector::new();
        let report = Engine::default()
            .drive(
                request(scope()),
                ports(&projection, &Model, &tools, &journal, &Validator),
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
        let (projection, _) = Projector::new();
        let result = Engine::default()
            .drive(
                request(scope()),
                ports(
                    &projection,
                    &Model,
                    &tools,
                    &Journal {
                        reject_tool: true,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    &Validator,
                ),
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
        let (projection, corrections) = Projector::new();
        let result = Engine::default()
            .drive(
                request(scope()),
                ports(&projection, &MalformedBatchModel, &tools, &journal, &Validator),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::InvalidModelOutput)));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        // The malformed batch gets exactly one host correction, then stops.
        assert_eq!(journal.model_tokens.load(Ordering::SeqCst), 2);
        assert_eq!(corrections.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn schema_mismatch_is_a_bounded_observation_without_provider_call() {
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let mut engine_request = request(scope());
        engine_request.allowed_catalog.tools[0].input_schema =
            r#"{"type":"object","required":["value"]}"#.into();
        let (projection, _) = Projector::new();
        let report = Engine::default()
            .drive(
                engine_request,
                ports(
                    &projection,
                    &Model,
                    &tools,
                    &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    &Validator,
                ),
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
        let mut engine_request = request(scope());
        engine_request.max_output_bytes = 1;
        let (projection, _) = Projector::new();
        let result = Engine::default()
            .drive(
                engine_request,
                ports(
                    &projection,
                    &Model,
                    &tools,
                    &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    &Validator,
                ),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::BudgetExceeded)));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn provider_result_identity_and_bytes_are_fail_closed() {
        let wrong = InvalidResultTools { oversized: false };
        let (projection, _) = Projector::new();
        let result = Engine::default()
            .drive(
                request(scope()),
                ports(
                    &projection,
                    &Model,
                    &wrong,
                    &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    &Validator,
                ),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::InvalidModelOutput)));

        let oversized = InvalidResultTools { oversized: true };
        let (projection, _) = Projector::new();
        let result = Engine::default()
            .drive(
                request(scope()),
                ports(
                    &projection,
                    &Model,
                    &oversized,
                    &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    &Validator,
                ),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::InvalidModelOutput)));
    }

    #[tokio::test]
    async fn deadline_after_intent_ack_is_not_flattened_to_cancelled() {
        let cancellation = Cancellation::new();
        let mut engine_request = request(scope());
        engine_request.scope = ExecutionScope::root(
            cancellation.clone(),
            tokio::time::Instant::now() + Duration::from_secs(5),
            engine_request.scope.budget().clone(),
            engine_request.scope.trace_context(),
        );
        let (projection, _) = Projector::new();
        let result = Engine::default()
            .drive(
                engine_request,
                ports(
                    &projection,
                    &Model,
                    &Tools {
                        calls: Arc::new(AtomicUsize::new(0)),
                    },
                    &DeadlineAfterAckJournal { cancellation },
                    &Validator,
                ),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::DeadlineExceeded)));
    }

    #[tokio::test]
    async fn validated_batch_ack_failure_dispatches_nothing() {
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (mut journal, events) = RecordingJournal::new();
        journal.fail_batch = true;
        let (projection, _) = Projector::new();
        let result = Engine::default()
            .drive(
                request(scope()),
                ports(&projection, &Model, &tools, &journal, &Validator),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::StorageUnavailable)));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        let events = events.lock().unwrap();
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, JournalEvent::Output { .. }))
        );
        assert!(
            events.iter().all(|event| !matches!(
                event,
                JournalEvent::ToolIntent { .. } | JournalEvent::DelegationIntent { .. }
            ))
        );
    }

    #[tokio::test]
    async fn recovery_resumes_validated_batch_without_model_recall() {
        let execution_id = Uuid::new_v4();
        let batch = ValidatedModelBatch {
            execution_id,
            attempt_id: Uuid::new_v4(),
            projection_ref: ProjectionRef::new(),
            batch_id: Uuid::new_v4(),
            steps: vec![ModelStep::CallTool {
                tool_id: "lookup".into(),
                definition_revision: 1,
                input: "{}".into(),
            }],
            catalog_revision: 1,
            tool_revisions: vec![PinnedToolRevision {
                tool_id: "lookup".into(),
                definition_revision: 1,
            }],
            agent_revisions: vec![],
            projection_coverage: DependencyCoverage::Independent,
        };
        batch.validate(1024).unwrap();
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, events) = RecordingJournal::new();
        let (projection, projections) = Projector::new();
        struct AnswerOnce {
            calls: Arc<AtomicUsize>,
            saw_exchange: Arc<AtomicUsize>,
        }
        impl ModelPort for AnswerOnce {
            fn generate<'a>(
                &'a self,
                request: ModelRequest,
                _: &'a ExecutionScope,
            ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>>
            {
                self.calls.fetch_add(1, Ordering::SeqCst);
                if has_tool_exchange(&request) {
                    self.saw_exchange.fetch_add(1, Ordering::SeqCst);
                }
                Box::pin(async move {
                    Ok(ModelResponse {
                        attempt_id: request.attempt_id,
                        steps: vec![ModelStep::Answer {
                            text: "done".into(),
                            artifacts: vec![],
                        }],
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    })
                })
            }
        }
        let model = AnswerOnce {
            calls: Arc::new(AtomicUsize::new(0)),
            saw_exchange: Arc::new(AtomicUsize::new(0)),
        };
        let mut engine_request = request(scope());
        engine_request.resume = Some(EngineResumeState {
            validated_batch: batch.clone(),
            cursor: BatchCursor {
                batch_id: batch.batch_id,
                next_step_index: 0,
            },
        });
        let report = Engine::default()
            .drive(engine_request, ports(&projection, &model, &tools, &journal, &Validator))
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(report.execution_id, execution_id);
        // The stored step dispatched fresh (no replay) under its stable identity,
        // then exactly one model call ran for the following iteration.
        assert_eq!(tools.calls.load(Ordering::SeqCst), 1);
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);
        assert_eq!(model.saw_exchange.load(Ordering::SeqCst), 1);
        assert_eq!(projections.lock().unwrap().len(), 1);
        let events = events.lock().unwrap();
        let intent = events.iter().find_map(|event| match event {
            JournalEvent::ToolIntent { call } => Some(call.clone()),
            _ => None,
        });
        let intent = intent.expect("resumed step journals its intent");
        assert_eq!(
            intent.invocation_key,
            stable_invocation_key(execution_id, batch.batch_id, 0, InvocationKind::Tool)
        );
        assert_eq!(
            intent.call_id,
            stable_call_id(execution_id, batch.batch_id, 0)
        );
    }

    fn history_dependency() -> floe_context_contract::ContextDependency {
        use floe_context_contract::{
            ConnectionId, ConnectorId, ConsumerPolicyAuthority, ExecutionOwnerId, GrantAuthority,
            GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
            GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
        };
        let person = floe_context_contract::PersonId::new();
        let source = GrantSourceBinding::try_new(
            person,
            ConnectionId::try_new("connection").unwrap(),
            ConnectorId::try_new("connector").unwrap(),
            ExecutionOwnerId::try_new("owner").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let now = chrono::Utc::now();
        floe_context_contract::ContextDependency::try_new(
            person,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("resource").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"fingerprint".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - chrono::Duration::minutes(1),
            now + chrono::Duration::minutes(5),
        )
        .unwrap()
    }

    struct DependentProjector {
        coverage: DependencyCoverage,
    }

    impl ModelProjectionPort for DependentProjector {
        fn project<'a>(
            &'a self,
            request: ModelProjectionRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<AuthorizedModelProjection, AgentFailure>>
        {
            request.validate().unwrap();
            let envelope = test_envelope(
                request.conversation.clone(),
                request.correction.clone(),
                request.max_output_bytes,
            );
            let coverage = self.coverage.clone();
            Box::pin(async move {
                Ok(AuthorizedModelProjection {
                    projection_ref: ProjectionRef::new(),
                    projection_revision: 1,
                    envelope,
                    coverage,
                    input_data_classes: vec![DataClass::Synthetic],
                })
            })
        }
    }

    struct AnswerOnly;

    impl ModelPort for AnswerOnly {
        fn generate<'a>(
            &'a self,
            request: ModelRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
            Box::pin(async move {
                Ok(ModelResponse {
                    attempt_id: request.attempt_id,
                    steps: vec![ModelStep::Answer {
                        text: "done".into(),
                        artifacts: vec![],
                    }],
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                })
            })
        }
    }

    #[tokio::test]
    async fn answering_projection_coverage_flows_from_batch_to_report() {
        let dependency = history_dependency();
        let coverage = DependencyCoverage::dependent(dependency).unwrap();
        let projection = DependentProjector {
            coverage: coverage.clone(),
        };
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, events) = RecordingJournal::new();
        let report = Engine::default()
            .drive(
                request(scope()),
                EnginePorts {
                    projection: &projection,
                    model: &AnswerOnly,
                    tools: &tools,
                    delegation: &Delegations,
                    journal: &journal,
                    validator: &Validator,
                },
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(report.answering_projection_coverage, Some(coverage.clone()));
        let journaled = events
            .lock()
            .unwrap()
            .iter()
            .find_map(|event| match event {
                JournalEvent::ValidatedBatch { batch } => Some(batch.clone()),
                _ => None,
            })
            .expect("answer batch is journaled before execution");
        assert_eq!(journaled.projection_coverage, coverage);
    }

    #[tokio::test]
    async fn resumed_answer_batch_commits_persisted_projection_coverage_without_model_recall() {
        let dependency = history_dependency();
        let coverage = DependencyCoverage::dependent(dependency).unwrap();
        let execution_id = Uuid::new_v4();
        let batch = ValidatedModelBatch {
            execution_id,
            attempt_id: Uuid::new_v4(),
            projection_ref: ProjectionRef::new(),
            batch_id: Uuid::new_v4(),
            steps: vec![ModelStep::Answer {
                text: "resumed".into(),
                artifacts: vec![],
            }],
            catalog_revision: 1,
            tool_revisions: vec![],
            agent_revisions: vec![],
            projection_coverage: coverage.clone(),
        };
        batch.validate(1024).unwrap();
        struct MustNotGenerate;
        impl ModelPort for MustNotGenerate {
            fn generate<'a>(
                &'a self,
                _: ModelRequest,
                _: &'a ExecutionScope,
            ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>>
            {
                panic!("resumed answer must not recall the model")
            }
        }
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, _) = RecordingJournal::new();
        let (projection, _) = Projector::new();
        let mut engine_request = request(scope());
        engine_request.resume = Some(EngineResumeState {
            validated_batch: batch.clone(),
            cursor: BatchCursor {
                batch_id: batch.batch_id,
                next_step_index: 0,
            },
        });
        let report = Engine::default()
            .drive(
                engine_request,
                ports(&projection, &MustNotGenerate, &tools, &journal, &Validator),
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("resumed"));
        assert_eq!(report.answering_projection_coverage, Some(coverage));
        assert_eq!(report.execution_id, execution_id);
    }

    #[tokio::test]
    async fn stable_invocation_identity_is_deterministic_per_step() {
        let execution_id = Uuid::new_v4();
        let batch_id = Uuid::new_v4();
        assert_eq!(
            stable_invocation_key(execution_id, batch_id, 0, InvocationKind::Tool),
            stable_invocation_key(execution_id, batch_id, 0, InvocationKind::Tool)
        );
        assert_eq!(
            stable_call_id(execution_id, batch_id, 0),
            stable_call_id(execution_id, batch_id, 0)
        );
        assert_ne!(
            stable_invocation_key(execution_id, batch_id, 0, InvocationKind::Tool),
            stable_invocation_key(execution_id, batch_id, 1, InvocationKind::Tool)
        );
        assert_ne!(
            stable_invocation_key(execution_id, batch_id, 0, InvocationKind::Tool),
            stable_invocation_key(execution_id, batch_id, 0, InvocationKind::Delegation)
        );
        assert_ne!(
            stable_invocation_key(execution_id, batch_id, 0, InvocationKind::Tool),
            stable_invocation_key(execution_id, Uuid::new_v4(), 0, InvocationKind::Tool)
        );
        assert_ne!(
            stable_call_id(execution_id, batch_id, 0),
            stable_task_id(execution_id, batch_id, 0).as_uuid()
        );
    }

    #[tokio::test]
    async fn cursor_ack_loss_replays_result_without_side_effect() {
        // Run 1 journals intent and result, then loses the cursor ack.
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (mut journal, run_events) = RecordingJournal::new();
        journal.fail_cursor_from = Some(1);
        let (projection, _) = Projector::new();
        let result = Engine::default()
            .drive(
                request(scope()),
                ports(&projection, &Model, &tools, &journal, &Validator),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::StorageUnavailable)));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 1);
        let run_events = run_events.lock().unwrap().clone();
        let batch = run_events.iter().find_map(|event| match event {
            JournalEvent::ValidatedBatch { batch } => Some(batch.clone()),
            _ => None,
        });
        let batch = batch.expect("batch is journaled before execution");
        let (call, result) = run_events
            .iter()
            .find_map(|event| match event {
                JournalEvent::ToolResult { result } => Some(result.clone()),
                _ => None,
            })
            .and_then(|result| {
                run_events
                    .iter()
                    .find_map(|event| match event {
                        JournalEvent::ToolIntent { call }
                            if call.call_id == result.call_id =>
                        {
                            Some((call.clone(), result.clone()))
                        }
                        _ => None,
                    })
            })
            .expect("intent and result are journaled before the lost cursor ack");

        // Run 2 resumes from cursor 0 with the settled receipt carried over a
        // run boundary (no run linkage): same identity, no redispatch.
        let receipt = ReplayReceipt {
            principal: "person:test".into(),
            run_id: None,
            task_id: None,
            agent_id: None,
            tool_id: Some(call.tool_id.clone()),
            definition_revision: call.definition_revision,
            input_digest: floe_agent_contract::input_digest(&call.input),
            invocation_key: call.invocation_key,
            call_id: call.call_id,
            result: result.text.clone(),
            task_result: None,
            task_state: None,
            task_artifacts: vec![],
            task_coverage: DependencyCoverage::Unknown,
            task_issue: None,
            tool_artifacts: result.artifacts.clone(),
            tool_coverage: result.coverage.clone(),
            tool_issue: None,
        };
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, resumed_events) = RecordingJournal::new();
        let (projection, _) = Projector::new();
        struct AnswerDone;
        impl ModelPort for AnswerDone {
            fn generate<'a>(
                &'a self,
                request: ModelRequest,
                _: &'a ExecutionScope,
            ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>>
            {
                Box::pin(async move {
                    Ok(ModelResponse {
                        attempt_id: request.attempt_id,
                        steps: vec![ModelStep::Answer {
                            text: "done".into(),
                            artifacts: vec![],
                        }],
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    })
                })
            }
        }
        let mut engine_request = request(scope());
        engine_request.replay = vec![receipt];
        engine_request.resume = Some(EngineResumeState {
            validated_batch: batch.clone(),
            cursor: BatchCursor {
                batch_id: batch.batch_id,
                next_step_index: 0,
            },
        });
        let report = Engine::default()
            .drive(
                engine_request,
                ports(&projection, &AnswerDone, &tools, &journal, &Validator),
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(report.execution_id, batch.execution_id);
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        let resumed_events = resumed_events.lock().unwrap();
        let replayed = resumed_events
            .iter()
            .find_map(|event| match event {
                JournalEvent::ToolIntent { call } => Some(call.clone()),
                _ => None,
            })
            .expect("resumed step re-journals its intent in the new run");
        assert_eq!(replayed.call_id, call.call_id);
        assert_eq!(replayed.invocation_key, call.invocation_key);
        assert!(
            resumed_events.iter().any(|event| matches!(
                event,
                JournalEvent::BatchProgress { cursor }
                    if cursor.batch_id == batch.batch_id && cursor.next_step_index == 1
            ))
        );
    }

    #[tokio::test]
    async fn invalid_model_output_gets_one_host_correction() {
        let model = InvalidOnceModel {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (projection, corrections) = Projector::new();
        let report = Engine::default()
            .drive(
                request(scope()),
                ports(
                    &projection,
                    &model,
                    &tools,
                    &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    &Validator,
                ),
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("fixed"));
        assert_eq!(model.calls.load(Ordering::SeqCst), 2);
        assert_eq!(report.attempt_ids.len(), 2);
        assert_ne!(report.attempt_ids[0], report.attempt_ids[1]);
        let corrections = corrections.lock().unwrap();
        assert_eq!(corrections.len(), 2);
        assert!(corrections[0].is_none());
        assert_eq!(
            corrections[1].as_ref().map(|correction| correction.text.as_str()),
            Some(MODEL_CORRECTION_TEXT)
        );
    }

    #[tokio::test]
    async fn second_invalid_model_output_stops_without_unbounded_retry() {
        // Always invalid: every call fails, so the engine must stop after the
        // single correction instead of retrying forever.
        struct AlwaysInvalid {
            calls: Arc<AtomicUsize>,
        }
        impl ModelPort for AlwaysInvalid {
            fn generate<'a>(
                &'a self,
                request: ModelRequest,
                _: &'a ExecutionScope,
            ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>>
            {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    Ok(ModelResponse {
                        attempt_id: request.attempt_id,
                        steps: vec![ModelStep::Answer {
                            text: String::new(),
                            artifacts: vec![],
                        }],
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    })
                })
            }
        }
        let invalid = AlwaysInvalid {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (projection, corrections) = Projector::new();
        let result = Engine::default()
            .drive(
                request(scope()),
                ports(
                    &projection,
                    &invalid,
                    &tools,
                    &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    &Validator,
                ),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::InvalidModelOutput)));
        assert_eq!(invalid.calls.load(Ordering::SeqCst), 2);
        assert_eq!(corrections.lock().unwrap().len(), 2);
    }

    struct ScriptedModel {
        steps: Vec<ModelStep>,
        calls: Arc<AtomicUsize>,
    }
    impl ModelPort for ScriptedModel {
        fn generate<'a>(
            &'a self,
            request: ModelRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let steps = self.steps.clone();
            Box::pin(async move {
                Ok(ModelResponse {
                    attempt_id: request.attempt_id,
                    steps,
                    usage: ModelUsage {
                        tokens: 1,
                        cost_micros: 1,
                    },
                })
            })
        }
    }

    struct CountingDelegations {
        calls: Arc<AtomicUsize>,
    }
    impl DelegationPort for CountingDelegations {
        fn delegate<'a>(
            &'a self,
            _: DelegationRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<TaskReceipt, AgentFailure>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
        }
    }

    fn answer(text: &str) -> ModelStep {
        ModelStep::Answer {
            text: text.into(),
            artifacts: vec![],
        }
    }

    fn tool_call() -> ModelStep {
        ModelStep::CallTool {
            tool_id: "lookup".into(),
            definition_revision: 1,
            input: "{}".into(),
        }
    }

    fn delegation() -> ModelStep {
        ModelStep::Delegate {
            agent_id: "expert-a".into(),
            definition_revision: 1,
            message: "summarize".into(),
            context_refs: vec![],
        }
    }

    struct ShapeOutcome {
        result: Result<EngineReport, AgentFailure>,
        model_calls: usize,
        tool_calls: usize,
        delegation_calls: usize,
        validated_batches: usize,
        projections: usize,
    }

    async fn drive_shape(steps: Vec<ModelStep>) -> ShapeOutcome {
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let delegations = CountingDelegations {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let model = ScriptedModel {
            steps,
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, events) = RecordingJournal::new();
        let (projection, corrections) = Projector::new();
        let result = Engine::default()
            .drive(
                request(scope()),
                EnginePorts {
                    projection: &projection,
                    model: &model,
                    tools: &tools,
                    delegation: &delegations,
                    journal: &journal,
                    validator: &Validator,
                },
            )
            .await;
        let validated_batches = events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| matches!(event, JournalEvent::ValidatedBatch { .. }))
            .count();
        ShapeOutcome {
            result,
            model_calls: model.calls.load(Ordering::SeqCst),
            tool_calls: tools.calls.load(Ordering::SeqCst),
            delegation_calls: delegations.calls.load(Ordering::SeqCst),
            validated_batches,
            projections: corrections.lock().unwrap().len(),
        }
    }

    fn assert_shape_rejected(outcome: ShapeOutcome) {
        assert!(matches!(
            outcome.result,
            Err(AgentFailure::InvalidModelOutput)
        ));
        // Both the initial attempt and the single host correction run the
        // shape check, and neither dispatches anything.
        assert_eq!(outcome.model_calls, 2);
        assert_eq!(outcome.projections, 2);
        assert_eq!(outcome.tool_calls, 0);
        assert_eq!(outcome.delegation_calls, 0);
        assert_eq!(outcome.validated_batches, 0);
    }

    #[tokio::test]
    async fn answer_then_tool_rejects_before_any_dispatch() {
        assert_shape_rejected(drive_shape(vec![answer("done"), tool_call()]).await);
    }

    #[tokio::test]
    async fn tool_then_answer_rejects_before_any_dispatch() {
        assert_shape_rejected(drive_shape(vec![tool_call(), answer("done")]).await);
    }

    #[tokio::test]
    async fn answer_then_delegation_rejects_before_any_dispatch() {
        assert_shape_rejected(drive_shape(vec![answer("done"), delegation()]).await);
    }

    #[tokio::test]
    async fn multiple_answers_are_invalid() {
        assert_shape_rejected(drive_shape(vec![answer("one"), answer("two")]).await);
    }

    #[tokio::test]
    async fn answer_must_be_final_step() {
        assert_shape_rejected(
            drive_shape(vec![
                answer("done"),
                ModelStep::Preamble {
                    text: "trailing".into(),
                },
            ])
            .await,
        );
    }

    #[tokio::test]
    async fn tool_and_delegation_cannot_share_batch() {
        assert_shape_rejected(drive_shape(vec![tool_call(), delegation()]).await);
    }

    #[tokio::test]
    async fn too_many_tool_calls_are_rejected_before_dispatch() {
        let steps = (0..9).map(|_| tool_call()).collect();
        assert_shape_rejected(drive_shape(steps).await);
    }

    #[tokio::test]
    async fn oversized_context_refs_fail_before_provider_dispatch() {
        // 129 tiny references: under the output byte budget, over the count.
        let refs = (0..129).map(|_| "r".to_string()).collect();
        assert_shape_rejected(
            drive_shape(vec![ModelStep::Delegate {
                agent_id: "expert-a".into(),
                definition_revision: 1,
                message: "summarize".into(),
                context_refs: refs,
            }])
            .await,
        );
    }

    #[tokio::test]
    async fn preamble_may_lead_an_answer_batch() {
        let outcome = drive_shape(vec![
            ModelStep::Preamble {
                text: "thinking".into(),
            },
            answer("done"),
        ])
        .await;
        let report = outcome.result.unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(outcome.tool_calls, 0);
        assert_eq!(outcome.delegation_calls, 0);
        assert_eq!(outcome.validated_batches, 1);
    }

    #[tokio::test]
    async fn trailing_preamble_is_invalid_model_output() {
        assert_shape_rejected(
            drive_shape(vec![
                tool_call(),
                ModelStep::Preamble {
                    text: "trailing".into(),
                },
            ])
            .await,
        );
        assert_shape_rejected(
            drive_shape(vec![
                delegation(),
                ModelStep::Preamble {
                    text: "trailing".into(),
                },
            ])
            .await,
        );
    }

    #[tokio::test]
    async fn preamble_advances_cursor_with_stable_identity() {
        struct PreambleToolOnce;
        impl ModelPort for PreambleToolOnce {
            fn generate<'a>(
                &'a self,
                request: ModelRequest,
                _: &'a ExecutionScope,
            ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>>
            {
                Box::pin(async move {
                    let steps = if has_tool_exchange(&request) {
                        vec![answer("done")]
                    } else {
                        vec![
                            ModelStep::Preamble {
                                text: "thinking".into(),
                            },
                            tool_call(),
                        ]
                    };
                    Ok(ModelResponse {
                        attempt_id: request.attempt_id,
                        steps,
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    })
                })
            }
        }
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, events) = RecordingJournal::new();
        let (projection, _) = Projector::new();
        let report = Engine::default()
            .drive(
                request(scope()),
                ports(
                    &projection,
                    &PreambleToolOnce,
                    &tools,
                    &journal,
                    &Validator,
                ),
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 1);
        let events = events.lock().unwrap();
        let batch = events
            .iter()
            .find_map(|event| match event {
                JournalEvent::ValidatedBatch { batch } => Some(batch.clone()),
                _ => None,
            })
            .expect("preamble batch is validated");
        assert!(matches!(batch.steps.as_slice(), [ModelStep::Preamble { .. }, ModelStep::CallTool { .. }]));
        let cursors = events
            .iter()
            .filter_map(|event| match event {
                JournalEvent::BatchProgress { cursor } if cursor.batch_id == batch.batch_id => {
                    Some(cursor.next_step_index)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            cursors.contains(&1),
            "preamble consumes its step: {cursors:?}"
        );
        assert!(
            cursors.contains(&2),
            "tool consumes its step: {cursors:?}"
        );
        assert_eq!(
            stable_preamble_id(batch.execution_id, batch.batch_id, 0),
            stable_preamble_id(batch.execution_id, batch.batch_id, 0)
        );
        assert_ne!(
            stable_preamble_id(batch.execution_id, batch.batch_id, 0),
            stable_call_id(batch.execution_id, batch.batch_id, 0)
        );
    }

    #[tokio::test]
    async fn preamble_may_lead_a_tool_batch() {
        struct AnswerAfterTools;
        impl ModelPort for AnswerAfterTools {
            fn generate<'a>(
                &'a self,
                request: ModelRequest,
                _: &'a ExecutionScope,
            ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>>
            {
                Box::pin(async move {
                    let steps = if has_tool_exchange(&request) {
                        vec![answer("done")]
                    } else {
                        vec![
                            ModelStep::Preamble {
                                text: "thinking".into(),
                            },
                            tool_call(),
                        ]
                    };
                    Ok(ModelResponse {
                        attempt_id: request.attempt_id,
                        steps,
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    })
                })
            }
        }
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (projection, _) = Projector::new();
        let report = Engine::default()
            .drive(
                request(scope()),
                ports(
                    &projection,
                    &AnswerAfterTools,
                    &tools,
                    &Journal {
                        reject_tool: false,
                        model_tokens: Arc::new(AtomicUsize::new(0)),
                    },
                    &Validator,
                ),
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 1);
    }

    fn unknown_tool_call() -> ModelStep {
        ModelStep::CallTool {
            tool_id: "missing.tool".into(),
            definition_revision: 1,
            input: "{}".into(),
        }
    }

    #[tokio::test]
    async fn unknown_tool_never_calls_tool_port() {
        struct UnknownOnce;
        impl ModelPort for UnknownOnce {
            fn generate<'a>(
                &'a self,
                request: ModelRequest,
                _: &'a ExecutionScope,
            ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>>
            {
                Box::pin(async move {
                    let steps = if has_tool_exchange(&request) {
                        vec![answer("done")]
                    } else {
                        vec![unknown_tool_call()]
                    };
                    Ok(ModelResponse {
                        attempt_id: request.attempt_id,
                        steps,
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    })
                })
            }
        }
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, events) = RecordingJournal::new();
        let (projection, _) = Projector::new();
        let report = Engine::default()
            .drive(
                request(scope()),
                ports(
                    &projection,
                    &UnknownOnce,
                    &tools,
                    &journal,
                    &Validator,
                ),
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        // The soft observation is a durable intent/result pair under the
        // step's stable identity, carrying a retryable invalid-output issue
        // rather than a capability barrier.
        let events = events.lock().unwrap();
        let batch = events
            .iter()
            .find_map(|event| match event {
                JournalEvent::ValidatedBatch { batch } => Some(batch.clone()),
                _ => None,
            })
            .expect("soft step still validates its batch");
        let (call, result) = events
            .iter()
            .find_map(|event| match event {
                JournalEvent::ToolResult { result } => Some(result.clone()),
                _ => None,
            })
            .and_then(|result| {
                events
                    .iter()
                    .find_map(|event| match event {
                        JournalEvent::ToolIntent { call }
                            if call.call_id == result.call_id =>
                        {
                            Some((call.clone(), result.clone()))
                        }
                        _ => None,
                    })
            })
            .expect("soft step journals an intent/result pair");
        assert_eq!(call.tool_id, "missing.tool");
        assert_eq!(
            call.invocation_key,
            stable_invocation_key(
                batch.execution_id,
                batch.batch_id,
                0,
                InvocationKind::Tool
            )
        );
        assert_eq!(
            call.call_id,
            stable_call_id(batch.execution_id, batch.batch_id, 0)
        );
        assert_eq!(result.text, "tool is not registered");
        assert!(matches!(
            result.issue,
            Some(floe_agent_contract::OutcomeIssue {
                failure: AgentFailure::InvalidModelOutput,
                retryable: true,
            })
        ));
        assert_eq!(result.coverage, DependencyCoverage::Independent);
        assert!(
            events.iter().any(|event| matches!(
                event,
                JournalEvent::BatchProgress { cursor }
                    if cursor.batch_id == batch.batch_id && cursor.next_step_index == 1
            )),
            "soft step advances the cursor: {events:?}"
        );
    }

    #[tokio::test]
    async fn unknown_tool_observation_survives_crash_after_later_step() {
        // Run 1 journals the soft pair and the later real pair, then loses
        // the final cursor ack.
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (mut journal, run_events) = RecordingJournal::new();
        journal.fail_cursor_from = Some(2);
        let (projection, _) = Projector::new();
        let model = ScriptedModel {
            steps: vec![unknown_tool_call(), tool_call()],
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let result = Engine::default()
            .drive(
                request(scope()),
                ports(&projection, &model, &tools, &journal, &Validator),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::StorageUnavailable)));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 1);
        let run_events = run_events.lock().unwrap().clone();
        let batch = run_events
            .iter()
            .find_map(|event| match event {
                JournalEvent::ValidatedBatch { batch } => Some(batch.clone()),
                _ => None,
            })
            .expect("batch is journaled before execution");
        // Pair every journaled intent with its result, as recovery does, and
        // carry both the soft and the real observation into the resume.
        let mut exchanges = Vec::new();
        let mut replay = Vec::new();
        for event in &run_events {
            if let JournalEvent::ToolResult { result } = event {
                let call = run_events
                    .iter()
                    .find_map(|event| match event {
                        JournalEvent::ToolIntent { call }
                            if call.call_id == result.call_id =>
                        {
                            Some(call.clone())
                        }
                        _ => None,
                    })
                    .expect("every result pairs with its intent");
                replay.push(ReplayReceipt {
                    principal: "person:test".into(),
                    run_id: None,
                    task_id: None,
                    agent_id: None,
                    tool_id: Some(call.tool_id.clone()),
                    definition_revision: call.definition_revision,
                    input_digest: floe_agent_contract::input_digest(&call.input),
                    invocation_key: call.invocation_key,
                    call_id: call.call_id,
                    result: result.text.clone(),
                    task_result: None,
                    task_state: None,
                    task_artifacts: vec![],
                    task_coverage: DependencyCoverage::Unknown,
                    task_issue: None,
                    tool_artifacts: result.artifacts.clone(),
                    tool_coverage: result.coverage.clone(),
                    tool_issue: result.issue.as_ref().map(|issue| issue.failure),
                });
                exchanges.push(ModelConversationEntry::ToolExchange {
                    call,
                    result: result.clone(),
                });
            }
        }
        assert_eq!(exchanges.len(), 2);
        assert!(
            run_events.iter().any(|event| matches!(
                event,
                JournalEvent::BatchProgress { cursor }
                    if cursor.batch_id == batch.batch_id && cursor.next_step_index == 1
            )),
            "cursor past the soft step is durable: {run_events:?}"
        );

        // Run 2 resumes past the soft step: no redispatch, and the answering
        // model still sees the soft observation.
        struct ObservingAnswer {
            saw_soft: Arc<AtomicUsize>,
            saw_real: Arc<AtomicUsize>,
        }
        impl ModelPort for ObservingAnswer {
            fn generate<'a>(
                &'a self,
                request: ModelRequest,
                _: &'a ExecutionScope,
            ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>>
            {
                let conversation = request.projection.envelope.conversation.clone();
                let saw_soft = Arc::clone(&self.saw_soft);
                let saw_real = Arc::clone(&self.saw_real);
                Box::pin(async move {
                    for entry in &conversation.current_turn {
                        if let ModelConversationEntry::ToolExchange { result, .. } = entry {
                            if result.text.contains("not registered") {
                                saw_soft.fetch_add(1, Ordering::SeqCst);
                            }
                            if result.text == "observed" {
                                saw_real.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    }
                    Ok(ModelResponse {
                        attempt_id: request.attempt_id,
                        steps: vec![answer("done")],
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    })
                })
            }
        }
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let model = ObservingAnswer {
            saw_soft: Arc::new(AtomicUsize::new(0)),
            saw_real: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, resumed_events) = RecordingJournal::new();
        let (projection, _) = Projector::new();
        let mut engine_request = request(scope());
        engine_request.conversation.current_turn.extend(exchanges);
        engine_request.replay = replay;
        engine_request.resume = Some(EngineResumeState {
            validated_batch: batch.clone(),
            cursor: BatchCursor {
                batch_id: batch.batch_id,
                next_step_index: 1,
            },
        });
        let report = Engine::default()
            .drive(
                engine_request,
                ports(&projection, &model, &tools, &journal, &Validator),
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(tools.calls.load(Ordering::SeqCst), 0);
        // The soft observation arrives exactly once, carried past the crash;
        // the re-executed real step re-pushes its own, as resumed steps do.
        assert_eq!(model.saw_soft.load(Ordering::SeqCst), 1);
        assert_eq!(model.saw_real.load(Ordering::SeqCst), 2);
        let resumed_events = resumed_events.lock().unwrap();
        let intents = resumed_events
            .iter()
            .filter_map(|event| match event {
                JournalEvent::ToolIntent { call } => Some(call.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(intents.len(), 1);
        assert_eq!(
            intents[0].call_id,
            stable_call_id(batch.execution_id, batch.batch_id, 1)
        );
    }

    fn agent_card(id: &str) -> AgentCard {
        AgentCard {
            schema_version: floe_agent_contract::AGENT_SCHEMA_VERSION,
            protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
            id: id.into(),
            version: "1".into(),
            name: id.into(),
            description: "fixture expert".into(),
            supported_placements: vec![floe_agent_contract::ModelPlacement::DeviceLocal],
            domain_tags: vec![],
            skills: vec![],
        }
    }

    #[tokio::test]
    async fn stale_agent_never_calls_delegation_port() {
        let delegations = CountingDelegations {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, events) = RecordingJournal::new();
        let (projection, _) = Projector::new();
        // The catalog carries expert-a at revision 2; the batch pins 1.
        let mut engine_request = request(scope());
        engine_request.allowed_catalog.cards = vec![AgentDefinition {
            card: agent_card("expert-a"),
            definition_revision: 2,
        }];
        // A stale delegation still completes its iteration; the following
        // answer ends the run.
        struct AnswerNext;
        impl ModelPort for AnswerNext {
            fn generate<'a>(
                &'a self,
                request: ModelRequest,
                _: &'a ExecutionScope,
            ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>>
            {
                let delegate = request
                    .projection
                    .envelope
                    .conversation
                    .current_turn
                    .iter()
                    .any(|entry| {
                        matches!(entry, ModelConversationEntry::DelegationExchange { .. })
                    });
                let steps = if delegate {
                    vec![answer("done")]
                } else {
                    vec![delegation()]
                };
                Box::pin(async move {
                    Ok(ModelResponse {
                        attempt_id: request.attempt_id,
                        steps,
                        usage: ModelUsage {
                            tokens: 1,
                            cost_micros: 1,
                        },
                    })
                })
            }
        }
        let report = Engine::default()
            .drive(
                engine_request,
                EnginePorts {
                    projection: &projection,
                    model: &AnswerNext,
                    tools: &tools,
                    delegation: &delegations,
                    journal: &journal,
                    validator: &Validator,
                },
            )
            .await
            .unwrap();
        assert_eq!(report.output.as_deref(), Some("done"));
        assert_eq!(delegations.calls.load(Ordering::SeqCst), 0);
        // The soft rejection is a durable intent/result pair under the
        // step's stable identity, carrying the exact request linkage.
        let events = events.lock().unwrap();
        let batch = events
            .iter()
            .find_map(|event| match event {
                JournalEvent::ValidatedBatch { batch } => Some(batch.clone()),
                _ => None,
            })
            .expect("soft step still validates its batch");
        let (delegated, receipt) = events
            .iter()
            .find_map(|event| match event {
                JournalEvent::DelegationResult { receipt } => Some(receipt.clone()),
                _ => None,
            })
            .and_then(|receipt| {
                events
                    .iter()
                    .find_map(|event| match event {
                        JournalEvent::DelegationIntent { request }
                            if request.task_id == receipt.task_id =>
                        {
                            Some((request.clone(), receipt.clone()))
                        }
                        _ => None,
                    })
            })
            .expect("soft step journals an intent/result pair");
        assert_eq!(delegated.selected_agent_id, "expert-a");
        assert_eq!(delegated.selected_definition_revision, 1);
        assert_eq!(
            delegated.task_id,
            stable_task_id(batch.execution_id, batch.batch_id, 0)
        );
        assert_eq!(receipt.snapshot.state, floe_agent_contract::TaskState::Rejected);
        assert_eq!(receipt.snapshot.result, None);
        assert_eq!(
            receipt.snapshot.issue,
            Some(AgentFailure::InvalidModelOutput)
        );
        assert_eq!(
            receipt.snapshot.coverage,
            DependencyCoverage::Independent
        );
        assert!(
            events.iter().any(|event| matches!(
                event,
                JournalEvent::BatchProgress { cursor }
                    if cursor.batch_id == batch.batch_id && cursor.next_step_index == 1
            )),
            "soft step advances the cursor: {events:?}"
        );
    }

    struct DispatchedFailureModel;
    impl ModelPort for DispatchedFailureModel {
        fn generate<'a>(
            &'a self,
            _: ModelRequest,
            scope: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
            Box::pin(async move {
                // Simulate the transitional bridge past its dispatch fence:
                // reserve once, mark dispatched, then fail without settling so
                // the ledger charges the unknown estimate in-memory.
                let mut tokens = 40;
                let mut cost_micros = 40;
                let mut attempt = scope
                    .budget()
                    .begin(&mut tokens, &mut cost_micros)
                    .expect("attempt budget");
                attempt.mark_dispatched();
                Err(AgentFailure::ModelUnavailable)
            })
        }
    }

    struct UndispatchedFailureModel;
    impl ModelPort for UndispatchedFailureModel {
        fn generate<'a>(
            &'a self,
            _: ModelRequest,
            _: &'a ExecutionScope,
        ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
            // Preflight failure before any provider handoff: never touches
            // the scope budget, so no unknown charge may be journaled.
            Box::pin(async { Err(AgentFailure::Cancelled) })
        }
    }

    fn continuation_scope(
        ledger: &BudgetLedger,
    ) -> ExecutionScope {
        ExecutionScope::root(
            Cancellation::new(),
            tokio::time::Instant::now() + Duration::from_secs(5),
            ledger.work_lease(),
            floe_agent_contract::TraceContext::new(Uuid::new_v4()),
        )
    }

    #[tokio::test]
    async fn failed_dispatched_model_usage_survives_continuation() {
        let ledger = BudgetLedger::new(BudgetConfig::new(100, 100), Default::default());
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, events) = RecordingJournal::new();
        let (projection, _) = Projector::new();
        let result = Engine::default()
            .drive(
                request(continuation_scope(&ledger)),
                ports(
                    &projection,
                    &DispatchedFailureModel,
                    &tools,
                    &journal,
                    &Validator,
                ),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::ModelUnavailable)));
        let usage = events
            .lock()
            .unwrap()
            .iter()
            .find_map(|event| match event {
                JournalEvent::ModelResult { usage, .. } => Some(*usage),
                _ => None,
            })
            .expect("failed attempt journals a result");
        // The durable fallback carries the bridge's unknown estimate in both
        // dimensions so a restart cannot resurrect tokens or cost.
        assert_eq!(usage.tokens, 40);
        assert_eq!(usage.cost_micros, 40);
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.unknown_tokens, 40);
        assert_eq!(snapshot.unknown_cost_micros, 40);
        // Continuation seeds its root ledger from the journaled usage: the
        // prior charge stays visible and the next run gets at most the rest.
        let prior = floe_execution::budget::ModelUsage {
            attempts: 1,
            tokens: usage.tokens,
            cost_micros: usage.cost_micros,
            estimated_tokens: 0,
        };
        assert!(prior.tokens >= 40);
        let next = BudgetLedger::new(BudgetConfig::new(100, 100), prior);
        let mut tokens = 100;
        let mut cost_micros = 100;
        next.work_lease()
            .begin(&mut tokens, &mut cost_micros)
            .unwrap();
        assert!(tokens <= 60);
        assert!(cost_micros <= 60);
    }

    #[tokio::test]
    async fn undispatched_model_failure_does_not_charge_unknown_usage() {
        let ledger = BudgetLedger::new(BudgetConfig::new(100, 100), Default::default());
        let tools = Tools {
            calls: Arc::new(AtomicUsize::new(0)),
        };
        let (journal, events) = RecordingJournal::new();
        let (projection, _) = Projector::new();
        let result = Engine::default()
            .drive(
                request(continuation_scope(&ledger)),
                ports(
                    &projection,
                    &UndispatchedFailureModel,
                    &tools,
                    &journal,
                    &Validator,
                ),
            )
            .await;
        assert!(matches!(result, Err(AgentFailure::Cancelled)));
        let usage = events
            .lock()
            .unwrap()
            .iter()
            .find_map(|event| match event {
                JournalEvent::ModelResult { usage, .. } => Some(*usage),
                _ => None,
            })
            .expect("failed attempt journals a result");
        assert_eq!(usage, ModelUsage::default());
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.unknown_tokens, 0);
        assert_eq!(snapshot.unknown_cost_micros, 0);
        assert_eq!(snapshot.usage.tokens, 0);
    }
}
