use std::time::Duration;

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::prompts::MAX_STABLE_INSTRUCTIONS_BYTES;
use floe_agent_contract::{AgentFailure, SessionProtection, valid_context_refs};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::time::Instant;
use uuid::Uuid;

const CONTEXT_RESERVATION: u64 = 4096;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalModelAvailability {
    Available,
    UnsupportedOs,
    UnsupportedProfile,
    DeviceNotEligible,
    AppleIntelligenceNotEnabled,
    ModelNotReady,
    ModelUnavailable,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reply {
    schema_version: u32,
    status: String,
    #[serde(rename = "requestID")]
    request_id: Option<Uuid>,
    availability: Option<LocalModelAvailability>,
    step: Option<WireStep>,
    failure: Option<AgentFailure>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireStep {
    kind: String,
    text: Option<String>,
    #[serde(rename = "capabilityID")]
    capability_id: Option<String>,
    input: Option<String>,
}

trait Transport: Sync {
    fn call(&self, request: Value) -> Result<Reply, AgentFailure>;
}

struct Lease<'transport, Connection: Transport> {
    connection: &'transport Connection,
    request_id: Uuid,
}

impl<Connection: Transport> Lease<'_, Connection> {
    fn command(&self, operation: &str) -> Value {
        json!({"schemaVersion": 1, "operation": operation, "requestID": self.request_id})
    }
}

impl<Connection: Transport> Drop for Lease<'_, Connection> {
    fn drop(&mut self) {
        let _ = self.connection.call(self.command("release"));
    }
}

/// Canonical deadline/cancellation fence.
fn check_canonical_deadline(
    request: &floe_inference::CanonicalModelRequest,
) -> Result<(), AgentFailure> {
    if request.cancellation.is_cancelled() {
        Err(AgentFailure::Cancelled)
    } else if request.deadline <= Instant::now() {
        Err(AgentFailure::DeadlineExceeded)
    } else {
        Ok(())
    }
}

/// Canonical native input from the immutable envelope and the bounded catalog.
///
/// Instructions render
/// from `stable_instructions`, the prompt renders from the envelope, and the
/// callable tools/delegates are exactly what the envelope already advertises
/// from the catalog. The session-protection boundary is preserved without
/// Access/Context judgment: only the protection level and the declared input
/// data classes participate.
fn prepare_canonical(
    request: &floe_inference::CanonicalModelRequest,
    protection: SessionProtection,
) -> Result<Value, AgentFailure> {
    request.validate()?;
    check_canonical_deadline(request)?;
    match protection {
        SessionProtection::KeyUnavailable => return Err(AgentFailure::VaultUnavailable),
        SessionProtection::SyntheticOnly
            if request
                .input_data_classes
                .iter()
                .any(|class| *class != floe_agent_contract::DataClass::Synthetic) =>
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        _ => {}
    }
    if request.input_data_classes.iter().any(|class| {
        matches!(
            class,
            floe_agent_contract::DataClass::Credential
                | floe_agent_contract::DataClass::DeviceOnlyRaw
        )
    }) {
        return Err(AgentFailure::PolicyDenied);
    }
    let instructions = request.envelope.stable_instructions.render();
    if instructions.len() > MAX_STABLE_INSTRUCTIONS_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    if request.remaining_tokens < CONTEXT_RESERVATION {
        return Err(AgentFailure::BudgetExceeded);
    }
    for tool in &request.catalog.tools {
        tool.validate()?;
    }
    for card in &request.catalog.cards {
        card.validate()?;
    }
    let envelope = &request.envelope;
    let prompt = super::wire::ModelFrames::from_envelope(envelope)?.foundation_prompt()?;
    if prompt.len() > 12288 {
        return Err(AgentFailure::BudgetExceeded);
    }
    let milliseconds = request
        .deadline
        .saturating_duration_since(Instant::now())
        .as_millis();
    if milliseconds == 0 {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(json!({
        "instructions": instructions,
        "prompt": prompt,
        "maxResponseTokens": 1024,
        "maxOutputBytes": request.max_output_bytes.min(16384),
        "deadlineMilliseconds": milliseconds.min(30000) as u64,
    }))
}

/// Canonical native generation over any `Transport`.
///
/// The native start/poll/release identity is the model attempt identity:
/// no second UUID is minted for the same attempt. Request preparation,
/// polling, deadline, release, failure mapping and step decoding share
/// that identity.
async fn generate_canonical(
    connection: &impl Transport,
    request: floe_inference::CanonicalModelRequest,
    protection: SessionProtection,
) -> Result<floe_inference::CanonicalModelResponse, AgentFailure> {
    let input = prepare_canonical(&request, protection)?;
    let lease = Lease {
        connection,
        request_id: request.attempt_id,
    };
    let mut command = lease.command("start");
    command["input"] = input;
    loop {
        check_canonical_deadline(&request)?;
        let reply = connection.call(command)?;
        if reply.schema_version != AGENT_VERSION || reply.request_id != Some(lease.request_id) {
            return Err(AgentFailure::InvalidModelOutput);
        }
        check_canonical_deadline(&request)?;
        match reply.status.as_str() {
            "pending" if reply.step.is_none() && reply.failure.is_none() => {
                tokio::time::sleep_until(
                    (Instant::now() + Duration::from_millis(20)).min(request.deadline),
                )
                .await;
                command = lease.command("poll");
            }
            "error" if reply.step.is_none() => {
                return Err(reply.failure.unwrap_or(AgentFailure::ModelUnavailable));
            }
            "done" if reply.failure.is_none() => {
                let step = decode_canonical_step(
                    reply.step.ok_or(AgentFailure::InvalidModelOutput)?,
                    &request,
                )?;
                return Ok(floe_inference::CanonicalModelResponse {
                    output: Ok(vec![step]),
                    usage: floe_inference::ProviderUsageObservation {
                        tokens: None,
                        cost_micros: Some(0),
                    },
                });
            }
            _ => return Err(AgentFailure::InvalidModelOutput),
        }
    }
}

/// Resolve one native wire step to its canonical agent step.
///
/// Tool and Expert identity resolve against the bounded catalog only, and the
/// definition revision is copied from the catalog entry the wire id matched.
/// Unknown ids fail closed; malformed input, context refs and oversized output
/// are rejected before anything returns to Inference.
fn decode_canonical_step(
    step: WireStep,
    request: &floe_inference::CanonicalModelRequest,
) -> Result<floe_agent_contract::ModelStep, AgentFailure> {
    let step = match (
        step.kind.as_str(),
        step.text,
        step.capability_id,
        step.input,
    ) {
        ("answer", Some(text), None, None) if !text.trim().is_empty() => {
            floe_agent_contract::ModelStep::Answer {
                text,
                artifacts: vec![],
            }
        }
        ("call", None, Some(capability_id), Some(input)) => {
            if capability_id == "floe.a2a.delegate" {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct DelegationInput {
                    agent_id: String,
                    message: String,
                    #[serde(default)]
                    context_refs: Vec<String>,
                }
                let delegation: DelegationInput =
                    serde_json::from_str(&input).map_err(|_| AgentFailure::InvalidModelOutput)?;
                if !valid_context_refs(&delegation.context_refs) {
                    return Err(AgentFailure::InvalidModelOutput);
                }
                let definition = request
                    .catalog
                    .cards
                    .iter()
                    .find(|definition| definition.card.id == delegation.agent_id)
                    .ok_or(AgentFailure::CapabilityDenied)?;
                if delegation.message.trim().is_empty() || delegation.message.len() > 4096 {
                    return Err(AgentFailure::InvalidModelOutput);
                }
                floe_agent_contract::ModelStep::Delegate {
                    agent_id: definition.card.id.clone(),
                    definition_revision: definition.definition_revision,
                    message: delegation.message,
                    context_refs: delegation.context_refs,
                }
            } else {
                let descriptor = request
                    .catalog
                    .tools
                    .iter()
                    .find(|tool| tool.id == capability_id)
                    .ok_or(AgentFailure::CapabilityDenied)?;
                if serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&input)
                    .is_err()
                {
                    return Err(AgentFailure::InvalidModelOutput);
                }
                floe_agent_contract::ModelStep::CallTool {
                    tool_id: descriptor.id.clone(),
                    definition_revision: descriptor.definition_revision,
                    input,
                }
            }
        }
        _ => return Err(AgentFailure::InvalidModelOutput),
    };
    if serde_json::to_vec(&step)
        .map_err(|_| AgentFailure::InvalidModelOutput)?
        .len()
        > request.max_output_bytes.min(16384)
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(step)
}

struct NativeTransport;

impl Transport for NativeTransport {
    fn call(&self, request: Value) -> Result<Reply, AgentFailure> {
        native_call(request)
    }
}

static LOCAL_MODEL: floe_native::ByteCall =
    floe_native::ByteCall::new(floe_native::NativeLibrary {
        relative_path: "Frameworks/libfloe_local_model.dylib",
        invoke_symbol: c"floe_local_model",
        release_symbol: c"floe_local_model_free",
        #[cfg(target_os = "macos")]
        bundle_parents: floe_native::MACOS_BUNDLE_ROOT,
        #[cfg(not(target_os = "macos"))]
        bundle_parents: floe_native::BUNDLE_SIBLING,
    });

const MAX_LOCAL_MODEL_BYTES: usize = 32_768;

fn native_call(request: Value) -> Result<Reply, AgentFailure> {
    let input = serde_json::to_vec(&request).map_err(|_| AgentFailure::InvalidInput)?;
    if input.len() > MAX_LOCAL_MODEL_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    let output = LOCAL_MODEL
        .call(&input, MAX_LOCAL_MODEL_BYTES)
        .map_err(local_model_failure)?;
    serde_json::from_slice(&output).map_err(|_| AgentFailure::InvalidModelOutput)
}

fn local_model_failure(error: floe_native::NativeCallError) -> AgentFailure {
    match error {
        floe_native::NativeCallError::ResponseTooLarge => AgentFailure::BudgetExceeded,
        floe_native::NativeCallError::InvalidRequest => AgentFailure::InvalidInput,
        _ => AgentFailure::ModelUnavailable,
    }
}

/// Canonical device-only provider. Observes one stable non-secret profile;
/// credentials never leave the prepared transport.
///
/// The observed scope is composition configuration: the root observes the
/// canonical root scope, while domain callers (Experts, Learner) observe
/// their own purpose/consumer through the same device model.
pub struct FoundationModelProvider {
    protection: SessionProtection,
}

impl FoundationModelProvider {
    pub fn availability(&self) -> Result<LocalModelAvailability, AgentFailure> {
        let reply =
            NativeTransport.call(json!({"schemaVersion": 1, "operation": "availability"}))?;
        if reply.schema_version != AGENT_VERSION || reply.status != "availability" {
            return Err(AgentFailure::InvalidModelOutput);
        }
        reply.availability.ok_or(AgentFailure::InvalidModelOutput)
    }

    pub fn synthetic() -> Self {
        Self {
            protection: SessionProtection::SyntheticOnly,
        }
    }
    pub fn encrypted() -> Self {
        Self {
            protection: SessionProtection::Encrypted,
        }
    }
}

pub struct PreparedFoundationTransport {
    protection: SessionProtection,
    binding_digest: floe_agent_contract::ModelBindingDigest,
}
impl floe_inference::PreparedModelTransport for PreparedFoundationTransport {
    fn dispatch_target(&self) -> floe_access::ModelDispatchTarget {
        floe_access::ModelDispatchTarget::Device
    }
    fn generate<'a>(
        &'a self,
        request: floe_inference::CanonicalModelRequest,
        target: floe_inference::AdmittedDispatchTarget,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<floe_inference::CanonicalModelResponse, AgentFailure>,
    > {
        Box::pin(async move {
            if !target.matches(
                &self.binding_digest,
                floe_agent_contract::ProcessingBoundary::Device,
            ) {
                return Err(AgentFailure::PolicyDenied);
            }
            generate_canonical(&NativeTransport, request, self.protection)
                .await
                .map_err(map_canonical_failure)
        })
    }
}
fn map_canonical_failure(failure: AgentFailure) -> AgentFailure {
    match failure {
        AgentFailure::ModelUnavailable => AgentFailure::LocalModelUnavailable,
        AgentFailure::InvalidModelOutput => AgentFailure::LocalModelInvalidOutput,
        failure => failure,
    }
}
impl floe_inference::ModelProvider for FoundationModelProvider {
    type Prepared = PreparedFoundationTransport;
    fn observe_primary<'a>(
        &'a self,
        _request: &'a floe_agent_contract::ModelPlanRequest,
        _scope: &'a floe_execution::ExecutionScope,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<
            floe_inference::PrimaryObservation<Self::Prepared>,
            floe_inference::ModelObservationError,
        >,
    > {
        Box::pin(async { Err(floe_inference::ModelObservationError::InvalidIdentity) })
    }
    fn observe_local_fallback<'a>(
        &'a self,
        request: &'a floe_agent_contract::ModelPlanRequest,
        scope: &'a floe_execution::ExecutionScope,
    ) -> floe_agent_contract::BoxFuture<
        'a,
        Result<
            floe_inference::LocalObservation<Self::Prepared>,
            floe_inference::ModelObservationError,
        >,
    > {
        Box::pin(async move {
            use floe_inference::{
                LocalAvailabilityReason, LocalObservation, ModelObservationError,
            };
            use sha2::{Digest, Sha256};
            request
                .validate()
                .map_err(|_| ModelObservationError::InvalidIdentity)?;
            if scope.cancellation().is_cancelled() {
                return Err(ModelObservationError::Cancelled);
            }
            if scope.deadline() <= tokio::time::Instant::now() {
                return Err(ModelObservationError::Timeout);
            }
            if !LOCAL_MODEL.available() {
                return Ok(LocalObservation::Unavailable(
                    LocalAvailabilityReason::Unsupported,
                ));
            }
            let availability = self
                .availability()
                .map_err(|_| ModelObservationError::TransportUnavailable)?;
            let reason = match availability {
                LocalModelAvailability::Available => None,
                LocalModelAvailability::UnsupportedOs
                | LocalModelAvailability::UnsupportedProfile
                | LocalModelAvailability::DeviceNotEligible => {
                    Some(LocalAvailabilityReason::Unsupported)
                }
                LocalModelAvailability::AppleIntelligenceNotEnabled => {
                    Some(LocalAvailabilityReason::Disabled)
                }
                LocalModelAvailability::ModelNotReady
                | LocalModelAvailability::ModelUnavailable => {
                    Some(LocalAvailabilityReason::NotReady)
                }
            };
            if let Some(reason) = reason {
                return Ok(LocalObservation::Unavailable(reason));
            }
            let mut hasher = Sha256::new();
            hasher.update(b"floe.foundation.binding.v1\0");
            hasher.update(request.principal.as_bytes());
            hasher.update([0]);
            hasher.update(request.device_id.as_bytes());
            let binding_digest = floe_agent_contract::ModelBindingDigest(hasher.finalize().into());
            Ok(LocalObservation::Available(
                floe_inference::PreparedModelProfile {
                    capability: floe_inference::ObservedModelCapability {
                        purpose: floe_inference::ModelPurpose::new(request.purpose.clone())
                            .ok_or(ModelObservationError::InvalidIdentity)?,
                        consumer: floe_inference::ModelConsumer::new(request.consumer.clone())
                            .ok_or(ModelObservationError::InvalidIdentity)?,
                        capabilities: floe_agent_contract::ModelCapabilities::chat(),
                        boundary: floe_agent_contract::ProcessingBoundary::Device,
                        binding_digest,
                    },
                    transport: PreparedFoundationTransport {
                        protection: self.protection,
                        binding_digest,
                    },
                },
            ))
        })
    }
}
