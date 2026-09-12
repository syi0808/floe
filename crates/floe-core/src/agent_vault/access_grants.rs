use floe_domain::{
    ConnectionId, ConnectorId, DataAccessGrant, ExecutionOwnerId, GrantAuthority, GrantId,
    GrantScope, GrantSourceBinding, GrantState, GrantTransitionError, GrantValidationError,
    SourceAuthority,
};
use serde::{Deserialize, Serialize};
use turso::{Row, transaction::TransactionBehavior};
use uuid::Uuid;

use super::*;

const ACCESS_GRANT_SCHEMA_VERSION: i64 = 1;
const MAX_ACCESS_GRANTS: usize = 128;
const MAX_CLEANUP_ITEMS: usize = 256;
const MAX_GRANT_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessGrantCleanup {
    pub grant_id: GrantId,
    pub invalidated_authority: GrantAuthority,
}

#[derive(Clone)]
pub(super) enum AccessGrantMutation {
    Review {
        source: GrantSourceBinding,
        scope: GrantScope,
    },
    Activate {
        source: GrantSourceBinding,
        scope: GrantScope,
    },
    Pause,
    Revoke,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn initialize_access_grant_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let components = [
            "data_access_grant_schema",
            "data_access_grants",
            "data_access_grant_cleanup",
        ];
        let mut component_rows = connection.query("SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('data_access_grant_schema', 'data_access_grants', 'data_access_grant_cleanup')", ()).await.map_err(storage)?;
        let mut existing = Vec::new();
        while let Some(row) = component_rows.next().await.map_err(storage)? {
            existing.push(row.get::<String>(0).map_err(storage)?);
        }
        if existing.is_empty() {
            let transaction = match connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
            {
                Ok(transaction) => transaction,
                Err(error) => {
                    let failure = access_transaction_start_error(error);
                    if failure != AgentFailure::Conflict {
                        self.latch_access_unavailable();
                    }
                    return Err(failure);
                }
            };
            let result = async {
            transaction.execute("CREATE TABLE data_access_grant_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))", ()).await.map_err(storage)?;
            transaction
                .execute(
                    "INSERT INTO data_access_grant_schema (id, version) VALUES (1, 1)",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction.execute("CREATE TABLE data_access_grants (grant_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, authority_owner TEXT NOT NULL, connection_id TEXT NOT NULL, connector TEXT NOT NULL, execution_owner TEXT NOT NULL, source_incarnation TEXT NOT NULL, source_epoch INTEGER NOT NULL, grant_incarnation TEXT NOT NULL, access_epoch INTEGER NOT NULL, state TEXT NOT NULL, payload TEXT NOT NULL)", ()).await.map_err(storage)?;
            transaction.execute("CREATE INDEX data_access_grants_person_state ON data_access_grants (person_id, state, grant_id)", ()).await.map_err(storage)?;
            transaction.execute("CREATE TABLE data_access_grant_cleanup (cleanup_id TEXT PRIMARY KEY, grant_id TEXT NOT NULL, person_id TEXT NOT NULL, invalidated_incarnation TEXT NOT NULL, invalidated_epoch INTEGER NOT NULL, payload TEXT NOT NULL, UNIQUE(grant_id, invalidated_incarnation, invalidated_epoch))", ()).await.map_err(storage)?;
            transaction.execute("CREATE INDEX data_access_grant_cleanup_ready ON data_access_grant_cleanup (person_id, grant_id, invalidated_epoch)", ()).await.map_err(storage)?;
            Ok(())
            }.await;
            self.finish_access_grant_transaction(transaction, result)
                .await?;
        } else if components
            .iter()
            .any(|name| !existing.iter().any(|existing_name| existing_name == name))
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut marker = connection
            .query(
                "SELECT version FROM data_access_grant_schema WHERE id = 1",
                (),
            )
            .await
            .map_err(storage)?;
        match marker.next().await.map_err(storage)? {
            Some(row) if row.get::<i64>(0).map_err(storage)? == ACCESS_GRANT_SCHEMA_VERSION => {}
            Some(_) => return Err(AgentFailure::UnsupportedVersion),
            None => return Err(AgentFailure::VaultUnavailable),
        }
        let mut marker_rows = connection
            .query("SELECT id FROM data_access_grant_schema", ())
            .await
            .map_err(storage)?;
        if marker_rows.next().await.map_err(storage)?.is_none()
            || marker_rows.next().await.map_err(storage)?.is_some()
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        self.validate_all_access_grants(&connection).await
    }

    pub async fn create_data_access_grant(
        &self,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> Result<DataAccessGrant, AgentFailure> {
        self.create_data_access_grant_with_id(GrantId::new(), source, scope)
            .await
    }

    pub async fn create_data_access_grant_with_id(
        &self,
        id: GrantId,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if source.person_id() != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        source.validate().map_err(grant_input)?;
        scope.validate().map_err(grant_input)?;
        let grant = DataAccessGrant::new(id, self.vault_id, source, scope).map_err(grant_input)?;
        let payload = grant_payload(&grant)?;
        let mut connection = self.connection()?;
        let transaction = match connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
        {
            Ok(transaction) => transaction,
            Err(error) => {
                let failure = access_transaction_start_error(error);
                if failure != AgentFailure::Conflict {
                    self.latch_access_unavailable();
                }
                return Err(failure);
            }
        };
        let result = async {
            self.ensure_access_grant_schema_transaction(&transaction).await?;
            let mut existing = transaction.query("SELECT 1 FROM data_access_grants WHERE grant_id = ?", [id.as_uuid().to_string()]).await.map_err(storage)?;
            if existing.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::Conflict); }
            let mut count_rows = transaction.query("SELECT COUNT(*) FROM data_access_grants", ()).await.map_err(storage)?;
            let count = count_rows.next().await.map_err(storage)?.ok_or(AgentFailure::VaultUnavailable)?.get::<i64>(0).map_err(storage)?;
            if count < 0 || usize::try_from(count).map_err(|_| AgentFailure::VaultUnavailable)? >= MAX_ACCESS_GRANTS { return Err(AgentFailure::BudgetExceeded); }
            let changed = transaction.execute(
                "INSERT INTO data_access_grants (grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                grant_values(&grant, payload.clone())?,
            ).await;
            if let Err(error) = changed {
                return Err(match error {
                    turso::Error::Constraint(_) => AgentFailure::Conflict,
                    other => storage(other),
                });
            }
            self.check_access()?;
            Ok(grant)
        }.await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub(super) async fn create_data_access_grant_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        id: GrantId,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if source.person_id() != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        source.validate().map_err(grant_input)?;
        scope.validate().map_err(grant_input)?;
        let grant = DataAccessGrant::new(id, self.vault_id, source, scope).map_err(grant_input)?;
        let payload = grant_payload(&grant)?;
        self.ensure_access_grant_schema_transaction(transaction)
            .await?;
        let mut existing = transaction
            .query(
                "SELECT 1 FROM data_access_grants WHERE grant_id = ?",
                [id.as_uuid().to_string()],
            )
            .await
            .map_err(storage)?;
        if existing.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::Conflict);
        }
        let mut count_rows = transaction
            .query("SELECT COUNT(*) FROM data_access_grants", ())
            .await
            .map_err(storage)?;
        let count = count_rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<i64>(0)
            .map_err(storage)?;
        if count < 0
            || usize::try_from(count).map_err(|_| AgentFailure::VaultUnavailable)?
                >= MAX_ACCESS_GRANTS
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let inserted = transaction
            .execute(
                "INSERT INTO data_access_grants (grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                grant_values(&grant, payload)?,
            )
            .await;
        if let Err(error) = inserted {
            return Err(match error {
                turso::Error::Constraint(_) => AgentFailure::Conflict,
                other => storage(other),
            });
        }
        Ok(grant)
    }

    pub async fn get_data_access_grant(
        &self,
        id: GrantId,
    ) -> Result<DataAccessGrant, AgentFailure> {
        let connection = self.connection()?;
        self.ensure_access_grant_schema(&connection).await?;
        let mut rows = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ? AND authority_owner = ?", (id.as_uuid().to_string(), self.person_id.to_string(), self.vault_id.to_string())).await.map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        let grant = decode_grant(&row).map_err(|failure| self.reject_access_corruption(failure))?;
        self.check_access()?;
        Ok(grant)
    }

    pub async fn list_data_access_grants(
        &self,
        limit: usize,
    ) -> Result<Vec<DataAccessGrant>, AgentFailure> {
        if limit == 0 || limit > MAX_ACCESS_GRANTS {
            return Err(AgentFailure::BudgetExceeded);
        }
        let connection = self.connection()?;
        self.ensure_access_grant_schema(&connection).await?;
        let mut rows = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE person_id = ? AND authority_owner = ? ORDER BY grant_id LIMIT ?", (self.person_id.to_string(), self.vault_id.to_string(), i64::try_from(limit).map_err(|_| AgentFailure::BudgetExceeded)?)).await.map_err(storage)?;
        let mut grants = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            grants.push(
                decode_grant(&row).map_err(|failure| self.reject_access_corruption(failure))?,
            );
        }
        self.check_access()?;
        Ok(grants)
    }

    pub async fn activate_data_access_grant(
        &self,
        id: GrantId,
        expected: GrantAuthority,
        source: GrantSourceBinding,
        scope: GrantScope,
    ) -> Result<DataAccessGrant, AgentFailure> {
        self.mutate_data_access_grant(
            id,
            expected,
            AccessGrantMutation::Activate { source, scope },
        )
        .await
    }

    pub async fn pause_data_access_grant(
        &self,
        id: GrantId,
        expected: GrantAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        self.mutate_data_access_grant(id, expected, AccessGrantMutation::Pause)
            .await
    }

    pub async fn revoke_data_access_grant(
        &self,
        id: GrantId,
        expected: GrantAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        self.mutate_data_access_grant(id, expected, AccessGrantMutation::Revoke)
            .await
    }

    pub async fn pending_data_access_grant_cleanup(
        &self,
        limit: usize,
    ) -> Result<Vec<AccessGrantCleanup>, AgentFailure> {
        if limit == 0 || limit > MAX_CLEANUP_ITEMS {
            return Err(AgentFailure::BudgetExceeded);
        }
        let connection = self.connection()?;
        self.ensure_access_grant_schema(&connection).await?;
        let mut rows = connection.query("SELECT cleanup_id, grant_id, person_id, invalidated_incarnation, invalidated_epoch, payload FROM data_access_grant_cleanup WHERE person_id = ? ORDER BY invalidated_epoch, cleanup_id LIMIT ?", (self.person_id.to_string(), i64::try_from(limit).map_err(|_| AgentFailure::BudgetExceeded)?)).await.map_err(storage)?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let item = self
                .decode_cleanup_row(&row)
                .map_err(|failure| self.reject_access_corruption(failure))?;
            let mut grants = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ?", (item.grant_id.as_uuid().to_string(), self.person_id.to_string())).await.map_err(storage)?;
            let current = grants
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?;
            let current =
                decode_grant(&current).map_err(|failure| self.reject_access_corruption(failure))?;
            if current.authority_owner() != self.vault_id
                || current.authority().incarnation() != item.invalidated_authority.incarnation()
                || item.invalidated_authority.access_epoch() >= current.authority().access_epoch()
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            items.push(item);
        }
        self.check_access()?;
        Ok(items)
    }

    pub async fn acknowledge_data_access_grant_cleanup(
        &self,
        item: AccessGrantCleanup,
    ) -> Result<(), AgentFailure> {
        if !item.grant_id.is_valid() || !item.invalidated_authority.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = match connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
        {
            Ok(transaction) => transaction,
            Err(error) => {
                let failure = access_transaction_start_error(error);
                if failure != AgentFailure::Conflict {
                    self.latch_access_unavailable();
                }
                return Err(failure);
            }
        };
        let result = async {
            self.ensure_access_grant_schema_transaction(&transaction).await?;
            let changed = transaction.execute("DELETE FROM data_access_grant_cleanup WHERE cleanup_id = ? AND grant_id = ? AND person_id = ? AND invalidated_incarnation = ? AND invalidated_epoch = ?", (cleanup_id(&item), item.grant_id.as_uuid().to_string(), self.person_id.to_string(), item.invalidated_authority.incarnation().to_string(), integer(item.invalidated_authority.access_epoch().get())?)).await.map_err(storage)?;
            if changed > 1 { return Err(AgentFailure::VaultUnavailable); }
            self.check_access()?;
            Ok(())
        }.await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    async fn mutate_data_access_grant(
        &self,
        id: GrantId,
        expected: GrantAuthority,
        mutation: AccessGrantMutation,
    ) -> Result<DataAccessGrant, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = match connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
        {
            Ok(transaction) => transaction,
            Err(error) => {
                let failure = access_transaction_start_error(error);
                if failure != AgentFailure::Conflict {
                    self.latch_access_unavailable();
                }
                return Err(failure);
            }
        };
        let result = async {
            let result = self
                .mutate_data_access_grant_in_transaction(&transaction, id, expected, mutation)
                .await?;
            self.check_access()?;
            Ok(result)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub(super) async fn mutate_data_access_grant_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        id: GrantId,
        expected: GrantAuthority,
        mutation: AccessGrantMutation,
    ) -> Result<DataAccessGrant, AgentFailure> {
        self.ensure_access_grant_schema_transaction(transaction)
            .await?;
        let mut rows = transaction.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ? AND authority_owner = ?", (id.as_uuid().to_string(), self.person_id.to_string(), self.vault_id.to_string())).await.map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        let mut grant = decode_grant(&row)?;
        let previous = grant.clone();
        let changed = match mutation {
            AccessGrantMutation::Review { source, scope } => grant
                .review(expected, source, scope)
                .map_err(grant_transition)?,
            AccessGrantMutation::Activate { source, scope } => grant
                .activate_review(expected, source, scope)
                .map_err(grant_transition)?,
            AccessGrantMutation::Pause => grant.pause(expected).map_err(grant_transition)?,
            AccessGrantMutation::Revoke => grant.revoke(expected).map_err(grant_transition)?,
        };
        if changed {
            if previous.state() == GrantState::Active {
                let mut cleanup_count = transaction
                    .query("SELECT COUNT(*) FROM data_access_grant_cleanup", ())
                    .await
                    .map_err(storage)?;
                let cleanup_count = cleanup_count
                    .next()
                    .await
                    .map_err(storage)?
                    .ok_or(AgentFailure::VaultUnavailable)?
                    .get::<i64>(0)
                    .map_err(storage)?;
                if cleanup_count < 0
                    || usize::try_from(cleanup_count).map_err(|_| AgentFailure::VaultUnavailable)?
                        >= MAX_CLEANUP_ITEMS
                {
                    return Err(AgentFailure::BudgetExceeded);
                }
                let cleanup = AccessGrantCleanup {
                    grant_id: id,
                    invalidated_authority: previous.authority(),
                };
                transaction.execute("INSERT INTO data_access_grant_cleanup (cleanup_id, grant_id, person_id, invalidated_incarnation, invalidated_epoch, payload) VALUES (?, ?, ?, ?, ?, ?)", (cleanup_id(&cleanup), id.as_uuid().to_string(), self.person_id.to_string(), previous.authority().incarnation().to_string(), integer(previous.authority().access_epoch().get())?, serde_json::to_string(&cleanup).map_err(|_| AgentFailure::StorageUnavailable)?)).await.map_err(storage)?;
            }
            let payload = grant_payload(&grant)?;
            let updated = transaction.execute("UPDATE data_access_grants SET grant_incarnation = ?, access_epoch = ?, source_incarnation = ?, source_epoch = ?, state = ?, payload = ? WHERE grant_id = ? AND person_id = ? AND authority_owner = ? AND grant_incarnation = ? AND access_epoch = ?", (grant.authority().incarnation().to_string(), integer(grant.authority().access_epoch().get())?, grant.source().source_authority().incarnation().to_string(), integer(grant.source().source_authority().epoch().get())?, state_name(grant.state()), payload, id.as_uuid().to_string(), self.person_id.to_string(), self.vault_id.to_string(), expected.incarnation().to_string(), integer(expected.access_epoch().get())?)).await.map_err(storage)?;
            if updated != 1 {
                return Err(AgentFailure::Conflict);
            }
        }
        Ok(grant)
    }

    pub(super) async fn read_data_access_grant_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        id: GrantId,
    ) -> Result<DataAccessGrant, AgentFailure> {
        self.ensure_access_grant_schema_transaction(transaction)
            .await?;
        let mut rows = transaction
            .query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ? AND authority_owner = ?", (id.as_uuid().to_string(), self.person_id.to_string(), self.vault_id.to_string()))
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        decode_grant(&row)
    }

    async fn ensure_access_grant_schema(
        &self,
        connection: &turso::Connection,
    ) -> Result<(), AgentFailure> {
        let mut components = connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('data_access_grant_schema', 'data_access_grants', 'data_access_grant_cleanup')",
                (),
            )
            .await
            .map_err(storage)?;
        let mut component_count = 0;
        while components.next().await.map_err(storage)?.is_some() {
            component_count += 1;
        }
        if component_count != 3 {
            self.latch_access_unavailable();
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut rows = connection
            .query(
                "SELECT version FROM data_access_grant_schema WHERE id = 1",
                (),
            )
            .await
            .map_err(storage)?;
        let marker = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        if marker.get::<i64>(0).map_err(storage)? != ACCESS_GRANT_SCHEMA_VERSION
            || rows.next().await.map_err(storage)?.is_some()
        {
            self.latch_access_unavailable();
            return Err(AgentFailure::UnsupportedVersion);
        }
        Ok(())
    }

    async fn ensure_access_grant_schema_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
    ) -> Result<(), AgentFailure> {
        let mut components = transaction
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('data_access_grant_schema', 'data_access_grants', 'data_access_grant_cleanup')",
                (),
            )
            .await
            .map_err(storage)?;
        let mut component_count = 0;
        while components.next().await.map_err(storage)?.is_some() {
            component_count += 1;
        }
        if component_count != 3 {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut rows = transaction
            .query(
                "SELECT version FROM data_access_grant_schema WHERE id = 1",
                (),
            )
            .await
            .map_err(storage)?;
        let marker = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        if marker.get::<i64>(0).map_err(storage)? != ACCESS_GRANT_SCHEMA_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::UnsupportedVersion);
        }
        Ok(())
    }

    async fn validate_all_access_grants(
        &self,
        connection: &turso::Connection,
    ) -> Result<(), AgentFailure> {
        let mut rows = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants", ()).await.map_err(storage)?;
        let mut count = 0;
        while let Some(row) = rows.next().await.map_err(storage)? {
            count += 1;
            if count > MAX_ACCESS_GRANTS {
                return Err(AgentFailure::BudgetExceeded);
            }
            let grant = decode_grant(&row)?;
            if grant.source().person_id() != self.person_id
                || grant.authority_owner() != self.vault_id
            {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
        let mut cleanup_rows = connection.query("SELECT cleanup_id, grant_id, person_id, invalidated_incarnation, invalidated_epoch, payload FROM data_access_grant_cleanup", ()).await.map_err(storage)?;
        while let Some(row) = cleanup_rows.next().await.map_err(storage)? {
            let item = self.decode_cleanup_row(&row)?;
            let mut grants = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, source_incarnation, source_epoch, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ?", (item.grant_id.as_uuid().to_string(), self.person_id.to_string())).await.map_err(storage)?;
            let current = grants
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?;
            let current = decode_grant(&current)?;
            if current.authority_owner() != self.vault_id
                || current.authority().incarnation() != item.invalidated_authority.incarnation()
                || item.invalidated_authority.access_epoch() >= current.authority().access_epoch()
            {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
        Ok(())
    }

    fn decode_cleanup_row(&self, row: &Row) -> Result<AccessGrantCleanup, AgentFailure> {
        let item: AccessGrantCleanup = decode_payload(&row.get::<String>(5).map_err(storage)?)?;
        let indexed_grant = GrantId::from_uuid(
            Uuid::parse_str(&row.get::<String>(1).map_err(storage)?).map_err(storage)?,
        )
        .ok_or(AgentFailure::VaultUnavailable)?;
        let indexed_person = row.get::<String>(2).map_err(storage)?;
        let indexed_incarnation =
            Uuid::parse_str(&row.get::<String>(3).map_err(storage)?).map_err(storage)?;
        let indexed_epoch = u64::try_from(row.get::<i64>(4).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let indexed_authority = GrantAuthority::from_parts(
            indexed_incarnation,
            std::num::NonZeroU64::new(indexed_epoch).ok_or(AgentFailure::VaultUnavailable)?,
        )
        .ok_or(AgentFailure::VaultUnavailable)?;
        if !item.grant_id.is_valid()
            || !item.invalidated_authority.is_valid()
            || item.grant_id != indexed_grant
            || indexed_person != self.person_id.to_string()
            || item.invalidated_authority != indexed_authority
            || cleanup_id(&item) != row.get::<String>(0).map_err(storage)?
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(item)
    }

    fn latch_access_unavailable(&self) {
        self.unavailable
            .store(true, std::sync::atomic::Ordering::Release);
    }

    fn reject_access_corruption(&self, failure: AgentFailure) -> AgentFailure {
        if matches!(
            failure,
            AgentFailure::VaultUnavailable
                | AgentFailure::UnsupportedVersion
                | AgentFailure::BudgetExceeded
        ) {
            self.latch_access_unavailable();
        }
        failure
    }

    pub(super) async fn finish_access_grant_transaction<T>(
        &self,
        transaction: turso::transaction::Transaction<'_>,
        result: Result<T, AgentFailure>,
    ) -> Result<T, AgentFailure> {
        match result {
            Ok(value) => match transaction.commit().await {
                Ok(()) => {
                    if let Err(failure) = self.check_access() {
                        self.latch_access_unavailable();
                        Err(failure)
                    } else {
                        Ok(value)
                    }
                }
                Err(_) => {
                    self.latch_access_unavailable();
                    Err(AgentFailure::StorageUnavailable)
                }
            },
            Err(failure) => {
                let should_latch = matches!(
                    failure,
                    AgentFailure::StorageUnavailable
                        | AgentFailure::VaultUnavailable
                        | AgentFailure::UnsupportedVersion
                );
                if transaction.rollback().await.is_err() {
                    self.latch_access_unavailable();
                    return Err(AgentFailure::VaultUnavailable);
                }
                if should_latch {
                    self.latch_access_unavailable();
                }
                Err(failure)
            }
        }
    }
}

fn grant_input(_: GrantValidationError) -> AgentFailure {
    AgentFailure::InvalidInput
}
fn access_transaction_start_error(error: turso::Error) -> AgentFailure {
    match error {
        turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => AgentFailure::Conflict,
        _ => AgentFailure::StorageUnavailable,
    }
}
fn grant_transition(error: GrantTransitionError) -> AgentFailure {
    match error {
        GrantTransitionError::Conflict => AgentFailure::Conflict,
        GrantTransitionError::Terminal | GrantTransitionError::Identity => {
            AgentFailure::PolicyDenied
        }
        GrantTransitionError::Overflow => AgentFailure::Conflict,
        GrantTransitionError::Invalid(_) => AgentFailure::InvalidInput,
    }
}
fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::Conflict)
}
fn state_name(state: GrantState) -> &'static str {
    match state {
        GrantState::Paused => "paused",
        GrantState::Active => "active",
        GrantState::Revoked => "revoked",
    }
}
fn cleanup_id(item: &AccessGrantCleanup) -> String {
    format!(
        "{}:{}:{}",
        item.grant_id.as_uuid(),
        item.invalidated_authority.incarnation(),
        item.invalidated_authority.access_epoch()
    )
}
fn grant_payload(grant: &DataAccessGrant) -> Result<String, AgentFailure> {
    let payload = serde_json::to_string(grant).map_err(|_| AgentFailure::StorageUnavailable)?;
    if payload.len() > MAX_GRANT_PAYLOAD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}
fn decode_payload<T: for<'de> Deserialize<'de>>(payload: &str) -> Result<T, AgentFailure> {
    if payload.len() > MAX_GRANT_PAYLOAD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    serde_json::from_str(payload).map_err(|_| AgentFailure::VaultUnavailable)
}
fn grant_values(
    grant: &DataAccessGrant,
    payload: String,
) -> Result<
    (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        i64,
        String,
        String,
    ),
    AgentFailure,
> {
    Ok((
        grant.id().as_uuid().to_string(),
        grant.source().person_id().to_string(),
        grant.authority_owner().to_string(),
        grant.source().connection_id().as_str().to_owned(),
        grant.source().connector().as_str().to_string(),
        grant.source().execution_owner().as_str().to_string(),
        grant.source().source_authority().incarnation().to_string(),
        integer(grant.source().source_authority().epoch().get())?,
        grant.authority().incarnation().to_string(),
        integer(grant.authority().access_epoch().get())?,
        state_name(grant.state()).to_string(),
        payload,
    ))
}
fn decode_grant(row: &Row) -> Result<DataAccessGrant, AgentFailure> {
    let grant: DataAccessGrant = decode_payload(&row.get::<String>(11).map_err(storage)?)?;
    grant
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let id = GrantId::from_uuid(
        Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(storage)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    let person = floe_domain::PersonId(
        Uuid::parse_str(&row.get::<String>(1).map_err(storage)?).map_err(storage)?,
    );
    let owner = Uuid::parse_str(&row.get::<String>(2).map_err(storage)?).map_err(storage)?;
    let connection = ConnectionId::try_new(row.get::<String>(3).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let connector = ConnectorId::try_new(row.get::<String>(4).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let execution_owner = ExecutionOwnerId::try_new(row.get::<String>(5).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let source_incarnation =
        Uuid::parse_str(&row.get::<String>(6).map_err(storage)?).map_err(storage)?;
    let source_epoch = std::num::NonZeroU64::new(
        u64::try_from(row.get::<i64>(7).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    let grant_incarnation =
        Uuid::parse_str(&row.get::<String>(8).map_err(storage)?).map_err(storage)?;
    let access_epoch = std::num::NonZeroU64::new(
        u64::try_from(row.get::<i64>(9).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    if grant.id() != id
        || grant.source().person_id() != person
        || grant.authority_owner() != owner
        || grant.source().connection_id() != connection
        || grant.source().connector() != &connector
        || grant.source().execution_owner() != &execution_owner
        || grant.source().source_authority()
            != SourceAuthority::from_parts(source_incarnation, source_epoch)
                .ok_or(AgentFailure::VaultUnavailable)?
        || grant.authority()
            != GrantAuthority::from_parts(grant_incarnation, access_epoch)
                .ok_or(AgentFailure::VaultUnavailable)?
        || state_name(grant.state()) != row.get::<String>(10).map_err(storage)?
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(grant)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
        sync::{Arc, Mutex},
    };

    use floe_domain::{
        GrantDataCategory, GrantOperation, GrantPurpose, ProcessingRestriction, ResourceHandle,
    };

    use super::*;

    #[derive(Clone, Default)]
    struct TestKeys(Arc<TestKeyState>);

    #[derive(Default)]
    struct TestKeyState {
        values: Mutex<HashMap<(floe_domain::PersonId, Uuid), [u8; 32]>>,
        blocked: AtomicBool,
        calls: AtomicUsize,
        fail_on_call: AtomicUsize,
    }

    impl VaultKeyProvider for TestKeys {
        fn load(
            &self,
            person_id: floe_domain::PersonId,
            vault_id: Uuid,
        ) -> Result<VaultKey, AgentFailure> {
            let call = self.0.calls.fetch_add(1, Ordering::AcqRel) + 1;
            let fail_on_call = self.0.fail_on_call.load(Ordering::Acquire);
            if self.0.blocked.load(Ordering::Acquire) || (fail_on_call != 0 && call >= fail_on_call)
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            self.0
                .values
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }
        fn insert(
            &self,
            person_id: floe_domain::PersonId,
            vault_id: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .values
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    fn root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        root
    }

    fn request(person_id: floe_domain::PersonId) -> (GrantSourceBinding, GrantScope) {
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::new(),
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("opaque-host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Scheduling],
            vec![floe_domain::GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        (source, scope)
    }

    #[tokio::test]
    async fn lifecycle_is_paused_cas_checked_terminal_and_durable() {
        let root = root();
        let person_id = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let (source, scope) = request(person_id);
        let grant = vault
            .create_data_access_grant(source.clone(), scope.clone())
            .await
            .unwrap();
        assert_eq!(grant.state(), GrantState::Paused);
        assert_eq!(
            vault
                .pause_data_access_grant(grant.id(), grant.authority())
                .await
                .unwrap()
                .authority(),
            grant.authority()
        );
        let active = vault
            .activate_data_access_grant(
                grant.id(),
                grant.authority(),
                source.clone(),
                scope.clone(),
            )
            .await
            .unwrap();
        assert_eq!(active.state(), GrantState::Active);
        assert_eq!(
            vault
                .activate_data_access_grant(
                    active.id(),
                    grant.authority(),
                    source.clone(),
                    scope.clone()
                )
                .await
                .unwrap_err(),
            AgentFailure::Conflict
        );
        let paused = vault
            .pause_data_access_grant(active.id(), active.authority())
            .await
            .unwrap();
        let old_cleanup = vault.pending_data_access_grant_cleanup(8).await.unwrap();
        assert_eq!(old_cleanup.len(), 1);
        let resumed = vault
            .activate_data_access_grant(paused.id(), paused.authority(), source, scope)
            .await
            .unwrap();
        let revoked = vault
            .revoke_data_access_grant(resumed.id(), resumed.authority())
            .await
            .unwrap();
        assert_eq!(revoked.state(), GrantState::Revoked);
        vault
            .acknowledge_data_access_grant_cleanup(old_cleanup[0].clone())
            .await
            .unwrap();
        assert_eq!(
            vault
                .pending_data_access_grant_cleanup(8)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            vault
                .create_data_access_grant_with_id(
                    revoked.id(),
                    request(person_id).0,
                    request(person_id).1
                )
                .await
                .unwrap_err(),
            AgentFailure::Conflict
        );
        assert_eq!(
            vault
                .activate_data_access_grant(
                    revoked.id(),
                    revoked.authority(),
                    request(person_id).0,
                    request(person_id).1
                )
                .await
                .unwrap_err(),
            AgentFailure::PolicyDenied
        );
        vault.checkpoint().await.unwrap();
        drop(vault);
        let reopened = EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap();
        assert_eq!(
            reopened
                .get_data_access_grant(revoked.id())
                .await
                .unwrap()
                .state(),
            GrantState::Revoked
        );
        assert_eq!(
            reopened
                .pending_data_access_grant_cleanup(8)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn owner_and_key_binding_fail_closed() {
        let root = root();
        let person_id = floe_domain::PersonId::new();
        let other_person = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let (mut source, scope) = request(other_person);
        assert_eq!(
            vault
                .create_data_access_grant(source.clone(), scope.clone())
                .await
                .unwrap_err(),
            AgentFailure::NotFound
        );
        source = request(person_id).0;
        let grant = vault.create_data_access_grant(source, scope).await.unwrap();
        keys.0.blocked.store(true, Ordering::Release);
        assert_eq!(
            vault.get_data_access_grant(grant.id()).await.unwrap_err(),
            AgentFailure::VaultUnavailable
        );
    }

    #[tokio::test]
    async fn corrupt_marker_or_payload_fails_closed_on_reopen() {
        let root = root();
        let person_id = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let (source, scope) = request(person_id);
        let _grant = vault.create_data_access_grant(source, scope).await.unwrap();
        let connection = vault.database.connect().unwrap();
        connection
            .execute("DROP TABLE data_access_grant_schema", ())
            .await
            .unwrap();
        connection
            .execute(
                "CREATE TABLE data_access_grant_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL)",
                (),
            )
            .await
            .unwrap();
        connection
            .execute("INSERT INTO data_access_grant_schema VALUES (1, 9)", ())
            .await
            .unwrap();
        vault.checkpoint().await.unwrap();
        drop(vault);
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person_id, keys.clone()).await,
            Err(AgentFailure::UnsupportedVersion | AgentFailure::VaultUnavailable)
        ));

        let second_root = tempfile::tempdir().unwrap();
        fs::set_permissions(second_root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person_id = floe_domain::PersonId::new();
        let vault = EncryptedAgentVault::create(second_root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let (source, scope) = request(person_id);
        let grant = vault
            .create_data_access_grant(source.clone(), scope.clone())
            .await
            .unwrap();
        let active = vault
            .activate_data_access_grant(grant.id(), grant.authority(), source, scope)
            .await
            .unwrap();
        let connection = vault.database.connect().unwrap();
        let mut payload_rows = connection
            .query(
                "SELECT payload FROM data_access_grants WHERE grant_id = ?",
                [active.id().as_uuid().to_string()],
            )
            .await
            .unwrap();
        let payload = payload_rows
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<String>(0)
            .unwrap();
        let mut payload_value: serde_json::Value = serde_json::from_str(&payload).unwrap();
        payload_value["review_required"] = serde_json::json!(true);
        connection
            .execute(
                "UPDATE data_access_grants SET payload = ? WHERE grant_id = ?",
                (
                    serde_json::to_string(&payload_value).unwrap(),
                    active.id().as_uuid().to_string(),
                ),
            )
            .await
            .unwrap();
        vault.checkpoint().await.unwrap();
        drop(vault);
        assert_eq!(
            EncryptedAgentVault::open(second_root.path(), person_id, keys)
                .await
                .err(),
            Some(AgentFailure::VaultUnavailable)
        );
    }

    #[tokio::test]
    async fn competing_mutations_accept_only_one_expected_authority() {
        let root = root();
        let person_id = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys)
            .await
            .unwrap();
        let (source, scope) = request(person_id);
        let paused = vault
            .create_data_access_grant(source.clone(), scope.clone())
            .await
            .unwrap();
        let active = vault
            .activate_data_access_grant(paused.id(), paused.authority(), source, scope)
            .await
            .unwrap();
        let first = vault.pause_data_access_grant(active.id(), active.authority());
        let second = vault.pause_data_access_grant(active.id(), active.authority());
        let (first, second) = tokio::join!(first, second);
        assert_eq!((first.is_ok() as usize) + (second.is_ok() as usize), 1);
        assert_eq!(
            vault
                .get_data_access_grant(active.id())
                .await
                .unwrap()
                .state(),
            GrantState::Paused
        );
    }

    #[tokio::test]
    async fn write_constraint_rolls_back_state_and_cleanup_then_latches_vault() {
        let root = root();
        let person_id = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let (source, scope) = request(person_id);
        let initial = vault
            .create_data_access_grant(source.clone(), scope.clone())
            .await
            .unwrap();
        let first = vault
            .activate_data_access_grant(initial.id(), initial.authority(), source, scope)
            .await
            .unwrap();
        let (second_source, second_scope) = request(person_id);
        let second = vault
            .create_data_access_grant(second_source, second_scope)
            .await
            .unwrap();
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "CREATE UNIQUE INDEX grant_test_state ON data_access_grants(state)",
            (),
        )
        .await
        .unwrap();
        assert_eq!(
            vault
                .pause_data_access_grant(first.id(), first.authority())
                .await
                .err(),
            Some(AgentFailure::StorageUnavailable)
        );
        assert_eq!(
            vault.get_data_access_grant(first.id()).await.err(),
            Some(AgentFailure::VaultUnavailable)
        );
        let mut state_rows = raw
            .query(
                "SELECT state FROM data_access_grants WHERE grant_id = ?",
                [first.id().as_uuid().to_string()],
            )
            .await
            .unwrap();
        assert_eq!(
            state_rows
                .next()
                .await
                .unwrap()
                .unwrap()
                .get::<String>(0)
                .unwrap(),
            "active"
        );
        let mut cleanup_rows = raw
            .query(
                "SELECT COUNT(*) FROM data_access_grant_cleanup WHERE grant_id = ?",
                [first.id().as_uuid().to_string()],
            )
            .await
            .unwrap();
        assert_eq!(
            cleanup_rows
                .next()
                .await
                .unwrap()
                .unwrap()
                .get::<i64>(0)
                .unwrap(),
            0
        );
        raw.execute("DROP INDEX grant_test_state", ())
            .await
            .unwrap();
        drop(vault);
        let reopened = EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap();
        assert_eq!(
            reopened
                .get_data_access_grant(first.id())
                .await
                .unwrap()
                .state(),
            GrantState::Active
        );
        assert_eq!(
            reopened
                .get_data_access_grant(second.id())
                .await
                .unwrap()
                .state(),
            GrantState::Paused
        );
        assert!(
            reopened
                .pending_data_access_grant_cleanup(8)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn key_loss_during_precommit_rolls_back_and_latches_vault() {
        let root = root();
        let person_id = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let (source, scope) = request(person_id);
        let paused = vault
            .create_data_access_grant(source.clone(), scope.clone())
            .await
            .unwrap();
        let active = vault
            .activate_data_access_grant(paused.id(), paused.authority(), source, scope)
            .await
            .unwrap();
        let fail_on_call = keys.0.calls.load(Ordering::Acquire) + 2;
        keys.0.fail_on_call.store(fail_on_call, Ordering::Release);
        assert_eq!(
            vault
                .pause_data_access_grant(active.id(), active.authority())
                .await
                .err(),
            Some(AgentFailure::VaultUnavailable)
        );
        keys.0.fail_on_call.store(0, Ordering::Release);
        let raw = vault.database.connect().unwrap();
        let mut state_rows = raw
            .query(
                "SELECT state FROM data_access_grants WHERE grant_id = ?",
                [active.id().as_uuid().to_string()],
            )
            .await
            .unwrap();
        assert_eq!(
            state_rows
                .next()
                .await
                .unwrap()
                .unwrap()
                .get::<String>(0)
                .unwrap(),
            "active"
        );
        let mut cleanup_rows = raw
            .query("SELECT COUNT(*) FROM data_access_grant_cleanup", ())
            .await
            .unwrap();
        assert_eq!(
            cleanup_rows
                .next()
                .await
                .unwrap()
                .unwrap()
                .get::<i64>(0)
                .unwrap(),
            0
        );
        drop(vault);
        let reopened = EncryptedAgentVault::open(root.path(), person_id, keys)
            .await
            .unwrap();
        assert_eq!(
            reopened
                .get_data_access_grant(active.id())
                .await
                .unwrap()
                .state(),
            GrantState::Active
        );
    }

    #[tokio::test]
    async fn missing_component_and_cleanup_mismatch_fail_closed() {
        let first_root = root();
        let person_id = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(first_root.path(), person_id, keys.clone())
            .await
            .unwrap();
        let (source, scope) = request(person_id);
        let grant = vault
            .create_data_access_grant(source.clone(), scope.clone())
            .await
            .unwrap();
        let active = vault
            .activate_data_access_grant(grant.id(), grant.authority(), source, scope)
            .await
            .unwrap();
        vault
            .pause_data_access_grant(active.id(), active.authority())
            .await
            .unwrap();
        let raw = vault.database.connect().unwrap();
        raw.execute(
            "UPDATE data_access_grant_cleanup SET payload = '{}' WHERE grant_id = ?",
            [active.id().as_uuid().to_string()],
        )
        .await
        .unwrap();
        assert_eq!(
            vault.pending_data_access_grant_cleanup(8).await.err(),
            Some(AgentFailure::VaultUnavailable)
        );
        drop(vault);
        assert_eq!(
            EncryptedAgentVault::open(first_root.path(), person_id, keys)
                .await
                .err(),
            Some(AgentFailure::VaultUnavailable)
        );

        let missing_root = root();
        let missing_person = floe_domain::PersonId::new();
        let missing_keys = TestKeys::default();
        let missing_vault =
            EncryptedAgentVault::create(missing_root.path(), missing_person, missing_keys.clone())
                .await
                .unwrap();
        let missing_raw = missing_vault.database.connect().unwrap();
        missing_raw
            .execute("DROP TABLE data_access_grants", ())
            .await
            .unwrap();
        missing_vault.checkpoint().await.unwrap();
        drop(missing_vault);
        assert_eq!(
            EncryptedAgentVault::open(missing_root.path(), missing_person, missing_keys)
                .await
                .err(),
            Some(AgentFailure::VaultUnavailable)
        );
    }

    #[tokio::test]
    async fn capacity_reopen_and_ciphertext_boundaries_hold() {
        let capacity_root = root();
        let person_id = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(capacity_root.path(), person_id, keys.clone())
            .await
            .unwrap();
        for _index in 0..MAX_ACCESS_GRANTS {
            let (source, scope) = request(person_id);
            vault.create_data_access_grant(source, scope).await.unwrap();
        }
        let (source, scope) = request(person_id);
        assert_eq!(
            vault.create_data_access_grant(source, scope).await.err(),
            Some(AgentFailure::BudgetExceeded)
        );
        vault.checkpoint().await.unwrap();
        drop(vault);
        let reopened = EncryptedAgentVault::open(capacity_root.path(), person_id, keys)
            .await
            .unwrap();
        assert_eq!(
            reopened
                .list_data_access_grants(MAX_ACCESS_GRANTS)
                .await
                .unwrap()
                .len(),
            MAX_ACCESS_GRANTS
        );

        let sentinel_root = root();
        let sentinel_person = floe_domain::PersonId::new();
        let sentinel_keys = TestKeys::default();
        let sentinel_vault =
            EncryptedAgentVault::create(sentinel_root.path(), sentinel_person, sentinel_keys)
                .await
                .unwrap();
        let sentinel = "resource-sentinel-not-plaintext";
        let (source, _) = request(sentinel_person);
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new(sentinel).unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Scheduling],
            vec![floe_domain::GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        sentinel_vault
            .create_data_access_grant(source, scope)
            .await
            .unwrap();
        sentinel_vault.checkpoint().await.unwrap();
        let directory = sentinel_root.path().join(sentinel_person.to_string());
        for filename in ["sessions.db", "sessions.db-wal", "sessions.db-shm"] {
            let path = directory.join(filename);
            if let Ok(bytes) = fs::read(path) {
                assert!(
                    !bytes
                        .windows(sentinel.len())
                        .any(|window| window == sentinel.as_bytes())
                );
            }
        }
    }
}
