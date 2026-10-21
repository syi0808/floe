//! Voice Start's App-owned native lease, Gateway handoff and retained event driver.
use crate::ReadyOwners;
use floe_execution::{
    Cancellation, ExecutionScope,
    budget::{BudgetConfig, BudgetLedger, ModelUsage},
};
use floe_kernel::{AgentFailure, OwnerActor, TraceContext};
use floe_model_contract::LiveModelCapabilities;
use floe_native::live_media::{
    LiveMediaBinding, LiveMediaLease, LiveMediaStopReceipt, ProcessLiveMediaRegistry,
};
use floe_voice::{
    MediaCommand, MediaCommandKind, MediaState, VoiceBinding, VoiceMutation, VoiceMutationKind,
    VoicePreparation, VoiceStartIntent,
};
use std::sync::Arc;
use tokio::{sync::oneshot, time::Instant};
use uuid::Uuid;

const MAX_LIVE_TOKENS: u64 = 409_600;
const MAX_LIVE_COST_MICROS: u64 = 50_000;
const STOP_OBSERVATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub(crate) fn live_scope(
    request_id: Uuid,
    cancellation: Cancellation,
    timeout: std::time::Duration,
) -> ExecutionScope {
    let budget = BudgetLedger::new(
        BudgetConfig::new(MAX_LIVE_TOKENS, MAX_LIVE_COST_MICROS),
        ModelUsage::default(),
    );
    ExecutionScope::root(
        cancellation,
        Instant::now() + timeout,
        budget.root_lease(),
        TraceContext::new(request_id),
    )
}

pub(crate) async fn drive_start<'a>(
    preparation: VoicePreparation<'a>,
    owners: Arc<ReadyOwners>,
    registry: Arc<ProcessLiveMediaRegistry>,
    actor: OwnerActor,
    surface_session_id: Uuid,
    voice_session_id: Uuid,
    scope: ExecutionScope,
    bound: Arc<std::sync::Mutex<Option<oneshot::Sender<Result<(), AgentFailure>>>>>,
) -> Result<(), AgentFailure> {
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        let _ = (
            preparation,
            owners,
            registry,
            actor,
            surface_session_id,
            voice_session_id,
            scope,
            bound,
        );
        return Err(AgentFailure::CapabilityUnavailable);
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        let scope = ExecutionScope::root(
            preparation.cancellation().clone(),
            scope.deadline(),
            scope.budget().child(MAX_LIVE_TOKENS, MAX_LIVE_COST_MICROS),
            scope.trace_context(),
        );
        let prepared = owners
            .live_inference
            .prepare_live(
                floe_inference::LiveModelPlanRequest {
                    actor: actor.clone(),
                    required_capabilities: LiveModelCapabilities::conversational(),
                },
                &scope,
            )
            .await?;
        let plan = prepared.plan().clone();
        let through = owners
            .conversation
            .surface_head(&actor, surface_session_id, &scope)
            .await?;
        let source = floe_context_contract::LiveProjectionSource::SurfacePrefix { through };
        let projection = match owners
        .live_context_projector
        .project_prefix(floe_context::LivePrefixProjectionRequest {
            plan: &plan,
            voice_session_id,
            surface_session_id,
            projection_id: Uuid::new_v4(),
            source: &source,
            instructions: "Present the canonical conversation. Delegate substantive work to Manager.",
        }, &scope)
        .await?
    {
        floe_context::LiveProjectionOutcome::Ready(projection) => projection,
        floe_context::LiveProjectionOutcome::NeedsSourceReview(_) => {
            return Err(AgentFailure::PolicyDenied);
        }
        floe_context::LiveProjectionOutcome::NeedsCompaction { .. } => {
            return Err(AgentFailure::ModelInputCapacityExceeded);
        }
    };
        let reservation = floe_execution::budget::ModelReservationCeiling {
            tokens: MAX_LIVE_TOKENS,
            cost_micros: MAX_LIVE_COST_MICROS,
        };
        let voice_binding = VoiceBinding {
            actor: actor.clone(),
            surface_session_id,
            voice_session_id,
            media_lease_id: Uuid::new_v4(),
            plan_id: plan.operation_id,
        };
        let intent =
            VoiceStartIntent::prepare(voice_binding.clone(), &plan, &projection, reservation)?;
        let native_binding = native_binding(&voice_binding);
        // Own the muted candidate before durably binding Voice. If the Apple
        // backend is unavailable, Voice can still cancel the unbound Start
        // without leaving a bound call that has no physical lease to stop.
        let lease =
            prepare_apple_lease(native_binding.clone(), registry.host_instance_id()).await?;
        let lease: Arc<dyn LiveMediaLease> = lease;
        if lease.binding() != &native_binding {
            let _ = direct_stop(lease.clone(), scope.deadline()).await;
            return Err(AgentFailure::PolicyDenied);
        }
        let controller = match preparation.bind(intent).await {
            Ok(controller) => controller,
            Err(error) => {
                let cleanup = direct_stop(lease.clone(), scope.deadline()).await;
                return Err(cleanup.err().unwrap_or(error));
            }
        };
        let record = owners
            .voice
            .read_call(&actor, voice_session_id, &scope)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let prepare_command = record
            .session
            .pending_media_command()
            .filter(|command| {
                command.kind == MediaCommandKind::PrepareMuted
                    && voice_command_matches(&voice_binding, command)
            })
            .cloned()
            .ok_or(AgentFailure::Conflict)?;
        let register = registry.clone();
        let register_lease = lease.clone();
        if let Err(error) = tokio::task::spawn_blocking(move || register.register(register_lease))
            .await
            .map_err(|_| AgentFailure::Interrupted)?
        {
            let stop = stop_voice_media(
                owners.voice.as_ref(),
                &registry,
                &actor,
                &voice_binding,
                &scope,
                Some(lease),
            )
            .await;
            return Err(stop.err().unwrap_or(error));
        }
        if let Err(error) = record_muted_receipt(
            owners.voice.as_ref(),
            &actor,
            &voice_binding,
            &prepare_command,
            &scope,
        )
        .await
        {
            let stop = stop_voice_media(
                owners.voice.as_ref(),
                &registry,
                &actor,
                &voice_binding,
                &scope,
                Some(lease.clone()),
            )
            .await;
            return Err(stop.err().unwrap_or(error));
        }
        signal_bound(&bound);

        let live = async {
            let call = owners
                .live_inference
                .start_live(
                    prepared,
                    floe_inference::LiveStartRequest {
                        voice_session_id,
                        surface_session_id,
                        media_lease_id: voice_binding.media_lease_id,
                        projection,
                        reservation,
                    },
                    owners.voice.as_ref(),
                    &scope,
                )
                .await?;
            let completion = controller
                .consume_live(call, owners.live_context_projector.as_ref())
                .await?;
            completion.failure.map_or(Ok(()), Err)
        }
        .await;
        let stopped = stop_voice_media(
            owners.voice.as_ref(),
            &registry,
            &actor,
            &voice_binding,
            &scope,
            Some(lease),
        )
        .await;
        live.and(stopped)
    }
}

pub(crate) async fn stop_bound_call(
    owners: &ReadyOwners,
    registry: Arc<ProcessLiveMediaRegistry>,
    actor: &OwnerActor,
    voice_session_id: Uuid,
    surface_session_id: Uuid,
    scope: &ExecutionScope,
) -> Result<(), AgentFailure> {
    let record = owners
        .voice
        .read_call(actor, voice_session_id, scope)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    if record.intent.binding.actor != *actor
        || record.intent.binding.surface_session_id != surface_session_id
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let binding = record.intent.binding.clone();
    if record.session.local_lease_released() {
        return Ok(());
    }
    let native = native_binding(&binding);
    registry.fence_binding(&native)?;
    stop_voice_media(
        owners.voice.as_ref(),
        &registry,
        actor,
        &binding,
        scope,
        None,
    )
    .await?;
    Ok(())
}

async fn stop_voice_media(
    voice: &floe_voice::VoiceService,
    registry: &Arc<ProcessLiveMediaRegistry>,
    actor: &OwnerActor,
    binding: &VoiceBinding,
    scope: &ExecutionScope,
    candidate: Option<Arc<dyn LiveMediaLease>>,
) -> Result<(), AgentFailure> {
    let native = native_binding(binding);
    let record = voice
        .read_call(actor, binding.voice_session_id, scope)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    validate_binding(&record, binding)?;
    if record.session.local_lease_released() {
        return Ok(());
    }
    let registered = registry.registered_bindings().contains(&native);
    if registered {
        registry.fence_binding(&native)?;
    } else if let Some(lease) = &candidate {
        if lease.binding() != &native {
            return Err(AgentFailure::PolicyDenied);
        }
        lease.fence();
    }
    let stop_command = ensure_stop_intent(voice, actor, binding, record, scope).await?;
    let receipt: Option<LiveMediaStopReceipt> = if registered {
        let deadline = (Instant::now() + STOP_OBSERVATION_TIMEOUT).min(scope.deadline());
        Some(registry.stop_and_confirm(&native, deadline).await?)
    } else if let Some(lease) = candidate {
        direct_stop(lease, scope.deadline()).await?;
        None
    } else {
        return Err(AgentFailure::CapabilityUnavailable);
    };
    persist_stopped_receipt(voice, actor, binding, &stop_command, scope).await?;
    if let Some(receipt) = receipt {
        if receipt.binding() != &native {
            return Err(AgentFailure::PolicyDenied);
        }
        registry.acknowledge_stop(&receipt)?;
    }
    Ok(())
}

async fn ensure_stop_intent(
    voice: &floe_voice::VoiceService,
    actor: &OwnerActor,
    binding: &VoiceBinding,
    record: floe_voice::VoiceCallRecord,
    scope: &ExecutionScope,
) -> Result<MediaCommand, AgentFailure> {
    if record.session.stop_requested() {
        return record
            .session
            .pending_media_command()
            .filter(|command| {
                command.kind == MediaCommandKind::Stop && voice_command_matches(binding, command)
            })
            .cloned()
            .ok_or(AgentFailure::Conflict);
    }
    let request = VoiceMutation {
        voice_session_id: binding.voice_session_id,
        command_id: deterministic_command_id(
            binding.voice_session_id,
            b"floe.voice.stop-intent.v1\0",
        ),
        expected_revision: record.revision,
        kind: VoiceMutationKind::Stop {},
    };
    match voice.mutate(actor, request, scope).await {
        Ok(admission) => admission
            .record
            .session
            .pending_media_command()
            .filter(|command| {
                command.kind == MediaCommandKind::Stop && voice_command_matches(binding, command)
            })
            .cloned()
            .ok_or(AgentFailure::Conflict),
        Err(failure) => {
            let observed = voice
                .read_call(actor, binding.voice_session_id, scope)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            validate_binding(&observed, binding)?;
            observed
                .session
                .pending_media_command()
                .filter(|command| {
                    command.kind == MediaCommandKind::Stop
                        && voice_command_matches(binding, command)
                })
                .cloned()
                .ok_or_else(|| failure.into_failure())
        }
    }
}

async fn record_muted_receipt(
    voice: &floe_voice::VoiceService,
    actor: &OwnerActor,
    binding: &VoiceBinding,
    command: &MediaCommand,
    scope: &ExecutionScope,
) -> Result<(), AgentFailure> {
    let record = voice
        .read_call(actor, binding.voice_session_id, scope)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    validate_binding(&record, binding)?;
    if record.session.media() == MediaState::Muted
        && record.session.pending_media_command().is_none()
    {
        return Ok(());
    }
    if record.session.pending_media_command() != Some(command)
        || command.kind != MediaCommandKind::PrepareMuted
    {
        return Err(AgentFailure::Conflict);
    }
    let request = VoiceMutation {
        voice_session_id: binding.voice_session_id,
        command_id: deterministic_command_id(
            binding.voice_session_id,
            b"floe.voice.prepare-muted-receipt.v1\0",
        ),
        expected_revision: record.revision,
        kind: VoiceMutationKind::NativeReceipt {
            command: command.clone(),
            state: MediaState::Muted,
        },
    };
    match voice.mutate(actor, request, scope).await {
        Ok(_) => Ok(()),
        Err(failure) => {
            let observed = voice
                .read_call(actor, binding.voice_session_id, scope)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            validate_binding(&observed, binding)?;
            if observed.session.media() == MediaState::Muted
                && observed.session.pending_media_command().is_none()
            {
                Ok(())
            } else {
                Err(failure.into_failure())
            }
        }
    }
}

async fn persist_stopped_receipt(
    voice: &floe_voice::VoiceService,
    actor: &OwnerActor,
    binding: &VoiceBinding,
    command: &MediaCommand,
    scope: &ExecutionScope,
) -> Result<(), AgentFailure> {
    let record = voice
        .read_call(actor, binding.voice_session_id, scope)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    validate_binding(&record, binding)?;
    if record.session.local_lease_released() {
        return Ok(());
    }
    if record.session.pending_media_command() != Some(command)
        || command.kind != MediaCommandKind::Stop
    {
        return Err(AgentFailure::Conflict);
    }
    let request = VoiceMutation {
        voice_session_id: binding.voice_session_id,
        command_id: deterministic_command_id(
            binding.voice_session_id,
            b"floe.voice.stop-receipt.v1\0",
        ),
        expected_revision: record.revision,
        kind: VoiceMutationKind::NativeReceipt {
            command: command.clone(),
            state: MediaState::Stopped,
        },
    };
    match voice.mutate(actor, request, scope).await {
        Ok(_) => Ok(()),
        Err(failure) => {
            let observed = voice
                .read_call(actor, binding.voice_session_id, scope)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            validate_binding(&observed, binding)?;
            if observed.session.local_lease_released() {
                Ok(())
            } else {
                Err(failure.into_failure())
            }
        }
    }
}

async fn direct_stop(
    lease: Arc<dyn LiveMediaLease>,
    deadline: Instant,
) -> Result<(), AgentFailure> {
    lease.fence();
    let _ = lease.request_stop_once(deadline).await;
    let observation = lease.observe_stop(deadline).await?;
    observation.validate_for(lease.binding())?;
    // Readback is authoritative after a one-shot Stop whose acknowledgement
    // was lost. The exact binding check above still rejects foreign evidence.
    Ok(())
}

fn signal_bound(bound: &std::sync::Mutex<Option<oneshot::Sender<Result<(), AgentFailure>>>>) {
    if let Some(sender) = bound.lock().unwrap_or_else(|p| p.into_inner()).take() {
        let _ = sender.send(Ok(()));
    }
}

pub(crate) fn signal_start_failure(
    bound: &std::sync::Mutex<Option<oneshot::Sender<Result<(), AgentFailure>>>>,
    failure: AgentFailure,
) {
    if let Some(sender) = bound.lock().unwrap_or_else(|p| p.into_inner()).take() {
        let _ = sender.send(Err(failure));
    }
}

fn native_binding(binding: &VoiceBinding) -> LiveMediaBinding {
    LiveMediaBinding {
        actor: binding.actor.clone(),
        voice_session_id: binding.voice_session_id,
        media_lease_id: binding.media_lease_id,
        plan_id: binding.plan_id,
    }
}

fn validate_binding(
    record: &floe_voice::VoiceCallRecord,
    binding: &VoiceBinding,
) -> Result<(), AgentFailure> {
    if record.intent.binding != *binding {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn voice_command_matches(binding: &VoiceBinding, command: &MediaCommand) -> bool {
    &command.binding == binding
}

fn deterministic_command_id(voice: Uuid, domain: &[u8]) -> Uuid {
    Uuid::new_v5(&voice, domain)
}

async fn prepare_apple_lease(
    binding: LiveMediaBinding,
    host_instance_id: Uuid,
) -> Result<Arc<dyn LiveMediaLease>, AgentFailure> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        tokio::task::spawn_blocking(move || {
            floe_native::live_media::AppleLiveMediaLease::prepare_muted(binding, host_instance_id)
                .map(|lease| lease as Arc<dyn LiveMediaLease>)
        })
        .await
        .map_err(|_| AgentFailure::Interrupted)?
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        let _ = (binding, host_instance_id);
        Err(AgentFailure::CapabilityUnavailable)
    }
}
