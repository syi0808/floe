use std::sync::Arc;

use crate::{EncryptedAgentVault, VaultKeyProvider};
use floe_agent_contract::{AgentFailure, BoxFuture, ExecutionScope, OwnerActor};
use floe_experts::{
    AgentRegistry, ExpertCommandLookup, RegistryCommit, RegistryCommitIntent,
    RegistryCommitReceipt, RegistryRepository,
};
use floe_kernel::CommandFailure;
use turso::transaction::TransactionBehavior;

use crate::vault::expert_binding_reviews::{
    EXPERT_COMMAND_LIMIT, EXPERT_COMMAND_REGISTRY, MAX_EXPERT_RECEIPT_PAYLOAD_BYTES,
    assistant_feature_task_review_receipt_on, count_expert_command_admissions_on,
    ensure_expert_binding_tables_on, insert_expert_command_admission_on, map_unique_conflict,
    read_expert_command_admission_on, read_registry_receipt_on, validate_registry_receipt,
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

    async fn finish_command_transaction<T>(
        &self,
        transaction: turso::transaction::Transaction<'_>,
        result: Result<T, CommandFailure<AgentFailure>>,
    ) -> Result<T, CommandFailure<AgentFailure>> {
        let result = self
            .vault
            .finish_registry_command_transaction(transaction, result)
            .await;
        match (self.vault.check_access(), result) {
            (Ok(()), result) => result,
            (Err(failure), Ok(_)) => Err(CommandFailure::Admitted(failure)),
            (Err(_), Err(failure)) => Err(failure),
        }
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
    ) -> BoxFuture<'a, Result<ExpertCommandLookup<RegistryCommitReceipt>, AgentFailure>> {
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
                    return Ok(ExpertCommandLookup::Absent);
                };
                if admission.family != EXPERT_COMMAND_REGISTRY {
                    return Ok(ExpertCommandLookup::Occupied);
                }
                if admission.person_id != actor.person_id || admission.device_id != actor.device_id
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
                Ok(ExpertCommandLookup::Existing(receipt))
            }
            .await;
            self.after_access(result)
        })
    }

    fn commit<'a>(
        &'a self,
        commit: RegistryCommit,
        _scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<RegistryCommitReceipt, CommandFailure<AgentFailure>>> {
        Box::pin(async move {
            self.authorize(&commit.actor)
                .map_err(CommandFailure::NotAdmitted)?;
            if !commit.command_id.is_valid() {
                return Err(CommandFailure::NotApplied(AgentFailure::InvalidInput));
            }
            let mut connection = self
                .vault
                .connection()
                .map_err(CommandFailure::Indeterminate)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(|error| {
                    CommandFailure::Indeterminate(
                        self.vault.registry_transaction_start_error(error),
                    )
                })?;
            let mut prior_command = false;
            let mut replay_checked = false;
            let result = async {
                    ensure_expert_binding_tables_on(&transaction).await?;
                    let occupant =
                        read_expert_command_admission_on(&transaction, commit.command_id).await?;
                    if let Some(admission) = occupant {
                        prior_command = true;
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
                    replay_checked = true;
                    if commit.request_digest == [0; 32] {
                        return Err(AgentFailure::InvalidInput);
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
                                commit.command_id,
                                commit.request_digest,
                                &commit.intent,
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
                                commit.command_id,
                                commit.request_digest,
                                &commit.intent,
                            )?;
                            self.vault
                                .install_expert_registry_on(&transaction, &commit.next)
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
                                commit.actor.device_id.as_str(),
                                hex_digest(&commit.request_digest),
                                to_i64(receipt.snapshot.revision)?,
                                payload,
                            ),
                        )
                        .await
                        .map_err(map_unique_conflict)?;
                    if let RegistryCommitIntent::ConfigureAssistantFeature(configuration) =
                        &commit.intent
                    {
                        if let Some(binding_receipt) =
                            assistant_feature_task_review_receipt_on(
                                &transaction,
                                &commit.actor,
                                configuration,
                                receipt.clone(),
                            )
                            .await?
                        {
                            let binding_payload = serde_json::to_string(&binding_receipt)
                                .map_err(|_| AgentFailure::StorageUnavailable)?;
                            if binding_payload.len() > MAX_EXPERT_RECEIPT_PAYLOAD_BYTES {
                                return Err(AgentFailure::BudgetExceeded);
                            }
                            transaction
                                .execute(
                                    "INSERT INTO agent_expert_binding_review_consumptions (review_id, command_id, person_id, device_id, review_digest, request_digest, committed_at_unix_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
                                    (
                                        binding_receipt.review_ref.id.to_string(),
                                        commit.command_id.as_uuid().to_string(),
                                        commit.actor.person_id.to_string(),
                                        commit.actor.device_id.as_str(),
                                        hex_digest(&binding_receipt.review_ref.digest),
                                        hex_digest(&commit.request_digest),
                                        binding_receipt.committed_at_unix_ms,
                                    ),
                                )
                                .await
                                .map_err(map_unique_conflict)?;
                            transaction
                                .execute(
                                    "INSERT INTO agent_expert_binding_replacement_receipts (command_id, consumed_review_id, person_id, device_id, review_digest, request_digest, committed_at_unix_ms, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                                    (
                                        commit.command_id.as_uuid().to_string(),
                                        binding_receipt.review_ref.id.to_string(),
                                        commit.actor.person_id.to_string(),
                                        commit.actor.device_id.as_str(),
                                        hex_digest(&binding_receipt.review_ref.digest),
                                        hex_digest(&commit.request_digest),
                                        binding_receipt.committed_at_unix_ms,
                                        binding_payload,
                                    ),
                                )
                                .await
                                .map_err(map_unique_conflict)?;
                        }
                    }
                    self.vault.check_access()?;
                    Ok(receipt)
                }
                .await;
            let result = result.map_err(|failure| {
                if prior_command {
                    CommandFailure::Indeterminate(failure)
                } else if replay_checked {
                    CommandFailure::NotApplied(failure)
                } else {
                    CommandFailure::Indeterminate(failure)
                }
            });
            self.finish_command_transaction(transaction, result).await
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

#[cfg(all(test, unix))]
mod tests {
    use std::{
        collections::HashMap,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use floe_agent_contract::{
        CommandId, OwnerActor, TaskExecutionKey, TaskExecutionReceiptRef, TaskId,
    };
    use floe_context_contract::{
        ConnectionId, ConnectorId, ExecutionOwnerId, ResourceHandle, SourceAuthority,
        SourceSelectionReference,
    };
    use floe_execution::{
        Cancellation, ExecutionScope,
        budget::{BudgetConfig, BudgetLedger, ModelUsage},
    };
    use floe_experts::{
        AgentRegistry, AssistantFeatureBindingChange, AssistantFeatureConfiguration,
        BindingPrepareIdentity, BindingReviewDescriptor, BindingReviewRef, BindingReviewRepository,
        Candidate, CandidateAvailability, CandidateSourceExpectation, ExpertInstallOperation,
        ExpertSourceRequirement, PackageRef, RegistryCommit, RegistryCommitIntent,
        RegistryRepository, ReviewedCandidate, binding_review_digest,
    };
    use floe_kernel::{AgentFailure, CommandFailure, PersonId, TraceContext};
    use sha2::{Digest, Sha256};
    use tokio::time::Instant;
    use uuid::Uuid;

    use super::*;
    use crate::{RootKey, VaultExpertBindingReviewRepository, VaultKeyProvider};

    #[derive(Default)]
    struct TestKeys(Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>);

    impl VaultKeyProvider for TestKeys {
        fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<RootKey, AgentFailure> {
            self.0
                .lock()
                .map_err(|_| AgentFailure::VaultUnavailable)?
                .get(&(person_id, vault_id))
                .copied()
                .map(RootKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
            key: &RootKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .map_err(|_| AgentFailure::VaultUnavailable)?
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("floe-feature-rollback-{}", Uuid::new_v4()));
            std::fs::create_dir(&path).expect("create isolated Vault root");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("restrict isolated Vault root");
            Self(path)
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn multi_selection_failure_rolls_back_registry_and_linked_receipts() {
        let root = TestRoot::new();
        let person_id = PersonId::new();
        let actor = OwnerActor {
            person_id,
            device_id: "device-feature-test".into(),
            runtime_epoch: 1,
        };
        let vault = Arc::new(
            EncryptedAgentVault::create(&root.0, person_id, TestKeys::default())
                .await
                .expect("create a real encrypted Vault for the transaction test"),
        );
        let registry_repository =
            VaultExpertRegistryRepository::new(vault.clone(), actor.clone()).unwrap();
        let review_repository =
            VaultExpertBindingReviewRepository::new(vault.clone(), actor.clone()).unwrap();
        let instance_id = vault.registry_instance_id();
        let manifests = floe_experts_builtin::manifests();
        let manifest = manifests
            .iter()
            .find(|manifest| manifest.package.id == "floe.builtin.commitments")
            .expect("built-in Commitments manifest");
        let mut registry = AgentRegistry::new(instance_id);
        registry
            .install_bundle(
                person_id,
                &ExpertInstallOperation {
                    instance_id,
                    expected_revision: 0,
                    operation_id: Uuid::new_v4(),
                },
                &manifests,
            )
            .expect("install built-in Experts in the in-memory registry");
        let before = registry.snapshot();
        let installation = before
            .installations
            .iter()
            .find(|installation| installation.package.id == manifest.package.id)
            .expect("Commitments installation");
        let assignment = before
            .assignments
            .iter()
            .find(|assignment| {
                assignment.person_id == person_id && assignment.installation_id == installation.id
            })
            .expect("Commitments assignment");
        let connection = vault.connection().unwrap();
        vault
            .install_expert_registry_on(&connection, &before)
            .await
            .expect("persist the starting registry in Vault");
        drop(connection);

        let scope = test_scope();
        let requirements = manifest
            .source_requirements
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            requirements.len(),
            2,
            "Commitments has multiple source choices"
        );
        let first = prepare_review(
            &review_repository,
            &actor,
            assignment.id,
            installation.id,
            instance_id,
            &manifest.package,
            manifest.definition.definition_revision,
            requirements[0].clone(),
            true,
            0,
            &scope,
        )
        .await;
        let second = prepare_review(
            &review_repository,
            &actor,
            assignment.id,
            installation.id,
            instance_id,
            &manifest.package,
            manifest.definition.definition_revision,
            requirements[1].clone(),
            false,
            1,
            &scope,
        )
        .await;

        let first_candidate = first.candidates[0].clone();
        let second_candidate = second.candidates[0].clone();
        let second_invalid_candidate_ref = Uuid::new_v4();
        let configuration = AssistantFeatureConfiguration {
            installation_id: installation.id,
            expected_revision: before.revision,
            enabled: false,
            committed_at_unix_ms: 1_000,
            binding_changes: vec![
                AssistantFeatureBindingChange {
                    assignment_id: assignment.id,
                    requirement_key: requirements[0].key.clone(),
                    review_ref: first.review_ref.clone(),
                    expected_binding_revision: assignment.binding.revision,
                    candidate_refs: vec![first_candidate.candidate_ref],
                    selected: vec![first_candidate.candidate.reference.clone()],
                },
                AssistantFeatureBindingChange {
                    assignment_id: assignment.id,
                    requirement_key: requirements[1].key.clone(),
                    review_ref: second.review_ref.clone(),
                    expected_binding_revision: assignment.binding.revision,
                    candidate_refs: vec![second_invalid_candidate_ref],
                    selected: vec![second_candidate.candidate.reference.clone()],
                },
            ],
        };
        let command_id = CommandId::new();
        let request_digest = [0x42; 32];
        let mut next = AgentRegistry::restore(before.clone(), instance_id).unwrap();
        next.configure_assistant_feature(
            before.revision,
            command_id.as_uuid(),
            request_digest,
            &configuration,
            person_id,
        )
        .expect("registry accepts the two source changes before Vault verifies reviews");
        let result = registry_repository
            .commit(
                RegistryCommit {
                    actor: actor.clone(),
                    command_id,
                    request_digest,
                    expected_revision: before.revision,
                    intent: RegistryCommitIntent::ConfigureAssistantFeature(configuration),
                    next: next.snapshot(),
                },
                &scope,
            )
            .await;
        assert!(matches!(
            result,
            Err(CommandFailure::NotApplied(AgentFailure::InvalidInput))
        ));

        let after = registry_repository.read(&actor, &scope).await.unwrap();
        assert_eq!(
            after, before,
            "the installation toggle and both bindings roll back"
        );
        assert!(matches!(
            registry_repository
                .find_command(&actor, command_id, &scope)
                .await
                .unwrap(),
            floe_experts::ExpertCommandLookup::Absent
        ));
        assert!(
            review_repository
                .find_review_replacement(&actor, first.review_ref.clone(), &scope)
                .await
                .unwrap()
                .is_none()
        );

        let connection = vault.connection().unwrap();
        assert_eq!(
            count_rows(
                &connection,
                "SELECT count(*) FROM agent_expert_binding_review_consumptions WHERE review_id = ?",
                first.review_ref.id.to_string(),
            )
            .await,
            0,
            "task-origin review consumption rolls back",
        );
        assert_eq!(
            count_rows(
                &connection,
                "SELECT count(*) FROM agent_expert_binding_replacement_receipts WHERE command_id = ?",
                command_id.as_uuid().to_string(),
            )
            .await,
            0,
            "linked replacement receipt rolls back",
        );
        assert_eq!(
            count_rows(
                &connection,
                "SELECT count(*) FROM agent_expert_registry_receipts WHERE command_id = ?",
                command_id.as_uuid().to_string(),
            )
            .await,
            0,
            "registry receipt rolls back",
        );
        assert_eq!(
            count_rows(
                &connection,
                "SELECT count(*) FROM agent_expert_command_admissions WHERE command_id = ?",
                command_id.as_uuid().to_string(),
            )
            .await,
            0,
            "command admission rolls back",
        );
    }

    async fn prepare_review(
        repository: &VaultExpertBindingReviewRepository<TestKeys>,
        actor: &OwnerActor,
        assignment_id: Uuid,
        installation_id: Uuid,
        registry_instance_id: Uuid,
        package: &PackageRef,
        definition_revision: u64,
        requirement: ExpertSourceRequirement,
        task_origin: bool,
        index: u8,
        scope: &ExecutionScope,
    ) -> BindingReviewDescriptor {
        let reference = SourceSelectionReference {
            connector_id: ConnectorId::try_new(format!("test.connector.{index}")).unwrap(),
            connection_id: ConnectionId::new(),
            execution_owner_id: ExecutionOwnerId::try_new(format!("test.owner.{index}")).unwrap(),
            capability_id: requirement.capability.clone(),
            resource: ResourceHandle::try_new(format!("resource.{index}")).unwrap(),
            contract_version: requirement.contract_version,
        };
        let task_origin = task_origin.then(|| TaskExecutionReceiptRef {
            execution: TaskExecutionKey {
                task_id: TaskId::new(),
                execution_id: Uuid::new_v4(),
                executor_generation: 1,
            },
            task_revision: 2,
            journal_revision: 0,
            digest: [0x55; 32],
        });
        let identity = BindingPrepareIdentity {
            command_id: CommandId::new(),
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            assignment_id,
            requirement_key: requirement.key.clone(),
            expected_binding_revision: 1,
            task_origin,
        };
        let review_id = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            &serde_json::to_vec(&("floe.expert-binding-review-id.v1", &identity)).unwrap(),
        );
        let candidate_ref = Uuid::new_v5(&review_id, &serde_json::to_vec(&reference).unwrap());
        let candidate_id = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&("floe.source-selection-candidate.sha256.v1", &reference))
                    .unwrap(),
            )
        );
        let candidate = Candidate {
            candidate_id,
            label: format!("Candidate {index}"),
            detail: "Reviewed source".into(),
            availability: CandidateAvailability::Available,
            reference: reference.clone(),
        };
        let mut descriptor = BindingReviewDescriptor {
            review_ref: BindingReviewRef {
                id: review_id,
                digest: [0; 32],
            },
            identity,
            registry_instance_id,
            installation_id,
            package: package.clone(),
            definition_revision,
            requirement,
            candidates: vec![ReviewedCandidate {
                candidate_ref,
                candidate,
                selected: false,
            }],
            catalog_revision: definition_revision,
            catalog_digest: [0x33; 32],
            source_expectations: vec![CandidateSourceExpectation {
                reference,
                source_revision: 1,
                source_authority: SourceAuthority::new(),
            }],
            created_at_unix_ms: 10,
            expires_at_unix_ms: 600_010,
        };
        descriptor.review_ref.digest = binding_review_digest(&descriptor).unwrap();
        floe_experts::validate_binding_review_descriptor(&descriptor).unwrap();
        repository
            .prepare(descriptor, scope)
            .await
            .expect("store a valid reviewed source in the encrypted Vault")
    }

    fn test_scope() -> ExecutionScope {
        let budget = BudgetLedger::new(BudgetConfig::new(64, 64), ModelUsage::default());
        ExecutionScope::root(
            Cancellation::new(),
            Instant::now() + Duration::from_secs(30),
            budget.root_lease(),
            TraceContext::new(Uuid::new_v4()),
        )
    }

    async fn count_rows(connection: &turso::Connection, sql: &str, key: String) -> i64 {
        let mut rows = connection.query(sql, (key,)).await.unwrap();
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
    }
}
