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
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
#[derive(Default)]
struct Probe {
    starts: AtomicUsize,
    fenced: AtomicBool,
    events: std::sync::Mutex<std::collections::VecDeque<floe_inference::LiveDelivery>>,
    changed: tokio::sync::Notify,
    acknowledged: AtomicU64,
    lose_ack_at: AtomicU64,
    fail_ack_at: AtomicU64,
    ack_calls: AtomicUsize,
}
struct LiveProvider(Arc<Probe>);
struct LiveTransport(Arc<Probe>);
struct LiveSession(Arc<Probe>);
impl LiveSessionControl for LiveSession {
    fn fence(&self) {
        self.0.fenced.store(true, Ordering::SeqCst);
    }
}
impl floe_inference::LiveEventTransport for LiveSession {
    fn observe<'a>(
        &'a self,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_inference::LiveDelivery, AgentFailure>> {
        Box::pin(async move {
            loop {
                let changed = self.0.changed.notified();
                if let Some(event) = self.0.events.lock().unwrap().front().cloned() {
                    return Ok(event);
                }
                changed.await;
            }
        })
    }
    fn acknowledge<'a>(
        &'a self,
        sequence: u64,
        _: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async move {
            self.0.ack_calls.fetch_add(1, Ordering::SeqCst);
            if self.0.fail_ack_at.load(Ordering::SeqCst) == sequence {
                return Err(AgentFailure::ServerModelUnavailable);
            }
            if sequence <= self.0.acknowledged.load(Ordering::SeqCst) {
                return Ok(());
            }
            let mut events = self.0.events.lock().unwrap();
            if events.front().map(|event| event.sequence) != Some(sequence) {
                return Err(AgentFailure::Conflict);
            }
            events.pop_front();
            self.0.acknowledged.store(sequence, Ordering::SeqCst);
            if self.0.lose_ack_at.load(Ordering::SeqCst) == sequence {
                self.0.lose_ack_at.store(0, Ordering::SeqCst);
                return Err(AgentFailure::ServerModelUnavailable);
            }
            Ok(())
        })
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
                ..Default::default()
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

async fn native_fixture_mutation(
    voice: &floe_voice::VoiceService,
    actor: &floe_kernel::OwnerActor,
    record: &floe_voice::VoiceCallRecord,
    kind: VoiceMutationKind,
    scope: &ExecutionScope,
) -> floe_voice::VoiceCallRecord {
    let request = VoiceMutation {
        voice_session_id: record.intent.binding.voice_session_id,
        command_id: Uuid::new_v4(),
        expected_revision: record.revision,
        kind,
    };
    support::exact_recovery::observe_exact_request(
        request,
        |exact| voice.mutate(actor, exact, scope),
        |failure| {
            matches!(
                failure,
                floe_kernel::CommandFailure::NotAdmitted(AgentFailure::StorageBusy)
                    | floe_kernel::CommandFailure::Indeterminate(AgentFailure::StorageBusy)
                    | floe_kernel::CommandFailure::Admitted(AgentFailure::StorageBusy)
            )
        },
    )
    .await
    .unwrap()
    .record
}
fn drive_retained_stream(lost_ack: bool, over_budget: bool, unavailable_ack: bool) {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let surface = start_session(&host);
    let work = support::with_ready(&host, |services, caller, owners| {
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
                ..Default::default()
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

            use floe_model_contract::{LiveEndEvidence, LiveProviderEvent, LiveSpeaker};
            let binding = record.intent.binding.clone();
            probe
                .events
                .lock()
                .unwrap()
                .push_back(floe_inference::LiveDelivery {
                    sequence: 1,
                    event: LiveProviderEvent::Ready {},
                });
            if lost_ack {
                probe.lose_ack_at.store(4, Ordering::SeqCst);
            }
            if unavailable_ack {
                probe.fail_ack_at.store(4, Ordering::SeqCst);
            }
            let native_voice = owners.voice.clone();
            let native_actor = actor.clone();
            let native_scope = scope.clone();
            let native_probe = probe.clone();
            let native = tokio::spawn(async move {
                let ready = loop {
                    assert!(tokio::time::Instant::now() < native_scope.deadline());
                    match native_voice
                        .read_call(&native_actor, voice, &native_scope)
                        .await
                    {
                        Ok(Some(record)) if record.session.remote() == RemoteState::Connected => {
                            break record;
                        }
                        Ok(_) | Err(AgentFailure::StorageBusy) => {
                            tokio::time::sleep(Duration::from_millis(5)).await
                        }
                        Err(error) => panic!("native fixture observed {error:?}"),
                    }
                };
                // Explicit synthetic user Unmute and exact native ACK; Ready
                // itself never unmutes. No microphone permission or SDK is used.
                let unmute = native_fixture_mutation(
                    &native_voice,
                    &native_actor,
                    &ready,
                    VoiceMutationKind::Unmute {},
                    &native_scope,
                )
                .await;
                let command = unmute.session.pending_media_command().unwrap().clone();
                native_fixture_mutation(
                    &native_voice,
                    &native_actor,
                    &unmute,
                    VoiceMutationKind::NativeReceipt {
                        command,
                        state: MediaState::Capturing,
                    },
                    &native_scope,
                )
                .await;
                let events = [
                    LiveProviderEvent::TranscriptDelta {
                        speaker: LiveSpeaker::Person,
                        utterance_id: None,
                        text: "not a final input".into(),
                    },
                    LiveProviderEvent::PersonUtteranceFinal {
                        utterance_id: "user-final".into(),
                        text: USER_TEXT.into(),
                    },
                    LiveProviderEvent::DelegationRequested {
                        delegation_id: "work".into(),
                        utterance_ref: Some("user-final".into()),
                        input_hint: None,
                    },
                    LiveProviderEvent::AssistantUtteranceFinal {
                        utterance_id: "presentation".into(),
                        text: "synthetic filler, not a Manager answer".into(),
                    },
                    LiveProviderEvent::Ended {
                        evidence: LiveEndEvidence::ProviderConfirmed,
                        usage: LiveUsageObservation {
                            tokens: Some(if over_budget { 41 } else { 9 }),
                            cost_micros: Some(if over_budget { 61 } else { 4 }),
                        },
                    },
                ];
                native_probe
                    .events
                    .lock()
                    .unwrap()
                    .extend(events.into_iter().enumerate().map(|(i, event)| {
                        floe_inference::LiveDelivery {
                            sequence: i as u64 + 2,
                            event,
                        }
                    }));
                native_probe.changed.notify_one();
            });
            let live_voice = owners.voice.clone();
            let live_scope = scope.clone();
            owners
                .voice
                .run_controller(binding, &scope, move |controller| {
                    Box::pin(async move {
                        let owned = ExecutionScope::root(
                            controller.cancellation().clone(),
                            live_scope.deadline(),
                            live_scope.budget().child(40, 60),
                            live_scope.trace_context(),
                        );
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
                                live_voice.as_ref(),
                                &owned,
                            )
                            .await?;
                        let result = controller.consume_live(call).await?;
                        if unavailable_ack {
                            assert_eq!(result.failure, Some(AgentFailure::ServerModelUnavailable));
                            assert!(result.usage.accounting.unknown_tokens);
                        } else if over_budget {
                            assert_eq!(result.failure, Some(AgentFailure::BudgetExceeded));
                            assert_eq!(result.usage.accounting.observed_tokens, Some(41));
                            assert_eq!(result.usage.charged_cost_micros, 61);
                        } else {
                            assert_eq!(result.failure, None);
                            assert_eq!(result.usage.accounting.observed_tokens, Some(9));
                        }
                        Ok(())
                    })
                })
                .await
                .unwrap();
            native.await.unwrap();
            assert!(probe.fenced.load(Ordering::SeqCst));
            if !unavailable_ack {
                assert_eq!(
                    probe.ack_calls.load(Ordering::SeqCst),
                    if lost_ack { 7 } else { 6 }
                );
            } else {
                assert!(probe.ack_calls.load(Ordering::SeqCst) > 4);
            }
            assert_eq!(probe.starts.load(Ordering::SeqCst), 1);
            assert_eq!(
                probe.acknowledged.load(Ordering::SeqCst),
                if unavailable_ack { 3 } else { 6 }
            );
            let retained = owners
                .voice
                .read_call(&actor, voice, &scope)
                .await
                .unwrap()
                .unwrap();
            assert!(retained.usage.is_some());
            if over_budget {
                assert_eq!(retained.usage.unwrap().accounting.observed_tokens, Some(41));
                assert_eq!(retained.usage.unwrap().charged_cost_micros, 61);
            }
            assert!(retained.session.stop_requested());
            assert!(!retained.session.local_lease_released());
            assert_eq!(
                retained.session.remote(),
                if unavailable_ack {
                    RemoteState::Unknown
                } else {
                    RemoteState::Closed
                }
            );
            let through = owners
                .conversation
                .surface_head(&actor, surface, &scope)
                .await
                .unwrap();
            let entries = owners
                .conversation
                .surface_entries(&actor, through, 0, 64, &scope)
                .await
                .unwrap();
            assert_eq!(
                entries
                    .iter()
                    .filter(|entry| matches!(
                        entry,
                        floe_conversation::SurfaceEntryReceipt::Input(_)
                    ))
                    .count(),
                1,
                "partial captions and live filler must not become additional user inputs"
            );
            owners
                .voice
                .read_event(&actor, voice, "delegation", "work", &scope)
                .await
                .unwrap()
                .unwrap()
                .work
                .unwrap()
        })
    });
    let run = support::wait_work_run(&host, work);
    assert_eq!(
        wait_terminal_run(&host, run.run_id).state,
        RunState::Completed
    );
    assert_eq!(assert_script_clean(&recorder).generated.len(), 1);
}
#[test]
fn registered_retained_live_stream_drives_canonical_manager_work_and_terminal_custody() {
    drive_retained_stream(false, false, false);
}
#[test]
fn lost_live_delivery_ack_preserves_one_manager_work_without_recreating_call() {
    drive_retained_stream(true, false, false);
}

#[test]
fn retained_live_stream_keeps_over_budget_terminal_receipt_in_real_voice_journal() {
    drive_retained_stream(false, true, false);
}

#[test]
fn unavailable_live_ack_ends_call_but_retains_canonical_work_and_unknown_usage() {
    drive_retained_stream(false, false, true);
}
