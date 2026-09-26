use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use floe_agent_contract::{
    DependencyCoverage, InvocationKey, TaskId, TaskSnapshot, TaskState,
};
use floe_conversation::AgentMessage;
use floe_execution::Cancellation;
use floe_experts::{
    AgentRegistry, ExpertSettlement, ExpertTaskCompletion, RegistryConfiguration,
    RegistryConfigurationTarget, RegistrySnapshot,
};
use floe_vault::{EncryptedAgentVault, VaultKey, VaultKeyProvider, VaultTaskRecord};

use super::expert_evidence::delegation_message;
use super::schedule_host::TestScheduleHost;
use super::*;

mod builtin_setup;

#[derive(Clone, Default)]
struct Keys(Arc<KeyState>);

#[derive(Default)]
struct KeyState {
    values: Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>,
    blocked: AtomicBool,
}

impl VaultKeyProvider for Keys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
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
        self.0.values.lock().unwrap().insert((person, vault), *key.as_bytes());
        Ok(())
    }
}

struct Fixture {
    vault: EncryptedAgentVault<Keys>,
    person: PersonId,
    keys: Keys,
    root: tempfile::TempDir,
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
        let seed = TestScheduleHost::new_with_instance(
            self.person,
            self.vault.registry_instance_id(),
        )
        .unwrap()
        .snapshot()
        .unwrap();
        self.vault.initialize_expert_registry(&seed).await.unwrap();
        seed
    }

    async fn stage(
        &self,
        generation: u64,
        invocation_id: Uuid,
    ) -> (ExpertTaskCompletion, RegistrySnapshot) {
        let current = self.vault.expert_registry().await.unwrap().unwrap();
        let mut registry = AgentRegistry::restore(current.clone(), current.instance_id).unwrap();
        let assignment = current
            .assignments
            .iter()
            .find(|entry| entry.person_id == self.person)
            .unwrap();
        let package = current.installations.iter().find(|entry| entry.id == assignment.installation_id).unwrap().package.clone();
        let resolved = registry
            .resolve_assignment(
                registry.instance_id(),
                self.person,
                assignment.id,
                &package,
                1,
            )
            .unwrap();
        registry.complete(&resolved, invocation_id).unwrap();
        let staged = registry.snapshot();
        let task_id = TaskId::new();
        let submitted = TaskSnapshot {
            task_id,
            parent_run_id: None,
            principal: self.person.to_string(),
            agent_id: package.id.clone(),
            definition_revision: 1,
            state: TaskState::Submitted,
            result: None,
            artifacts: vec![],
            coverage: DependencyCoverage::Unknown,
            issue: None,
        };
        let admission = floe_experts::ExpertAdmissionIdentity {
            registry_instance_id: current.instance_id,
            assignment_id: assignment.id,
            installation_id: assignment.installation_id,
            package: current
                .installations
                .iter()
                .find(|installation| installation.id == assignment.installation_id)
                .unwrap()
                .package
                .clone(),
            definition_revision: 1,
        };
        self.vault
            .admit_task(VaultTaskRecord {
                snapshot: submitted.clone(),
                admission: admission.clone(),
                invocation_key: InvocationKey::from_uuid(invocation_id).unwrap(),
                request_digest: [1; 32],
                aggregate_revision: 1,
                executor_generation: generation,
            })
            .await
            .unwrap();
        self.vault
            .compare_and_swap_task(
                task_id,
                1,
                generation,
                TaskSnapshot {
                    state: TaskState::Working,
                    ..submitted.clone()
                },
            )
            .await
            .unwrap();
        let result = "Synthetic schedule review completed.".to_owned();
        let completion = ExpertTaskCompletion {
            settlement: ExpertSettlement::new(
                &package.id,
                admission,
                assignment.private_state.revision,
                staged
                    .assignments
                    .iter()
                    .find(|entry| entry.id == assignment.id)
                    .unwrap()
                    .private_state
                    .clone(),
                invocation_id,
                vec![],
                result.clone(),
            ),
            task_id,
            expected_task_revision: 2,
            executor_generation: generation,
            task_snapshot: TaskSnapshot {
                state: TaskState::Completed,
                result: Some(result),
                coverage: DependencyCoverage::Independent,
                ..submitted
            },
        };
        (completion, staged)
    }

    async fn sample(&self) -> TaskSnapshot {
        if self.vault.expert_registry().await.unwrap().is_none() {
            self.prepare().await;
        }
        let generation = self.vault.activate_task_executor().await.unwrap().executor_generation;
        let (completion, _) = self.stage(generation, Uuid::new_v4()).await;
        let terminal = completion.task_snapshot.clone();
        self.vault
            .settle_expert_task_checked(completion, || Ok(()))
            .await
            .unwrap();
        let mut session = self.vault.create_session().await.unwrap();
        let turn_id = Uuid::new_v4();
        session.revision = 1;
        session.active_turn = Some(turn_id);
        session.messages.push(AgentMessage::User {
            turn_id,
            text: "Review the sample day".into(),
        });
        self.vault.compare_and_swap(&session, 0).await.unwrap();
        session.revision = 2;
        session.messages.push(delegation_message(turn_id, &terminal));
        self.vault.compare_and_swap(&session, 1).await.unwrap();
        terminal
    }
}

#[tokio::test]
async fn registry_and_private_state_survive_reopen_with_settled_tasks() {
    let mut fixture = Fixture::new().await;
    let first = fixture.sample().await;
    let second = fixture.sample().await;
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    let expert = before
        .assignments
        .iter()
        .find(|entry| entry.person_id == fixture.person)
        .unwrap();
    assert_eq!(expert.private_state.revision, 2);
    for entry in fs::read_dir(fixture.root.path().join(fixture.person.to_string())).unwrap() {
        let bytes = fs::read(entry.unwrap().path()).unwrap();
        for marker in ["floe.schedule", "completed_invocations", "Synthetic schedule review"] {
            assert!(!bytes.windows(marker.len()).any(|window| window == marker.as_bytes()));
        }
    }
    fixture.vault.checkpoint().await.unwrap();
    drop(fixture.vault);
    fixture.vault = EncryptedAgentVault::open(
        fixture.root.path(),
        fixture.person,
        fixture.keys.clone(),
    )
    .await
    .unwrap();
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(before.clone()));
    assert_eq!(fixture.vault.task(first.task_id).await.unwrap().unwrap().snapshot, first);
    assert_eq!(fixture.vault.task(second.task_id).await.unwrap().unwrap().snapshot, second);
    fixture.sample().await;
    let after = fixture.vault.expert_registry().await.unwrap().unwrap();
    assert_eq!(
        after.assignments.iter().find(|entry| entry.id == expert.id).unwrap().private_state.revision,
        3
    );
}

#[tokio::test]
async fn configuration_preserves_private_state_and_rejects_stale_authority() {
    let mut fixture = Fixture::new().await;
    assert!(fixture.vault.registry_overview().await.unwrap().is_none());
    fixture.sample().await;
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    let expert = before
        .assignments
        .iter()
        .find(|entry| entry.person_id == fixture.person)
        .unwrap();
    let next = fixture
        .vault
        .configure_registry(
            RegistryConfiguration {
                instance_id: before.instance_id,
                expected_revision: before.revision,
                target: RegistryConfigurationTarget::Assignment {
                    id: expert.id,
                    enabled: false,
                },
            },
            Cancellation::default(),
        )
        .await
        .unwrap();
    assert_eq!(next.revision, before.revision + 1);
    let after = fixture.vault.expert_registry().await.unwrap().unwrap();
    let updated = after.assignments.iter().find(|entry| entry.id == expert.id).unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.private_state, expert.private_state);
    assert_eq!(
        fixture.vault.save_expert_registry(before.revision, &after).await,
        Err(AgentFailure::Conflict)
    );
    drop(fixture.vault);
    fixture.vault = EncryptedAgentVault::open(
        fixture.root.path(),
        fixture.person,
        fixture.keys.clone(),
    )
    .await
    .unwrap();
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(after));
}

#[tokio::test]
async fn settlement_rolls_back_registry_and_task_on_failure_and_stale_cas() {
    let fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let generation = fixture.vault.activate_task_executor().await.unwrap().executor_generation;
    let (completion, staged) = fixture.stage(generation, Uuid::new_v4()).await;
    let task_id = completion.task_id;
    assert_eq!(
        fixture
            .vault
            .settle_expert_task_checked(completion, || Err(AgentFailure::StorageUnavailable))
            .await,
        Err(AgentFailure::StorageUnavailable)
    );
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(baseline.clone()));
    assert_eq!(fixture.vault.task(task_id).await.unwrap().unwrap().snapshot.state, TaskState::Working);
    let (retry, _) = fixture.stage(generation, Uuid::new_v4()).await;
    fixture.vault.settle_expert_task_checked(retry, || Ok(())).await.unwrap();
    assert_ne!(fixture.vault.expert_registry().await.unwrap(), Some(baseline));
    assert_eq!(fixture.vault.task(task_id).await.unwrap().unwrap().snapshot.state, TaskState::Working);
    assert_eq!(staged.revision, fixture.vault.expert_registry().await.unwrap().unwrap().revision);
}

#[tokio::test]
async fn key_loss_cannot_partially_settle_registry_or_task() {
    let mut fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let generation = fixture.vault.activate_task_executor().await.unwrap().executor_generation;
    let (completion, _) = fixture.stage(generation, Uuid::new_v4()).await;
    let task_id = completion.task_id;
    let retry = ExpertTaskCompletion {
        settlement: completion.settlement.clone(),
        task_id: completion.task_id,
        expected_task_revision: completion.expected_task_revision,
        executor_generation: completion.executor_generation,
        task_snapshot: completion.task_snapshot.clone(),
    };
    fixture.keys.0.blocked.store(true, Ordering::Release);
    assert_eq!(
        fixture.vault.settle_expert_task_checked(completion, || Ok(())).await,
        Err(AgentFailure::VaultUnavailable)
    );
    fixture.keys.0.blocked.store(false, Ordering::Release);
    drop(fixture.vault);
    fixture.vault = EncryptedAgentVault::open(
        fixture.root.path(),
        fixture.person,
        fixture.keys.clone(),
    )
    .await
    .unwrap();
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(baseline));
    assert_eq!(fixture.vault.task(task_id).await.unwrap().unwrap().snapshot.state, TaskState::Working);
    assert_eq!(
        fixture.vault.settle_expert_task_checked(retry, || Ok(())).await,
        Err(AgentFailure::Conflict)
    );
    let activation = fixture.vault.activate_task_executor().await.unwrap();
    assert_eq!(activation.interrupted.len(), 1);
    assert_eq!(activation.interrupted[0].snapshot.task_id, task_id);
    assert_eq!(fixture.vault.task(task_id).await.unwrap().unwrap().snapshot.state, TaskState::Interrupted);
    let (fresh, _) = fixture.stage(activation.executor_generation, Uuid::new_v4()).await;
    fixture.vault.settle_expert_task_checked(fresh, || Ok(())).await.unwrap();
}

#[tokio::test]
async fn active_task_settlement_preserves_unrelated_installation_disable() {
    let fixture = Fixture::new().await;
    let baseline = fixture.prepare().await;
    let generation = fixture.vault.activate_task_executor().await.unwrap().executor_generation;
    let (completion, _) = fixture.stage(generation, Uuid::new_v4()).await;
    let task_id = completion.task_id;
    let mut registry = AgentRegistry::restore(baseline.clone(), baseline.instance_id).unwrap();
    registry
        .set_installation_enabled(registry.revision(), baseline.installations[0].id, false)
        .unwrap();
    fixture
        .vault
        .save_expert_registry(baseline.revision, &registry.snapshot())
        .await
        .unwrap();
    fixture.vault.settle_expert_task_checked(completion, || Ok(())).await.unwrap();
    let after = fixture.vault.expert_registry().await.unwrap().unwrap();
    assert!(!after.installations.iter().find(|entry| entry.id == baseline.installations[0].id).unwrap().enabled);
    assert_eq!(after.assignments.iter().filter(|entry| entry.private_state.revision == 1).count(), 1);
    assert_eq!(fixture.vault.task(task_id).await.unwrap().unwrap().snapshot.state, TaskState::Completed);
}

#[tokio::test]
async fn concurrent_same_assignment_private_state_settlement_conflicts() {
    let fixture = Fixture::new().await;
    fixture.prepare().await;
    let generation = fixture.vault.activate_task_executor().await.unwrap().executor_generation;
    let (first, _) = fixture.stage(generation, Uuid::new_v4()).await;
    let (raced, _) = fixture.stage(generation, Uuid::new_v4()).await;
    let raced_task_id = raced.task_id;
    fixture.vault.settle_expert_task_checked(first, || Ok(())).await.unwrap();
    assert_eq!(
        fixture.vault.settle_expert_task_checked(raced, || Ok(())).await,
        Err(AgentFailure::Conflict),
    );
    assert_eq!(
        fixture.vault.task(raced_task_id).await.unwrap().unwrap().snapshot.state,
        TaskState::Working,
    );
}

#[tokio::test]
async fn admitted_task_finishes_after_its_assignment_is_disabled() {
    let fixture = Fixture::new().await;
    fixture.prepare().await;
    let generation = fixture.vault.activate_task_executor().await.unwrap().executor_generation;
    let (completion, _) = fixture.stage(generation, Uuid::new_v4()).await;
    let assignment_id = completion.settlement.admission.assignment_id;
    let before = fixture.vault.expert_registry().await.unwrap().unwrap();
    fixture
        .vault
        .configure_registry(
            RegistryConfiguration {
                instance_id: before.instance_id,
                expected_revision: before.revision,
                target: RegistryConfigurationTarget::Assignment {
                    id: assignment_id,
                    enabled: false,
                },
            },
            Cancellation::default(),
        )
        .await
        .unwrap();
    fixture.vault.settle_expert_task_checked(completion, || Ok(())).await.unwrap();
    let after = fixture.vault.expert_registry().await.unwrap().unwrap();
    let assignment = after.assignments.iter().find(|entry| entry.id == assignment_id).unwrap();
    assert!(!assignment.enabled);
    assert_eq!(assignment.private_state.revision, 1);
}

#[tokio::test]
async fn cancelled_configuration_and_missing_registry_fail_closed() {
    let fixture = Fixture::new().await;
    let before = fixture.prepare().await;
    let mut registry = AgentRegistry::restore(before.clone(), before.instance_id).unwrap();
    registry
        .set_installation_enabled(before.revision, before.installations[0].id, false)
        .unwrap();
    assert_eq!(
        fixture
            .vault
            .save_expert_registry_checked(before.revision, &registry.snapshot(), || {
                Err(AgentFailure::Cancelled)
            })
            .await,
        Err(AgentFailure::Cancelled)
    );
    assert_eq!(fixture.vault.expert_registry().await.unwrap(), Some(before));
    let key = fixture.keys.0.values.lock().unwrap().values().next().copied().unwrap();
    let hexkey = key.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    let path = fixture.root.path().join(fixture.person.to_string()).join("sessions.db");
    turso::Builder::new_local(path.to_str().unwrap())
        .experimental_encryption(true)
        .with_encryption(turso::EncryptionOpts {
            cipher: "aes256gcm".into(),
            hexkey,
        })
        .build()
        .await
        .unwrap()
        .connect()
        .unwrap()
        .execute("DELETE FROM agent_expert_registry", ())
        .await
        .unwrap();
    assert_eq!(fixture.vault.expert_registry().await, Err(AgentFailure::VaultUnavailable));
    drop(fixture.vault);
    assert!(EncryptedAgentVault::open(fixture.root.path(), fixture.person, fixture.keys).await.is_err());
}
