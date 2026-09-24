use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Utc;
use floe_access::{
    ContextualRecipientAuthority, DependencyAuthorization, DependencyResolver, RecipientConsent,
    RecipientConsentStore, SystemConsentClock, grant_recipient_consent,
};
use floe_agent_contract::{
    AgentContext, AgentFailure, AllowedCatalog, BoxFuture, ContextDependency, DataClass,
    ModelCallOutcome, ModelConversation, ModelConversationEntry, ModelPort, ModelRequest,
    ModelStep,
    prompts::{
        BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION, CAPABILITY_PROTOCOL,
        CAPABILITY_PROTOCOL_REVISION, PromptAssembly, PromptComponentKind, PromptRole,
        product_component,
    },
};
use floe_context_contract::RecipientLineage;
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_inference::{
    CANONICAL_MODEL_CONSUMER, CANONICAL_MODEL_PURPOSE, DataRecipient, InferenceService,
    ModelProvider, SavedServerConnection,
};
use floe_kernel::{PersonId, RunId, TraceContext};
use floe_provider_adapters::{
    control::{CurrentSavedConnectionStore, SavedConnectionAdmission},
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
    pub result: Result<ModelCallOutcome, AgentFailure>,
    pub usage: ModelUsage,
    attempt_id: Uuid,
}

#[derive(Default)]
struct MemoryConsents {
    records: Mutex<HashMap<Uuid, RecipientConsent>>,
}

impl RecipientConsentStore for MemoryConsents {
    fn grant_consent<'a>(
        &'a self,
        consent: RecipientConsent,
    ) -> BoxFuture<'a, Result<RecipientConsent, AgentFailure>> {
        Box::pin(async move {
            consent.validate().map_err(|_| AgentFailure::InvalidInput)?;
            let mut records = self.records.lock().unwrap();
            if let Some(existing) = records.get(&consent.id()) {
                return Ok(existing.clone());
            }
            records.insert(consent.id(), consent.clone());
            Ok(consent)
        })
    }

    fn find_consent<'a>(
        &'a self,
        consent_id: Uuid,
    ) -> BoxFuture<'a, Result<Option<RecipientConsent>, AgentFailure>> {
        Box::pin(async move { Ok(self.records.lock().unwrap().get(&consent_id).cloned()) })
    }

    fn revoke_consent<'a>(&'a self, consent_id: Uuid) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            let mut records = self.records.lock().unwrap();
            let Some(existing) = records.get(&consent_id).cloned() else {
                return Err(AgentFailure::NotFound);
            };
            let revoked = existing
                .revoked()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            records.insert(consent_id, revoked);
            Ok(())
        })
    }

    fn prune_expired<'a>(&'a self, now_unix_ms: i64) -> BoxFuture<'a, Result<u64, AgentFailure>> {
        Box::pin(async move {
            let mut records = self.records.lock().unwrap();
            let before = records.len();
            records.retain(|_, consent| consent.expires_at().timestamp_millis() > now_unix_ms);
            Ok((before - records.len()) as u64)
        })
    }
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
    attempt_inner(saved, false).await
}

/// The consented path: grants the exact contextual consent the dispatch
/// derives, so an admitted pairing reaches the transport.
pub async fn attempt_with_consent(saved: &SavedServerConnection) -> Outcome {
    attempt_inner(saved, true).await
}

async fn attempt_inner(saved: &SavedServerConnection, grant_consent: bool) -> Outcome {
    let current = CurrentSavedConnectionStore::fixed(Some(saved.clone()));
    let provider =
        RootModelProvider::from_current_connection(&current, &saved.person_id, &saved.device_id)
            .unwrap();
    let admission =
        SavedConnectionAdmission::new(current, saved.person_id.clone(), saved.device_id.clone());
    let consents = MemoryConsents::default();
    let lineage = RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4()).unwrap();
    if grant_consent {
        let person = PersonId::from_uuid(Uuid::parse_str(&saved.person_id).unwrap()).unwrap();
        let consent = RecipientConsent::try_new(
            person,
            saved.device_id.clone(),
            saved.client_id.clone(),
            "OpenAI (Codex OAuth)",
            "server-model",
            CANONICAL_MODEL_PURPOSE,
            CANONICAL_MODEL_CONSUMER,
            vec![DataClass::Synthetic],
            vec![],
            lineage,
            Uuid::new_v4(),
            1,
            Utc::now(),
        )
        .unwrap();
        grant_recipient_consent(&consents, consent).await.unwrap();
    }
    let authority = ContextualRecipientAuthority::new(&consents, admission, SystemConsentClock);
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
        lineage: Some(lineage),
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
    println!(
        "real_model_attempts={} settled_tokens={} estimated_tokens={} failure={:?}",
        outcome.usage.attempts,
        outcome.usage.tokens,
        outcome.usage.estimated_tokens,
        outcome.result.as_ref().err()
    );
    let response = match outcome.result.expect("real configured Codex generation") {
        ModelCallOutcome::Ready(response) => response,
        ModelCallOutcome::NeedsUserAction(requirement) => {
            panic!(
                "live generation blocked unexpectedly: {}",
                requirement.recipient()
            )
        }
    };
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
}
