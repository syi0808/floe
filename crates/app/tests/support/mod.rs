use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use floe_agent_contract::{
    AgentFailure, AllowedCatalog, BoxFuture, ModelCapabilities, ModelPlanRequest, ModelStep,
    ProcessingBoundary,
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

const OWNER_TIMEOUT: Duration = Duration::from_secs(15);
const PREPARATION_TIMEOUT: Duration = Duration::from_secs(55);
const SCRIPTED_REPLY: &str = "A deterministic scripted reply.";

#[derive(Clone, Copy)]
pub enum PrimaryBehavior {
    NoGateway,
    Fail(ModelObservationError),
}

#[derive(Clone)]
pub enum ModelOutput {
    Answer,
    UnsupportedManagerToolCall,
    WaitForCancellation(Arc<GenerateBarrier>),
    ScheduleExpertFlow,
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
    schedule_flow: bool,
}

#[derive(Clone)]
pub struct ScriptRecorder(Arc<Mutex<RecorderState>>);

impl ScriptRecorder {
    fn new(schedule_flow: bool) -> Self {
        Self(Arc::new(Mutex::new(RecorderState {
            schedule_flow,
            ..RecorderState::default()
        })))
    }

    fn record_plan(
        &self,
        stage: PlanStage,
        request: &ModelPlanRequest,
        run_id: Option<Uuid>,
        task_id: Option<Uuid>,
    ) -> Result<(), ModelObservationError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| ModelObservationError::StorageUnavailable)?;
        let expected = if state.schedule_flow {
            match state.plans.len() {
                0 => Some((PlanStage::Primary, CONVERSATION_CONSUMER)),
                1 => Some((PlanStage::LocalFallback, CONVERSATION_CONSUMER)),
                2 => Some((
                    PlanStage::Primary,
                    floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER,
                )),
                3 => Some((
                    PlanStage::LocalFallback,
                    floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER,
                )),
                4 => Some((
                    PlanStage::Primary,
                    floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER,
                )),
                5 => Some((
                    PlanStage::LocalFallback,
                    floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER,
                )),
                6 => Some((PlanStage::Primary, CONVERSATION_CONSUMER)),
                7 => Some((PlanStage::LocalFallback, CONVERSATION_CONSUMER)),
                _ => None,
            }
        } else {
            match state.plans.len() {
                0 => Some((PlanStage::Primary, CONVERSATION_CONSUMER)),
                1 => Some((PlanStage::LocalFallback, CONVERSATION_CONSUMER)),
                _ => None,
            }
        };
        let Some((expected_stage, expected_consumer)) = expected else {
            state
                .violations
                .push("unexpected additional model plan observation".into());
            return Err(ModelObservationError::InvalidIdentity);
        };
        if stage != expected_stage
            || request.consumer != expected_consumer
            || request.purpose != CONVERSATION_PURPOSE
            || request.required_capabilities.validate().is_err()
            || run_id.is_none()
            || (request.consumer == CONVERSATION_CONSUMER && task_id.is_some())
            || (request.consumer == floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER
                && task_id.is_none())
        {
            state
                .violations
                .push(format!("unexpected {stage:?} plan request: {request:?}"));
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
        state.plans.push((stage, request.clone()));
        state.plan_bindings.push(ScriptCallBinding {
            consumer: request.consumer.clone(),
            run_id,
            task_id,
            attempt_id: None,
        });
        Ok(())
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
        let expected = if state.schedule_flow {
            [
                CONVERSATION_CONSUMER,
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER,
                floe_experts::DELEGATED_EXPERT_INFERENCE_CONSUMER,
                CONVERSATION_CONSUMER,
            ]
            .get(state.generated.len())
            .copied()
        } else if state.generated.is_empty() {
            Some(CONVERSATION_CONSUMER)
        } else {
            None
        };
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
        let expected_plan_index = if state.schedule_flow {
            state.generated.len() * 2 + 1
        } else {
            1
        };
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
        Ok(index)
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
        let schedule_flow = matches!(&output, ModelOutput::ScheduleExpertFlow);
        Self {
            primary,
            expected_input: expected_input.into(),
            output,
            recorder: ScriptRecorder::new(schedule_flow),
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

    fn validate_canonical(
        &self,
        request: &InferenceRequest,
        plan: &ModelPlanRequest,
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
        let schedule_flow = matches!(&self.output, ModelOutput::ScheduleExpertFlow);
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
        if let Some(expected_catalog) = state.catalogs.get(&plan.consumer) {
            if expected_catalog != &request.catalog {
                state
                    .violations
                    .push("canonical request changed its consumer catalog".into());
                return Err(AgentFailure::InvalidInput);
            }
        } else {
            state
                .catalogs
                .insert(plan.consumer.clone(), request.catalog.clone());
        }
        Ok(())
    }

    fn schedule_flow_response(
        &self,
        call_index: usize,
        request: &InferenceRequest,
    ) -> Result<CanonicalModelResponse, AgentFailure> {
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
            1 => {
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
            2 => Ok(response(vec![ModelStep::Answer {
                text:
                    "The selected synthetic calendar has one planning event in the requested range."
                        .into(),
                artifacts: vec![],
            }])),
            3 => Ok(response(vec![ModelStep::Answer {
                text: SCRIPTED_REPLY.into(),
                artifacts: vec![],
            }])),
            _ => {
                self.recorder
                    .record_violation("unexpected scripted schedule flow generation index");
                Err(AgentFailure::InvalidInput)
            }
        }
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
            model.recorder.record_plan(
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
            model.recorder.record_plan(
                PlanStage::LocalFallback,
                request,
                scope.root_run_id().map(RunId::as_uuid),
                scope.task_id().map(|task_id| task_id.as_uuid()),
            )?;
            let binding_digest = device_binding(request)?;
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
                capabilities: request.required_capabilities.clone(),
            };
            let transport: Box<dyn PreparedModelTransport> = Box::new(transport);
            Ok(LocalObservation::Available(PreparedModelProfile {
                capability: ObservedModelCapability {
                    purpose,
                    consumer,
                    capabilities: request.required_capabilities.clone(),
                    boundary: ProcessingBoundary::Device,
                    binding_digest,
                },
                transport,
            }))
        })
    }
}

fn device_binding(
    request: &ModelPlanRequest,
) -> Result<floe_agent_contract::ModelBindingDigest, ModelObservationError> {
    use sha2::Digest;
    let mut input = Vec::from(b"floe.p1a.scripted-device-model\0".as_slice());
    input.extend_from_slice(request.principal.as_bytes());
    input.push(0);
    input.extend_from_slice(request.device_id.as_bytes());
    input.push(0);
    input.extend_from_slice(request.purpose.as_bytes());
    input.push(0);
    input.extend_from_slice(request.consumer.as_bytes());
    Ok(floe_agent_contract::ModelBindingDigest(
        sha2::Sha256::digest(input).into(),
    ))
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
        self.model.validate_canonical(request, &self.plan)
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
                || self.capabilities
                    != ModelCapabilities::for_request(
                        &request.envelope.run_instructions.output_format,
                        &request.catalog,
                    )?
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
                    .await?;
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
                    .await?;
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
        std::thread::yield_now();
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
