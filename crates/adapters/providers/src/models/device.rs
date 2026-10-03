//! Authorized reasoning translated to the domain-neutral native DeviceModel contract.
use std::time::Duration;
use floe_agent_contract::{AgentFailure, BoxFuture, ModelBindingDigest, ModelCapabilities, ModelCapability,
    ModelPlanRequest, ModelStep, SessionProtection};
use floe_execution::ExecutionScope;
use floe_inference::{AdmittedDispatchTarget, CanonicalModelRequest, CanonicalModelResponse,
    LocalAvailabilityReason, LocalObservation, ModelObservationError, ObservedModelCapability,
    PreparedModelProfile, PreparedModelTransport, ProviderUsageObservation};
use floe_model_contract::{DeviceModelCapability, DeviceModelCommand, DeviceModelFailure, DeviceModelObservation,
    DeviceModelOutput, DeviceModelProfile, DeviceModelReply, DeviceModelRequest, DeviceModelRequirements,
    DeviceModelUnavailable, DeviceTool, ModelSchema, DEVICE_MODEL_VERSION, MAX_DEVICE_REQUEST_BYTES,
    MAX_DEVICE_RESPONSE_BYTES};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::time::Instant;

// Retains the conservative admission bound of the existing device reasoning path.
const MIN_REASONING_TOKEN_RESERVATION: u64 = 4096;
static DEVICE_MODEL: floe_native::ByteCall = floe_native::ByteCall::new(floe_native::NativeLibrary {
    relative_path: "Frameworks/libfloe_local_model.dylib",
    invoke_symbol: c"floe_device_model", release_symbol: c"floe_device_model_free",
    #[cfg(target_os = "macos")]
    bundle_parents: floe_native::MACOS_BUNDLE_ROOT,
    #[cfg(not(target_os = "macos"))]
    bundle_parents: floe_native::BUNDLE_SIBLING,
});
fn invoke(command: &DeviceModelCommand) -> Result<DeviceModelReply, AgentFailure> {
    let input = serde_json::to_vec(command).map_err(|_| AgentFailure::InvalidInput)?;
    if input.len() > MAX_DEVICE_REQUEST_BYTES { return Err(AgentFailure::BudgetExceeded); }
    let output = DEVICE_MODEL.call(&input, MAX_DEVICE_RESPONSE_BYTES).map_err(|error| match error {
        floe_native::NativeCallError::ResponseTooLarge => AgentFailure::BudgetExceeded,
        floe_native::NativeCallError::InvalidRequest => AgentFailure::InvalidInput,
        _ => AgentFailure::LocalModelUnavailable,
    })?;
    DeviceModelReply::decode(&output).map_err(|_| AgentFailure::LocalModelInvalidOutput)
}
fn failure(failure: DeviceModelFailure) -> AgentFailure {
    match failure {
        DeviceModelFailure::Unsupported | DeviceModelFailure::Disabled | DeviceModelFailure::NotReady | DeviceModelFailure::Unavailable => AgentFailure::LocalModelUnavailable,
        DeviceModelFailure::InvalidInput => AgentFailure::InvalidInput,
        DeviceModelFailure::InvalidOutput => AgentFailure::LocalModelInvalidOutput,
        DeviceModelFailure::DeadlineExceeded => AgentFailure::DeadlineExceeded,
        DeviceModelFailure::Cancelled => AgentFailure::Cancelled,
        DeviceModelFailure::PolicyDenied => AgentFailure::PolicyDenied,
        DeviceModelFailure::QuotaExceeded | DeviceModelFailure::Busy => AgentFailure::QuotaExceeded,
        DeviceModelFailure::Conflict => AgentFailure::Conflict,
        DeviceModelFailure::NotFound => AgentFailure::NotFound,
    }
}
fn requirements(request: &ModelPlanRequest) -> DeviceModelRequirements {
    let mut capabilities = request.required_capabilities.0.iter().map(|capability| match capability {
        ModelCapability::Chat => DeviceModelCapability::Text,
        ModelCapability::StructuredOutput => DeviceModelCapability::StructuredOutput,
        ModelCapability::ToolProposals => DeviceModelCapability::ToolProposals,
    }).collect::<Vec<_>>();
    capabilities.sort(); capabilities.dedup();
    DeviceModelRequirements { capabilities }
}
fn model_capabilities(profile: &DeviceModelProfile) -> ModelCapabilities {
    let mut capabilities = Vec::new();
    if profile.capabilities.contains(&DeviceModelCapability::Text) { capabilities.push(ModelCapability::Chat); }
    if profile.capabilities.contains(&DeviceModelCapability::StructuredOutput) { capabilities.push(ModelCapability::StructuredOutput); }
    if profile.capabilities.contains(&DeviceModelCapability::ToolProposals) { capabilities.push(ModelCapability::ToolProposals); }
    ModelCapabilities(capabilities)
}

pub struct DeviceModelProvider { protection: SessionProtection }
impl DeviceModelProvider {
    pub fn encrypted() -> Self { Self { protection: SessionProtection::Encrypted } }
    pub fn synthetic() -> Self { Self { protection: SessionProtection::SyntheticOnly } }
    pub fn observe_local_fallback<'a>(&'a self, request: &'a ModelPlanRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a,Result<LocalObservation<PreparedDeviceTransport>,ModelObservationError>>
    {
        Box::pin(async move {
            request.validate().map_err(|_| ModelObservationError::InvalidIdentity)?;
            if scope.cancellation().is_cancelled() { return Err(ModelObservationError::Cancelled); }
            if scope.deadline() <= Instant::now() { return Err(ModelObservationError::Timeout); }
            if !DEVICE_MODEL.available() { return Ok(LocalObservation::Unavailable(LocalAvailabilityReason::Unsupported)); }
            let requested = requirements(request);
            requested.validate().map_err(|_| ModelObservationError::InvalidIdentity)?;
            let observation = match invoke(&DeviceModelCommand::Prepare { schema_version:DEVICE_MODEL_VERSION, requirements:requested }) {
                Ok(DeviceModelReply::Observation { observation, .. }) => observation,
                Ok(DeviceModelReply::Error { operation_id:None, failure:reason, .. }) => return Err(observation_failure(failure(reason))),
                Ok(_) => return Err(ModelObservationError::InvalidInventory),
                Err(error) => return Err(observation_failure(error)),
            };
            let profile = match observation {
                DeviceModelObservation::Available { binding_id, capabilities, limits } => DeviceModelProfile { binding_id, capabilities, limits },
                DeviceModelObservation::Unavailable { reason } => return Ok(LocalObservation::Unavailable(match reason {
                    DeviceModelUnavailable::Unsupported => LocalAvailabilityReason::Unsupported,
                    DeviceModelUnavailable::Disabled => LocalAvailabilityReason::Disabled,
                    DeviceModelUnavailable::NotReady => LocalAvailabilityReason::NotReady,
                })),
            };
            profile.validate().map_err(|_| ModelObservationError::InvalidInventory)?;
            let capabilities = model_capabilities(&profile);
            capabilities.validate().map_err(|_| ModelObservationError::InvalidInventory)?;
            if !capabilities.includes(&request.required_capabilities) { return Err(ModelObservationError::PermissionDenied); }
            let bytes = serde_json::to_vec(&("floe.device-model.binding",DEVICE_MODEL_VERSION,&request.principal,&request.device_id,&profile))
                .map_err(|_| ModelObservationError::InvalidIdentity)?;
            let binding_digest = ModelBindingDigest(Sha256::digest(bytes).into());
            Ok(LocalObservation::Available(PreparedModelProfile {
                capability: ObservedModelCapability { purpose:floe_inference::ModelPurpose::new(request.purpose.clone()).ok_or(ModelObservationError::InvalidIdentity)?,
                    consumer:floe_inference::ModelConsumer::new(request.consumer.clone()).ok_or(ModelObservationError::InvalidIdentity)?,
                    capabilities, boundary:floe_agent_contract::ProcessingBoundary::Device, binding_digest },
                transport: PreparedDeviceTransport { protection:self.protection, profile, binding_digest },
            }))
        })
    }
}
fn observation_failure(error: AgentFailure) -> ModelObservationError {
    match error {
        AgentFailure::Cancelled => ModelObservationError::Cancelled,
        AgentFailure::DeadlineExceeded => ModelObservationError::Timeout,
        AgentFailure::PolicyDenied => ModelObservationError::PermissionDenied,
        AgentFailure::QuotaExceeded => ModelObservationError::QuotaExceeded,
        AgentFailure::LocalModelInvalidOutput | AgentFailure::InvalidInput => ModelObservationError::InvalidInventory,
        _ => ModelObservationError::TransportUnavailable,
    }
}
pub struct PreparedDeviceTransport { protection:SessionProtection, profile:DeviceModelProfile, binding_digest:ModelBindingDigest }
impl PreparedDeviceTransport {
    fn render_request(&self, request: &CanonicalModelRequest) -> Result<DeviceModelRequest,AgentFailure> {
        request.validate()?;
        match self.protection {
            SessionProtection::KeyUnavailable => return Err(AgentFailure::VaultUnavailable),
            SessionProtection::SyntheticOnly if request.input_data_classes.iter().any(|class| *class != floe_agent_contract::DataClass::Synthetic) => return Err(AgentFailure::VaultUnavailable),
            _ => {}
        }
        if request.remaining_tokens < MIN_REASONING_TOKEN_RESERVATION { return Err(AgentFailure::BudgetExceeded); }
        check_deadline(&request)?;
        let encoded = super::agent_codec::encode_agent_input(&request)?;
        let tools = encoded["tools"].as_array().ok_or(AgentFailure::InvalidInput)?.iter().map(|tool| {
            let function=&tool["function"];
            Ok(DeviceTool { name:function["name"].as_str().ok_or(AgentFailure::InvalidInput)?.into(),
                description:function["description"].as_str().ok_or(AgentFailure::InvalidInput)?.into(),
                input_schema:ModelSchema::new(function["parameters"].clone()).map_err(|_| AgentFailure::InvalidInput)? })
        }).collect::<Result<Vec<_>,AgentFailure>>()?;
        let input=json!({"messages":encoded["messages"]});
        let milliseconds=request.deadline.saturating_duration_since(Instant::now()).as_millis();
        if milliseconds == 0 { return Err(AgentFailure::DeadlineExceeded); }
        let native=DeviceModelRequest { operation_id:request.attempt_id,binding_id:self.profile.binding_id.clone(),
            instructions:request.envelope.stable_instructions.render(),input,output_format:request.envelope.run_instructions.output_format.clone(),tools,
            max_response_tokens:self.profile.limits.max_response_tokens.min(request.remaining_tokens.min(u32::MAX as u64) as u32),
            max_output_bytes:request.max_output_bytes.min(self.profile.limits.max_output_bytes),
            deadline_milliseconds:milliseconds.min(self.profile.limits.max_deadline_milliseconds as u128) as u32 };
        self.profile.validate_request(&native).map_err(|_| AgentFailure::InvalidInput)?;
        Ok(native)
    }
}
impl PreparedModelTransport for PreparedDeviceTransport {
    fn validate_request(&self, request: &CanonicalModelRequest) -> Result<(),AgentFailure> { self.render_request(request).map(|_| ()) }
    fn dispatch_target(&self) -> floe_access::ModelDispatchTarget { floe_access::ModelDispatchTarget::Device }
    fn generate<'a>(&'a self, request:CanonicalModelRequest, target:AdmittedDispatchTarget) -> BoxFuture<'a,Result<CanonicalModelResponse,AgentFailure>> {
        Box::pin(async move {
            if !target.matches(&self.binding_digest,floe_agent_contract::ProcessingBoundary::Device) { return Err(AgentFailure::PolicyDenied); }
            let native=self.render_request(&request)?;
            let _lease=NativeLease(request.attempt_id);
            let mut command=DeviceModelCommand::Start { schema_version:DEVICE_MODEL_VERSION,request:native.clone() };
            loop {
                check_deadline(&request)?;
                let reply=invoke(&command)?;
                match reply {
                    DeviceModelReply::Pending { operation_id,.. } if operation_id==request.attempt_id => {
                        check_deadline(&request)?;
                        tokio::time::sleep_until((Instant::now()+Duration::from_millis(20)).min(request.deadline)).await;
                        command=DeviceModelCommand::Poll { schema_version:DEVICE_MODEL_VERSION,operation_id:request.attempt_id };
                    }
                    DeviceModelReply::Error { operation_id:Some(operation_id),failure:reason,.. } if operation_id==request.attempt_id => return Err(failure(reason)),
                    DeviceModelReply::Done { response,.. } => {
                        response.validate_envelope(&native).map_err(|_| AgentFailure::LocalModelInvalidOutput)?;
                        let usage=ProviderUsageObservation {tokens:response.usage.tokens,cost_micros:response.usage.cost_micros};
                        let output=check_deadline(&request).and_then(|_| {
                            response.validate_output(&native).map_err(|_| AgentFailure::LocalModelInvalidOutput)?;
                            decode_output(response.output,&request)
                        });
                        return Ok(CanonicalModelResponse {output,usage});
                    }
                    _ => return Err(AgentFailure::LocalModelInvalidOutput),
                }
            }
        })
    }
}
fn decode_output(output:DeviceModelOutput, request:&CanonicalModelRequest) -> Result<Vec<ModelStep>,AgentFailure> {
    let step=match output {
        DeviceModelOutput::Text {text} => ModelStep::Answer {text,artifacts:vec![]},
        DeviceModelOutput::Json {value} => ModelStep::Answer {text:serde_json::to_string(&value).map_err(|_| AgentFailure::LocalModelInvalidOutput)?,artifacts:vec![]},
        DeviceModelOutput::ToolProposal {name,input} => super::agent_codec::decode_agent_step(
            super::agent_codec::WireStep::Call {capability_id:name,input:serde_json::to_string(&input).map_err(|_| AgentFailure::LocalModelInvalidOutput)?},&request.catalog)
            .map_err(|error| if error==AgentFailure::ServerModelInvalidOutput {AgentFailure::LocalModelInvalidOutput} else {error})?,
        DeviceModelOutput::Failure {failure:reason} => return Err(failure(reason)),
    };
    Ok(vec![step])
}
struct NativeLease(uuid::Uuid);
impl Drop for NativeLease {
    fn drop(&mut self) { let _=invoke(&DeviceModelCommand::Release {schema_version:DEVICE_MODEL_VERSION,operation_id:self.0}); }
}
fn check_deadline(request:&CanonicalModelRequest) -> Result<(),AgentFailure> {
    if request.cancellation.is_cancelled() { Err(AgentFailure::Cancelled) }
    else if request.deadline<=Instant::now() { Err(AgentFailure::DeadlineExceeded) } else { Ok(()) }
}
