//! The sole Expert model loop. Packages supply judgment; Context supplies admitted reads.
use std::sync::{Arc, Mutex};
use floe_agent_contract::{AgentEndpoint, AgentFailure, AllowedCatalog, BoxFuture, DelegationPort,
    DelegationRequest, DependencyCoverage, EndpointInvocation, EndpointSettlement, EngineRequest,
    ExpertBlockReport, ExpertExecutionOutcome, ExpertReport, ModelConversation, ModelConversationEntry,
    ModelPort, ModelProjectionOutcome, ModelProjectionPort, ModelProjectionRequest, OwnerActor,
    RoleSpec, TaskBlockage, TaskReceipt, ToolCall, ToolDescriptor, ToolInvocationOutcome, ToolPort, ToolResult};
use floe_agent_runtime::{Engine, EngineBlockage, EngineOutcome, EnginePorts, FinalPayloadValidator, ValidatedFinalPayload};
use floe_context_contract::SourceReadOutcome;
use floe_execution::ExecutionScope;
use crate::{ExpertAdmissionIdentity, ExpertClock, ExpertExecutionSelection, ExpertManifest,
    ExpertProgram, ExpertProgramRequest, ExpertProgramSpec, ExpertProjectionPort,
    ExpertProjectionRequest, ExpertSourcePort, ExpertSourceRequest,
    ExpertToolObservation, RegistryRepository};

pub struct EngineExpertEndpoint {
    actor: OwnerActor,
    admission: ExpertAdmissionIdentity,
    selection: ExpertExecutionSelection,
    manifest: ExpertManifest,
    program: Arc<dyn ExpertProgram>,
    registry: Arc<dyn RegistryRepository>,
    model: Arc<dyn ModelPort>,
    sources: Arc<dyn ExpertSourcePort>,
    projection: Arc<dyn ExpertProjectionPort>,
    clock: Arc<dyn ExpertClock>,
}

impl EngineExpertEndpoint {
    #[allow(clippy::too_many_arguments)]
    pub fn new(actor: OwnerActor, admission: ExpertAdmissionIdentity, selection: ExpertExecutionSelection,
        manifest: ExpertManifest, program: Arc<dyn ExpertProgram>, registry: Arc<dyn RegistryRepository>,
        model: Arc<dyn ModelPort>, sources: Arc<dyn ExpertSourcePort>,
        projection: Arc<dyn ExpertProjectionPort>, clock: Arc<dyn ExpertClock>) -> Result<Self, AgentFailure>
    {
        actor.validate()?;
        manifest.validate()?;
        admission.validate(&manifest.definition)?;
        selection.validate()?;
        Ok(Self { actor, admission, selection, manifest, program, registry, model, sources, projection, clock })
    }
}

struct CapturedRead { observation: ExpertToolObservation }
#[derive(Default)]
struct TaskEvidence {
    reads: Mutex<Vec<CapturedRead>>,
    projection: Mutex<Option<floe_agent_contract::AuthorizedModelProjection>>,
    acknowledged_coverage: Mutex<Option<DependencyCoverage>>,
}
impl TaskEvidence {
    fn observations(&self) -> Result<Vec<ExpertToolObservation>, AgentFailure> {
        Ok(self.reads.lock().map_err(|_| AgentFailure::StorageUnavailable)?
            .iter().map(|read| read.observation.clone()).collect())
    }
    fn coverage(&self) -> Result<DependencyCoverage, AgentFailure> {
        Ok(self.acknowledged_coverage.lock().map_err(|_| AgentFailure::StorageUnavailable)?
            .clone().unwrap_or(DependencyCoverage::Independent))
    }
    fn acknowledge(&self, event: &floe_agent_contract::JournalEvent) -> Result<(), AgentFailure> {
        let observed = match event {
            floe_agent_contract::JournalEvent::ValidatedBatch { batch } => &batch.projection_coverage,
            floe_agent_contract::JournalEvent::ToolResult { result } => &result.coverage,
            _ => return Ok(()),
        };
        let mut coverage = self.acknowledged_coverage.lock().map_err(|_| AgentFailure::StorageUnavailable)?;
        *coverage = Some(coverage.as_ref().unwrap_or(&DependencyCoverage::Independent).merge(observed)?);
        Ok(())
    }
}

// This forwards to the sole durable Task journal and projects only acknowledged
// coverage. Captured-but-unacknowledged reads cannot become terminal evidence.
struct ExpertJournal<'a> {
    inner: &'a dyn floe_agent_contract::ExecutionJournal,
    evidence: &'a TaskEvidence,
}
impl floe_agent_contract::ExecutionJournal for ExpertJournal<'_> {
    fn record_intent<'a>(&'a self, event: floe_agent_contract::JournalEvent)
        -> BoxFuture<'a, Result<floe_agent_contract::JournalAck, AgentFailure>>
    { Box::pin(async move {
        let ack = self.inner.record_intent(event.clone()).await?;
        self.evidence.acknowledge(&event)?;
        Ok(ack)
    }) }
    fn record_result<'a>(&'a self, event: floe_agent_contract::JournalEvent)
        -> BoxFuture<'a, Result<floe_agent_contract::JournalAck, AgentFailure>>
    { Box::pin(async move {
        let ack = self.inner.record_result(event.clone()).await?;
        self.evidence.acknowledge(&event)?;
        Ok(ack)
    }) }
    fn record_output<'a>(&'a self, event: floe_agent_contract::JournalEvent)
        -> BoxFuture<'a, Result<floe_agent_contract::JournalAck, AgentFailure>>
    { Box::pin(async move {
        let ack = self.inner.record_output(event.clone()).await?;
        self.evidence.acknowledge(&event)?;
        Ok(ack)
    }) }
    fn checkpoint<'a>(&'a self, event: floe_agent_contract::JournalEvent)
        -> BoxFuture<'a, Result<floe_agent_contract::JournalAck, AgentFailure>>
    { Box::pin(async move {
        let ack = self.inner.checkpoint(event.clone()).await?;
        self.evidence.acknowledge(&event)?;
        Ok(ack)
    }) }
}

struct ExpertTools<'a> {
    endpoint: &'a EngineExpertEndpoint,
    invocation: &'a EndpointInvocation,
    evidence: &'a TaskEvidence,
    spec: &'a ExpertProgramSpec,
}
impl ToolPort for ExpertTools<'_> {
    fn invoke<'a>(&'a self, call: ToolCall, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ToolInvocationOutcome, AgentFailure>>
    {
        Box::pin(async move {
            let requirement = self.endpoint.selection.requirements.iter()
                .find(|requirement| requirement.key == call.tool_id)
                .filter(|_| call.definition_revision == self.endpoint.admission.definition_revision)
                .ok_or(AgentFailure::CapabilityDenied)?;
            if !self.spec.tools.iter().any(|tool| tool.requirement_key == requirement.key)
                || scope.task_id() != Some(self.invocation.request.task_id)
            { return Err(AgentFailure::CapabilityDenied); }
            let maximum = self.invocation.request.execution_context.max_output_bytes;
            match self.endpoint.sources.read(ExpertSourceRequest {
                actor: self.endpoint.actor.clone(), execution: self.invocation.execution,
                admission: self.endpoint.admission.clone(), requirement: requirement.clone(),
                call: call.clone(), max_output_bytes: maximum,
            }, scope).await? {
                SourceReadOutcome::Ready(read) => {
                    let text = serde_json::to_string(&read.payload).map_err(|_| AgentFailure::InvalidInput)?;
                    if text.len() > maximum { return Err(AgentFailure::BudgetExceeded); }
                    let result = ToolResult { call_id: call.call_id, text, artifacts: vec![],
                        coverage: read.coverage.clone(), issue: None };
                    result.validate(call.call_id, maximum)?;
                    let observation = ExpertToolObservation { call, requirement_key: requirement.key.clone(),
                        outcome: crate::ExpertSourceObservation::Ready {
                            payload: read.payload.clone(), coverage: read.coverage.clone(),
                        } };
                    let mut reads = self.evidence.reads.lock().map_err(|_| AgentFailure::StorageUnavailable)?;
                    if reads.len() >= 32 || reads.iter().any(|prior| prior.observation.call.call_id == observation.call.call_id)
                    { return Err(AgentFailure::BudgetExceeded); }
                    self.invocation.resources.retain(Box::new(read))?;
                    reads.push(CapturedRead { observation });
                    Ok(ToolInvocationOutcome::Completed(result))
                }
                SourceReadOutcome::NeedsUserAction(blockers) =>
                    Ok(ToolInvocationOutcome::NeedsSourceReview { call_id: call.call_id, blockers }),
                SourceReadOutcome::Unavailable(reason) => {
                    let mut reads = self.evidence.reads.lock().map_err(|_| AgentFailure::StorageUnavailable)?;
                    if reads.len() >= 32 || reads.iter().any(|prior| prior.observation.call.call_id == call.call_id)
                    { return Err(AgentFailure::BudgetExceeded); }
                    let call_id = call.call_id;
                    reads.push(CapturedRead { observation: ExpertToolObservation {
                        call, requirement_key: requirement.key.clone(),
                        outcome: crate::ExpertSourceObservation::Unavailable { reason },
                    } });
                    Ok(ToolInvocationOutcome::Completed(ToolResult {
                        call_id, text: "Source is unavailable for this read.".into(), artifacts: vec![],
                        coverage: DependencyCoverage::Independent,
                        issue: Some(floe_agent_contract::OutcomeIssue {
                            failure: AgentFailure::CapabilityUnavailable, retryable: false,
                        }),
                    }))
                }
            }
        })
    }
}

struct ExpertProjection<'a> {
    endpoint: &'a EngineExpertEndpoint,
    invocation: &'a EndpointInvocation,
    evidence: &'a TaskEvidence,
    spec: &'a ExpertProgramSpec,
}
impl ModelProjectionPort for ExpertProjection<'_> {
    fn project<'a>(&'a self, request: ModelProjectionRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ModelProjectionOutcome, AgentFailure>>
    {
        Box::pin(async move {
            if request.role.role_id != self.endpoint.admission.package.id
                || request.catalog.revision != self.endpoint.admission.definition_revision
                || request.plan.consumer != crate::DELEGATED_EXPERT_INFERENCE_CONSUMER
                || scope.task_id() != Some(self.invocation.request.task_id)
            { return Err(AgentFailure::PolicyDenied); }
            let outcome = self.endpoint.projection.project(ExpertProjectionRequest {
                actor: self.endpoint.actor.clone(), execution: self.invocation.execution,
                request, context: self.invocation.request.execution_context.agent_context.clone(),
                prompt: self.spec.prompt.clone(), observations: self.evidence.observations()?,
                package_data_class: self.endpoint.manifest.data_class,
                inherited_coverage: self.invocation.request.execution_context.projection_coverage.clone(),
            }, scope).await?;
            if let ModelProjectionOutcome::Ready(projection) = &outcome {
                projection.validate()?;
                *self.evidence.projection.lock().map_err(|_| AgentFailure::StorageUnavailable)? = Some(projection.clone());
            }
            Ok(outcome)
        })
    }
}

struct ExpertValidator<'a> {
    program: &'a dyn ExpertProgram,
    request: &'a ExpertProgramRequest,
    evidence: &'a TaskEvidence,
    clock: &'a dyn ExpertClock,
    settlement: Mutex<Option<EndpointSettlement>>,
}
impl FinalPayloadValidator for ExpertValidator<'_> {
    fn validate(&self, role: &str, text: &str, artifacts: &[floe_agent_contract::Artifact])
        -> Result<ValidatedFinalPayload, AgentFailure>
    {
        if role != self.request.admission.package.id { return Err(AgentFailure::InvalidInput); }
        let projection = self.evidence.projection.lock().map_err(|_| AgentFailure::StorageUnavailable)?
            .clone().ok_or(AgentFailure::PolicyDenied)?;
        let data = projection.envelope.contextual_data;
        let mut judgment = self.request.clone();
        judgment.now_unix_ms = self.clock.now_unix_ms();
        judgment.context.projection_version = data.projection_version;
        judgment.context.memories = data.memories;
        judgment.context.optional_context_issues = data.optional_context_issues;
        judgment.context.evidence = data.evidence;
        judgment.coverage = projection.coverage;
        let output = self.program.finalize(&judgment, &self.evidence.observations()?, text, artifacts)?;
        if let Some(settlement) = &output.settlement { settlement.validate()?; }
        *self.settlement.lock().map_err(|_| AgentFailure::StorageUnavailable)? = output.settlement;
        Ok(output.payload)
    }
}
struct NoExpertDelegation;
impl DelegationPort for NoExpertDelegation {
    fn delegate<'a>(&'a self, _: DelegationRequest, _: &'a ExecutionScope)
        -> BoxFuture<'a, Result<TaskReceipt, AgentFailure>>
    { Box::pin(async { Err(AgentFailure::CapabilityDenied) }) }
}

impl AgentEndpoint for EngineExpertEndpoint {
    fn execute<'a>(&'a self, invocation: EndpointInvocation, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ExpertExecutionOutcome, AgentFailure>>
    {
        Box::pin(async move {
            invocation.execution.validate()?;
            let request = &invocation.request;
            request.execution_context.validate()?;
            if request.principal != self.actor.person_id.to_string()
                || request.execution_context.device_id != self.actor.device_id
                || request.task_id != invocation.execution.task_id
                || scope.task_id() != Some(request.task_id)
                || scope.root_run_id().map(|id| id.as_uuid()) != request.parent_run_id
                || invocation.request_digest != floe_agent_contract::delegation_request_digest(request)
                || request.selected_agent_id != self.admission.package.id
                || request.selected_definition_revision != self.admission.definition_revision
            { return Err(AgentFailure::PolicyDenied); }
            let registry_snapshot = self.registry.read(&self.actor, scope).await?;
            let registry = crate::AgentRegistry::restore(registry_snapshot.clone(), registry_snapshot.instance_id)?;
            let resolved = registry.resolve_admitted(self.actor.person_id, &self.admission)?;
            // Registry binding/enable changes govern future admissions. This
            // Task keeps its admitted selection while live source authority is
            // revalidated by Context; a rebind cannot redirect or cancel it.
            if resolved.manifest != self.manifest { return Err(AgentFailure::Conflict); }
            let started_at_unix_ms = self.clock.now_unix_ms();
            let program_request = ExpertProgramRequest {
                actor: self.actor.clone(), request: request.clone(), admission: self.admission.clone(),
                selection: self.selection.clone(), private_state: resolved.assignment.private_state,
                now_unix_ms: started_at_unix_ms,
                context: request.execution_context.agent_context.clone(),
                coverage: request.execution_context.projection_coverage.clone(),
                started_at_unix_ms, state_schema_version: self.manifest.state_schema_version,
                data_class: self.manifest.data_class,
            };
            let spec = self.program.specification(&program_request)?;
            spec.prompt.validate()?;
            let mut keys = std::collections::HashSet::new();
            let tools = spec.tools.iter().map(|tool| {
                if !keys.insert(&tool.requirement_key) || !self.selection.requirements.iter().any(|requirement| requirement.key == tool.requirement_key)
                { return Err(AgentFailure::CapabilityDenied); }
                let descriptor = ToolDescriptor { id: tool.requirement_key.clone(),
                    definition_revision: self.admission.definition_revision,
                    description: tool.description.clone(), input_schema: tool.input_schema.clone(),
                    output_data_class: match self.manifest.data_class {
                        floe_agent_contract::DataClass::HighlySensitive => "highlysensitive",
                        floe_agent_contract::DataClass::Synthetic => "synthetic",
                        _ => "personal",
                    }.into() };
                descriptor.validate()?;
                Ok(descriptor)
            }).collect::<Result<Vec<_>, AgentFailure>>()?;
            if tools.len() != self.selection.requirements.len() { return Err(AgentFailure::CapabilityDenied); }
            let evidence = TaskEvidence::default();
            let validator = ExpertValidator { program: self.program.as_ref(), request: &program_request,
                evidence: &evidence, clock: self.clock.as_ref(), settlement: Mutex::new(None) };
            let outcome = Engine::default().drive(EngineRequest {
                execution_id: invocation.execution.execution_id,
                principal: request.principal.clone(), device_id: self.actor.device_id.clone(),
                role_spec: RoleSpec { role_id: self.admission.package.id.clone(),
                    instructions: spec.prompt.render(), output_contract: spec.output_contract.clone(), output_format: floe_agent_contract::ModelOutputFormat::Text },
                scope: scope.clone(),
                conversation: ModelConversation { history: vec![], current_turn: vec![ModelConversationEntry::User {
                    message_id: request.invocation_key.as_uuid(), text: request.message.clone(),
                }] },
                allowed_catalog: AllowedCatalog { cards: vec![], tools, revision: self.admission.definition_revision },
                purpose: "everyday_assistance".into(), consumer: crate::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                max_iterations: 16, max_output_bytes: request.execution_context.max_output_bytes,
                replay: vec![], resume: None, delegation_context: None,
            }, EnginePorts {
                projection: &ExpertProjection { endpoint: self, invocation: &invocation, evidence: &evidence, spec: &spec },
                model: self.model.as_ref(),
                tools: &ExpertTools { endpoint: self, invocation: &invocation, evidence: &evidence, spec: &spec },
                delegation: &NoExpertDelegation,
                journal: &ExpertJournal { inner: invocation.journal.as_ref(), evidence: &evidence }, validator: &validator,
            }).await?;
            match outcome {
                EngineOutcome::Completed(report) => {
                    let Some(text) = report.output else { return Err(AgentFailure::Stalled); };
                    let artifacts = match report.steps.last() {
                        Some(floe_agent_contract::EngineStep::Answer { artifacts, .. }) => artifacts.clone(),
                        _ => return Err(AgentFailure::StorageUnavailable),
                    };
                    let coverage = report.answering_projection_coverage.ok_or(AgentFailure::StorageUnavailable)?;
                    Ok(ExpertExecutionOutcome::Completed(ExpertReport {
                        task_id: request.task_id, principal: request.principal.clone(),
                        agent_id: request.selected_agent_id.clone(), definition_revision: request.selected_definition_revision,
                        result: text, artifacts, coverage,
                        settlement: validator.settlement.into_inner().map_err(|_| AgentFailure::StorageUnavailable)?,
                    }))
                }
                EngineOutcome::Blocked(block) => {
                    let blockage = match block.blockage {
                        EngineBlockage::ModelProjection { plan, review } => TaskBlockage::ModelProjection { plan, review },
                        EngineBlockage::SourceRead { call_id, blockers } => TaskBlockage::SourceRead { tool_call_id: call_id, blockers },
                        EngineBlockage::Delegation { .. } => return Err(AgentFailure::CapabilityDenied),
                    };
                    Ok(ExpertExecutionOutcome::Blocked(ExpertBlockReport {
                        task_id: request.task_id, principal: request.principal.clone(),
                        agent_id: request.selected_agent_id.clone(), definition_revision: request.selected_definition_revision,
                        coverage: evidence.coverage()?, blockage,
                    }))
                }
            }
        })
    }
}
