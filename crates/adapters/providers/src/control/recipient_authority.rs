//! Current saved-connection pairing admission for model dispatch.
//!
//! Each [`ModelConnectionAdmission::admit`] call reloads the current saved
//! connection and binds it to the verified person/device with
//! [`admit_saved_connection`]. Admission carries pairing identity only
//! (person/device/client). Contextual consent lives in the Access-owned consent
//! store, consulted by [`ContextualRecipientAuthority`] alongside this
//! admission. Credentials are never returned or logged; malformed,
//! foreign or absent state fails closed.

use floe_access::{AdmittedModelConnection, ModelConnectionAdmission};
use floe_agent_contract::AgentFailure;

use crate::control::server_connection::SavedServerConnectionStore;

/// Current pairing admission backed by a saved-connection store.
///
/// `Store` supplies the current saved connection on every check. Production
/// uses [`SavedServerConnectionStore`] (the host keychain slot); tests inject
/// a fake store with the same reload-per-check semantics.
pub struct SavedConnectionAdmission<Store> {
    store: Store,
    person_id: String,
    device_id: String,
}

impl<Store> SavedConnectionAdmission<Store> {
    pub fn new(store: Store, person_id: String, device_id: String) -> Self {
        Self {
            store,
            person_id,
            device_id,
        }
    }
}

impl<Store> ModelConnectionAdmission for SavedConnectionAdmission<Store>
where
    Store: floe_inference::SavedConnectionStore + Send + Sync,
{
    fn admit(&self) -> Result<AdmittedModelConnection, AgentFailure> {
        // Reload the current saved connection on every check. Any store
        // failure, absence or admission failure denies. All denials map to
        // `PolicyDenied` so Inference treats them as admission denials (no
        // transport fallback bypass).
        let stored = self.store.load().map_err(|_| AgentFailure::PolicyDenied)?;
        let Some(saved) = stored else {
            return Err(AgentFailure::PolicyDenied);
        };
        let admitted =
            floe_inference::admit_saved_connection(saved, &self.person_id, &self.device_id)
                .map_err(|_| AgentFailure::PolicyDenied)?;
        Ok(AdmittedModelConnection {
            person_id: admitted.person_id,
            device_id: admitted.device_id,
            client_id: admitted.client_id,
        })
    }
}

/// Fixed saved connection for construction-time test injection.
///
/// Holds one injected connection and returns a clone on every `load`, so the
/// authority still reloads per check. Production never constructs this: the
/// product path reads the host keychain slot through
/// [`SavedServerConnectionStore`].
#[derive(Clone)]
pub struct FixedSavedConnectionStore {
    saved: Option<floe_inference::SavedServerConnection>,
}

impl FixedSavedConnectionStore {
    pub fn fixed(saved: Option<floe_inference::SavedServerConnection>) -> Self {
        Self { saved }
    }
}

impl floe_inference::SavedConnectionStore for FixedSavedConnectionStore {
    fn load(&self) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
        Ok(self.saved.clone())
    }
}

/// Shared host-scoped store; clones reload the same current state.
#[derive(Clone)]
pub struct CurrentSavedConnectionStore {
    store: std::sync::Arc<dyn floe_inference::SavedConnectionStore + Send + Sync>,
}

impl CurrentSavedConnectionStore {
    pub fn new(store: impl floe_inference::SavedConnectionStore + Send + Sync + 'static) -> Self {
        Self {
            store: std::sync::Arc::new(store),
        }
    }

    pub fn host_keychain() -> Self {
        Self::new(SavedServerConnectionStore)
    }

    pub fn fixed(saved: Option<floe_inference::SavedServerConnection>) -> Self {
        Self::new(FixedSavedConnectionStore::fixed(saved))
    }
}

impl floe_inference::SavedConnectionStore for CurrentSavedConnectionStore {
    fn load(&self) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
        self.store.load()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use chrono::{DateTime, Utc};
    use floe_access::{
        AdmittedModelConnection, ContextualRecipientAuthority, DependencyAuthorization,
        DependencyResolver, ModelConnectionAdmission, RecipientConsent, RecipientConsentClock,
        RecipientConsentStore, grant_recipient_consent,
    };
    use floe_agent_contract::prompts::{PromptAssembly, PromptComponentKind, PromptRole};
    use floe_agent_contract::{
        AgentFailure, AllowedCatalog, AuthorizedModelProjection, BoxFuture, ContextEnvelope,
        ContextManifest, ContextualData, DataClass, DependencyCoverage, ModelPort, ModelRequest,
        ModelStep, ProjectionRef, RuntimeContext, ScopedInstructions,
    };
    use floe_context_contract::{ContextDependency, RecipientLineage};
    use floe_execution::{
        Cancellation,
        budget::{BudgetConfig, BudgetLedger, ModelUsage as LedgerUsage},
    };
    use floe_kernel::{PersonId, RunId, TraceContext};
    use tokio::time::Instant;
    use uuid::Uuid;

    use super::*;
    use floe_inference::{
        CANONICAL_MODEL_CONSUMER, CANONICAL_MODEL_PURPOSE, CanonicalModelRequest,
        CanonicalModelResponse, DataRecipient, ExecutionLocation, InferenceService,
        ModelCapabilities, ModelConsumer, ModelProfile, ModelProvider, ModelPurpose,
        PreparedModelProfile, PreparedModelTransport,
    };

    const PERSON: &str = "00000000-0000-4000-8000-000000000001";
    const DEVICE: &str = "local-device";
    const RECIPIENT: &str = "partner.example";

    fn saved() -> floe_inference::SavedServerConnection {
        floe_inference::SavedServerConnection {
            base_url: "http://127.0.0.1:8431".into(),
            token: "t".repeat(32),
            client_id: "paired-client".into(),
            person_id: PERSON.into(),
            device_id: DEVICE.into(),
        }
    }

    fn admission_for(
        saved: Option<floe_inference::SavedServerConnection>,
    ) -> SavedConnectionAdmission<FixedSavedConnectionStore> {
        SavedConnectionAdmission::new(
            FixedSavedConnectionStore::fixed(saved),
            PERSON.into(),
            DEVICE.into(),
        )
    }

    fn pairing() -> AdmittedModelConnection {
        AdmittedModelConnection {
            person_id: PERSON.into(),
            device_id: DEVICE.into(),
            client_id: "paired-client".into(),
        }
    }

    #[test]
    fn connection_removed_denies_admission() {
        let admission = admission_for(None);
        assert_eq!(admission.admit().err(), Some(AgentFailure::PolicyDenied));
    }

    #[test]
    fn saved_connection_rebound_to_another_person_or_device_denies() {
        let mut foreign_person = saved();
        foreign_person.person_id = "00000000-0000-4000-8000-000000000002".into();
        assert_eq!(
            admission_for(Some(foreign_person)).admit().err(),
            Some(AgentFailure::PolicyDenied)
        );
        let mut foreign_device = saved();
        foreign_device.device_id = "other-device".into();
        assert_eq!(
            admission_for(Some(foreign_device)).admit().err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn malformed_stored_connection_fails_closed() {
        let mut duplicated = saved();
        duplicated.client_id = "".into();
        assert_eq!(
            admission_for(Some(duplicated)).admit().err(),
            Some(AgentFailure::PolicyDenied)
        );
        let mut untrimmed = saved();
        untrimmed.client_id = " paired-client".into();
        assert_eq!(
            admission_for(Some(untrimmed)).admit().err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn unchanged_pairing_admits_identity() {
        assert_eq!(admission_for(Some(saved())).admit().unwrap(), pairing());
    }

    #[test]
    fn credential_rotation_with_unchanged_binding_still_admits() {
        // Admission never inspects credentials: rotating token/base URL while
        // person, device and client are unchanged keeps pairing admission.
        // Transport success/failure stays independent.
        let mut rotated = saved();
        rotated.token = "r".repeat(40);
        rotated.base_url = "http://127.0.0.1:8555".into();
        assert_eq!(admission_for(Some(rotated)).admit().unwrap(), pairing());
    }

    #[test]
    fn every_check_reloads_the_current_store() {
        struct MutableStore {
            current: Arc<Mutex<Option<floe_inference::SavedServerConnection>>>,
        }
        impl floe_inference::SavedConnectionStore for MutableStore {
            fn load(&self) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
                Ok(self.current.lock().unwrap().clone())
            }
        }
        let current = Arc::new(Mutex::new(Some(saved())));
        let store = CurrentSavedConnectionStore::new(MutableStore {
            current: current.clone(),
        });
        let admission = SavedConnectionAdmission::new(store.clone(), PERSON.into(), DEVICE.into());
        assert_eq!(admission.admit().unwrap(), pairing());
        // Mutating the current store is observed on the very next check: no
        // snapshot was kept at construction.
        *current.lock().unwrap() = None;
        assert_eq!(admission.admit().err(), Some(AgentFailure::PolicyDenied));
        assert!(
            crate::sources::ServerSourceClient::from_current_connection(&store, PERSON, DEVICE)
                .unwrap()
                .is_none()
        );
    }

    // Canonical handoff regressions below drive `InferenceService` with the
    // production authority composition (saved-connection admission plus the
    // Access-owned consent lookup), not an Access unit-test fake. A memory
    // consent store stands in for the vault.

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
                    existing
                        .validate()
                        .map_err(|_| AgentFailure::StorageUnavailable)?;
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
            Box::pin(async move {
                if consent_id.is_nil() {
                    return Err(AgentFailure::InvalidInput);
                }
                Ok(self.records.lock().unwrap().get(&consent_id).cloned())
            })
        }

        fn revoke_consent<'a>(
            &'a self,
            consent_id: Uuid,
        ) -> BoxFuture<'a, Result<(), AgentFailure>> {
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

        fn prune_expired<'a>(
            &'a self,
            now_unix_ms: i64,
        ) -> BoxFuture<'a, Result<u64, AgentFailure>> {
            Box::pin(async move {
                if now_unix_ms < 0 {
                    return Err(AgentFailure::InvalidInput);
                }
                let mut records = self.records.lock().unwrap();
                let before = records.len();
                records.retain(|_, consent| consent.expires_at().timestamp_millis() > now_unix_ms);
                Ok((before - records.len()) as u64)
            })
        }
    }

    #[derive(Clone, Copy)]
    struct FixedClock {
        now: DateTime<Utc>,
    }

    impl RecipientConsentClock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.now
        }
    }

    const GRANT_NOW_MS: i64 = 1_800_000_000_000;

    fn grant_now() -> DateTime<Utc> {
        DateTime::from_timestamp_millis(GRANT_NOW_MS).unwrap()
    }

    /// Grant the exact consent the canonical dispatch derives: the reviewed
    /// recipient/profile/purpose/consumer, the projected input classes, no
    /// source scopes (independent coverage), and the dispatch lineage, bound
    /// to the live pairing identity.
    async fn grant_dispatch_consent(
        consents: &MemoryConsents,
        person: PersonId,
        lineage: RecipientLineage,
    ) {
        let consent = RecipientConsent::try_new(
            person,
            DEVICE,
            "paired-client",
            RECIPIENT,
            "server",
            CANONICAL_MODEL_PURPOSE,
            CANONICAL_MODEL_CONSUMER,
            vec![DataClass::Personal],
            vec![],
            lineage,
            Uuid::new_v4(),
            1,
            grant_now(),
        )
        .unwrap();
        grant_recipient_consent(consents, consent).await.unwrap();
    }

    #[derive(Clone)]
    struct TestTransport {
        calls: Arc<AtomicUsize>,
        seen_targets: Arc<Mutex<Vec<Option<String>>>>,
        tokens: u64,
        cost: u64,
    }

    impl TestTransport {
        fn answer() -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                seen_targets: Arc::new(Mutex::new(Vec::new())),
                tokens: 10,
                cost: 5,
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl PreparedModelTransport for TestTransport {
        async fn generate(
            &self,
            _request: CanonicalModelRequest,
            target: floe_inference::AdmittedDispatchTarget,
        ) -> Result<CanonicalModelResponse, AgentFailure> {
            assert!(target.matches("server", Some(RECIPIENT)));
            self.seen_targets
                .lock()
                .unwrap()
                .push(target.recipient().map(str::to_owned));
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CanonicalModelResponse {
                output: vec![ModelStep::Answer {
                    text: "hello".into(),
                    artifacts: vec![],
                }],
                used_tokens: self.tokens,
                cost_micros: self.cost,
            })
        }
    }

    struct TestProvider {
        profiles: Vec<(ModelProfile, TestTransport)>,
    }

    impl ModelProvider for TestProvider {
        type Prepared = TestTransport;

        async fn observe_profiles(&self) -> Vec<PreparedModelProfile<Self::Prepared>> {
            self.profiles
                .iter()
                .map(|(profile, transport)| PreparedModelProfile {
                    profile: profile.clone(),
                    transport: transport.clone(),
                })
                .collect()
        }
    }

    struct AllowResolver;

    impl DependencyResolver for AllowResolver {
        fn authorize<'a>(
            &'a self,
            _dependency: &'a ContextDependency,
            _request: &'a DependencyAuthorization,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>> {
            Box::pin(async move { Ok(()) })
        }
    }

    /// Store that serves the consented connection for the first `live_loads`
    /// loads, then serves `revoked`. The production authority reloads per
    /// check, so this deterministically revokes between admit/consume (1) or
    /// between handoff and post-response revalidation (2).
    struct RevokingStore {
        consented: floe_inference::SavedServerConnection,
        revoked: Option<floe_inference::SavedServerConnection>,
        live_loads: usize,
        loads: AtomicUsize,
    }

    impl floe_inference::SavedConnectionStore for RevokingStore {
        fn load(&self) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
            let count = self.loads.fetch_add(1, Ordering::SeqCst);
            if count < self.live_loads {
                Ok(Some(self.consented.clone()))
            } else {
                Ok(self.revoked.clone())
            }
        }
    }

    fn external_profile() -> ModelProfile {
        ModelProfile {
            id: "server".into(),
            purpose: ModelPurpose::new(CANONICAL_MODEL_PURPOSE).unwrap(),
            consumer: ModelConsumer::new(CANONICAL_MODEL_CONSUMER).unwrap(),
            execution_location: ExecutionLocation::Remote,
            data_recipient: DataRecipient::external(RECIPIENT).unwrap(),
            capabilities: ModelCapabilities(vec![]),
            available: true,
        }
    }

    fn envelope() -> ContextEnvelope {
        use floe_agent_contract::prompts::product_component;
        use floe_agent_contract::prompts::{BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION};
        use floe_agent_contract::prompts::{CAPABILITY_PROTOCOL, CAPABILITY_PROTOCOL_REVISION};
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

    fn projection() -> AuthorizedModelProjection {
        AuthorizedModelProjection {
            projection_ref: ProjectionRef::new(),
            projection_revision: 1,
            envelope: envelope(),
            coverage: DependencyCoverage::Independent,
            input_data_classes: vec![DataClass::Personal],
        }
    }

    fn model_request(
        projection: AuthorizedModelProjection,
        lineage: RecipientLineage,
    ) -> ModelRequest {
        ModelRequest {
            attempt_id: RunId::new().as_uuid(),
            principal: PersonId::new().to_string(),
            projection,
            catalog: AllowedCatalog::default(),
            purpose: CANONICAL_MODEL_PURPOSE.into(),
            consumer: CANONICAL_MODEL_CONSUMER.into(),
            preferred_profile_id: Some("server".into()),
            replay: vec![],
            lineage: Some(lineage),
        }
    }

    fn lineage() -> RecipientLineage {
        RecipientLineage::try_new(Uuid::new_v4(), Uuid::new_v4()).unwrap()
    }

    fn scope() -> (BudgetLedger, floe_execution::ExecutionScope) {
        let ledger = BudgetLedger::new(
            BudgetConfig::new(100_000, 10_000_000),
            LedgerUsage::default(),
        );
        let scope = floe_execution::ExecutionScope::root(
            Cancellation::default(),
            Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            TraceContext::new(Uuid::new_v4()).with_run_id(RunId::new()),
        );
        (ledger, scope)
    }

    #[tokio::test]
    async fn saved_pairing_alone_cannot_approve_external_dispatch() {
        let person = PersonId::new();
        let mut saved = saved();
        saved.person_id = person.to_string();
        let consents = MemoryConsents::default();
        let authority = ContextualRecipientAuthority::new(
            &consents,
            SavedConnectionAdmission::new(
                FixedSavedConnectionStore::fixed(Some(saved)),
                person.to_string(),
                DEVICE.into(),
            ),
            FixedClock { now: grant_now() },
        );
        let transport = TestTransport::answer();
        let service = InferenceService::new(
            TestProvider {
                profiles: vec![(external_profile(), transport.clone())],
            },
            AllowResolver,
            authority,
        );
        let mut request = model_request(projection(), lineage());
        request.principal = person.to_string();
        let (ledger, scope) = scope();
        assert!(matches!(
            service.generate(request, &scope).await.unwrap(),
            floe_agent_contract::ModelCallOutcome::NeedsUserAction(_)
        ));
        assert_eq!(transport.calls(), 0);
        assert_eq!(ledger.snapshot().settled.tokens, 0);
    }

    #[tokio::test]
    async fn exact_contextual_consent_produces_consumed_target() {
        let person = PersonId::new();
        let mut saved = saved();
        saved.person_id = person.to_string();
        let dispatch_lineage = lineage();
        let consents = MemoryConsents::default();
        grant_dispatch_consent(&consents, person, dispatch_lineage).await;
        let authority = ContextualRecipientAuthority::new(
            &consents,
            SavedConnectionAdmission::new(
                FixedSavedConnectionStore::fixed(Some(saved)),
                person.to_string(),
                DEVICE.into(),
            ),
            FixedClock { now: grant_now() },
        );
        let transport = TestTransport::answer();
        let service = InferenceService::new(
            TestProvider {
                profiles: vec![(external_profile(), transport.clone())],
            },
            AllowResolver,
            authority,
        );
        let mut request = model_request(projection(), dispatch_lineage);
        request.principal = person.to_string();
        let (_ledger, scope) = scope();
        assert!(matches!(
            service.generate(request, &scope).await.unwrap(),
            floe_agent_contract::ModelCallOutcome::Ready(_)
        ));
        assert_eq!(transport.calls(), 1);
        assert_eq!(transport.seen_targets.lock().unwrap().as_slice(), [Some(RECIPIENT.into())]);
    }

    #[tokio::test]
    async fn recipient_revoked_before_handoff_never_posts_agent_request() {
        // The pairing is live only for the admit check; every later Access
        // check (consume fence) sees the connection removed, even though a
        // contextual consent covers the dispatch.
        let person = PersonId::new();
        let store = Arc::new(RevokingStore {
            consented: {
                let mut saved = saved();
                saved.person_id = person.to_string();
                saved.device_id = DEVICE.into();
                saved
            },
            revoked: None,
            live_loads: 1,
            loads: AtomicUsize::new(0),
        });
        struct Shared(Arc<RevokingStore>);
        impl floe_inference::SavedConnectionStore for Shared {
            fn load(&self) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
                self.0.load()
            }
        }
        let dispatch_lineage = lineage();
        let consents = MemoryConsents::default();
        grant_dispatch_consent(&consents, person, dispatch_lineage).await;
        let admission = SavedConnectionAdmission::new(
            Shared(Arc::clone(&store)),
            person.to_string(),
            DEVICE.into(),
        );
        let authority = ContextualRecipientAuthority::new(
            &consents,
            admission,
            FixedClock { now: grant_now() },
        );
        let transport = TestTransport::answer();
        let provider = TestProvider {
            profiles: vec![(external_profile(), transport.clone())],
        };
        let service = InferenceService::new(provider, AllowResolver, authority);
        let mut request = model_request(projection(), dispatch_lineage);
        request.principal = person.to_string();
        let (ledger, scope) = scope();
        assert_eq!(
            service.generate(request, &scope).await.err(),
            Some(AgentFailure::PolicyDenied)
        );
        // Revocation before the consume fence: the provider is never posted.
        assert_eq!(transport.calls(), 0);
        // No successful usage is fabricated for a denied handoff.
        assert_eq!(ledger.snapshot().settled.tokens, 0);
        assert_eq!(ledger.snapshot().settled.attempts, 0);
        // Admit reloaded, then consume reloaded and denied.
        assert!(store.loads.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn external_model_response_is_suppressed_after_recipient_consent_revocation() {
        // Admit and consume see the live pairing; post-response revalidation
        // sees the connection removed, suppressing the transmitted response.
        let person = PersonId::new();
        let store = Arc::new(RevokingStore {
            consented: {
                let mut saved = saved();
                saved.person_id = person.to_string();
                saved.device_id = DEVICE.into();
                saved
            },
            revoked: None,
            live_loads: 2,
            loads: AtomicUsize::new(0),
        });
        struct Shared(Arc<RevokingStore>);
        impl floe_inference::SavedConnectionStore for Shared {
            fn load(&self) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
                self.0.load()
            }
        }
        let dispatch_lineage = lineage();
        let consents = MemoryConsents::default();
        grant_dispatch_consent(&consents, person, dispatch_lineage).await;
        let admission = SavedConnectionAdmission::new(
            Shared(Arc::clone(&store)),
            person.to_string(),
            DEVICE.into(),
        );
        let authority = ContextualRecipientAuthority::new(
            &consents,
            admission,
            FixedClock { now: grant_now() },
        );
        let transport = TestTransport::answer();
        let provider = TestProvider {
            profiles: vec![(external_profile(), transport.clone())],
        };
        let service = InferenceService::new(provider, AllowResolver, authority);
        let mut request = model_request(projection(), dispatch_lineage);
        request.principal = person.to_string();
        let (ledger, scope) = scope();
        assert_eq!(
            service.generate(request, &scope).await.err(),
            Some(AgentFailure::PolicyDenied)
        );
        // The provider was posted exactly once before revocation suppressed
        // the response content.
        assert_eq!(transport.calls(), 1);
        // Already-consumed model usage stays charged.
        assert_eq!(ledger.snapshot().settled.tokens, 10);
        assert_eq!(ledger.snapshot().settled.cost_micros, 5);
        assert_eq!(ledger.snapshot().settled.attempts, 1);
        // The current store was consulted again after handoff.
        assert!(store.loads.load(Ordering::SeqCst) >= 3);
    }
}
