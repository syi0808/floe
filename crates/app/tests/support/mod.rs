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
    VaultLifecycleCommand, VaultLifecycleCommands, VaultLifecycleQueries, VaultState, host_scope,
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
const LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(55);
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanStage {
    Primary,
    LocalFallback,
}

#[derive(Clone, Debug)]
pub struct ScriptSnapshot {
    pub plans: Vec<(PlanStage, ModelPlanRequest)>,
    pub generated: Vec<InferenceRequest>,
    pub violations: Vec<String>,
}

#[derive(Default)]
struct RecorderState {
    plans: Vec<(PlanStage, ModelPlanRequest)>,
    generated: Vec<InferenceRequest>,
    violations: Vec<String>,
    identity: Option<(String, String)>,
    catalog: Option<AllowedCatalog>,
}

#[derive(Clone, Default)]
pub struct ScriptRecorder(Arc<Mutex<RecorderState>>);

impl ScriptRecorder {
    fn record_plan(
        &self,
        stage: PlanStage,
        request: &ModelPlanRequest,
    ) -> Result<(), ModelObservationError> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| ModelObservationError::StorageUnavailable)?;
        let expected = match state.plans.len() {
            0 => PlanStage::Primary,
            1 => PlanStage::LocalFallback,
            _ => {
                state
                    .violations
                    .push("unexpected additional model plan observation".into());
                return Err(ModelObservationError::InvalidIdentity);
            }
        };
        if stage != expected
            || request.purpose != CONVERSATION_PURPOSE
            || request.consumer != CONVERSATION_CONSUMER
            || request.required_capabilities.validate().is_err()
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
        Ok(())
    }

    fn record_violation(&self, message: impl Into<String>) {
        if let Ok(mut state) = self.0.lock() {
            state.violations.push(message.into());
        }
    }

    fn record_generate(&self, request: &InferenceRequest) -> Result<(), AgentFailure> {
        let mut state = self.0.lock().map_err(|_| AgentFailure::Interrupted)?;
        if state.generated.len() >= 1 {
            state
                .violations
                .push("unexpected additional model generate call".into());
            return Err(AgentFailure::InvalidInput);
        }
        state.generated.push(request.clone());
        Ok(())
    }

    pub fn snapshot(&self) -> ScriptSnapshot {
        let state = self.0.lock().expect("script recorder mutex poisoned");
        ScriptSnapshot {
            plans: state.plans.clone(),
            generated: state.generated.clone(),
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
        Self {
            primary,
            expected_input: expected_input.into(),
            output,
            recorder: ScriptRecorder::default(),
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
        if plan.purpose != CONVERSATION_PURPOSE
            || plan.consumer != CONVERSATION_CONSUMER
            || request.envelope.run_instructions.purpose != CONVERSATION_PURPOSE
            || !request.catalog.tools.is_empty()
            || capabilities != plan.required_capabilities
            || expected_user_entries != 1
        {
            self.recorder.record_violation(
                "canonical request did not match expected input, purpose, or Manager catalog",
            );
            return Err(AgentFailure::InvalidInput);
        }
        let mut state = self
            .recorder
            .0
            .lock()
            .map_err(|_| AgentFailure::Interrupted)?;
        if let Some(expected_catalog) = &state.catalog {
            if expected_catalog != &request.catalog {
                state
                    .violations
                    .push("canonical request changed the expected Manager catalog".into());
                return Err(AgentFailure::InvalidInput);
            }
        } else {
            state.catalog = Some(request.catalog.clone());
        }
        Ok(())
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
        _: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<PrimaryObservation<Box<dyn PreparedModelTransport>>, ModelObservationError>,
    > {
        let model = self.model.clone();
        Box::pin(async move {
            model.recorder.record_plan(PlanStage::Primary, request)?;
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
            model
                .recorder
                .record_plan(PlanStage::LocalFallback, request)?;
            let binding_digest = device_binding(request)?;
            let purpose = ModelPurpose::new(request.purpose.clone())
                .ok_or(ModelObservationError::InvalidIdentity)?;
            let consumer = floe_inference::ModelConsumer::new(request.consumer.clone())
                .ok_or(ModelObservationError::InvalidIdentity)?;
            let transport = ScriptedTransport {
                model: model.clone(),
                plan: request.clone(),
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
            self.model.recorder.record_generate(&request)?;
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

pub fn activate_vault(
    host: &AppHost<AppComposition>,
    command: VaultLifecycleCommand,
) -> VaultState {
    let request = host
        .request(Uuid::new_v4())
        .expect("admit Vault lifecycle request");
    let services = request.services();
    let caller = request.caller();
    let operation_id = Uuid::new_v4();
    services
        .vault_command(caller, operation_id, command)
        .expect("admit Vault lifecycle command");
    let deadline = Instant::now() + LIFECYCLE_TIMEOUT;
    loop {
        let result = services
            .read_vault_result(caller, operation_id, false)
            .expect("read Vault lifecycle result");
        if result.done {
            let completed = services
                .read_vault_result(caller, operation_id, true)
                .expect("release completed Vault lifecycle receipt");
            assert_eq!(completed.failure, None);
            return completed.state.expect("activated Vault state");
        }
        assert!(
            Instant::now() < deadline,
            "Vault activation exceeded deadline"
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
    let deadline = Instant::now() + LIFECYCLE_TIMEOUT;
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
