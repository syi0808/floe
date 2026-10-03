use super::{
    credentials::{GatewayConnection, GatewayCredentialError, GatewayCredentialStore},
    http::GatewayHttpTransport,
    inference_wire::{self, AgentResponse, Inventory, PurposeCapability},
};
use crate::models::agent_codec::WireStep;
use floe_access::ModelDispatchTarget;
use floe_agent_contract::{AgentFailure, BoxFuture, ModelPlanRequest, ProcessingBoundary};
use floe_execution::{
    ExecutionScope,
    limits::{CallLimiter, CallLimits},
};
use floe_inference::{
    AdmittedDispatchTarget, CanonicalModelRequest, CanonicalModelResponse, ModelConsumer,
    ModelObservationError, ModelPurpose, ObservedModelCapability, PreparedModelProfile,
    PreparedModelTransport, PrimaryAbsence, PrimaryObservation, ProviderUsageObservation,
};
use std::{collections::BTreeSet, sync::OnceLock};

#[derive(Clone)]
pub struct GatewayModelProvider {
    store: GatewayCredentialStore,
}
impl GatewayModelProvider {
    pub fn new(store: GatewayCredentialStore) -> Self {
        Self { store }
    }
    pub async fn observe_primary(
        &self,
        request: &ModelPlanRequest,
        scope: &ExecutionScope,
    ) -> Result<PrimaryObservation<PreparedGatewayTransport>, ModelObservationError> {
        request
            .validate()
            .map_err(|_| ModelObservationError::InvalidIdentity)?;
        let Some(connection) = self
            .store
            .load(&request.principal, &request.device_id)
            .await
            .map_err(credential_observation)?
        else {
            return Ok(PrimaryObservation::Absent(
                PrimaryAbsence::NoGatewayConfigured,
            ));
        };
        let http = GatewayHttpTransport::new().map_err(observation_failure)?;
        let (status, bytes) = http
            .request(
                &connection.endpoint,
                Some(&connection.bearer),
                reqwest::Method::GET,
                "/v1/inference-purposes",
                None,
                scope.deadline(),
                scope.cancellation(),
            )
            .await
            .map_err(observation_failure)?;
        if status != 200 {
            return Err(inference_wire::failure(status, &bytes)
                .map(|pair| pair.0)
                .unwrap_or(ModelObservationError::InvalidInventory));
        }
        let inventory: Inventory =
            inference_wire::decode(&bytes).map_err(|_| ModelObservationError::InvalidInventory)?;
        // Authenticate the complete inventory and bind it to the same credential
        // generation even if pairing changes while the GET is in flight.
        let live = self
            .store
            .load(&request.principal, &request.device_id)
            .await
            .map_err(credential_observation)?
            .ok_or(ModelObservationError::InvalidIdentity)?;
        if live.binding != connection.binding
            || live.endpoint != connection.endpoint
            || live.bearer != connection.bearer
        {
            return Err(ModelObservationError::InvalidIdentity);
        }
        match inventory.selected(&request.purpose)? {
            PurposeCapability::NotConfigured => Ok(PrimaryObservation::Absent(
                PrimaryAbsence::PurposeNotConfigured,
            )),
            PurposeCapability::Disabled => {
                Ok(PrimaryObservation::Absent(PrimaryAbsence::PurposeDisabled))
            }
            PurposeCapability::Available {
                capability_revision,
                capabilities,
            } => Ok(PrimaryObservation::Available(PreparedModelProfile {
                capability: ObservedModelCapability {
                    purpose: ModelPurpose::new(request.purpose.clone())
                        .ok_or(ModelObservationError::InvalidIdentity)?,
                    consumer: ModelConsumer::new(request.consumer.clone())
                        .ok_or(ModelObservationError::InvalidIdentity)?,
                    capabilities: inference_wire::model_capabilities(&capabilities)
                        .map_err(|_| ModelObservationError::InvalidInventory)?,
                    boundary: ProcessingBoundary::Gateway,
                    binding_digest: connection.binding_digest(),
                },
                transport: PreparedGatewayTransport {
                    store: self.store.clone(),
                    connection,
                    purpose: request.purpose.clone(),
                    capability_revision,
                    http,
                },
            })),
        }
    }
}

pub struct PreparedGatewayTransport {
    store: GatewayCredentialStore,
    connection: GatewayConnection,
    purpose: String,
    capability_revision: String,
    http: GatewayHttpTransport,
}
impl PreparedModelTransport for PreparedGatewayTransport {
    fn validate_request(&self, request: &CanonicalModelRequest) -> Result<(), AgentFailure> {
        self.render_request(request).map(|_| ())
    }
    fn dispatch_target(&self) -> ModelDispatchTarget {
        ModelDispatchTarget::Gateway {
            expected: self.connection.binding.clone(),
        }
    }
    fn generate<'a>(
        &'a self,
        request: CanonicalModelRequest,
        target: AdmittedDispatchTarget,
    ) -> BoxFuture<'a, Result<CanonicalModelResponse, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            if !target.matches(
                &self.connection.binding_digest(),
                ProcessingBoundary::Gateway,
            ) {
                return Err(AgentFailure::PolicyDenied);
            }
            let current = self.current().await?;
            let body = self.render_request(&request)?;
            let _permit = model_calls()
                .acquire(body.len(), request.deadline, &request.cancellation)
                .await?;
            // The identity fence remains live after waiting for transport capacity.
            let rechecked = self.current().await?;
            if current.binding != rechecked.binding {
                return Err(AgentFailure::PolicyDenied);
            }
            let (status, bytes) = self
                .http
                .request(
                    &rechecked.endpoint,
                    Some(&rechecked.bearer),
                    reqwest::Method::POST,
                    "/v1/agent",
                    Some(body),
                    request.deadline,
                    &request.cancellation,
                )
                .await?;
            if status != 200 {
                return inference_wire::inference_failure(
                    status,
                    &bytes,
                    request.attempt_id,
                    &self.purpose,
                    &self.capability_revision,
                );
            }
            let envelope: AgentResponse = inference_wire::decode(&bytes)?;
            if envelope.schema_version != 2
                || envelope.purpose != self.purpose
                || envelope.attempt_id != request.attempt_id.to_string()
                || !inference_wire::valid_hex(&envelope.trace_id, 32)
            {
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
            if envelope.capability_revision != self.capability_revision {
                return Err(AgentFailure::PolicyDenied);
            }
            let usage = ProviderUsageObservation {
                tokens: envelope.usage.tokens,
                cost_micros: envelope.usage.cost_micros,
            };
            let output = match self.current().await {
                Ok(_) => decode_output(envelope.output, envelope.call_ids, &request),
                Err(error) => Err(error),
            };
            Ok(CanonicalModelResponse { output, usage })
        })
    }
}
impl PreparedGatewayTransport {
    fn render_request(&self, request: &CanonicalModelRequest) -> Result<Vec<u8>, AgentFailure> {
        request.validate()?;
        let instructions = request.envelope.stable_instructions.render();
        if instructions.trim().is_empty() || instructions.len() > 9_216 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let input = crate::models::agent_codec::encode_agent_input(&request)?;
        let mut classes = request
            .input_data_classes
            .iter()
            .map(|class| match class {
                floe_context_contract::DataClass::Synthetic => Ok("synthetic"),
                floe_context_contract::DataClass::Personal => Ok("personal"),
                floe_context_contract::DataClass::HighlySensitive => Ok("highly_sensitive"),
                _ => Err(AgentFailure::PolicyDenied),
            })
            .collect::<Result<Vec<_>, _>>()?;
        classes.sort_unstable();
        classes.dedup();
        if classes.is_empty() || classes.len() != request.input_data_classes.len() {
            return Err(AgentFailure::InvalidInput);
        }
        let body = serde_json::to_vec(&serde_json::json!({
            "schema_version": 2, "purpose": self.purpose,
            "capability_revision": self.capability_revision,
            "attempt_id": request.attempt_id.to_string(), "data_classes": classes,
            "output_format": request.envelope.run_instructions.output_format,
            "instructions": instructions, "input": input,
            "max_output_bytes": request.max_output_bytes.min(16_384),
        }))
        .map_err(|_| AgentFailure::InvalidInput)?;
        if body.len() > inference_wire::MAX_REQUEST_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(body)
    }
    async fn current(&self) -> Result<GatewayConnection, AgentFailure> {
        let current = self
            .store
            .load(
                &self.connection.binding.person_id,
                &self.connection.binding.device_id,
            )
            .await
            .map_err(|_| AgentFailure::PolicyDenied)?
            .ok_or(AgentFailure::PolicyDenied)?;
        if current.binding != self.connection.binding
            || current.endpoint != self.connection.endpoint
            || current.bearer != self.connection.bearer
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(current)
    }
}
fn decode_output(
    output: serde_json::Value,
    call_ids: serde_json::Value,
    request: &CanonicalModelRequest,
) -> Result<Vec<floe_agent_contract::ModelStep>, AgentFailure> {
    let values = output
        .as_array()
        .ok_or(AgentFailure::ServerModelInvalidOutput)?;
    if values.is_empty() || values.len() > 16 {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    let ids: Vec<String> =
        serde_json::from_value(call_ids).map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
    let mut steps = Vec::with_capacity(values.len());
    let mut calls = 0;
    for value in values {
        let step: WireStep = serde_json::from_value(value.clone())
            .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
        match &step {
            WireStep::Preamble { text } | WireStep::Answer { text } => {
                if text.trim().is_empty() {
                    return Err(AgentFailure::ServerModelInvalidOutput);
                }
            }
            WireStep::Call {
                capability_id,
                input,
            } => {
                if !crate::models::agent_codec::valid_alias(capability_id) {
                    return Err(AgentFailure::ServerModelInvalidOutput);
                }
                super::json::strict_json_bytes(input.as_bytes(), 32_768)
                    .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
                calls += 1;
            }
        }
        steps.push(crate::models::agent_codec::decode_agent_step(
            step,
            &request.catalog,
        )?);
    }
    if ids.len() != calls
        || ids.iter().collect::<BTreeSet<_>>().len() != calls
        || ids
            .iter()
            .any(|id| !crate::models::agent_codec::valid_call_id(id))
    {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    if serde_json::to_vec(&steps)
        .map_err(|_| AgentFailure::ServerModelInvalidOutput)?
        .len()
        > request.max_output_bytes.min(16_384)
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(steps)
}
fn model_calls() -> &'static CallLimiter {
    static LIMIT: OnceLock<CallLimiter> = OnceLock::new();
    LIMIT.get_or_init(|| {
        CallLimiter::new(CallLimits {
            max_running: 4,
            max_pending: 8,
            max_context_bytes: 98_304,
            max_total_context_bytes: 12 * 98_304,
        })
        .expect("static model limits")
    })
}
fn credential_observation(error: GatewayCredentialError) -> ModelObservationError {
    match error {
        GatewayCredentialError::Timeout => ModelObservationError::Timeout,
        GatewayCredentialError::Locked | GatewayCredentialError::Unavailable => {
            ModelObservationError::StorageUnavailable
        }
        GatewayCredentialError::Malformed | GatewayCredentialError::Unverified => {
            ModelObservationError::CredentialRejected
        }
        GatewayCredentialError::ForeignIdentity | GatewayCredentialError::Conflict => {
            ModelObservationError::InvalidIdentity
        }
    }
}
fn observation_failure(error: AgentFailure) -> ModelObservationError {
    match error {
        AgentFailure::Cancelled => ModelObservationError::Cancelled,
        AgentFailure::DeadlineExceeded | AgentFailure::ServerModelTimeout => {
            ModelObservationError::Timeout
        }
        AgentFailure::ServerModelInvalidOutput => ModelObservationError::InvalidInventory,
        _ => ModelObservationError::TransportUnavailable,
    }
}
