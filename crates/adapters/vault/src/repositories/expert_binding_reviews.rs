use std::sync::Arc;

use crate::{EncryptedAgentVault, VaultKeyProvider};
use floe_agent_contract::{AgentFailure, BoxFuture, CommandId, ExecutionScope, OwnerActor};
use floe_experts::{
    BindingReplacementReceipt, BindingReviewDescriptor, BindingReviewRef, BindingReviewRepository,
    ReviewedBindingReplacement,
};
use turso::transaction::TransactionBehavior;

use crate::vault::expert_binding_reviews::{
    EXPERT_COMMAND_LIMIT, EXPERT_COMMAND_PREPARE, EXPERT_COMMAND_REPLACEMENT,
    count_expert_command_admissions_on, ensure_expert_binding_tables_on,
    insert_expert_command_admission_on, read_binding_replacement_by_command_on,
    read_binding_replacement_for_review_on, read_binding_review_on,
    read_expert_command_admission_on, validate_binding_registry_successor,
    validate_registry_snapshot, verify_prepare_identity,
};

const MAX_REVIEW_PAYLOAD_BYTES: usize = 256 * 1024;
const MAX_REPLACEMENT_RECEIPT_BYTES: usize = 512 * 1024;

pub struct VaultExpertBindingReviewRepository<Keys> {
    vault: Arc<EncryptedAgentVault<Keys>>,
    actor: OwnerActor,
}

impl<Keys: VaultKeyProvider> VaultExpertBindingReviewRepository<Keys> {
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

    fn authorize_pinned_actor(&self) -> Result<(), AgentFailure> {
        self.vault.check_access()?;
        self.actor.validate()?;
        if self.actor.person_id != self.vault.person_id() {
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

    async fn read_review(
        &self,
        connection: &turso::Connection,
        reference: &BindingReviewRef,
    ) -> Result<BindingReviewDescriptor, AgentFailure> {
        read_binding_review_on(
            connection,
            self.vault.person_id(),
            &self.actor.device_id,
            reference,
            self.vault.registry_instance_id(),
        )
        .await
    }

    async fn read_replacement_by_command(
        &self,
        connection: &turso::Connection,
        command_id: CommandId,
    ) -> Result<Option<BindingReplacementReceipt>, AgentFailure> {
        read_binding_replacement_by_command_on(
            connection,
            self.vault.person_id(),
            &self.actor.device_id,
            command_id,
            self.vault.registry_instance_id(),
        )
        .await
    }
}

impl<Keys: VaultKeyProvider> BindingReviewRepository for VaultExpertBindingReviewRepository<Keys> {
    fn find_prepare<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<BindingReviewDescriptor>, AgentFailure>> {
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
                if admission.family != EXPERT_COMMAND_PREPARE {
                    return Err(AgentFailure::Conflict);
                }
                if admission.person_id != actor.person_id || admission.device_id != actor.device_id
                {
                    return Err(AgentFailure::CapabilityDenied);
                }
                let review_id = admission.review_id.ok_or(AgentFailure::VaultUnavailable)?;
                let reference = BindingReviewRef {
                    id: review_id,
                    digest: admission.request_digest,
                };
                let descriptor = self.read_review(&connection, &reference).await?;
                if descriptor.identity.command_id != command_id {
                    return Err(AgentFailure::VaultUnavailable);
                }
                Ok(Some(descriptor))
            }
            .await;
            self.after_access(result)
        })
    }

    fn prepare<'a>(
        &'a self,
        descriptor: BindingReviewDescriptor,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<BindingReviewDescriptor, AgentFailure>> {
        Box::pin(async move {
            self.authorize_pinned_actor()?;
            verify_prepare_identity(
                &descriptor,
                self.vault.person_id(),
                &self.actor,
                self.vault.registry_instance_id(),
            )?;
            let result = async {
                let mut connection = self.vault.connection()?;
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .await
                    .map_err(|error| self.vault.registry_transaction_start_error(error))?;
                let result = async {
                    ensure_expert_binding_tables_on(&transaction).await?;
                    if let Some(admission) =
                        read_expert_command_admission_on(&transaction, descriptor.identity.command_id)
                            .await?
                    {
                        if admission.family != EXPERT_COMMAND_PREPARE
                            || admission.person_id != descriptor.identity.person_id
                            || admission.device_id != descriptor.identity.device_id
                        {
                            return Err(AgentFailure::Conflict);
                        }
                        let review_id =
                            admission.review_id.ok_or(AgentFailure::VaultUnavailable)?;
                        let reference = BindingReviewRef {
                            id: review_id,
                            digest: admission.request_digest,
                        };
                        let stored = self.read_review(&transaction, &reference).await?;
                        if stored.identity != descriptor.identity {
                            return Err(AgentFailure::Conflict);
                        }
                        return Ok(stored);
                    }

                    ensure_expert_binding_tables_on(&transaction).await?;
                    floe_experts::validate_binding_review_descriptor(&descriptor)?;
                    let payload = serde_json::to_string(&descriptor)
                        .map_err(|_| AgentFailure::InvalidInput)?;
                    if payload.len() > MAX_REVIEW_PAYLOAD_BYTES {
                        return Err(AgentFailure::BudgetExceeded);
                    }
                    if count_expert_command_admissions_on(&transaction).await?
                        >= EXPERT_COMMAND_LIMIT
                    {
                        return Err(AgentFailure::BudgetExceeded);
                    }
                    insert_expert_command_admission_on(
                        &transaction,
                        descriptor.identity.command_id,
                        EXPERT_COMMAND_PREPARE,
                        descriptor.identity.person_id,
                        &descriptor.identity.device_id,
                        descriptor.review_ref.digest,
                        Some(descriptor.review_ref.id),
                    )
                    .await?;
                    transaction
                        .execute(
                            "INSERT INTO agent_expert_binding_reviews (review_id, person_id, device_id, command_id, assignment_id, requirement_key, review_digest, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                            (
                                descriptor.review_ref.id.to_string(),
                                descriptor.identity.person_id.to_string(),
                                descriptor.identity.device_id.as_str(),
                                descriptor.identity.command_id.as_uuid().to_string(),
                                descriptor.identity.assignment_id.to_string(),
                                descriptor.identity.requirement_key.as_str(),
                                hex_digest(&descriptor.review_ref.digest),
                                payload,
                            ),
                        )
                        .await
                        .map_err(crate::vault::expert_binding_reviews::map_unique_conflict)?;
                    self.vault.check_access()?;
                    Ok(descriptor)
                }
                .await;
                self.finish_transaction(transaction, result).await
            }
            .await;
            self.after_access(result)
        })
    }

    fn get<'a>(
        &'a self,
        actor: &'a OwnerActor,
        reference: BindingReviewRef,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<BindingReviewDescriptor, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            reference.validate()?;
            let result = async {
                let connection = self.vault.connection()?;
                self.read_review(&connection, &reference).await
            }
            .await;
            self.after_access(result)
        })
    }

    fn find_replacement<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<BindingReplacementReceipt>, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            if !command_id.is_valid() {
                return Err(AgentFailure::InvalidInput);
            }
            let result = async {
                let connection = self.vault.connection()?;
                self.read_replacement_by_command(&connection, command_id)
                    .await
            }
            .await;
            self.after_access(result)
        })
    }

    fn find_review_replacement<'a>(
        &'a self,
        actor: &'a OwnerActor,
        reference: BindingReviewRef,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<BindingReplacementReceipt>, AgentFailure>> {
        Box::pin(async move {
            self.authorize(actor)?;
            reference.validate()?;
            let result = async {
                let connection = self.vault.connection()?;
                let descriptor = self.read_review(&connection, &reference).await?;
                let receipt = read_binding_replacement_for_review_on(
                    &connection,
                    self.vault.person_id(),
                    &self.actor.device_id,
                    &descriptor,
                    self.vault.registry_instance_id(),
                )
                .await?;
                Ok(receipt)
            }
            .await;
            self.after_access(result)
        })
    }

    fn commit_replacement<'a>(
        &'a self,
        replacement: ReviewedBindingReplacement,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<BindingReplacementReceipt, AgentFailure>> {
        Box::pin(async move {
            self.authorize(&replacement.registry.actor)?;
            let result = async {
                let command_id = replacement.registry.command_id;
                if !command_id.is_valid() || replacement.registry.request_digest == [0; 32] {
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
                        read_expert_command_admission_on(&transaction, command_id).await?
                    {
                        if admission.family != EXPERT_COMMAND_REPLACEMENT
                            || admission.person_id != replacement.registry.actor.person_id
                            || admission.device_id != replacement.registry.actor.device_id
                            || admission.request_digest != replacement.registry.request_digest
                            || admission.review_id != Some(replacement.review_ref.id)
                        {
                            return Err(AgentFailure::Conflict);
                        }
                        let receipt = self
                            .read_replacement_by_command(&transaction, command_id)
                            .await?
                            .ok_or(AgentFailure::VaultUnavailable)?;
                        if receipt.review_ref != replacement.review_ref {
                            return Err(AgentFailure::Conflict);
                        }
                        let descriptor = self.read_review(&transaction, &receipt.review_ref).await?;
                        if replacement.expected_binding_revision
                            != descriptor.identity.expected_binding_revision
                        {
                            return Err(AgentFailure::Conflict);
                        }
                        crate::vault::expert_binding_reviews::verify_binding_candidate_selection(
                            &descriptor,
                            &replacement.candidate_refs,
                            &receipt.registry.snapshot,
                        )?;
                        return Ok(receipt);
                    }

                    if count_expert_command_admissions_on(&transaction).await?
                        >= EXPERT_COMMAND_LIMIT
                    {
                        return Err(AgentFailure::BudgetExceeded);
                    }
                    replacement.review_ref.validate()?;
                    let descriptor = self
                        .read_review(&transaction, &replacement.review_ref)
                        .await?;
                    if replacement.expected_binding_revision
                        != descriptor.identity.expected_binding_revision
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    if replacement.committed_at_unix_ms < descriptor.created_at_unix_ms {
                        return Err(AgentFailure::InvalidInput);
                    }
                    if replacement.committed_at_unix_ms >= descriptor.expires_at_unix_ms {
                        return Err(AgentFailure::DeadlineExceeded);
                    }
                    if read_binding_replacement_for_review_on(
                        &transaction,
                        self.vault.person_id(),
                        &self.actor.device_id,
                        &descriptor,
                        self.vault.registry_instance_id(),
                    )
                    .await?
                    .is_some()
                    {
                        return Err(AgentFailure::Conflict);
                    }

                    let previous = self
                        .vault
                        .registry_on(&transaction)
                        .await?
                        .ok_or(AgentFailure::NotFound)?;
                    if previous.revision != replacement.registry.expected_revision {
                        return Err(AgentFailure::Conflict);
                    }
                    let next_revision = replacement
                        .registry
                        .expected_revision
                        .checked_add(1)
                        .ok_or(AgentFailure::BudgetExceeded)?;
                    if replacement.registry.next.revision != next_revision {
                        return Err(AgentFailure::Conflict);
                    }
                    validate_registry_snapshot(
                        &replacement.registry.next,
                        self.vault.registry_instance_id(),
                        self.vault.person_id(),
                    )?;
                    validate_binding_registry_successor(
                        &previous,
                        &replacement.registry.next,
                        &descriptor,
                        &replacement.candidate_refs,
                        command_id,
                    )?;
                    let registry_payload = self.vault.registry_payload(&replacement.registry.next)?;

                    let receipt = BindingReplacementReceipt {
                        review_ref: descriptor.review_ref.clone(),
                        registry: floe_experts::RegistryCommitReceipt {
                            command_id,
                            person_id: replacement.registry.actor.person_id,
                            device_id: replacement.registry.actor.device_id.clone(),
                            request_digest: replacement.registry.request_digest,
                            snapshot: replacement.registry.next.clone(),
                        },
                        committed_at_unix_ms: replacement.committed_at_unix_ms,
                    };
                    floe_experts::project_binding_mutation_receipt(&receipt, &descriptor)
                        .map_err(|_| AgentFailure::InvalidInput)?;
                    let payload = serde_json::to_string(&receipt)
                        .map_err(|_| AgentFailure::StorageUnavailable)?;
                    if payload.len() > MAX_REPLACEMENT_RECEIPT_BYTES {
                        return Err(AgentFailure::BudgetExceeded);
                    }

                    self.vault
                        .update_registry(
                            &transaction,
                            replacement.registry.expected_revision,
                            replacement.registry.next.revision,
                            registry_payload,
                        )
                        .await?;
                    insert_expert_command_admission_on(
                        &transaction,
                        command_id,
                        EXPERT_COMMAND_REPLACEMENT,
                        replacement.registry.actor.person_id,
                        &replacement.registry.actor.device_id,
                        replacement.registry.request_digest,
                        Some(descriptor.review_ref.id),
                    )
                    .await?;
                    transaction
                        .execute(
                            "INSERT INTO agent_expert_binding_review_consumptions (review_id, command_id, person_id, device_id, review_digest, request_digest, committed_at_unix_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
                            (
                                descriptor.review_ref.id.to_string(),
                                command_id.as_uuid().to_string(),
                                descriptor.identity.person_id.to_string(),
                                descriptor.identity.device_id.as_str(),
                                hex_digest(&descriptor.review_ref.digest),
                                hex_digest(&replacement.registry.request_digest),
                                replacement.committed_at_unix_ms,
                            ),
                        )
                        .await
                        .map_err(crate::vault::expert_binding_reviews::map_unique_conflict)?;
                    transaction
                        .execute(
                            "INSERT INTO agent_expert_binding_replacement_receipts (command_id, consumed_review_id, person_id, device_id, review_digest, request_digest, committed_at_unix_ms, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                            (
                                command_id.as_uuid().to_string(),
                                descriptor.review_ref.id.to_string(),
                                descriptor.identity.person_id.to_string(),
                                descriptor.identity.device_id.as_str(),
                                hex_digest(&descriptor.review_ref.digest),
                                hex_digest(&replacement.registry.request_digest),
                                replacement.committed_at_unix_ms,
                                payload,
                            ),
                        )
                        .await
                        .map_err(crate::vault::expert_binding_reviews::map_unique_conflict)?;
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
