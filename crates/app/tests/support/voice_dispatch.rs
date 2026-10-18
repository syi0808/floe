//! Real Voice/Vault custody with real Inference/Access/budget ownership. The
//! transport and native acknowledgements are synthetic, never an audio test.
use super::*;
use floe_access::{
    DependencyAuthorization, DependencyResolver, GatewayAdmission, ModelDispatchTarget,
    VerifiedGatewayBinding,
};
use floe_context_contract::{
    ContextDependency, DataClass, DependencyCoverage, LiveContextProjection, LiveProjectionSource,
    MemoryRevisionReference,
};
use floe_conversation_contract::SurfaceAnchor;
use floe_execution::{
    BoxFuture, Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelReservationCeiling, ModelUsage},
};
use floe_inference::{
    AdmittedDispatchTarget, CanonicalLiveStart, LiveDispatchCustody, LiveDispatchIntent,
    LiveModelPlanRequest, LiveModelProvider, LiveSessionControl, LocalObservation,
    ObservedLiveModelCapability, PreparedLiveModelProfile, PreparedLiveTransport, PrimaryAbsence,
    PrimaryObservation,
};
use floe_kernel::{PersonId, TraceContext};
use floe_model_contract::{
    LiveContextPayload, LiveModelCapabilities, LiveTransportLimits, LiveUsageObservation,
    ModelBindingDigest, ModelSelectionCommitment, ProcessingBoundary,
};
use floe_voice::{
    MediaState, RemoteState, VoiceBinding, VoiceMutation, VoiceMutationKind, VoiceStartIntent,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
struct Probe {
    starts: AtomicUsize,
    fenced: AtomicBool,
}
struct LiveProvider(Arc<Probe>);
struct LiveTransport(Arc<Probe>);
struct LiveSession(Arc<Probe>);
impl LiveSessionControl for LiveSession {
    fn fence(&self) {
        self.0.fenced.store(true, Ordering::SeqCst);
    }
}
impl PreparedLiveTransport for LiveTransport {
    type Session = LiveSession;
    fn validate_start(&self, request: &CanonicalLiveStart) -> Result<(), AgentFailure> {
        request.plan.validate()
    }
    fn start(
        self,
        request: CanonicalLiveStart,
        target: AdmittedDispatchTarget,
    ) -> BoxFuture<'static, Result<LiveSession, AgentFailure>> {
        Box::pin(async move {
            assert!(target.matches(&request.plan.binding_digest, ProcessingBoundary::Device));
            self.0.starts.fetch_add(1, Ordering::SeqCst);
            Ok(LiveSession(self.0))
        })
    }
}
impl LiveModelProvider for LiveProvider {
    type Transport = LiveTransport;
    fn observe_primary_live<'a>(
        &'a self,
        _: &'a LiveModelPlanRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<PrimaryObservation<PreparedLiveModelProfile<LiveTransport>>, ModelObservationError>,
    > {
        Box::pin(async {
            Ok(PrimaryObservation::Absent(
                PrimaryAbsence::NoGatewayConfigured,
            ))
        })
    }
    fn observe_local_live<'a>(
        &'a self,
        _: &'a LiveModelPlanRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<LocalObservation<PreparedLiveModelProfile<LiveTransport>>, ModelObservationError>,
    > {
        Box::pin(async move {
            Ok(LocalObservation::Available(PreparedLiveModelProfile {
                capability: ObservedLiveModelCapability {
                    purpose: floe_inference::LIVE_PRESENTATION_PURPOSE.into(),
                    consumer: floe_inference::LIVE_PRESENTATION_CONSUMER.into(),
                    capabilities: LiveModelCapabilities::conversational(),
                    limits: LiveTransportLimits {
                        initial_context_bytes: 16384,
                        append_bytes: 65536,
                        local_session_duration_ms: 60000,
                    },
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([3; 32]),
                    selection_commitment: ModelSelectionCommitment([4; 32]),
                },
                target: ModelDispatchTarget::Device,
                transport: LiveTransport(self.0.clone()),
            }))
        })
    }
}
struct Independent;
impl DependencyResolver for Independent {
    fn authorize<'a>(
        &'a self,
        _: &'a ContextDependency,
        _: &'a DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { panic!("independent fixture has no source dependency") })
    }
    fn authorize_memories<'a>(
        &'a self,
        _: PersonId,
        _: &'a [MemoryRevisionReference],
        _: &'a DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { panic!("independent fixture has no memory") })
    }
}
impl GatewayAdmission for Independent {
    fn admit<'a>(
        &'a self,
        _: &'a VerifiedGatewayBinding,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<VerifiedGatewayBinding, AgentFailure>> {
        Box::pin(async { panic!("local fixture has no Gateway") })
    }
}
#[test]
fn live_inference_consumes_exact_real_voice_start_once_and_does_not_fake_native_stop() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let surface = start_session(&host);
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let scope = ExecutionScope::root(
                Cancellation::new(),
                tokio::time::Instant::now() + WAIT_TIMEOUT,
                BudgetLedger::new(BudgetConfig::new(100, 100), ModelUsage::default()).root_lease(),
                TraceContext::new(Uuid::new_v4()),
            );
            let probe = Arc::new(Probe {
                starts: AtomicUsize::new(0),
                fenced: AtomicBool::new(false),
            });
            let inference = floe_inference::InferenceService::new(
                LiveProvider(probe.clone()),
                Independent,
                Independent,
            );
            let prepared = inference
                .prepare_live(
                    LiveModelPlanRequest {
                        actor: actor.clone(),
                        required_capabilities: LiveModelCapabilities::conversational(),
                    },
                    &scope,
                )
                .await
                .unwrap();
            let plan = prepared.plan().clone();
            let voice = Uuid::new_v4();
            let media = Uuid::new_v4();
            let through = owners
                .conversation
                .surface_head(&actor, surface, &scope)
                .await
                .unwrap();
            assert_eq!(through, SurfaceAnchor::empty(surface).unwrap());
            let projection = LiveContextProjection {
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                runtime_epoch: actor.runtime_epoch,
                voice_session_id: voice,
                plan_id: plan.operation_id,
                binding_digest: plan.binding_digest,
                projection_id: Uuid::new_v4(),
                projection_revision: 1,
                source: LiveProjectionSource::SurfacePrefix { through },
                coverage: DependencyCoverage::Independent,
                memories: vec![],
                input_data_classes: vec![DataClass::Synthetic],
                payload: LiveContextPayload::new("Synthetic initial context".into()).unwrap(),
            };
            let reservation = ModelReservationCeiling {
                tokens: 40,
                cost_micros: 60,
            };
            let start = VoiceStartIntent::prepare(
                VoiceBinding {
                    actor: actor.clone(),
                    surface_session_id: surface,
                    voice_session_id: voice,
                    media_lease_id: media,
                    plan_id: plan.operation_id,
                },
                &plan,
                &projection,
                reservation,
            )
            .unwrap();
            let claim = LiveDispatchIntent {
                actor: actor.clone(),
                voice_session_id: voice,
                surface_session_id: surface,
                media_lease_id: media,
                plan: plan.clone(),
                projection_digest: projection.digest().unwrap(),
                reservation,
            };
            let record = owners
                .voice
                .admit_start(&actor, start, &scope)
                .await
                .unwrap()
                .record;
            assert!(
                owners.voice.claim(&claim, &scope).await.is_err(),
                "unconfirmed native preparation cannot hand off a model"
            );
            let media_command = record.session.pending_media_command().unwrap().clone();
            owners
                .voice
                .mutate(
                    &actor,
                    VoiceMutation {
                        voice_session_id: voice,
                        command_id: Uuid::new_v4(),
                        expected_revision: record.revision,
                        kind: VoiceMutationKind::NativeReceipt {
                            command: media_command,
                            state: MediaState::Muted,
                        },
                    },
                    &scope,
                )
                .await
                .unwrap();
            let call = inference
                .start_live(
                    prepared,
                    floe_inference::LiveStartRequest {
                        voice_session_id: voice,
                        surface_session_id: surface,
                        media_lease_id: media,
                        projection,
                        reservation,
                    },
                    owners.voice.as_ref(),
                    &scope,
                )
                .await
                .unwrap();
            assert_eq!(probe.starts.load(Ordering::SeqCst), 1);
            let retained = owners
                .voice
                .read_call(&actor, voice, &scope)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(retained.session.remote(), RemoteState::Dispatching);
            assert_eq!(retained.session.media(), MediaState::Muted);
            assert!(
                !owners.voice.claim(&claim, &scope).await.unwrap(),
                "durable start replay cannot authorize another provider call"
            );
            let mut changed = claim;
            changed.projection_digest = [9; 32];
            assert!(matches!(
                owners.voice.claim(&changed, &scope).await,
                Err(AgentFailure::PolicyDenied)
            ));
            let receipt = call.finish(LiveUsageObservation::default()).unwrap();
            assert!(receipt.accounting.unknown_tokens && receipt.accounting.unknown_cost);
            assert!(probe.fenced.load(Ordering::SeqCst));
            assert_eq!(receipt.attempt_id, plan.operation_id);
            assert_eq!(
                owners
                    .voice
                    .settle_usage(&actor, voice, receipt, &scope)
                    .await
                    .unwrap(),
                receipt
            );
            assert_eq!(
                owners
                    .voice
                    .settle_usage(&actor, voice, receipt, &scope)
                    .await
                    .unwrap(),
                receipt
            );
            let mut changed_usage = receipt;
            changed_usage.charged_tokens -= 1;
            assert!(
                owners
                    .voice
                    .settle_usage(&actor, voice, changed_usage, &scope)
                    .await
                    .is_err()
            );

            let retained = owners
                .voice
                .read_call(&actor, voice, &scope)
                .await
                .unwrap()
                .unwrap();
            assert!(
                retained.session.stop_requested(),
                "terminal usage requests stop but does not confirm it"
            );
            assert!(
                !retained.session.local_lease_released(),
                "a transport fence is not an actual native Stop receipt"
            );
        });
    });
    assert_eq!(
        assert_script_clean(&recorder).generated.len(),
        0,
        "live presentation is not a second Manager generation loop"
    );
}
