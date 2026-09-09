use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
    time::Duration,
};

use floe_agent::*;
use tokio::{sync::Notify, time::Instant};

use super::*;
use crate::{
    AgentFixturePrompt, AgentFixtureTurn, agent_fixture::FixtureCapabilities, recover_agent_sample,
};

mod calendar_setup;

#[tokio::test]
async fn calendar_session_insert_rolls_back_when_the_post_insert_key_check_fails() {
    let fixture = Fixture::new().await;
    let setup_id = Uuid::new_v4();
    fixture
        .vault
        .install_calendar_expert(
            CalendarExpertSetup {
                instance_id: fixture.vault.registry_instance_id(),
                expected_revision: 0,
                setup_id,
                provider: floe_domain::CalendarProvider::EventKit,
                calendar_ids: vec!["synthetic-insert-check".into()],
            },
            Cancellation::default(),
        )
        .await
        .unwrap();
    fixture.keys.0.fail_on_read.store(4, Ordering::Release);
    assert_eq!(
        fixture
            .vault
            .create_calendar_session(setup_id, Cancellation::default())
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert!(fixture.keys.0.blocked.load(Ordering::Acquire));
    let connection = fixture.vault.database.connect().unwrap();
    let mut rows = connection
        .query("SELECT count(*) FROM agent_sessions", ())
        .await
        .unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        0
    );
}

#[tokio::test]
async fn expert_atomic_commit_cannot_rebind_an_existing_conversation() {
    let fixture = Fixture::new().await;
    let mut session = fixture.vault.create_sample_session().await.unwrap();
    let seeded = FixtureCapabilities::new_with_instance(
        fixture.person,
        fixture.vault.registry_instance_id(),
    )
    .unwrap()
    .snapshot()
    .unwrap();
    fixture
        .vault
        .initialize_expert_registry(&seeded)
        .await
        .unwrap();
    session.scope = Some(AgentSessionScope::Calendar {
        setup_id: Uuid::new_v4(),
        provider: floe_domain::CalendarProvider::Fixture,
    });
    session.revision = 1;
    assert_eq!(
        fixture
            .vault
            .commit_expert_session(&session, 0, seeded.revision, &seeded)
            .await,
        Err(AgentFailure::Conflict)
    );
    assert!(
        fixture
            .vault
            .load(fixture.person, session.id)
            .await
            .unwrap()
            .scope
            .is_none()
    );
    assert_eq!(
        fixture.vault.expert_registry().await.unwrap().unwrap(),
        seeded
    );
}

#[derive(Clone, Default)]
struct Keys(Arc<KeyState>);

#[derive(Default)]
struct KeyState {
    values: Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>,
    blocked: AtomicBool,
    fail_on_read: std::sync::atomic::AtomicUsize,
}

impl VaultKeyProvider for Keys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
        let remaining = self.0.fail_on_read.load(Ordering::Acquire);
        if remaining > 0 && self.0.fail_on_read.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.blocked.store(true, Ordering::Release);
        }
        if self.0.blocked.load(Ordering::Acquire) {
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
        self.0
            .values
            .lock()
            .unwrap()
            .insert((person, vault), *key.as_bytes());
        Ok(())
    }
}

struct Fixture {
    vault: EncryptedAgentVault<Keys>,
    person: PersonId,
    keys: Keys,
    root: tempfile::TempDir,
}

#[tokio::test]
async fn overview_is_read_only_and_configuration_preserves_private_state_and_grants_after_reopen() {
    let mut fixture = Fixture::new().await;
    assert_eq!(fixture.vault.registry_overview().await.unwrap(), None);
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), None);
    fixture.sample().await;
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    let overview = fixture.vault.registry_overview().await.unwrap().unwrap();
    assert_eq!(overview.person_id, fixture.person);
    let expert = before
        .assignments
        .iter()
        .find(|assignment| !assignment.granted_tool_assignments.is_empty())
        .unwrap();
    let encoded = serde_json::to_string(&overview).unwrap();
    assert!(!encoded.contains("last_invocation_id"));
    assert!(!encoded.contains(&expert.granted_view_handles[0].to_string()));
    assert!(!encoded.contains("private_state"));
    let next = fixture
        .vault
        .configure_registry(
            RegistryConfiguration {
                instance_id: overview.instance_id,
                expected_revision: overview.revision,
                target: RegistryConfigurationTarget::Assignment {
                    id: expert.id,
                    enabled: false,
                },
            },
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(next.revision, overview.revision + 1);
    let after = fixture.vault.expert_registry().await.unwrap().unwrap();
    let updated = after
        .assignments
        .iter()
        .find(|assignment| assignment.id == expert.id)
        .unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.private_state, expert.private_state);
    assert_eq!(updated.granted_view_handles, expert.granted_view_handles);
    assert_eq!(after.packages, before.packages);
    assert_eq!(after.installations, before.installations);
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    assert_eq!(
        fixture.vault.registry_overview().await.unwrap().unwrap(),
        next
    );
}

#[tokio::test]
async fn configuration_rejects_stale_instance_revision_unknown_targets_and_cancellation() {
    let fixture = Fixture::new().await;
    let before = fixture.prepare().await;
    for mode in 0..4 {
        let mut configuration = RegistryConfiguration {
            instance_id: before.instance_id,
            expected_revision: before.revision,
            target: RegistryConfigurationTarget::Installation {
                id: before.installations[0].id,
                enabled: false,
            },
        };
        let cancellation = Cancellation::default();
        match mode {
            0 => configuration.instance_id = Uuid::new_v4(),
            1 => configuration.expected_revision -= 1,
            2 => {
                configuration.target = RegistryConfigurationTarget::Assignment {
                    id: Uuid::new_v4(),
                    enabled: true,
                }
            }
            _ => cancellation.cancel(),
        }
        assert!(
            fixture
                .vault
                .configure_registry(configuration, cancellation)
                .await
                .is_err()
        );
        assert_eq!(
            fixture.vault.expert_registry().await.unwrap().unwrap(),
            before
        );
    }
    fixture.keys.0.fail_on_read.store(2, Ordering::Release);
    assert_eq!(
        fixture.vault.registry_overview().await,
        Err(AgentFailure::VaultUnavailable)
    );
}

#[tokio::test]
async fn cancelled_configuration_validation_rolls_back_the_staged_enablement_update() {
    let fixture = Fixture::new().await;
    let before = fixture.prepare().await;
    let mut registry = AgentRegistry::restore(before.clone(), before.instance_id).unwrap();
    registry
        .set_installation_enabled(before.revision, before.installations[0].id, false)
        .unwrap();
    let checks = std::sync::atomic::AtomicUsize::new(0);
    let result = fixture
        .vault
        .save_expert_registry_checked(before.revision, &registry.snapshot(), || {
            if checks.fetch_add(1, Ordering::AcqRel) > 0 {
                Err(AgentFailure::Cancelled)
            } else {
                Ok(())
            }
        })
        .await;
    assert_eq!(result, Err(AgentFailure::Cancelled));
    assert_eq!(
        fixture.vault.expert_registry().await.unwrap().unwrap(),
        before
    );
}

#[tokio::test]
async fn calendar_bindings_persist_encrypted_without_exposing_sources_in_the_overview() {
    let mut fixture = Fixture::new().await;
    let snapshot = fixture.prepare().await;
    let mut registry =
        AgentRegistry::restore(snapshot, fixture.vault.registry_instance_id()).unwrap();
    let revision = registry.revision();
    let handle = registry
        .register_calendar_view(
            revision,
            fixture.person,
            floe_domain::CalendarProvider::Fixture,
            vec!["private-calendar-canary".into()],
        )
        .unwrap();
    fixture
        .vault
        .save_expert_registry(revision, &registry.snapshot())
        .await
        .unwrap();
    let revision = registry.revision();
    registry
        .set_calendar_view_enabled(revision, fixture.person, handle, true)
        .unwrap();
    fixture
        .vault
        .save_expert_registry(revision, &registry.snapshot())
        .await
        .unwrap();
    let expected = registry.snapshot();
    let overview =
        serde_json::to_string(&fixture.vault.registry_overview().await.unwrap()).unwrap();
    assert!(
        !overview.contains("private-calendar-canary") && !overview.contains(&handle.to_string())
    );
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    assert_eq!(
        fixture.vault.expert_registry().await.unwrap().unwrap(),
        expected
    );
    let bytes = fs::read(
        fixture
            .root
            .path()
            .join(fixture.person.to_string())
            .join("sessions.db"),
    )
    .unwrap();
    assert!(
        !bytes
            .windows(b"private-calendar-canary".len())
            .any(|window| window == b"private-calendar-canary")
    );
}

#[tokio::test]
async fn persisted_binding_cannot_be_retargeted_removed_or_created_over_a_legacy_handle() {
    let fixture = Fixture::new().await;
    let snapshot = fixture.prepare().await;
    let mut registry =
        AgentRegistry::restore(snapshot.clone(), fixture.vault.registry_instance_id()).unwrap();
    let revision = registry.revision();
    registry
        .register_calendar_view(
            revision,
            fixture.person,
            floe_domain::CalendarProvider::Fixture,
            vec!["home".into()],
        )
        .unwrap();
    let initial = registry.snapshot();
    for mode in 0..3 {
        let mut forged = initial.clone();
        match mode {
            0 => forged.calendar_views[0].enabled = true,
            1 => forged.calendar_views[0].person_id = PersonId::new(),
            _ => forged.calendar_views[0].handle = snapshot.assignments[0].granted_view_handles[0],
        }
        assert!(
            fixture
                .vault
                .save_expert_registry(revision, &forged)
                .await
                .is_err()
        );
    }
    fixture
        .vault
        .save_expert_registry(revision, &initial)
        .await
        .unwrap();
    for mode in 0..4 {
        let mut forged = initial.clone();
        forged.revision += 1;
        match mode {
            0 => forged.calendar_views[0].calendar_ids = vec!["different".into()],
            1 => forged.calendar_views[0].provider = floe_domain::CalendarProvider::EventKit,
            2 => forged.calendar_views[0].handle = Uuid::new_v4(),
            _ => forged.calendar_views.clear(),
        }
        assert_eq!(
            fixture
                .vault
                .save_expert_registry(initial.revision, &forged)
                .await,
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            fixture.vault.expert_registry().await.unwrap().unwrap(),
            initial
        );
    }
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person = PersonId::new();
        let keys = Keys::default();
        let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        Self {
            vault,
            person,
            keys,
            root,
        }
    }

    async fn prepare(&self) -> RegistrySnapshot {
        let capabilities =
            FixtureCapabilities::new_with_instance(self.person, self.vault.registry_instance_id())
                .unwrap();
        let snapshot = capabilities.snapshot().unwrap();
        self.vault
            .initialize_expert_registry(&snapshot)
            .await
            .unwrap();
        snapshot
    }

    async fn stage(&self, call_id: Uuid) -> (AgentSession, AgentSession, RegistrySnapshot) {
        let mut previous = self.vault.create_sample_session().await.unwrap();
        let turn_id = Uuid::new_v4();
        previous.active_turn = Some(turn_id);
        previous.messages.push(AgentMessage::User {
            turn_id,
            text: "Synthetic request".into(),
        });
        previous.revision = 1;
        self.vault.compare_and_swap(&previous, 0).await.unwrap();
        let snapshot = self.vault.expert_registry().await.unwrap().unwrap();
        let capabilities = FixtureCapabilities::from_snapshot(self.person, snapshot).unwrap();
        let output = capabilities
            .invoke(CapabilityInvocation {
                usage: Default::default(),
                schema_version: 1,
                call_id,
                person_id: self.person,
                session_id: previous.id,
                turn_id,
                capability_id: "fixture.schedule.read".into(),
                input: "sample-day".into(),
                max_output_bytes: 16384,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: Cancellation::default(),
            })
            .await
            .unwrap();
        let mut next = previous.clone();
        next.messages.push(AgentMessage::Capability {
            turn_id,
            call_id,
            capability_id: "fixture.schedule.read".into(),
            input: "sample-day".into(),
            result: Ok(output),
        });
        next.revision += 1;
        (previous, next, capabilities.snapshot().unwrap())
    }

    async fn receipts(&self) -> i64 {
        self.vault
            .connection()
            .unwrap()
            .query("SELECT count(*) FROM agent_expert_receipts", ())
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<i64>(0)
            .unwrap()
    }

    async fn sample(&self) -> AgentSession {
        let session = self.vault.create_sample_session().await.unwrap();
        self.vault
            .run_persisted_agent_sample(
                AgentFixtureTurn {
                    person_id: self.person,
                    session_id: session.id,
                    expected_revision: 0,
                    prompt: AgentFixturePrompt::Today,
                },
                Cancellation::default(),
                Duration::ZERO,
                |_| {},
            )
            .await
            .unwrap()
    }
}

fn expert(session: &AgentSession) -> ExpertResult {
    let AgentMessage::Capability {
        result: Ok(output), ..
    } = &session.messages[1]
    else {
        panic!("expected Expert result");
    };
    serde_json::from_str(output).unwrap()
}

#[tokio::test]
async fn registry_and_private_state_survive_sessions_reopen_wal_and_checkpoint_encrypted() {
    let mut fixture = Fixture::new().await;
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), None);
    let first = fixture.sample().await;
    let first_result = expert(&first);
    assert_eq!(first_result.state_revision, 1);
    let second_result = expert(&fixture.sample().await);
    assert_eq!(second_result.state_revision, 2);
    assert_eq!(first_result.assignment_id, second_result.assignment_id);
    assert_eq!(first_result.view_handle, second_result.view_handle);
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    for entry in fs::read_dir(fixture.root.path().join(fixture.person.to_string())).unwrap() {
        let bytes = fs::read(entry.unwrap().path()).unwrap();
        for marker in ["floe.schedule", "completed_invocations", "Design review"] {
            assert!(
                !bytes
                    .windows(marker.len())
                    .any(|window| window == marker.as_bytes())
            );
        }
    }
    fixture.vault.checkpoint().await.unwrap();
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(before));
    assert_eq!(
        fixture.vault.load(fixture.person, first.id).await.unwrap(),
        first
    );
    let third = expert(&fixture.sample().await);
    assert_eq!(third.state_revision, 3);
    assert_eq!(third.instance_id, first_result.instance_id);
    assert_eq!(third.assignment_id, first_result.assignment_id);
    assert_eq!(fixture.receipts().await, 3);
}

#[tokio::test]
async fn key_failure_rolls_back_component_initialization_before_retry() {
    let mut fixture = Fixture::new().await;
    let capabilities = FixtureCapabilities::new_with_instance(
        fixture.person,
        fixture.vault.registry_instance_id(),
    )
    .unwrap();
    let seed = capabilities.snapshot().unwrap();
    fixture.keys.0.fail_on_read.store(2, Ordering::Release);
    assert_eq!(
        fixture.vault.initialize_expert_registry(&seed).await,
        Err(AgentFailure::VaultUnavailable)
    );
    fixture.keys.0.blocked.store(false, Ordering::Release);
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), None);
    fixture
        .vault
        .initialize_expert_registry(&seed)
        .await
        .unwrap();
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(seed));
    assert_eq!(fixture.keys.0.values.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn initialization_is_explicit_and_missing_component_is_not_recreated() {
    let mut fixture = Fixture::new().await;
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), None);
    let snapshot = fixture.prepare().await;
    assert_eq!(
        fixture.vault.initialize_expert_registry(&snapshot).await,
        Err(AgentFailure::Conflict)
    );
    fixture
        .vault
        .connection()
        .unwrap()
        .execute("DELETE FROM agent_expert_registry", ())
        .await
        .unwrap();
    assert_eq!(
        fixture.vault.expert_registry().await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert_eq!(
        fixture.vault.initialize_expert_registry(&snapshot).await,
        Err(AgentFailure::VaultUnavailable)
    );
    drop(fixture.vault);
    assert!(matches!(
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone()).await,
        Err(AgentFailure::VaultUnavailable)
    ));
}

#[tokio::test]
async fn saved_revocation_and_configuration_cas_do_not_reset_private_state_or_namespace() {
    let mut fixture = Fixture::new().await;
    let first = expert(&fixture.sample().await);
    let previous = fixture.vault.expert_registry().await.unwrap().unwrap();
    let mut registry =
        AgentRegistry::restore(previous.clone(), fixture.vault.registry_instance_id()).unwrap();
    registry
        .set_assignment_enabled(
            registry.revision(),
            fixture.person,
            first.assignment_id,
            false,
        )
        .unwrap();
    let next = registry.snapshot();
    fixture
        .vault
        .save_expert_registry(previous.revision, &next)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .vault
            .save_expert_registry(previous.revision, &next)
            .await,
        Err(AgentFailure::Conflict)
    );
    let mut foreign = next.clone();
    foreign.revision += 1;
    foreign.assignments[0].person_id = PersonId::new();
    assert_eq!(
        fixture
            .vault
            .save_expert_registry(next.revision, &foreign)
            .await,
        Err(AgentFailure::NotFound)
    );
    let mut forged = next.clone();
    forged.revision += 1;
    let state = &mut forged
        .assignments
        .iter_mut()
        .find(|entry| entry.id == first.assignment_id)
        .unwrap()
        .private_state;
    state.revision += 1;
    state.completed_invocations += 1;
    state.last_invocation_id = Some(Uuid::new_v4());
    assert_eq!(
        fixture
            .vault
            .save_expert_registry(next.revision, &forged)
            .await,
        Err(AgentFailure::Conflict)
    );
    let mut moved = next.clone();
    moved.revision += 1;
    let source = moved.installations[0].clone();
    moved.installations.push(floe_agent::PackageInstallation {
        id: Uuid::new_v4(),
        ..source
    });
    moved.assignments[0].installation_id = moved.installations.last().unwrap().id;
    assert_eq!(
        fixture
            .vault
            .save_expert_registry(next.revision, &moved)
            .await,
        Err(AgentFailure::Conflict)
    );
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    let unavailable = fixture.sample().await;
    assert!(matches!(
        &unavailable.messages[1],
        AgentMessage::Capability {
            result: Err(AgentFailure::CapabilityDenied),
            ..
        }
    ));
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(next));
    assert_eq!(fixture.receipts().await, 1);
}

#[tokio::test]
async fn persisted_authority_change_during_model_work_never_publishes_expert_success() {
    let fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let session = fixture.vault.create_sample_session().await.unwrap();
    let started = Notify::new();
    let run = fixture.vault.run_persisted_agent_sample(
        AgentFixtureTurn {
            person_id: fixture.person,
            session_id: session.id,
            expected_revision: 0,
            prompt: AgentFixturePrompt::Today,
        },
        Cancellation::default(),
        Duration::from_millis(100),
        |event| {
            if matches!(event.event, AgentEventKind::ModelStarted { .. }) {
                started.notify_one();
            }
        },
    );
    let revoke = async {
        tokio::time::timeout(Duration::from_secs(1), started.notified())
            .await
            .unwrap();
        let mut registry =
            AgentRegistry::restore(baseline.clone(), fixture.vault.registry_instance_id()).unwrap();
        registry
            .set_installation_enabled(registry.revision(), baseline.installations[0].id, false)
            .unwrap();
        fixture
            .vault
            .save_expert_registry(baseline.revision, &registry.snapshot())
            .await
            .unwrap();
    };
    let (result, ()) = tokio::join!(run, revoke);
    assert_eq!(result, Err(AgentFailure::Conflict));
    let actual = fixture
        .vault
        .load(fixture.person, session.id)
        .await
        .unwrap();
    assert_eq!(actual.messages.len(), 1);
    assert!(actual.active_turn.is_some());
    let registry = fixture.vault.expert_registry().await.unwrap().unwrap();
    assert!(
        registry
            .assignments
            .iter()
            .all(|assignment| assignment.private_state.revision == 0)
    );
    assert_eq!(fixture.receipts().await, 0);
}

#[tokio::test]
async fn failed_paired_commit_rolls_back_state_and_receipt_then_retry_is_single() {
    let fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let (previous, next, staged) = fixture.stage(Uuid::new_v4()).await;
    assert_eq!(
        fixture
            .vault
            .commit_expert_session_with_hook(
                &next,
                previous.revision,
                baseline.revision,
                &staged,
                std::future::ready(Err(AgentFailure::StorageUnavailable))
            )
            .await,
        Err(AgentFailure::StorageUnavailable)
    );
    assert_eq!(
        fixture
            .vault
            .load(fixture.person, previous.id)
            .await
            .unwrap(),
        previous
    );
    assert_eq!(
        fixture.vault.expert_registry().await.unwrap(),
        Some(baseline.clone())
    );
    assert_eq!(fixture.receipts().await, 0);
    assert_eq!(
        fixture
            .vault
            .commit_expert_session(&next, previous.revision, baseline.revision, &staged)
            .await
            .unwrap(),
        staged
    );
    assert_eq!(fixture.receipts().await, 1);
    assert_eq!(
        fixture
            .vault
            .commit_expert_session(&next, previous.revision, baseline.revision, &staged)
            .await,
        Err(AgentFailure::Conflict)
    );
}

#[tokio::test]
async fn dropping_a_transaction_after_registry_write_cannot_leave_half_a_commit() {
    let fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let (previous, next, staged) = fixture.stage(Uuid::new_v4()).await;
    let entered = Notify::new();
    {
        let operation = fixture.vault.commit_expert_session_with_hook(
            &next,
            previous.revision,
            baseline.revision,
            &staged,
            async {
                entered.notify_one();
                std::future::pending().await
            },
        );
        tokio::pin!(operation);
        tokio::select! {
            result = &mut operation => panic!("unexpected completion: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(1), entered.notified()) => result.unwrap(),
        }
    }
    assert_eq!(
        fixture
            .vault
            .load(fixture.person, previous.id)
            .await
            .unwrap(),
        previous
    );
    assert_eq!(
        fixture.vault.expert_registry().await.unwrap(),
        Some(baseline.clone())
    );
    assert_eq!(fixture.receipts().await, 0);
    fixture
        .vault
        .commit_expert_session(&next, previous.revision, baseline.revision, &staged)
        .await
        .unwrap();
}

#[tokio::test]
async fn key_loss_at_commit_fails_closed_without_advancing_state_or_session() {
    let mut fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let (previous, next, staged) = fixture.stage(Uuid::new_v4()).await;
    let keys = fixture.keys.clone();
    assert_eq!(
        fixture
            .vault
            .commit_expert_session_with_hook(
                &next,
                previous.revision,
                baseline.revision,
                &staged,
                async {
                    keys.0.blocked.store(true, Ordering::Release);
                    Ok(())
                }
            )
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert_eq!(
        fixture.vault.expert_registry().await,
        Err(AgentFailure::VaultUnavailable)
    );
    fixture.keys.0.blocked.store(false, Ordering::Release);
    assert_eq!(
        fixture.vault.expert_registry().await,
        Err(AgentFailure::VaultUnavailable)
    );
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    assert_eq!(
        fixture.vault.expert_registry().await.unwrap(),
        Some(baseline)
    );
    assert_eq!(
        fixture
            .vault
            .load(fixture.person, previous.id)
            .await
            .unwrap(),
        previous
    );
    assert_eq!(fixture.receipts().await, 0);
}

#[tokio::test]
async fn key_loss_after_commit_requires_reopen_and_reconciles_a_complete_transaction() {
    let mut fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let (previous, next, staged) = fixture.stage(Uuid::new_v4()).await;
    fixture.keys.0.fail_on_read.store(3, Ordering::Release);
    assert_eq!(
        fixture
            .vault
            .commit_expert_session(&next, previous.revision, baseline.revision, &staged)
            .await,
        Err(AgentFailure::VaultUnavailable)
    );
    assert_eq!(
        fixture.vault.protection(),
        SessionProtection::KeyUnavailable
    );
    fixture.keys.0.blocked.store(false, Ordering::Release);
    assert_eq!(
        fixture.vault.expert_registry().await,
        Err(AgentFailure::VaultUnavailable)
    );
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    assert_eq!(
        fixture.vault.expert_registry().await.unwrap(),
        Some(staged.clone())
    );
    assert_eq!(
        fixture
            .vault
            .load(fixture.person, previous.id)
            .await
            .unwrap(),
        next
    );
    assert_eq!(fixture.receipts().await, 1);
    assert_eq!(
        fixture
            .vault
            .commit_expert_session(&next, previous.revision, baseline.revision, &staged)
            .await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(staged));
    assert_eq!(fixture.receipts().await, 1);
}

#[tokio::test]
async fn older_invocation_replay_is_rejected_by_the_durable_receipt_ledger() {
    let fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let original = Uuid::new_v4();
    let (previous, next, staged) = fixture.stage(original).await;
    fixture
        .vault
        .commit_expert_session(&next, previous.revision, baseline.revision, &staged)
        .await
        .unwrap();
    let (previous, next, newer) = fixture.stage(Uuid::new_v4()).await;
    fixture
        .vault
        .commit_expert_session(&next, previous.revision, staged.revision, &newer)
        .await
        .unwrap();
    let (previous, next, replay) = fixture.stage(original).await;
    assert_eq!(
        fixture
            .vault
            .commit_expert_session(&next, previous.revision, newer.revision, &replay)
            .await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(newer));
    assert_eq!(fixture.receipts().await, 2);
}

#[tokio::test]
async fn abandoned_draft_is_discarded_and_committed_expert_recovery_never_replays() {
    let mut fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let (previous, _uncommitted, staged) = fixture.stage(Uuid::new_v4()).await;
    let mut halted = previous.clone();
    halted.revision += 1;
    halted.active_turn = None;
    halted.last_outcome = Some(AgentOutcome::Halted {
        reason: AgentFailure::Cancelled,
    });
    let reconciled = fixture
        .vault
        .commit_expert_session(&halted, previous.revision, baseline.revision, &staged)
        .await
        .unwrap();
    assert_eq!(reconciled, baseline);
    assert_eq!(fixture.receipts().await, 0);
    let (previous, next, staged) = fixture.stage(Uuid::new_v4()).await;
    fixture
        .vault
        .commit_expert_session(&next, previous.revision, baseline.revision, &staged)
        .await
        .unwrap();
    drop(fixture.vault);
    fixture.vault =
        EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys.clone())
            .await
            .unwrap();
    let recovered = recover_agent_sample(&fixture.vault, fixture.person, next.id, next.revision)
        .await
        .unwrap();
    assert_eq!(
        recovered.last_outcome,
        Some(AgentOutcome::Halted {
            reason: AgentFailure::Interrupted
        })
    );
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(staged));
    assert_eq!(fixture.receipts().await, 1);
}
