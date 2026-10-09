use super::database_failure;
use floe_experts::{AgentRegistry, RegistrySnapshot};
use floe_kernel::CommandFailure;

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
            .map_err(database_failure)?;
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
                } else {
                    #[cfg(test)]
                    if self
                        .registry_transaction_ack_loss
                        .swap(false, Ordering::AcqRel)
                    {
                        self.unavailable.store(true, Ordering::Release);
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    if let Err(failure) = self.check_access() {
                        self.unavailable.store(true, Ordering::Release);
                        Err(failure)
                    } else {
                        Ok(value)
                    }
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

    /// Finish a durable product command while preserving its owner's
    /// transaction classification. A failed commit is always uncertain; a
    /// successfully committed receipt remains admitted even if the final
    /// access fence fails.
    pub(crate) async fn finish_registry_command_transaction<T>(
        &self,
        transaction: turso::transaction::Transaction<'_>,
        result: Result<T, CommandFailure<AgentFailure>>,
    ) -> Result<T, CommandFailure<AgentFailure>> {
        match result {
            Ok(value) => {
                if transaction.commit().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    Err(CommandFailure::Indeterminate(
                        AgentFailure::StorageUnavailable,
                    ))
                } else if let Err(failure) = self.check_access() {
                    self.unavailable.store(true, Ordering::Release);
                    Err(CommandFailure::Admitted(failure))
                } else {
                    Ok(value)
                }
            }
            Err(failure) => {
                if transaction.rollback().await.is_err() {
                    self.unavailable.store(true, Ordering::Release);
                    return Err(CommandFailure::Indeterminate(
                        AgentFailure::VaultUnavailable,
                    ));
                }
                if matches!(
                    failure,
                    CommandFailure::NotApplied(
                        AgentFailure::StorageUnavailable
                            | AgentFailure::VaultUnavailable
                            | AgentFailure::UnsupportedVersion
                    ) | CommandFailure::Indeterminate(_)
                ) {
                    self.unavailable.store(true, Ordering::Release);
                }
                Err(failure)
            }
        }
    }

    pub(crate) fn registry_transaction_start_error(&self, error: turso::Error) -> AgentFailure {
        let failure = database_failure(error);
        if failure != AgentFailure::StorageBusy {
            self.unavailable.store(true, Ordering::Release);
        }
        failure
    }

    pub(crate) async fn update_registry(
        &self,
        connection: &turso::Connection,
        previous: u64,
        revision: u64,
        payload: String,
    ) -> Result<(), AgentFailure> {
        let changed = connection.execute("UPDATE agent_expert_registry SET revision = ?, payload = ? WHERE id = 1 AND revision = ?",
            (integer(revision)?, payload, integer(previous)?)).await.map_err(database_failure)?;
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
        let mut rows = connection.query("SELECT revision, CASE WHEN length(CAST(payload AS BLOB)) <= 262144 THEN payload ELSE NULL END FROM agent_expert_registry WHERE id = 1", ()).await.map_err(database_failure)?;
        let Some(row) = rows.next().await.map_err(database_failure)? else {
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
        // Bound the stored payload before JSON helpers inspect it. Besides
        // keeping the normal path within the Session byte budget, this makes
        // malformed oversized rows fail with the same explicit budget result
        // as a valid oversized row.
        let mut size_rows = connection
            .query(
                "SELECT length(CAST(payload AS BLOB)) FROM agent_sessions WHERE id = ? LIMIT 2",
                [id.to_string()],
            )
            .await
            .map_err(database_failure)?;
        let Some(size_row) = size_rows.next().await.map_err(database_failure)? else {
            return Err(AgentFailure::NotFound);
        };
        let payload_bytes = usize::try_from(size_row.get::<i64>(0).map_err(storage)?)
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        if payload_bytes > AgentBudget::default().max_session_bytes {
            return Err(AgentFailure::BudgetExceeded);
        }
        if size_rows.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        drop(size_rows);

        // Manager Sessions keep owner metadata in this bounded payload and
        // their transcript in normalized Core custody. Inspect the JSON
        // array length first so a stale profile fails closed without loading
        // its historical message vector into Rust.
        let mut header_rows = connection
            .query(
                "SELECT revision, json_type(payload, '$.scope'), json_extract(payload, '$.data_classes[0]'), json_array_length(payload, '$.messages') FROM agent_sessions WHERE id = ? LIMIT 2",
                [id.to_string()],
            )
            .await
            .map_err(database_failure)?;
        let Some(header) = header_rows.next().await.map_err(database_failure)? else {
            return Err(AgentFailure::NotFound);
        };
        if header_rows
            .next()
            .await
            .map_err(database_failure)?
            .is_some()
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let is_personal_manager = header.get::<Option<String>>(1).map_err(storage)?.as_deref()
            == Some("null")
            && header.get::<Option<String>>(2).map_err(storage)?.as_deref() == Some("personal");
        let revision = header.get::<i64>(0).map_err(storage)?;
        if is_personal_manager {
            if revision < 0 || header.get::<Option<i64>>(3).map_err(storage)? != Some(0) {
                return Err(AgentFailure::UnsupportedVersion);
            }
            drop(header_rows);
            let mut rows = connection
                .query(
                    "SELECT revision, json_set(payload, '$.messages', json('[]')) FROM agent_sessions WHERE id = ? AND revision = ? LIMIT 2",
                    (id.to_string(), revision),
                )
                .await
                .map_err(database_failure)?;
            let row = rows
                .next()
                .await
                .map_err(database_failure)?
                .ok_or(AgentFailure::VaultUnavailable)?;
            let stored_revision = row.get::<i64>(0).map_err(storage)?;
            let payload = row.get::<String>(1).map_err(storage)?;
            if rows.next().await.map_err(database_failure)?.is_some() {
                return Err(AgentFailure::VaultUnavailable);
            }
            drop(rows);
            let session: AgentSession = serde_json::from_str(&payload).map_err(unavailable)?;
            self.payload(&session)?;
            if session.id != id
                || integer(session.revision)? != stored_revision
                || !session.messages.is_empty()
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            return Ok(session);
        }
        drop(header_rows);
        let mut rows = connection
            .query(
                "SELECT revision, payload FROM agent_sessions WHERE id = ?",
                [id.to_string()],
            )
            .await
            .map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::NotFound)?;
        let payload = row.get::<String>(1).map_err(storage)?;
        let session = self.decode_session_payload(&payload)?;
        if session.id != id || integer(session.revision)? != row.get::<i64>(0).map_err(storage)? {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(session)
    }
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::BudgetExceeded)
}
