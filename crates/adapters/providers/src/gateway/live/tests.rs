use super::*;
use floe_access::{
    GatewayCredentialExpectation, GatewayTrustReader, RemoteProducerIdentity,
    VerifiedGatewayBinding,
};
use floe_connections::{
    GatewayCredentialMaterial, GatewayCredentialRead, GatewayCredentialSnapshot,
    GatewayPrivateReader, PairingError, PairingPrivateSnapshot,
};
use floe_execution::{
    Cancellation,
    budget::{BudgetConfig, BudgetLedger, ModelReservationCeiling, ModelUsage},
};
use floe_inference::{LiveEventTransport, LiveSessionControl};
use floe_kernel::{PersonId, TraceContext};
use floe_model_contract::{LiveAppendKind, LiveAppendPayload, LiveContextPayload, LiveModelPlan};
use floe_native::live_media::{
    LiveMediaLease, LiveMediaRegistry, LiveMediaStopObservation, LiveSessionDescription,
    ProcessLiveMediaRegistry,
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use uuid::Uuid;

struct Trust;
impl GatewayTrustReader for Trust {
    fn credential_expectation<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<GatewayCredentialExpectation, AgentFailure>> {
        Box::pin(async { Ok(GatewayCredentialExpectation::Unpaired) })
    }
    fn pinned_producer<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<RemoteProducerIdentity, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::PolicyDenied) })
    }
}
struct Reader {
    binding: VerifiedGatewayBinding,
    endpoint: String,
    bearer: String,
}
impl GatewayPrivateReader for Reader {
    fn pairing_private<'a>(
        &'a self,
        _: Uuid,
        _: PersonId,
        _: &'a str,
        _: u64,
    ) -> BoxFuture<'a, Result<PairingPrivateSnapshot, PairingError>> {
        Box::pin(async { Err(PairingError::Rejected) })
    }
    fn credential<'a>(
        &'a self,
        person: PersonId,
        device: &'a str,
    ) -> BoxFuture<'a, Result<GatewayCredentialRead, PairingError>> {
        Box::pin(async move {
            if self.binding.person_id != person.to_string() || self.binding.device_id != device {
                return Err(PairingError::ForeignIdentity);
            }
            Ok(GatewayCredentialRead::Active(GatewayCredentialSnapshot {
                operation_id: Uuid::new_v4(),
                binding: self.binding.clone(),
                endpoint: self.endpoint.clone(),
                bearer: GatewayCredentialMaterial::new(self.bearer.as_bytes().to_vec()).unwrap(),
            }))
        })
    }
}
struct Native {
    binding: LiveMediaBinding,
    ended: Cancellation,
    answer: AtomicBool,
}
impl LiveMediaLease for Native {
    fn binding(&self) -> &LiveMediaBinding {
        &self.binding
    }
    fn validate_current(&self) -> Result<(), AgentFailure> {
        if self.ended.is_cancelled() {
            Err(AgentFailure::Interrupted)
        } else {
            Ok(())
        }
    }
    fn offer<'a>(
        &'a self,
        _: Instant,
        c: &'a Cancellation,
    ) -> BoxFuture<'a, Result<LiveSessionDescription, AgentFailure>> {
        Box::pin(async move {
            assert!(!c.is_cancelled());
            LiveSessionDescription::new("v=0\r\ns=synthetic-native-offer\r\n".into())
        })
    }
    fn apply_answer<'a>(
        &'a self,
        a: LiveSessionDescription,
        _: Instant,
        _: &'a Cancellation,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.validate_current()?;
            assert!(a.as_str().starts_with("v=0\r\n"));
            self.answer.store(true, Ordering::Release);
            Ok(())
        })
    }
    fn fence(&self) {
        self.ended.cancel()
    }
    fn lifetime_cancellation(&self) -> Cancellation {
        self.ended.clone()
    }
    fn request_stop_once<'a>(&'a self, _: Instant) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.fence();
            Ok(())
        })
    }
    fn observe_stop<'a>(
        &'a self,
        _: Instant,
    ) -> BoxFuture<'a, Result<LiveMediaStopObservation, AgentFailure>> {
        Box::pin(async move {
            Ok(LiveMediaStopObservation {
                binding: self.binding.clone(),
                capture_stopped: true,
                playback_stopped: true,
            })
        })
    }
}
struct Media(Arc<ProcessLiveMediaRegistry>);
impl LiveMediaRegistry for Media {
    fn lease(&self, binding: &LiveMediaBinding) -> Result<Arc<dyn LiveMediaLease>, AgentFailure> {
        self.0.lease(binding)
    }
}
#[test]
fn rejects_fabricated_live_provider_and_false_write_evidence() {
    let absent: wire::Capability = wire::decode(
        br#"{"schema_version":1,"purpose":"live_presentation","status":"not_configured"}"#,
    )
    .unwrap();
    absent.validate().unwrap();
    assert!(wire::decode::<wire::Capability>(br#"{"schema_version":1,"schema_version":1,"purpose":"live_presentation","status":"not_configured"}"#).is_err());
    let mutated:wire::Capability=wire::decode(br#"{"schema_version":1,"purpose":"live_presentation","status":"available","capability_revision":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","profile":{"provider_id":"other","model_id":"gpt-live-1-codex","endpoint":"https://example.invalid","capabilities":["duplex_audio"],"limits":{"initial_context_bytes":1,"append_bytes":1,"local_session_duration_ms":1},"protocol_source":"untrusted","experimental":true}}"#).unwrap();
    assert!(mutated.validate().is_err());
    let false_write:wire::Write=wire::decode(br#"{"schema_version":1,"operation_id":"id","phase":"written","attempted":false,"completed_frames":0}"#).unwrap();
    assert!(false_write.validate("id").is_err());
    let injected:wire::Event=wire::decode(br#"{"schema_version":1,"ticket_id":"id","sequence":1,"digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","event":{"kind":"ended","evidence":"provider_confirmed","usage":{"tokens":0,"cost_micros":0}}}"#).unwrap();
    assert!(injected.validate("id").is_err());
}

#[tokio::test]
#[ignore = "requires the Go live-control synthetic fixture; invoked by its explicit cross-language gate"]
async fn rust_live_control_to_go_e2e() {
    let person = PersonId(Uuid::parse_str(&std::env::var("FLOE_LIVE_PERSON").unwrap()).unwrap());
    let actor = OwnerActor {
        person_id: person,
        device_id: "synthetic-device".into(),
        runtime_epoch: 1,
    };
    let binding = VerifiedGatewayBinding {
        person_id: person.to_string(),
        device_id: actor.device_id.clone(),
        client_id: "synthetic-live-client".into(),
        producer_instance: "synthetic-live-producer".into(),
        producer_key_fingerprint: "a".repeat(64),
        producer_audience: "floe".into(),
        enrollment_id: "synthetic-live-enrollment".into(),
        credential_generation: 1,
    };
    let reader = Reader {
        binding,
        endpoint: std::env::var("FLOE_LIVE_ENDPOINT").unwrap(),
        bearer: std::env::var("FLOE_LIVE_BEARER").unwrap(),
    };
    let store = GatewayCredentialStore::new(Arc::new(Trust), Arc::new(reader), &actor).unwrap();
    let media = Arc::new(Media(Arc::new(ProcessLiveMediaRegistry::new())));
    let provider = GatewayLiveModelProvider::new(store, media.clone());
    let budget = BudgetLedger::new(BudgetConfig::new(10000, 10000), ModelUsage::default());
    let scope = ExecutionScope::root(
        Cancellation::new(),
        Instant::now() + Duration::from_secs(15),
        budget.work_lease(),
        TraceContext::new(Uuid::new_v4()),
    );
    let request = LiveModelPlanRequest {
        actor: actor.clone(),
        required_capabilities: LiveModelCapabilities::conversational(),
    };
    let PrimaryObservation::Available(profile) = provider
        .observe_primary_live(&request, &scope)
        .await
        .unwrap()
    else {
        panic!("configured Go live purpose absent")
    };
    let plan = LiveModelPlan {
        operation_id: Uuid::new_v4(),
        actor: actor.clone(),
        capabilities: profile.capability.capabilities,
        limits: profile.capability.limits,
        boundary: profile.capability.boundary,
        binding_digest: profile.capability.binding_digest,
        selection_commitment: profile.capability.selection_commitment,
    };
    let voice = Uuid::new_v4();
    let lease = Uuid::new_v4();
    let native = Arc::new(Native {
        binding: LiveMediaBinding {
            actor: actor.clone(),
            voice_session_id: voice,
            media_lease_id: lease,
            plan_id: plan.operation_id,
        },
        ended: Cancellation::new(),
        answer: AtomicBool::new(false),
    });
    media.0.register(native.clone()).unwrap();
    let start = CanonicalLiveStart {
        actor,
        voice_session_id: voice,
        media_lease_id: lease,
        plan,
        context: LiveContextPayload::new("synthetic initial context ".repeat(5000)).unwrap(),
        input_data_classes: vec![floe_context_contract::DataClass::Synthetic],
        reservation: ModelReservationCeiling {
            tokens: 100,
            cost_micros: 100,
        },
        deadline: scope.deadline(),
        cancellation: scope.cancellation().clone(),
    };
    profile.transport.validate_start(&start).unwrap();
    // This test targets the actual Rust/Go/native-port transport composition; the
    // separate registered Voice/App tests own durable intent and Access admission.
    let session = session::start(profile.transport, start).await.unwrap();
    assert!(native.answer.load(Ordering::Acquire));
    let first = session.observe(&scope).await.unwrap();
    let replay = session.observe(&scope).await.unwrap();
    assert_eq!(first, replay);
    session.acknowledge(first.sequence, &scope).await.unwrap();
    session.acknowledge(first.sequence, &scope).await.unwrap();
    let append = LiveAppendPayload {
        operation_id: Uuid::new_v4(),
        kind: LiveAppendKind::PresentableResult,
        delegation_id: Some("delegation".into()),
        text: "synthetic manager result".into(),
    };
    session.append(&append, &scope).await.unwrap();
    session.append(&append, &scope).await.unwrap();
    session.fence();
    assert!(native.ended.is_cancelled());
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(session);
    tokio::time::sleep(Duration::from_millis(100)).await;
}
