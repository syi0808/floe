//! App-process live media retirement. Voice remains the durable lifecycle
//! owner; this module only coordinates exact native bindings before Vault seal.
use floe_kernel::{AgentFailure, OwnerActor};
use floe_native::live_media::{LiveMediaBinding, ProcessLiveMediaRegistry};
use floe_vault::{EncryptedAgentVault, VaultKeyProvider};
use std::sync::Arc;
use tokio::time::Instant;
use uuid::Uuid;

/// Fence the retiring Voice generation and all process-held bindings. Fencing
/// happens synchronously at App admission close; physical Stop follows in the
/// generation drain and is not inferred from this fence.
pub(crate) fn fence_retiring_media(
    registry: &ProcessLiveMediaRegistry,
    retiring: Option<&OwnerActor>,
) -> Result<(), AgentFailure> {
    let mut actors = Vec::new();
    if let Some(actor) = retiring {
        actors.push(actor.clone());
    }
    for binding in registry.registered_bindings() {
        if !actors.iter().any(|actor| actor == &binding.actor) {
            actors.push(binding.actor);
        }
    }
    let mut failure = None;
    for actor in actors {
        if let Err(error) = registry.fence_generation(&actor) {
            failure.get_or_insert(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

/// Stop every lease retained by this App process, then acknowledge it only
/// after Voice durably records the exact Stop NativeReceipt. Stop tasks are
/// launched before any journal I/O, so slow storage cannot postpone privacy
/// teardown. A failure leaves the registry receipt retained for re-observation.
pub(crate) async fn drain_registered_media<Keys: VaultKeyProvider + 'static>(
    vault: Arc<EncryptedAgentVault<Keys>>,
    registry: Arc<ProcessLiveMediaRegistry>,
    actor: &OwnerActor,
    deadline: Instant,
) -> Result<(), AgentFailure> {
    let bindings = registry.registered_bindings();
    let mut stops = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let registry = registry.clone();
        stops.push((
            binding.clone(),
            tokio::spawn(async move { registry.stop_and_confirm(&binding, deadline).await }),
        ));
    }

    let mut failure = None;
    for (binding, stop) in stops {
        let stop_command = durable_stop_intent(vault.as_ref(), actor, &binding).await;
        let native_receipt = match stop.await {
            Ok(Ok(receipt)) => Some(receipt),
            Ok(Err(error)) => {
                failure.get_or_insert(error);
                None
            }
            Err(_) => {
                failure.get_or_insert(AgentFailure::Interrupted);
                None
            }
        };
        let stop_command = match stop_command {
            Ok(command) => command,
            Err(error) => {
                failure.get_or_insert(error);
                None
            }
        };
        let Some(native_receipt) = native_receipt else {
            continue;
        };
        if native_receipt.binding() != &binding {
            failure.get_or_insert(AgentFailure::PolicyDenied);
            continue;
        }
        match stop_command {
            Some(stop_command) => {
                if let Err(error) =
                    persist_native_receipt(vault.as_ref(), actor, &binding, &stop_command).await
                {
                    failure.get_or_insert(error);
                    continue;
                }
            }
            None => {
                if let Err(error) = verify_durable_stop(vault.as_ref(), actor, &binding).await {
                    failure.get_or_insert(error);
                    continue;
                }
            }
        }
        if let Err(error) = registry.acknowledge_stop(&native_receipt) {
            failure.get_or_insert(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

async fn verify_durable_stop<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    actor: &OwnerActor,
    native: &LiveMediaBinding,
) -> Result<(), AgentFailure> {
    let record = floe_voice::VoiceJournal::read_call(vault, actor, native.voice_session_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    validate_voice_binding(&record, native)?;
    if record.session.local_lease_released() {
        Ok(())
    } else {
        Err(AgentFailure::Conflict)
    }
}

async fn durable_stop_intent<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    actor: &OwnerActor,
    native: &LiveMediaBinding,
) -> Result<Option<floe_voice::MediaCommand>, AgentFailure> {
    let record = floe_voice::VoiceJournal::read_call(vault, actor, native.voice_session_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    validate_voice_binding(&record, native)?;
    if record.session.local_lease_released() {
        return Ok(None);
    }
    if let Some(command) = record.session.pending_media_command() {
        if command.kind == floe_voice::MediaCommandKind::Stop
            && native_matches_voice(native, command)
        {
            return Ok(Some(command.clone()));
        }
    }
    if record.session.stop_requested() {
        return Err(AgentFailure::Conflict);
    }

    let request = floe_voice::VoiceMutation {
        voice_session_id: native.voice_session_id,
        command_id: command_id(native, b"floe.voice.process-stop-intent.v1\0"),
        expected_revision: record.revision,
        kind: floe_voice::VoiceMutationKind::Stop {},
    };
    match floe_voice::VoiceJournal::mutate(vault, actor, request).await {
        Ok(admission) => stop_command_from(&admission.record, native),
        Err(failure) => {
            // Resolve a lost journal acknowledgement by reading the canonical
            // record. Never infer the Stop command from the attempted request.
            let observed =
                floe_voice::VoiceJournal::read_call(vault, actor, native.voice_session_id).await?;
            let observed = observed.ok_or(AgentFailure::NotFound)?;
            validate_voice_binding(&observed, native)?;
            if observed.session.local_lease_released() {
                Ok(None)
            } else if observed.session.stop_requested() {
                stop_command_from(&observed, native)
            } else {
                Err(failure.into_failure())
            }
        }
    }
}

fn stop_command_from(
    record: &floe_voice::VoiceCallRecord,
    native: &LiveMediaBinding,
) -> Result<Option<floe_voice::MediaCommand>, AgentFailure> {
    let Some(command) = record.session.pending_media_command() else {
        return if record.session.local_lease_released() {
            Ok(None)
        } else {
            Err(AgentFailure::Conflict)
        };
    };
    if command.kind != floe_voice::MediaCommandKind::Stop || !native_matches_voice(native, command)
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(Some(command.clone()))
}

async fn persist_native_receipt<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    actor: &OwnerActor,
    native: &LiveMediaBinding,
    command: &floe_voice::MediaCommand,
) -> Result<(), AgentFailure> {
    let record = floe_voice::VoiceJournal::read_call(vault, actor, native.voice_session_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    validate_voice_binding(&record, native)?;
    if record.session.local_lease_released() {
        return Ok(());
    }
    if record.session.pending_media_command() != Some(command) {
        return Err(AgentFailure::Conflict);
    }
    let request = floe_voice::VoiceMutation {
        voice_session_id: native.voice_session_id,
        command_id: command_id(native, b"floe.voice.process-stop-native-receipt.v1\0"),
        expected_revision: record.revision,
        kind: floe_voice::VoiceMutationKind::NativeReceipt {
            command: command.clone(),
            state: floe_voice::MediaState::Stopped,
        },
    };
    let mutation_failure = match floe_voice::VoiceJournal::mutate(vault, actor, request).await {
        Ok(_) => return Ok(()),
        Err(error) => error.into_failure(),
    };
    // The owner may have committed the exact receipt and lost its response.
    // Only a canonical reread showing released local media resolves that case.
    let observed = floe_voice::VoiceJournal::read_call(vault, actor, native.voice_session_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    validate_voice_binding(&observed, native)?;
    if observed.session.local_lease_released() {
        Ok(())
    } else {
        Err(mutation_failure)
    }
}

fn validate_voice_binding(
    record: &floe_voice::VoiceCallRecord,
    native: &LiveMediaBinding,
) -> Result<(), AgentFailure> {
    let voice = &record.intent.binding;
    if voice.actor != native.actor
        || voice.voice_session_id != native.voice_session_id
        || voice.media_lease_id != native.media_lease_id
        || voice.plan_id != native.plan_id
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn native_matches_voice(native: &LiveMediaBinding, command: &floe_voice::MediaCommand) -> bool {
    command.binding.actor == native.actor
        && command.binding.voice_session_id == native.voice_session_id
        && command.binding.media_lease_id == native.media_lease_id
        && command.binding.plan_id == native.plan_id
}

fn command_id(native: &LiveMediaBinding, domain: &[u8]) -> Uuid {
    let mut name = domain.to_vec();
    name.extend_from_slice(native.media_lease_id.as_bytes());
    Uuid::new_v5(&native.voice_session_id, &name)
}
