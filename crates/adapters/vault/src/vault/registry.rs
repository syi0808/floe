use floe_experts::{AgentRegistry, RegistrySnapshot};

use super::*;

const MAX_REGISTRY_BYTES: usize = 262_144;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(crate) fn registry_instance_id(&self) -> Uuid {
        self.vault_id
    }

    pub(super) async fn expert_registry(&self) -> Result<Option<RegistrySnapshot>, AgentFailure> {
        self.registry_on(&self.connection()?).await
    }

    /// First registry installation and its command receipt share the caller's transaction.
    pub(crate) async fn install_expert_registry_on(
        &self,
        connection: &turso::Connection,
        snapshot: &RegistrySnapshot,
    ) -> Result<(), AgentFailure> {
        let payload = self.registry_payload(snapshot)?;
        if snapshot
            .assignments
            .iter()
            .any(|assignment| assignment.private_state.revision != 0)
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self.registry_on(connection).await?.is_some() {
            return Err(AgentFailure::Conflict);
        }
        connection
            .execute(
                "INSERT INTO agent_expert_registry VALUES (1, ?, ?)",
                (integer(snapshot.revision)?, payload),
            )
            .await
            .map_err(storage)?;
        self.check_access()
    }

    pub(crate) async fn finish_registry_transaction_checked<T>(
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

    pub(crate) fn registry_transaction_start_error(&self, error: turso::Error) -> AgentFailure {
        match error {
            turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => AgentFailure::Conflict,
            _ => {
                self.unavailable.store(true, Ordering::Release);
                AgentFailure::StorageUnavailable
            }
        }
    }

    pub(crate) async fn update_registry(
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

    pub(crate) fn registry_payload(
        &self,
        snapshot: &RegistrySnapshot,
    ) -> Result<String, AgentFailure> {
        if snapshot
            .assignments
            .iter()
            .any(|assignment| assignment.person_id != self.person_id)
            || snapshot
                .install_receipts
                .iter()
                .any(|receipt| receipt.person_id != self.person_id)
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

    pub(crate) async fn registry_on(
        &self,
        connection: &turso::Connection,
    ) -> Result<Option<RegistrySnapshot>, AgentFailure> {
        crate::schema::inspect_family(connection, crate::schema::Family::Registry)
            .await
            .map_err(crate::schema::SchemaFailure::into_agent)?;
        let mut rows = connection.query("SELECT revision, CASE WHEN length(CAST(payload AS BLOB)) <= 262144 THEN payload ELSE NULL END FROM agent_expert_registry WHERE id = 1", ()).await.map_err(unavailable)?;
        let Some(row) = rows.next().await.map_err(unavailable)? else {
            return Ok(None);
        };
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
