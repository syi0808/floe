use std::sync::Arc;

use floe_agent_contract::{AgentFailure, BoxFuture, TaskId, TaskSnapshot};
use floe_core::{
    EncryptedAgentVault, VaultKeyProvider, VaultTaskActivation, VaultTaskAdmission, VaultTaskRecord,
};
use floe_experts::{TaskActivation, TaskAdmission, TaskRecord, TaskRepository};

pub(super) struct VaultTaskRepository<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
}

impl<Keys> VaultTaskRepository<Keys> {
    pub(super) fn new(vault: Arc<EncryptedAgentVault<Keys>>) -> Self {
        Self { vault }
    }
}

impl<Keys: VaultKeyProvider> TaskRepository for VaultTaskRepository<Keys> {
    fn activate<'a>(&'a self) -> BoxFuture<'a, Result<TaskActivation, AgentFailure>> {
        Box::pin(async move {
            let activation = self.vault.activate_task_executor().await?;
            Ok(from_vault_activation(activation))
        })
    }

    fn admit<'a>(
        &'a self,
        proposed: TaskRecord,
    ) -> BoxFuture<'a, Result<TaskAdmission, AgentFailure>> {
        Box::pin(async move {
            match self.vault.admit_task(to_vault_record(proposed)).await? {
                VaultTaskAdmission::Created(record) => {
                    Ok(TaskAdmission::Created(from_vault_record(record)))
                }
                VaultTaskAdmission::Existing(record) => {
                    Ok(TaskAdmission::Existing(from_vault_record(record)))
                }
            }
        })
    }

    fn compare_and_swap<'a>(
        &'a self,
        task_id: TaskId,
        expected_aggregate_revision: u64,
        executor_generation: u64,
        snapshot: TaskSnapshot,
    ) -> BoxFuture<'a, Result<TaskRecord, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .compare_and_swap_task(
                    task_id,
                    expected_aggregate_revision,
                    executor_generation,
                    snapshot,
                )
                .await
                .map(from_vault_record)
        })
    }

    fn get<'a>(
        &'a self,
        task_id: TaskId,
    ) -> BoxFuture<'a, Result<Option<TaskRecord>, AgentFailure>> {
        Box::pin(async move {
            self.vault
                .task(task_id)
                .await
                .map(|record| record.map(from_vault_record))
        })
    }
}

fn to_vault_record(record: TaskRecord) -> VaultTaskRecord {
    VaultTaskRecord {
        snapshot: record.snapshot,
        invocation_key: record.invocation_key,
        request_digest: record.request_digest,
        aggregate_revision: record.aggregate_revision,
        executor_generation: record.executor_generation,
    }
}

fn from_vault_record(record: VaultTaskRecord) -> TaskRecord {
    TaskRecord {
        snapshot: record.snapshot,
        invocation_key: record.invocation_key,
        request_digest: record.request_digest,
        aggregate_revision: record.aggregate_revision,
        executor_generation: record.executor_generation,
    }
}

fn from_vault_activation(activation: VaultTaskActivation) -> TaskActivation {
    TaskActivation {
        executor_generation: activation.executor_generation,
        interrupted: activation
            .interrupted
            .into_iter()
            .map(from_vault_record)
            .collect(),
    }
}

#[cfg(test)]
mod tests;
