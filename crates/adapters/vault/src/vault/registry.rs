use floe_access::DependencyCoverage;
use floe_agent_contract::TaskState;
use floe_experts::{AgentRegistry, RegistrySnapshot};
use turso::transaction::TransactionBehavior;

use super::tasks::VaultTaskRecord;
use super::*;

const MAX_REGISTRY_BYTES: usize = 262_144;

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

    pub async fn enabled_expert_admissions(
        &self,
    ) -> Result<
        Vec<(
            floe_experts::AgentCard,
            floe_experts::ExpertAdmissionIdentity,
        )>,
        AgentFailure,
    > {
        let entries = match self.expert_registry().await? {
            Some(snapshot) => AgentRegistry::restore(snapshot, self.vault_id)?
                .enabled_expert_admissions(self.person_id)?,
            None => vec![],
        };
        self.check_access()?;
        Ok(entries)
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
        if settlement.admission.assignment_id.is_nil()
            || settlement.invocation_id.is_nil()
            || settlement.owner() != task_snapshot.agent_id
            || task_snapshot.task_id != task_id
            || task_snapshot.principal != self.person_id.to_string()
            || task_snapshot.state != TaskState::Completed
            || task_snapshot.coverage != coverage
            || task_snapshot.result.as_deref() != Some(settlement.task_result.as_str())
            || settlement.next_private_state.schema_version != 1
            || settlement.next_private_state.last_invocation_id
                != Some(settlement.invocation_id)
            || settlement.expected_private_state_revision.checked_add(1)
                != Some(settlement.next_private_state.revision)
            || settlement.next_private_state.completed_invocations
                != settlement.next_private_state.revision
        {
            return Err(AgentFailure::Conflict);
        }
        check()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            let mut registry = self
                .registry_on(&transaction)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if registry.instance_id != settlement.admission.registry_instance_id {
                return Err(AgentFailure::Conflict);
            }
            let assignment = registry
                .assignments
                .iter_mut()
                .find(|assignment| assignment.id == settlement.admission.assignment_id
                    && assignment.person_id == self.person_id)
                .ok_or(AgentFailure::Conflict)?;
            if assignment.installation_id != settlement.admission.installation_id
                || assignment.private_state.revision
                    != settlement.expected_private_state_revision
                || assignment.private_state.completed_invocations
                    != settlement.expected_private_state_revision
                || assignment.private_state.last_invocation_id
                    == Some(settlement.invocation_id)
                || !registry.installations.iter().any(|installation| {
                    installation.id == assignment.installation_id
                        && installation.package == settlement.admission.package
                })
                || settlement.admission.definition_revision
                    != task_snapshot.definition_revision
            {
                return Err(AgentFailure::Conflict);
            }
            let current = self
                .task_on(&transaction, task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.admission != settlement.admission
                || current.invocation_key.as_uuid() != settlement.invocation_id
            {
                return Err(AgentFailure::Conflict);
            }
            let next_task = current.transition(
                expected_task_revision,
                executor_generation,
                task_snapshot,
                self.person_id,
            )?;
            if self.active_executor_generation(&transaction).await? != executor_generation {
                return Err(AgentFailure::Conflict);
            }
            self.validate_context_dependency_coverage_in_transaction(
                &transaction,
                &next_task.snapshot.coverage,
            )
            .await?;
            assignment.private_state = settlement.next_private_state;
            let previous_revision = registry.revision;
            registry.revision = previous_revision
                .checked_add(1)
                .ok_or(AgentFailure::BudgetExceeded)?;
            let payload = self.registry_payload(&registry)?;
            self.update_registry(
                &transaction,
                previous_revision,
                registry.revision,
                payload,
            )
            .await?;
            if super::tasks::write_task(
                &transaction,
                &next_task,
                expected_task_revision,
                executor_generation,
            )
            .await? != 1
            {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            check()?;
            Ok(next_task)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result).await
    }

    pub(super) async fn save_expert_registry_change_checked(
        &self,
        expected_revision: u64,
        snapshot: &RegistrySnapshot,
        mutable_builtin_setup: Option<uuid::Uuid>,
        check: impl Fn() -> Result<(), AgentFailure> + Sync,
    ) -> Result<(), AgentFailure> {
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
            self.check_access()?;
            check()?;
            Ok(())
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
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
            let mut existing = connection
                .query(
                    "SELECT name FROM sqlite_schema WHERE name = 'agent_expert_registry'",
                    (),
                )
                .await
                .map_err(unavailable)?;
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
        Ok(Some(snapshot))
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
