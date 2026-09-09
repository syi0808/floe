use floe_agent::{AgentMessage, AgentRegistry, ExpertResult, RegistrySnapshot};
use turso::transaction::TransactionBehavior;

use super::*;

const MAX_REGISTRY_BYTES: usize = 262_144;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn calendar_expert_overview(
        &self,
    ) -> Result<floe_agent::CalendarExpertOverview, AgentFailure> {
        let registry = match self.expert_registry().await? {
            Some(snapshot) => AgentRegistry::restore(snapshot, self.vault_id)?,
            None => AgentRegistry::new(self.vault_id),
        };
        let overview = registry.calendar_expert_overview(self.person_id);
        self.check_access()?;
        Ok(overview)
    }

    pub async fn install_calendar_expert(
        &self,
        request: floe_agent::CalendarExpertSetup,
        cancellation: floe_agent::Cancellation,
    ) -> Result<floe_agent::CalendarExpertSetupResult, AgentFailure> {
        let check = || {
            if cancellation.is_cancelled() {
                Err(AgentFailure::Cancelled)
            } else {
                Ok(())
            }
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
        let setup = registry.install_calendar_expert(self.person_id, &request)?;
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
        Ok(floe_agent::CalendarExpertSetupResult {
            setup,
            registry: registry.overview(self.person_id),
        })
    }

    pub async fn configure_calendar_access(
        &self,
        configuration: floe_agent::CalendarAccessConfiguration,
        cancellation: floe_agent::Cancellation,
    ) -> Result<floe_agent::CalendarExpertOverview, AgentFailure> {
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
        registry.configure_calendar_access(self.person_id, &configuration)?;
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
        self.check_access()?;
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        Ok(registry.calendar_expert_overview(self.person_id))
    }

    pub async fn registry_overview(
        &self,
    ) -> Result<Option<floe_agent::RegistryOverview>, AgentFailure> {
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
        configuration: floe_agent::RegistryConfiguration,
        cancellation: floe_agent::Cancellation,
    ) -> Result<floe_agent::RegistryOverview, AgentFailure> {
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
            floe_agent::RegistryConfigurationTarget::CalendarView { id, enabled } => registry
                .set_calendar_view_enabled(
                    configuration.expected_revision,
                    self.person_id,
                    id,
                    enabled,
                )?,
            floe_agent::RegistryConfigurationTarget::Installation { id, enabled } => {
                registry.set_installation_enabled(configuration.expected_revision, id, enabled)?
            }
            floe_agent::RegistryConfigurationTarget::Assignment { id, enabled } => registry
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

    pub(crate) async fn initialize_expert_registry_checked(
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
            .map_err(storage)?;
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

    pub(crate) async fn save_expert_registry_checked(
        &self,
        expected_revision: u64,
        snapshot: &RegistrySnapshot,
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
            .map_err(storage)?;
        let result = async {
            let previous = self
                .registry_on(&transaction)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if previous.revision != expected_revision {
                return Err(AgentFailure::Conflict);
            }
            if previous
                .calendar_setups
                .iter()
                .any(|receipt| !snapshot.calendar_setups.contains(receipt))
            {
                return Err(AgentFailure::Conflict);
            }
            if previous
                .revoked_calendar_setups
                .iter()
                .any(|setup_id| !snapshot.revoked_calendar_setups.contains(setup_id))
                || snapshot.revoked_calendar_setups.iter().any(|setup_id| {
                    !previous.revoked_calendar_setups.contains(setup_id)
                        && !previous
                            .calendar_setups
                            .iter()
                            .any(|setup| setup.setup_id == *setup_id)
                })
            {
                return Err(AgentFailure::Conflict);
            }
            for receipt in &snapshot.calendar_setups {
                if previous.calendar_setups.contains(receipt) {
                    continue;
                }
                if receipt.expected_revision != expected_revision
                    || previous
                        .calendar_views
                        .iter()
                        .any(|binding| binding.handle == receipt.view_handle)
                    || previous.installations.iter().any(|installation| {
                        [receipt.tool_installation_id, receipt.expert_installation_id]
                            .contains(&installation.id)
                    })
                    || previous.assignments.iter().any(|assignment| {
                        [receipt.tool_assignment_id, receipt.expert_assignment_id]
                            .contains(&assignment.id)
                    })
                    || snapshot.installations.iter().any(|installation| {
                        [receipt.tool_installation_id, receipt.expert_installation_id]
                            .contains(&installation.id)
                            && installation.enabled
                    })
                    || snapshot.assignments.iter().any(|assignment| {
                        [receipt.tool_assignment_id, receipt.expert_assignment_id]
                            .contains(&assignment.id)
                            && assignment.enabled
                    })
                {
                    return Err(AgentFailure::Conflict);
                }
            }
            for binding in &snapshot.calendar_views {
                match previous
                    .calendar_views
                    .iter()
                    .find(|entry| entry.handle == binding.handle)
                {
                    Some(entry)
                        if entry.person_id == binding.person_id
                            && entry.provider == binding.provider
                            && entry.calendar_ids == binding.calendar_ids => {}
                    None if !binding.enabled
                        && !previous.assignments.iter().any(|assignment| {
                            assignment.granted_view_handles.contains(&binding.handle)
                        }) => {}
                    _ => return Err(AgentFailure::Conflict),
                }
            }
            if previous.calendar_views.iter().any(|entry| {
                !snapshot
                    .calendar_views
                    .iter()
                    .any(|binding| binding.handle == entry.handle)
            }) {
                return Err(AgentFailure::Conflict);
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
                        == floe_agent::ExpertPrivateState::default() => {}
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
        self.finish_registry_transaction(transaction, result).await
    }

    pub(crate) async fn commit_expert_session(
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

    pub(crate) async fn commit_expert_session_with_hook(
        &self,
        session: &AgentSession,
        previous_revision: u64,
        expected_registry_revision: u64,
        staged: &RegistrySnapshot,
        after_registry_write: impl std::future::Future<Output = Result<(), AgentFailure>> + Send,
    ) -> Result<RegistrySnapshot, AgentFailure> {
        let payload = self.payload(session)?;
        if previous_revision.checked_add(1) != Some(session.revision) {
            return Err(AgentFailure::Conflict);
        }
        self.registry_payload(staged)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let previous = self.session_on(&transaction, session.id).await?;
            let stored = self.registry_on(&transaction).await?.ok_or(AgentFailure::NotFound)?;
            if previous.data_classes.iter().any(|class| !session.data_classes.contains(class)) {
                return Err(AgentFailure::PolicyDenied);
            }
            if previous.scope != session.scope || previous.revision != previous_revision || stored.revision != expected_registry_revision
                || session.messages.len() < previous.messages.len()
                || session.messages.len() > previous.messages.len() + 1
                || session.messages[..previous.messages.len()] != previous.messages {
                return Err(AgentFailure::Conflict);
            }
            let mut next = stored.clone();
            if let Some(AgentMessage::Delegation { turn_id, task }) = session.messages.get(previous.messages.len()) {
                if let Some(output) = task.data_part(floe_agent::EXPERT_RESULT_MEDIA_TYPE) {
                    let receipt: ExpertResult = serde_json::from_str(output).map_err(|_| AgentFailure::InvalidInput)?;
                    let call_id = task.id;
                    if previous.active_turn != Some(*turn_id) || session.active_turn != previous.active_turn
                        || receipt.invocation_id != call_id || receipt.person_id != self.person_id
                        || !session.data_classes.contains(&receipt.data_class) {
                        return Err(AgentFailure::Conflict);
                    }
                    let mut duplicate = transaction.query("SELECT 1 FROM agent_expert_receipts WHERE invocation_id = ?", [call_id.to_string()]).await.map_err(storage)?;
                    if duplicate.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::Conflict); }
                    drop(duplicate);
                    let mut registry = AgentRegistry::restore(stored, self.vault_id)?;
                    registry.record_result(expected_registry_revision, &receipt)?;
                    next = registry.snapshot();
                    if &next != staged { return Err(AgentFailure::Conflict); }
                    self.update_registry(&transaction, expected_registry_revision, next.revision, self.registry_payload(&next)?).await?;
                    transaction.execute("INSERT INTO agent_expert_receipts VALUES (?, ?, ?, ?)",
                        (call_id.to_string(), session.id.to_string(), receipt.assignment_id.to_string(), integer(next.revision)?)).await.map_err(storage)?;
                }
            }
            after_registry_write.await?;
            let changed = transaction.execute("UPDATE agent_sessions SET revision = ?, payload = ? WHERE id = ? AND revision = ?",
                (integer(session.revision)?, payload, session.id.to_string(), integer(previous_revision)?)).await.map_err(storage)?;
            if changed != 1 { return Err(AgentFailure::Conflict); }
            self.check_access()?;
            Ok(next)
        }.await;
        self.finish_registry_transaction(transaction, result).await
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
            .calendar_views
            .iter()
            .any(|binding| binding.person_id != self.person_id)
        {
            return Err(AgentFailure::NotFound);
        }
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

#[cfg(test)]
mod tests;
