use std::collections::{HashMap, HashSet};

use floe_agent_contract::{
    AgentFailure, AllowedCatalog, AuthorizedModelProjection, BatchCursor, DelegationPort,
    DelegationRequest, DependencyCoverage, EngineRequest, EngineStep, ExecutionJournal,
    InvocationKey, JournalAck, JournalEvent, MODEL_CORRECTION_TEXT, ModelCapabilities,
    ModelConversation, ModelConversationEntry, ModelCorrection, ModelPlanRequest, ModelPort,
    ModelProjectionOutcome, ModelProjectionPort, ModelProjectionRequest, ModelRequest,
    ModelResponse, ModelStep, ModelUsage, PinnedAgentRevision, PinnedToolRevision, ReplayReceipt,
    SourceProjectionReview, TaskId, TaskReceipt, ToolCall, ToolInvocationOutcome, ToolPort, ToolResult,
    ValidatedModelBatch,
};
use uuid::Uuid;

pub use floe_agent_contract::ValidatedFinalPayload;

pub trait FinalPayloadValidator: Sync {
    fn validate(
        &self,
        role: &str,
        text: &str,
        artifacts: &[floe_agent_contract::Artifact],
    ) -> Result<ValidatedFinalPayload, AgentFailure>;
}

struct ContractValidator;
impl FinalPayloadValidator for ContractValidator {
    fn validate(
        &self,
        _: &str,
        text: &str,
        artifacts: &[floe_agent_contract::Artifact],
    ) -> Result<ValidatedFinalPayload, AgentFailure> {
        if text.trim().is_empty() || text.len() > floe_agent_contract::MAX_OUTPUT_BYTES {
            return Err(AgentFailure::InvalidModelOutput);
        }
        artifacts.iter().try_for_each(|artifact| {
            artifact
                .coverage
                .validate()
                .map_err(|_| AgentFailure::InvalidModelOutput)
        })?;
        Ok(ValidatedFinalPayload { text: text.to_owned(), artifacts: artifacts.to_owned() })
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
    pub max_task_tokens: u64,
    pub max_task_cost_micros: u64,
    pub max_tool_calls: u32,
    pub max_delegations: u32,
}
impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_attempt_tokens: 4_096,
            max_attempt_cost_micros: 1_000_000,
            max_task_tokens: 16_384,
            max_task_cost_micros: 1_000_000,
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

/// Source review happens before model intent and carries only settled work.
#[derive(Clone, Debug)]
pub enum EngineOutcome {
    Completed(EngineReport),
    Blocked(EngineBlock),
}

#[derive(Clone, Debug)]
pub struct EngineBlock {
    pub blockage: EngineBlockage,
    pub report: EngineReport,
}

#[derive(Clone, Debug)]
pub enum EngineBlockage {
    ModelProjection { plan: floe_agent_contract::PreparedModelPlan, review: SourceProjectionReview },
    SourceRead { call_id: Uuid, blockers: floe_agent_contract::SourceAccessBlockers },
    Delegation { receipt: TaskReceipt },
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
    ) -> Result<EngineOutcome, AgentFailure> {
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
    ) -> Result<EngineOutcome, AgentFailure> {
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
    async fn run(self) -> Result<EngineOutcome, AgentFailure> {
        let mut drive = ActiveDrive {
            config: self.config,
            execution_id: self.request.execution_id,
            request: self.request,
            ports: self.ports,
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
            match drive.validated_batch().await? {
                BatchOutcome::Ready((batch, corrections)) => {
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
                }
                BatchOutcome::Blocked(review) => {
                    return Ok(EngineOutcome::Blocked(review));
                }
            }
            drive.completed_iterations += 1;
            drive
                .checkpoint(JournalEvent::Checkpoint {
                    iteration: drive.completed_iterations,
                })
                .await?;
        }
        Ok(EngineOutcome::Completed(EngineReport {
            steps: drive.steps,
            output: None,
            answering_projection_coverage: None,
            iterations: drive.completed_iterations,
            attempt_ids: drive.attempts,
            execution_id: drive.execution_id,
        }))
    }
}

enum BatchOutcome {
    Ready((ValidatedModelBatch, Vec<Option<String>>)),
    Blocked(EngineBlock),
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
        self.request
            .scope
            .run(self.ports.journal.checkpoint(event))
            .await
    }

    async fn record_model_result(
        &self,
        attempt_id: Uuid,
        receipt: Option<floe_execution::budget::ModelAttemptReceipt>,
    ) -> Result<(), AgentFailure> {
        let (usage, accounting) = receipt.map_or_else(
            || {
                (
                    ModelUsage::default(),
                    floe_execution::budget::ModelAccounting::default(),
                )
            },
            |receipt| {
                (
                    ModelUsage {
                        tokens: receipt.charged_tokens,
                        cost_micros: receipt.charged_cost_micros,
                    },
                    receipt.accounting,
                )
            },
        );
        // Accounting acknowledgment must remain possible after model cancellation.
        // Failure keeps the intent and its receipt for conservative recovery.
        let acknowledgment = self
            .ports
            .journal
            .record_result(JournalEvent::ModelResult {
                attempt_id,
                usage,
                accounting,
            })
            .await?;
        if !matches!(acknowledgment, JournalAck::Accepted { .. }) {
            return Err(AgentFailure::Conflict);
        }
        if receipt.is_some() {
            self.request
                .scope
                .budget()
                .acknowledge_model_attempt(attempt_id)?;
        }
        Ok(())
    }

    /// Select once, then project and dispatch against that immutable object.
    /// A correction has fresh attempt/projection identities and source fences.
    async fn validated_batch(&mut self) -> Result<BatchOutcome, AgentFailure> {
        let plan_request = ModelPlanRequest {
            principal: self.request.principal.clone(),
            device_id: self.request.device_id.clone(),
            purpose: self.request.purpose.clone(),
            consumer: self.request.consumer.clone(),
            required_capabilities: ModelCapabilities::chat(),
        };
        plan_request.validate()?;
        let prepared = self
            .request
            .scope
            .run(
                self.ports
                    .model
                    .prepare(plan_request.clone(), &self.request.scope),
            )
            .await?;
        let plan = prepared.plan().clone();
        plan.validate()?;
        if plan.principal != plan_request.principal
            || plan.device_id != plan_request.device_id
            || plan.purpose != plan_request.purpose
            || plan.consumer != plan_request.consumer
            || !plan
                .capabilities
                .includes(&plan_request.required_capabilities)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut correction: Option<ModelCorrection> = None;
        loop {
            let projection_operation_id = Uuid::new_v4();
            let projection_request = ModelProjectionRequest {
                principal: self.request.principal.clone(),
                projection_operation_id,
                plan: plan.clone(),
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
            let projection = match projection {
                ModelProjectionOutcome::Ready(projection) => projection,
                ModelProjectionOutcome::NeedsSourceReview(review) => {
                    review.validate()?;
                    if review.projection_operation_id != projection_operation_id {
                        return Err(AgentFailure::PolicyDenied);
                    }
                    return Ok(BatchOutcome::Blocked(EngineBlock {
                        blockage: EngineBlockage::ModelProjection { plan: plan.clone(), review },
                        report: EngineReport {
                            steps: self.steps.clone(),
                            output: None,
                            answering_projection_coverage: None,
                            iterations: self.completed_iterations,
                            attempt_ids: self.attempts.clone(),
                            execution_id: self.execution_id,
                        },
                    }));
                }
            };
            projection.validate()?;
            if projection.projection_operation_id != projection_operation_id
                || projection.plan_id != plan.operation_id
                || projection.binding_digest != plan.binding_digest
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let attempt_id = Uuid::new_v4();
            let model_projection = projection.clone();
            let model_scope = self.request.scope.child_scope(
                self.request.scope.deadline(),
                self.config.max_attempt_tokens.max(1),
                self.config.max_attempt_cost_micros.max(1),
                None,
            );
            let reservation_ceiling =
                floe_execution::budget::ModelReservationCeiling::for_lease(model_scope.budget());
            let model_request = ModelRequest {
                attempt_id,
                reservation_ceiling,
                principal: self.request.principal.clone(),
                device_id: self.request.device_id.clone(),
                projection,
                catalog: self.request.allowed_catalog.clone(),
                purpose: self.request.purpose.clone(),
                consumer: self.request.consumer.clone(),
                replay: self.model_replay.clone(),
            };
            model_request.validate()?;
            let intent = self
                .request
                .scope
                .run(self.ports.journal.record_intent(JournalEvent::ModelIntent {
                    parent_task_id: self.request.scope.task_id(),
                    reservation_ceiling,
                    attempt_id,
                    projection_ref: model_projection.projection_ref,
                    plan: plan.clone(),
                }))
                .await?;
            if let JournalAck::Replayed(receipt) = intent {
                return Err(if receipt.tool_id.is_some() || receipt.agent_id.is_some() {
                    AgentFailure::InvalidInput
                } else {
                    AgentFailure::Conflict
                });
            }
            let response = model_scope
                .run(prepared.generate(model_request, &model_scope))
                .await;
            // The bounded future has returned or dropped; terminal accounting
            // is now immutable, including conservative dispatch-uncertain charges.
            let receipt = model_scope.budget().model_attempt_receipt(attempt_id);
            if receipt.is_none() && model_scope.budget().model_attempt_admitted(attempt_id) {
                return Err(AgentFailure::StorageUnavailable);
            }
            match response {
                Err(failure) => {
                    self.record_model_result(attempt_id, receipt).await?;
                    self.attempts.push(attempt_id);
                    // A failed transport/envelope may be dispatch-uncertain.
                    // Never reissue it to recover output or usage.
                    return Err(failure);
                }
                Ok(response) => {
                    let receipt = receipt.ok_or(AgentFailure::StorageUnavailable)?;
                    self.record_model_result(attempt_id, Some(receipt)).await?;
                    self.attempts.push(attempt_id);
                    if response.usage.tokens != receipt.charged_tokens
                        || response.usage.cost_micros != receipt.charged_cost_micros
                        || response.accounting != receipt.accounting
                    {
                        return Err(AgentFailure::InvalidModelOutput);
                    }
                    match self.validated_response(attempt_id, &model_projection, &response) {
                        Ok(validated) => return Ok(BatchOutcome::Ready(validated)),
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
        let encoded_steps =
            serde_json::to_vec(&response.steps).map_err(|_| AgentFailure::InvalidModelOutput)?;
        if encoded_steps.len() > self.request.max_output_bytes {
            return Err(AgentFailure::BudgetExceeded);
        }
        validate_batch_shape(&response.steps)?;
        let corrections = validate_model_steps(&response.steps, &self.request.allowed_catalog)?;
        let mut validated_steps = response.steps.clone();
        for step in &mut validated_steps {
            if let ModelStep::Answer { text, artifacts } = step {
                let payload = self.ports
                    .validator
                    .validate(&self.request.role_spec.role_id, text, artifacts)?;
                *text = payload.text;
                *artifacts = payload.artifacts;
                if text.trim().is_empty() || text.len() > self.request.max_output_bytes {
                    return Err(AgentFailure::InvalidModelOutput);
                }
                if artifacts
                    .iter()
                    .any(|artifact| artifact.validate(self.request.max_output_bytes).is_err()
                        || match (&projection.coverage, &artifact.coverage) {
                            (_, DependencyCoverage::Independent) => false,
                            (DependencyCoverage::Dependent { dependencies: admitted },
                                DependencyCoverage::Dependent { dependencies: claimed }) =>
                                claimed.iter().any(|dependency| !admitted.contains(dependency)),
                            _ => true,
                        })
                {
                    return Err(AgentFailure::InvalidModelOutput);
                }
            }
        }
        if serde_json::to_vec(&validated_steps).map_err(|_| AgentFailure::InvalidModelOutput)?.len()
            > self.request.max_output_bytes
        { return Err(AgentFailure::BudgetExceeded); }
        let (tool_revisions, agent_revisions) =
            pin_revisions(&response.steps, &self.request.allowed_catalog);
        // A delegating batch binds the exact execution context before anything
        // is dispatched; batches without a Delegate step persist none.
        let delegation_context = if response
            .steps
            .iter()
            .any(|step| matches!(step, ModelStep::Delegate { .. }))
        {
            Some(
                self.request
                    .delegation_context
                    .clone()
                    .ok_or(AgentFailure::InvalidInput)?
                    .bind_projection(projection.coverage.clone())?,
            )
        } else {
            None
        };
        Ok((
            ValidatedModelBatch {
                execution_id: self.execution_id,
                attempt_id,
                projection_ref: projection.projection_ref,
                batch_id: Uuid::new_v4(),
                steps: validated_steps,
                catalog_revision: self.request.allowed_catalog.revision,
                tool_revisions,
                agent_revisions,
                projection_coverage: projection.coverage.clone(),
                delegation_context,
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
    ) -> Result<Option<EngineOutcome>, AgentFailure> {
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
                        message_id: stable_preamble_id(batch.execution_id, batch.batch_id, ordinal),
                        text: text.clone(),
                    })?;
                }
                ModelStep::Answer { text, artifacts } => {
                    // Canonical validation is authoritative; this only guards
                    // against a trailing step being silently ignored.
                    debug_assert_eq!(ordinal as usize + 1, batch.steps.len());
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
                    return Ok(Some(EngineOutcome::Completed(self.report(
                        Some(text.clone()),
                        Some(batch.projection_coverage.clone()),
                    ))));
                }
                ModelStep::CallTool {
                    tool_id,
                    definition_revision,
                    input,
                } => {
                    if let Some(blockage) = self.execute_tool(
                        batch,
                        ordinal,
                        corrections.get(step_index).and_then(Option::as_ref),
                        tool_id,
                        *definition_revision,
                        input,
                    )
                    .await? {
                        return Ok(Some(EngineOutcome::Blocked(EngineBlock {
                            blockage, report: self.report(None, None),
                        })));
                    }
                }
                ModelStep::Delegate {
                    agent_id,
                    definition_revision,
                    message,
                    context_refs,
                } => {
                    if let Some(blockage) = self.execute_delegation(
                        batch,
                        ordinal,
                        agent_id,
                        *definition_revision,
                        message,
                        context_refs,
                    )
                    .await? {
                        return Ok(Some(EngineOutcome::Blocked(EngineBlock {
                            blockage, report: self.report(None, None),
                        })));
                    }
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
    ) -> Result<Option<EngineBlockage>, AgentFailure> {
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
        let invocation_key = stable_invocation_key(
            self.execution_id,
            batch.batch_id,
            ordinal,
            InvocationKind::Tool,
        );
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
                issue: receipt
                    .tool_issue
                    .map(|failure| floe_agent_contract::OutcomeIssue {
                        failure,
                        retryable: false,
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
                issue: receipt
                    .tool_issue
                    .map(|failure| floe_agent_contract::OutcomeIssue {
                        failure,
                        retryable: false,
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
                Ok(ToolInvocationOutcome::NeedsSourceReview { call_id, blockers }) => {
                    if call_id != call.call_id { return Err(AgentFailure::PolicyDenied); }
                    blockers.validate().map_err(|_| AgentFailure::InvalidInput)?;
                    let ack = self.ports.journal.record_result(JournalEvent::ToolReviewRequired {
                        call_id, blockers: blockers.clone(),
                    }).await?;
                    if !matches!(ack, JournalAck::Accepted { .. }) { return Err(AgentFailure::Conflict); }
                    return Ok(Some(EngineBlockage::SourceRead { call_id, blockers }));
                }
                Ok(ToolInvocationOutcome::Completed(result)) => {
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
            .run(self.ports.journal.record_result(JournalEvent::ToolResult {
                result: result.clone(),
            }))
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
        Ok(None)
    }

    async fn execute_delegation(
        &mut self,
        batch: &ValidatedModelBatch,
        ordinal: u32,
        agent_id: &str,
        definition_revision: u64,
        message: &str,
        context_refs: &[String],
    ) -> Result<Option<EngineBlockage>, AgentFailure> {
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
        // The batch-bound context is authoritative: resumed execution reuses
        // the stored binding even when the resuming host state differs, and a
        // delegating batch without one is corrupt durable state.
        let execution_context = batch
            .delegation_context
            .clone()
            .ok_or(AgentFailure::StorageUnavailable)?;
        let delegation = DelegationRequest {
            task_id: stable_task_id(self.execution_id, batch.batch_id, ordinal),
            parent_run_id: self.request.scope.root_run_id().map(|id| id.as_uuid()),
            principal: self.request.principal.clone(),
            invocation_key,
            selected_agent_id: agent_id.to_owned(),
            selected_definition_revision: definition_revision,
            message: message.to_owned(),
            context_refs: context_refs.to_owned(),
            execution_context,
        };
        if !self.seen_invocations.insert(delegation.invocation_key) {
            return Err(AgentFailure::Conflict);
        }
        let intent = self
            .request
            .scope
            .run(
                self.ports
                    .journal
                    .record_intent(JournalEvent::DelegationIntent {
                        request: delegation.clone(),
                    }),
            )
            .await?;
        let child = self.request.scope.child_scope(
            self.request.scope.deadline(),
            self.config.max_task_tokens.max(1),
            self.config.max_task_cost_micros.max(1),
            Some(delegation.task_id),
        );
        let receipt = if let JournalAck::Replayed(replay_receipt) = intent {
            replay_task(&delegation, &replay_receipt, self.request.max_output_bytes)?
        } else if let Some(replay_receipt) = find_task_replay(&self.model_replay, &delegation) {
            replay_resumed_task(&delegation, &replay_receipt, self.request.max_output_bytes)?
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
                    blockage: None,
                },
                replay: None,
                execution: floe_agent_contract::TaskExecutionEvidence::Unadmitted,
            }
        } else {
            let delegated = self.ports.delegation.delegate(delegation.clone(), &child);
            let receipt = child.run(delegated).await?;
            verify_receipt(&delegation, &receipt, self.request.max_output_bytes)?;
            if let Some(replayed) = receipt.replay.clone() {
                self.model_replay.push(replayed);
            }
            receipt
        };
        self.request
            .scope
            .run(
                self.ports
                    .journal
                    .record_result(JournalEvent::DelegationResult {
                        receipt: Box::new(receipt.clone()),
                    }),
            )
            .await?;
        if receipt.snapshot.state == floe_agent_contract::TaskState::Blocked {
            self.steps.push(EngineStep::Delegation(Box::new(receipt.clone())));
            return Ok(Some(EngineBlockage::Delegation { receipt }));
        }
        self.checkpoint(JournalEvent::BatchProgress {
            cursor: BatchCursor {
                batch_id: batch.batch_id,
                next_step_index: ordinal + 1,
            },
        })
        .await?;
        let mut history_request = delegation;
        history_request.parent_run_id = receipt.snapshot.parent_run_id;
        self.push_current(ModelConversationEntry::DelegationExchange {
            request: history_request,
            receipt: receipt.clone(),
        })?;
        self.steps.push(EngineStep::Delegation(Box::new(receipt)));
        Ok(None)
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
                }) && !tools
                    .iter()
                    .any(|pinned: &PinnedToolRevision| pinned.tool_id == *tool_id)
                {
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
                }) && !agents
                    .iter()
                    .any(|pinned: &PinnedAgentRevision| pinned.agent_id == *agent_id)
                {
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
        task_execution: None,
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
    restored_task_receipt(request, receipt, maximum_bytes)
}

/// Cross-run result replay: same invocation identity and input, but the linkage
/// (run, parent) belongs to the run that recorded it, so linkage is not
/// compared here. The original Task receipt remains immutable.
fn replay_resumed_task(
    request: &DelegationRequest,
    receipt: &ReplayReceipt,
    maximum_bytes: usize,
) -> Result<TaskReceipt, AgentFailure> {
    verify_resumed_task_replay(request, receipt)?;
    restored_task_receipt(request, receipt, maximum_bytes)
}

fn restored_task_receipt(request: &DelegationRequest, replay: &ReplayReceipt, maximum_bytes: usize)
    -> Result<TaskReceipt, AgentFailure>
{
    let (snapshot, execution) = match &replay.task_execution {
        Some(execution) => {
            execution.validate(maximum_bytes)?;
            (execution.snapshot.clone(), floe_agent_contract::TaskExecutionEvidence::Admitted(execution.clone()))
        }
        None => (
            floe_agent_contract::TaskSnapshot {
                task_id: request.task_id,
                parent_run_id: replay.run_id.map(|id| id.as_uuid()),
                principal: request.principal.clone(),
                agent_id: request.selected_agent_id.clone(),
                definition_revision: request.selected_definition_revision,
                state: replay.task_state.ok_or(AgentFailure::StorageUnavailable)?,
                result: replay.task_result.clone(), artifacts: replay.task_artifacts.clone(),
                coverage: replay.task_coverage.clone(), issue: replay.task_issue, blockage: None,
            },
            floe_agent_contract::TaskExecutionEvidence::Unadmitted,
        ),
    };
    if snapshot.task_id != request.task_id || snapshot.principal != request.principal
        || snapshot.agent_id != request.selected_agent_id
        || snapshot.definition_revision != request.selected_definition_revision
        || snapshot.result != replay.task_result || Some(snapshot.state) != replay.task_state
        || snapshot.artifacts != replay.task_artifacts || snapshot.coverage != replay.task_coverage
        || snapshot.issue != replay.task_issue
    { return Err(AgentFailure::Conflict); }
    let task = TaskReceipt {
        task_id: request.task_id, snapshot, replay: Some(replay.clone()),
        execution,
    };
    task.validate(maximum_bytes)?;
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
    receipt.validate(maximum_bytes)?;
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
        && receipt.task_execution.is_none()
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
        && receipt.task_execution.is_none()
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
        && receipt.input_digest == floe_agent_contract::delegation_request_digest(request))
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
                floe_agent_contract::TaskState::Submitted | floe_agent_contract::TaskState::Working
            )
        )
        && receipt.tool_artifacts.is_empty()
        && receipt.tool_issue.is_none()
        && receipt
            .task_artifacts
            .iter()
            .all(|artifact| artifact.coverage.validate().is_ok())
        && receipt.task_coverage.validate().is_ok()
        && receipt.input_digest == {
            let mut original = request.clone();
            original.parent_run_id = receipt.task_execution.as_ref()
                .map_or(receipt.run_id.map(|id| id.as_uuid()), |execution| execution.snapshot.parent_run_id);
            floe_agent_contract::delegation_request_digest(&original)
        })
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
