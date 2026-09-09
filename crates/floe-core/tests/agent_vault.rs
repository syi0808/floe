#![cfg(unix)]

use std::{
    collections::HashMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use floe_agent::*;
use floe_core::{EncryptedAgentVault, VaultKey, VaultKeyProvider};
use floe_domain::PersonId;
use uuid::Uuid;

#[path = "agent_vault/calendar_sessions.rs"]
mod calendar_sessions;

#[derive(Clone, Default)]
struct Keys(Arc<KeyState>);

#[derive(Default)]
struct KeyState {
    values: Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>,
    blocked: AtomicBool,
    inserts: AtomicUsize,
}

fn private_root() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    directory
}

impl VaultKeyProvider for Keys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
        if self.0.blocked.load(Ordering::SeqCst) {
            return Err(AgentFailure::VaultUnavailable);
        }
        self.0
            .values
            .lock()
            .unwrap()
            .get(&(person, vault))
            .copied()
            .map(VaultKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }

    fn insert(&self, person: PersonId, vault: Uuid, key: &VaultKey) -> Result<(), AgentFailure> {
        self.0.inserts.fetch_add(1, Ordering::SeqCst);
        if self.0.blocked.load(Ordering::SeqCst) {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut values = self.0.values.lock().unwrap();
        if values.contains_key(&(person, vault)) {
            return Err(AgentFailure::VaultUnavailable);
        }
        values.insert((person, vault), *key.as_bytes());
        Ok(())
    }
}

fn assert_no_plaintext(directory: &Path, markers: &[&str]) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = fs::read(path).unwrap();
            for marker in markers {
                assert!(
                    !bytes
                        .windows(marker.len())
                        .any(|window| window == marker.as_bytes()),
                    "Synthetic secret marker leaked to disk"
                );
            }
        }
    }
}

#[tokio::test]
async fn encrypted_messages_and_tool_results_survive_wal_and_checkpoint_reopen() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    assert_eq!(vault.protection(), SessionProtection::Encrypted);
    let mut session = vault.create_session().await.unwrap();
    let turn = Uuid::new_v4();
    let markers = [
        "synthetic-private-user-931ea",
        "synthetic-tool-input-7db83",
        "synthetic-tool-result-236b6",
        "synthetic-answer-111f4",
        "synthetic-provider-replay-c914e",
        "synthetic-expert-private-result-632ba",
    ];
    session.messages = vec![
        AgentMessage::User {
            turn_id: turn,
            text: markers[0].into(),
        },
        AgentMessage::Capability {
            turn_id: turn,
            call_id: Uuid::new_v4(),
            capability_id: "fixture.read.v1".into(),
            input: markers[1].into(),
            result: Ok(markers[2].into()),
        },
        AgentMessage::Assistant {
            turn_id: turn,
            text: markers[3].into(),
        },
    ];
    let AgentMessage::Capability { call_id, .. } = session.messages[1] else {
        panic!("missing synthetic call")
    };
    session.capability_executions.push(CapabilityExecution {
        scope_id: session.id,
        result: Some(Ok(markers[2].into())),
        turn_id: turn,
        call_id,
        capability_id: "fixture.read.v1".into(),
        input: markers[1].into(),
        state: CapabilityExecutionState::Settled,
        replay: Some(ProviderReplay {
            gateway: "http://127.0.0.1:8431".into(),
            purpose: "everyday_assistance".into(),
            external: true,
            source: "a".repeat(64),
            provider_call_id: "provider-original".into(),
            items: serde_json::json!([{"type":"reasoning","encrypted_content":markers[4]}]),
        }),
    });
    let mut child = session.capability_executions[0].clone();
    child.scope_id = Uuid::new_v4();
    child.turn_id = child.scope_id;
    child.call_id = Uuid::new_v4();
    child.capability_id = "schedule.find_free_windows".into();
    child.result = Some(Ok(markers[5].into()));
    session.capability_executions.push(child);
    let attempt_id = Uuid::new_v4();
    session.model_attempts.push(ModelAttemptRecord {
        id: attempt_id,
        turn_id: turn,
        scope_id: session.id,
        attempt: 1,
        placement: ModelPlacement::DeviceLocal,
        state: ModelAttemptState::Accepted,
        failure: None,
        usage: ModelUsage { attempts: 1, tokens: 10, ..ModelUsage::default() },
    });
    session.usage = AgentUsage { model_attempts: 1, tokens: 10, ..AgentUsage::default() };
    session.revision = 1;
    session.last_outcome = Some(AgentOutcome::Completed);
    vault.compare_and_swap(&session, 0).await.unwrap();
    let directory = root.path().join(person.to_string());
    assert!(
        fs::metadata(directory.join("sessions.db-wal"))
            .unwrap()
            .len()
            > 0
    );
    assert_no_plaintext(&directory, &markers);
    assert_no_plaintext(&directory, &[&attempt_id.to_string()]);
    vault.checkpoint().await.unwrap();
    assert!(fs::metadata(directory.join("sessions.db")).unwrap().len() > 4096);
    assert_no_plaintext(&directory, &markers);
    assert_no_plaintext(&directory, &[&attempt_id.to_string()]);
    drop(vault);
    assert!(
        turso::Builder::new_local(directory.join("sessions.db").to_str().unwrap())
            .build()
            .await
            .is_err()
    );
    let vault = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    assert_eq!(vault.load(person, session.id).await.unwrap(), session);
}

#[tokio::test]
async fn vaults_enforce_person_revision_version_and_size_boundaries() {
    let root = private_root();
    let person = PersonId::new();
    let other = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let second = EncryptedAgentVault::create(root.path(), other, keys.clone())
        .await
        .unwrap();
    let original = vault.create_session().await.unwrap();
    assert_eq!(
        vault.load(other, original.id).await,
        Err(AgentFailure::NotFound)
    );
    assert_eq!(
        second.load(other, original.id).await,
        Err(AgentFailure::NotFound)
    );
    let mut next = original.clone();
    next.revision = 1;
    assert_eq!(
        second.compare_and_swap(&next, 0).await,
        Err(AgentFailure::NotFound)
    );
    vault.compare_and_swap(&next, 0).await.unwrap();
    assert_eq!(
        vault.compare_and_swap(&next, 0).await,
        Err(AgentFailure::Conflict)
    );
    next.revision = 3;
    assert_eq!(
        vault.compare_and_swap(&next, 1).await,
        Err(AgentFailure::Conflict)
    );
    next.revision = 2;
    next.schema_version = 99;
    assert_eq!(
        vault.compare_and_swap(&next, 1).await,
        Err(AgentFailure::UnsupportedVersion)
    );
    next.schema_version = AGENT_VERSION;
    next.data_classes = vec![DataClass::Credential];
    assert_eq!(
        vault.compare_and_swap(&next, 1).await,
        Err(AgentFailure::PolicyDenied)
    );
    next.data_classes = vec![DataClass::Personal];
    next.messages.push(AgentMessage::User {
        turn_id: Uuid::new_v4(),
        text: "x".repeat(AgentBudget::default().max_session_bytes),
    });
    assert_eq!(
        vault.compare_and_swap(&next, 1).await,
        Err(AgentFailure::BudgetExceeded)
    );
    assert_eq!(vault.load(person, original.id).await.unwrap().revision, 1);
    let values = keys.0.values.lock().unwrap();
    assert_eq!(values.len(), 2);
    let mut keys = values.values();
    assert!(keys.next() != keys.next());
}

#[tokio::test]
async fn missing_wrong_and_revoked_keys_never_create_replacement_data() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let session = vault.create_session().await.unwrap();
    keys.0.blocked.store(true, Ordering::SeqCst);
    assert_eq!(
        vault.load(person, session.id).await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert_eq!(vault.protection(), SessionProtection::KeyUnavailable);
    keys.0.blocked.store(false, Ordering::SeqCst);
    assert_eq!(
        vault.create_session().await,
        Err(AgentFailure::VaultUnavailable)
    );
    drop(vault);
    let path = root.path().join(person.to_string()).join("sessions.db");
    let before = fs::read(&path).unwrap();
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), person, Keys::default()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    let saved = keys.0.values.lock().unwrap().clone();
    for key in keys.0.values.lock().unwrap().values_mut() {
        *key = [0; 32];
    }
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), person, keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(keys.0.inserts.load(Ordering::SeqCst), 1);
    *keys.0.values.lock().unwrap() = saved;
    let reopened = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    assert_eq!(reopened.load(person, session.id).await.unwrap(), session);
}

#[tokio::test]
async fn failed_provisioning_and_missing_database_are_not_silently_reinitialized() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    keys.0.blocked.store(true, Ordering::SeqCst);
    assert!(matches!(
        EncryptedAgentVault::create(root.path(), person, keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    keys.0.blocked.store(false, Ordering::SeqCst);
    assert!(matches!(
        EncryptedAgentVault::create(root.path(), person, keys.clone()).await,
        Err(AgentFailure::Conflict)
    ));
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), person, keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    assert_eq!(keys.0.inserts.load(Ordering::SeqCst), 1);
    assert!(
        !root
            .path()
            .join(person.to_string())
            .join("sessions.db")
            .exists()
    );
    let other = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), other, keys.clone())
        .await
        .unwrap();
    drop(vault);
    let path = root.path().join(other.to_string()).join("sessions.db");
    fs::remove_file(&path).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), other, keys.clone())
            .await
            .is_err()
    );
    assert!(!path.exists());
    fs::write(&path, []).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), other, keys)
            .await
            .is_err()
    );
    assert_eq!(fs::metadata(path).unwrap().len(), 0);
}

#[tokio::test]
async fn ciphertext_tampering_fails_closed() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    vault.create_session().await.unwrap();
    vault.checkpoint().await.unwrap();
    drop(vault);
    let path = root.path().join(person.to_string()).join("sessions.db");
    let mut ciphertext = fs::read(&path).unwrap();
    ciphertext[200] ^= 0x40;
    fs::write(&path, &ciphertext).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), ciphertext);
}

#[tokio::test]
async fn plaintext_database_is_not_opened_or_automatically_migrated() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    drop(vault);
    let source = root.path().join("synthetic-plaintext.db");
    let database = turso::Builder::new_local(source.to_str().unwrap())
        .build()
        .await
        .unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE TABLE fixture (payload TEXT)", ())
        .await
        .unwrap();
    connection
        .execute(
            "INSERT INTO fixture VALUES ('synthetic-plaintext-marker')",
            (),
        )
        .await
        .unwrap();
    let mut rows = connection
        .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
        .await
        .unwrap();
    while rows.next().await.unwrap().is_some() {}
    drop(rows);
    drop(connection);
    drop(database);
    let destination = root.path().join(person.to_string()).join("sessions.db");
    fs::copy(&source, &destination).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .is_err()
    );
    assert_eq!(fs::read(source).unwrap(), fs::read(destination).unwrap());
}

#[tokio::test]
async fn encrypted_identity_rejects_relabeling_even_with_a_matching_key() {
    let root = private_root();
    let person = PersonId::new();
    let other = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let session = vault.create_session().await.unwrap();
    vault.checkpoint().await.unwrap();
    drop(vault);
    let second = EncryptedAgentVault::create(root.path(), other, keys.clone())
        .await
        .unwrap();
    drop(second);
    let source = root.path().join(person.to_string());
    let destination = root.path().join(other.to_string());
    for name in ["vault.id", "sessions.db"] {
        fs::copy(source.join(name), destination.join(name)).unwrap();
    }
    let vault_id = Uuid::parse_str(&fs::read_to_string(source.join("vault.id")).unwrap()).unwrap();
    let key = keys.load(person, vault_id).unwrap();
    keys.insert(other, vault_id, &key).unwrap();
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), other, keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
    let original = EncryptedAgentVault::open(root.path(), person, keys)
        .await
        .unwrap();
    assert_eq!(original.load(person, session.id).await.unwrap(), session);
}

#[tokio::test]
async fn insecure_and_symlinked_paths_are_rejected() {
    let root = private_root();
    let person = PersonId::new();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        EncryptedAgentVault::create(root.path(), person, Keys::default())
            .await
            .is_err()
    );
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    drop(vault);
    let directory = root.path().join(person.to_string());
    let lock = directory.join("host.lock");
    fs::remove_file(&lock).unwrap();
    let target = root.path().join("unrelated");
    fs::write(&target, "untouched").unwrap();
    symlink(&target, lock).unwrap();
    assert!(
        EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .is_err()
    );
    assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
}

#[tokio::test]
async fn a_second_handle_or_process_cannot_steal_a_live_vault() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    assert!(matches!(
        EncryptedAgentVault::open(root.path(), person, keys.clone()).await,
        Err(AgentFailure::Conflict)
    ));
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "vault_lock_child"])
        .env("FLOE_TEST_VAULT_ROOT", root.path())
        .env("FLOE_TEST_VAULT_PERSON", person.to_string())
        .status()
        .unwrap();
    assert!(status.success());
    drop(vault);
    assert!(
        EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn vault_lock_child() {
    let Ok(root) = std::env::var("FLOE_TEST_VAULT_ROOT") else {
        return;
    };
    let person =
        PersonId(Uuid::parse_str(&std::env::var("FLOE_TEST_VAULT_PERSON").unwrap()).unwrap());
    assert!(matches!(
        EncryptedAgentVault::open(Path::new(&root), person, Keys::default()).await,
        Err(AgentFailure::Conflict)
    ));
}

struct LocalModel {
    keys: Keys,
    revoke: bool,
    calls: AtomicUsize,
}

impl ModelRunner for LocalModel {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn generate(&self, _: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.revoke {
            self.keys.0.blocked.store(true, Ordering::SeqCst);
        }
        Ok(ModelResponse {
            replay: None,
            schema_version: AGENT_VERSION,
            step: ModelStep::Answer {
                text: "synthetic-private-answer".into(),
            },
            used_tokens: 10,
            cost_micros: 0,
        })
    }
}

struct NoCapabilities;

impl CapabilityHost for NoCapabilities {
    fn descriptors(&self, _: PersonId) -> Vec<CapabilityDescriptor> {
        vec![]
    }

    async fn invoke(&self, _: CapabilityInvocation) -> Result<String, AgentFailure> {
        panic!("No capability may be replayed during vault recovery")
    }
}

fn local_policy() -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "synthetic-vault-test".into(),
        data_classes: vec![DataClass::Personal],
        allowed_placements: vec![ModelPlacement::DeviceLocal],
        performance_class: "fixture".into(),
        projection_version: 1,
        external_transfer_consent: TransferConsent::NotGranted,
        bounded_sensitive_projection: false,
    }
}

#[tokio::test]
async fn runtime_fails_closed_on_key_loss_and_recovers_without_model_replay() {
    let root = private_root();
    let person = PersonId::new();
    let keys = Keys::default();
    let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
        .await
        .unwrap();
    let session = vault.create_session().await.unwrap();
    let model = LocalModel {
        keys: keys.clone(),
        revoke: true,
        calls: AtomicUsize::new(0),
    };
    let policy = local_policy();
    let command = AgentCommand {
        schema_version: AGENT_VERSION,
        person_id: person,
        session_id: session.id,
        expected_revision: 0,
        text: "synthetic-private-question".into(),
    };
    let context = AgentContext {
        projection_version: 1,
        evidence: vec![],
    };
    let mut events = vec![];
    let runtime = AgentRuntime {
        store: &vault,
        model: &model,
        capabilities: &NoCapabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    assert_eq!(
        runtime
            .run_turn(
                command.clone(),
                context.clone(),
                Cancellation::default(),
                |event| events.push(event)
            )
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert!(!events.iter().any(|event| matches!(
        event.event,
        AgentEventKind::Finished {
            outcome: AgentOutcome::Completed,
            ..
        }
    )));
    assert!(!events.iter().any(|event| matches!(
        event.event,
        AgentEventKind::MessageCommitted {
            message: AgentMessage::Assistant { .. },
            ..
        }
    )));
    assert_eq!(
        runtime
            .run_turn(
                command.clone(),
                context.clone(),
                Cancellation::default(),
                |_| {}
            )
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    drop(vault);
    keys.0.blocked.store(false, Ordering::SeqCst);
    let reopened = EncryptedAgentVault::open(root.path(), person, keys.clone())
        .await
        .unwrap();
    let interrupted = reopened.load(person, session.id).await.unwrap();
    assert!(interrupted.active_turn.is_some());
    assert_eq!(interrupted.messages.len(), 1);
    let runtime = AgentRuntime {
        store: &reopened,
        model: &model,
        capabilities: &NoCapabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    let recovered = runtime
        .recover_interrupted(person, session.id, interrupted.revision)
        .await
        .unwrap();
    assert_eq!(recovered.messages, interrupted.messages);
    assert_eq!(
        recovered.last_outcome,
        Some(AgentOutcome::Halted {
            reason: AgentFailure::Interrupted
        })
    );
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert_eq!(reopened.load(person, session.id).await.unwrap(), recovered);
    let model = LocalModel {
        keys,
        revoke: false,
        calls: AtomicUsize::new(0),
    };
    let runtime = AgentRuntime {
        store: &reopened,
        model: &model,
        capabilities: &NoCapabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    let completed = runtime
        .run_turn(
            AgentCommand {
                expected_revision: recovered.revision,
                ..command
            },
            context,
            Cancellation::default(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(completed.last_outcome, Some(AgentOutcome::Completed));
    assert_eq!(completed.messages.len(), 3);
}
