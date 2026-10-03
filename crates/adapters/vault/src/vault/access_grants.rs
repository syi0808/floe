use chrono::Utc;
pub(super) use floe_access::AccessGrantMutation;
use floe_access::{
    ConnectionId, ConnectorId, DataAccessGrant, ExecutionOwnerId, GrantAuthority, GrantId,
    GrantPolicyError, GrantScope, GrantSourceBinding, GrantState, GrantTransitionError,
    GrantValidationError, apply_grant_mutation, create_grant,
};
use serde::{Deserialize, Serialize};
use turso::{Row, transaction::TransactionBehavior};
use uuid::Uuid;

use super::*;

const ACCESS_GRANT_SCHEMA_VERSION: i64 = 2;
const MAX_ACCESS_GRANTS: usize = 128;
const MAX_CLEANUP_ITEMS: usize = 256;
const MAX_GRANT_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessGrantCleanup {
    pub grant_id: GrantId,
    pub invalidated_authority: GrantAuthority,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn data_access_grant_for_source_resource(
        &self,
        source: &GrantSourceBinding,
        resource: &floe_access::ResourceHandle,
    ) -> Result<Option<DataAccessGrant>, AgentFailure> {
        let grants = self
            .data_access_grants_for_source(source, MAX_ACCESS_GRANTS)
            .await?;
        exact_source_resource_grant(grants, resource)
    }

    pub(super) async fn data_access_grant_for_source_resource_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        source: &GrantSourceBinding,
        resource: &floe_access::ResourceHandle,
    ) -> Result<Option<DataAccessGrant>, AgentFailure> {
        let grants = self
            .data_access_grants_for_source_in_transaction(transaction, source, MAX_ACCESS_GRANTS)
            .await?;
        exact_source_resource_grant(grants, resource)
    }

    pub(super) async fn data_access_grants_for_source_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        source: &GrantSourceBinding,
        limit: usize,
    ) -> Result<Vec<DataAccessGrant>, AgentFailure> {
        if limit == 0 || limit > MAX_ACCESS_GRANTS {
            return Err(AgentFailure::BudgetExceeded);
        }
        if source.person_id() != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        self.ensure_access_grant_schema_transaction(transaction)
            .await?;
        let mut rows = transaction
            .query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE person_id = ? AND authority_owner = ? AND connection_id = ? AND connector = ? AND execution_owner = ? ORDER BY access_epoch DESC, grant_id LIMIT ?", (self.person_id.to_string(), self.vault_id.to_string(), source.connection_id().as_str().to_owned(), source.connector().as_str().to_owned(), source.execution_owner().as_str().to_owned(), i64::try_from(limit).map_err(|_| AgentFailure::BudgetExceeded)?))
            .await
            .map_err(storage)?;
        let mut grants = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let grant = decode_grant(&row)?;
            if grant.source() != source {
                return Err(AgentFailure::VaultUnavailable);
            }
            grants.push(grant);
        }
        Ok(grants)
    }

    pub(super) async fn validate_current_grant_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        dependency: &floe_access::ContextDependency,
    ) -> Result<(), AgentFailure> {
        if dependency.person_id() != self.person_id || dependency.expires_at() <= Utc::now() {
            return Err(AgentFailure::PolicyDenied);
        }
        let grant = self
            .read_data_access_grant_in_transaction(transaction, dependency.grant_id())
            .await
            .map_err(|error| match error {
                AgentFailure::NotFound => AgentFailure::PolicyDenied,
                other => other,
            })?;
        floe_access::validate_grant_dependency(&grant, dependency)
    }

    pub(super) async fn validate_context_dependency_coverage_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        coverage: &floe_access::DependencyCoverage,
    ) -> Result<(), AgentFailure> {
        let floe_access::DependencyCoverage::Dependent { dependencies } = coverage else {
            return Ok(());
        };
        for dependency in dependencies {
            self.validate_current_grant_in_transaction(transaction, dependency)
                .await?;
        }
        Ok(())
    }

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
            transaction.execute("CREATE TABLE data_access_grant_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 2))", ()).await.map_err(storage)?;
            transaction
                .execute(
                    "INSERT INTO data_access_grant_schema (id, version) VALUES (1, 2)",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction.execute("CREATE TABLE data_access_grants (grant_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, authority_owner TEXT NOT NULL, connection_id TEXT NOT NULL, connector TEXT NOT NULL, execution_owner TEXT NOT NULL, grant_incarnation TEXT NOT NULL, access_epoch INTEGER NOT NULL, state TEXT NOT NULL, payload TEXT NOT NULL)", ()).await.map_err(storage)?;
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

    pub async fn get_data_access_grant(
        &self,
        id: GrantId,
    ) -> Result<DataAccessGrant, AgentFailure> {
        let connection = self.connection()?;
        self.ensure_access_grant_schema(&connection).await?;
        let mut rows = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ? AND authority_owner = ?", (id.as_uuid().to_string(), self.person_id.to_string(), self.vault_id.to_string())).await.map_err(storage)?;
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
        let mut rows = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE person_id = ? AND authority_owner = ? ORDER BY grant_id LIMIT ?", (self.person_id.to_string(), self.vault_id.to_string(), i64::try_from(limit).map_err(|_| AgentFailure::BudgetExceeded)?)).await.map_err(storage)?;
        let mut grants = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            grants.push(
                decode_grant(&row).map_err(|failure| self.reject_access_corruption(failure))?,
            );
        }
        self.check_access()?;
        Ok(grants)
    }

    /// Every grant for one stable source identity, newest grant authority first.
    /// Callers requiring one logical resource must filter it and reject ambiguity.
    pub async fn data_access_grants_for_source(
        &self,
        source: &GrantSourceBinding,
        limit: usize,
    ) -> Result<Vec<DataAccessGrant>, AgentFailure> {
        if limit == 0 || limit > MAX_ACCESS_GRANTS {
            return Err(AgentFailure::BudgetExceeded);
        }
        if source.person_id() != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        let connection = self.connection()?;
        self.ensure_access_grant_schema(&connection).await?;
        let mut rows = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE person_id = ? AND authority_owner = ? AND connection_id = ? AND connector = ? AND execution_owner = ? ORDER BY access_epoch DESC, grant_id LIMIT ?", (self.person_id.to_string(), self.vault_id.to_string(), source.connection_id().as_str().to_owned(), source.connector().as_str().to_owned(), source.execution_owner().as_str().to_owned(), i64::try_from(limit).map_err(|_| AgentFailure::BudgetExceeded)?)).await.map_err(storage)?;
        let mut grants = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let grant =
                decode_grant(&row).map_err(|failure| self.reject_access_corruption(failure))?;
            if grant.source() != source {
                return Err(AgentFailure::VaultUnavailable);
            }
            grants.push(grant);
        }
        self.check_access()?;
        Ok(grants)
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
            let mut grants = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ?", (item.grant_id.as_uuid().to_string(), self.person_id.to_string())).await.map_err(storage)?;
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

    pub(super) async fn pending_data_access_grant_cleanup_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        limit: usize,
    ) -> Result<Vec<AccessGrantCleanup>, AgentFailure> {
        if limit == 0 || limit > MAX_CLEANUP_ITEMS {
            return Err(AgentFailure::BudgetExceeded);
        }
        self.ensure_access_grant_schema_transaction(transaction)
            .await?;
        let mut rows = transaction
            .query(
                "SELECT cleanup_id, grant_id, person_id, invalidated_incarnation, invalidated_epoch, payload FROM data_access_grant_cleanup WHERE person_id = ? ORDER BY invalidated_epoch, cleanup_id LIMIT ?",
                (
                    self.person_id.to_string(),
                    i64::try_from(limit).map_err(|_| AgentFailure::BudgetExceeded)?,
                ),
            )
            .await
            .map_err(storage)?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let item = self
                .decode_cleanup_row(&row)
                .map_err(|failure| self.reject_access_corruption(failure))?;
            let mut grants = transaction
                .query(
                    "SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ?",
                    (
                        item.grant_id.as_uuid().to_string(),
                        self.person_id.to_string(),
                    ),
                )
                .await
                .map_err(storage)?;
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
        Ok(items)
    }

    pub(super) async fn acknowledge_data_access_grant_cleanup_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        item: &AccessGrantCleanup,
    ) -> Result<bool, AgentFailure> {
        self.ensure_access_grant_schema_transaction(transaction)
            .await?;
        let changed = transaction
            .execute(
                "DELETE FROM data_access_grant_cleanup WHERE cleanup_id = ? AND grant_id = ? AND person_id = ? AND invalidated_incarnation = ? AND invalidated_epoch = ?",
                (
                    cleanup_id(item),
                    item.grant_id.as_uuid().to_string(),
                    self.person_id.to_string(),
                    item.invalidated_authority.incarnation().to_string(),
                    integer(item.invalidated_authority.access_epoch().get())?,
                ),
            )
            .await
            .map_err(storage)?;
        if changed > 1 {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(changed == 1)
    }

    /// Persist an Access-validated immutable successor under exact CAS. No policy
    /// is inferred here and no authority is generated by the adapter.
    pub(super) async fn write_reviewed_grant_on(
        &self, transaction: &turso::transaction::Transaction<'_>,
        mutation: &floe_access::GrantMutation, before: &[DataAccessGrant],
    ) -> Result<Option<String>, AgentFailure> {
        let grant = &mutation.successor;
        let payload = grant_payload(grant)?;
        match &mutation.expected {
            floe_access::ExpectedGrant::Absent { .. } => {
                let mut rows = transaction.query("SELECT COUNT(*) FROM data_access_grants", ()).await.map_err(storage)?;
                let count = rows.next().await.map_err(storage)?.ok_or(AgentFailure::VaultUnavailable)?.get::<i64>(0).map_err(storage)?;
                if count < 0 || count as usize >= MAX_ACCESS_GRANTS { return Err(AgentFailure::BudgetExceeded); }
                transaction.execute("INSERT INTO data_access_grants (grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", grant_values(grant, payload)?).await.map_err(storage)?;
                Ok(None)
            }
            floe_access::ExpectedGrant::Present { grant_id, authority, .. } => {
                let previous = before.iter().find(|grant| grant.id() == *grant_id && grant.authority() == *authority).ok_or(AgentFailure::Conflict)?;
                self.invalidate_agent_actions_for_grant_in_transaction(transaction, *grant_id, *authority).await?;
                let cleanup = AccessGrantCleanup { grant_id: *grant_id, invalidated_authority: *authority };
                let cleanup_key = if previous.state() == GrantState::Active {
                    let mut rows = transaction.query("SELECT COUNT(*) FROM data_access_grant_cleanup", ()).await.map_err(storage)?;
                    let count = rows.next().await.map_err(storage)?.ok_or(AgentFailure::VaultUnavailable)?.get::<i64>(0).map_err(storage)?;
                    if count < 0 || count as usize >= MAX_CLEANUP_ITEMS { return Err(AgentFailure::BudgetExceeded); }
                    let key = cleanup_id(&cleanup);
                    transaction.execute("INSERT INTO data_access_grant_cleanup (cleanup_id, grant_id, person_id, invalidated_incarnation, invalidated_epoch, payload) VALUES (?, ?, ?, ?, ?, ?)", (key.clone(), grant_id.as_uuid().to_string(), self.person_id.to_string(), authority.incarnation().to_string(), integer(authority.access_epoch().get())?, serde_json::to_string(&cleanup).map_err(|_| AgentFailure::StorageUnavailable)?)).await.map_err(storage)?;
                    Some(key)
                } else { None };
                let changed = transaction.execute("UPDATE data_access_grants SET grant_incarnation = ?, access_epoch = ?, state = ?, payload = ? WHERE grant_id = ? AND person_id = ? AND authority_owner = ? AND grant_incarnation = ? AND access_epoch = ?", (grant.authority().incarnation().to_string(), integer(grant.authority().access_epoch().get())?, state_name(grant.state()), payload, grant_id.as_uuid().to_string(), self.person_id.to_string(), self.vault_id.to_string(), authority.incarnation().to_string(), integer(authority.access_epoch().get())?)).await.map_err(storage)?;
                if changed != 1 { return Err(AgentFailure::Conflict); }
                Ok(cleanup_key)
            }
        }
    }

    pub(super) async fn read_data_access_grant_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        id: GrantId,
    ) -> Result<DataAccessGrant, AgentFailure> {
        self.ensure_access_grant_schema_transaction(transaction)
            .await?;
        let mut rows = transaction
            .query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ? AND authority_owner = ?", (id.as_uuid().to_string(), self.person_id.to_string(), self.vault_id.to_string()))
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

    pub(super) async fn ensure_access_grant_schema_transaction(
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
        let mut rows = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload FROM data_access_grants", ()).await.map_err(storage)?;
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
            let mut grants = connection.query("SELECT grant_id, person_id, authority_owner, connection_id, connector, execution_owner, grant_incarnation, access_epoch, state, payload FROM data_access_grants WHERE grant_id = ? AND person_id = ?", (item.grant_id.as_uuid().to_string(), self.person_id.to_string())).await.map_err(storage)?;
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
fn grant_policy(error: GrantPolicyError) -> AgentFailure {
    match error {
        GrantPolicyError::Unauthorized => AgentFailure::PolicyDenied,
        GrantPolicyError::Invalid(error) => grant_input(error),
        GrantPolicyError::Transition(error) => grant_transition(error),
    }
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
        GrantTransitionError::Terminal => AgentFailure::PolicyDenied,
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
fn exact_source_resource_grant(
    grants: Vec<DataAccessGrant>,
    resource: &floe_access::ResourceHandle,
) -> Result<Option<DataAccessGrant>, AgentFailure> {
    let mut current = None;
    for grant in grants {
        if grant.state() == GrantState::Revoked || !grant.scope().resources().contains(resource) {
            continue;
        }
        if grant.scope().resources() != [resource.clone()] {
            return Err(AgentFailure::PolicyDenied);
        }
        if current.replace(grant).is_some() {
            return Err(AgentFailure::Conflict);
        }
    }
    Ok(current)
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
        grant.authority().incarnation().to_string(),
        integer(grant.authority().access_epoch().get())?,
        state_name(grant.state()).to_string(),
        payload,
    ))
}
pub(super) fn decode_grant(row: &Row) -> Result<DataAccessGrant, AgentFailure> {
    let grant: DataAccessGrant = decode_payload(&row.get::<String>(9).map_err(storage)?)?;
    grant
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let id = GrantId::from_uuid(
        Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(storage)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    let person = floe_kernel::PersonId(
        Uuid::parse_str(&row.get::<String>(1).map_err(storage)?).map_err(storage)?,
    );
    let owner = Uuid::parse_str(&row.get::<String>(2).map_err(storage)?).map_err(storage)?;
    let connection = ConnectionId::try_new(row.get::<String>(3).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let connector = ConnectorId::try_new(row.get::<String>(4).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let execution_owner = ExecutionOwnerId::try_new(row.get::<String>(5).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let grant_incarnation =
        Uuid::parse_str(&row.get::<String>(6).map_err(storage)?).map_err(storage)?;
    let access_epoch = std::num::NonZeroU64::new(
        u64::try_from(row.get::<i64>(7).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    if grant.id() != id
        || grant.source().person_id() != person
        || grant.authority_owner() != owner
        || grant.source().connection_id() != connection
        || grant.source().connector() != &connector
        || grant.source().execution_owner() != &execution_owner
        || grant.authority()
            != GrantAuthority::from_parts(grant_incarnation, access_epoch)
                .ok_or(AgentFailure::VaultUnavailable)?
        || state_name(grant.state()) != row.get::<String>(8).map_err(storage)?
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(grant)
}
