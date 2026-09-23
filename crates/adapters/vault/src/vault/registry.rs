use floe_access::DependencyCoverage;
use floe_agent_contract::{TaskId, TaskSnapshot, TaskState};
use floe_conversation::AgentMessage;
use floe_experts::{AgentRegistry, ExpertResult, RegistrySnapshot};
use turso::transaction::TransactionBehavior;

use super::tasks::VaultTaskRecord;
use super::*;

const MAX_REGISTRY_BYTES: usize = 262_144;

fn expert_identity_matches(
    current: &RegistrySnapshot,
    staged: &RegistrySnapshot,
    result: &ExpertResult,
) -> bool {
    let current_assignment = current
        .assignments
        .iter()
        .find(|assignment| assignment.id == result.assignment_id);
    let staged_assignment = staged
        .assignments
        .iter()
        .find(|assignment| assignment.id == result.assignment_id);
    let (Some(current_assignment), Some(staged_assignment)) =
        (current_assignment, staged_assignment)
    else {
        return false;
    };
    if current_assignment.person_id != staged_assignment.person_id
        || current_assignment.installation_id != staged_assignment.installation_id
        || current_assignment.enabled != staged_assignment.enabled
        || current_assignment.granted_tool_assignments != staged_assignment.granted_tool_assignments
    {
        return false;
    }
    let current_installation = current
        .installations
        .iter()
        .find(|installation| installation.id == current_assignment.installation_id);
    let staged_installation = staged
        .installations
        .iter()
        .find(|installation| installation.id == staged_assignment.installation_id);
    let (Some(current_installation), Some(staged_installation)) =
        (current_installation, staged_installation)
    else {
        return false;
    };
    if current_installation.package != staged_installation.package
        || current_installation.enabled != staged_installation.enabled
    {
        return false;
    }
    // Evidence identity is source-observation based and validated by
    // Access/Context, never by Registry view state.
    !result.evidence_id.is_nil()
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn builtin_expert_overview(
        &self,
    ) -> Result<Option<floe_experts::BuiltinExpertSetupResult>, AgentFailure> {
        let Some(snapshot) = self.expert_registry().await? else {
            return Ok(None);
        };
        let registry = AgentRegistry::restore(snapshot, self.vault_id)?;
        let Some(setup) = registry
            .snapshot()
            .builtin_setups
            .iter()
            .find(|setup| setup.person_id == self.person_id)
            .cloned()
        else {
            return Ok(None);
        };
        self.check_access()?;
        Ok(Some(floe_experts::BuiltinExpertSetupResult {
            setup,
            registry: registry.overview(self.person_id),
        }))
    }

    pub async fn install_builtin_experts(
        &self,
        request: floe_experts::BuiltinExpertSetup,
        specs: &[floe_experts::ExpertSetupSpec],
        cancellation: floe_execution::Cancellation,
    ) -> Result<floe_experts::BuiltinExpertSetupResult, AgentFailure> {
        let check = || {
            cancellation
                .is_cancelled()
                .then_some(AgentFailure::Cancelled)
                .map_or(Ok(()), Err)
        };
        check()?;
        if request.instance_id != self.vault_id {
            return Err(AgentFailure::NotFound);
        }
        let previous = self.expert_registry().await?;
        let mut registry = match &previous {
            Some(snapshot) => AgentRegistry::restore(snapshot.clone(), self.vault_id)?,
            None => AgentRegistry::new(self.vault_id),
        };
        let revision = registry.revision();
        let setup = registry.install_builtin_experts(self.person_id, &request, specs)?;
        if registry.revision() != revision {
            match previous {
                Some(_) => {
                    self.save_expert_registry_checked(revision, &registry.snapshot(), &check)
                        .await?
                }
                None => {
                    self.initialize_expert_registry_checked(&registry.snapshot(), &check)
                        .await?
                }
            }
        }
        self.check_access()?;
        check()?;
        Ok(floe_experts::BuiltinExpertSetupResult {
            setup,
            registry: registry.overview(self.person_id),
        })
    }

    pub async fn install_builtin_experts_enabled(
        &self,
        request: floe_experts::BuiltinExpertSetup,
        specs: &[floe_experts::ExpertSetupSpec],
        cancellation: floe_execution::Cancellation,
    ) -> Result<floe_experts::BuiltinExpertSetupResult, AgentFailure> {
        let check = || {
            cancellation
                .is_cancelled()
                .then_some(AgentFailure::Cancelled)
                .map_or(Ok(()), Err)
        };
        check()?;
        if request.instance_id != self.vault_id {
            return Err(AgentFailure::NotFound);
        }
        let previous = self.expert_registry().await?;
        let mut registry = match &previous {
            Some(snapshot) => AgentRegistry::restore(snapshot.clone(), self.vault_id)?,
            None => AgentRegistry::new(self.vault_id),
        };
        let revision = registry.revision();
        let setup = registry.install_builtin_experts_enabled(self.person_id, &request, specs)?;
        if registry.revision() != revision {
            match previous {
                Some(_) => {
                    self.save_expert_registry_change_checked(
                        revision,
                        &registry.snapshot(),
                        Some(setup.setup_id),
                        None,
                        None,
                        &check,
                    )
                    .await?;
                }
                None => {
                    self.initialize_expert_registry_checked(&registry.snapshot(), &check)
                        .await?
                }
            }
        }
        self.check_access()?;
        check()?;
        Ok(floe_experts::BuiltinExpertSetupResult {
            setup,
            registry: registry.overview(self.person_id),
        })
    }

    pub async fn enabled_expert_cards(&self) -> Result<Vec<floe_experts::AgentCard>, AgentFailure> {
        let cards = match self.expert_registry().await? {
            Some(snapshot) => AgentRegistry::restore(snapshot, self.vault_id)?
                .enabled_expert_cards(self.person_id),
            None => vec![],
        };
        self.check_access()?;
        Ok(cards)
    }

    pub async fn enabled_builtin_expert_cards(
        &self,
    ) -> Result<Vec<floe_experts::AgentCard>, AgentFailure> {
        let cards = match self.expert_registry().await? {
            Some(snapshot) => AgentRegistry::restore(snapshot, self.vault_id)?
                .enabled_builtin_expert_cards(self.person_id),
            None => vec![],
        };
        self.check_access()?;
        Ok(cards)
    }

    pub async fn registry_overview(
        &self,
    ) -> Result<Option<floe_experts::RegistryOverview>, AgentFailure> {
        let overview = self
            .expert_registry()
            .await?
            .map(|snapshot| {
                AgentRegistry::restore(snapshot, self.vault_id)
                    .map(|registry| registry.overview(self.person_id))
            })
            .transpose()?;
        self.check_access()?;
        Ok(overview)
    }

    pub async fn configure_registry(
        &self,
        configuration: floe_experts::RegistryConfiguration,
        cancellation: floe_execution::Cancellation,
    ) -> Result<floe_experts::RegistryOverview, AgentFailure> {
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if configuration.instance_id != self.vault_id {
            return Err(AgentFailure::NotFound);
        }
        let snapshot = self
            .expert_registry()
            .await?
            .ok_or(AgentFailure::NotFound)?;
        let mut registry = AgentRegistry::restore(snapshot, self.vault_id)?;
        match configuration.target {
            floe_experts::RegistryConfigurationTarget::Installation { id, enabled } => {
                registry.set_installation_enabled(configuration.expected_revision, id, enabled)?
            }
            floe_experts::RegistryConfigurationTarget::Assignment { id, enabled } => registry
                .set_assignment_enabled(
                    configuration.expected_revision,
                    self.person_id,
                    id,
                    enabled,
                )?,
        }
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        self.save_expert_registry_checked(
            configuration.expected_revision,
            &registry.snapshot(),
            || {
                if cancellation.is_cancelled() {
                    Err(AgentFailure::Cancelled)
                } else {
                    Ok(())
                }
            },
        )
        .await?;
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        Ok(registry.overview(self.person_id))
    }

    pub fn registry_instance_id(&self) -> Uuid {
        self.vault_id
    }

    pub async fn expert_registry(&self) -> Result<Option<RegistrySnapshot>, AgentFailure> {
        self.registry_on(&self.connection()?).await
    }

    pub async fn initialize_expert_registry(
        &self,
        snapshot: &RegistrySnapshot,
    ) -> Result<(), AgentFailure> {
        self.initialize_expert_registry_checked(snapshot, || Ok(()))
            .await
    }

    pub async fn initialize_expert_registry_checked(
        &self,
        snapshot: &RegistrySnapshot,
        check: impl Fn() -> Result<(), AgentFailure> + Sync,
    ) -> Result<(), AgentFailure> {
        check()?;
        let payload = self.registry_payload(snapshot)?;
        if snapshot
            .assignments
            .iter()
            .any(|assignment| assignment.private_state.revision != 0)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            if self.registry_on(&transaction).await?.is_some() { return Err(AgentFailure::Conflict); }
            transaction.execute("CREATE TABLE agent_expert_registry (id INTEGER PRIMARY KEY CHECK (id = 1), revision INTEGER NOT NULL, payload TEXT NOT NULL)", ()).await.map_err(storage)?;
            transaction.execute("CREATE TABLE agent_expert_receipts (invocation_id TEXT PRIMARY KEY, session_id TEXT NOT NULL, assignment_id TEXT NOT NULL, registry_revision INTEGER NOT NULL)", ()).await.map_err(storage)?;
            transaction.execute("INSERT INTO agent_expert_registry VALUES (1, ?, ?)",
                (integer(snapshot.revision)?, payload)).await.map_err(storage)?;
            let changed = transaction.execute("UPDATE vault_identity SET version = 2 WHERE id = 1 AND version = 1", ()).await.map_err(storage)?;
            if changed != 1 { return Err(AgentFailure::Conflict); }
            self.check_access()?;
            check()?;
            Ok(())
        }.await;
        self.finish_registry_transaction(transaction, result).await
    }

    pub async fn save_expert_registry(
        &self,
        expected_revision: u64,
        snapshot: &RegistrySnapshot,
    ) -> Result<(), AgentFailure> {
        self.save_expert_registry_checked(expected_revision, snapshot, || Ok(()))
            .await
    }

    pub async fn save_expert_registry_checked(
        &self,
        expected_revision: u64,
        snapshot: &RegistrySnapshot,
        check: impl Fn() -> Result<(), AgentFailure> + Sync,
    ) -> Result<(), AgentFailure> {
        self.save_expert_registry_change_checked(
            expected_revision,
            snapshot,
            None,
            None,
            None,
            check,
        )
        .await?;
        Ok(())
    }

    pub async fn settle_expert_task_checked(
        &self,
        completion: floe_experts::ExpertTaskCompletion,
        check: impl Fn() -> Result<(), AgentFailure> + Sync,
    ) -> Result<VaultTaskRecord, AgentFailure> {
        let floe_experts::ExpertTaskCompletion {
            settlement,
            task_id,
            expected_task_revision,
            executor_generation,
            task_snapshot,
        } = completion;
        let coverage = if settlement.dependencies.is_empty() {
            DependencyCoverage::Independent
        } else {
            DependencyCoverage::Dependent {
                dependencies: settlement.dependencies.clone(),
            }
        };
        coverage
            .validate()
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if settlement.assignment_id.is_nil()
            || settlement.invocation_id.is_nil()
            || settlement.owner() != task_snapshot.agent_id
            || task_snapshot.task_id != task_id
            || task_snapshot.principal != self.person_id.to_string()
            || task_snapshot.state != TaskState::Completed
            || task_snapshot.coverage != coverage
            || task_snapshot.result.as_deref() != Some(settlement.task_result.as_str())
            || settlement
                .staged_registry
                .assignments
                .iter()
                .find(|assignment| assignment.id == settlement.assignment_id)
                .is_none_or(|assignment| {
                    assignment.person_id != self.person_id
                        || assignment.private_state.last_invocation_id
                            != Some(settlement.invocation_id)
                        || !settlement
                            .staged_registry
                            .installations
                            .iter()
                            .any(|installation| {
                                installation.id == assignment.installation_id
                                    && installation.package.id == task_snapshot.agent_id
                            })
                })
        {
            return Err(AgentFailure::Conflict);
        }
        self.save_expert_registry_change_checked(
            settlement.expected_registry_revision,
            &settlement.staged_registry,
            None,
            Some(settlement.assignment_id),
            Some((
                task_id,
                expected_task_revision,
                executor_generation,
                &task_snapshot,
            )),
            check,
        )
        .await?
        .ok_or(AgentFailure::StorageUnavailable)
    }

    pub(super) async fn save_expert_registry_change_checked(
        &self,
        expected_revision: u64,
        snapshot: &RegistrySnapshot,
        mutable_builtin_setup: Option<uuid::Uuid>,
        mutable_assignment: Option<uuid::Uuid>,
        task_completion: Option<(TaskId, u64, u64, &TaskSnapshot)>,
        check: impl Fn() -> Result<(), AgentFailure> + Sync,
    ) -> Result<Option<VaultTaskRecord>, AgentFailure> {
        check()?;
        let payload = self.registry_payload(snapshot)?;
        if expected_revision.checked_add(1) != Some(snapshot.revision) {
            return Err(AgentFailure::Conflict);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let previous = self
                .registry_on(&transaction)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if previous.revision != expected_revision {
                return Err(AgentFailure::Conflict);
            }
            if let Some(assignment_id) = mutable_assignment {
                let before = previous
                    .assignments
                    .iter()
                    .find(|assignment| assignment.id == assignment_id)
                    .ok_or(AgentFailure::Conflict)?;
                let after = snapshot
                    .assignments
                    .iter()
                    .find(|assignment| assignment.id == assignment_id)
                    .ok_or(AgentFailure::Conflict)?;
                if before.person_id != after.person_id
                    || before.installation_id != after.installation_id
                    || before.enabled != after.enabled
                    || before.granted_tool_assignments != after.granted_tool_assignments
                    || before.private_state.schema_version != after.private_state.schema_version
                    || before.private_state.revision.checked_add(1)
                        != Some(after.private_state.revision)
                    || before.private_state.completed_invocations.checked_add(1)
                        != Some(after.private_state.completed_invocations)
                    || after.private_state.completed_invocations != after.private_state.revision
                    || after.private_state.last_invocation_id.is_none()
                    || after.private_state.last_invocation_id
                        == before.private_state.last_invocation_id
                {
                    return Err(AgentFailure::Conflict);
                }
                let mut normalized = snapshot.clone();
                normalized.revision = previous.revision;
                normalized
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.id == assignment_id)
                    .ok_or(AgentFailure::Conflict)?
                    .private_state = before.private_state.clone();
                if normalized != previous {
                    return Err(AgentFailure::Conflict);
                }
            }
            let builtin_receipt_allowed =
                |before: &floe_experts::BuiltinExpertSetupReceipt,
                 after: &floe_experts::BuiltinExpertSetupReceipt| {
                    before == after
                        || (mutable_builtin_setup == Some(before.setup_id)
                            && before.setup_id == after.setup_id
                            && before.person_id == after.person_id
                            && before.expected_revision == after.expected_revision
                            && before.assignments.len() == after.assignments.len()
                            && before.assignments.iter().zip(&after.assignments).all(
                                |(before, after)| {
                                    before.expert == after.expert
                                        && before.tool_installation_id == after.tool_installation_id
                                        && before.expert_installation_id
                                            == after.expert_installation_id
                                        && before.tool_assignment_id == after.tool_assignment_id
                                        && before.expert_assignment_id == after.expert_assignment_id
                                },
                            ))
                };
            if previous.builtin_setups.iter().any(|before| {
                !snapshot
                    .builtin_setups
                    .iter()
                    .any(|after| builtin_receipt_allowed(before, after))
            }) {
                return Err(AgentFailure::Conflict);
            }
            for receipt in &snapshot.builtin_setups {
                if previous
                    .builtin_setups
                    .iter()
                    .any(|before| builtin_receipt_allowed(before, receipt))
                {
                    continue;
                }
                if receipt.expected_revision != expected_revision
                    || previous
                        .builtin_setups
                        .iter()
                        .any(|entry| entry.setup_id == receipt.setup_id)
                    || receipt.assignments.iter().any(|created| {
                        previous.installations.iter().any(|entry| {
                            [created.tool_installation_id, created.expert_installation_id]
                                .contains(&entry.id)
                        }) || previous.assignments.iter().any(|entry| {
                            [created.tool_assignment_id, created.expert_assignment_id]
                                .contains(&entry.id)
                        }) || snapshot.installations.iter().any(|entry| {
                            [created.tool_installation_id, created.expert_installation_id]
                                .contains(&entry.id)
                                && entry.enabled
                                && mutable_builtin_setup != Some(receipt.setup_id)
                        }) || snapshot.assignments.iter().any(|entry| {
                            [created.tool_assignment_id, created.expert_assignment_id]
                                .contains(&entry.id)
                                && entry.enabled
                                && mutable_builtin_setup != Some(receipt.setup_id)
                        })
                    })
                {
                    return Err(AgentFailure::Conflict);
                }
            }
            for assignment in &snapshot.assignments {
                match previous
                    .assignments
                    .iter()
                    .find(|entry| entry.id == assignment.id)
                {
                    Some(entry)
                        if entry.private_state == assignment.private_state
                            && entry.person_id == assignment.person_id
                            && entry.installation_id == assignment.installation_id => {}
                    Some(entry)
                        if mutable_assignment == Some(assignment.id)
                            && entry.id == assignment.id
                            && entry.person_id == assignment.person_id
                            && entry.installation_id == assignment.installation_id
                            && entry.enabled == assignment.enabled
                            && entry.granted_tool_assignments
                                == assignment.granted_tool_assignments
                            && entry.private_state.schema_version
                                == assignment.private_state.schema_version
                            && entry.private_state.revision.checked_add(1)
                                == Some(assignment.private_state.revision)
                            && entry.private_state.completed_invocations.checked_add(1)
                                == Some(assignment.private_state.completed_invocations)
                            && assignment.private_state.completed_invocations
                                == assignment.private_state.revision
                            && assignment.private_state.last_invocation_id.is_some()
                            && assignment.private_state.last_invocation_id
                                != entry.private_state.last_invocation_id => {}
                    None if assignment.private_state
                        == floe_experts::ExpertPrivateState::default() => {}
                    _ => return Err(AgentFailure::Conflict),
                }
            }
            if previous
                .assignments
                .iter()
                .any(|entry| !snapshot.assignments.iter().any(|next| next.id == entry.id))
                || previous
                    .packages
                    .iter()
                    .any(|entry| !snapshot.packages.contains(entry))
                || previous.installations.iter().any(|entry| {
                    !snapshot
                        .installations
                        .iter()
                        .any(|next| next.id == entry.id && next.package == entry.package)
                })
            {
                return Err(AgentFailure::Conflict);
            }
            self.update_registry(&transaction, expected_revision, snapshot.revision, payload)
                .await?;
            let completed_task = if let Some((
                task_id,
                expected_task_revision,
                executor_generation,
                task_snapshot,
            )) = task_completion
            {
                if self.active_executor_generation(&transaction).await? != executor_generation {
                    return Err(AgentFailure::Conflict);
                }
                let assignment_id = mutable_assignment.ok_or(AgentFailure::Conflict)?;
                let invocation_id = snapshot
                    .assignments
                    .iter()
                    .find(|assignment| assignment.id == assignment_id)
                    .and_then(|assignment| assignment.private_state.last_invocation_id)
                    .ok_or(AgentFailure::Conflict)?;
                let current = self
                    .task_on(&transaction, task_id)
                    .await?
                    .ok_or(AgentFailure::NotFound)?;
                if current.invocation_key.as_uuid() != invocation_id
                    || current.snapshot.agent_id != task_snapshot.agent_id
                    || !previous.assignments.iter().any(|assignment| {
                        assignment.id == assignment_id
                            && assignment.person_id == self.person_id
                            && previous.installations.iter().any(|installation| {
                                installation.id == assignment.installation_id
                                    && installation.package.id == task_snapshot.agent_id
                            })
                    })
                {
                    return Err(AgentFailure::Conflict);
                }
                let next = current.transition(
                    expected_task_revision,
                    executor_generation,
                    task_snapshot.clone(),
                    self.person_id,
                )?;
                self.validate_context_dependency_coverage_in_transaction(
                    &transaction,
                    &next.snapshot.coverage,
                )
                .await?;
                if super::tasks::write_task(
                    &transaction,
                    &next,
                    expected_task_revision,
                    executor_generation,
                )
                .await?
                    != 1
                {
                    return Err(AgentFailure::Conflict);
                }
                Some(next)
            } else {
                None
            };
            self.check_access()?;
            check()?;
            Ok(completed_task)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn commit_expert_session(
        &self,
        session: &AgentSession,
        previous_revision: u64,
        expected_registry_revision: u64,
        staged: &RegistrySnapshot,
    ) -> Result<RegistrySnapshot, AgentFailure> {
        self.commit_expert_session_with_hook(
            session,
            previous_revision,
            expected_registry_revision,
            staged,
            std::future::ready(Ok(())),
        )
        .await
    }

    pub async fn commit_expert_session_with_hook(
        &self,
        session: &AgentSession,
        previous_revision: u64,
        expected_registry_revision: u64,
        staged: &RegistrySnapshot,
        after_registry_write: impl std::future::Future<Output = Result<(), AgentFailure>> + Send,
    ) -> Result<RegistrySnapshot, AgentFailure> {
        self.commit_expert_session_inner(
            session,
            previous_revision,
            expected_registry_revision,
            staged,
            None,
            None,
            after_registry_write,
        )
        .await
    }

    pub async fn commit_expert_session_scoped_with_hook(
        &self,
        session: &AgentSession,
        previous_revision: u64,
        expected_registry_revision: u64,
        staged: &RegistrySnapshot,
        assignment_id: uuid::Uuid,
        evidence_id: uuid::Uuid,
        after_registry_write: impl std::future::Future<Output = Result<(), AgentFailure>> + Send,
    ) -> Result<RegistrySnapshot, AgentFailure> {
        self.commit_expert_session_inner(
            session,
            previous_revision,
            expected_registry_revision,
            staged,
            Some((assignment_id, evidence_id)),
            None,
            after_registry_write,
        )
        .await
    }

    pub async fn commit_expert_session_scoped_with_coverage_hook(
        &self,
        session: &AgentSession,
        previous_revision: u64,
        expected_registry_revision: u64,
        staged: &RegistrySnapshot,
        assignment_id: uuid::Uuid,
        evidence_id: uuid::Uuid,
        turn_id: Uuid,
        coverage: DependencyCoverage,
        after_registry_write: impl std::future::Future<Output = Result<(), AgentFailure>> + Send,
    ) -> Result<RegistrySnapshot, AgentFailure> {
        if turn_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        self.commit_expert_session_inner(
            session,
            previous_revision,
            expected_registry_revision,
            staged,
            Some((assignment_id, evidence_id)),
            Some((turn_id, coverage)),
            after_registry_write,
        )
        .await
    }

    async fn commit_expert_session_inner(
        &self,
        session: &AgentSession,
        previous_revision: u64,
        expected_registry_revision: u64,
        staged: &RegistrySnapshot,
        scope: Option<(uuid::Uuid, uuid::Uuid)>,
        coverage: Option<(Uuid, DependencyCoverage)>,
        after_registry_write: impl std::future::Future<Output = Result<(), AgentFailure>> + Send,
    ) -> Result<RegistrySnapshot, AgentFailure> {
        self.payload(session)?;
        if previous_revision.checked_add(1) != Some(session.revision) {
            return Err(AgentFailure::Conflict);
        }
        self.registry_payload(staged)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let mut candidate = session.clone();
            let previous = self.session_on(&transaction, session.id).await?;
            let stored = self.registry_on(&transaction).await?.ok_or(AgentFailure::NotFound)?;
            if scope.is_none() && stored.revision != expected_registry_revision {
                return Err(AgentFailure::Conflict);
            }
            if previous.data_classes.iter().any(|class| !session.data_classes.contains(class)) {
                return Err(AgentFailure::PolicyDenied);
            }
            if previous.scope != session.scope || previous.revision != previous_revision
                || session.messages.len() < previous.messages.len()
                || session.messages.len() > previous.messages.len() + 1
                || session.messages[..previous.messages.len()] != previous.messages {
                return Err(AgentFailure::Conflict);
            }
            let mut next = stored.clone();
            if let Some(AgentMessage::Delegation { turn_id, task }) = session.messages.get(previous.messages.len())
                && let Some(output) = task.data_part(floe_experts::EXPERT_RESULT_MEDIA_TYPE) {
                    let receipt: ExpertResult = serde_json::from_str(output).map_err(|_| AgentFailure::InvalidInput)?;
                    let call_id = task.id;
                    if previous.active_turn != Some(*turn_id) || session.active_turn != previous.active_turn
                        || receipt.invocation_id != call_id || receipt.person_id != self.person_id
                        || !session.data_classes.contains(&receipt.data_class) {
                        return Err(AgentFailure::Conflict);
                    }
                    if let Some((assignment_id, evidence_id)) = scope
                        && (assignment_id != receipt.assignment_id || evidence_id != receipt.evidence_id)
                    {
                        return Err(AgentFailure::Conflict);
                    }
                    if !expert_identity_matches(&stored, staged, &receipt) {
                        return Err(AgentFailure::Conflict);
                    }
                    let mut duplicate = transaction.query("SELECT 1 FROM agent_expert_receipts WHERE invocation_id = ?", [call_id.to_string()]).await.map_err(storage)?;
                    if duplicate.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::Conflict); }
                    drop(duplicate);
                    let mut registry = AgentRegistry::restore(stored.clone(), self.vault_id)?;
                    if scope.is_some() {
                        registry.record_result_current(&receipt)?;
                    } else {
                        registry.record_result(expected_registry_revision, &receipt)?;
                    }
                    next = registry.snapshot();
                    if scope.is_none() && next != *staged {
                        return Err(AgentFailure::Conflict);
                    }
                    let registry_revision = if scope.is_some() {
                        stored.revision
                    } else {
                        expected_registry_revision
                    };
                    self.update_registry(&transaction, registry_revision, next.revision, self.registry_payload(&next)?).await?;
                    transaction.execute("INSERT INTO agent_expert_receipts VALUES (?, ?, ?, ?)",
                        (call_id.to_string(), session.id.to_string(), receipt.assignment_id.to_string(), integer(next.revision)?)).await.map_err(storage)?;
            }
            after_registry_write.await?;
            if let Some((turn_id, coverage)) = coverage {
                self.validate_context_dependency_coverage_in_transaction(&transaction, &coverage)
                    .await?;
                super::context_dependencies::merge_context_dependency_coverage(
                    &transaction,
                    self.person_id,
                    session.id,
                    turn_id,
                    coverage,
                )
                .await?;
            }
            self.sanitize_session_for_context_cleanup(&transaction, &mut candidate)
                .await?;
            let payload = self.payload(&candidate)?;
            let changed = transaction.execute("UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                (integer(session.revision)?, payload, session.id.to_string(), integer(previous_revision)?)).await.map_err(storage)?;
            if changed != 1 { return Err(AgentFailure::Conflict); }
            self.check_access()?;
            Ok(next)
        }.await;
        let finish = if scope.is_some() {
            self.finish_registry_transaction_checked(transaction, result)
                .await
        } else {
            self.finish_registry_transaction(transaction, result).await
        };
        finish
    }

    pub(super) async fn finish_registry_transaction<T>(
        &self,
        transaction: turso::transaction::Transaction<'_>,
        result: Result<T, AgentFailure>,
    ) -> Result<T, AgentFailure> {
        match result {
            Ok(value) => {
                transaction.commit().await.map_err(storage)?;
                self.check_access()?;
                Ok(value)
            }
            Err(failure) => {
                if transaction.rollback().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(AgentFailure::VaultUnavailable);
                }
                Err(failure)
            }
        }
    }

    pub(super) async fn finish_registry_transaction_checked<T>(
        &self,
        transaction: turso::transaction::Transaction<'_>,
        result: Result<T, AgentFailure>,
    ) -> Result<T, AgentFailure> {
        match result {
            Ok(value) => {
                if transaction.commit().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    Err(AgentFailure::StorageUnavailable)
                } else if let Err(failure) = self.check_access() {
                    self.unavailable.store(true, Ordering::Release);
                    Err(failure)
                } else {
                    Ok(value)
                }
            }
            Err(failure) => {
                if transaction.rollback().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(AgentFailure::VaultUnavailable);
                }
                if matches!(
                    failure,
                    AgentFailure::StorageUnavailable
                        | AgentFailure::VaultUnavailable
                        | AgentFailure::UnsupportedVersion
                ) {
                    self.unavailable.store(true, Ordering::Release);
                }
                Err(failure)
            }
        }
    }

    pub(super) fn registry_transaction_start_error(&self, error: turso::Error) -> AgentFailure {
        match error {
            turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => AgentFailure::Conflict,
            _ => {
                self.unavailable.store(true, Ordering::Release);
                AgentFailure::StorageUnavailable
            }
        }
    }

    async fn update_registry(
        &self,
        connection: &turso::Connection,
        previous: u64,
        revision: u64,
        payload: String,
    ) -> Result<(), AgentFailure> {
        let changed = connection.execute("UPDATE agent_expert_registry SET revision = ?, payload = ? WHERE id = 1 AND revision = ?",
            (integer(revision)?, payload, integer(previous)?)).await.map_err(storage)?;
        if changed != 1 {
            return Err(AgentFailure::Conflict);
        }
        Ok(())
    }

    fn registry_payload(&self, snapshot: &RegistrySnapshot) -> Result<String, AgentFailure> {
        if snapshot
            .assignments
            .iter()
            .any(|assignment| assignment.person_id != self.person_id)
        {
            return Err(AgentFailure::NotFound);
        }
        AgentRegistry::restore(snapshot.clone(), self.vault_id)?;
        integer(snapshot.revision)?;
        let payload = serde_json::to_string(snapshot).map_err(storage)?;
        if payload.len() > MAX_REGISTRY_BYTES {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(payload)
    }

    pub(super) async fn registry_on(
        &self,
        connection: &turso::Connection,
    ) -> Result<Option<RegistrySnapshot>, AgentFailure> {
        let mut identity = connection
            .query("SELECT version FROM vault_identity WHERE id = 1", ())
            .await
            .map_err(unavailable)?;
        let version = identity
            .next()
            .await
            .map_err(unavailable)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<i64>(0)
            .map_err(unavailable)?;
        drop(identity);
        if version == 1 {
            let mut existing = connection.query("SELECT name FROM sqlite_schema WHERE name IN ('agent_expert_registry', 'agent_expert_receipts')", ()).await.map_err(unavailable)?;
            if existing.next().await.map_err(unavailable)?.is_some() {
                return Err(AgentFailure::VaultUnavailable);
            }
            return Ok(None);
        }
        if version != 2 {
            return Err(AgentFailure::UnsupportedVersion);
        }
        let mut rows = connection.query("SELECT revision, payload FROM agent_expert_registry WHERE id = 1 AND length(CAST(payload AS BLOB)) <= 262144", ()).await.map_err(unavailable)?;
        let row = rows
            .next()
            .await
            .map_err(unavailable)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        let snapshot: RegistrySnapshot =
            serde_json::from_str(&row.get::<String>(1).map_err(unavailable)?)
                .map_err(unavailable)?;
        self.registry_payload(&snapshot).map_err(unavailable)?;
        if integer(snapshot.revision)? != row.get::<i64>(0).map_err(unavailable)? {
            return Err(AgentFailure::VaultUnavailable);
        }
        connection.query("SELECT invocation_id, session_id, assignment_id, registry_revision FROM agent_expert_receipts LIMIT 0", ()).await.map_err(unavailable)?;
        Ok(Some(snapshot))
    }

    pub(super) async fn write_registry_snapshot_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        expected_revision: Option<u64>,
        snapshot: &RegistrySnapshot,
    ) -> Result<(), AgentFailure> {
        let payload = self.registry_payload(snapshot)?;
        match expected_revision {
            Some(expected_revision) => {
                let previous = self
                    .registry_on(transaction)
                    .await?
                    .ok_or(AgentFailure::NotFound)?;
                if previous.revision != expected_revision {
                    return Err(AgentFailure::Conflict);
                }
                self.update_registry(transaction, expected_revision, snapshot.revision, payload)
                    .await
            }
            None => {
                if self.registry_on(transaction).await?.is_some() {
                    return Err(AgentFailure::Conflict);
                }
                transaction
                    .execute(
                        "CREATE TABLE agent_expert_registry (id INTEGER PRIMARY KEY CHECK (id = 1), revision INTEGER NOT NULL, payload TEXT NOT NULL)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE TABLE agent_expert_receipts (invocation_id TEXT PRIMARY KEY, session_id TEXT NOT NULL, assignment_id TEXT NOT NULL, registry_revision INTEGER NOT NULL)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "INSERT INTO agent_expert_registry VALUES (1, ?, ?)",
                        (integer(snapshot.revision)?, payload),
                    )
                    .await
                    .map_err(storage)?;
                let changed = transaction
                    .execute(
                        "UPDATE vault_identity SET version = 2 WHERE id = 1 AND version = 1",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                if changed != 1 {
                    return Err(AgentFailure::Conflict);
                }
                Ok(())
            }
        }
    }

    pub(super) async fn session_on(
        &self,
        connection: &turso::Connection,
        id: Uuid,
    ) -> Result<AgentSession, AgentFailure> {
        let mut rows = connection.query("SELECT revision, payload FROM agent_sessions WHERE id = ? AND length(CAST(payload AS BLOB)) <= 262144", [id.to_string()]).await.map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        let session: AgentSession =
            serde_json::from_str(&row.get::<String>(1).map_err(storage)?).map_err(unavailable)?;
        self.payload(&session)?;
        if session.id != id || integer(session.revision)? != row.get::<i64>(0).map_err(storage)? {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(session)
    }
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::BudgetExceeded)
}
