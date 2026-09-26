use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
};

use floe_agent_contract::{DependencyCoverage, InvocationKey};

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

fn submitted(
    person_id: PersonId,
    task_id: TaskId,
    generation: u64,
    admission: floe_experts::ExpertAdmissionIdentity,
) -> VaultTaskRecord {
    VaultTaskRecord {
        snapshot: TaskSnapshot {
            task_id,
            parent_run_id: Some(Uuid::new_v4()),
            principal: person_id.to_string(),
            agent_id: "floe.builtin.schedule".into(),
            definition_revision: 7,
            state: TaskState::Submitted,
            result: None,
            artifacts: vec![],
            coverage: DependencyCoverage::Unknown,
            issue: None,
        },
        admission,
        selection: floe_experts::ExpertExecutionSelection::without_requirements(1).unwrap(),
        invocation_key: InvocationKey::new(),
        request_digest: [7; 32],
        aggregate_revision: 1,
        executor_generation: generation,
    }
}

#[tokio::test]
async fn durable_task_admission_cas_and_generation_recovery_are_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let keys = Keys::default();
    let mut vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
        .await
        .unwrap();
    let admission = crate::test_expert_registry::install(&vault, person_id, 7).await.unwrap();
    let task_id = TaskId::new();
    let proposed = submitted(person_id, task_id, 1, admission.clone());

    assert_eq!(
        vault.admit_task(proposed.clone()).await,
        Err(AgentFailure::Conflict)
    );
    let activated = vault.activate_task_executor().await.unwrap();
    assert_eq!(activated.executor_generation, 1);
    assert!(activated.interrupted.is_empty());
    assert_eq!(
        vault.admit_task(proposed.clone()).await.unwrap(),
        VaultTaskAdmission::Created(proposed.clone())
    );
    assert_eq!(
        vault.admit_task(proposed.clone()).await.unwrap(),
        VaultTaskAdmission::Existing(proposed.clone())
    );
    let mut changed_admission = proposed.clone();
    changed_admission.admission.assignment_id = Uuid::new_v4();
    assert_eq!(
        vault.admit_task(changed_admission).await,
        Err(AgentFailure::Conflict)
    );
    let mut duplicate_invocation = proposed.clone();
    duplicate_invocation.snapshot.task_id = TaskId::new();
    assert_eq!(
        vault.admit_task(duplicate_invocation).await,
        Err(AgentFailure::Conflict)
    );
    let mut changed_retry = proposed.clone();
    changed_retry.request_digest = [8; 32];
    assert_eq!(
        vault.admit_task(changed_retry).await,
        Err(AgentFailure::Conflict)
    );
    let working = vault
        .compare_and_swap_task(
            task_id,
            1,
            1,
            TaskSnapshot {
                state: TaskState::Working,
                ..proposed.snapshot.clone()
            },
        )
        .await
        .unwrap();
    assert_eq!(working.aggregate_revision, 2);
    assert_eq!(working.snapshot.state, TaskState::Working);
    assert_eq!(
        vault
            .compare_and_swap_task(task_id, 1, 1, working.snapshot.clone())
            .await,
        Err(AgentFailure::Conflict)
    );

    let recovered = vault.activate_task_executor().await.unwrap();
    assert_eq!(recovered.executor_generation, 2);
    assert_eq!(recovered.interrupted.len(), 1);
    assert_eq!(
        recovered.interrupted[0].snapshot.state,
        TaskState::Interrupted
    );
    assert_eq!(recovered.interrupted[0].aggregate_revision, 3);
    assert_eq!(recovered.interrupted[0].executor_generation, 2);
    assert_eq!(
        vault
            .compare_and_swap_task(task_id, 2, 1, working.snapshot)
            .await,
        Err(AgentFailure::Conflict)
    );

    let second_id = TaskId::new();
    let second = submitted(person_id, second_id, 2, admission.clone());
    vault.admit_task(second.clone()).await.unwrap();
    drop(vault);
    vault = EncryptedAgentVault::open(root.path(), person_id, keys)
        .await
        .unwrap();
    assert_eq!(
        vault
            .admit_task(submitted(person_id, TaskId::new(), 2, admission.clone()))
            .await,
        Err(AgentFailure::Conflict)
    );
    assert_eq!(
        vault.task(task_id).await.unwrap(),
        Some(recovered.interrupted[0].clone())
    );
    let restarted = vault.activate_task_executor().await.unwrap();
    assert_eq!(restarted.executor_generation, 3);
    assert_eq!(restarted.interrupted.len(), 1);
    assert_eq!(restarted.interrupted[0].snapshot.task_id, second_id);
    assert_eq!(
        restarted.interrupted[0].snapshot.state,
        TaskState::Interrupted
    );
    assert!(
        vault
            .activate_task_executor()
            .await
            .unwrap()
            .interrupted
            .is_empty()
    );
}

#[tokio::test]
async fn task_store_rejects_foreign_principals_and_illegal_terminal_shapes() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let person_id = PersonId::new();
    let vault = EncryptedAgentVault::create(root.path(), person_id, Keys::default())
        .await
        .unwrap();
    let admission = crate::test_expert_registry::install(&vault, person_id, 7).await.unwrap();
    let generation = vault
        .activate_task_executor()
        .await
        .unwrap()
        .executor_generation;
    let task_id = TaskId::new();
    let mut foreign = submitted(person_id, task_id, generation, admission.clone());
    foreign.snapshot.principal = PersonId::new().to_string();
    assert_eq!(
        vault.admit_task(foreign).await,
        Err(AgentFailure::CapabilityDenied)
    );
    let proposed = submitted(person_id, task_id, generation, admission);
    vault.admit_task(proposed.clone()).await.unwrap();
    let invalid = TaskSnapshot {
        state: TaskState::Completed,
        result: Some("result".into()),
        artifacts: vec![],
        coverage: DependencyCoverage::Independent,
        ..proposed.snapshot
    };
    assert_eq!(
        vault
            .compare_and_swap_task(task_id, 1, generation, invalid)
            .await,
        Err(AgentFailure::Conflict)
    );
}
