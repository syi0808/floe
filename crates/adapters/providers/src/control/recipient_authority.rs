//! Current saved-connection recipient authority for model dispatch.
//!
//! Each [`ModelDispatchRecipientAuthority::check_recipient`] call reloads the
//! current saved connection and binds it to the verified person/device with
//! [`admit_saved_connection`]. A copied recipient list is never stored: any
//! removal, recipient change, consent revocation or identity mismatch denies.
//! Credentials are never returned or logged; malformed, foreign, absent or
//! revoked state fails closed.

use floe_agent_contract::AgentFailure;

use crate::control::server_connection::SavedServerConnectionStore;

/// Current exact-recipient authority backed by a saved-connection store.
///
/// `Store` supplies the current saved connection on every check. Production
/// uses [`SavedServerConnectionStore`] (the host keychain slot); tests inject
/// a fake store with the same reload-per-check semantics.
pub struct SavedConnectionRecipientAuthority<Store> {
    store: Store,
    person_id: String,
    device_id: String,
}

impl<Store> SavedConnectionRecipientAuthority<Store> {
    pub fn new(store: Store, person_id: String, device_id: String) -> Self {
        Self {
            store,
            person_id,
            device_id,
        }
    }
}

impl<Store> floe_access::ModelDispatchRecipientAuthority
    for SavedConnectionRecipientAuthority<Store>
where
    Store: floe_inference::SavedConnectionStore + Send + Sync,
{
    fn check_recipient(&self, recipient: &str) -> Result<(), AgentFailure> {
        // Reload the current saved connection on every check. Any store
        // failure, absence, admission failure or consent mismatch denies.
        // All denials map to `PolicyDenied` so Inference treats them as
        // admission denials (no transport fallback bypass).
        let stored = self
            .store
            .load()
            .map_err(|_| AgentFailure::PolicyDenied)?;
        let Some(saved) = stored else {
            return Err(AgentFailure::PolicyDenied);
        };
        let admitted = floe_inference::admit_saved_connection(
            saved,
            &self.person_id,
            &self.device_id,
        )
        .map_err(|_| AgentFailure::PolicyDenied)?;
        if !admitted.allow_external {
            return Err(AgentFailure::PolicyDenied);
        }
        if admitted
            .external_recipients
            .iter()
            .any(|allowed| allowed == recipient)
        {
            Ok(())
        } else {
            Err(AgentFailure::PolicyDenied)
        }
    }
}

/// Test-only fixed saved connection with current-store semantics.
///
/// Holds one injected connection and returns a clone on every `load`, so the
/// authority still reloads per check. Production never constructs this: the
/// product path passes `None` and reads the host keychain slot through
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

/// The current saved-connection source for the canonical root authority.
///
/// `Keychain` is the production source: the host keychain slot re-read on
/// every recipient check. `Fixed` carries a test-only injected connection
/// with the same reload-per-check shape.
pub enum CurrentSavedConnectionStore {
    Keychain(SavedServerConnectionStore),
    Fixed(FixedSavedConnectionStore),
}

impl floe_inference::SavedConnectionStore for CurrentSavedConnectionStore {
    fn load(&self) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
        match self {
            Self::Keychain(store) => store.load(),
            Self::Fixed(store) => store.load(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use floe_access::{DependencyAuthorization, DependencyResolver, ModelDispatchRecipientAuthority};
    use floe_agent_contract::{
        AgentFailure, AllowedCatalog, AuthorizedModelProjection, ContextEnvelope, ContextManifest,
        ContextualData, DataClass, DependencyCoverage, ModelPort, ModelRequest, ModelStep,
        ProjectionRef, RuntimeContext, ScopedInstructions,
    };
    use floe_agent_contract::prompts::{PromptAssembly, PromptComponentKind, PromptRole};
    use floe_context_contract::ContextDependency;
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
            allow_external: true,
            external_recipients: vec![RECIPIENT.into()],
        }
    }

    fn authority_for(
        saved: Option<floe_inference::SavedServerConnection>,
    ) -> SavedConnectionRecipientAuthority<FixedSavedConnectionStore> {
        SavedConnectionRecipientAuthority::new(
            FixedSavedConnectionStore::fixed(saved),
            PERSON.into(),
            DEVICE.into(),
        )
    }

    #[test]
    fn connection_removed_denies_recipient() {
        let authority = authority_for(None);
        assert_eq!(
            authority.check_recipient(RECIPIENT).err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn allow_external_revoked_denies_recipient() {
        let mut revoked = saved();
        revoked.allow_external = false;
        revoked.external_recipients = vec![];
        let authority = authority_for(Some(revoked));
        assert_eq!(
            authority.check_recipient(RECIPIENT).err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn recipient_removed_denies_exact_recipient() {
        let mut removed = saved();
        removed.allow_external = false;
        removed.external_recipients = vec![];
        // Admitted shape with external disabled carries no recipients, so the
        // previously consented exact recipient no longer holds authority.
        let authority = authority_for(Some(removed));
        assert_eq!(
            authority.check_recipient(RECIPIENT).err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn different_recipient_added_while_requested_removed_denies() {
        let mut replaced = saved();
        replaced.external_recipients = vec!["someone-else.example".into()];
        let authority = authority_for(Some(replaced));
        assert_eq!(
            authority.check_recipient(RECIPIENT).err(),
            Some(AgentFailure::PolicyDenied)
        );
        // The replacement itself is exact-matched, proving equality is exact
        // rather than a blanket external allow.
        assert!(authority.check_recipient("someone-else.example").is_ok());
    }

    #[test]
    fn saved_connection_rebound_to_another_person_or_device_denies() {
        let mut foreign_person = saved();
        foreign_person.person_id = "00000000-0000-4000-8000-000000000002".into();
        assert_eq!(
            authority_for(Some(foreign_person))
                .check_recipient(RECIPIENT)
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
        let mut foreign_device = saved();
        foreign_device.device_id = "other-device".into();
        assert_eq!(
            authority_for(Some(foreign_device))
                .check_recipient(RECIPIENT)
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn malformed_stored_connection_fails_closed() {
        // Duplicate recipients violate admission invariants.
        let mut duplicated = saved();
        duplicated.external_recipients = vec![RECIPIENT.into(), RECIPIENT.into()];
        assert_eq!(
            authority_for(Some(duplicated))
                .check_recipient(RECIPIENT)
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
        // allow_external inconsistent with the recipient list violates
        // admission invariants.
        let mut inconsistent = saved();
        inconsistent.allow_external = true;
        inconsistent.external_recipients = vec![];
        assert_eq!(
            authority_for(Some(inconsistent))
                .check_recipient(RECIPIENT)
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
        // Untrimmed recipient violates admission invariants.
        let mut untrimmed = saved();
        untrimmed.external_recipients = vec![" partner.example".into()];
        assert_eq!(
            authority_for(Some(untrimmed))
                .check_recipient(RECIPIENT)
                .err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn unchanged_exact_consent_is_accepted() {
        let authority = authority_for(Some(saved()));
        assert!(authority.check_recipient(RECIPIENT).is_ok());
        // Exact match is case-sensitive: a near miss still denies.
        assert_eq!(
            authority.check_recipient("Partner.Example").err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    #[test]
    fn credential_rotation_with_unchanged_recipient_still_passes_authority() {
        // Access never inspects credentials: rotating token/base URL while the
        // exact recipient, person, device and consent flag are unchanged keeps
        // recipient authority. Transport success/failure stays independent.
        let mut rotated = saved();
        rotated.token = "r".repeat(40);
        rotated.base_url = "http://127.0.0.1:8555".into();
        let authority = authority_for(Some(rotated));
        assert!(authority.check_recipient(RECIPIENT).is_ok());
    }

    #[test]
    fn every_check_reloads_the_current_store() {
        use std::sync::Mutex;
        struct MutableStore {
            current: Mutex<Option<floe_inference::SavedServerConnection>>,
        }
        impl floe_inference::SavedConnectionStore for MutableStore {
            fn load(&self) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
                Ok(self.current.lock().unwrap().clone())
            }
        }
        let store = Arc::new(MutableStore {
            current: Mutex::new(Some(saved())),
        });
        struct Shared<'a>(&'a MutableStore);
        impl floe_inference::SavedConnectionStore for Shared<'_> {
            fn load(
                &self,
            ) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
                self.0.load()
            }
        }
        let authority =
            SavedConnectionRecipientAuthority::new(Shared(&store), PERSON.into(), DEVICE.into());
        assert!(authority.check_recipient(RECIPIENT).is_ok());
        // Mutating the current store is observed on the very next check: no
        // snapshot was kept at construction.
        *store.current.lock().unwrap() = None;
        assert_eq!(
            authority.check_recipient(RECIPIENT).err(),
            Some(AgentFailure::PolicyDenied)
        );
    }

    // Canonical handoff regressions below drive `InferenceService` with the
    // production authority type, not an Access unit-test fake.

    #[derive(Clone)]
    struct TestTransport {
        calls: Arc<AtomicUsize>,
        tokens: u64,
        cost: u64,
    }

    impl TestTransport {
        fn answer() -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
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
        ) -> Result<CanonicalModelResponse, AgentFailure> {
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
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), AgentFailure>> + Send + 'a>>
        {
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
        use floe_agent_contract::prompts::{BEHAVIOR_KERNEL, BEHAVIOR_KERNEL_REVISION};
        use floe_agent_contract::prompts::{CAPABILITY_PROTOCOL, CAPABILITY_PROTOCOL_REVISION};
        use floe_agent_contract::prompts::product_component;
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

    fn model_request(projection: AuthorizedModelProjection) -> ModelRequest {
        ModelRequest {
            attempt_id: RunId::new().as_uuid(),
            principal: PersonId::new().to_string(),
            projection,
            catalog: AllowedCatalog::default(),
            purpose: CANONICAL_MODEL_PURPOSE.into(),
            consumer: CANONICAL_MODEL_CONSUMER.into(),
            preferred_profile_id: Some("server".into()),
            replay: vec![],
        }
    }

    fn scope() -> (BudgetLedger, floe_execution::ExecutionScope) {
        let ledger =
            BudgetLedger::new(BudgetConfig::new(100_000, 10_000_000), LedgerUsage::default());
        let scope = floe_execution::ExecutionScope::root(
            Cancellation::default(),
            Instant::now() + std::time::Duration::from_secs(30),
            ledger.work_lease(),
            TraceContext::new(Uuid::new_v4()).with_run_id(RunId::new()),
        );
        (ledger, scope)
    }

    #[tokio::test]
    async fn recipient_revoked_before_handoff_never_posts_agent_request() {
        // The store consents only for the admit check; every later Access
        // check (consume fence) sees the connection removed.
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
            fn load(
                &self,
            ) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
                self.0.load()
            }
        }
        let authority = SavedConnectionRecipientAuthority::new(
            Shared(Arc::clone(&store)),
            person.to_string(),
            DEVICE.into(),
        );
        let transport = TestTransport::answer();
        let provider = TestProvider {
            profiles: vec![(external_profile(), transport.clone())],
        };
        let service = InferenceService::new(provider, AllowResolver, authority);
        let mut request = model_request(projection());
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
        // Admit and consume see consent; post-response revalidation sees the
        // connection removed.
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
            fn load(
                &self,
            ) -> Result<Option<floe_inference::SavedServerConnection>, AgentFailure> {
                self.0.load()
            }
        }
        let authority = SavedConnectionRecipientAuthority::new(
            Shared(Arc::clone(&store)),
            person.to_string(),
            DEVICE.into(),
        );
        let transport = TestTransport::answer();
        let provider = TestProvider {
            profiles: vec![(external_profile(), transport.clone())],
        };
        let service = InferenceService::new(provider, AllowResolver, authority);
        let mut request = model_request(projection());
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
