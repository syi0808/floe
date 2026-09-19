use floe_access::{
    DependencyResolver, ModelDispatchRecipientAuthority, ModelDispatchRequest, ModelDispatchTarget,
    admit_model_dispatch, consume_model_dispatch, revalidate_model_dispatch,
};
use floe_agent_contract::{
    AgentFailure, AllowedCatalog, ModelPort, ModelRequest, ModelResponse, ModelStep, ModelUsage,
};
use floe_execution::ExecutionScope;
use uuid::Uuid;

use crate::api::{DataRecipient, ExecutionLocation, ModelProfile};
use crate::ports::model_provider::{
    CanonicalModelRequest, ModelProvider, PreparedModelProfile, PreparedModelTransport,
};

pub const CANONICAL_MODEL_PURPOSE: &str = "everyday_assistance";
pub const CANONICAL_MODEL_CONSUMER: &str = "conversation.root";

const MAX_ATTEMPT_TOKENS: u64 = 4_096;
const MAX_ATTEMPT_COST_MICROS: u64 = 1_000_000;

/// Canonical root ModelPort. Inference owns profile selection, route planning,
/// transport retry/fallback and model budget settlement. Access owns
/// exact-recipient dispatch authority. Provider owns transport/credentials.
///
/// The root path never adapts back into the legacy `ModelTransport`.
pub struct InferenceService<Provider, Resolver, Authority> {
    provider: Provider,
    resolver: Resolver,
    authority: Authority,
}

impl<Provider, Resolver, Authority> InferenceService<Provider, Resolver, Authority> {
    pub fn new(provider: Provider, resolver: Resolver, authority: Authority) -> Self {
        Self {
            provider,
            resolver,
            authority,
        }
    }
}

impl<Provider, Resolver, Authority> ModelPort for InferenceService<Provider, Resolver, Authority>
where
    Provider: ModelProvider + Sync,
    Provider::Prepared: Send,
    Resolver: DependencyResolver,
    Authority: ModelDispatchRecipientAuthority,
{
    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        scope: &'a ExecutionScope,
    ) -> floe_agent_contract::BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        Box::pin(async move { self.generate_inner(request, scope).await })
    }
}

impl<Provider, Resolver, Authority> InferenceService<Provider, Resolver, Authority>
where
    Provider: ModelProvider + Sync,
    Provider::Prepared: Send,
    Resolver: DependencyResolver,
    Authority: ModelDispatchRecipientAuthority,
{
    async fn generate_inner(
        &self,
        request: ModelRequest,
        scope: &ExecutionScope,
    ) -> Result<ModelResponse, AgentFailure> {
        request.validate()?;
        if scope.cancellation().is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if tokio::time::Instant::now() >= scope.deadline() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        // Purpose/consumer must agree across the canonical chain. The root
        // uses one purpose and one consumer; App no longer invents another
        // through legacy policy.
        if request.purpose != CANONICAL_MODEL_PURPOSE
            || request.consumer != CANONICAL_MODEL_CONSUMER
            || request.projection.envelope.scoped_instructions.purpose != request.purpose
        {
            return Err(AgentFailure::InvalidInput);
        }
        let person_id = parse_person(&request.principal)?;

        // Observe non-secret profiles. Secrets stay inside prepared transport.
        let observed = self.provider.observe_profiles().await;
        let candidates = plan_candidates(&observed, &request)?;
        // Explicit: exactly one candidate, never silently falls back.
        // Auto: ranked local-first candidates with same-rank ambiguity denied.
        let mut last_failure: Option<AgentFailure> = None;
        for candidate in candidates {
            match self
                .attempt_candidate(&request, scope, person_id, candidate)
                .await
            {
                Ok(response) => return Ok(response),
                Err(failure) => {
                    // Explicit never falls back; Auto falls back only on
                    // transport failure, never on admission denial.
                    if request.preferred_profile_id.is_some() {
                        return Err(failure);
                    }
                    if is_admission_denial(&failure) {
                        return Err(failure);
                    }
                    last_failure = Some(failure);
                }
            }
        }
        Err(last_failure.unwrap_or(AgentFailure::ModelUnavailable))
    }

    async fn attempt_candidate(
        &self,
        request: &ModelRequest,
        scope: &ExecutionScope,
        person_id: floe_access::PersonId,
        candidate: &PreparedModelProfile<Provider::Prepared>,
    ) -> Result<ModelResponse, AgentFailure> {
        let target = dispatch_target(&candidate.profile)?;
        let dispatch = ModelDispatchRequest {
            person_id,
            projection_ref: request.projection.projection_ref.as_uuid(),
            projection_revision: request.projection.projection_revision,
            coverage: request.projection.coverage.clone(),
            input_data_classes: request.projection.input_data_classes.clone(),
            purpose: request.purpose.clone(),
            consumer: request.consumer.clone(),
            target,
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
        };
        // Access admit before any budget reservation.
        let permit = admit_model_dispatch(dispatch, &self.resolver, &self.authority).await?;

        // Sole model budget owner: the live scope budget, no detached ledger.
        let mut tokens = scope.budget().max_tokens().min(MAX_ATTEMPT_TOKENS);
        let mut cost = scope
            .budget()
            .max_cost_micros()
            .min(MAX_ATTEMPT_COST_MICROS);
        let mut attempt = scope.budget().begin(&mut tokens, &mut cost)?;

        // Consume immediately before the provider handoff.
        let fence = consume_model_dispatch(permit).await?;

        let canonical = CanonicalModelRequest {
            attempt_id: request.attempt_id,
            envelope: request.projection.envelope.clone(),
            catalog: request.catalog.clone(),
            input_data_classes: request.projection.input_data_classes.clone(),
            remaining_tokens: tokens,
            remaining_cost_micros: cost,
            max_output_bytes: request
                .projection
                .envelope
                .runtime
                .max_output_bytes
                .min(floe_agent_contract::MAX_OUTPUT_BYTES),
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
        };
        canonical.validate()?;
        attempt.mark_dispatched();
        let provider_response = candidate.transport.generate(canonical).await;
        let provider_response = match provider_response {
            Ok(response) => response,
            Err(failure) => {
                // Dropping `attempt` after mark_dispatched charges the
                // existing unknown estimate. No zero-charge fallback.
                drop(attempt);
                return Err(failure);
            }
        };
        // Trustworthy usage is settled exactly once, even when the payload
        // that follows is invalid or later suppressed.
        let usage = ModelUsage {
            tokens: provider_response.used_tokens,
            cost_micros: provider_response.cost_micros,
        };
        // Map to canonical catalog revisions; revision mismatches and shape
        // failures keep the trustworthy usage charged.
        let mapped = map_output(&provider_response.output, &request.catalog);
        let steps = match mapped {
            Ok(steps) if !steps.is_empty() && steps.len() <= 16 => steps,
            _ => {
                attempt
                    .settle(usage.tokens, usage.cost_micros)
                    .map_err(|_| AgentFailure::BudgetExceeded)?;
                return Err(AgentFailure::ServerModelInvalidOutput);
            }
        };
        attempt
            .settle(usage.tokens, usage.cost_micros)
            .map_err(|_| AgentFailure::BudgetExceeded)?;
        // Post-response authority revalidation before the response leaves.
        // Usage stays charged when revalidation fails.
        if revalidate_model_dispatch(&fence).await.is_err() {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(ModelResponse {
            attempt_id: request.attempt_id,
            steps,
            usage,
        })
    }
}

fn parse_person(principal: &str) -> Result<floe_access::PersonId, AgentFailure> {
    let uuid = Uuid::parse_str(principal).map_err(|_| AgentFailure::InvalidInput)?;
    floe_access::PersonId::from_uuid(uuid).ok_or(AgentFailure::InvalidInput)
}

fn dispatch_target(profile: &ModelProfile) -> Result<ModelDispatchTarget, AgentFailure> {
    match &profile.data_recipient {
        DataRecipient::Device => Ok(ModelDispatchTarget::Device),
        DataRecipient::External(recipient) => {
            if recipient.trim().is_empty() {
                return Err(AgentFailure::InvalidInput);
            }
            Ok(ModelDispatchTarget::External {
                recipient: recipient.clone(),
            })
        }
    }
}

fn is_admission_denial(failure: &AgentFailure) -> bool {
    matches!(
        failure,
        AgentFailure::PolicyDenied
            | AgentFailure::CapabilityDenied
            | AgentFailure::InvalidInput
            | AgentFailure::Cancelled
            | AgentFailure::DeadlineExceeded
            | AgentFailure::BudgetExceeded
    )
}

fn rank_profile(profile: &ModelProfile) -> u8 {
    match (&profile.execution_location, &profile.data_recipient) {
        (ExecutionLocation::Device, DataRecipient::Device) => 0,
        (ExecutionLocation::Gateway, DataRecipient::Device) => 1,
        (_, DataRecipient::Device) => 1,
        (_, DataRecipient::External(_)) => 2,
    }
}

fn plan_candidates<'a, Prepared>(
    observed: &'a [PreparedModelProfile<Prepared>],
    request: &ModelRequest,
) -> Result<Vec<&'a PreparedModelProfile<Prepared>>, AgentFailure> {
    if let Some(preferred) = request.preferred_profile_id.as_deref() {
        let exact = observed
            .iter()
            .find(|candidate| candidate.profile.id == preferred)
            .ok_or(AgentFailure::ModelUnavailable)?;
        validate_candidate(&exact.profile, request)?;
        return Ok(vec![exact]);
    }
    let mut eligible: Vec<&PreparedModelProfile<Prepared>> = observed
        .iter()
        .filter(|candidate| candidate.profile.available)
        .filter(|candidate| candidate.profile.purpose.as_str() == request.purpose)
        .filter(|candidate| candidate.profile.consumer.as_str() == request.consumer)
        .filter(|candidate| capabilities_hold(&candidate.profile, request))
        .filter(|candidate| placement_consistent(&candidate.profile))
        .collect();
    if eligible.is_empty() {
        return Err(AgentFailure::ModelUnavailable);
    }
    eligible.sort_by_key(|candidate| (rank_profile(&candidate.profile), candidate.profile.id.clone()));
    // Same-rank ambiguity fails closed rather than choosing arbitrarily.
    let top_rank = rank_profile(&eligible[0].profile);
    let top_count = eligible
        .iter()
        .take_while(|candidate| rank_profile(&candidate.profile) == top_rank)
        .count();
    if top_count > 1 {
        return Err(AgentFailure::PolicyDenied);
    }
    // Return in rank order; the caller tries each with a fresh Access permit
    // and falls back only on transport failure.
    Ok(eligible)
}

fn validate_candidate(profile: &ModelProfile, request: &ModelRequest) -> Result<(), AgentFailure> {
    if !profile.available {
        return Err(AgentFailure::ModelUnavailable);
    }
    if profile.purpose.as_str() != request.purpose
        || profile.consumer.as_str() != request.consumer
        || !capabilities_hold(profile, request)
        || !placement_consistent(profile)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn capabilities_hold(profile: &ModelProfile, request: &ModelRequest) -> bool {
    // The canonical root needs no special capability beyond a valid catalog;
    // an empty catalog is allowed and transport decides wire details.
    let _ = request;
    !profile
        .capabilities
        .0
        .iter()
        .any(|value| value.trim().is_empty())
}

fn placement_consistent(profile: &ModelProfile) -> bool {
    // A remote execution claiming a device-only recipient is incoherent.
    !matches!(
        (&profile.execution_location, &profile.data_recipient),
        (ExecutionLocation::Remote, DataRecipient::Device)
    )
}

fn map_output(
    output: &[floe_agent_contract::ModelStep],
    catalog: &AllowedCatalog,
) -> Result<Vec<floe_agent_contract::ModelStep>, AgentFailure> {
    for step in output {
        match step {
            ModelStep::CallTool {
                tool_id,
                definition_revision,
                ..
            } => {
                let held = catalog.tools.iter().any(|descriptor| {
                    descriptor.id == *tool_id
                        && descriptor.definition_revision == *definition_revision
                });
                if !held {
                    return Err(AgentFailure::ServerModelInvalidOutput);
                }
            }
            ModelStep::Delegate {
                agent_id,
                definition_revision,
                ..
            } => {
                let held = catalog.cards.iter().any(|definition| {
                    definition.card.id == *agent_id
                        && definition.definition_revision == *definition_revision
                });
                if !held {
                    return Err(AgentFailure::ServerModelInvalidOutput);
                }
            }
            ModelStep::Preamble { .. } | ModelStep::Answer { .. } => {}
        }
    }
    Ok(output.to_vec())
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use floe_access::{DependencyResolver, ModelDispatchRecipientAuthority};
    use floe_agent_contract::{
        AgentFailure, AllowedCatalog, AuthorizedModelProjection, ContextEnvelope, ContextManifest,
        ContextualData, DataClass, DependencyCoverage, ModelRequest, ModelResponse, ModelStep,
        ModelUsage, ProjectionRef, RuntimeContext, ScopedInstructions,
    };
    use floe_agent_contract::prompts::{
        PromptAssembly, PromptComponent, PromptComponentKind, PromptRole,
    };
    use floe_context_contract::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ContextDependency, ExecutionOwnerId,
        GrantAuthority, GrantConsumer, GrantDataCategory, GrantId, GrantOperation, GrantPurpose,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_execution::{
        Cancellation,
        budget::{BudgetConfig, BudgetLedger, ModelUsage as LedgerUsage},
    };
    use floe_kernel::{PersonId, RunId, TraceContext};
    use tokio::time::Instant;
    use uuid::Uuid;

    use super::*;
    use crate::api::{
        DataRecipient, ExecutionLocation, ModelCapabilities, ModelConsumer, ModelProfile,
        ModelPurpose,
    };
    use floe_access::DependencyAuthorization;
    use crate::ports::model_provider::{
        CanonicalModelRequest, CanonicalModelResponse, ModelProvider, PreparedModelProfile,
        PreparedModelTransport,
    };

    struct TestTransport {
        calls: AtomicUsize,
        seen_attempt: std::sync::Mutex<Vec<Uuid>>,
        behavior: std::sync::Mutex<TransportBehavior>,
    }

    #[derive(Clone)]
    enum TransportBehavior {
        Answer { tokens: u64, cost: u64 },
        Fail(AgentFailure),
        InvalidOutput { tokens: u64, cost: u64 },
    }

    impl TestTransport {
        fn answer() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                seen_attempt: std::sync::Mutex::new(Vec::new()),
                behavior: std::sync::Mutex::new(TransportBehavior::Answer {
                    tokens: 10,
                    cost: 5,
                }),
            }
        }

        fn failing(failure: AgentFailure) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                seen_attempt: std::sync::Mutex::new(Vec::new()),
                behavior: std::sync::Mutex::new(TransportBehavior::Fail(failure)),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl PreparedModelTransport for TestTransport {
        async fn generate(
            &self,
            request: CanonicalModelRequest,
        ) -> Result<CanonicalModelResponse, AgentFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen_attempt.lock().unwrap().push(request.attempt_id);
            match self.behavior.lock().unwrap().clone() {
                TransportBehavior::Answer { tokens, cost } => Ok(CanonicalModelResponse {
                    output: vec![ModelStep::Answer {
                        text: "hello".into(),
                        artifacts: vec![],
                    }],
                    used_tokens: tokens,
                    cost_micros: cost,
                }),
                TransportBehavior::Fail(failure) => Err(failure),
                TransportBehavior::InvalidOutput { tokens, cost } => Ok(CanonicalModelResponse {
                    output: vec![],
                    used_tokens: tokens,
                    cost_micros: cost,
                }),
            }
        }
    }

    struct TestProvider {
        profiles: Vec<(ModelProfile, Arc<TestTransport>)>,
    }

    impl ModelProvider for TestProvider {
        type Prepared = Arc<TestTransport>;

        async fn observe_profiles(&self) -> Vec<PreparedModelProfile<Self::Prepared>> {
            self.profiles
                .iter()
                .map(|(profile, transport)| PreparedModelProfile {
                    profile: profile.clone(),
                    transport: Arc::clone(transport),
                })
                .collect()
        }
    }

    impl PreparedModelTransport for Arc<TestTransport> {
        async fn generate(
            &self,
            request: CanonicalModelRequest,
        ) -> Result<CanonicalModelResponse, AgentFailure> {
            (**self).generate(request).await
        }
    }

    struct AllowResolver;

    impl DependencyResolver for AllowResolver {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>>
        {
            Box::pin(async move { Ok(()) })
        }
    }

    struct DenyResolver;

    impl DependencyResolver for DenyResolver {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>>
        {
            Box::pin(async move { Err(AgentFailure::PolicyDenied) })
        }
    }

    struct AllowAuthority;

    impl ModelDispatchRecipientAuthority for AllowAuthority {
        fn check_recipient(&self, _recipient: &str) -> Result<(), AgentFailure> {
            Ok(())
        }
    }

    struct CountingAuthority {
        calls: AtomicUsize,
        deny_from: usize,
    }

    impl ModelDispatchRecipientAuthority for CountingAuthority {
        fn check_recipient(&self, _recipient: &str) -> Result<(), AgentFailure> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call >= self.deny_from {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(())
        }
    }

    fn device_profile(id: &str, available: bool) -> ModelProfile {
        ModelProfile {
            id: id.into(),
            purpose: ModelPurpose::new(CANONICAL_MODEL_PURPOSE).unwrap(),
            consumer: ModelConsumer::new(CANONICAL_MODEL_CONSUMER).unwrap(),
            execution_location: ExecutionLocation::Device,
            data_recipient: DataRecipient::Device,
            capabilities: ModelCapabilities(vec![]),
            available,
        }
    }

    fn external_profile(id: &str, recipient: &str, available: bool) -> ModelProfile {
        ModelProfile {
            id: id.into(),
            purpose: ModelPurpose::new(CANONICAL_MODEL_PURPOSE).unwrap(),
            consumer: ModelConsumer::new(CANONICAL_MODEL_CONSUMER).unwrap(),
            execution_location: ExecutionLocation::Remote,
            data_recipient: DataRecipient::external(recipient).unwrap(),
            capabilities: ModelCapabilities(vec![]),
            available,
        }
    }

    fn envelope() -> ContextEnvelope {
        use floe_agent_contract::prompts::{
            BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION, CAPABILITY_PROTOCOL,
            CAPABILITY_PROTOCOL_REVISION, product_component,
        };
        let assembly = PromptAssembly {
            schema_version: floe_agent_contract::AGENT_VERSION,
            role: PromptRole::Manager,
            components: vec![
                product_component(
                    PromptComponentKind::BehaviorKernel,
                    "behavior-kernel",
                    BEHAVIOR_KERNEL_REVISION,
                    BEHAVIOR_KERNEL,
                ),
                product_component(PromptComponentKind::Role, "manager", 1, "manager role"),
                product_component(
                    PromptComponentKind::CapabilityProtocol,
                    "capability-protocol",
                    CAPABILITY_PROTOCOL_REVISION,
                    CAPABILITY_PROTOCOL,
                ),
            ],
        };
        ContextEnvelope {
            schema_version: floe_agent_contract::AGENT_VERSION,
            stable_instructions: assembly.clone(),
            scoped_instructions: ScopedInstructions {
                purpose: CANONICAL_MODEL_PURPOSE.into(),
                response_contract: "c".into(),
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
            conversation: floe_agent_contract::ModelConversation {
                history: vec![],
                current_turn: vec![floe_agent_contract::ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "hi".into(),
                }],
            },
            runtime: RuntimeContext {
                max_output_bytes: 1024,
            },
            manifest: ContextManifest {
                prompt_components: vec![],
                evidence: vec![],
                memories: vec![],
                agent_cards: vec![],
            },
        }
    }

    fn projection(coverage: DependencyCoverage, classes: Vec<DataClass>) -> AuthorizedModelProjection {
        AuthorizedModelProjection {
            projection_ref: ProjectionRef::new(),
            projection_revision: 1,
            envelope: envelope(),
            coverage,
            input_data_classes: classes,
        }
    }

    fn model_request(
        attempt_id: Uuid,
        principal: &str,
        projection: AuthorizedModelProjection,
        preferred: Option<String>,
    ) -> ModelRequest {
        ModelRequest {
            attempt_id,
            principal: principal.into(),
            projection,
            catalog: AllowedCatalog::default(),
            purpose: CANONICAL_MODEL_PURPOSE.into(),
            consumer: CANONICAL_MODEL_CONSUMER.into(),
            preferred_profile_id: preferred,
            replay: vec![],
        }
    }

    fn scope() -> (BudgetLedger, ExecutionScope) {
        let ledger = BudgetLedger::new(BudgetConfig::new(100_000, 10_000_000), LedgerUsage::default());
        let scope = ExecutionScope::root(
            Cancellation::default(),
            Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            TraceContext::new(Uuid::new_v4()).with_run_id(RunId::new()),
        );
        (ledger, scope)
    }

    fn approved_dependency(person_id: PersonId, recipient: &str) -> ContextDependency {
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::try_new("c").unwrap(),
            ConnectorId::try_new("k").unwrap(),
            ExecutionOwnerId::try_new("o").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let now = chrono::Utc::now();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("r").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            ProcessingRestriction::ApprovedRecipient {
                recipient: recipient.into(),
                categories: vec![GrantDataCategory::Metadata],
            },
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"f".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now - chrono::Duration::minutes(1),
            now + chrono::Duration::minutes(5),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn attempt_id_is_preserved_end_to_end() {
        let person = PersonId::new();
        let transport = Arc::new(TestTransport::answer());
        let provider = TestProvider {
            profiles: vec![(device_profile("device", true), Arc::clone(&transport))],
        };
        let service = InferenceService::new(provider, AllowResolver, AllowAuthority);
        let attempt_id = RunId::new().as_uuid();
        let request = model_request(
            attempt_id,
            &person.to_string(),
            projection(DependencyCoverage::Independent, vec![DataClass::Personal]),
            None,
        );
        let (_ledger, scope) = scope();
        let response: ModelResponse = service.generate(request, &scope).await.unwrap();
        assert_eq!(response.attempt_id, attempt_id);
        assert_eq!(transport.seen_attempt.lock().unwrap().as_slice(), &[attempt_id]);
        assert_eq!(transport.calls(), 1);
    }

    #[tokio::test]
    async fn explicit_missing_profile_never_falls_back() {
        let person = PersonId::new();
        let transport = Arc::new(TestTransport::answer());
        let provider = TestProvider {
            profiles: vec![(device_profile("device", true), Arc::clone(&transport))],
        };
        let service = InferenceService::new(provider, AllowResolver, AllowAuthority);
        let request = model_request(
            RunId::new().as_uuid(),
            &person.to_string(),
            projection(DependencyCoverage::Independent, vec![DataClass::Personal]),
            Some("missing".into()),
        );
        let (_ledger, scope) = scope();
        assert_eq!(
            service.generate(request, &scope).await.err(),
            Some(AgentFailure::ModelUnavailable)
        );
        assert_eq!(transport.calls(), 0);
    }

    #[tokio::test]
    async fn auto_prefers_available_device_profile() {
        let person = PersonId::new();
        let device = Arc::new(TestTransport::answer());
        let server = Arc::new(TestTransport::answer());
        let provider = TestProvider {
            profiles: vec![
                (device_profile("device", true), Arc::clone(&device)),
                (external_profile("server", "ext", true), Arc::clone(&server)),
            ],
        };
        // External needs an approved dependency; Independent device wins first.
        let service = InferenceService::new(provider, AllowResolver, AllowAuthority);
        let request = model_request(
            RunId::new().as_uuid(),
            &person.to_string(),
            projection(DependencyCoverage::Independent, vec![DataClass::Personal]),
            None,
        );
        let (_ledger, scope) = scope();
        service.generate(request, &scope).await.unwrap();
        assert_eq!(device.calls(), 1);
        assert_eq!(server.calls(), 0);
    }

    #[tokio::test]
    async fn auto_uses_server_only_when_local_unavailable() {
        let person = PersonId::new();
        let device = Arc::new(TestTransport::answer());
        let server = Arc::new(TestTransport::answer());
        let provider = TestProvider {
            profiles: vec![
                (device_profile("device", false), Arc::clone(&device)),
                (external_profile("server", "ext", true), Arc::clone(&server)),
            ],
        };
        let service = InferenceService::new(provider, AllowResolver, AllowAuthority);
        let mut proj = projection(DependencyCoverage::Independent, vec![DataClass::Personal]);
        // Independent external is allowed when recipient authority holds.
        let request = model_request(RunId::new().as_uuid(), &person.to_string(), proj, None);
        let (_ledger, scope) = scope();
        service.generate(request, &scope).await.unwrap();
        assert_eq!(device.calls(), 0);
        assert_eq!(server.calls(), 1);
    }

    #[tokio::test]
    async fn access_denial_means_zero_provider_calls_and_zero_charge() {
        let person = PersonId::new();
        let transport = Arc::new(TestTransport::answer());
        let provider = TestProvider {
            profiles: vec![(external_profile("server", "ext", true), Arc::clone(&transport))],
        };
        let service = InferenceService::new(provider, DenyResolver, AllowAuthority);
        let mut proj = projection(
            DependencyCoverage::dependent(approved_dependency(person, "ext")).unwrap(),
            vec![DataClass::Personal],
        );
        let _ = &mut proj;
        let request = model_request(RunId::new().as_uuid(), &person.to_string(), proj, None);
        let (ledger, scope) = scope();
        assert_eq!(
            service.generate(request, &scope).await.err(),
            Some(AgentFailure::PolicyDenied)
        );
        assert_eq!(transport.calls(), 0);
        assert_eq!(ledger.snapshot().settled.tokens, 0);
    }

    #[tokio::test]
    async fn fallback_to_another_target_obtains_a_new_permit() {
        let person = PersonId::new();
        let failing = Arc::new(TestTransport::failing(AgentFailure::ServerModelUnavailable));
        let succeeding = Arc::new(TestTransport::answer());
        // Both device-ranked? Make failing device (rank 0 unavailable? No, need
        // two ranks: failing device-ranked gateway then succeeding external.
        // Simpler: two device profiles would be ambiguous (denied). Use one
        // device failing and one external succeeding with Independent.
        let provider = TestProvider {
            profiles: vec![
                (
                    ModelProfile {
                        id: "gateway".into(),
                        purpose: ModelPurpose::new(CANONICAL_MODEL_PURPOSE).unwrap(),
                        consumer: ModelConsumer::new(CANONICAL_MODEL_CONSUMER).unwrap(),
                        execution_location: ExecutionLocation::Gateway,
                        data_recipient: DataRecipient::Device,
                        capabilities: ModelCapabilities(vec![]),
                        available: true,
                    },
                    Arc::clone(&failing),
                ),
                (external_profile("server", "ext", true), Arc::clone(&succeeding)),
            ],
        };
        // Gateway (rank 1) is top when no Device rank 0 exists; it fails with
        // transport error so the service falls back to external (rank 2) with
        // a fresh Access permit.
        let service = InferenceService::new(provider, AllowResolver, AllowAuthority);
        let request = model_request(
            RunId::new().as_uuid(),
            &person.to_string(),
            projection(DependencyCoverage::Independent, vec![DataClass::Personal]),
            None,
        );
        let (_ledger, scope) = scope();
        // Gateway fails closed on ambiguity? Only one gateway, so it is tried.
        // Our planner denies same-rank ambiguity only; ranks differ so fallback occurs.
        let result = service.generate(request, &scope).await;
        // Gateway transport fails, fallback to server succeeds.
        assert!(result.is_ok());
        assert_eq!(failing.calls(), 1);
        assert_eq!(succeeding.calls(), 1);
    }

    #[tokio::test]
    async fn provider_failure_after_handoff_charges_unknown_estimate() {
        let person = PersonId::new();
        let transport = Arc::new(TestTransport::failing(AgentFailure::ServerModelUnavailable));
        let provider = TestProvider {
            profiles: vec![(device_profile("device", true), Arc::clone(&transport))],
        };
        let service = InferenceService::new(provider, AllowResolver, AllowAuthority);
        let request = model_request(
            RunId::new().as_uuid(),
            &person.to_string(),
            projection(DependencyCoverage::Independent, vec![DataClass::Personal]),
            None,
        );
        let (ledger, scope) = scope();
        assert_eq!(
            service.generate(request, &scope).await.err(),
            Some(AgentFailure::ServerModelUnavailable)
        );
        let snapshot = ledger.snapshot();
        // Unknown estimate is charged, not zero.
        assert!(snapshot.unknown_tokens > 0 || snapshot.settled.tokens > 0);
        assert_eq!(transport.calls(), 1);
    }

    #[tokio::test]
    async fn success_settles_actual_usage_exactly_once() {
        let person = PersonId::new();
        let transport = Arc::new(TestTransport::answer());
        let provider = TestProvider {
            profiles: vec![(device_profile("device", true), Arc::clone(&transport))],
        };
        let service = InferenceService::new(provider, AllowResolver, AllowAuthority);
        let request = model_request(
            RunId::new().as_uuid(),
            &person.to_string(),
            projection(DependencyCoverage::Independent, vec![DataClass::Personal]),
            None,
        );
        let (ledger, scope) = scope();
        let response = service.generate(request, &scope).await.unwrap();
        assert_eq!(response.usage.tokens, 10);
        assert_eq!(response.usage.cost_micros, 5);
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.settled.tokens, 10);
        assert_eq!(snapshot.settled.cost_micros, 5);
        assert_eq!(snapshot.settled.attempts, 1);
    }

    #[tokio::test]
    async fn post_response_revoke_drops_result_but_keeps_charge() {
        let person = PersonId::new();
        let transport = Arc::new(TestTransport::answer());
        let provider = TestProvider {
            profiles: vec![(device_profile("device", true), Arc::clone(&transport))],
        };
        // Independent device does not consult recipient authority, so use an
        // external dependent profile where revalidation consults it.
        let ext_transport = Arc::new(TestTransport::answer());
        let ext_provider = TestProvider {
            profiles: vec![(external_profile("server", "ext", true), Arc::clone(&ext_transport))],
        };
        let authority = CountingAuthority {
            calls: AtomicUsize::new(0),
            deny_from: 2,
        };
        let _ = (provider, transport);
        let service = InferenceService::new(ext_provider, AllowResolver, authority);
        let proj = projection(
            DependencyCoverage::dependent(approved_dependency(person, "ext")).unwrap(),
            vec![DataClass::Personal],
        );
        let request = model_request(RunId::new().as_uuid(), &person.to_string(), proj, None);
        let (ledger, scope) = scope();
        assert_eq!(
            service.generate(request, &scope).await.err(),
            Some(AgentFailure::PolicyDenied)
        );
        // Usage stays charged even though the response is discarded.
        assert_eq!(ledger.snapshot().settled.tokens, 10);
        assert_eq!(ext_transport.calls(), 1);
    }

    #[tokio::test]
    async fn invalid_provider_response_keeps_usage_charged() {
        let person = PersonId::new();
        let transport = Arc::new(TestTransport {
            calls: AtomicUsize::new(0),
            seen_attempt: std::sync::Mutex::new(Vec::new()),
            behavior: std::sync::Mutex::new(TransportBehavior::InvalidOutput {
                tokens: 7,
                cost: 3,
            }),
        });
        let provider = TestProvider {
            profiles: vec![(device_profile("device", true), Arc::clone(&transport))],
        };
        let service = InferenceService::new(provider, AllowResolver, AllowAuthority);
        let request = model_request(
            RunId::new().as_uuid(),
            &person.to_string(),
            projection(DependencyCoverage::Independent, vec![DataClass::Personal]),
            None,
        );
        let (ledger, scope) = scope();
        assert_eq!(
            service.generate(request, &scope).await.err(),
            Some(AgentFailure::ServerModelInvalidOutput)
        );
        assert_eq!(ledger.snapshot().settled.tokens, 7);
        assert_eq!(ledger.snapshot().settled.cost_micros, 3);
    }

    #[tokio::test]
    async fn canonical_public_types_contain_no_secret_or_endpoint() {
        let person = PersonId::new();
        let transport = Arc::new(TestTransport::answer());
        let provider = TestProvider {
            profiles: vec![(device_profile("device", true), Arc::clone(&transport))],
        };
        let observed = provider.observe_profiles().await;
        for prepared in &observed {
            let debug = format!("{:?}", prepared.profile);
            assert!(!debug.contains("bearer"));
            assert!(!debug.contains("token"));
            assert!(!debug.contains("http"));
            assert!(!debug.contains("127.0.0.1"));
        }
        let request = model_request(
            RunId::new().as_uuid(),
            &person.to_string(),
            projection(DependencyCoverage::Independent, vec![DataClass::Personal]),
            None,
        );
        let canonical = CanonicalModelRequest {
            attempt_id: request.attempt_id,
            envelope: request.projection.envelope.clone(),
            catalog: request.catalog.clone(),
            input_data_classes: vec![DataClass::Personal],
            remaining_tokens: 100,
            remaining_cost_micros: 100,
            max_output_bytes: 1024,
            deadline: Instant::now() + std::time::Duration::from_secs(5),
            cancellation: Cancellation::default(),
        };
        let debug = format!("{canonical:?}");
        assert!(!debug.to_lowercase().contains("bearer"));
        assert!(!debug.contains("127.0.0.1"));
        assert!(!debug.contains("http://"));
        assert!(!debug.contains("https://"));
        // The root canonical service type itself carries no secret route
        // bundle: its public generics are provider/resolver/authority traits,
        // never a bearer, base URL or endpoint string.
        let service_name = std::any::type_name::<
            InferenceService<TestProvider, AllowResolver, AllowAuthority>,
        >();
        assert!(!service_name.contains("bearer"));
    }
}


