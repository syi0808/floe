//! Synthetic selected transport, real Access fences and budget owner.
use super::*;
use crate::{
    LiveModelPlanRequest, LocalObservation, ModelObservationError, ObservedLiveModelCapability,
    PreparedLiveModelProfile, PrimaryAbsence, PrimaryObservation,
};
use floe_agent_contract::{OwnerActor, PersonId, TraceContext};
use floe_context_contract::{
    ContextDependency, DataClass, DependencyCoverage, LiveContextProjection, LiveProjectionSource,
    MemoryRevisionReference,
};
use floe_conversation_contract::SurfaceAnchor;
use floe_execution::{
    BoxFuture,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_model_contract::{
    LiveContextPayload, LiveModelCapabilities, LiveTransportLimits, ModelBindingDigest,
    ModelSelectionCommitment, ProcessingBoundary,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use uuid::Uuid;

struct FenceGate {
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
}
struct State {
    fence_gate: Mutex<Option<FenceGate>>,
    owner_stop: Cancellation,
    stalled: tokio::sync::Notify,
    live: AtomicBool,
    stall: AtomicBool,
    claims: AtomicUsize,
    starts: AtomicUsize,
    reads: AtomicUsize,
    fenced: AtomicBool,
    reject_preflight: AtomicBool,
    fail_start: AtomicBool,
    revoke_in_claim: AtomicBool,
    revoke_in_start: AtomicBool,
    existing_claim: AtomicBool,
    stall_in_start: AtomicBool,
}
impl State {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            fence_gate: Mutex::new(None),
            owner_stop: Cancellation::new(),
            stalled: tokio::sync::Notify::new(),
            live: AtomicBool::new(true),
            stall: AtomicBool::new(false),
            claims: AtomicUsize::new(0),
            starts: AtomicUsize::new(0),
            reads: AtomicUsize::new(0),
            fenced: AtomicBool::new(false),
            reject_preflight: AtomicBool::new(false),
            fail_start: AtomicBool::new(false),
            revoke_in_claim: AtomicBool::new(false),
            revoke_in_start: AtomicBool::new(false),
            existing_claim: AtomicBool::new(false),
            stall_in_start: AtomicBool::new(false),
        })
    }
}
struct Resolver(Arc<State>);
impl DependencyResolver for Resolver {
    fn authorize<'a>(
        &'a self,
        _: &'a ContextDependency,
        _: &'a floe_access::DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { Err(AgentFailure::PolicyDenied) })
    }
    fn authorize_memories<'a>(
        &'a self,
        _: PersonId,
        refs: &'a [MemoryRevisionReference],
        _: &'a floe_access::DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            assert_eq!(refs.len(), 1);
            self.0.reads.fetch_add(1, Ordering::SeqCst);
            if self.0.stall.load(Ordering::SeqCst) {
                self.0.stalled.notify_one();
                std::future::pending::<()>().await;
            }
            if self.0.live.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(AgentFailure::StaleContext)
            }
        })
    }
}
struct NoGateway;
impl GatewayAdmission for NoGateway {
    fn admit<'a>(
        &'a self,
        _: &'a floe_access::VerifiedGatewayBinding,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_access::VerifiedGatewayBinding, AgentFailure>> {
        Box::pin(async { panic!("device fixture cannot consult a Gateway") })
    }
}
struct Provider(Arc<State>);
struct Transport(Arc<State>);
struct Session(Arc<State>);
impl LiveSessionControl for Session {
    fn fence(&self) {
        self.0.fenced.store(true, Ordering::SeqCst);
        let gate = self.0.fence_gate.lock().unwrap().take();
        if let Some(gate) = gate {
            gate.entered.send(()).unwrap();
            gate.release.recv_timeout(Duration::from_secs(2)).unwrap();
        }
    }
}
impl PreparedLiveTransport for Transport {
    type Session = Session;
    fn validate_start(&self, request: &CanonicalLiveStart) -> Result<(), AgentFailure> {
        assert!(request.deadline > tokio::time::Instant::now());
        if self.0.reject_preflight.load(Ordering::SeqCst) {
            Err(AgentFailure::InvalidInput)
        } else {
            Ok(())
        }
    }
    fn start(
        self,
        request: CanonicalLiveStart,
        target: AdmittedDispatchTarget,
    ) -> BoxFuture<'static, Result<Session, AgentFailure>> {
        Box::pin(async move {
            assert!(target.matches(&request.plan.binding_digest, ProcessingBoundary::Device));
            assert_eq!(self.0.claims.load(Ordering::SeqCst), 1);
            assert!(self.0.reads.load(Ordering::SeqCst) >= 2);
            self.0.starts.fetch_add(1, Ordering::SeqCst);
            if self.0.fail_start.load(Ordering::SeqCst) {
                return Err(AgentFailure::ServerModelUnavailable);
            }
            if self.0.revoke_in_start.load(Ordering::SeqCst) {
                self.0.live.store(false, Ordering::SeqCst);
            }
            if self.0.stall_in_start.load(Ordering::SeqCst) {
                self.0.stall.store(true, Ordering::SeqCst);
            }
            Ok(Session(self.0))
        })
    }
}
impl LiveModelProvider for Provider {
    type Transport = Transport;
    fn observe_primary_live<'a>(
        &'a self,
        _: &'a LiveModelPlanRequest,
        _: &'a ExecutionScope,
    ) -> BoxFuture<
        'a,
        Result<PrimaryObservation<PreparedLiveModelProfile<Transport>>, ModelObservationError>,
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
        Result<LocalObservation<PreparedLiveModelProfile<Transport>>, ModelObservationError>,
    > {
        Box::pin(async move {
            Ok(LocalObservation::Available(PreparedLiveModelProfile {
                capability: ObservedLiveModelCapability {
                    purpose: LIVE_PRESENTATION_PURPOSE.into(),
                    consumer: LIVE_PRESENTATION_CONSUMER.into(),
                    capabilities: LiveModelCapabilities::conversational(),
                    limits: LiveTransportLimits {
                        initial_context_bytes: 16384,
                        append_bytes: 65536,
                        local_session_duration_ms: 60000,
                    },
                    boundary: ProcessingBoundary::Device,
                    binding_digest: ModelBindingDigest([1; 32]),
                    selection_commitment: ModelSelectionCommitment([2; 32]),
                },
                target: ModelDispatchTarget::Device,
                transport: Transport(self.0.clone()),
            }))
        })
    }
}
struct Custody(Arc<State>);
impl LiveDispatchCustody for Custody {
    fn lifetime_cancellation(&self, actor: &OwnerActor) -> Result<Cancellation, AgentFailure> {
        actor.validate()?;
        Ok(self.0.owner_stop.clone())
    }

    fn claim<'a>(
        &'a self,
        intent: &'a LiveDispatchIntent,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            intent.validate()?;
            self.0.claims.fetch_add(1, Ordering::SeqCst);
            if self.0.revoke_in_claim.load(Ordering::SeqCst) {
                self.0.live.store(false, Ordering::SeqCst);
            }
            Ok(!self.0.existing_claim.load(Ordering::SeqCst))
        })
    }
}
type Service = InferenceService<Provider, Resolver, NoGateway>;
fn fixture_service(state: &Arc<State>) -> Service {
    InferenceService::new(Provider(state.clone()), Resolver(state.clone()), NoGateway)
}
fn fixture_scope() -> ExecutionScope {
    ExecutionScope::root(
        Cancellation::new(),
        tokio::time::Instant::now() + Duration::from_secs(10),
        BudgetLedger::new(BudgetConfig::new(100, 100), ModelUsage::default()).root_lease(),
        TraceContext::new(Uuid::new_v4()),
    )
}
async fn fixture_prepared(
    service: &Service,
    scope: &ExecutionScope,
) -> (PreparedLiveSelection<Transport>, LiveStartRequest) {
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "fixture".into(),
        runtime_epoch: 1,
    };
    let prepared = service
        .prepare_live(
            LiveModelPlanRequest {
                actor: actor.clone(),
                required_capabilities: LiveModelCapabilities::conversational(),
            },
            scope,
        )
        .await
        .unwrap();
    let plan = prepared.plan();
    let voice = Uuid::new_v4();
    let surface = Uuid::new_v4();
    let projection = LiveContextProjection {
        person_id: actor.person_id,
        device_id: actor.device_id,
        runtime_epoch: actor.runtime_epoch,
        voice_session_id: voice,
        plan_id: plan.operation_id,
        binding_digest: plan.binding_digest,
        projection_id: Uuid::new_v4(),
        projection_revision: 1,
        source: LiveProjectionSource::SurfacePrefix {
            through: SurfaceAnchor::empty(surface).unwrap(),
        },
        coverage: DependencyCoverage::Independent,
        memories: vec![MemoryRevisionReference {
            target_id: Uuid::new_v4(),
            revision: 1,
        }],
        input_data_classes: vec![DataClass::Personal],
        payload: LiveContextPayload::new("Synthetic context".into()).unwrap(),
    };
    (
        prepared,
        LiveStartRequest {
            voice_session_id: voice,
            surface_session_id: surface,
            media_lease_id: Uuid::new_v4(),
            projection,
            reservation: ModelReservationCeiling {
                tokens: 40,
                cost_micros: 60,
            },
        },
    )
}
async fn wait_fenced(state: &State) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !state.fenced.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn start_consumes_custody_and_access_then_drop_retains_unknown_usage() {
    let state = State::new();
    let service = fixture_service(&state);
    let scope = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &scope).await;
    let id = prepared.plan().operation_id;
    let call = service
        .start_live(prepared, request, &Custody(state.clone()), &scope)
        .await
        .unwrap();
    assert_eq!(state.starts.load(Ordering::SeqCst), 1);
    assert!(!state.fenced.load(Ordering::SeqCst));
    call.revalidate().await.unwrap();
    drop(call);
    let receipt = scope.budget().model_attempt_receipt(id).unwrap();
    assert!(
        receipt.dispatched && receipt.accounting.unknown_tokens && receipt.accounting.unknown_cost
    );
    assert_eq!(
        (receipt.charged_tokens, receipt.charged_cost_micros),
        (40, 60)
    );
    assert!(state.fenced.load(Ordering::SeqCst));
    assert!(!scope.cancellation().is_cancelled());
}
#[tokio::test]
async fn preflight_and_existing_claim_never_dispatch_or_spend_a_model_attempt() {
    for preflight in [true, false] {
        let state = State::new();
        state.reject_preflight.store(preflight, Ordering::SeqCst);
        state.existing_claim.store(!preflight, Ordering::SeqCst);
        let service = fixture_service(&state);
        let scope = fixture_scope();
        let (prepared, request) = fixture_prepared(&service, &scope).await;
        let id = prepared.plan().operation_id;
        assert!(
            service
                .start_live(prepared, request, &Custody(state.clone()), &scope)
                .await
                .is_err()
        );
        assert_eq!(state.starts.load(Ordering::SeqCst), 0);
        assert_eq!(state.claims.load(Ordering::SeqCst), usize::from(!preflight));
        let receipt = scope.budget().model_attempt_receipt(id).unwrap();
        assert!(!receipt.dispatched);
        assert_eq!(
            (receipt.charged_tokens, receipt.charged_cost_micros),
            (0, 0)
        );
    }
}
#[tokio::test]
async fn memory_forget_during_custody_prevents_provider_handoff() {
    let state = State::new();
    state.revoke_in_claim.store(true, Ordering::SeqCst);
    let service = fixture_service(&state);
    let scope = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &scope).await;
    assert!(matches!(
        service
            .start_live(prepared, request, &Custody(state.clone()), &scope)
            .await,
        Err(AgentFailure::StaleContext)
    ));
    assert_eq!(state.starts.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn known_returned_session_is_fenced_if_post_dispatch_permission_changes() {
    let state = State::new();
    state.revoke_in_start.store(true, Ordering::SeqCst);
    let service = fixture_service(&state);
    let scope = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &scope).await;
    let id = prepared.plan().operation_id;
    assert!(matches!(
        service
            .start_live(prepared, request, &Custody(state.clone()), &scope)
            .await,
        Err(AgentFailure::StaleContext)
    ));
    assert!(state.fenced.load(Ordering::SeqCst));
    assert!(
        scope
            .budget()
            .model_attempt_receipt(id)
            .unwrap()
            .accounting
            .unknown_cost
    );
}
#[tokio::test]
async fn independent_monitor_fences_revocation_and_stalled_source_without_reader_polling() {
    for stall in [false, true] {
        let state = State::new();
        let service = fixture_service(&state);
        let scope = fixture_scope();
        let (prepared, request) = fixture_prepared(&service, &scope).await;
        let call = service
            .start_live(prepared, request, &Custody(state.clone()), &scope)
            .await
            .unwrap();
        if stall {
            state.stall.store(true, Ordering::SeqCst);
        } else {
            state.live.store(false, Ordering::SeqCst);
        }
        wait_fenced(&state).await;
        assert!(call.failure().is_some());
        let receipt = call.finish(LiveUsageObservation::default()).unwrap();
        assert!(receipt.accounting.unknown_tokens && receipt.accounting.unknown_cost);
    }
}
#[tokio::test]
async fn preparation_cancellation_remains_live_after_handoff_to_another_scope() {
    let state = State::new();
    let service = fixture_service(&state);
    let preparation = fixture_scope();
    let owner = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &preparation).await;
    let call = service
        .start_live(prepared, request, &Custody(state.clone()), &owner)
        .await
        .unwrap();
    preparation
        .cancellation()
        .cancel_with_reason(floe_execution::CancelReason::User);
    wait_fenced(&state).await;
    assert!(call.revalidate().await.is_err());
    assert!(!owner.cancellation().is_cancelled());
}
#[tokio::test]
async fn settlement_keeps_real_overrun_receipt_and_never_refunds_unknown_transport_failure() {
    let state = State::new();
    let service = fixture_service(&state);
    let scope = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &scope).await;
    let id = prepared.plan().operation_id;
    let call = service
        .start_live(prepared, request, &Custody(state.clone()), &scope)
        .await
        .unwrap();
    assert_eq!(
        call.finish(LiveUsageObservation {
            tokens: Some(41),
            cost_micros: Some(61)
        }),
        Err(AgentFailure::BudgetExceeded)
    );
    let receipt = scope.budget().model_attempt_receipt(id).unwrap();
    assert_eq!(receipt.accounting.observed_tokens, Some(41));
    assert_eq!(receipt.charged_cost_micros, 61);
    let state = State::new();
    state.fail_start.store(true, Ordering::SeqCst);
    let service = fixture_service(&state);
    let scope = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &scope).await;
    let id = prepared.plan().operation_id;
    assert!(
        service
            .start_live(prepared, request, &Custody(state.clone()), &scope)
            .await
            .is_err()
    );
    assert!(
        scope
            .budget()
            .model_attempt_receipt(id)
            .unwrap()
            .accounting
            .unknown_cost
    );
    assert_eq!(state.starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancellation_during_post_start_checks_fences_the_known_returned_session() {
    let state = State::new();
    state.stall_in_start.store(true, Ordering::SeqCst);
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let id = prepared.plan().operation_id;
    let owner = root.child_scope(
        tokio::time::Instant::now() + Duration::from_millis(50),
        100,
        100,
        None,
    );
    assert!(matches!(
        service
            .start_live(prepared, request, &Custody(state.clone()), &owner)
            .await,
        Err(AgentFailure::DeadlineExceeded)
    ));
    assert_eq!(state.starts.load(Ordering::SeqCst), 1);
    assert!(
        state.fenced.load(Ordering::SeqCst),
        "dropping post-start validation cannot leak a returned session"
    );
    assert!(
        root.budget()
            .model_attempt_receipt(id)
            .unwrap()
            .accounting
            .unknown_cost
    );
}

#[tokio::test]
async fn voice_owner_retirement_fences_even_an_independent_selected_call() {
    let state = State::new();
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    state
        .owner_stop
        .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
    wait_fenced(&state).await;
    assert!(call.revalidate().await.is_err());
    assert!(!root.cancellation().is_cancelled());
}

#[tokio::test]
async fn owner_retirement_does_not_wait_for_an_inflight_source_check() {
    let state = State::new();
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    state.stall.store(true, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(2), state.stalled.notified())
        .await
        .unwrap();
    state
        .owner_stop
        .cancel_with_reason(floe_execution::CancelReason::OwnerDropped);
    // This observation is shorter than the 500 ms source-read bound: retirement
    // must interrupt the check itself, not wait for that I/O/timeout to finish.
    tokio::time::timeout(Duration::from_millis(100), wait_fenced(&state))
        .await
        .expect("owner retirement waited for a stalled source authorization read");
    assert!(call.failure().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_failure_is_recorded_before_local_fencing_and_derivative_cancellation() {
    let state = State::new();
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let call = Arc::new(
        service
            .start_live(prepared, request, &Custody(state.clone()), &root)
            .await
            .unwrap(),
    );
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    *state.fence_gate.lock().unwrap() = Some(FenceGate {
        entered: entered_tx,
        release: release_rx,
    });
    state.live.store(false, Ordering::SeqCst);
    let observer = call.clone();
    let check = tokio::spawn(async move { observer.revalidate().await });
    tokio::task::spawn_blocking(move || entered_rx.recv_timeout(Duration::from_secs(2)))
        .await
        .unwrap()
        .unwrap();
    let _observed = tokio::time::timeout(Duration::from_secs(1), async {
        while call.failure().is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    let cause = call.failure();
    release_tx.send(()).unwrap();
    assert_eq!(check.await.unwrap(), Err(AgentFailure::StaleContext));
    assert_eq!(
        cause,
        Some(AgentFailure::StaleContext),
        "cancellation caused by revocation must not hide the revocation"
    );
}
