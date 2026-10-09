use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex, mpsc},
    time::{Duration, Instant},
};

use floe_agent_contract::{
    AgentFailure, AllowedCatalog, BoxFuture, ModelBindingDigest, ModelBudgetProfile,
    ModelCapabilities, ModelPlanRequest, ModelSelectionCommitment, ModelStep, ProcessingBoundary,
};
use floe_app::{
    AppComposition, AppHost, AppOpenOptions, CallerContext, ModelProviderFactory, ReadyOwners,
    RuntimeReadinessState, host_scope,
};
use floe_conversation::{
    CONVERSATION_CONSUMER, CONVERSATION_PURPOSE, CommandReceipt, EventPayload, EventRead,
    ReadConversationEvents, RunEventRecord, RunReceipt, SessionSnapshot, SessionStartAdmission,
    StartTurn,
};
use floe_execution::{Cancellation, ExecutionScope};
use floe_inference::{
    AdmittedDispatchTarget, CanonicalModelRequest as InferenceRequest, CanonicalModelResponse,
    LocalObservation, ModelObservationError, ModelProvider, ModelPurpose, ObservedModelCapability,
    PreparedModelProfile, PreparedModelTransport, PrimaryAbsence, PrimaryObservation,
    ProviderUsageObservation,
};
use floe_kernel::{CommandId, RunId};
use floe_provider_adapters::gateway::GatewayCredentialStore;
use tokio::sync::oneshot;
use uuid::Uuid;

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
use floe_access::{CalendarReadAccessRequest, CalendarReadAccessStamp};
#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
use floe_calendar_operations::{
    ActionBlockedReason, ActionSourceFence, ActionUnknownReason, CalendarDestinationObservation,
    CalendarEffectOutcome, CalendarEffectReceipt, CalendarOperationExecutor,
    CalendarReceiptEvidence, CalendarWriteResult, CommittedCalendarEffect, ExecutionIntent,
    PreparedCalendarEffect,
};
#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
use floe_connections::SourceConnection;
#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
use floe_context::{
    CalendarObservation, CalendarObserveRequest, ExpertRemoteSource, ExpertSourceTransport,
};

const OWNER_TIMEOUT: Duration = Duration::from_secs(15);
const PREPARATION_TIMEOUT: Duration = Duration::from_secs(55);
const LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(60);
const SCRIPTED_REPLY: &str = "A deterministic scripted reply.";

#[derive(Clone, Copy)]
pub enum PrimaryBehavior {
    NoGateway,
    Fail(ModelObservationError),
}

#[derive(Clone)]
pub enum ModelOutput {
    Answer,
    RepeatedAnswers(usize),
    UnsupportedManagerToolCall,
    WaitForCancellation(Arc<GenerateBarrier>),
    ScheduleExpertFlow,
    ScheduleExpertContinuationFlow,
    ScheduleExpertRevocationFlow,
    ScheduleBlockedResumeFlow,
    ScheduleFinalizationFlow,
    ScheduleOperationApprovalFlow,
}

#[derive(Clone, Debug)]
pub struct ScriptModelSelection {
    pub commitment: ModelSelectionCommitment,
    pub boundary: ProcessingBoundary,
    pub binding_digest: ModelBindingDigest,
    pub budget_profile: ModelBudgetProfile,
}

impl ScriptModelSelection {
    pub fn device(commitment_byte: u8) -> Self {
        Self {
            commitment: ModelSelectionCommitment([commitment_byte; 32]),
            boundary: ProcessingBoundary::Device,
            binding_digest: ModelBindingDigest([9; 32]),
            budget_profile: ModelBudgetProfile::unknown(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanStage {
    Primary,
    LocalFallback,
}

#[derive(Clone, Debug)]
pub struct ScriptSnapshot {
    pub plans: Vec<(PlanStage, ModelPlanRequest)>,
    pub plan_bindings: Vec<ScriptCallBinding>,
    pub generated: Vec<InferenceRequest>,
    pub generated_bindings: Vec<ScriptCallBinding>,
    pub violations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptCallBinding {
    pub consumer: String,
    pub run_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub attempt_id: Option<Uuid>,
}

#[derive(Default)]
struct RecorderState {
    plans: Vec<(PlanStage, ModelPlanRequest)>,
    plan_bindings: Vec<ScriptCallBinding>,
    generated: Vec<InferenceRequest>,
    generated_bindings: Vec<ScriptCallBinding>,
    violations: Vec<String>,
    identity: Option<(String, String)>,
    catalogs: std::collections::BTreeMap<String, AllowedCatalog>,
    expected_consumers: Vec<String>,
    model_selections: Vec<ScriptModelSelection>,
}

#[derive(Clone)]
pub struct ScriptRecorder(Arc<Mutex<RecorderState>>, Arc<Condvar>);

impl ScriptRecorder {
    fn new(expected_consumers: Vec<String>) -> Self {
        Self(
            Arc::new(Mutex::new(RecorderState {
                expected_consumers,
                ..RecorderState::default()
            })),
            Arc::new(Condvar::new()),
        )
    }

    fn record_plan(
        &self,
        observed_stage: PlanStage,
        request: &ModelPlanRequest,
        run_id: Option<Uuid>,
        task_id: Option<Uuid>,
    ) -> Result<usize, ModelObservationError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| ModelObservationError::StorageUnavailable)?;
        let call_index = state.plans.len() / 2;
        let expected_stage = if state.plans.len() % 2 == 0 {
            PlanStage::Primary
        } else {
            PlanStage::LocalFallback
        };
        let consumer = state.expected_consumers.get(call_index).map(String::as_str);
        let expected = consumer.map(|consumer| (expected_stage, consumer));
        let Some((expected_stage, expected_consumer)) = expected else {
            state
                .violations
                .push("unexpected additional model plan observation".into());
            return Err(ModelObservationError::InvalidIdentity);
        };
        if observed_stage != expected_stage
            || request.consumer != expected_consumer
            || request.purpose != CONVERSATION_PURPOSE
            || request.required_capabilities.validate().is_err()
            || run_id.is_none()
            || (request.consumer == CONVERSATION_CONSUMER && task_id.is_some())
            || (request.consumer == floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER
                && task_id.is_none())
        {
            state.violations.push(format!(
                "unexpected {observed_stage:?} plan request: {request:?}"
            ));
            return Err(ModelObservationError::InvalidIdentity);
        }
        match &state.identity {
            Some((principal, device_id))
                if principal != &request.principal || device_id != &request.device_id =>
            {
                state
                    .violations
                    .push("model plan changed its bound Person/device".into());
                return Err(ModelObservationError::InvalidIdentity);
            }
            None => {
                state.identity = Some((request.principal.clone(), request.device_id.clone()));
            }
            _ => {}
        }
        let plan_index = state.plans.len();
        state.plans.push((observed_stage, request.clone()));
        state.plan_bindings.push(ScriptCallBinding {
            consumer: request.consumer.clone(),
            run_id,
            task_id,
            attempt_id: None,
        });
        Ok(plan_index)
    }

    pub fn set_model_selection_sequence(&self, selections: Vec<ScriptModelSelection>) {
        self.0
            .lock()
            .expect("script recorder mutex poisoned")
            .model_selections = selections;
    }

    fn model_selection(
        &self,
        call_index: usize,
    ) -> Result<ScriptModelSelection, ModelObservationError> {
        let state = self
            .0
            .lock()
            .map_err(|_| ModelObservationError::StorageUnavailable)?;
        Ok(state
            .model_selections
            .get(call_index)
            .or_else(|| state.model_selections.last())
            .cloned()
            .unwrap_or_else(|| ScriptModelSelection::device(1)))
    }

    fn record_violation(&self, message: impl Into<String>) {
        if let Ok(mut state) = self.0.lock() {
            state.violations.push(message.into());
        }
    }

    fn record_generate(
        &self,
        plan: &ModelPlanRequest,
        request: &InferenceRequest,
        run_id: Option<Uuid>,
        task_id: Option<Uuid>,
    ) -> Result<usize, AgentFailure> {
        let mut state = self.0.lock().map_err(|_| AgentFailure::Interrupted)?;
        let expected = state
            .expected_consumers
            .get(state.generated.len())
            .map(String::as_str);
        let Some(expected_consumer) = expected else {
            state
                .violations
                .push("unexpected additional model generate call".into());
            return Err(AgentFailure::InvalidInput);
        };
        if plan.consumer != expected_consumer
            || (plan.consumer == CONVERSATION_CONSUMER && task_id.is_some())
            || (plan.consumer == floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER
                && task_id.is_none())
            || run_id.is_none()
        {
            state.violations.push(format!(
                "model generate was bound to an unexpected consumer/run/task: consumer={}, run={run_id:?}, task={task_id:?}",
                plan.consumer
            ));
            return Err(AgentFailure::InvalidInput);
        }
        let expected_plan_index = state.generated.len() * 2 + 1;
        let Some(((stage, observed_plan), binding)) = state
            .plans
            .get(expected_plan_index)
            .zip(state.plan_bindings.get(expected_plan_index))
        else {
            state
                .violations
                .push("model generate had no matching fallback plan observation".into());
            return Err(AgentFailure::InvalidInput);
        };
        if *stage != PlanStage::LocalFallback
            || observed_plan != plan
            || binding.run_id != run_id
            || binding.task_id != task_id
        {
            state.violations.push(format!(
                "model generate did not match its observed fallback plan binding: consumer={}, run={run_id:?}, task={task_id:?}",
                plan.consumer
            ));
            return Err(AgentFailure::InvalidInput);
        }
        let index = state.generated.len();
        state.generated.push(request.clone());
        state.generated_bindings.push(ScriptCallBinding {
            consumer: plan.consumer.clone(),
            run_id,
            task_id,
            attempt_id: Some(request.attempt_id),
        });
        drop(state);
        self.1.notify_all();
        Ok(index)
    }

    pub fn wait_for_generated_run(&self, run_id: Uuid, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.0.lock().expect("script recorder mutex poisoned");
        loop {
            if state
                .generated_bindings
                .iter()
                .any(|binding| binding.run_id == Some(run_id))
            {
                return true;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            let (next, timeout) = self
                .1
                .wait_timeout(state, remaining)
                .expect("script recorder mutex poisoned while waiting");
            state = next;
            if timeout.timed_out() {
                return state
                    .generated_bindings
                    .iter()
                    .any(|binding| binding.run_id == Some(run_id));
            }
        }
    }

    pub fn snapshot(&self) -> ScriptSnapshot {
        let state = self.0.lock().expect("script recorder mutex poisoned");
        ScriptSnapshot {
            plans: state.plans.clone(),
            plan_bindings: state.plan_bindings.clone(),
            generated: state.generated.clone(),
            generated_bindings: state.generated_bindings.clone(),
            violations: state.violations.clone(),
        }
    }
}

pub struct ScriptedModel {
    primary: PrimaryBehavior,
    expected_input: String,
    output: ModelOutput,
    recorder: ScriptRecorder,
}

impl ScriptedModel {
    pub fn new(
        primary: PrimaryBehavior,
        expected_input: impl Into<String>,
        output: ModelOutput,
    ) -> Self {
        let expected_consumers = match &output {
            ModelOutput::RepeatedAnswers(count) => {
                vec![CONVERSATION_CONSUMER.into(); *count]
            }
            ModelOutput::ScheduleExpertFlow => vec![
                CONVERSATION_CONSUMER.into(),
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                CONVERSATION_CONSUMER.into(),
            ],
            ModelOutput::ScheduleExpertContinuationFlow => vec![
                CONVERSATION_CONSUMER.into(),
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                CONVERSATION_CONSUMER.into(),
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                CONVERSATION_CONSUMER.into(),
            ],
            ModelOutput::ScheduleExpertRevocationFlow => vec![
                CONVERSATION_CONSUMER.into(),
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                CONVERSATION_CONSUMER.into(),
                CONVERSATION_CONSUMER.into(),
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                CONVERSATION_CONSUMER.into(),
            ],
            ModelOutput::ScheduleBlockedResumeFlow => {
                vec![CONVERSATION_CONSUMER.into(), CONVERSATION_CONSUMER.into()]
            }
            ModelOutput::ScheduleFinalizationFlow | ModelOutput::ScheduleOperationApprovalFlow => {
                vec![
                    CONVERSATION_CONSUMER.into(),
                    floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                    floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER.into(),
                    CONVERSATION_CONSUMER.into(),
                    CONVERSATION_CONSUMER.into(),
                    CONVERSATION_CONSUMER.into(),
                ]
            }
            _ => vec![CONVERSATION_CONSUMER.into()],
        };
        Self {
            primary,
            expected_input: expected_input.into(),
            output,
            recorder: ScriptRecorder::new(expected_consumers),
        }
    }

    pub fn recorder(&self) -> ScriptRecorder {
        self.recorder.clone()
    }

    pub fn app_options(&self) -> AppOpenOptions {
        AppOpenOptions::default().with_model_provider_factory(ScriptedModelProviderFactory {
            model: self.clone(),
        })
    }

    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
    pub fn app_options_with_qa_source_transport(
        &self,
        transport: impl ExpertSourceTransport + 'static,
    ) -> AppOpenOptions {
        self.app_options()
            .with_qa_expert_source_transport(transport)
    }

    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
    pub fn app_options_with_qa_calendar_executor(
        &self,
        executor: ScriptedCalendarExecutor,
    ) -> AppOpenOptions {
        self.app_options()
            .with_qa_calendar_operation_executor(executor)
    }

    fn validate_canonical(
        &self,
        request: &InferenceRequest,
        plan: &ModelPlanRequest,
        run_id: Option<Uuid>,
        task_id: Option<Uuid>,
    ) -> Result<(), AgentFailure> {
        request.validate()?;
        let expected_user_entries = request
            .envelope
            .conversation
            .current_turn
            .iter()
            .filter(|entry| {
                matches!(entry, floe_agent_contract::ModelConversationEntry::User { text, .. } if text == &self.expected_input)
            })
            .count();
        let capabilities = ModelCapabilities::for_request(
            &request.envelope.run_instructions.output_format,
            &request.catalog,
        )?;
        let schedule_flow = matches!(
            &self.output,
            ModelOutput::ScheduleExpertFlow
                | ModelOutput::ScheduleExpertContinuationFlow
                | ModelOutput::ScheduleExpertRevocationFlow
                | ModelOutput::ScheduleBlockedResumeFlow
                | ModelOutput::ScheduleFinalizationFlow
                | ModelOutput::ScheduleOperationApprovalFlow
        );
        let consumer_is_conversation = plan.consumer == CONVERSATION_CONSUMER;
        let consumer_is_expert = plan.consumer == floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER;
        let valid_catalog = if !schedule_flow || consumer_is_conversation {
            request.catalog.tools.is_empty()
        } else if consumer_is_expert {
            request.catalog.tools.len() == 1
                && request.catalog.tools[0].id == "floe.source.calendar"
                && request.catalog.cards.is_empty()
        } else {
            false
        };
        if (!schedule_flow && !consumer_is_conversation)
            || (schedule_flow && !consumer_is_conversation && !consumer_is_expert)
            || plan.purpose != CONVERSATION_PURPOSE
            || request.envelope.run_instructions.purpose != plan.purpose
            || !valid_catalog
            || capabilities != plan.required_capabilities
            || expected_user_entries != 1
        {
            self.recorder.record_violation(
                "canonical request did not match expected input, purpose, or consumer catalog",
            );
            return Err(AgentFailure::InvalidInput);
        }
        let mut state = self
            .recorder
            .0
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        let catalog_key = if request.envelope.run_instructions.response_contract
            == floe_conversation::FINALIZATION_OUTPUT_CONTRACT
        {
            format!("{}:{run_id:?}:{task_id:?}:finalization", plan.consumer)
        } else {
            format!("{}:{run_id:?}:{task_id:?}", plan.consumer)
        };
        if let Some(expected_catalog) = state.catalogs.get(&catalog_key) {
            if expected_catalog != &request.catalog {
                state
                    .violations
                    .push("canonical request changed its consumer catalog".into());
                return Err(AgentFailure::InvalidInput);
            }
        } else {
            state.catalogs.insert(catalog_key, request.catalog.clone());
        }
        Ok(())
    }

    fn schedule_flow_response(
        &self,
        call_index: usize,
        request: &InferenceRequest,
    ) -> Result<CanonicalModelResponse, AgentFailure> {
        let call_index = if matches!(&self.output, ModelOutput::ScheduleExpertRevocationFlow) {
            call_index % 4
        } else {
            call_index
        };
        match call_index {
            0 => {
                if request.catalog.tools.is_empty() {
                    let schedule = request
                        .catalog
                        .cards
                        .iter()
                        .find(|definition| definition.card.id == "floe.builtin.schedule")
                        .ok_or_else(|| {
                            self.recorder.record_violation(format!(
                                "conversation script could not find Schedule card; cards={:?}",
                                request.catalog.cards
                            ));
                            AgentFailure::InvalidInput
                        })?;
                    Ok(response(vec![ModelStep::Delegate {
                        agent_id: schedule.card.id.clone(),
                        definition_revision: schedule.definition_revision,
                        message: self.expected_input.clone(),
                        context_refs: vec![],
                    }]))
                } else {
                    Err(AgentFailure::InvalidInput)
                }
            }
            1 if matches!(&self.output, ModelOutput::ScheduleBlockedResumeFlow) => {
                Ok(response(vec![ModelStep::Answer {
                    text: "I resumed with the original request and no new source result.".into(),
                    artifacts: vec![],
                }]))
            }
            1 | 4
                if call_index == 1
                    || matches!(&self.output, ModelOutput::ScheduleExpertContinuationFlow) =>
            {
                let [calendar] = request.catalog.tools.as_slice() else {
                    self.recorder.record_violation(format!(
                        "Schedule script expected one source tool at generation {call_index}; tools={:?}",
                        request.catalog.tools
                    ));
                    return Err(AgentFailure::InvalidInput);
                };
                if calendar.id != "floe.source.calendar" {
                    self.recorder.record_violation(format!(
                        "Schedule script expected floe.source.calendar at generation {call_index}; observed tool={calendar:?}"
                    ));
                    return Err(AgentFailure::InvalidInput);
                }
                let schema: serde_json::Value = serde_json::from_str(&calendar.input_schema)
                    .map_err(|error| {
                        self.recorder.record_violation(format!(
                            "Schedule script received invalid tool schema at generation {call_index}: {error}; schema={}",
                            calendar.input_schema
                        ));
                        AgentFailure::InvalidInput
                    })?;
                let pinned_range = |property: &str| {
                    schema["properties"][property]["const"]
                        .as_i64()
                        .ok_or_else(|| {
                            self.recorder.record_violation(format!(
                                "Schedule script missing integer const for {property}; schema={schema}"
                            ));
                            AgentFailure::InvalidInput
                        })
                };
                let range_start_unix_ms = pinned_range("range_start_unix_ms")?;
                let range_end_unix_ms = pinned_range("range_end_unix_ms")?;
                let input = serde_json::json!({
                    "range_start_unix_ms": range_start_unix_ms,
                    "range_end_unix_ms": range_end_unix_ms,
                    "limit": 8,
                });
                Ok(response(vec![ModelStep::CallTool {
                    tool_id: calendar.id.clone(),
                    definition_revision: calendar.definition_revision,
                    input: serde_json::to_string(&input).map_err(|_| AgentFailure::InvalidInput)?,
                }]))
            }
            5 if matches!(&self.output, ModelOutput::ScheduleOperationApprovalFlow) => {
                Ok(response(vec![ModelStep::Answer {
                    text: "I resumed the reviewed Calendar operation.".into(),
                    artifacts: vec![],
                }]))
            }
            4 if matches!(&self.output, ModelOutput::ScheduleOperationApprovalFlow) => {
                Ok(response(vec![ModelStep::Answer {
                    text: "I resumed the reviewed Calendar operation.".into(),
                    artifacts: vec![],
                }]))
            }
            2 | 5 => Ok(response(vec![ModelStep::Answer {
                text:
                    "The selected synthetic calendar has one planning event in the requested range."
                        .into(),
                artifacts: vec![],
            }])),
            3 => match &self.output {
                ModelOutput::ScheduleExpertFlow => Ok(response(vec![ModelStep::Answer {
                    text: SCRIPTED_REPLY.into(),
                    artifacts: vec![],
                }])),
                ModelOutput::ScheduleExpertRevocationFlow => {
                    Ok(response(vec![ModelStep::Answer {
                        text: SCRIPTED_REPLY.into(),
                        artifacts: vec![],
                    }]))
                }
                ModelOutput::ScheduleExpertContinuationFlow => {
                    let schedule = request
                        .catalog
                        .cards
                        .iter()
                        .find(|definition| definition.card.id == "floe.builtin.schedule")
                        .ok_or(AgentFailure::InvalidInput)?;
                    Ok(response(vec![ModelStep::Delegate {
                        agent_id: schedule.card.id.clone(),
                        definition_revision: schedule.definition_revision,
                        message: self.expected_input.clone(),
                        context_refs: vec![],
                    }]))
                }
                ModelOutput::ScheduleBlockedResumeFlow => Err(AgentFailure::InvalidInput),
                ModelOutput::ScheduleFinalizationFlow => Err(AgentFailure::BudgetExceeded),
                ModelOutput::ScheduleOperationApprovalFlow => {
                    Ok(response(vec![ModelStep::Answer {
                        text: "I found a time that fits the calendar.".into(),
                        artifacts: vec![],
                    }]))
                }
                _ => Err(AgentFailure::InvalidInput),
            },
            6 if matches!(&self.output, ModelOutput::ScheduleExpertContinuationFlow) => {
                Ok(response(vec![ModelStep::Answer {
                    text: SCRIPTED_REPLY.into(),
                    artifacts: vec![],
                }]))
            }
            4 if matches!(
                &self.output,
                ModelOutput::ScheduleFinalizationFlow | ModelOutput::ScheduleOperationApprovalFlow
            ) =>
            {
                self.finalization_answer(request)
            }
            _ => {
                self.recorder
                    .record_violation("unexpected scripted schedule flow generation index");
                Err(AgentFailure::InvalidInput)
            }
        }
    }

    fn finalization_answer(
        &self,
        request: &InferenceRequest,
    ) -> Result<CanonicalModelResponse, AgentFailure> {
        if request.envelope.run_instructions.response_contract
            != floe_conversation::FINALIZATION_OUTPUT_CONTRACT
        {
            self.recorder.record_violation(
                "finalizer request did not carry the finalization response contract",
            );
            return Err(AgentFailure::InvalidInput);
        }
        Ok(response(vec![ModelStep::Answer {
            text: SCRIPTED_REPLY.into(),
            artifacts: vec![],
        }]))
    }
}

impl Clone for ScriptedModel {
    fn clone(&self) -> Self {
        Self {
            primary: self.primary,
            expected_input: self.expected_input.clone(),
            output: self.output.clone(),
            recorder: self.recorder.clone(),
        }
    }
}

struct ScriptedModelProviderFactory {
    model: ScriptedModel,
}

impl ModelProviderFactory for ScriptedModelProviderFactory {
    fn create(&self, _: GatewayCredentialStore) -> Arc<dyn ModelProvider> {
        Arc::new(ScriptedModelProvider {
            model: self.model.clone(),
        })
    }
}

struct ScriptedModelProvider {
    model: ScriptedModel,
}

impl ModelProvider for ScriptedModelProvider {
    fn observe_primary<'a>(
        &'a self,
        request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<PrimaryObservation<Box<dyn PreparedModelTransport>>, ModelObservationError>,
    > {
        let model = self.model.clone();
        Box::pin(async move {
            let _ = model.recorder.record_plan(
                PlanStage::Primary,
                request,
                scope.root_run_id().map(RunId::as_uuid),
                scope.task_id().map(|task_id| task_id.as_uuid()),
            )?;
            match model.primary {
                PrimaryBehavior::NoGateway => Ok(PrimaryObservation::Absent(
                    PrimaryAbsence::NoGatewayConfigured,
                )),
                PrimaryBehavior::Fail(error) => Err(error),
            }
        })
    }

    fn observe_local_fallback<'a>(
        &'a self,
        request: &'a ModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<LocalObservation<Box<dyn PreparedModelTransport>>, ModelObservationError>,
    > {
        let model = self.model.clone();
        Box::pin(async move {
            if !matches!(model.primary, PrimaryBehavior::NoGateway) {
                model
                    .recorder
                    .record_violation("fallback observed after Primary failure");
                return Err(ModelObservationError::InvalidIdentity);
            }
            if scope.cancellation().is_cancelled() {
                return Err(ModelObservationError::Cancelled);
            }
            let plan_index = model.recorder.record_plan(
                PlanStage::LocalFallback,
                request,
                scope.root_run_id().map(RunId::as_uuid),
                scope.task_id().map(|task_id| task_id.as_uuid()),
            )?;
            let selection = model.recorder.model_selection(plan_index / 2)?;
            let capabilities = ModelCapabilities(vec![
                floe_agent_contract::ModelCapability::Chat,
                floe_agent_contract::ModelCapability::ToolProposals,
            ]);
            let binding_digest = selection.binding_digest;
            let purpose = ModelPurpose::new(request.purpose.clone())
                .ok_or(ModelObservationError::InvalidIdentity)?;
            let consumer = floe_inference::ModelConsumer::new(request.consumer.clone())
                .ok_or(ModelObservationError::InvalidIdentity)?;
            let transport = ScriptedTransport {
                model: model.clone(),
                plan: request.clone(),
                run_id: scope.root_run_id().map(RunId::as_uuid),
                task_id: scope.task_id().map(|task_id| task_id.as_uuid()),
                binding_digest,
                capabilities: capabilities.clone(),
            };
            let transport: Box<dyn PreparedModelTransport> = Box::new(transport);
            Ok(LocalObservation::Available(PreparedModelProfile {
                capability: ObservedModelCapability {
                    purpose,
                    consumer,
                    capabilities,
                    boundary: selection.boundary,
                    binding_digest,
                    selection_commitment: selection.commitment,
                    budget_profile: selection.budget_profile,
                },
                transport,
            }))
        })
    }
}

struct ScriptedTransport {
    model: ScriptedModel,
    plan: ModelPlanRequest,
    run_id: Option<Uuid>,
    task_id: Option<Uuid>,
    binding_digest: floe_agent_contract::ModelBindingDigest,
    capabilities: ModelCapabilities,
}

impl PreparedModelTransport for ScriptedTransport {
    fn validate_request(&self, request: &InferenceRequest) -> Result<(), AgentFailure> {
        self.model
            .validate_canonical(request, &self.plan, self.run_id, self.task_id)
    }

    fn dispatch_target(&self) -> floe_access::ModelDispatchTarget {
        floe_access::ModelDispatchTarget::Device
    }

    fn generate<'a>(
        &'a self,
        request: InferenceRequest,
        target: AdmittedDispatchTarget,
    ) -> BoxFuture<'a, Result<CanonicalModelResponse, AgentFailure>> {
        Box::pin(async move {
            self.validate_request(&request)?;
            if !target.matches(&self.binding_digest, ProcessingBoundary::Device)
                || !self.capabilities.includes(&ModelCapabilities::for_request(
                    &request.envelope.run_instructions.output_format,
                    &request.catalog,
                )?)
            {
                self.model.recorder.record_violation(
                    "model dispatch target or capabilities did not match the admitted Device plan",
                );
                return Err(AgentFailure::PolicyDenied);
            }
            let call_index = self.model.recorder.record_generate(
                &self.plan,
                &request,
                self.run_id,
                self.task_id,
            )?;
            match &self.model.output {
                ModelOutput::Answer => Ok(response(vec![ModelStep::Answer {
                    text: SCRIPTED_REPLY.into(),
                    artifacts: vec![],
                }])),
                ModelOutput::RepeatedAnswers(_) => Ok(response(vec![ModelStep::Answer {
                    text: SCRIPTED_REPLY.into(),
                    artifacts: vec![],
                }])),
                ModelOutput::UnsupportedManagerToolCall => {
                    Ok(response(vec![ModelStep::CallTool {
                        tool_id: "unsupported.manager.test".into(),
                        definition_revision: 1,
                        input: "{}".into(),
                    }]))
                }
                ModelOutput::WaitForCancellation(barrier) => {
                    barrier.wait(&request.cancellation).await?;
                    Ok(response(vec![ModelStep::Answer {
                        text: SCRIPTED_REPLY.into(),
                        artifacts: vec![],
                    }]))
                }
                ModelOutput::ScheduleExpertFlow => {
                    self.model.schedule_flow_response(call_index, &request)
                }
                ModelOutput::ScheduleExpertContinuationFlow => {
                    self.model.schedule_flow_response(call_index, &request)
                }
                ModelOutput::ScheduleExpertRevocationFlow => {
                    self.model.schedule_flow_response(call_index, &request)
                }
                ModelOutput::ScheduleBlockedResumeFlow => {
                    self.model.schedule_flow_response(call_index, &request)
                }
                ModelOutput::ScheduleFinalizationFlow => {
                    self.model.schedule_flow_response(call_index, &request)
                }
                ModelOutput::ScheduleOperationApprovalFlow => {
                    self.model.schedule_flow_response(call_index, &request)
                }
            }
        })
    }
}

fn response(steps: Vec<ModelStep>) -> CanonicalModelResponse {
    CanonicalModelResponse {
        output: Ok(steps),
        usage: ProviderUsageObservation {
            tokens: Some(19),
            cost_micros: Some(23),
        },
    }
}

pub struct GenerateBarrier {
    entered: Mutex<Option<mpsc::SyncSender<()>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
}

impl GenerateBarrier {
    pub fn new() -> (Arc<Self>, mpsc::Receiver<()>, oneshot::Sender<()>) {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = oneshot::channel();
        (
            Arc::new(Self {
                entered: Mutex::new(Some(entered_tx)),
                release: Mutex::new(Some(release_rx)),
            }),
            entered_rx,
            release_tx,
        )
    }

    async fn wait(&self, cancellation: &Cancellation) -> Result<(), AgentFailure> {
        self.entered
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?
            .take()
            .ok_or(AgentFailure::InvalidInput)?
            .send(())
            .map_err(|_| AgentFailure::Interrupted)?;
        let release = self
            .release
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?
            .take()
            .ok_or(AgentFailure::InvalidInput)?;
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(AgentFailure::Cancelled),
            result = release => result.map_err(|_| AgentFailure::Interrupted),
        }
    }
}

pub struct IsolatedProfile {
    root: tempfile::TempDir,
}

impl IsolatedProfile {
    pub fn new() -> Self {
        Self {
            root: tempfile::Builder::new()
                .prefix("floe-p1a-conversation-")
                .tempdir()
                .expect("create isolated development profile"),
        }
    }

    pub fn open(&self, model: &ScriptedModel) -> AppHost<AppComposition> {
        let path = self.root.path().to_str().expect("temporary path is UTF-8");
        floe_app::open_default_with_options(path, model.app_options())
            .unwrap_or_else(|error| panic!("open isolated App profile: {error:?}"))
    }

    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
    pub fn open_with_qa_source_transport(
        &self,
        model: &ScriptedModel,
        transport: impl ExpertSourceTransport + 'static,
    ) -> AppHost<AppComposition> {
        let path = self.root.path().to_str().expect("temporary path is UTF-8");
        floe_app::open_default_with_options(
            path,
            model.app_options_with_qa_source_transport(transport),
        )
        .unwrap_or_else(|error| panic!("open isolated App profile: {error:?}"))
    }

    #[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
    pub fn open_with_qa_calendar_executor(
        &self,
        model: &ScriptedModel,
        executor: ScriptedCalendarExecutor,
    ) -> AppHost<AppComposition> {
        let path = self.root.path().to_str().expect("temporary path is UTF-8");
        floe_app::open_default_with_options(
            path,
            model.app_options_with_qa_calendar_executor(executor),
        )
        .unwrap_or_else(|error| panic!("open isolated App profile: {error:?}"))
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalendarExecutorScript {
    Commit,
    PreDispatchFailure,
    ApplyThenLoseAck,
    MissingReceipt,
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CalendarExecutorSnapshot {
    pub destinations: usize,
    pub preparations: usize,
    pub pre_dispatch_failures: usize,
    pub dispatch_attempts: usize,
    pub external_effects: usize,
    pub lookups: usize,
    pub effect_kinds: Vec<&'static str>,
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[derive(Clone)]
pub struct ScriptedCalendarExecutor {
    state: Arc<Mutex<ScriptedCalendarExecutorState>>,
    changed: Arc<Condvar>,
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
#[derive(Default)]
struct ScriptedCalendarExecutorState {
    scripts: VecDeque<CalendarExecutorScript>,
    snapshot: CalendarExecutorSnapshot,
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
impl ScriptedCalendarExecutor {
    pub fn new(scripts: impl IntoIterator<Item = CalendarExecutorScript>) -> Self {
        Self {
            state: Arc::new(Mutex::new(ScriptedCalendarExecutorState {
                scripts: scripts.into_iter().collect(),
                snapshot: CalendarExecutorSnapshot::default(),
            })),
            changed: Arc::new(Condvar::new()),
        }
    }

    pub fn snapshot(&self) -> CalendarExecutorSnapshot {
        self.state
            .lock()
            .expect("scripted calendar executor lock")
            .snapshot
            .clone()
    }

    pub fn queue_script(&self, script: CalendarExecutorScript) {
        self.state
            .lock()
            .expect("scripted calendar executor lock")
            .scripts
            .push_back(script);
        self.changed.notify_all();
    }

    pub fn wait_for_snapshot(
        &self,
        timeout: Duration,
        accepted: impl Fn(&CalendarExecutorSnapshot) -> bool,
    ) -> Option<CalendarExecutorSnapshot> {
        let deadline = Instant::now() + timeout;
        let mut state = self.state.lock().ok()?;
        loop {
            if accepted(&state.snapshot) {
                return Some(state.snapshot.clone());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (next, result) = self.changed.wait_timeout(state, remaining).ok()?;
            state = next;
            if result.timed_out() && !accepted(&state.snapshot) {
                return None;
            }
        }
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
impl ScriptedCalendarExecutorState {
    fn next_script(&mut self) -> Option<CalendarExecutorScript> {
        self.scripts.pop_front()
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
impl CalendarOperationExecutor for ScriptedCalendarExecutor {
    fn destinations<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        source: &'a ActionSourceFence,
        scope: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<Vec<CalendarDestinationObservation>, ActionBlockedReason>,
    > {
        Box::pin(async move {
            if actor.validate().is_err() || scope.cancellation().is_cancelled() {
                return Err(ActionBlockedReason::PolicyDenied);
            }
            let mut state = self
                .state
                .lock()
                .map_err(|_| ActionBlockedReason::ExecutorUnavailable)?;
            state.snapshot.destinations += 1;
            self.changed.notify_all();
            Ok(source
                .resources
                .iter()
                .map(|calendar_id| CalendarDestinationObservation {
                    calendar_id: calendar_id.clone(),
                    calendar_name: "Synthetic QA Calendar".into(),
                    can_modify: true,
                })
                .collect())
        })
    }

    fn prepare<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        record: &'a floe_calendar_operations::ActionRecord,
        _: &'a [floe_calendar_operations::ActionDependencySourceFence],
        _: &'a [floe_day::Event],
        scope: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<'a, Result<Box<dyn PreparedCalendarEffect>, ActionBlockedReason>>
    {
        Box::pin(async move {
            if actor.validate().is_err() || scope.cancellation().is_cancelled() {
                return Err(ActionBlockedReason::PolicyDenied);
            }
            let mut state = self
                .state
                .lock()
                .map_err(|_| ActionBlockedReason::ExecutorUnavailable)?;
            state.snapshot.preparations += 1;
            let script = state.next_script();
            if script == Some(CalendarExecutorScript::PreDispatchFailure) {
                state.snapshot.pre_dispatch_failures += 1;
                self.changed.notify_all();
                return Err(ActionBlockedReason::ExecutorUnavailable);
            }
            self.changed.notify_all();
            Ok(Box::new(ScriptedPreparedCalendarEffect {
                state: self.state.clone(),
                changed: self.changed.clone(),
                script,
                executor_generation: actor.runtime_epoch,
                effect: record.effect.clone(),
            }) as Box<dyn PreparedCalendarEffect>)
        })
    }

    fn recover<'a>(
        &'a self,
        actor: &'a floe_kernel::OwnerActor,
        intent: &'a ExecutionIntent,
        scope: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<'a, CalendarEffectOutcome> {
        Box::pin(async move {
            if actor.validate().is_err() || scope.cancellation().is_cancelled() {
                return CalendarEffectOutcome::Unknown {
                    identity: intent.identity(),
                    reason: ActionUnknownReason::Timeout,
                };
            }
            if let Ok(mut state) = self.state.lock() {
                state.snapshot.lookups += 1;
                self.changed.notify_all();
            }
            CalendarEffectOutcome::Unknown {
                identity: intent.identity(),
                reason: ActionUnknownReason::NativeReceiptUnavailable,
            }
        })
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
struct ScriptedPreparedCalendarEffect {
    state: Arc<Mutex<ScriptedCalendarExecutorState>>,
    changed: Arc<Condvar>,
    script: Option<CalendarExecutorScript>,
    executor_generation: u64,
    effect: floe_calendar_operations::CalendarEffect,
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
impl Drop for ScriptedPreparedCalendarEffect {
    fn drop(&mut self) {
        if let Some(script) = self.script.take()
            && let Ok(mut state) = self.state.lock()
        {
            state.scripts.push_front(script);
        }
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
impl PreparedCalendarEffect for ScriptedPreparedCalendarEffect {
    fn executor_generation(&self) -> u64 {
        self.executor_generation
    }

    fn dispatch(
        mut self: Box<Self>,
        admission: floe_calendar_operations::DispatchAdmission,
        _: ExecutionScope,
    ) -> floe_execution::BoxFuture<'static, CalendarEffectOutcome> {
        let script = self.script.take().unwrap_or(CalendarExecutorScript::Commit);
        let state = self.state.clone();
        let changed = self.changed.clone();
        let effect = self.effect.clone();
        Box::pin(async move {
            let intent = admission.intent;
            if let Ok(mut state) = state.lock() {
                state.snapshot.dispatch_attempts += 1;
                state.snapshot.effect_kinds.push(match &effect {
                    floe_calendar_operations::CalendarEffect::Create { .. } => "create",
                    floe_calendar_operations::CalendarEffect::Update { .. } => "update",
                    floe_calendar_operations::CalendarEffect::Delete { .. } => "delete",
                });
                if matches!(
                    script,
                    CalendarExecutorScript::Commit
                        | CalendarExecutorScript::ApplyThenLoseAck
                        | CalendarExecutorScript::MissingReceipt
                ) {
                    state.snapshot.external_effects += 1;
                }
                changed.notify_all();
            }
            if matches!(
                script,
                CalendarExecutorScript::ApplyThenLoseAck | CalendarExecutorScript::MissingReceipt
            ) {
                return CalendarEffectOutcome::Unknown {
                    identity: intent.identity(),
                    reason: ActionUnknownReason::NativeReceiptUnavailable,
                };
            }
            let committed_effect = match &effect {
                floe_calendar_operations::CalendarEffect::Create {
                    title, schedule, ..
                } => CommittedCalendarEffect::Created {
                    event: CalendarWriteResult {
                        external_id: format!("qa-created-{}", intent.execution_id),
                        external_revision:
                            floe_day::CalendarExternalRevision::ObservationFingerprint([1; 32]),
                        title: title.clone(),
                        schedule: schedule.clone(),
                        can_modify: true,
                    },
                },
                floe_calendar_operations::CalendarEffect::Update {
                    target,
                    title,
                    schedule,
                    ..
                } => CommittedCalendarEffect::Updated {
                    target: target.clone(),
                    event: CalendarWriteResult {
                        external_id: match &target.original.source {
                            floe_day::SourceRef::Calendar(source) => source.external_id.clone(),
                            _ => "qa-updated".into(),
                        },
                        external_revision:
                            floe_day::CalendarExternalRevision::ObservationFingerprint([2; 32]),
                        title: title.clone(),
                        schedule: schedule.clone(),
                        can_modify: true,
                    },
                },
                floe_calendar_operations::CalendarEffect::Delete { target, .. } => {
                    CommittedCalendarEffect::Deleted {
                        target: target.clone(),
                    }
                }
            };
            let committed_at = intent.prepared_at + chrono::Duration::milliseconds(1);
            CalendarEffectOutcome::Committed {
                receipt: CalendarEffectReceipt {
                    identity: intent.identity(),
                    effect: committed_effect,
                    evidence: CalendarReceiptEvidence::NativeAcknowledgement {
                        host_epoch: Uuid::new_v4(),
                        receipt_id: Uuid::new_v4(),
                    },
                    committed_at,
                },
            }
        })
    }
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
pub struct UnavailableCalendarTransport;

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
impl ExpertSourceTransport for UnavailableCalendarTransport {
    fn remote<'a>(
        &'a self,
        _: &'a floe_kernel::OwnerActor,
        _: &'a ExecutionScope,
    ) -> floe_execution::BoxFuture<'a, Result<Option<ExpertRemoteSource>, AgentFailure>> {
        Box::pin(async { Ok(None) })
    }

    fn check_calendar<'a>(
        &'a self,
        _: &'a floe_kernel::OwnerActor,
        _: &'a SourceConnection,
        _: CalendarReadAccessRequest,
    ) -> floe_execution::BoxFuture<'a, Result<CalendarReadAccessStamp, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
    }

    fn observe_calendar<'a>(
        &'a self,
        _: &'a floe_kernel::OwnerActor,
        _: &'a SourceConnection,
        _: CalendarObserveRequest,
    ) -> floe_execution::BoxFuture<'a, Result<CalendarObservation, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::CapabilityUnavailable) })
    }
}

pub fn prepare_runtime(host: &AppHost<AppComposition>) -> RuntimeReadinessState {
    let request = host
        .request(Uuid::new_v4())
        .expect("admit Runtime preparation request");
    let services = request.services();
    let caller = request.caller();
    let operation_id = Uuid::new_v4();
    services
        .prepare_runtime(caller, operation_id)
        .expect("admit Runtime preparation command");
    let deadline = Instant::now() + PREPARATION_TIMEOUT;
    loop {
        let result = services
            .get_runtime_preparation(caller, operation_id)
            .expect("read Runtime preparation result");
        if result.done {
            let completed = services
                .acknowledge_runtime_preparation(caller, operation_id)
                .expect("acknowledge completed Runtime preparation");
            assert_eq!(completed.failure, None);
            return services
                .runtime_readiness(caller, Uuid::new_v4())
                .expect("observe Runtime readiness")
                .state;
        }
        assert!(
            Instant::now() < deadline,
            "Runtime preparation exceeded deadline"
        );
        std::thread::yield_now();
    }
}

pub fn with_ready<T>(
    host: &AppHost<AppComposition>,
    operation: impl FnOnce(&AppComposition, &CallerContext, Arc<ReadyOwners>) -> T,
) -> T {
    let request = host
        .request(Uuid::new_v4())
        .expect("admit owner request through AppHost");
    let services = request.services();
    let caller = request.caller();
    let owners = services
        .ready_owners(caller)
        .expect("get active generation's real owners");
    operation(services, caller, owners)
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
pub fn configure_fixture_calendar(
    host: &AppHost<AppComposition>,
    selected_calendar_label: &str,
) -> floe_connections::SourceSummary {
    let selected_calendar_label = selected_calendar_label.to_owned();
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                let overview_scope =
                    host_scope(Uuid::new_v4(), Cancellation::new(), Duration::from_secs(30));
                let overview = owners.connections.overview(&actor, &overview_scope).await?;
                let integration = overview
                    .integrations
                    .iter()
                    .find(|integration| {
                        integration.service_kind
                            == floe_connections::IntegrationServiceKind::SyntheticQaCalendar
                    })
                    .ok_or(AgentFailure::CapabilityUnavailable)?;

                let integration_review = owners
                    .connections
                    .prepare_integration_review(
                        &actor,
                        Uuid::new_v4(),
                        integration.integration_ref,
                        integration.revision,
                        &host_scope(Uuid::new_v4(), Cancellation::new(), Duration::from_secs(30)),
                    )
                    .await
                    .map_err(|failure| failure.into_failure())?;
                let operation = owners
                    .connections
                    .start_integration(
                        &actor,
                        Uuid::new_v4(),
                        integration.integration_ref,
                        integration_review.review_ref,
                        integration.revision,
                        &host_scope(Uuid::new_v4(), Cancellation::new(), Duration::from_secs(30)),
                    )
                    .await
                    .map_err(|failure| failure.into_failure())?;

                let deadline = Instant::now() + LIFECYCLE_TIMEOUT;
                let operation = loop {
                    if Instant::now() >= deadline {
                        return Err(AgentFailure::DeadlineExceeded);
                    }
                    let snapshot = owners
                        .connections
                        .get_operation(
                            &actor,
                            operation.operation_ref,
                            &host_scope(
                                Uuid::new_v4(),
                                Cancellation::new(),
                                Duration::from_secs(15),
                            ),
                        )
                        .await?;
                    match snapshot.state {
                        floe_connections::ConnectionOperationState::Completed => break snapshot,
                        floe_connections::ConnectionOperationState::Failed
                        | floe_connections::ConnectionOperationState::Cancelled
                        | floe_connections::ConnectionOperationState::RepairRequired => {
                            return Err(AgentFailure::CapabilityUnavailable);
                        }
                        _ => tokio::time::sleep(Duration::from_millis(25)).await,
                    }
                };
                let source = operation
                    .source
                    .ok_or(AgentFailure::CapabilityUnavailable)?;
                let source_review = owners
                    .connections
                    .prepare_source_review(
                        &actor,
                        Uuid::new_v4(),
                        source.source_ref,
                        source.revision,
                        &host_scope(Uuid::new_v4(), Cancellation::new(), Duration::from_secs(30)),
                    )
                    .await
                    .map_err(|failure| failure.into_failure())?;
                let resource = source_review
                    .permitted_choices
                    .iter()
                    .find(|choice| choice.label == selected_calendar_label)
                    .ok_or(AgentFailure::NotFound)?;
                let configured = owners
                    .connections
                    .configure_source(
                        &actor,
                        Uuid::new_v4(),
                        source.source_ref,
                        source_review.review_ref,
                        vec![resource.resource_ref],
                        source.revision,
                        &host_scope(Uuid::new_v4(), Cancellation::new(), Duration::from_secs(30)),
                    )
                    .await
                    .map_err(|failure| failure.into_failure())?;
                let configured = match configured {
                    floe_connections::SourceConfigurationResult::Configured { source } => source,
                    floe_connections::SourceConfigurationResult::NotSavedReviewRequired {
                        ..
                    } => {
                        return Err(AgentFailure::AccessReviewRequired);
                    }
                };
                let observe_review = owners
                    .connections
                    .prepare_observe_review(
                        &actor,
                        Uuid::new_v4(),
                        configured.source_ref,
                        configured.revision,
                        floe_connections::ProcessingChoice::DeviceOnly,
                        &host_scope(Uuid::new_v4(), Cancellation::new(), Duration::from_secs(30)),
                    )
                    .await
                    .map_err(|failure| failure.into_failure())?;
                owners
                    .connections
                    .apply_observe(
                        &actor,
                        Uuid::new_v4(),
                        configured.source_ref,
                        configured.revision,
                        observe_review.review_ref,
                        floe_connections::ObserveDecision::Allow,
                        &host_scope(Uuid::new_v4(), Cancellation::new(), Duration::from_secs(30)),
                    )
                    .await
                    .map_err(|failure| failure.into_failure())
            })
            .expect("configure synthetic Calendar through real Connections and Access owners")
    })
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
pub fn disconnect_fixture_calendar(
    host: &AppHost<AppComposition>,
    source: &floe_connections::SourceSummary,
) -> floe_connections::ConnectionOperationSnapshot {
    let source_ref = source.source_ref;
    let revision = source.revision;
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                owners
                    .connections
                    .disconnect(
                        &actor,
                        Uuid::new_v4(),
                        source_ref,
                        revision,
                        &host_scope(Uuid::new_v4(), Cancellation::new(), Duration::from_secs(30)),
                    )
                    .await
                    .map_err(|failure| failure.into_failure())
            })
            .expect("revoke the fixture calendar's Access grants through Connections/Access")
    })
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
pub fn bind_schedule_expert(host: &AppHost<AppComposition>) {
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services
            .execute_owner(async move {
                let scope =
                    || host_scope(Uuid::new_v4(), Cancellation::new(), Duration::from_secs(30));
                let directory = owners.experts.directory(&actor, &scope()).await?;
                let assignment = directory
                    .assignments
                    .iter()
                    .find(|assignment| {
                        assignment.display_name == "Schedule Expert" && assignment.enabled
                    })
                    .ok_or(AgentFailure::NotFound)?;
                let requirement = assignment
                    .requirements
                    .iter()
                    .find(|requirement| requirement.requirement_ref == "floe.source.calendar")
                    .ok_or(AgentFailure::NotFound)?;
                let review = owners
                    .experts
                    .prepare_binding_review(
                        &actor,
                        CommandId::from_uuid(Uuid::new_v4()).ok_or(AgentFailure::InvalidInput)?,
                        assignment.assignment_ref,
                        requirement.requirement_ref.clone(),
                        assignment.binding_revision,
                        &scope(),
                    )
                    .await
                    .map_err(floe_kernel::CommandFailure::into_failure)?;
                let candidate = review
                    .candidate_refs_and_labels
                    .iter()
                    .find(|candidate| {
                        candidate.label == "Synthetic QA Calendar"
                            && candidate.availability
                                == floe_experts::CandidateAvailability::Available
                    })
                    .ok_or(AgentFailure::NotFound)?;
                owners
                    .experts
                    .replace_binding(
                        &actor,
                        CommandId::from_uuid(Uuid::new_v4()).ok_or(AgentFailure::InvalidInput)?,
                        review.review_ref,
                        review.binding_revision,
                        vec![candidate.candidate_ref],
                        &scope(),
                    )
                    .await
                    .map_err(floe_kernel::CommandFailure::into_failure)?;
                Ok::<(), AgentFailure>(())
            })
            .expect("bind Schedule Expert through the real Experts owner")
    });
}

pub fn start_session(host: &AppHost<AppComposition>) -> Uuid {
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let command_id = CommandId::from_uuid(Uuid::new_v4()).expect("random command id");
        let scope = host_scope(command_id.as_uuid(), Cancellation::new(), OWNER_TIMEOUT);
        services
            .execute_owner(async move {
                match owners
                    .conversation
                    .start_session(&actor, command_id, &scope)
                    .await
                    .map_err(|failure| match failure {
                        floe_conversation::SessionStartFailure::NotAdmitted(reason)
                        | floe_conversation::SessionStartFailure::Indeterminate(reason) => reason,
                    })? {
                    SessionStartAdmission::Started(receipt)
                    | SessionStartAdmission::Replayed(receipt) => Ok(receipt.session_id),
                    SessionStartAdmission::NotApplied(refusal) => Err(refusal.reason()),
                }
            })
            .expect("start real Conversation session")
    })
}

pub fn start_turn(
    host: &AppHost<AppComposition>,
    session_id: Uuid,
    text: impl Into<String>,
) -> CommandReceipt {
    let text = text.into();
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let session_scope = host_scope(Uuid::new_v4(), Cancellation::new(), OWNER_TIMEOUT);
        let session = services
            .execute_owner({
                let owner = owners.conversation.clone();
                let actor = actor.clone();
                async move {
                    owner
                        .get_session(&actor, session_id, None, &session_scope)
                        .await
                }
            })
            .expect("read real Conversation session");
        let command_id = CommandId::from_uuid(Uuid::new_v4()).expect("random command id");
        let scope = host_scope(command_id.as_uuid(), Cancellation::new(), OWNER_TIMEOUT);
        services
            .execute_owner(async move {
                owners
                    .conversation
                    .start_turn(
                        &actor,
                        StartTurn {
                            command_id,
                            session_id,
                            expected_revision: session.revision,
                            text,
                            continuation_ref: None,
                            retry_of: None,
                        },
                        &scope,
                    )
                    .await
            })
            .expect("admit real Conversation turn")
    })
}

pub fn cancel_run(host: &AppHost<AppComposition>, run_id: RunId) {
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let command_id = CommandId::from_uuid(Uuid::new_v4()).expect("random command id");
        let scope = host_scope(command_id.as_uuid(), Cancellation::new(), OWNER_TIMEOUT);
        services
            .execute_owner(async move {
                owners
                    .conversation
                    .cancel_run(&actor, command_id, run_id, &scope)
                    .await
            })
            .expect("cancel real Conversation run");
    });
}

pub fn read_run(host: &AppHost<AppComposition>, run_id: RunId) -> Option<RunReceipt> {
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = host_scope(Uuid::new_v4(), Cancellation::new(), OWNER_TIMEOUT);
        services
            .execute_owner(
                async move { owners.conversation.read_run(&actor, run_id, &scope).await },
            )
            .expect("read persisted Conversation receipt")
    })
}

pub fn wait_terminal_run(host: &AppHost<AppComposition>, run_id: RunId) -> RunReceipt {
    let deadline = Instant::now() + PREPARATION_TIMEOUT;
    loop {
        if let Some(receipt) = read_run(host, run_id)
            && receipt.state.is_terminal()
        {
            return receipt;
        }
        assert!(
            Instant::now() < deadline,
            "Conversation run exceeded deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub fn read_session(host: &AppHost<AppComposition>, session_id: Uuid) -> SessionSnapshot {
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = host_scope(Uuid::new_v4(), Cancellation::new(), OWNER_TIMEOUT);
        services
            .execute_owner(async move {
                owners
                    .conversation
                    .get_session(&actor, session_id, None, &scope)
                    .await
            })
            .expect("read persisted Conversation transcript")
    })
}

pub fn read_session_before(
    host: &AppHost<AppComposition>,
    session_id: Uuid,
    before_message_id: Uuid,
) -> SessionSnapshot {
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = host_scope(Uuid::new_v4(), Cancellation::new(), OWNER_TIMEOUT);
        services
            .execute_owner(async move {
                owners
                    .conversation
                    .get_session(&actor, session_id, Some(before_message_id), &scope)
                    .await
            })
            .expect("read normalized Conversation history before public cursor")
    })
}

pub fn read_events(host: &AppHost<AppComposition>) -> EventRead {
    with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        let scope = host_scope(Uuid::new_v4(), Cancellation::new(), OWNER_TIMEOUT);
        let request = ReadConversationEvents {
            runtime_epoch: Some(actor.runtime_epoch),
            cursor: Some(0),
            limit: 128,
        };
        services
            .execute_owner(async move {
                owners
                    .conversation
                    .read_events(&actor, request, &scope)
                    .await
            })
            .expect("read Conversation owner events")
    })
}

#[cfg(all(feature = "qa-fixtures", target_os = "linux"))]
pub fn read_vault_conversation_journal(
    host: &AppHost<AppComposition>,
    run_id: RunId,
) -> Vec<floe_vault::VaultConversationJournalEntry> {
    with_ready(host, |services, caller, _| {
        services
            .qa_conversation_journal(caller, run_id)
            .expect("read encrypted Conversation journal through App QA")
    })
}

pub fn terminal_run_event(events: EventRead, run_id: RunId) -> Option<RunEventRecord> {
    let EventRead::Events { events, .. } = events else {
        return None;
    };
    events.into_iter().find_map(|event| match event.payload {
        EventPayload::RunUpdated(run) if run.run_id == run_id && run.state.is_terminal() => {
            Some(run)
        }
        _ => None,
    })
}

pub fn assert_script_clean(recorder: &ScriptRecorder) -> ScriptSnapshot {
    let snapshot = recorder.snapshot();
    assert!(
        snapshot.violations.is_empty(),
        "script violations: {:?}",
        snapshot.violations
    );
    snapshot
}
