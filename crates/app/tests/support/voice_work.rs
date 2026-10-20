//! Synthetic live events/native acknowledgements, real encrypted Voice journal,
//! generation-owned Conversation queue, Manager runtime and result publication.
use super::*;
use floe_context_contract::{
    DataClass, DependencyCoverage, LiveContextProjection, LiveProjectionSource,
};
use floe_execution::Cancellation;
use floe_kernel::{CommandFailure, OwnerActor};
use floe_model_contract::{
    LiveContextPayload, LiveModelCapabilities, LiveModelPlan, LiveProviderEvent,
    LiveTransportLimits, ModelBindingDigest, ModelSelectionCommitment, ProcessingBoundary,
};
use floe_voice::{
    MediaState, VoiceBinding, VoiceCallRecord, VoiceEventOutcome, VoiceIngressResult,
    VoiceMutation, VoiceMutationKind, VoiceStartIntent, VoiceUnroutedReason,
};

fn busy(failure: &CommandFailure<AgentFailure>) -> bool {
    matches!(
        failure,
        CommandFailure::NotAdmitted(AgentFailure::StorageBusy)
            | CommandFailure::Indeterminate(AgentFailure::StorageBusy)
            | CommandFailure::Admitted(AgentFailure::StorageBusy)
    )
}
async fn mutate(
    voice: &floe_voice::VoiceService,
    actor: &OwnerActor,
    record: &VoiceCallRecord,
    kind: VoiceMutationKind,
) -> VoiceCallRecord {
    let request = VoiceMutation {
        voice_session_id: record.intent.binding.voice_session_id,
        command_id: Uuid::new_v4(),
        expected_revision: record.revision,
        kind,
    };
    let scope = host_scope(request.command_id, Cancellation::new(), WAIT_TIMEOUT);
    support::exact_recovery::observe_exact_request(
        request,
        |exact| voice.mutate(actor, exact, &scope),
        busy,
    )
    .await
    .unwrap()
    .record
}
fn capturing(host: &AppHost<AppComposition>, session: Uuid) -> VoiceCallRecord {
    support::with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            let plan = LiveModelPlan {
                operation_id: Uuid::new_v4(),
                actor: actor.clone(),
                capabilities: LiveModelCapabilities::conversational(),
                limits: LiveTransportLimits {
                    initial_context_bytes: 16384,
                    append_bytes: 500,
                    local_session_duration_ms: 60000,
                },
                boundary: ProcessingBoundary::Device,
                binding_digest: ModelBindingDigest([1; 32]),
                selection_commitment: ModelSelectionCommitment([2; 32]),
            };
            let binding = VoiceBinding {
                actor: actor.clone(),
                surface_session_id: session,
                voice_session_id: Uuid::new_v4(),
                media_lease_id: Uuid::new_v4(),
                plan_id: plan.operation_id,
            };
            let projection = LiveContextProjection {
                person_id: actor.person_id,
                device_id: actor.device_id.clone(),
                runtime_epoch: actor.runtime_epoch,
                voice_session_id: binding.voice_session_id,
                plan_id: plan.operation_id,
                binding_digest: plan.binding_digest,
                projection_id: Uuid::new_v4(),
                projection_revision: 1,
                source: LiveProjectionSource::SurfacePrefix {
                    through: owners
                        .conversation
                        .surface_head(&actor, session, &scope)
                        .await
                        .unwrap(),
                },
                coverage: DependencyCoverage::Independent,
                memories: vec![],
                input_data_classes: vec![DataClass::Personal],
                payload: LiveContextPayload::new(
                    "Synthetic context; no actual audio or provider".into(),
                )
                .unwrap(),
            };
            support::exact_recovery::observe_exact_request(
                floe_voice::VoiceStartCommand {
                    command_id: binding.voice_session_id,
                    surface_session_id: session,
                },
                |exact| owners.voice.reserve_start(&actor, exact, &scope),
                busy,
            )
            .await
            .unwrap();
            let intent = VoiceStartIntent::prepare(
                binding.clone(),
                &plan,
                &projection,
                floe_execution::budget::ModelReservationCeiling {
                    tokens: 100,
                    cost_micros: 100,
                },
            )
            .unwrap();
            let mut record = support::exact_recovery::observe_exact_request(
                intent,
                |exact| owners.voice.admit_start(&actor, exact, &scope),
                busy,
            )
            .await
            .unwrap()
            .record;
            let command = record.session.pending_media_command().unwrap().clone();
            record = mutate(
                &owners.voice,
                &actor,
                &record,
                VoiceMutationKind::NativeReceipt {
                    command,
                    state: MediaState::Muted,
                },
            )
            .await;
            record = mutate(
                &owners.voice,
                &actor,
                &record,
                VoiceMutationKind::BeginRemoteDispatch {},
            )
            .await;
            record = mutate(
                &owners.voice,
                &actor,
                &record,
                VoiceMutationKind::RemoteReady { binding },
            )
            .await;
            record = mutate(&owners.voice, &actor, &record, VoiceMutationKind::Unmute {}).await;
            let command = record.session.pending_media_command().unwrap().clone();
            mutate(
                &owners.voice,
                &actor,
                &record,
                VoiceMutationKind::NativeReceipt {
                    command,
                    state: MediaState::Capturing,
                },
            )
            .await
        })
    })
}
fn ingest(
    host: &AppHost<AppComposition>,
    voice: Uuid,
    event: LiveProviderEvent,
) -> Result<VoiceIngressResult, CommandFailure<AgentFailure>> {
    support::with_ready(host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            support::exact_recovery::observe_exact_request(
                event,
                |exact| owners.voice.ingest(&actor, voice, exact, &scope),
                busy,
            )
            .await
        })
    })
}
fn person(id: &str) -> LiveProviderEvent {
    LiveProviderEvent::PersonUtteranceFinal {
        utterance_id: id.into(),
        text: USER_TEXT.into(),
    }
}
fn delegate(id: &str, utterance: Option<&str>) -> LiveProviderEvent {
    LiveProviderEvent::DelegationRequested {
        delegation_id: id.into(),
        utterance_ref: utterance.map(str::to_owned),
        input_hint: Some(USER_TEXT.into()),
    }
}
#[test]
fn voice_work_drives_same_manager_fifo_and_survives_hangup_without_duplicate_runs() {
    let (barrier, entered, release) = GenerateBarrier::new();
    let model = ScriptedModel::new(
        PrimaryBehavior::NoGateway,
        USER_TEXT,
        ModelOutput::WaitFirstThenAnswer(barrier),
    );
    let recorder = model.recorder();
    let (_profile, host) = create_ready_app(&model);
    let session = start_session(&host);
    let record = capturing(&host, session);
    let voice = record.intent.binding.voice_session_id;
    let first_input = ingest(&host, voice, person("first")).unwrap();
    assert!(first_input.admission.input.is_some());
    let first = ingest(&host, voice, delegate("d1", Some("first"))).unwrap();
    assert!(first.wake_failure.is_none());
    let first_work = first.admission.work.unwrap();
    entered
        .recv_timeout(WAIT_TIMEOUT)
        .expect("Voice wakes the real Manager without another text command");
    let replay = ingest(&host, voice, delegate("d1", Some("first"))).unwrap();
    assert!(replay.admission.replayed);
    assert_eq!(replay.admission.work.as_ref(), Some(&first_work));
    let alias = ingest(&host, voice, delegate("d1-alias", Some("first"))).unwrap();
    assert_eq!(alias.admission.work.as_ref(), Some(&first_work));
    assert!(matches!(
        ingest(
            &host,
            voice,
            LiveProviderEvent::PersonUtteranceFinal {
                utterance_id: "first".into(),
                text: "changed body".into()
            }
        ),
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    ));
    let second_input = ingest(&host, voice, person("second")).unwrap();
    let second = ingest(&host, voice, delegate("d2", Some("second")))
        .unwrap()
        .admission
        .work
        .unwrap();
    assert_eq!(second.anchor, second_input.admission.input.unwrap().through);
    let ambiguous = ingest(&host, voice, delegate("hint", None)).unwrap();
    assert!(matches!(
        ambiguous.admission.record.outcome,
        VoiceEventOutcome::Unrouted {
            reason: VoiceUnroutedReason::AmbiguousInput,
            ..
        }
    ));
    assert!(ambiguous.admission.work.is_none());
    let filler = ingest(
        &host,
        voice,
        LiveProviderEvent::AssistantUtteranceFinal {
            utterance_id: "filler".into(),
            text: "I am checking that.".into(),
        },
    )
    .unwrap();
    assert!(filler.admission.input.is_none() && filler.admission.work.is_none());
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let record = mutate(&owners.voice, &actor, &record, VoiceMutationKind::Stop {}).await;
            let command = record.session.pending_media_command().unwrap().clone();
            let record = mutate(
                &owners.voice,
                &actor,
                &record,
                VoiceMutationKind::NativeReceipt {
                    command,
                    state: MediaState::Stopped,
                },
            )
            .await;
            assert_eq!(record.session.media(), MediaState::Stopped);
        });
    });
    let after_stop = ingest(&host, voice, delegate("d2-after-stop", Some("second"))).unwrap();
    assert_eq!(after_stop.admission.work.as_ref(), Some(&second));
    ingest(&host, voice, person("late-final")).unwrap();
    let stopped = ingest(&host, voice, delegate("late-work", Some("late-final"))).unwrap();
    assert!(matches!(
        stopped.admission.record.outcome,
        VoiceEventOutcome::Unrouted {
            reason: VoiceUnroutedReason::SessionStopped,
            ..
        }
    ));
    assert_eq!(
        recorder.snapshot().generated.len(),
        1,
        "another live input never bypasses active Manager"
    );
    release.send(()).unwrap();
    let run1 = support::wait_work_run(&host, first_work);
    assert_eq!(
        wait_terminal_run(&host, run1.run_id).state,
        RunState::Completed
    );
    let run2 = support::wait_work_run(&host, second);
    assert_eq!(
        wait_terminal_run(&host, run2.run_id).state,
        RunState::Completed
    );
    assert_ne!(run1.run_id, run2.run_id);
    assert_eq!(assert_script_clean(&recorder).generated.len(), 2);
}

/// Delays only custody acknowledgement to test the service's owned lifetime.
/// The receipt itself came from the real journal above; this is not a transport.
struct DelayedJournal {
    receipt: floe_voice::VoiceIngressAdmission,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    finished: std::sync::atomic::AtomicUsize,
}
impl floe_voice::VoiceJournal for DelayedJournal {
    fn reserve_start<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: floe_voice::VoiceStartCommand,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceStartCommandAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async { panic!("this custody fixture must not prepare a call") })
    }
    fn read_start<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<Option<floe_voice::VoiceStartCommandRecord>, AgentFailure>,
    > {
        Box::pin(async { panic!("this custody fixture must not read preparation") })
    }
    fn read_recoverable_start<'a>(
        &'a self,
        _: &'a OwnerActor,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<Option<floe_voice::VoiceStartCommandRecord>, AgentFailure>,
    > {
        Box::pin(async { panic!("this custody fixture must not recover preparation") })
    }
    fn cancel_start<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: floe_voice::VoiceStartCommand,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceStartCommandRecord, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async { panic!("this custody fixture must not cancel preparation") })
    }

    fn admit_start<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: VoiceStartIntent,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceStartAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async { Err(CommandFailure::NotAdmitted(AgentFailure::InvalidInput)) })
    }
    fn read_call<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
    ) -> floe_execution::BoxFuture<'a, Result<Option<VoiceCallRecord>, AgentFailure>> {
        Box::pin(async { Err(AgentFailure::InvalidInput) })
    }
    fn mutate<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: VoiceMutation,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceMutationAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async { Err(CommandFailure::NotAdmitted(AgentFailure::InvalidInput)) })
    }
}
impl floe_voice::VoiceIngressJournal for DelayedJournal {
    fn read_event<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: &'a str,
        _: &'a str,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<Option<floe_voice::VoiceIngressAdmission>, AgentFailure>,
    > {
        Box::pin(async { Ok(Some(self.receipt.clone())) })
    }
    fn ingest<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: LiveProviderEvent,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceIngressAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async move {
            self.entered.notify_one();
            self.release.notified().await;
            self.finished
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            Ok(self.receipt.clone())
        })
    }
}
#[test]
fn voice_custody_outlives_observer_cancellation_and_retirement_drains_it() {
    use std::sync::{Arc, atomic::Ordering};
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let session = start_session(&host);
    let call = capturing(&host, session);
    let voice_id = call.intent.binding.voice_session_id;
    let receipt = ingest(&host, voice_id, person("lifetime"))
        .unwrap()
        .admission;
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let journal = Arc::new(DelayedJournal {
                receipt,
                entered: Default::default(),
                release: Default::default(),
                finished: Default::default(),
            });
            let voice = Arc::new(
                floe_voice::VoiceService::new(
                    actor.clone(),
                    journal.clone(),
                    owners.conversation.clone(),
                )
                .unwrap(),
            );
            let cancellation = Cancellation::new();
            cancellation.cancel_with_reason(floe_execution::CancelReason::User);
            let scope = host_scope(Uuid::new_v4(), cancellation, WAIT_TIMEOUT);
            assert!(matches!(
                voice
                    .ingest(&actor, voice_id, person("lifetime"), &scope)
                    .await,
                Err(CommandFailure::NotAdmitted(AgentFailure::Cancelled))
            ));
            let cancellation = Cancellation::new();
            let scope = host_scope(Uuid::new_v4(), cancellation.clone(), WAIT_TIMEOUT);
            let child = voice.clone();
            let child_actor = actor.clone();
            let observer = tokio::spawn(async move {
                child
                    .ingest(&child_actor, voice_id, person("lifetime"), &scope)
                    .await
            });
            tokio::time::timeout(WAIT_TIMEOUT, journal.entered.notified())
                .await
                .unwrap();
            cancellation.cancel_with_reason(floe_execution::CancelReason::User);
            assert!(matches!(
                observer.await.unwrap(),
                Err(CommandFailure::Indeterminate(AgentFailure::Cancelled))
            ));
            assert_eq!(journal.finished.load(Ordering::Acquire), 0);
            let owner_lifetime =
                floe_inference::LiveDispatchCustody::lifetime_cancellation(voice.as_ref(), &actor)
                    .unwrap();
            voice.close_admission();
            assert!(
                owner_lifetime.is_cancelled(),
                "retirement reaches a retained live dispatcher"
            );
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            assert!(matches!(
                voice
                    .ingest(&actor, voice_id, person("lifetime"), &scope)
                    .await,
                Err(CommandFailure::NotAdmitted(AgentFailure::VaultUnavailable))
            ));
            let child = voice.clone();
            let drain = tokio::spawn(async move { child.shutdown_and_drain(&scope).await });
            tokio::task::yield_now().await;
            assert!(
                !drain.is_finished(),
                "retirement cannot pretend pending custody completed"
            );
            journal.release.notify_one();
            tokio::time::timeout(WAIT_TIMEOUT, drain)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(journal.finished.load(Ordering::Acquire), 1);
        });
    });
}

/// A delayed read before the accounting mutation must already belong to the
/// admitted custody task, rather than the UI observer's cancellable future.
struct DelayedSettlementJournal {
    start: std::sync::Mutex<Option<floe_voice::VoiceStartCommandRecord>>,
    pause_reads: std::sync::atomic::AtomicBool,
    record: std::sync::Mutex<VoiceCallRecord>,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    writes: std::sync::atomic::AtomicUsize,
}
impl floe_voice::VoiceJournal for DelayedSettlementJournal {
    fn reserve_start<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command: floe_voice::VoiceStartCommand,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceStartCommandAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async move {
            let mut slot = self.start.lock().unwrap();
            let replayed = slot.is_some();
            let record = slot.get_or_insert_with(|| floe_voice::VoiceStartCommandRecord {
                actor: actor.clone(),
                command: command.clone(),
                state: floe_voice::VoiceStartCommandState::Preparing,
            });
            if record.actor != *actor || record.command != command {
                return Err(CommandFailure::NotAdmitted(AgentFailure::PolicyDenied));
            }
            Ok(floe_voice::VoiceStartCommandAdmission {
                record: record.clone(),
                replayed,
            })
        })
    }
    fn read_start<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<Option<floe_voice::VoiceStartCommandRecord>, AgentFailure>,
    > {
        Box::pin(async move { Ok(self.start.lock().unwrap().clone()) })
    }
    fn read_recoverable_start<'a>(
        &'a self,
        _: &'a OwnerActor,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<Option<floe_voice::VoiceStartCommandRecord>, AgentFailure>,
    > {
        Box::pin(async move {
            Ok(self
                .start
                .lock()
                .unwrap()
                .clone()
                .filter(|record| record.state == floe_voice::VoiceStartCommandState::Preparing))
        })
    }
    fn cancel_start<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: floe_voice::VoiceStartCommand,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceStartCommandRecord, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async move {
            let mut slot = self.start.lock().unwrap();
            let record = slot.as_mut().unwrap();
            if record.state == floe_voice::VoiceStartCommandState::Bound {
                return Err(CommandFailure::NotAdmitted(AgentFailure::Conflict));
            }
            record.state = floe_voice::VoiceStartCommandState::Cancelled;
            Ok(record.clone())
        })
    }
    fn admit_start<'a>(
        &'a self,
        _: &'a OwnerActor,
        intent: VoiceStartIntent,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceStartAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async move {
            let record = self.record.lock().unwrap().clone();
            if record.intent != intent {
                return Err(CommandFailure::NotAdmitted(AgentFailure::PolicyDenied));
            }
            let mut slot = self.start.lock().unwrap();
            let start = slot.as_mut().unwrap();
            if start.state != floe_voice::VoiceStartCommandState::Preparing {
                return Err(CommandFailure::NotAdmitted(AgentFailure::Conflict));
            }
            start.state = floe_voice::VoiceStartCommandState::Bound;
            Ok(floe_voice::VoiceStartAdmission {
                record,
                replayed: false,
            })
        })
    }
    fn read_call<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
    ) -> floe_execution::BoxFuture<'a, Result<Option<VoiceCallRecord>, AgentFailure>> {
        Box::pin(async move {
            if self.pause_reads.load(std::sync::atomic::Ordering::SeqCst) {
                self.entered.notify_one();
                self.release.notified().await;
            }
            Ok(Some(self.record.lock().unwrap().clone()))
        })
    }
    fn mutate<'a>(
        &'a self,
        actor: &'a OwnerActor,
        request: VoiceMutation,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceMutationAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async move {
            assert!(matches!(
                request.kind,
                VoiceMutationKind::SettleUsage { .. }
            ));
            let mut record = self.record.lock().unwrap();
            let (next, receipt) = record
                .apply(actor, &request)
                .map_err(CommandFailure::NotAdmitted)?;
            *record = next.clone();
            self.writes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(floe_voice::VoiceMutationAdmission {
                record: next,
                receipt,
                replayed: false,
            })
        })
    }
}
impl floe_voice::VoiceIngressJournal for DelayedSettlementJournal {
    fn read_event<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: &'a str,
        _: &'a str,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<Option<floe_voice::VoiceIngressAdmission>, AgentFailure>,
    > {
        Box::pin(async { panic!("settlement cannot read an ingress event") })
    }
    fn ingest<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: LiveProviderEvent,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceIngressAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async { panic!("settlement cannot admit new input") })
    }
}
#[test]
fn admitted_voice_usage_settlement_survives_cancel_during_read_and_shutdown() {
    use floe_execution::budget::{ModelAccounting, ModelAttemptReceipt};
    use std::sync::{Arc, atomic::Ordering};
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let surface = start_session(&host);
    let call = capturing(&host, surface);
    let voice_id = call.intent.binding.voice_session_id;
    let receipt = ModelAttemptReceipt {
        attempt_id: call.intent.binding.plan_id,
        charged_tokens: 7,
        charged_cost_micros: 3,
        accounting: ModelAccounting {
            observed_tokens: Some(7),
            observed_cost_micros: Some(3),
            unknown_tokens: false,
            unknown_cost: false,
        },
        dispatched: true,
    };
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let journal = Arc::new(DelayedSettlementJournal {
                start: Default::default(),
                pause_reads: std::sync::atomic::AtomicBool::new(true),
                record: std::sync::Mutex::new(call),
                entered: Default::default(),
                release: Default::default(),
                writes: Default::default(),
            });
            let voice = Arc::new(
                floe_voice::VoiceService::new(
                    actor.clone(),
                    journal.clone(),
                    owners.conversation.clone(),
                )
                .unwrap(),
            );
            let cancellation = Cancellation::new();
            let scope = host_scope(Uuid::new_v4(), cancellation.clone(), WAIT_TIMEOUT);
            let child = voice.clone();
            let child_actor = actor.clone();
            let observer = tokio::spawn(async move {
                child
                    .settle_usage(&child_actor, voice_id, receipt, &scope)
                    .await
            });
            tokio::time::timeout(WAIT_TIMEOUT, journal.entered.notified())
                .await
                .unwrap();
            cancellation.cancel_with_reason(floe_execution::CancelReason::User);
            assert!(matches!(
                observer.await.unwrap(),
                Err(CommandFailure::Indeterminate(AgentFailure::Cancelled))
            ));
            voice.close_admission();
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            assert!(matches!(
                voice.settle_usage(&actor, voice_id, receipt, &scope).await,
                Err(CommandFailure::NotAdmitted(AgentFailure::VaultUnavailable))
            ));
            let child = voice.clone();
            let drain = tokio::spawn(async move { child.shutdown_and_drain(&scope).await });
            tokio::task::yield_now().await;
            journal.release.notify_one();
            tokio::time::timeout(WAIT_TIMEOUT, drain)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(journal.writes.load(Ordering::SeqCst), 1);
            let record = journal.record.lock().unwrap();
            assert_eq!(record.usage, Some(receipt));
            assert!(record.session.stop_requested());
            assert_ne!(record.session.media(), MediaState::Stopped);
        });
    });
}

#[test]
fn registered_voice_controller_outlives_observer_and_settles_after_retirement() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let surface = start_session(&host);
    let call = VoiceCallRecord::prepare(capturing(&host, surface).intent).unwrap();
    let intent = call.intent.clone();
    let binding = intent.binding.clone();
    let command = floe_voice::VoiceStartCommand {
        command_id: binding.voice_session_id,
        surface_session_id: surface,
    };
    let receipt = floe_execution::budget::ModelAttemptReceipt {
        attempt_id: binding.plan_id,
        charged_tokens: 7,
        charged_cost_micros: 3,
        accounting: floe_execution::budget::ModelAccounting {
            observed_tokens: Some(7),
            observed_cost_micros: Some(3),
            unknown_tokens: false,
            unknown_cost: false,
        },
        dispatched: true,
    };
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let journal = Arc::new(DelayedSettlementJournal {
                start: Default::default(),
                pause_reads: AtomicBool::new(false),
                record: std::sync::Mutex::new(call),
                entered: Default::default(),
                release: Default::default(),
                writes: Default::default(),
            });
            let voice = Arc::new(
                floe_voice::VoiceService::new(
                    actor.clone(),
                    journal.clone(),
                    owners.conversation.clone(),
                )
                .unwrap(),
            );
            let entered = Arc::new(tokio::sync::Notify::new());
            let driver_entered = entered.clone();
            let cancellation = Cancellation::new();
            let scope = host_scope(Uuid::new_v4(), cancellation.clone(), WAIT_TIMEOUT);
            let child = voice.clone();
            let driver_command = command.clone();
            let driver_actor = actor.clone();
            let observer = tokio::spawn(async move {
                child
                    .run_start_controller(
                        &driver_actor,
                        driver_command,
                        &scope,
                        move |preparation| {
                            Box::pin(async move {
                                let controller = preparation.bind(intent).await?;
                                driver_entered.notify_one();
                                controller.cancellation().cancelled().await;
                                assert_eq!(
                                    controller
                                        .settle_usage(receipt)
                                        .await
                                        .map_err(CommandFailure::into_failure)?,
                                    receipt
                                );
                                Ok(())
                            })
                        },
                    )
                    .await
            });
            tokio::time::timeout(WAIT_TIMEOUT, entered.notified())
                .await
                .unwrap();
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            let replay = voice
                .run_start_controller(&actor, command.clone(), &scope, |_| {
                    Box::pin(async { panic!("duplicate driver must not start") })
                })
                .await
                .unwrap();
            assert_eq!(replay.state, floe_voice::VoiceStartCommandState::Bound);
            cancellation.cancel();
            assert!(matches!(
                observer.await.unwrap(),
                Err(CommandFailure::Indeterminate(AgentFailure::Cancelled))
            ));
            journal.pause_reads.store(true, Ordering::SeqCst);
            voice.close_admission();
            assert!(matches!(
                voice
                    .run_start_controller(&actor, command, &scope, |_| Box::pin(async {
                        panic!("retired owner must not start another driver")
                    }))
                    .await,
                Err(CommandFailure::NotAdmitted(AgentFailure::VaultUnavailable))
            ));
            tokio::time::timeout(WAIT_TIMEOUT, journal.entered.notified())
                .await
                .unwrap();
            let child = voice.clone();
            let drain = tokio::spawn(async move { child.shutdown_and_drain(&scope).await });
            tokio::task::yield_now().await;
            assert!(!drain.is_finished());
            journal.release.notify_one();
            tokio::time::timeout(WAIT_TIMEOUT, drain)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(journal.writes.load(Ordering::SeqCst), 1);
            let record = journal.record.lock().unwrap();
            assert_eq!(record.usage, Some(receipt));
            assert_ne!(record.session.media(), MediaState::Stopped);
        });
    });
}

#[test]
fn voice_controller_failure_is_not_hidden_by_observer_delivery_or_later_drain() {
    use std::sync::{Arc, atomic::AtomicBool};
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let surface = start_session(&host);
    let call = VoiceCallRecord::prepare(capturing(&host, surface).intent).unwrap();
    let intent = call.intent.clone();
    let binding = intent.binding.clone();
    let command = floe_voice::VoiceStartCommand {
        command_id: binding.voice_session_id,
        surface_session_id: surface,
    };
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let journal = Arc::new(DelayedSettlementJournal {
                start: Default::default(),
                pause_reads: AtomicBool::new(false),
                record: std::sync::Mutex::new(call),
                entered: Default::default(),
                release: Default::default(),
                writes: Default::default(),
            });
            let voice =
                floe_voice::VoiceService::new(actor.clone(), journal, owners.conversation.clone())
                    .unwrap();
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            let mut foreign = actor.clone();
            foreign.runtime_epoch += 1;
            assert!(matches!(
                voice
                    .run_start_controller(&foreign, command.clone(), &scope, |_| Box::pin(async {
                        panic!("foreign actor cannot enter a driver")
                    }))
                    .await,
                Err(CommandFailure::NotAdmitted(AgentFailure::PolicyDenied))
            ));
            let (send, receive) = tokio::sync::oneshot::channel();
            assert!(matches!(
                voice
                    .run_start_controller(&actor, command, &scope, move |preparation| Box::pin(
                        async move {
                            let controller = preparation.bind(intent).await?;
                            send.send(controller.cancellation().clone()).unwrap();
                            Err(AgentFailure::StorageUnavailable)
                        }
                    ))
                    .await,
                Err(CommandFailure::Admitted(AgentFailure::StorageUnavailable))
            ));
            assert!(
                receive.await.unwrap().is_cancelled(),
                "driver exit fences its child lifetime"
            );
            assert_eq!(
                voice.shutdown_and_drain(&scope).await,
                Err(AgentFailure::Interrupted)
            );
            assert_eq!(
                voice.shutdown_and_drain(&scope).await,
                Err(AgentFailure::Interrupted)
            );
        });
    });
}

impl floe_voice::VoiceOutboundJournal for DelayedJournal {
    fn pending_result<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: &'a str,
    ) -> floe_execution::BoxFuture<'a, Result<Option<floe_voice::VoiceResultCandidate>, AgentFailure>>
    {
        Box::pin(async { panic!("fixture cannot discover results") })
    }

    fn claim_append<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: floe_voice::VoiceAppendIntent,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceAppendAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async { panic!("fixture cannot append model input") })
    }
    fn settle_append<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: floe_voice::VoiceAppendReceipt,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceAppendRecord, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async { panic!("fixture cannot settle append") })
    }
    fn read_append<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: Uuid,
    ) -> floe_execution::BoxFuture<'a, Result<Option<floe_voice::VoiceAppendRecord>, AgentFailure>>
    {
        Box::pin(async { panic!("fixture cannot read append") })
    }
}

impl floe_voice::VoiceOutboundJournal for DelayedSettlementJournal {
    fn pending_result<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: &'a str,
    ) -> floe_execution::BoxFuture<'a, Result<Option<floe_voice::VoiceResultCandidate>, AgentFailure>>
    {
        Box::pin(async { panic!("fixture cannot discover results") })
    }

    fn claim_append<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: floe_voice::VoiceAppendIntent,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceAppendAdmission, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async { panic!("fixture cannot append model input") })
    }
    fn settle_append<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: floe_voice::VoiceAppendReceipt,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_voice::VoiceAppendRecord, CommandFailure<AgentFailure>>,
    > {
        Box::pin(async { panic!("fixture cannot settle append") })
    }
    fn read_append<'a>(
        &'a self,
        _: &'a OwnerActor,
        _: Uuid,
        _: Uuid,
    ) -> floe_execution::BoxFuture<'a, Result<Option<floe_voice::VoiceAppendRecord>, AgentFailure>>
    {
        Box::pin(async { panic!("fixture cannot read append") })
    }
}

// Exercise the real encrypted Start journal before any provider/native work.
fn preparation_intent(
    actor: OwnerActor,
    command: &floe_voice::VoiceStartCommand,
) -> VoiceStartIntent {
    let plan = LiveModelPlan {
        operation_id: Uuid::new_v4(),
        actor: actor.clone(),
        capabilities: LiveModelCapabilities::conversational(),
        limits: LiveTransportLimits {
            initial_context_bytes: 16384,
            append_bytes: 500,
            local_session_duration_ms: 60000,
        },
        boundary: ProcessingBoundary::Device,
        binding_digest: ModelBindingDigest([1; 32]),
        selection_commitment: ModelSelectionCommitment([2; 32]),
    };
    let projection = LiveContextProjection {
        person_id: actor.person_id,
        device_id: actor.device_id.clone(),
        runtime_epoch: actor.runtime_epoch,
        voice_session_id: command.command_id,
        plan_id: plan.operation_id,
        binding_digest: plan.binding_digest,
        projection_id: Uuid::new_v4(),
        projection_revision: 1,
        source: LiveProjectionSource::SurfacePrefix {
            through: floe_conversation::SurfaceAnchor::empty(command.surface_session_id).unwrap(),
        },
        coverage: DependencyCoverage::Independent,
        memories: vec![],
        input_data_classes: vec![DataClass::Synthetic],
        payload: LiveContextPayload::new("Synthetic preparation only".into()).unwrap(),
    };
    VoiceStartIntent::prepare(
        VoiceBinding {
            actor,
            voice_session_id: command.command_id,
            surface_session_id: command.surface_session_id,
            plan_id: plan.operation_id,
            media_lease_id: Uuid::new_v4(),
        },
        &plan,
        &projection,
        floe_execution::budget::ModelReservationCeiling {
            tokens: 10,
            cost_micros: 10,
        },
    )
    .unwrap()
}

fn preparation_survives_observer_and_fences_late_binding(retire: bool) {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let surface = start_session(&host);
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let voice = owners.voice.clone();
            let command = floe_voice::VoiceStartCommand {
                command_id: Uuid::new_v4(),
                surface_session_id: surface,
            };
            let intent = preparation_intent(actor.clone(), &command);
            let count = Arc::new(AtomicUsize::new(0));
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let (token_send, token_receive) = tokio::sync::oneshot::channel();
            let cancel = Cancellation::new();
            let observe = host_scope(Uuid::new_v4(), cancel.clone(), WAIT_TIMEOUT);
            let child = voice.clone();
            let owner = actor.clone();
            let request = command.clone();
            let child_count = count.clone();
            let child_entered = entered.clone();
            let child_release = release.clone();
            let observer = tokio::spawn(async move {
                child
                    .run_start_controller(&owner, request, &observe, move |preparation| {
                        Box::pin(async move {
                            child_count.fetch_add(1, Ordering::SeqCst);
                            token_send.send(preparation.cancellation().clone()).unwrap();
                            child_entered.notify_one();
                            // Deliberately ignores cancellation until released: late
                            // planning must remain owned and must not bind a call.
                            child_release.notified().await;
                            assert_eq!(
                                preparation.bind(intent).await.err(),
                                Some(AgentFailure::Cancelled)
                            );
                            Ok(())
                        })
                    })
                    .await
            });
            tokio::time::timeout(WAIT_TIMEOUT, entered.notified())
                .await
                .unwrap();
            let lifetime = token_receive.await.unwrap();
            cancel.cancel();
            assert!(matches!(
                observer.await.unwrap(),
                Err(CommandFailure::Indeterminate(AgentFailure::Cancelled))
            ));
            assert!(
                !lifetime.is_cancelled(),
                "screen observer is not the preparation lifetime"
            );
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            for _ in 0..3 {
                let replay = voice
                    .run_start_controller(&actor, command.clone(), &scope, |_| {
                        Box::pin(async {
                            panic!("exact replay must have no planning/native effect")
                        })
                    })
                    .await
                    .unwrap();
                assert_eq!(replay.state, floe_voice::VoiceStartCommandState::Preparing);
            }
            if retire {
                voice.close_admission();
            } else {
                let cancelled = voice
                    .cancel_start(&actor, command.clone(), &scope)
                    .await
                    .unwrap();
                assert_eq!(
                    cancelled.state,
                    floe_voice::VoiceStartCommandState::Cancelled
                );
            }
            assert!(lifetime.is_cancelled());
            let child = voice.clone();
            let drain_scope = scope.clone();
            let drain = tokio::spawn(async move { child.shutdown_and_drain(&drain_scope).await });
            tokio::task::yield_now().await;
            assert!(
                !drain.is_finished(),
                "late preparation must remain in generation drain"
            );
            release.notify_one();
            tokio::time::timeout(WAIT_TIMEOUT, drain)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(count.load(Ordering::SeqCst), 1);
        });
    });
}
#[test]
fn voice_preparation_survives_observer_and_explicit_cancel_fences_late_binding() {
    preparation_survives_observer_and_fences_late_binding(false);
}
#[test]
fn voice_preparation_retirement_drains_late_planning_without_binding() {
    preparation_survives_observer_and_fences_late_binding(true);
}

#[test]
fn voice_preparation_failure_cancels_custody_and_replay_never_replans() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let surface = start_session(&host);
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            let command = floe_voice::VoiceStartCommand {
                command_id: Uuid::new_v4(),
                surface_session_id: surface,
            };
            assert!(matches!(
                owners
                    .voice
                    .run_start_controller(&actor, command.clone(), &scope, |_| Box::pin(async {
                        Err(AgentFailure::ServerModelUnavailable)
                    }))
                    .await,
                Err(CommandFailure::Admitted(
                    AgentFailure::ServerModelUnavailable
                ))
            ));
            let record = owners
                .voice
                .read_start(&actor, command.command_id, &scope)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(record.state, floe_voice::VoiceStartCommandState::Cancelled);
            assert!(
                owners
                    .voice
                    .read_call(&actor, command.command_id, &scope)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                owners
                    .voice
                    .run_start_controller(&actor, command, &scope, |_| Box::pin(async {
                        panic!("failed preparation must not be reissued")
                    }))
                    .await
                    .unwrap(),
                record
            );
            assert_eq!(
                owners.voice.shutdown_and_drain(&scope).await,
                Err(AgentFailure::Interrupted)
            );
        });
    });
}

#[test]
fn voice_start_cancelled_observer_before_admission_has_no_custody_or_driver() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let surface = start_session(&host);
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let command = floe_voice::VoiceStartCommand {
                command_id: Uuid::new_v4(),
                surface_session_id: surface,
            };
            let cancel = Cancellation::new();
            cancel.cancel();
            let scope = host_scope(Uuid::new_v4(), cancel, WAIT_TIMEOUT);
            assert!(matches!(
                owners
                    .voice
                    .run_start_controller(&actor, command.clone(), &scope, |_| Box::pin(async {
                        panic!("cancelled admission cannot start preparation")
                    }))
                    .await,
                Err(CommandFailure::NotAdmitted(AgentFailure::Cancelled))
            ));
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            assert!(
                owners
                    .voice
                    .read_start(&actor, command.command_id, &scope)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                owners
                    .voice
                    .read_call(&actor, command.command_id, &scope)
                    .await
                    .unwrap()
                    .is_none()
            );
        });
    });
}

#[test]
fn voice_preparation_cannot_bind_another_command_and_changed_replay_has_no_driver() {
    let model = ScriptedModel::new(PrimaryBehavior::NoGateway, USER_TEXT, ModelOutput::Answer);
    let (_profile, host) = create_ready_app(&model);
    let surface = start_session(&host);
    let other_surface = start_session(&host);
    support::with_ready(&host, |services, caller, owners| {
        let actor = caller.owner_actor();
        services.execute_owner(async move {
            let command = floe_voice::VoiceStartCommand {
                command_id: Uuid::new_v4(),
                surface_session_id: surface,
            };
            let mut different = command.clone();
            different.command_id = Uuid::new_v4();
            let intent = preparation_intent(actor.clone(), &different);
            let scope = host_scope(Uuid::new_v4(), Cancellation::new(), WAIT_TIMEOUT);
            let record = owners
                .voice
                .run_start_controller(&actor, command.clone(), &scope, move |preparation| {
                    Box::pin(async move {
                        assert_eq!(
                            preparation.bind(intent).await.err(),
                            Some(AgentFailure::PolicyDenied)
                        );
                        Ok(())
                    })
                })
                .await
                .unwrap();
            assert_eq!(record.state, floe_voice::VoiceStartCommandState::Cancelled);
            assert!(
                owners
                    .voice
                    .read_call(&actor, different.command_id, &scope)
                    .await
                    .unwrap()
                    .is_none()
            );
            let mut changed = command.clone();
            changed.surface_session_id = other_surface;
            assert!(matches!(
                owners
                    .voice
                    .run_start_controller(&actor, changed, &scope, |_| Box::pin(async {
                        panic!("changed replay cannot plan")
                    }))
                    .await,
                Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
            ));
            assert_eq!(
                owners
                    .voice
                    .run_start_controller(&actor, command, &scope, |_| Box::pin(async {
                        panic!("cancelled replay cannot plan")
                    }))
                    .await
                    .unwrap(),
                record
            );
        });
    });
}
