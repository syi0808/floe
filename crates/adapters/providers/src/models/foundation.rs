use std::time::Duration;

use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::{AgentFailure, SessionProtection, valid_context_refs};
#[cfg(test)]
use floe_agent_contract::ModelPlacement;
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
fn check_canonical_deadline(request: &floe_inference::CanonicalModelRequest) -> Result<(), AgentFailure> {
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
            if request.input_data_classes.iter().any(|class| {
                *class != floe_agent_contract::DataClass::Synthetic
            }) =>
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
    if instructions.len() > 4096 {
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
    let prompt = json!({
        "scoped_instructions": envelope.scoped_instructions,
        "contextual_data": envelope.contextual_data,
        "conversation": {
            "history": super::wire::wire_messages(&envelope.conversation.history),
            "current_turn": super::wire::wire_messages(&envelope.conversation.current_turn),
        },
        "runtime": envelope.runtime,
        "manifest": envelope.manifest,
    })
    .to_string();
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
                    output: vec![step],
                    used_tokens: CONTEXT_RESERVATION,
                    cost_micros: 0,
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
    purpose: floe_inference::ModelPurpose,
    consumer: floe_inference::ModelConsumer,
}

impl FoundationModelProvider {
    pub fn availability(&self) -> Result<LocalModelAvailability, AgentFailure> {
        let reply = NativeTransport.call(json!({"schemaVersion": 1, "operation": "availability"}))?;
        if reply.schema_version != AGENT_VERSION || reply.status != "availability" {
            return Err(AgentFailure::InvalidModelOutput);
        }
        reply.availability.ok_or(AgentFailure::InvalidModelOutput)
    }

    pub fn synthetic() -> Self {
        Self {
            protection: SessionProtection::SyntheticOnly,
            purpose: floe_inference::ModelPurpose::new(
                floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            )
            .expect("canonical purpose"),
            consumer: floe_inference::ModelConsumer::new(
                floe_inference::CANONICAL_MODEL_CONSUMER,
            )
            .expect("canonical consumer"),
        }
    }

    pub fn encrypted() -> Self {
        Self {
            protection: SessionProtection::Encrypted,
            purpose: floe_inference::ModelPurpose::new(
                floe_inference::EVERYDAY_ASSISTANCE_PURPOSE,
            )
            .expect("canonical purpose"),
            consumer: floe_inference::ModelConsumer::new(
                floe_inference::CANONICAL_MODEL_CONSUMER,
            )
            .expect("canonical consumer"),
        }
    }

    /// Observe the device profile under a domain purpose/consumer.
    pub fn scoped(
        protection: SessionProtection,
        purpose: &str,
        consumer: &str,
    ) -> Result<Self, AgentFailure> {
        Ok(Self {
            protection,
            purpose: floe_inference::ModelPurpose::new(purpose)
                .ok_or(AgentFailure::InvalidInput)?,
            consumer: floe_inference::ModelConsumer::new(consumer)
                .ok_or(AgentFailure::InvalidInput)?,
        })
    }
}

pub struct PreparedFoundationTransport {
    protection: SessionProtection,
}

impl floe_inference::PreparedModelTransport for PreparedFoundationTransport {
    async fn generate(
        &self,
        request: floe_inference::CanonicalModelRequest,
        target: floe_inference::AdmittedDispatchTarget,
    ) -> Result<floe_inference::CanonicalModelResponse, AgentFailure> {
        if !target.matches("foundation-device", None) {
            return Err(AgentFailure::PolicyDenied);
        }
        // The transport performs no Access/Context judgment. It renders from
        // the immutable envelope and maps wire output back to catalog
        // revisions; Engine remains the grammar owner.
        generate_canonical(&NativeTransport, request, self.protection)
            .await
            .map_err(map_canonical_failure)
    }
}

/// Native availability and invalid-output failures keep the local failure
/// class; policy, budget, cancellation and deadline failures pass through.
fn map_canonical_failure(failure: AgentFailure) -> AgentFailure {
    match failure {
        AgentFailure::ModelUnavailable => AgentFailure::LocalModelUnavailable,
        AgentFailure::InvalidModelOutput => AgentFailure::LocalModelInvalidOutput,
        failure => failure,
    }
}

impl floe_inference::ModelProvider for FoundationModelProvider {
    type Prepared = PreparedFoundationTransport;

    async fn observe_profiles(
        &self,
    ) -> Vec<floe_inference::PreparedModelProfile<Self::Prepared>> {
        // The profile identity is stable, but availability is honest: when
        // the bundled entry point does not resolve (test binaries, platforms
        // without the model), the device must not be offered. A dispatched
        // Foundation attempt that cannot run still consumes its budget
        // allowance, which would starve the server fallback inside the same
        // delegation scope.
        vec![floe_inference::PreparedModelProfile {
            profile: floe_inference::ModelProfile {
                id: "foundation-device".into(),
                purpose: self.purpose.clone(),
                consumer: self.consumer.clone(),
                execution_location: floe_inference::ExecutionLocation::Device,
                data_recipient: floe_inference::DataRecipient::Device,
                capabilities: floe_inference::ModelCapabilities(vec!["chat".into()]),
                available: LOCAL_MODEL.available(),
            },
            transport: PreparedFoundationTransport {
                protection: self.protection,
            },
        }]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use floe_agent_contract::prompts::{
        BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION, CAPABILITY_PROTOCOL,
        CAPABILITY_PROTOCOL_REVISION, PromptAssembly, PromptComponentKind, PromptRole,
        product_component,
    };
    use floe_agent_contract::{
        ContextEnvelope, ContextManifest, ContextualData, ModelConversation, ModelConversationEntry,
        PromptManifestEntry, RuntimeContext, ScopedInstructions,
    };
    use floe_agent_contract::DataClass;
    use floe_execution::Cancellation;

    use super::*;

    struct Mock {
        calls: Mutex<Vec<Value>>,
        reply: Value,
        pending: bool,
    }

    impl Mock {
        fn new(reply: Value) -> Self {
            Self {
                calls: Mutex::new(vec![]),
                reply,
                pending: false,
            }
        }

        fn released(&self) -> bool {
            self.calls
                .lock()
                .unwrap()
                .last()
                .is_some_and(|call| call["operation"] == "release")
        }
    }

    impl Transport for Mock {
        fn call(&self, request: Value) -> Result<Reply, AgentFailure> {
            self.calls.lock().unwrap().push(request.clone());
            let mut reply = if self.pending && request["operation"] != "release" {
                json!({"schemaVersion": 1, "status": "pending"})
            } else {
                self.reply.clone()
            };
            if reply.get("requestID").is_none() {
                reply["requestID"] = request["requestID"].clone();
            }
            serde_json::from_value(reply).map_err(|_| AgentFailure::InvalidModelOutput)
        }
    }

    /// A minimal role prompt: this exercises the transport, not a role.
    fn prompt() -> PromptAssembly {
        let prompt = PromptAssembly {
            schema_version: 1,
            role: PromptRole::Manager,
            components: vec![
                product_component(
                    PromptComponentKind::BehaviorKernel,
                    "behavior-kernel",
                    BEHAVIOR_KERNEL_REVISION,
                    BEHAVIOR_KERNEL,
                ),
                product_component(
                    PromptComponentKind::Role,
                    "fixture-role",
                    1,
                    "Answer the fixture assignment.",
                ),
                product_component(
                    PromptComponentKind::CapabilityProtocol,
                    "capability-protocol",
                    CAPABILITY_PROTOCOL_REVISION,
                    CAPABILITY_PROTOCOL,
                ),
            ],
        };
        prompt.validate().unwrap();
        prompt
    }

    fn answer() -> Value {
        json!({"schemaVersion": 1, "status": "done", "step": {"kind": "answer", "text": "Synthetic answer"}})
    }

    fn agent_card() -> floe_agent_contract::AgentCard {
        floe_agent_contract::AgentCard {
            schema_version: 1,
            protocol_version: floe_agent_contract::A2A_PROTOCOL_VERSION.into(),
            id: "expert-a".into(),
            version: "1".into(),
            name: "expert-a".into(),
            description: "fixture expert".into(),
            supported_placements: vec![ModelPlacement::DeviceLocal],
            domain_tags: vec![],
            skills: vec![],
        }
    }

    fn delegate_reply(input: &str) -> Value {
        json!({"schemaVersion": 1, "status": "done", "step": {
            "kind": "call", "capabilityID": "floe.a2a.delegate", "input": input }})
    }

    fn canonical_catalog() -> floe_agent_contract::AllowedCatalog {
        floe_agent_contract::AllowedCatalog {
            cards: vec![floe_agent_contract::AgentDefinition {
                card: agent_card(),
                definition_revision: 2,
            }],
            tools: vec![floe_agent_contract::ToolDescriptor {
                id: "fixture.read".into(),
                definition_revision: 3,
                description: "read the fixture".into(),
                input_schema: "{}".into(),
                output_data_class: "synthetic".into(),
            }],
            revision: 1,
        }
    }

    fn canonical_request() -> floe_inference::CanonicalModelRequest {
        let prompt = prompt();
        let envelope = ContextEnvelope {
            schema_version: 1,
            stable_instructions: prompt.clone(),
            scoped_instructions: ScopedInstructions {
                purpose: floe_inference::EVERYDAY_ASSISTANCE_PURPOSE.into(),
                response_contract: String::new(),
                available_capabilities: vec![],
                active_experts: vec![],
                correction: None,
            },
            contextual_data: ContextualData {
                projection_version: 1,
                memories: vec![],
                optional_context_issues: vec![],
                evidence: vec![],
            },
            conversation: ModelConversation {
                history: vec![],
                current_turn: vec![ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "Summarize this fixture".into(),
                }],
            },
            runtime: RuntimeContext {
                max_output_bytes: 16384,
            },
            manifest: ContextManifest {
                prompt_components: prompt
                    .components
                    .iter()
                    .map(|component| PromptManifestEntry {
                        kind: component.kind,
                        source: component.source.clone(),
                        revision: component.revision,
                    })
                    .collect(),
                evidence: vec![],
                memories: vec![],
                agent_cards: vec![],
            },
        };
        floe_inference::CanonicalModelRequest {
            attempt_id: Uuid::new_v4(),
            envelope,
            catalog: canonical_catalog(),
            input_data_classes: vec![DataClass::Synthetic],
            remaining_tokens: 8192,
            remaining_cost_micros: 0,
            max_output_bytes: 16384,
            deadline: Instant::now() + Duration::from_secs(1),
            cancellation: Cancellation::default(),
        }
    }

    #[tokio::test]
    async fn canonical_answer_uses_attempt_id_as_native_request_id() {
        let transport = Mock::new(answer());
        let request = canonical_request();
        let attempt_id = request.attempt_id;
        let expected_instructions = request.envelope.stable_instructions.render();
        let response = generate_canonical(&transport, request, SessionProtection::SyntheticOnly)
            .await
            .unwrap();
        assert_eq!(response.used_tokens, 4096);
        assert_eq!(response.cost_micros, 0);
        assert_eq!(
            response.output.as_slice(),
            [floe_agent_contract::ModelStep::Answer {
                text: "Synthetic answer".into(),
                artifacts: vec![],
            }]
        );
        assert!(transport.released());
        let calls = transport.calls.lock().unwrap();
        assert_eq!(calls[0]["operation"], "start");
        assert_eq!(calls[0]["requestID"], json!(attempt_id));
        assert_eq!(calls[0]["input"]["instructions"], expected_instructions);
        let prompt: Value =
            serde_json::from_str(calls[0]["input"]["prompt"].as_str().unwrap()).unwrap();
        assert_eq!(
            prompt["conversation"]["current_turn"][0]["content"],
            "Summarize this fixture"
        );
        assert_eq!(calls[0]["input"]["maxResponseTokens"], 1024);
    }

    #[tokio::test]
    async fn canonical_tool_call_resolves_exact_catalog_revision() {
        let transport = Mock::new(json!({"schemaVersion": 1, "status": "done", "step": {
            "kind": "call", "capabilityID": "fixture.read", "input": "{\"day\":\"today\"}" }}));
        let response =
            generate_canonical(&transport, canonical_request(), SessionProtection::SyntheticOnly)
                .await
                .unwrap();
        assert_eq!(
            response.output.as_slice(),
            [floe_agent_contract::ModelStep::CallTool {
                tool_id: "fixture.read".into(),
                definition_revision: 3,
                input: "{\"day\":\"today\"}".into(),
            }]
        );
        assert!(transport.released());
    }

    #[tokio::test]
    async fn canonical_delegate_resolves_exact_agent_revision_and_context_refs() {
        let input = serde_json::to_string(&json!({
            "agent_id": "expert-a",
            "message": "hi",
            "context_refs": ["turn:1", "evidence:9"],
        }))
        .unwrap();
        let transport = Mock::new(delegate_reply(&input));
        let response =
            generate_canonical(&transport, canonical_request(), SessionProtection::SyntheticOnly)
                .await
                .unwrap();
        assert_eq!(
            response.output.as_slice(),
            [floe_agent_contract::ModelStep::Delegate {
                agent_id: "expert-a".into(),
                definition_revision: 2,
                message: "hi".into(),
                context_refs: vec!["turn:1".into(), "evidence:9".into()],
            }]
        );
        assert!(transport.released());
    }

    #[tokio::test]
    async fn canonical_unknown_tool_and_agent_fail_closed() {
        let unknown_tool = Mock::new(json!({"schemaVersion": 1, "status": "done", "step": {
            "kind": "call", "capabilityID": "calendar.create", "input": "{}" }}));
        assert_eq!(
            generate_canonical(
                &unknown_tool,
                canonical_request(),
                SessionProtection::SyntheticOnly,
            )
            .await
            .err(),
            Some(AgentFailure::CapabilityDenied)
        );
        assert!(unknown_tool.released());
        let input = serde_json::to_string(&json!({
            "agent_id": "expert-unknown",
            "message": "hi",
        }))
        .unwrap();
        let unknown_agent = Mock::new(delegate_reply(&input));
        assert_eq!(
            generate_canonical(
                &unknown_agent,
                canonical_request(),
                SessionProtection::SyntheticOnly,
            )
            .await
            .err(),
            Some(AgentFailure::CapabilityDenied)
        );
        assert!(unknown_agent.released());
    }

    #[tokio::test]
    async fn canonical_malformed_and_oversized_output_fail_closed() {
        for reply in [
            json!({"schemaVersion": 2, "status": "pending"}),
            json!({"schemaVersion": 1, "status": "pending", "requestID": Uuid::new_v4()}),
            json!({"schemaVersion": 1, "status": "done", "step": {"kind": "answer", "text": "ok", "input": "injected"}}),
            json!({"schemaVersion": 1, "status": "done", "step": {"kind": "call", "capabilityID": "fixture.read", "input": "not-json"}}),
        ] {
            let transport = Mock::new(reply);
            assert_eq!(
                generate_canonical(
                    &transport,
                    canonical_request(),
                    SessionProtection::SyntheticOnly,
                )
                .await
                .err(),
                Some(AgentFailure::InvalidModelOutput)
            );
            assert!(transport.released());
        }
        let refs: Vec<String> = (0..129).map(|_| "r".into()).collect();
        let input = serde_json::to_string(&json!({
            "agent_id": "expert-a",
            "message": "hi",
            "context_refs": refs,
        }))
        .unwrap();
        let transport = Mock::new(delegate_reply(&input));
        assert_eq!(
            generate_canonical(
                &transport,
                canonical_request(),
                SessionProtection::SyntheticOnly,
            )
            .await
            .err(),
            Some(AgentFailure::InvalidModelOutput)
        );
        assert!(transport.released());
        let transport = Mock::new(answer());
        let mut small = canonical_request();
        small.max_output_bytes = 1;
        assert_eq!(
            generate_canonical(&transport, small, SessionProtection::SyntheticOnly)
                .await
                .err(),
            Some(AgentFailure::BudgetExceeded)
        );
        assert!(transport.released());
    }

    #[tokio::test]
    async fn canonical_cancellation_deadline_and_protection_never_reach_native() {
        let transport = Mock::new(answer());
        let cancelled = canonical_request();
        cancelled.cancellation.cancel();
        assert_eq!(
            generate_canonical(&transport, cancelled, SessionProtection::SyntheticOnly)
                .await
                .err(),
            Some(AgentFailure::Cancelled)
        );
        let mut expired = canonical_request();
        expired.deadline = Instant::now();
        assert_eq!(
            generate_canonical(&transport, expired, SessionProtection::SyntheticOnly)
                .await
                .err(),
            Some(AgentFailure::DeadlineExceeded)
        );
        let mut personal = canonical_request();
        personal.input_data_classes = vec![DataClass::Personal];
        assert_eq!(
            generate_canonical(&transport, personal, SessionProtection::SyntheticOnly)
                .await
                .err(),
            Some(AgentFailure::VaultUnavailable)
        );
        assert_eq!(
            generate_canonical(
                &transport,
                canonical_request(),
                SessionProtection::KeyUnavailable,
            )
            .await
            .err(),
            Some(AgentFailure::VaultUnavailable)
        );
        for class in [DataClass::Credential, DataClass::DeviceOnlyRaw] {
            let mut denied = canonical_request();
            denied.input_data_classes = vec![class];
            assert_eq!(
                generate_canonical(&transport, denied, SessionProtection::Encrypted)
                    .await
                    .err(),
                Some(AgentFailure::PolicyDenied)
            );
        }
        let mut limited = canonical_request();
        limited.remaining_tokens = 4095;
        assert_eq!(
            generate_canonical(&transport, limited, SessionProtection::SyntheticOnly)
                .await
                .err(),
            Some(AgentFailure::BudgetExceeded)
        );
        assert!(transport.calls.lock().unwrap().is_empty());
        let mut personal = canonical_request();
        personal.input_data_classes = vec![DataClass::Personal];
        assert!(
            generate_canonical(&transport, personal, SessionProtection::Encrypted)
                .await
                .is_ok()
        );
        assert!(transport.released());
    }

    #[tokio::test]
    async fn canonical_pending_deadline_releases_the_native_lease() {
        let transport = Arc::new(Mock {
            pending: true,
            ..Mock::new(answer())
        });
        let mut limited = canonical_request();
        limited.deadline = Instant::now() + Duration::from_millis(10);
        assert_eq!(
            generate_canonical(
                transport.as_ref(),
                limited,
                SessionProtection::SyntheticOnly,
            )
            .await
            .err(),
            Some(AgentFailure::DeadlineExceeded)
        );
        assert!(transport.released());
    }

    #[tokio::test]
    async fn canonical_native_error_maps_to_local_failure_class() {
        let transport = Mock::new(json!({"schemaVersion": 1, "status": "error"}));
        assert_eq!(
            generate_canonical(
                &transport,
                canonical_request(),
                SessionProtection::SyntheticOnly,
            )
            .await
            .err(),
            Some(AgentFailure::ModelUnavailable)
        );
        assert!(transport.released());
        assert_eq!(
            map_canonical_failure(AgentFailure::ModelUnavailable),
            AgentFailure::LocalModelUnavailable
        );
        assert_eq!(
            map_canonical_failure(AgentFailure::InvalidModelOutput),
            AgentFailure::LocalModelInvalidOutput
        );
        assert_eq!(
            map_canonical_failure(AgentFailure::CapabilityDenied),
            AgentFailure::CapabilityDenied
        );
    }
}
