use floe_access::{DependencyAuthorization, DependencyResolver};
use floe_agent_contract::{
    AgentContext, AgentFailure, AllowedCatalog, ContextDependency, DataClass, ModelConversation,
    ModelConversationEntry, ModelPort, ModelRequest, ModelResponse, ModelStep,
    prompts::{
        BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION, CAPABILITY_PROTOCOL,
        CAPABILITY_PROTOCOL_REVISION, PromptAssembly, PromptComponentKind, PromptRole,
        product_component,
    },
};
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_inference::{
    CANONICAL_MODEL_CONSUMER, CANONICAL_MODEL_PURPOSE, DataRecipient, InferenceService,
    ModelProvider, SavedServerConnection,
};
use floe_kernel::{RunId, TraceContext};
use floe_provider_adapters::{
    control::{CurrentSavedConnectionStore, SavedConnectionRecipientAuthority},
    models::RootModelProvider,
};
use tokio::time::{Duration, Instant};
use uuid::Uuid;

struct NoSourceDependencies;

impl DependencyResolver for NoSourceDependencies {
    fn authorize<'a>(
        &'a self,
        _: &'a ContextDependency,
        _: &'a DependencyAuthorization,
    ) -> floe_agent_contract::BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { Err(AgentFailure::PolicyDenied) })
    }
}

pub struct Outcome {
    pub result: Result<ModelResponse, AgentFailure>,
    pub usage: ModelUsage,
    attempt_id: Uuid,
}

pub async fn assert_remote_profile(saved: &SavedServerConnection) {
    let current = CurrentSavedConnectionStore::fixed(Some(saved.clone()));
    let provider =
        RootModelProvider::from_current_connection(&current, &saved.person_id, &saved.device_id)
            .unwrap();
    let profiles = provider.observe_profiles().await;
    let profile = &profiles
        .iter()
        .find(|entry| entry.profile.id == "server-model")
        .expect("configured real everyday_assistance profile")
        .profile;
    assert!(profile.available);
    assert_eq!(profile.purpose.as_str(), CANONICAL_MODEL_PURPOSE);
    assert_eq!(
        profile.data_recipient,
        DataRecipient::External("OpenAI (Codex OAuth)".into())
    );
}

pub async fn attempt(saved: &SavedServerConnection) -> Outcome {
    let current = CurrentSavedConnectionStore::fixed(Some(saved.clone()));
    let provider =
        RootModelProvider::from_current_connection(&current, &saved.person_id, &saved.device_id)
            .unwrap();
    let authority = SavedConnectionRecipientAuthority::new(
        current,
        saved.person_id.clone(),
        saved.device_id.clone(),
    );
    let service = InferenceService::new(provider, NoSourceDependencies, authority);
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
                "manager",
                1,
                "Answer the fictional validation greeting briefly. Do not use tools or delegate.",
            ),
            product_component(
                PromptComponentKind::CapabilityProtocol,
                "capability-protocol",
                CAPABILITY_PROTOCOL_REVISION,
                CAPABILITY_PROTOCOL,
            ),
        ],
    };
    let context = AgentContext {
        projection_version: 1,
        persona: None,
        memories: vec![],
        evidence: vec![],
        optional_context_issues: vec![],
    };
    let catalog = AllowedCatalog::default();
    let projection =
        floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
            role: floe_context::ContextProjectionRole::Manager,
            purpose: CANONICAL_MODEL_PURPOSE,
            response_contract: "Return one short friendly greeting, without tools or delegation.",
            correction: None,
            prompt,
            conversation: ModelConversation {
                history: vec![],
                current_turn: vec![ModelConversationEntry::User {
                    message_id: Uuid::new_v4(),
                    text: "Say hello. This is fictional validation data, not personal information."
                        .into(),
                }],
            },
            agent_context: &context,
            catalog: &catalog,
            active_experts: &[],
            authorized_history_dependencies: &[],
            input_data_classes: vec![DataClass::Synthetic],
            max_output_bytes: 4096,
        })
        .unwrap();
    let attempt_id = Uuid::new_v4();
    let request = ModelRequest {
        attempt_id,
        principal: saved.person_id.clone(),
        projection,
        catalog,
        purpose: CANONICAL_MODEL_PURPOSE.into(),
        consumer: CANONICAL_MODEL_CONSUMER.into(),
        preferred_profile_id: Some("server-model".into()),
        replay: vec![],
    };
    let ledger = BudgetLedger::new(BudgetConfig::new(8192, 1_000_000), ModelUsage::default());
    let scope = ExecutionScope::root(
        Cancellation::default(),
        Instant::now() + Duration::from_secs(40),
        ledger.work_lease(),
        TraceContext::new(Uuid::new_v4()).with_run_id(RunId::new()),
    );
    let result = service.generate(request, &scope).await;
    Outcome {
        result,
        usage: ledger.usage(),
        attempt_id,
    }
}

pub fn assert_generated(outcome: Outcome) {
    let response = outcome.result.expect("real configured Codex generation");
    assert_eq!(response.attempt_id, outcome.attempt_id);
    assert_eq!(outcome.usage.attempts, 1);
    assert_eq!(outcome.usage.tokens, response.usage.tokens);
    assert_eq!(outcome.usage.cost_micros, response.usage.cost_micros);
    assert_eq!(outcome.usage.estimated_tokens, 0);
    assert!(
        response
            .steps
            .iter()
            .any(|step| matches!(step, ModelStep::Answer { text, .. } if !text.trim().is_empty()))
    );
    assert!(
        response
            .steps
            .iter()
            .all(|step| matches!(step, ModelStep::Answer { .. } | ModelStep::Preamble { .. }))
    );
    println!(
        "real_model_attempts={} settled_tokens={}",
        outcome.usage.attempts, outcome.usage.tokens
    );
}
