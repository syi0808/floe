//! Synthetic lifecycle through the real encrypted owner journal. No microphone,
//! provider connection, account or paid model is used by these tests.
use super::*;
use crate::{RootKey, VaultKeyProvider};
use floe_context_contract::{
    DataClass, DependencyCoverage, LiveContextProjection, LiveProjectionSource,
};
use floe_conversation::{SessionStartAdmission, StartSessionRequest, SurfaceRepository};
use floe_kernel::{CommandId, PersonId};
use floe_model_contract::{
    LiveContextPayload, LiveEndEvidence, LiveModelCapabilities, LiveModelPlan, LiveTransportLimits,
    ModelBindingDigest, ModelSelectionCommitment, ProcessingBoundary,
};
use floe_voice::{MediaState, RemoteState, VoiceBinding, VoiceMutationKind};
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default)]
struct Keys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);
impl VaultKeyProvider for Keys {
    fn load(&self, p: PersonId, v: Uuid) -> Result<RootKey, AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .get(&(p, v))
            .copied()
            .map(RootKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }
    fn insert(&self, p: PersonId, v: Uuid, k: &RootKey) -> Result<(), AgentFailure> {
        self.0.lock().unwrap().insert((p, v), *k.as_bytes());
        Ok(())
    }
}
struct Fixture {
    root: PathBuf,
    keys: Keys,
    actor: OwnerActor,
    session: Uuid,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
async fn fixture() -> (Fixture, EncryptedAgentVault<Keys>) {
    let root = std::env::temp_dir().join(format!("floe-voice-journal-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let keys = Keys::default();
    let actor = OwnerActor {
        person_id: PersonId::new(),
        device_id: "fixture-device".into(),
        runtime_epoch: 1,
    };
    let vault = EncryptedAgentVault::create(&root, actor.person_id, keys.clone())
        .await
        .unwrap();
    let SessionStartAdmission::Started(receipt) = vault
        .start_conversation_session(StartSessionRequest {
            principal: actor.person_id.to_string(),
            command_id: CommandId::new(),
        })
        .await
        .unwrap()
    else {
        panic!("session")
    };
    (
        Fixture {
            root,
            keys,
            actor,
            session: receipt.session_id,
        },
        vault,
    )
}
async fn intent(f: &Fixture, vault: &EncryptedAgentVault<Keys>) -> VoiceStartIntent {
    intent_for_actor(f, vault, &f.actor).await
}
async fn intent_for_actor(
    f: &Fixture,
    vault: &EncryptedAgentVault<Keys>,
    actor: &OwnerActor,
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
    let binding = VoiceBinding {
        actor: actor.clone(),
        surface_session_id: f.session,
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
            through: vault.surface_head(actor, f.session).await.unwrap(),
        },
        coverage: DependencyCoverage::Independent,
        memories: vec![],
        input_data_classes: vec![DataClass::Personal],
        payload: LiveContextPayload::new("Synthetic Context projection, no audio".into()).unwrap(),
    };
    VoiceStartIntent::prepare(
        binding,
        &plan,
        &projection,
        floe_execution::budget::ModelReservationCeiling {
            tokens: 100,
            cost_micros: 100,
        },
    )
    .unwrap()
}
fn mutation(record: &VoiceCallRecord, kind: VoiceMutationKind) -> VoiceMutation {
    VoiceMutation {
        voice_session_id: record.intent.binding.voice_session_id,
        command_id: Uuid::new_v4(),
        expected_revision: record.revision,
        kind,
    }
}
async fn muted(f: &Fixture, vault: &EncryptedAgentVault<Keys>) -> VoiceCallRecord {
    let start = intent(f, vault).await;
    let record = vault.admit_start(&f.actor, start).await.unwrap().record;
    let command = record.session.pending_media_command().unwrap().clone();
    vault
        .mutate(
            &f.actor,
            mutation(
                &record,
                VoiceMutationKind::NativeReceipt {
                    command,
                    state: MediaState::Muted,
                },
            ),
        )
        .await
        .unwrap()
        .record
}
#[tokio::test]
async fn start_and_dispatch_replay_are_read_only_and_never_fresh_twice() {
    let (f, vault) = fixture().await;
    let record = muted(&f, &vault).await;
    let request = mutation(&record, VoiceMutationKind::BeginRemoteDispatch {});
    let admitted = vault.mutate(&f.actor, request.clone()).await.unwrap();
    assert!(admitted.fresh_remote_dispatch());
    let ready = mutation(
        &admitted.record,
        VoiceMutationKind::RemoteReady {
            binding: record.intent.binding.clone(),
        },
    );
    let current = vault.mutate(&f.actor, ready).await.unwrap().record;
    let mut writer = vault.connection().unwrap();
    let held = writer
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    let replay = vault.mutate(&f.actor, request.clone()).await.unwrap();
    assert!(replay.replayed && !replay.fresh_remote_dispatch());
    assert_eq!(replay.receipt, admitted.receipt);
    assert_eq!(replay.record.revision, current.revision);
    let start = vault
        .admit_start(&f.actor, record.intent.clone())
        .await
        .unwrap();
    assert!(start.replayed);
    assert_eq!(start.admission_revision(), 1);
    assert_eq!(start.record.revision, current.revision);
    let mut changed = request;
    changed.kind = VoiceMutationKind::Stop {};
    assert!(matches!(
        vault.mutate(&f.actor, changed).await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    ));
    held.rollback().await.unwrap();
    drop(writer);
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        reopened
            .read_call(&f.actor, record.intent.binding.voice_session_id)
            .await
            .unwrap()
            .unwrap()
            .digest()
            .unwrap(),
        current.digest().unwrap()
    );
}
#[tokio::test]
async fn one_local_lease_requires_native_stop_but_remote_unknown_survives() {
    let (f, vault) = fixture().await;
    let record = muted(&f, &vault).await;
    let other = intent(&f, &vault).await;
    assert!(matches!(
        vault.admit_start(&f.actor, other.clone()).await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    ));
    let dispatch = vault
        .mutate(
            &f.actor,
            mutation(&record, VoiceMutationKind::BeginRemoteDispatch {}),
        )
        .await
        .unwrap()
        .record;
    let ended = vault
        .mutate(
            &f.actor,
            mutation(
                &dispatch,
                VoiceMutationKind::RemoteEnd {
                    binding: record.intent.binding.clone(),
                    evidence: LiveEndEvidence::Disconnected,
                },
            ),
        )
        .await
        .unwrap();
    assert_eq!(ended.record.session.remote(), RemoteState::Unknown);
    assert!(!ended.record.session.local_lease_released());
    assert!(vault.admit_start(&f.actor, other.clone()).await.is_err());
    let stopped = vault
        .mutate(
            &f.actor,
            mutation(
                &ended.record,
                VoiceMutationKind::NativeReceipt {
                    command: ended.receipt.media_command.unwrap(),
                    state: MediaState::Stopped,
                },
            ),
        )
        .await
        .unwrap()
        .record;
    assert!(stopped.session.local_lease_released());
    assert_eq!(stopped.session.remote(), RemoteState::Unknown);
    assert_eq!(
        stopped.intent.reservation.cost_micros, 100,
        "unknown remote end cannot manufacture zero cost"
    );
    assert!(!vault.admit_start(&f.actor, other).await.unwrap().replayed);
}
#[tokio::test]
async fn lost_dispatch_ack_reopens_as_replay_without_dispatch_permission() {
    let (f, vault) = fixture().await;
    let record = muted(&f, &vault).await;
    let request = mutation(&record, VoiceMutationKind::BeginRemoteDispatch {});
    vault
        .registry_transaction_ack_loss
        .store(true, Ordering::Release);
    assert!(matches!(
        vault.mutate(&f.actor, request.clone()).await,
        Err(CommandFailure::Indeterminate(
            AgentFailure::StorageUnavailable
        ))
    ));
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    let replay = reopened.mutate(&f.actor, request.clone()).await.unwrap();
    assert!(!replay.fresh_remote_dispatch());
    assert!(replay.replayed);
    assert_eq!(replay.record.session.remote(), RemoteState::Dispatching);
    let mut next_epoch = f.actor.clone();
    next_epoch.runtime_epoch += 1;
    assert!(matches!(
        reopened.mutate(&next_epoch, request).await,
        Err(CommandFailure::Indeterminate(AgentFailure::PolicyDenied))
    ));
}
#[tokio::test]
async fn state_and_receipt_rollback_together_and_missing_history_fails_reopen() {
    let (f, vault) = fixture().await;
    let record = muted(&f, &vault).await;
    let request = mutation(&record, VoiceMutationKind::BeginRemoteDispatch {});
    vault
        .voice_fault_after_state_write
        .store(true, Ordering::Release);
    assert!(vault.mutate(&f.actor, request.clone()).await.is_err());
    assert_eq!(
        vault
            .read_call(&f.actor, record.intent.binding.voice_session_id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        record.revision
    );
    let admitted = vault.mutate(&f.actor, request).await.unwrap();
    assert!(admitted.fresh_remote_dispatch());
    let c = vault.connection().unwrap();
    c.execute(
        "DELETE FROM voice_session_commands WHERE voice_session_id = ? AND revision = 2",
        [record.intent.binding.voice_session_id.to_string()],
    )
    .await
    .unwrap();
    drop(c);
    drop(vault);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn foreign_scope_changed_start_and_wrong_native_lease_never_mutate_custody() {
    let (f, vault) = fixture().await;
    let original = intent(&f, &vault).await;
    let record = vault
        .admit_start(&f.actor, original.clone())
        .await
        .unwrap()
        .record;
    let mut changed = original;
    changed.context_digest = [8; 32];
    assert!(matches!(
        vault.admit_start(&f.actor, changed).await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    ));
    let mut person = f.actor.clone();
    person.person_id = PersonId::new();
    assert!(matches!(
        vault
            .read_call(&person, record.intent.binding.voice_session_id)
            .await,
        Err(AgentFailure::PolicyDenied)
    ));
    let mut device = f.actor.clone();
    device.device_id = "different-device".into();
    assert!(matches!(
        vault
            .read_call(&device, record.intent.binding.voice_session_id)
            .await,
        Err(AgentFailure::PolicyDenied)
    ));
    let mut command = record.session.pending_media_command().unwrap().clone();
    command.binding.media_lease_id = Uuid::new_v4();
    assert!(
        vault
            .mutate(
                &f.actor,
                mutation(
                    &record,
                    VoiceMutationKind::NativeReceipt {
                        command,
                        state: MediaState::Muted
                    }
                )
            )
            .await
            .is_err()
    );
    assert_eq!(
        vault
            .read_call(&f.actor, record.intent.binding.voice_session_id)
            .await
            .unwrap()
            .unwrap()
            .digest()
            .unwrap(),
        record.digest().unwrap()
    );
}
#[tokio::test]
async fn fresh_start_and_control_busy_leave_exact_retry_and_no_fake_receipt() {
    let (f, vault) = fixture().await;
    let start = intent(&f, &vault).await;
    let mut c = vault.connection().unwrap();
    let held = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert!(matches!(
        vault.admit_start(&f.actor, start.clone()).await,
        Err(CommandFailure::Indeterminate(AgentFailure::StorageBusy))
    ));
    assert!(
        vault
            .read_call(&f.actor, start.binding.voice_session_id)
            .await
            .unwrap()
            .is_none()
    );
    held.rollback().await.unwrap();
    let record = vault.admit_start(&f.actor, start).await.unwrap().record;
    let request = mutation(
        &record,
        VoiceMutationKind::NativeReceipt {
            command: record.session.pending_media_command().unwrap().clone(),
            state: MediaState::Muted,
        },
    );
    let held = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert!(matches!(
        vault.mutate(&f.actor, request.clone()).await,
        Err(CommandFailure::Indeterminate(AgentFailure::StorageBusy))
    ));
    assert_eq!(
        vault
            .read_call(&f.actor, record.intent.binding.voice_session_id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    held.rollback().await.unwrap();
    let admitted = vault.mutate(&f.actor, request).await.unwrap();
    assert!(!admitted.replayed);
    assert_eq!(admitted.record.revision, 2);
}

#[tokio::test]
async fn command_history_beyond_one_page_reopens_without_a_lifetime_cap() {
    let (f, vault) = fixture().await;
    let mut record = muted(&f, &vault).await;
    for _ in 0..130 {
        record = vault
            .mutate(&f.actor, mutation(&record, VoiceMutationKind::Mute {}))
            .await
            .unwrap()
            .record;
    }
    assert_eq!(record.revision, 132);
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        reopened
            .read_call(&f.actor, record.intent.binding.voice_session_id)
            .await
            .unwrap()
            .unwrap()
            .digest()
            .unwrap(),
        record.digest().unwrap()
    );
}
#[tokio::test]
async fn session_history_pages_validate_the_last_session_and_preserve_all_prior_calls() {
    let (f, vault) = fixture().await;
    let mut ids = Vec::new();
    for _ in 0..130 {
        let record = vault
            .admit_start(&f.actor, intent(&f, &vault).await)
            .await
            .unwrap()
            .record;
        ids.push(record.intent.binding.voice_session_id);
        let stopped = vault
            .mutate(&f.actor, mutation(&record, VoiceMutationKind::Stop {}))
            .await
            .unwrap();
        vault
            .mutate(
                &f.actor,
                mutation(
                    &stopped.record,
                    VoiceMutationKind::NativeReceipt {
                        command: stopped.receipt.media_command.unwrap(),
                        state: MediaState::Stopped,
                    },
                ),
            )
            .await
            .unwrap();
    }
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    let last = *ids.iter().max().unwrap();
    assert!(
        reopened
            .read_call(&f.actor, last)
            .await
            .unwrap()
            .unwrap()
            .session
            .local_lease_released()
    );
    let c = reopened.connection().unwrap();
    let mut rows = c
        .query("SELECT COUNT(*) FROM voice_sessions", ())
        .await
        .unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        130
    );
    drop(rows);
    c.execute(
        "DELETE FROM voice_session_commands WHERE voice_session_id = ? AND revision = 2",
        [last.to_string()],
    )
    .await
    .unwrap();
    drop(c);
    drop(reopened);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn orphan_command_custody_is_corruption_not_an_absent_new_call() {
    let (f, vault) = fixture().await;
    let missing = Uuid::new_v4();
    let c = vault.connection().unwrap();
    c.execute("PRAGMA foreign_keys = OFF", ()).await.unwrap();
    c.execute("INSERT INTO voice_session_commands (voice_session_id, command_id, request_digest, revision, payload) VALUES (?, ?, ?, 2, '{}')",(missing.to_string(),Uuid::new_v4().to_string(),"a".repeat(64))).await.unwrap();
    assert!(matches!(
        vault.read_call(&f.actor, missing).await,
        Err(AgentFailure::StorageUnavailable)
    ));
    drop(c);
    drop(vault);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn older_layout_without_voice_family_is_rejected_without_implicit_creation_or_key_reset() {
    let (f, vault) = fixture().await;
    let c = vault.connection().unwrap();
    for statement in [
        "DROP INDEX voice_input_lookup",
        "DROP TABLE voice_events",
        "DROP TABLE voice_session_commands",
        "DROP INDEX voice_one_local_media_lease",
        "DROP TABLE voice_sessions",
        "DROP TABLE voice_session_schema",
    ] {
        c.execute(statement, ()).await.unwrap();
    }
    let keys_before = f.keys.0.lock().unwrap().clone();
    drop(vault);
    assert!(matches!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone()).await,
        Err(AgentFailure::UnsupportedVersion)
    ));
    assert_eq!(*f.keys.0.lock().unwrap(), keys_before);
    let mut rows = c
        .query(
            "SELECT name FROM sqlite_schema WHERE name = 'voice_session_schema'",
            (),
        )
        .await
        .unwrap();
    assert!(
        rows.next().await.unwrap().is_none(),
        "open must not implicitly create the new family"
    );
}

async fn capture_from(
    f: &Fixture,
    vault: &EncryptedAgentVault<Keys>,
    record: VoiceCallRecord,
) -> VoiceCallRecord {
    let dispatch = vault
        .mutate(
            &f.actor,
            mutation(&record, VoiceMutationKind::BeginRemoteDispatch {}),
        )
        .await
        .unwrap()
        .record;
    let ready = vault
        .mutate(
            &f.actor,
            mutation(
                &dispatch,
                VoiceMutationKind::RemoteReady {
                    binding: record.intent.binding.clone(),
                },
            ),
        )
        .await
        .unwrap()
        .record;
    let unmute = vault
        .mutate(&f.actor, mutation(&ready, VoiceMutationKind::Unmute {}))
        .await
        .unwrap();
    vault
        .mutate(
            &f.actor,
            mutation(
                &unmute.record,
                VoiceMutationKind::NativeReceipt {
                    command: unmute.receipt.media_command.unwrap(),
                    state: MediaState::Capturing,
                },
            ),
        )
        .await
        .unwrap()
        .record
}
async fn capturing(f: &Fixture, vault: &EncryptedAgentVault<Keys>) -> VoiceCallRecord {
    capture_from(f, vault, muted(f, vault).await).await
}
fn utterance(id: &str, text: &str) -> floe_model_contract::LiveProviderEvent {
    floe_model_contract::LiveProviderEvent::PersonUtteranceFinal {
        utterance_id: id.into(),
        text: text.into(),
    }
}
fn delegation(
    id: &str,
    input: Option<&str>,
    hint: Option<&str>,
) -> floe_model_contract::LiveProviderEvent {
    floe_model_contract::LiveProviderEvent::DelegationRequested {
        delegation_id: id.into(),
        utterance_ref: input.map(str::to_owned),
        input_hint: hint.map(str::to_owned),
    }
}

#[tokio::test]
async fn ingress_exact_input_and_delegation_aliases_keep_one_original_work() {
    use floe_voice::VoiceIngressJournal;
    let (f, vault) = fixture().await;
    let call = capturing(&f, &vault).await;
    let id = call.intent.binding.voice_session_id;
    let event = utterance("u1", "  Original synthetic task  ");
    let first = vault.ingest(&f.actor, id, event.clone()).await.unwrap();
    let input = first.input.unwrap();
    assert_eq!(input.input.text, "Original synthetic task");
    let repeat = vault.ingest(&f.actor, id, event.clone()).await.unwrap();
    assert!(repeat.replayed);
    assert_eq!(repeat.input, Some(input.clone()));
    assert!(matches!(
        vault
            .ingest(&f.actor, id, utterance("u1", "changed body"))
            .await,
        Err(CommandFailure::Indeterminate(AgentFailure::Conflict))
    ));
    let work = vault
        .ingest(
            &f.actor,
            id,
            delegation("d1", Some("u1"), Some("untrusted replacement text")),
        )
        .await
        .unwrap()
        .work
        .unwrap();
    assert_eq!(work.input_id, input.input.message_id);
    assert_eq!(work.anchor, input.through);
    let alias = vault
        .ingest(&f.actor, id, delegation("d2", Some("u1"), None))
        .await
        .unwrap();
    assert_eq!(alias.work, Some(work.clone()));
    assert_eq!(
        vault
            .pending_surface_work(&f.actor, f.session)
            .await
            .unwrap()
            .len(),
        1
    );
    let mut c = vault.connection().unwrap();
    let held = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    assert!(vault.ingest(&f.actor, id, event).await.unwrap().replayed);
    assert!(matches!(
        vault
            .ingest(&f.actor, id, utterance("u2", "Later input"))
            .await,
        Err(CommandFailure::Indeterminate(AgentFailure::StorageBusy))
    ));
    held.rollback().await.unwrap();
    drop(c);
    vault
        .ingest(&f.actor, id, utterance("u2", "Later input"))
        .await
        .unwrap();
    assert_eq!(
        vault
            .surface_head(&f.actor, f.session)
            .await
            .unwrap()
            .sequence,
        2
    );
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        reopened
            .read_event(&f.actor, id, "delegation", "d1")
            .await
            .unwrap()
            .unwrap()
            .work,
        Some(work)
    );
}
#[tokio::test]
async fn ingress_cross_owner_rollback_and_lost_work_ack_never_duplicate_custody() {
    use floe_voice::VoiceIngressJournal;
    let (f, vault) = fixture().await;
    let call = capturing(&f, &vault).await;
    let id = call.intent.binding.voice_session_id;
    vault
        .voice_fault_after_state_write
        .store(true, Ordering::Release);
    assert!(
        vault
            .ingest(&f.actor, id, utterance("u1", "Work once"))
            .await
            .is_err()
    );
    assert_eq!(
        vault
            .surface_head(&f.actor, f.session)
            .await
            .unwrap()
            .sequence,
        0
    );
    assert!(
        vault
            .read_event(&f.actor, id, "person_final", "u1")
            .await
            .unwrap()
            .is_none()
    );
    vault
        .ingest(&f.actor, id, utterance("u1", "Work once"))
        .await
        .unwrap();
    let request = delegation("d1", Some("u1"), None);
    vault
        .registry_transaction_ack_loss
        .store(true, Ordering::Release);
    assert!(matches!(
        vault.ingest(&f.actor, id, request.clone()).await,
        Err(CommandFailure::Indeterminate(
            AgentFailure::StorageUnavailable
        ))
    ));
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    let receipt = reopened.ingest(&f.actor, id, request).await.unwrap();
    assert!(receipt.replayed);
    assert!(receipt.work.is_some());
    assert_eq!(
        reopened
            .pending_surface_work(&f.actor, f.session)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        reopened
            .surface_head(&f.actor, f.session)
            .await
            .unwrap()
            .sequence,
        1
    );
}
#[tokio::test]
async fn ingress_missing_and_ambiguous_hints_never_invent_or_rebind_user_input() {
    use floe_voice::{VoiceEventOutcome, VoiceIngressJournal, VoiceUnroutedReason};
    let (f, vault) = fixture().await;
    let call = capturing(&f, &vault).await;
    let id = call.intent.binding.voice_session_id;
    let missing = delegation("missing", None, Some("Same words"));
    let no_input = vault.ingest(&f.actor, id, missing.clone()).await.unwrap();
    assert!(matches!(
        no_input.record.outcome,
        VoiceEventOutcome::Unrouted {
            reason: VoiceUnroutedReason::MissingInput,
            ..
        }
    ));
    assert_eq!(
        vault
            .surface_head(&f.actor, f.session)
            .await
            .unwrap()
            .sequence,
        0
    );
    vault
        .ingest(&f.actor, id, utterance("u1", "Same words"))
        .await
        .unwrap();
    assert!(
        vault
            .ingest(&f.actor, id, missing)
            .await
            .unwrap()
            .work
            .is_none(),
        "later speech cannot rewrite an earlier missing-input decision"
    );
    let first = vault
        .ingest(&f.actor, id, delegation("unique", None, Some("Same words")))
        .await
        .unwrap()
        .work
        .unwrap();
    vault
        .ingest(&f.actor, id, utterance("u2", "Same words"))
        .await
        .unwrap();
    let ambiguous = vault
        .ingest(
            &f.actor,
            id,
            delegation("ambiguous", None, Some("Same words")),
        )
        .await
        .unwrap();
    assert!(matches!(
        ambiguous.record.outcome,
        VoiceEventOutcome::Unrouted {
            reason: VoiceUnroutedReason::AmbiguousInput,
            ..
        }
    ));
    let second = vault
        .ingest(&f.actor, id, delegation("explicit", Some("u2"), None))
        .await
        .unwrap()
        .work
        .unwrap();
    assert_ne!(first.work_id, second.work_id);
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        reopened
            .pending_surface_work(&f.actor, f.session)
            .await
            .unwrap()
            .len(),
        2
    );
}
#[tokio::test]
async fn ingress_stop_drains_input_but_preserves_only_already_admitted_work() {
    use floe_voice::{VoiceEventOutcome, VoiceIngressJournal, VoiceUnroutedReason};
    let (f, vault) = fixture().await;
    let call = capturing(&f, &vault).await;
    let id = call.intent.binding.voice_session_id;
    vault
        .ingest(&f.actor, id, utterance("u1", "Admitted before hangup"))
        .await
        .unwrap();
    let work = vault
        .ingest(&f.actor, id, delegation("d1", Some("u1"), None))
        .await
        .unwrap()
        .work
        .unwrap();
    vault
        .ingest(&f.actor, id, utterance("u2", "Not yet delegated"))
        .await
        .unwrap();
    let stop = vault
        .mutate(&f.actor, mutation(&call, VoiceMutationKind::Stop {}))
        .await
        .unwrap();
    vault
        .ingest(
            &f.actor,
            id,
            utterance("u3", "Already received final input drains safely"),
        )
        .await
        .unwrap();
    assert_eq!(
        vault
            .ingest(&f.actor, id, delegation("alias", Some("u1"), None))
            .await
            .unwrap()
            .work,
        Some(work)
    );
    let refused = vault
        .ingest(&f.actor, id, delegation("d2", Some("u2"), None))
        .await
        .unwrap();
    assert!(matches!(
        refused.record.outcome,
        VoiceEventOutcome::Unrouted {
            reason: VoiceUnroutedReason::SessionStopped,
            ..
        }
    ));
    let filler = floe_model_contract::LiveProviderEvent::AssistantUtteranceFinal {
        utterance_id: "a1".into(),
        text: "Synthetic filler, not a Manager answer".into(),
    };
    let result = vault.ingest(&f.actor, id, filler).await.unwrap();
    assert!(matches!(
        result.record.outcome,
        VoiceEventOutcome::PresentationRetained {}
    ));
    assert_eq!(
        vault
            .surface_head(&f.actor, f.session)
            .await
            .unwrap()
            .sequence,
        3
    );
    let closed = vault
        .mutate(
            &f.actor,
            mutation(&stop.record, VoiceMutationKind::CloseIngress {}),
        )
        .await
        .unwrap()
        .record;
    assert!(closed.ingress_closed);
    assert!(
        vault
            .ingest(&f.actor, id, utterance("late", "Too late"))
            .await
            .is_err()
    );
    assert!(
        vault
            .ingest(
                &f.actor,
                id,
                utterance("u3", "Already received final input drains safely")
            )
            .await
            .unwrap()
            .replayed
    );
    assert_eq!(
        vault
            .pending_surface_work(&f.actor, f.session)
            .await
            .unwrap()
            .len(),
        1
    );
    drop(vault);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn ingress_requires_capture_and_retains_rejected_text_without_truncation_or_fake_turns() {
    use floe_voice::{VoiceEventOutcome, VoiceIngressJournal, VoiceInputRejection};
    let (f, vault) = fixture().await;
    let muted = muted(&f, &vault).await;
    let id = muted.intent.binding.voice_session_id;
    assert!(
        vault
            .ingest(&f.actor, id, utterance("early", "No confirmed capture yet"))
            .await
            .is_err()
    );
    capture_from(&f, &vault, muted).await;
    let delta = floe_model_contract::LiveProviderEvent::TranscriptDelta {
        speaker: floe_model_contract::LiveSpeaker::Person,
        utterance_id: Some("u1".into()),
        text: "partial caption".into(),
    };
    assert!(vault.ingest(&f.actor, id, delta).await.is_err());
    let long = utterance(
        "long",
        &"x".repeat(floe_conversation::MAX_TURN_TEXT_BYTES + 1),
    );
    let rejected = vault.ingest(&f.actor, id, long.clone()).await.unwrap();
    assert_eq!(rejected.record.event, long);
    assert!(matches!(
        rejected.record.outcome,
        VoiceEventOutcome::RejectedInput {
            reason: VoiceInputRejection::TooLarge
        }
    ));
    let rejected = vault
        .ingest(&f.actor, id, utterance("invalid", "bad\0text"))
        .await
        .unwrap();
    assert!(matches!(
        rejected.record.outcome,
        VoiceEventOutcome::RejectedInput {
            reason: VoiceInputRejection::InvalidText
        }
    ));
    assert_eq!(
        vault
            .surface_head(&f.actor, f.session)
            .await
            .unwrap()
            .sequence,
        0
    );
    drop(vault);
    assert!(
        EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn ingress_capacity_and_history_beyond_one_page_preserve_original_decisions() {
    use floe_voice::{VoiceEventOutcome, VoiceIngressJournal, VoiceUnroutedReason};
    let (f, vault) = fixture().await;
    let call = capturing(&f, &vault).await;
    let id = call.intent.binding.voice_session_id;
    let mut first = None;
    for n in 0..64 {
        let utterance_id = format!("u{n}");
        vault
            .ingest(
                &f.actor,
                id,
                utterance(&utterance_id, &format!("Synthetic work {n}")),
            )
            .await
            .unwrap();
        let work = vault
            .ingest(
                &f.actor,
                id,
                delegation(&format!("d{n}"), Some(&utterance_id), None),
            )
            .await
            .unwrap()
            .work
            .unwrap();
        if first.is_none() {
            first = Some(work);
        }
    }
    vault
        .ingest(
            &f.actor,
            id,
            utterance("extra", "Retained even while the work queue is full"),
        )
        .await
        .unwrap();
    let request = delegation("full", Some("extra"), None);
    let full = vault.ingest(&f.actor, id, request.clone()).await.unwrap();
    assert!(matches!(
        full.record.outcome,
        VoiceEventOutcome::Unrouted {
            reason: VoiceUnroutedReason::QueueFull,
            ..
        }
    ));
    vault
        .cancel_pending_work(
            &f.actor,
            floe_conversation::CancelPendingWork {
                command_id: CommandId::new(),
                work_command_id: first.unwrap().command_id,
            },
        )
        .await
        .unwrap();
    assert!(
        vault
            .ingest(&f.actor, id, request)
            .await
            .unwrap()
            .work
            .is_none()
    );
    assert!(
        vault
            .ingest(&f.actor, id, delegation("retry", Some("extra"), None))
            .await
            .unwrap()
            .work
            .is_some()
    );
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert!(matches!(
        reopened
            .read_event(&f.actor, id, "delegation", "full")
            .await
            .unwrap()
            .unwrap()
            .record
            .outcome,
        VoiceEventOutcome::Unrouted {
            reason: VoiceUnroutedReason::QueueFull,
            ..
        }
    ));
    assert_eq!(
        reopened
            .pending_surface_work(&f.actor, f.session)
            .await
            .unwrap()
            .len(),
        64
    );
}

#[tokio::test]
async fn new_runtime_can_retire_an_old_lease_but_cannot_revive_its_capture_or_dispatch() {
    let (f, vault) = fixture().await;
    let record = capturing(&f, &vault).await;
    let id = record.intent.binding.voice_session_id;
    let mut current = f.actor.clone();
    current.runtime_epoch += 1;
    for kind in [
        VoiceMutationKind::Unmute {},
        VoiceMutationKind::BeginRemoteDispatch {},
        VoiceMutationKind::RemoteReady {
            binding: record.intent.binding.clone(),
        },
    ] {
        assert!(matches!(
            vault.mutate(&current, mutation(&record, kind)).await,
            Err(CommandFailure::Indeterminate(AgentFailure::PolicyDenied))
        ));
    }
    let stop = vault
        .mutate(&current, mutation(&record, VoiceMutationKind::Stop {}))
        .await
        .unwrap();
    assert!(stop.record.ingress_closed);
    assert_eq!(stop.record.session.remote(), RemoteState::Unknown);
    let stopped = vault
        .mutate(
            &current,
            mutation(
                &stop.record,
                VoiceMutationKind::NativeReceipt {
                    command: stop.receipt.media_command.unwrap(),
                    state: MediaState::Stopped,
                },
            ),
        )
        .await
        .unwrap();
    assert!(stopped.record.session.local_lease_released());
    assert_eq!(stopped.receipt.caller_epoch, current.runtime_epoch);
    let next = intent_for_actor(&f, &vault, &current).await;
    vault.admit_start(&current, next).await.unwrap();
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert!(
        reopened
            .read_call(&current, id)
            .await
            .unwrap()
            .unwrap()
            .session
            .local_lease_released()
    );
}

#[tokio::test]
async fn concurrent_event_observers_share_one_input_and_one_work_receipt() {
    use floe_voice::VoiceIngressJournal;
    let (f, vault) = fixture().await;
    let call = capturing(&f, &vault).await;
    let voice = call.intent.binding.voice_session_id;
    async fn admit(
        vault: &EncryptedAgentVault<Keys>,
        actor: &OwnerActor,
        voice: Uuid,
        event: floe_model_contract::LiveProviderEvent,
    ) -> floe_voice::VoiceIngressAdmission {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                match vault.ingest(actor, voice, event.clone()).await {
                    Ok(value) => return value,
                    Err(
                        CommandFailure::Indeterminate(AgentFailure::StorageBusy)
                        | CommandFailure::Admitted(AgentFailure::StorageBusy)
                        | CommandFailure::NotAdmitted(AgentFailure::StorageBusy),
                    ) => {
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                    Err(other) => panic!("unexpected event admission failure: {other:?}"),
                }
            }
        })
        .await
        .expect("same-key contention remains recoverable")
    }
    let input = utterance("race", "Exactly one input");
    let (a, b) = tokio::join!(
        admit(&vault, &f.actor, voice, input.clone()),
        admit(&vault, &f.actor, voice, input)
    );
    assert_eq!(a.input, b.input);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(
        vault
            .surface_head(&f.actor, f.session)
            .await
            .unwrap()
            .sequence,
        1
    );
    let event = delegation("same", Some("race"), None);
    let (a, b) = tokio::join!(
        admit(&vault, &f.actor, voice, event.clone()),
        admit(&vault, &f.actor, voice, event)
    );
    assert_eq!(a.work, b.work);
    assert_ne!(a.replayed, b.replayed);
    let (c, d) = tokio::join!(
        admit(
            &vault,
            &f.actor,
            voice,
            delegation("alias-a", Some("race"), None)
        ),
        admit(
            &vault,
            &f.actor,
            voice,
            delegation("alias-b", Some("race"), None)
        )
    );
    assert_eq!(c.work, a.work);
    assert_eq!(d.work, a.work);
    assert_eq!(
        vault
            .pending_surface_work(&f.actor, f.session)
            .await
            .unwrap()
            .len(),
        1
    );
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        reopened
            .read_event(&f.actor, voice, "delegation", "same")
            .await
            .unwrap()
            .unwrap()
            .work,
        a.work
    );
}

#[tokio::test]
async fn terminal_live_usage_survives_lost_ack_and_replays_read_only_without_zero_refund() {
    let (f, vault) = fixture().await;
    let call = capturing(&f, &vault).await;
    let usage = call.unresolved_usage();
    assert!(usage.dispatched && usage.accounting.unknown_tokens && usage.accounting.unknown_cost);
    let request = mutation(&call, VoiceMutationKind::SettleUsage { receipt: usage });
    vault
        .registry_transaction_ack_loss
        .store(true, Ordering::Release);
    assert!(vault.mutate(&f.actor, request.clone()).await.is_err());
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    let retained = reopened
        .read_call(&f.actor, call.intent.binding.voice_session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retained.usage, Some(usage));
    let mut connection = reopened.connection().unwrap();
    let writer = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .unwrap();
    let replay = reopened.mutate(&f.actor, request.clone()).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.record.usage, Some(usage));
    writer.rollback().await.unwrap();
    let mut changed = usage;
    changed.charged_tokens -= 1;
    let changed = mutation(
        &retained,
        VoiceMutationKind::SettleUsage { receipt: changed },
    );
    assert!(reopened.mutate(&f.actor, changed).await.is_err());
    assert_eq!(
        reopened
            .read_call(&f.actor, call.intent.binding.voice_session_id)
            .await
            .unwrap()
            .unwrap()
            .usage,
        Some(usage)
    );
}
#[tokio::test]
async fn new_epoch_can_only_settle_conservative_usage_after_retiring_old_ingress() {
    let (f, vault) = fixture().await;
    let call = capturing(&f, &vault).await;
    let actor = OwnerActor {
        runtime_epoch: f.actor.runtime_epoch + 1,
        ..f.actor.clone()
    };
    let usage = call.unresolved_usage();
    let early = mutation(&call, VoiceMutationKind::SettleUsage { receipt: usage });
    assert!(vault.mutate(&actor, early).await.is_err());
    let retired = vault
        .mutate(&actor, mutation(&call, VoiceMutationKind::Stop {}))
        .await
        .unwrap()
        .record;
    let mut fake_zero = usage;
    fake_zero.charged_tokens = 0;
    fake_zero.charged_cost_micros = 0;
    fake_zero.accounting = floe_execution::budget::ModelAccounting {
        observed_tokens: Some(0),
        observed_cost_micros: Some(0),
        unknown_tokens: false,
        unknown_cost: false,
    };
    assert!(
        vault
            .mutate(
                &actor,
                mutation(
                    &retired,
                    VoiceMutationKind::SettleUsage { receipt: fake_zero }
                )
            )
            .await
            .is_err()
    );
    let settled = vault
        .mutate(
            &actor,
            mutation(&retired, VoiceMutationKind::SettleUsage { receipt: usage }),
        )
        .await
        .unwrap()
        .record;
    assert_eq!(settled.usage, Some(usage));
    assert!(!settled.session.local_lease_released());
    drop(vault);
    let reopened = EncryptedAgentVault::open(&f.root, f.actor.person_id, f.keys.clone())
        .await
        .unwrap();
    assert_eq!(
        reopened
            .read_call(&actor, call.intent.binding.voice_session_id)
            .await
            .unwrap()
            .unwrap()
            .usage,
        Some(usage)
    );
}
