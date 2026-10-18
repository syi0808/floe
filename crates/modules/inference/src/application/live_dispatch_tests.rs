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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use uuid::Uuid;

struct FenceGate {
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
}
struct State {
    append_claims: Mutex<std::collections::HashSet<Uuid>>,
    appends: AtomicUsize,
    append_fail: AtomicBool,
    append_stall: AtomicBool,
    append_revoke: AtomicBool,
    revoked_memory: Mutex<Option<Uuid>>,
    events: Mutex<std::collections::VecDeque<LiveDelivery>>,
    observations: AtomicUsize,
    acknowledged: AtomicU64,
    lose_ack_response: AtomicBool,
    stall_ack_response: AtomicBool,
    revoke_after_observe: AtomicBool,
    stall_after_observe: AtomicBool,
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
            append_claims: Mutex::new(Default::default()),
            appends: AtomicUsize::new(0),
            append_fail: AtomicBool::new(false),
            append_stall: AtomicBool::new(false),
            append_revoke: AtomicBool::new(false),
            revoked_memory: Mutex::new(None),
            events: Mutex::new(Default::default()),
            observations: AtomicUsize::new(0),
            acknowledged: AtomicU64::new(0),
            lose_ack_response: AtomicBool::new(false),
            stall_ack_response: AtomicBool::new(false),
            revoke_after_observe: AtomicBool::new(false),
            stall_after_observe: AtomicBool::new(false),
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
            assert!(!refs.is_empty());
            self.0.reads.fetch_add(1, Ordering::SeqCst);
            if self.0.stall.load(Ordering::SeqCst) {
                self.0.stalled.notify_one();
                std::future::pending::<()>().await;
            }
            if self.0.live.load(Ordering::SeqCst)
                && !refs
                    .iter()
                    .any(|r| Some(r.target_id) == *self.0.revoked_memory.lock().unwrap())
            {
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
impl LiveEventTransport for Session {
    fn validate_append(
        &self,
        payload: &floe_model_contract::LiveAppendPayload,
    ) -> Result<(), AgentFailure> {
        payload.validate().map_err(|_| AgentFailure::InvalidInput)
    }
    fn append<'a>(
        &'a self,
        payload: &'a floe_model_contract::LiveAppendPayload,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            assert!(
                self.0
                    .append_claims
                    .lock()
                    .unwrap()
                    .contains(&payload.operation_id)
            );
            self.0.appends.fetch_add(1, Ordering::SeqCst);
            if self.0.append_stall.load(Ordering::SeqCst) {
                std::future::pending::<()>().await;
            }
            if self.0.append_fail.load(Ordering::SeqCst) {
                Err(AgentFailure::ServerModelUnavailable)
            } else {
                Ok(())
            }
        })
    }

    fn observe<'a>(
        &'a self,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<LiveDelivery, AgentFailure>> {
        Box::pin(async move {
            self.0.observations.fetch_add(1, Ordering::SeqCst);
            let event = self.0.events.lock().unwrap().front().cloned();
            if self.0.revoke_after_observe.load(Ordering::SeqCst) {
                self.0.live.store(false, Ordering::SeqCst);
            }
            if self.0.stall_after_observe.load(Ordering::SeqCst) {
                self.0.stall.store(true, Ordering::SeqCst);
            }
            match event {
                Some(event) => Ok(event),
                None => std::future::pending().await,
            }
        })
    }
    fn acknowledge<'a>(
        &'a self,
        sequence: u64,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            if sequence <= self.0.acknowledged.load(Ordering::SeqCst) {
                return Ok(());
            }
            {
                let mut events = self.0.events.lock().unwrap();
                if events.front().map(|v| v.sequence) != Some(sequence) {
                    return Err(AgentFailure::Conflict);
                }
                events.pop_front();
            }
            self.0.acknowledged.store(sequence, Ordering::SeqCst);
            if self.0.stall_ack_response.load(Ordering::SeqCst) {
                return scope.run(std::future::pending()).await;
            }
            if self.0.lose_ack_response.swap(false, Ordering::SeqCst) {
                return Err(AgentFailure::ServerModelUnavailable);
            }
            Ok(())
        })
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

fn delivery(sequence: u64) -> LiveDelivery {
    LiveDelivery {
        sequence,
        event: floe_model_contract::LiveProviderEvent::PersonUtteranceFinal {
            utterance_id: format!("utterance-{sequence}"),
            text: "synthetic input".into(),
        },
    }
}
#[tokio::test]
async fn retained_live_delivery_and_lost_ack_never_consume_later_input() {
    let state = State::new();
    state
        .events
        .lock()
        .unwrap()
        .extend([delivery(1), delivery(2)]);
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    assert_eq!(call.receive().await.unwrap(), delivery(1));
    assert_eq!(call.receive().await.unwrap(), delivery(1));
    assert_eq!(state.observations.load(Ordering::SeqCst), 1);
    assert_eq!(call.acknowledge(2).await, Err(AgentFailure::Conflict));
    state.lose_ack_response.store(true, Ordering::SeqCst);
    assert_eq!(
        call.acknowledge(1).await,
        Err(AgentFailure::ServerModelUnavailable)
    );
    assert_eq!(call.receive().await.unwrap(), delivery(1));
    call.acknowledge(1).await.unwrap();
    call.acknowledge(1).await.unwrap();
    assert_eq!(call.receive().await.unwrap(), delivery(2));
    assert_eq!(state.observations.load(Ordering::SeqCst), 2);
}
#[tokio::test]
async fn observer_drop_during_post_delivery_source_check_retains_pending_event() {
    let state = State::new();
    state.events.lock().unwrap().push_back(delivery(1));
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    state.stall_after_observe.store(true, Ordering::SeqCst);
    let mut observation = Box::pin(call.receive());
    let polled = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(observation.as_mut(), cx))
    })
    .await;
    assert!(polled.is_pending());
    assert_eq!(state.observations.load(Ordering::SeqCst), 1);
    drop(observation);
    state.stall.store(false, Ordering::SeqCst);
    state.stall_after_observe.store(false, Ordering::SeqCst);
    assert_eq!(call.receive().await.unwrap(), delivery(1));
    assert_eq!(state.observations.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn terminal_usage_survives_denied_event_release_after_source_revocation() {
    let state = State::new();
    state.events.lock().unwrap().push_back(LiveDelivery {
        sequence: 1,
        event: floe_model_contract::LiveProviderEvent::Ended {
            evidence: floe_model_contract::LiveEndEvidence::ProviderConfirmed,
            usage: LiveUsageObservation {
                tokens: Some(9),
                cost_micros: Some(4),
            },
        },
    });
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    state.revoke_after_observe.store(true, Ordering::SeqCst);
    assert_eq!(call.receive().await, Err(AgentFailure::StaleContext));
    assert!(state.fenced.load(Ordering::SeqCst));
    let receipt = call.finish(LiveUsageObservation::default()).unwrap();
    assert_eq!(receipt.accounting.observed_tokens, Some(9));
    assert_eq!(receipt.charged_cost_micros, 4);
    assert!(!receipt.accounting.unknown_tokens);
    assert_eq!(state.acknowledged.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn invalid_live_delivery_sequence_is_fenced_before_release() {
    for sequence in [0, 2, u64::MAX] {
        let state = State::new();
        state.events.lock().unwrap().push_back(delivery(sequence));
        let service = fixture_service(&state);
        let root = fixture_scope();
        let (prepared, request) = fixture_prepared(&service, &root).await;
        let mut call = service
            .start_live(prepared, request, &Custody(state.clone()), &root)
            .await
            .unwrap();
        assert_eq!(call.receive().await, Err(AgentFailure::InvalidInput));
        assert!(state.fenced.load(Ordering::SeqCst));
    }
}

#[tokio::test]
async fn dropping_a_guard_after_terminal_delivery_preserves_known_ledger_usage() {
    let state = State::new();
    state.events.lock().unwrap().push_back(LiveDelivery {
        sequence: 1,
        event: floe_model_contract::LiveProviderEvent::Ended {
            evidence: floe_model_contract::LiveEndEvidence::Disconnected,
            usage: LiveUsageObservation {
                tokens: Some(11),
                cost_micros: None,
            },
        },
    });
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let id = prepared.plan().operation_id;
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    call.receive().await.unwrap();
    drop(call);
    let receipt = root.budget().model_attempt_receipt(id).unwrap();
    assert_eq!(receipt.accounting.observed_tokens, Some(11));
    assert!(!receipt.accounting.unknown_tokens);
    assert!(receipt.accounting.unknown_cost);
    assert_eq!(receipt.charged_tokens, 11);
}

#[tokio::test]
async fn observer_drop_after_remote_ack_keeps_local_head_and_owner_alive() {
    let state = State::new();
    state
        .events
        .lock()
        .unwrap()
        .extend([delivery(1), delivery(2)]);
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    call.receive().await.unwrap();
    state.stall_ack_response.store(true, Ordering::SeqCst);
    let mut ack = Box::pin(call.acknowledge(1));
    let polled = std::future::poll_fn(|cx| {
        std::task::Poll::Ready(std::future::Future::poll(ack.as_mut(), cx))
    })
    .await;
    assert!(polled.is_pending());
    assert_eq!(state.acknowledged.load(Ordering::SeqCst), 1);
    drop(ack);
    assert!(!root.cancellation().is_cancelled());
    assert_eq!(call.receive().await.unwrap(), delivery(1));
    state.stall_ack_response.store(false, Ordering::SeqCst);
    call.acknowledge(1).await.unwrap();
    assert_eq!(call.receive().await.unwrap(), delivery(2));
}

#[tokio::test]
async fn settled_live_call_can_ack_custody_but_cannot_release_more_events() {
    let state = State::new();
    state
        .events
        .lock()
        .unwrap()
        .extend([delivery(1), delivery(2)]);
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    call.receive().await.unwrap();
    let settled = call
        .settle_usage(LiveUsageObservation {
            tokens: Some(41),
            cost_micros: Some(61),
        })
        .unwrap();
    assert_eq!(settled.failure, Some(AgentFailure::BudgetExceeded));
    assert_eq!(settled.receipt.accounting.observed_tokens, Some(41));
    assert!(state.fenced.load(Ordering::SeqCst));
    assert_eq!(call.receive().await, Err(AgentFailure::Conflict));
    call.acknowledge(1).await.unwrap();
    assert_eq!(state.acknowledged.load(Ordering::SeqCst), 1);
    assert_eq!(state.observations.load(Ordering::SeqCst), 1);
    assert!(matches!(
        call.settle_usage(LiveUsageObservation::default()),
        Err(AgentFailure::Conflict)
    ));
}

#[tokio::test]
async fn waiting_delivery_returns_the_source_failure_not_derivative_cancellation() {
    let state = State::new();
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    let read = tokio::spawn(async move {
        let result = call.receive().await;
        (call, result)
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.observations.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    state.live.store(false, Ordering::SeqCst);
    let (call, result) = tokio::time::timeout(Duration::from_secs(2), read)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(call.failure(), Some(AgentFailure::StaleContext));
    assert_eq!(result, Err(AgentFailure::StaleContext));
    assert!(!root.cancellation().is_cancelled());
}

impl crate::LiveAppendCustody for Custody {
    fn claim_append<'a>(
        &'a self,
        intent: &'a crate::LiveAppendIntent,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<bool, AgentFailure>> {
        Box::pin(async move {
            if self.0.append_revoke.load(Ordering::SeqCst) {
                self.0.live.store(false, Ordering::SeqCst);
            }
            Ok(self
                .0
                .append_claims
                .lock()
                .unwrap()
                .insert(intent.payload.operation_id))
        })
    }
}
fn append_request(
    mut projection: LiveContextProjection,
    operation_id: Uuid,
) -> crate::LiveAppendRequest {
    let through = projection.source.through();
    projection.source = serde_json::from_value(serde_json::json!({
        "kind":"surface_result", "through":{"session_id":through.session_id,"sequence":1,"digest":vec![7u8;32]}, "surface_message_id":Uuid::new_v4()
    })).unwrap();
    projection.projection_id = operation_id;
    projection.payload = LiveContextPayload::new("Manager result".into()).unwrap();
    crate::LiveAppendRequest {
        operation_id,
        kind: floe_model_contract::LiveAppendKind::PresentableResult,
        delegation_id: Some("delegation-1".into()),
        projection,
    }
}
#[tokio::test]
async fn append_claim_is_one_shot_and_write_failure_remains_uncertain() {
    for fail in [false, true] {
        let state = State::new();
        let service = fixture_service(&state);
        let root = fixture_scope();
        let (prepared, request) = fixture_prepared(&service, &root).await;
        let projection = request.projection.clone();
        let mut call = service
            .start_live(prepared, request, &Custody(state.clone()), &root)
            .await
            .unwrap();
        state.append_fail.store(fail, Ordering::SeqCst);
        let id = Uuid::new_v4();
        let result = call
            .append(
                append_request(projection.clone(), id),
                &Custody(state.clone()),
            )
            .await
            .unwrap();
        assert_eq!(
            result,
            if fail {
                crate::LiveAppendWrite::Uncertain(AgentFailure::ServerModelUnavailable)
            } else {
                crate::LiveAppendWrite::Written
            }
        );
        assert!(
            call.append(append_request(projection, id), &Custody(state.clone()))
                .await
                .is_err()
        );
        assert_eq!(state.appends.load(Ordering::SeqCst), 1);
        assert_eq!(state.starts.load(Ordering::SeqCst), 1);
        assert!(!root.cancellation().is_cancelled());
    }
}
#[tokio::test]
async fn append_rechecks_source_after_custody_before_any_write() {
    let state = State::new();
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let projection = request.projection.clone();
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    state.append_revoke.store(true, Ordering::SeqCst);
    assert_eq!(
        call.append(
            append_request(projection, Uuid::new_v4()),
            &Custody(state.clone())
        )
        .await,
        Err(AgentFailure::StaleContext)
    );
    assert_eq!(state.appends.load(Ordering::SeqCst), 0);
    assert_eq!(state.append_claims.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn append_cumulative_memory_survives_later_independent_result_and_unknown_initial_history() {
    let state = State::new();
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, mut request) = fixture_prepared(&service, &root).await;
    request.projection.coverage = DependencyCoverage::Unknown;
    // Keep the initial memory so the start fixture checks real Access reads.
    let mut projection = request.projection.clone();
    projection.coverage = DependencyCoverage::Independent;
    let added = Uuid::new_v4();
    projection.memories = vec![MemoryRevisionReference {
        target_id: added,
        revision: 1,
    }];
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    call.append(
        append_request(projection.clone(), Uuid::new_v4()),
        &Custody(state.clone()),
    )
    .await
    .unwrap();
    projection.memories.clear();
    call.append(
        append_request(projection, Uuid::new_v4()),
        &Custody(state.clone()),
    )
    .await
    .unwrap();
    *state.revoked_memory.lock().unwrap() = Some(added);
    tokio::time::timeout(Duration::from_secs(2), async {
        while call.failure().is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(call.revalidate().await, Err(AgentFailure::StaleContext));
    assert_eq!(state.appends.load(Ordering::SeqCst), 2);
    assert!(state.fenced.load(Ordering::SeqCst));
}
#[tokio::test]
async fn append_rejects_unknown_revision_replacement_and_capacity_before_claim() {
    let state = State::new();
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let projection = request.projection.clone();
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    let mut unknown = append_request(projection.clone(), Uuid::new_v4());
    unknown.projection.coverage = DependencyCoverage::Unknown;
    assert_eq!(
        call.append(unknown, &Custody(state.clone())).await,
        Err(AgentFailure::PolicyDenied)
    );
    let mut changed = append_request(projection.clone(), Uuid::new_v4());
    changed.projection.memories[0].revision += 1;
    assert_eq!(
        call.append(changed, &Custody(state.clone())).await,
        Err(AgentFailure::StaleContext)
    );
    let mut large = append_request(projection, Uuid::new_v4());
    large.projection.payload =
        LiveContextPayload::new("x".repeat(call.plan().limits.append_bytes as usize + 1)).unwrap();
    assert_eq!(
        call.append(large, &Custody(state.clone())).await,
        Err(AgentFailure::ModelInputCapacityExceeded)
    );
    assert_eq!(state.appends.load(Ordering::SeqCst), 0);
    assert!(state.append_claims.lock().unwrap().is_empty());
    assert!(!state.fenced.load(Ordering::SeqCst));
}

#[tokio::test]
async fn dropped_append_write_fences_the_call_and_never_replays() {
    let state = State::new();
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let projection = request.projection.clone();
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    state.append_stall.store(true, Ordering::SeqCst);
    let id = Uuid::new_v4();
    let custody = Custody(state.clone());
    let mut operation = Box::pin(call.append(append_request(projection.clone(), id), &custody));
    std::future::poll_fn(|cx| {
        assert!(operation.as_mut().poll(cx).is_pending());
        if state.appends.load(Ordering::SeqCst) == 1 {
            std::task::Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await;
    drop(operation);
    assert_eq!(call.failure(), Some(AgentFailure::Interrupted));
    assert!(state.fenced.load(Ordering::SeqCst));
    assert_eq!(
        call.append(append_request(projection, id), &custody).await,
        Err(AgentFailure::Interrupted)
    );
    assert_eq!(state.appends.load(Ordering::SeqCst), 1);
    assert!(!root.cancellation().is_cancelled());
}

#[tokio::test]
async fn append_checks_cumulative_memory_capacity_and_call_identity_before_custody() {
    let state = State::new();
    let service = fixture_service(&state);
    let root = fixture_scope();
    let (prepared, request) = fixture_prepared(&service, &root).await;
    let projection = request.projection.clone();
    let mut call = service
        .start_live(prepared, request, &Custody(state.clone()), &root)
        .await
        .unwrap();
    let mut foreign = append_request(projection.clone(), Uuid::new_v4());
    foreign.projection.voice_session_id = Uuid::new_v4();
    assert_eq!(
        call.append(foreign, &Custody(state.clone())).await,
        Err(AgentFailure::PolicyDenied)
    );
    let mut oversized = append_request(projection, Uuid::new_v4());
    oversized.projection.memories = (0..floe_context_contract::MAX_CONTEXT_MEMORIES)
        .map(|_| MemoryRevisionReference {
            target_id: Uuid::new_v4(),
            revision: 1,
        })
        .collect();
    // Each incoming projection is valid alone; the already-sent initial memory
    // makes the cumulative obligation set exceed its bounded authority contract.
    oversized.projection.validate().unwrap();
    assert_eq!(
        call.append(oversized, &Custody(state.clone())).await,
        Err(AgentFailure::ModelInputCapacityExceeded)
    );
    assert!(state.append_claims.lock().unwrap().is_empty());
    assert_eq!(state.appends.load(Ordering::SeqCst), 0);
}
