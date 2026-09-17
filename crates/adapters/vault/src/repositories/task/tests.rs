use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

use floe_agent_contract::{DependencyCoverage, InvocationKey, TaskSnapshot, TaskState};
use crate::{VaultKey};
use floe_kernel::PersonId;
use uuid::Uuid;

use super::*;

#[derive(Clone, Default)]
struct Keys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

impl VaultKeyProvider for Keys {
    fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .get(&(person_id, vault_id))
            .copied()
            .map(VaultKey::from_bytes)
            .ok_or(AgentFailure::VaultUnavailable)
    }

    fn insert(
        &self,
        person_id: PersonId,
        vault_id: Uuid,
        key: &VaultKey,
    ) -> Result<(), AgentFailure> {
        self.0
            .lock()
            .unwrap()
            .insert((person_id, vault_id), *key.as_bytes());
        Ok(())
    }
}

fn submitted(person_id: PersonId, task_id: TaskId, generation: u64) -> TaskRecord {
    TaskRecord {
        snapshot: TaskSnapshot {
            task_id,
            parent_run_id: Some(Uuid::new_v4()),
            principal: person_id.to_string(),
            agent_id: "floe.builtin.schedule".into(),
            definition_revision: 3,
            state: TaskState::Submitted,
            result: None,
            artifacts: vec![],
            coverage: DependencyCoverage::Unknown,
            issue: None,
        },
        invocation_key: InvocationKey::new(),
        request_digest: [4; 32],
        aggregate_revision: 1,
        executor_generation: generation,
    }
}

#[tokio::test]
async fn adapter_round_trips_durable_records_and_recovers_after_reopen() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let keys = Keys::default();
    let vault = Arc::new(
        EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap(),
    );
    let repository = VaultTaskRepository::new(Arc::clone(&vault), "floe.builtin.schedule/v1");
    assert_eq!(
        repository.validate_settlement(
            &floe_agent_contract::EndpointSettlement::try_new("unknown", "{}").unwrap()
        ),
        Err(AgentFailure::CapabilityUnavailable)
    );
    let activation = repository.activate().await.unwrap();
    let task_id = TaskId::new();
    let proposed = submitted(person_id, task_id, activation.executor_generation);
    assert_eq!(
        repository.admit(proposed.clone()).await.unwrap(),
        TaskAdmission::Created(proposed.clone())
    );
    assert_eq!(
        repository.admit(proposed.clone()).await.unwrap(),
        TaskAdmission::Existing(proposed.clone())
    );
    let working = repository
        .compare_and_swap(
            task_id,
            1,
            activation.executor_generation,
            TaskSnapshot {
                state: TaskState::Working,
                ..proposed.snapshot
            },
        )
        .await
        .unwrap();
    assert_eq!(repository.get(task_id).await.unwrap(), Some(working));

    drop(repository);
    drop(vault);
    let reopened = EncryptedAgentVault::open(root.path(), person_id, keys)
        .await
        .unwrap();
    let reopened = Arc::new(reopened);
    let repository = Arc::new(crate::VaultTaskRepository::new(
        Arc::clone(&reopened),
        "floe.builtin.schedule/v1",
    ));
    let (_coordinator, recovered) = floe_experts::TaskCoordinator::activate(
        floe_experts::Directory::default(),
        repository,
        "everyday-assistance",
        floe_agent_contract::MAX_OUTPUT_BYTES,
    )
    .await
    .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].snapshot.task_id, task_id);
    assert_eq!(recovered[0].snapshot.state, TaskState::Interrupted);
    assert_eq!(
        reopened.task(task_id).await.unwrap().unwrap().snapshot,
        recovered[0].snapshot
    );
}
