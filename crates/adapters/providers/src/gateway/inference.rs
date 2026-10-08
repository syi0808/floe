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
use serde::Serialize;
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
                Some(connection.bearer.as_str()),
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
            || live.bearer.as_str() != connection.bearer.as_str()
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
                budget_profile,
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
                    selection_commitment: inference_wire::selection_commitment(
                        &capability_revision,
                    )?,
                    budget_profile: budget_profile.clone(),
                },
                transport: PreparedGatewayTransport {
                    store: self.store.clone(),
                    connection,
                    purpose: request.purpose.clone(),
                    capability_revision,
                    budget_profile,
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
    budget_profile: floe_agent_contract::ModelBudgetProfile,
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
                    Some(rechecked.bearer.as_str()),
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
            if envelope.schema_version != 3
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
        if request.budget_profile != self.budget_profile {
            return Err(AgentFailure::PolicyDenied);
        }
        let instructions = request.envelope.stable_instructions.render();
        if instructions.trim().is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        if instructions.len() > request.budget_profile.framing.max_instruction_bytes as usize {
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        let input = crate::models::agent_codec::encode_agent_input(&request)?;
        let input_bytes = serde_json::to_vec(&input).map_err(|_| AgentFailure::InvalidInput)?;
        let messages = input["messages"]
            .as_array()
            .ok_or(AgentFailure::InvalidInput)?;
        let tools = input["tools"]
            .as_array()
            .ok_or(AgentFailure::InvalidInput)?;
        if go_compatible_json_len(&input_bytes)
            > request.budget_profile.framing.max_input_json_bytes as usize
            || messages.len() > request.budget_profile.framing.max_messages as usize
            || tools.len() > request.budget_profile.framing.max_tools as usize
        {
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
        validate_model_context_estimate(
            &request.budget_profile,
            &instructions,
            &input,
            &request.envelope.run_instructions.output_format,
        )?;
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
            "schema_version": 3, "purpose": self.purpose,
            "capability_revision": self.capability_revision,
            "attempt_id": request.attempt_id.to_string(), "data_classes": classes,
            "output_format": request.envelope.run_instructions.output_format,
            "instructions": instructions, "input": input,
            "max_output_bytes": request.max_output_bytes.min(16_384),
        }))
        .map_err(|_| AgentFailure::InvalidInput)?;
        if body.len() > request.budget_profile.framing.max_request_bytes as usize {
            return Err(AgentFailure::ModelInputCapacityExceeded);
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
            || current.bearer.as_str() != self.connection.bearer.as_str()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(current)
    }
}

#[derive(Serialize)]
struct ModelInputEstimate<'a> {
    instructions: &'a str,
    input: &'a serde_json::Value,
    output_format: &'a floe_agent_contract::ModelOutputFormat,
}

fn validate_model_context_estimate(
    profile: &floe_agent_contract::ModelBudgetProfile,
    instructions: &str,
    input: &serde_json::Value,
    output_format: &floe_agent_contract::ModelOutputFormat,
) -> Result<(), AgentFailure> {
    if !profile.context_estimate_available() {
        return Ok(());
    }
    let encoded = serde_json::to_vec(&ModelInputEstimate {
        instructions,
        input,
        output_format,
    })
    .map_err(|_| AgentFailure::InvalidInput)?;
    let mut estimated = go_compatible_json_len(&encoded) as u64;
    for value in [
        profile.estimator.provider_overhead_tokens,
        profile.estimator.safety_margin_tokens,
        profile.selected_output_reservation.tokens,
    ] {
        let value = u64::from(value.ok_or(AgentFailure::InvalidInput)?);
        estimated = estimated
            .checked_add(value)
            .ok_or(AgentFailure::ModelInputCapacityExceeded)?;
    }
    if estimated
        > u64::from(
            profile
                .context_window
                .tokens
                .ok_or(AgentFailure::InvalidInput)?,
        )
    {
        return Err(AgentFailure::ModelInputCapacityExceeded);
    }
    Ok(())
}

// Go's encoding/json escapes HTML-sensitive characters and U+2028/U+2029.
// Match its serialized byte length so the Go and Rust context estimates share
// a deterministic UTF-8 definition; this remains a byte estimate, not a
// tokenizer result.
pub(super) fn go_compatible_json_len(encoded: &[u8]) -> usize {
    let mut length = encoded.len();
    let mut index = 0;
    while index < encoded.len() {
        match encoded[index] {
            b'<' | b'>' | b'&' => length += 5,
            0xe2 if encoded.get(index + 1) == Some(&0x80)
                && encoded
                    .get(index + 2)
                    .is_some_and(|value| *value == 0xa8 || *value == 0xa9) =>
            {
                length += 3;
                index += 2;
            }
            _ => {}
        }
        index += 1;
    }
    length
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
        GatewayCredentialError::Cancelled => ModelObservationError::Cancelled,
        GatewayCredentialError::Locked
        | GatewayCredentialError::Unavailable
        | GatewayCredentialError::Indeterminate => ModelObservationError::StorageUnavailable,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::CompositeModelProvider;
    use floe_access::{
        DependencyAuthorization, DependencyResolver, GatewayCredentialExpectation,
        GatewayTrustReader, RemoteProducerIdentity, VerifiedGatewayBinding,
    };
    use floe_agent_contract::{
        AttemptContext, ContextEnvelope, ContextManifest, ContextualData, DiscoveryContext,
        ModelCapabilities, ModelConversation, ModelConversationEntry, ModelOutputFormat,
        ModelPlanRequest, ModelPort, ModelRequest, ModelStep, RunInstructions,
        prompts::expert_prompt,
    };
    use floe_connections::{
        GatewayCredentialMaterial, GatewayCredentialRead, GatewayCredentialSnapshot,
        GatewayPrivateReader, PairingError, PairingPrivateSnapshot,
    };
    use floe_context_contract::{DataClass, DependencyCoverage};
    use floe_execution::{
        Cancellation, ExecutionScope,
        budget::{BudgetConfig, BudgetLedger, ModelUsage},
    };
    use floe_kernel::{OwnerActor, PersonId, TraceContext};
    use std::{sync::Arc, time::Duration};
    use uuid::Uuid;

    struct TestGatewayTrust {
        producer: RemoteProducerIdentity,
        operation_id: Uuid,
    }

    impl GatewayTrustReader for TestGatewayTrust {
        fn credential_expectation<'a>(
            &'a self,
        ) -> floe_execution::BoxFuture<'a, Result<GatewayCredentialExpectation, AgentFailure>>
        {
            Box::pin(async move {
                Ok(GatewayCredentialExpectation::Committed {
                    operation_id: self.operation_id,
                    generation: 1,
                })
            })
        }

        fn pinned_producer<'a>(
            &'a self,
        ) -> floe_execution::BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
            Box::pin(async move { Ok(self.producer.clone()) })
        }
    }

    struct TestGatewayPrivateReader {
        operation_id: Uuid,
        binding: VerifiedGatewayBinding,
        endpoint: String,
        bearer: GatewayCredentialMaterial,
    }

    impl GatewayPrivateReader for TestGatewayPrivateReader {
        fn pairing_private<'a>(
            &'a self,
            _operation_id: Uuid,
            _person: PersonId,
            _device: &'a str,
            _generation: u64,
        ) -> floe_execution::BoxFuture<'a, Result<PairingPrivateSnapshot, PairingError>> {
            Box::pin(async { Err(PairingError::Rejected) })
        }

        fn credential<'a>(
            &'a self,
            person: PersonId,
            device: &'a str,
        ) -> floe_execution::BoxFuture<'a, Result<GatewayCredentialRead, PairingError>> {
            let operation_id = self.operation_id;
            let binding = self.binding.clone();
            let endpoint = self.endpoint.clone();
            let bearer = self.bearer.clone();
            Box::pin(async move {
                if binding.person_id != person.to_string() || binding.device_id != device {
                    return Err(PairingError::ForeignIdentity);
                }
                Ok(GatewayCredentialRead::Active(GatewayCredentialSnapshot {
                    operation_id,
                    binding,
                    endpoint,
                    bearer,
                }))
            })
        }
    }

    struct NoDependencies;

    impl DependencyResolver for NoDependencies {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a floe_context_contract::ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> floe_execution::BoxFuture<'a, Result<(), AgentFailure>> {
            Box::pin(async { Err(AgentFailure::PolicyDenied) })
        }
    }

    fn execution_scope() -> ExecutionScope {
        let budget = BudgetLedger::new(BudgetConfig::new(10_000, 10_000), ModelUsage::default());
        ExecutionScope::root(
            Cancellation::new(),
            tokio::time::Instant::now() + Duration::from_secs(30),
            budget.work_lease(),
            TraceContext::new(Uuid::new_v4()),
        )
    }

    fn model_envelope(content: &str) -> ContextEnvelope {
        let mut envelope = ContextEnvelope {
            schema_version: floe_agent_contract::CONTEXT_ENVELOPE_SCHEMA_VERSION,
            stable_instructions: expert_prompt(
                "r3a-e2e",
                1,
                "Answer the synthetic request briefly.",
            ),
            run_instructions: RunInstructions {
                purpose: "quick_response".to_owned(),
                response_contract: "Return one short answer.".to_owned(),
                output_format: ModelOutputFormat::Text,
            },
            discovery: DiscoveryContext {
                revision: 1,
                available_capabilities: Vec::new(),
                active_experts: Vec::new(),
            },
            contextual_data: ContextualData {
                projection_version: 1,
                memories: Vec::new(),
                optional_context_issues: Vec::new(),
                evidence: Vec::new(),
            },
            conversation: ModelConversation {
                history: Vec::new(),
                current_turn: vec![ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: content.to_owned(),
                }],
            },
            attempt: AttemptContext {
                correction: None,
                max_output_bytes: 1024,
            },
            manifest: ContextManifest {
                stable_prompt_sha256: String::new(),
                run_frame_sha256: String::new(),
                expert_environment: None,
                prompt_components: Vec::new(),
                evidence: Vec::new(),
                memories: Vec::new(),
                agent_cards: Vec::new(),
            },
        };
        envelope.manifest = envelope
            .derived_manifest(None)
            .expect("derive test manifest");
        envelope
    }

    fn model_request(plan: &floe_agent_contract::PreparedModelPlan, content: &str) -> ModelRequest {
        ModelRequest {
            attempt_id: Uuid::new_v4(),
            reservation_ceiling: floe_execution::budget::ModelReservationCeiling {
                tokens: 2048,
                cost_micros: 10_000,
            },
            principal: plan.principal.clone(),
            device_id: plan.device_id.clone(),
            projection: floe_agent_contract::AuthorizedModelProjection {
                projection_ref: floe_agent_contract::ProjectionRef::new(),
                projection_operation_id: Uuid::new_v4(),
                plan_id: plan.operation_id,
                binding_digest: plan.binding_digest,
                projection_revision: 1,
                envelope: model_envelope(content),
                coverage: DependencyCoverage::Independent,
                input_data_classes: vec![DataClass::Synthetic],
            },
            catalog: floe_agent_contract::AllowedCatalog::default(),
            purpose: plan.purpose.clone(),
            consumer: plan.consumer.clone(),
            replay: Vec::new(),
        }
    }

    #[tokio::test]
    async fn rust_gateway_to_go_mock_provider_e2e() {
        if std::env::var("FLOE_R3A_E2E_REQUIRED").as_deref() != Ok("1") {
            return;
        }
        let endpoint = std::env::var("FLOE_R3A_GATEWAY_ENDPOINT").expect("Go test endpoint");
        let bearer = std::env::var("FLOE_R3A_GATEWAY_BEARER").expect("synthetic Go pairing bearer");
        let person_uuid = Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap();
        let person = PersonId(person_uuid);
        let device_id = "r3a-e2e-device".to_owned();
        let operation_id = Uuid::new_v4();
        let binding = VerifiedGatewayBinding {
            person_id: person_uuid.to_string(),
            device_id: device_id.clone(),
            client_id: "r3a-e2e-client".to_owned(),
            producer_instance: "r3a-e2e-producer".to_owned(),
            producer_key_fingerprint: "a".repeat(64),
            producer_audience: "floe".to_owned(),
            enrollment_id: "r3a-e2e-enrollment".to_owned(),
            credential_generation: 1,
        };
        let producer = RemoteProducerIdentity {
            schema_version: 1,
            instance_id: "r3a-e2e-producer".to_owned(),
            execution_owner: "r3a-e2e-owner".to_owned(),
            audience: "floe".to_owned(),
            key_id: "r3a-e2e-key".to_owned(),
            public_key: "synthetic-public-key".to_owned(),
            fingerprint: "a".repeat(64),
        };
        let private_reader = TestGatewayPrivateReader {
            operation_id,
            binding: binding.clone(),
            endpoint,
            bearer: GatewayCredentialMaterial::new(bearer.into_bytes()).unwrap(),
        };
        let store = GatewayCredentialStore::new(
            Arc::new(TestGatewayTrust {
                producer,
                operation_id,
            }),
            Arc::new(private_reader),
            &OwnerActor {
                person_id: person,
                device_id: device_id.clone(),
                runtime_epoch: 1,
            },
        )
        .unwrap();
        let gateway = GatewayModelProvider::new(store.clone());
        let request = ModelPlanRequest {
            principal: person_uuid.to_string(),
            device_id: device_id.clone(),
            purpose: "quick_response".to_owned(),
            consumer: "manager".to_owned(),
            required_capabilities: ModelCapabilities::chat(),
        };
        let scope = execution_scope();
        let observed = gateway.observe_primary(&request, &scope).await.unwrap();
        let PrimaryObservation::Available(observed) = observed else {
            panic!("Go inventory did not expose the configured Gateway purpose");
        };
        assert_eq!(
            observed
                .capability
                .budget_profile
                .framing
                .max_input_json_bytes,
            8192
        );
        assert_eq!(
            observed
                .capability
                .budget_profile
                .selected_output_reservation
                .tokens,
            Some(96)
        );
        assert!(observed.capability.budget_profile.validate().is_ok());

        let preflight_transport = observed.transport;
        let mismatch_profile = {
            let mut profile = observed.capability.budget_profile.clone();
            profile.framing.configured_input_json_bytes = Some(4096);
            profile.framing.max_input_json_bytes = 4096;
            profile
        };
        let mismatch_request = CanonicalModelRequest {
            attempt_id: Uuid::new_v4(),
            envelope: model_envelope("synthetic profile mismatch"),
            catalog: floe_agent_contract::AllowedCatalog::default(),
            budget_profile: mismatch_profile,
            input_data_classes: vec![DataClass::Synthetic],
            remaining_tokens: 2048,
            remaining_cost_micros: 10_000,
            max_output_bytes: 1024,
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
        };
        assert_eq!(
            preflight_transport.validate_request(&mismatch_request),
            Err(AgentFailure::PolicyDenied)
        );
        let oversized_canonical = CanonicalModelRequest {
            attempt_id: Uuid::new_v4(),
            envelope: model_envelope(&"한".repeat(3000)),
            catalog: floe_agent_contract::AllowedCatalog::default(),
            budget_profile: observed.capability.budget_profile.clone(),
            input_data_classes: vec![DataClass::Synthetic],
            remaining_tokens: 2048,
            remaining_cost_micros: 10_000,
            max_output_bytes: 1024,
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
        };
        assert_eq!(
            preflight_transport.validate_request(&oversized_canonical),
            Err(AgentFailure::ModelInputCapacityExceeded)
        );

        let planner = floe_inference::InferenceService::new(
            CompositeModelProvider::new(store.clone()),
            NoDependencies,
            store,
        );
        let prepared = planner.prepare(request, &scope).await.unwrap();
        let plan = prepared.plan().clone();
        assert_eq!(
            plan.budget_profile.as_ref(),
            Some(&observed.capability.budget_profile)
        );
        plan.validate_for_dispatch().unwrap();
        let oversized = model_request(&plan, &"한".repeat(3000));
        assert_eq!(
            prepared.generate(oversized, &scope).await,
            Err(AgentFailure::ModelInputCapacityExceeded)
        );

        let valid = model_request(&plan, "synthetic end-to-end request");
        let response = prepared.generate(valid, &scope).await.unwrap();
        assert!(
            matches!(response.steps.as_slice(), [ModelStep::Answer { text, .. }] if text == "Synthetic completion")
        );
        assert_eq!(response.usage.tokens, 17);
    }
}
