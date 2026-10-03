use std::sync::Arc;

use crate::{EncryptedAgentVault, VaultKeyProvider};
use floe_agent_contract::{AgentFailure, BoxFuture, ExecutionScope, OwnerActor};
use floe_experts::{
    AgentRegistry, RegistryCommit, RegistryCommitReceipt, RegistryRepository,
};
use turso::transaction::TransactionBehavior;

use crate::vault::expert_binding_reviews::{
    EXPERT_COMMAND_LIMIT, EXPERT_COMMAND_REGISTRY, count_expert_command_admissions_on,
    ensure_expert_binding_tables_on, insert_expert_command_admission_on,
    map_unique_conflict, read_expert_command_admission_on, read_registry_receipt_on, validate_registry_receipt,
    validate_registry_snapshot, validate_registry_successor,
};

const MAX_REGISTRY_RECEIPT_BYTES: usize = 512 * 1024;

pub struct VaultExpertRegistryRepository<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    actor: OwnerActor,
}

impl<Keys: VaultKeyProvider> VaultExpertRegistryRepository<Keys> {
    pub fn new(
        vault: Arc<EncryptedAgentVault<Keys>>,
        actor: OwnerActor,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        vault.check_access()?;
        if actor.person_id != vault.person_id() {
            return Err(AgentFailure::NotFound);
        }
        Ok(Self { vault, actor })
    }

    fn authorize(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        self.vault.check_access()?;
        actor.validate()?;
        if actor != &self.actor {
            return Err(AgentFailure::CapabilityDenied);
        }
        if actor.person_id != self.vault.person_id() {
            return Err(AgentFailure::NotFound);
        }
        Ok(())
    }

    fn after_access<T>(&self, result: Result<T, AgentFailure>) -> Result<T, AgentFailure> {
        self.vault.check_access()?;
        result
    }

    async fn finish_transaction<T>(
        &self,
        transaction: turso::transaction::Transaction<'_>,
        result: Result<T, AgentFailure>,
    ) -> Result<T, AgentFailure> {
        let result = self
            .vault
            .finish_registry_transaction_checked(transaction, result)
            .await;
        self.after_access(result)
    }
}

impl<Keys: VaultKeyProvider> RegistryRepository for VaultExpertRegistryRepository<Keys> {
    fn read<'a>(
        &'a self,
        actor: &'a OwnerActor,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<floe_experts::RegistrySnapshot, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            let result = async {
                let connection = self.vault.connection()?;
                let snapshot = self
                    .vault
                    .registry_on(&connection)
                    .await?
                    .unwrap_or_else(|| {
                        AgentRegistry::new(self.vault.registry_instance_id()).snapshot()
                    });
                validate_registry_snapshot(
                    &snapshot,
                    self.vault.registry_instance_id(),
                    self.vault.person_id(),
                )?;
                Ok(snapshot)
            }
            .await;
            self.after_access(result)
        })
    }

    fn find_command<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: floe_agent_contract::CommandId,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<RegistryCommitReceipt>, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            if !command_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            let result = async {
                let connection = self.vault.connection()?;
                let Some(admission) =
                    read_expert_command_admission_on(&connection, command_id).await?
                else {
                    return Ok(None);
                };
                if admission.family != EXPERT_COMMAND_REGISTRY {
                    return Err(AgentFailure::Conflict);
                }
                if admission.person_id != actor.person_id
                    || admission.device_id != actor.device_id
                {
                    return Err(AgentFailure::CapabilityDenied);
                }
                let receipt = read_registry_receipt_on(
                    &connection,
                    actor.person_id,
                    &actor.device_id,
                    command_id,
                    self.vault.registry_instance_id(),
                )
                .await?
                .ok_or(AgentFailure::VaultUnavailable)?;
                Ok(Some(receipt))
            }
            .await;
            self.after_access(result)
        })
    }

    fn commit<'a>(
        &'a self,
        commit: RegistryCommit,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<RegistryCommitReceipt, AgentFailure>> {
        Box::pin(async move {
            self.authorize(&commit.actor)?;
            let result = async {
                if !commit.command_id.is_valid() || commit.request_digest == [0; 32] {
                    return Err(AgentFailure::InvalidInput);
                }
                let mut connection = self.vault.connection()?;
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .await
                    .map_err(|error| self.vault.registry_transaction_start_error(error))?;
                let result = async {
                    ensure_expert_binding_tables_on(&transaction).await?;
                    if let Some(admission) =
                        read_expert_command_admission_on(&transaction, commit.command_id).await?
                    {
                        if admission.family != EXPERT_COMMAND_REGISTRY
                            || admission.person_id != commit.actor.person_id
                            || admission.device_id != commit.actor.device_id
                            || admission.request_digest != commit.request_digest
                        {
                            return Err(AgentFailure::Conflict);
                        }
                        return read_registry_receipt_on(
                            &transaction,
                            commit.actor.person_id,
                            &commit.actor.device_id,
                            commit.command_id,
                            self.vault.registry_instance_id(),
                        )
                        .await?
                        .ok_or(AgentFailure::VaultUnavailable);
                    }

                    if count_expert_command_admissions_on(&transaction).await?
                        >= EXPERT_COMMAND_LIMIT
                    {
                        return Err(AgentFailure::BudgetExceeded);
                    }
                    let instance_id = self.vault.registry_instance_id();
                    validate_registry_snapshot(
                        &commit.next,
                        instance_id,
                        self.vault.person_id(),
                    )?;
                    let next_revision = commit
                        .expected_revision
                        .checked_add(1)
                        .ok_or(AgentFailure::BudgetExceeded)?;
                    if commit.next.revision != next_revision {
                        return Err(AgentFailure::Conflict);
                    }
                    let previous = self.vault.registry_on(&transaction).await?;
                    match previous {
                        Some(previous) => {
                            if previous.revision != commit.expected_revision {
                                return Err(AgentFailure::Conflict);
                            }
                            validate_registry_successor(
                                &previous,
                                &commit.next,
                                commit.expected_revision,
                                self.vault.person_id(),
                            )?;
                            let payload = self.vault.registry_payload(&commit.next)?;
                            self.vault
                                .update_registry(
                                    &transaction,
                                    commit.expected_revision,
                                    commit.next.revision,
                                    payload,
                                )
                                .await?;
                        }
                        None => {
                            if commit.expected_revision != 0 {
                                return Err(AgentFailure::Conflict);
                            }
                            let empty = AgentRegistry::new(instance_id).snapshot();
                            validate_registry_successor(
                                &empty,
                                &commit.next,
                                0,
                                self.vault.person_id(),
                            )?;
                            self.vault
                                .initialize_expert_registry_on(&transaction, &commit.next)
                                .await?;
                        }
                    }

                    let receipt = RegistryCommitReceipt {
                        command_id: commit.command_id,
                        person_id: commit.actor.person_id,
                        device_id: commit.actor.device_id.clone(),
                        request_digest: commit.request_digest,
                        snapshot: commit.next.clone(),
                    };
                    validate_registry_receipt(
                        &receipt,
                        commit.command_id,
                        commit.actor.person_id,
                        &commit.actor.device_id,
                        commit.request_digest,
                        self.vault.registry_instance_id(),
                    )?;
                    let payload = serde_json::to_string(&receipt)
                        .map_err(|_| AgentFailure::StorageUnavailable)?;
                    if payload.len() > MAX_REGISTRY_RECEIPT_BYTES {
                        return Err(AgentFailure::BudgetExceeded);
                    }
                    insert_expert_command_admission_on(
                        &transaction,
                        commit.command_id,
                        EXPERT_COMMAND_REGISTRY,
                        commit.actor.person_id,
                        &commit.actor.device_id,
                        commit.request_digest,
                        None,
                    )
                    .await?;
                    transaction
                        .execute(
                            "INSERT INTO agent_expert_registry_receipts (command_id, person_id, device_id, request_digest, snapshot_revision, payload) VALUES (?, ?, ?, ?, ?, ?)",
                            (
                                commit.command_id.as_uuid().to_string(),
                                commit.actor.person_id.to_string(),
                                &commit.actor.device_id,
                                hex_digest(&commit.request_digest),
                                to_i64(receipt.snapshot.revision)?,
                                payload,
                            ),
                        )
                        .await
                        .map_err(map_unique_conflict)?;
                    self.vault.check_access()?;
                    Ok(receipt)
                }
                .await;
                self.finish_transaction(transaction, result).await
            }
            .await;
            self.after_access(result)
        })
    }
}

fn hex_digest(digest: &[u8; 32]) -> String {
    let mut text = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut text, "{byte:02x}");
    }
    text
}

fn to_i64(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::Conflict)
}
