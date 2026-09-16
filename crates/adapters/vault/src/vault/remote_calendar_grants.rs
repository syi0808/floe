use floe_agent_contract::AgentFailure;
use floe_access::{DataAccessGrant};
use floe_context_contract::{ConsumerPolicyAuthority, GrantAuthority, GrantId, GrantScope, GrantSourceBinding};
use serde::{Deserialize, Serialize};
use turso::transaction::TransactionBehavior;
use uuid::Uuid;

use super::{EncryptedAgentVault, VaultKeyProvider, access_grants::AccessGrantMutation};

const REMOTE_CALENDAR_GRANT_SCHEMA_VERSION: i64 = 1;
const MAX_REMOTE_CALENDAR_GRANTS: usize = 128;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RemoteCalendarGrantMapping {
    grant_id: GrantId,
    source: GrantSourceBinding,
    scope: GrantScope,
    policy_incarnation: Uuid,
    policy_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteCalendarGrantBinding {
    pub grant: DataAccessGrant,
    pub consumer_policy: ConsumerPolicyAuthority,
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn remote_calendar_grant_binding(
        &self,
        connector: &str,
        connection_id: &str,
        source_authority: floe_context_contract::SourceAuthority,
        resource: &str,
    ) -> Result<RemoteCalendarGrantBinding, AgentFailure> {
        if connector.is_empty() || connection_id.is_empty() || resource.is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT grant_id, policy_incarnation, policy_epoch, payload FROM remote_calendar_grant_mappings WHERE person_id = ? AND connector = ? AND connection_id = ? AND source_incarnation = ? AND source_epoch = ?",
                (
                    self.person_id.to_string(),
                    connector,
                    connection_id,
                    source_authority.incarnation().to_string(),
                    source_authority.epoch().get() as i64,
                ),
            )
            .await
            .map_err(storage)?;
        let first = rows.next().await.map_err(storage)?;
        let Some(row) = first else {
            return Err(AgentFailure::AccessReviewRequired);
        };
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::Conflict);
        }
        let encoded = row.get::<String>(3).map_err(storage)?;
        if encoded.len() > 16 * 1024 {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mapping: RemoteCalendarGrantMapping =
            serde_json::from_str(&encoded).map_err(|_| AgentFailure::VaultUnavailable)?;
        if row.get::<String>(0).map_err(storage)? != mapping.grant_id.as_uuid().to_string()
            || row.get::<String>(1).map_err(storage)? != mapping.policy_incarnation.to_string()
            || row.get::<i64>(2).map_err(storage)? != mapping.policy_epoch as i64
            || mapping.source.person_id() != self.person_id
            || mapping.source.connector().as_str() != connector
            || mapping.source.connection_id().as_str() != connection_id
            || mapping.source.source_authority() != source_authority
            || !mapping
                .scope
                .resources()
                .iter()
                .any(|candidate| candidate.as_str() == resource)
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let grant = self
            .get_data_access_grant(mapping.grant_id)
            .await
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        if grant.authority_owner() != self.vault_id
            || grant.state() != floe_access::GrantState::Active
            || grant.review_required()
            || grant.source() != &mapping.source
            || grant.scope() != &mapping.scope
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(RemoteCalendarGrantBinding {
            grant,
            consumer_policy: ConsumerPolicyAuthority::from_parts(
                mapping.policy_incarnation,
                std::num::NonZeroU64::new(mapping.policy_epoch)
                    .ok_or(AgentFailure::VaultUnavailable)?,
            )
            .ok_or(AgentFailure::VaultUnavailable)?,
        })
    }

    pub(super) async fn initialize_remote_calendar_grant_store(
        &self,
        fresh: bool,
    ) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result: Result<(), AgentFailure> = async {
            if fresh {
                transaction
                    .execute(
                        "CREATE TABLE remote_calendar_grant_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "INSERT INTO remote_calendar_grant_schema (id, version) VALUES (1, ?)",
                        [REMOTE_CALENDAR_GRANT_SCHEMA_VERSION],
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE TABLE remote_calendar_grant_mappings (grant_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, connector TEXT NOT NULL, connection_id TEXT NOT NULL, execution_owner TEXT NOT NULL, source_incarnation TEXT NOT NULL, source_epoch INTEGER NOT NULL, policy_incarnation TEXT NOT NULL, policy_epoch INTEGER NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, connector, connection_id, execution_owner, source_incarnation, source_epoch))",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                transaction
                    .execute(
                        "CREATE INDEX remote_calendar_grant_source ON remote_calendar_grant_mappings (person_id, connector, connection_id, source_incarnation, source_epoch)",
                        (),
                    )
                    .await
                    .map_err(storage)?;
            } else {
                let mut marker = transaction
                    .query(
                        "SELECT version FROM remote_calendar_grant_schema WHERE id = 1",
                        (),
                    )
                    .await
                    .map_err(storage)?;
                let version = marker
                    .next()
                    .await
                    .map_err(storage)?
                    .ok_or(AgentFailure::VaultUnavailable)?
                    .get::<i64>(0)
                    .map_err(storage)?;
                if version != REMOTE_CALENDAR_GRANT_SCHEMA_VERSION {
                    return Err(AgentFailure::UnsupportedVersion);
                }
                transaction
                    .query("SELECT * FROM remote_calendar_grant_mappings LIMIT 0", ())
                    .await
                    .map_err(storage)?;
            }
            Ok(())
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await?;
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT grant_id, person_id, connector, connection_id, execution_owner, source_incarnation, source_epoch, policy_incarnation, policy_epoch, payload FROM remote_calendar_grant_mappings",
                (),
            )
            .await
            .map_err(storage)?;
        let mut count = 0usize;
        while let Some(row) = rows.next().await.map_err(storage)? {
            count = count.saturating_add(1);
            if count > MAX_REMOTE_CALENDAR_GRANTS {
                return Err(AgentFailure::BudgetExceeded);
            }
            let encoded = row.get::<String>(9).map_err(storage)?;
            if encoded.len() > 16 * 1024 {
                return Err(AgentFailure::VaultUnavailable);
            }
            let mapping: RemoteCalendarGrantMapping =
                serde_json::from_str(&encoded).map_err(|_| AgentFailure::VaultUnavailable)?;
            if mapping.grant_id.as_uuid().to_string() != row.get::<String>(0).map_err(storage)?
                || self.person_id.to_string() != row.get::<String>(1).map_err(storage)?
                || mapping.source.connector().as_str() != row.get::<String>(2).map_err(storage)?
                || mapping.source.connection_id().as_str()
                    != row.get::<String>(3).map_err(storage)?
                || mapping.source.execution_owner().as_str()
                    != row.get::<String>(4).map_err(storage)?
                || mapping.source.source_authority().incarnation().to_string()
                    != row.get::<String>(5).map_err(storage)?
                || mapping.source.source_authority().epoch().get() as i64
                    != row.get::<i64>(6).map_err(storage)?
                || mapping.policy_incarnation.to_string()
                    != row.get::<String>(7).map_err(storage)?
                || mapping.policy_epoch as i64 != row.get::<i64>(8).map_err(storage)?
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            let grant = self
                .get_data_access_grant(mapping.grant_id)
                .await
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            if grant.authority_owner() != self.vault_id
                || grant.source() != &mapping.source
                || grant.scope() != &mapping.scope
            {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
        Ok(())
    }

    pub async fn review_and_activate_remote_calendar_grant(
        &self,
        grant_id: GrantId,
        expected: Option<GrantAuthority>,
        source: GrantSourceBinding,
        scope: GrantScope,
        expected_policy: Option<ConsumerPolicyAuthority>,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !grant_id.is_valid() || source.person_id() != self.person_id {
            return Err(AgentFailure::InvalidInput);
        }
        let creating_policy = expected_policy.is_none();
        let policy = expected_policy.unwrap_or_default();
        if !policy.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let policy_incarnation = policy.incarnation();
        let policy_epoch = policy.epoch().get();
        let mapping = RemoteCalendarGrantMapping {
            grant_id,
            source: source.clone(),
            scope: scope.clone(),
            policy_incarnation,
            policy_epoch,
        };
        let payload = serde_json::to_string(&mapping).map_err(|_| AgentFailure::InvalidInput)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            self.ensure_remote_calendar_mapping_schema(&transaction).await?;
            if creating_policy {
                let mut existing_policy = transaction
                    .query(
                        "SELECT grant_id FROM remote_calendar_grant_mappings WHERE grant_id = ?",
                        [grant_id.as_uuid().to_string()],
                    )
                    .await
                    .map_err(storage)?;
                if existing_policy.next().await.map_err(storage)?.is_some() {
                    return Err(AgentFailure::Conflict);
                }
            } else {
                let mut existing_policy = transaction
                    .query(
                        "SELECT policy_incarnation, policy_epoch FROM remote_calendar_grant_mappings WHERE grant_id = ? AND person_id = ?",
                        (grant_id.as_uuid().to_string(), self.person_id.to_string()),
                    )
                    .await
                    .map_err(storage)?;
                let existing_row = existing_policy
                    .next()
                    .await
                    .map_err(storage)?
                    .ok_or(AgentFailure::NotFound)?;
                if existing_row.get::<String>(0).map_err(storage)?
                    != policy_incarnation.to_string()
                    || existing_row.get::<i64>(1).map_err(storage)? != policy_epoch as i64
                {
                    return Err(AgentFailure::Conflict);
                }
            }
            let grant = match expected {
                Some(authority) => {
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        grant_id,
                        authority,
                        AccessGrantMutation::Activate { source, scope },
                    )
                    .await?
                }
                None => {
                    let existing = self
                        .create_data_access_grant_in_transaction(
                            &transaction,
                            grant_id,
                            source,
                            scope,
                        )
                        .await?;
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        grant_id,
                        existing.authority(),
                        AccessGrantMutation::Activate {
                            source: mapping.source.clone(),
                            scope: mapping.scope.clone(),
                        },
                    )
                    .await?
                }
            };
            let count = transaction
                .query("SELECT COUNT(*) FROM remote_calendar_grant_mappings", ())
                .await
                .map_err(storage)?
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?;
            let existing = transaction
                .execute(
                    "UPDATE remote_calendar_grant_mappings SET person_id = ?, connector = ?, connection_id = ?, execution_owner = ?, source_incarnation = ?, source_epoch = ?, policy_incarnation = ?, policy_epoch = ?, payload = ? WHERE grant_id = ?",
                    (self.person_id.to_string(), mapping.source.connector().as_str(), mapping.source.connection_id().as_str(), mapping.source.execution_owner().as_str(), mapping.source.source_authority().incarnation().to_string(), mapping.source.source_authority().epoch().get() as i64, policy_incarnation.to_string(), policy_epoch as i64, payload.clone(), grant_id.as_uuid().to_string()),
                )
                .await
                .map_err(storage)?;
            if existing == 0 {
                if usize::try_from(count).map_err(|_| AgentFailure::VaultUnavailable)?
                    >= MAX_REMOTE_CALENDAR_GRANTS
                {
                    return Err(AgentFailure::BudgetExceeded);
                }
                transaction
                    .execute(
                        "INSERT INTO remote_calendar_grant_mappings (grant_id, person_id, connector, connection_id, execution_owner, source_incarnation, source_epoch, policy_incarnation, policy_epoch, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        (grant_id.as_uuid().to_string(), self.person_id.to_string(), mapping.source.connector().as_str(), mapping.source.connection_id().as_str(), mapping.source.execution_owner().as_str(), mapping.source.source_authority().incarnation().to_string(), mapping.source.source_authority().epoch().get() as i64, policy_incarnation.to_string(), policy_epoch as i64, payload),
                    )
                    .await
                    .map_err(storage)?;
            }
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub async fn pause_remote_calendar_grant(
        &self,
        grant_id: GrantId,
        expected: GrantAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = self
            .mutate_data_access_grant_in_transaction(
                &transaction,
                grant_id,
                expected,
                AccessGrantMutation::Pause,
            )
            .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub(super) async fn ensure_remote_calendar_mapping_schema(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
    ) -> Result<(), AgentFailure> {
        transaction
            .query(
                "SELECT version FROM remote_calendar_grant_schema WHERE id = 1",
                (),
            )
            .await
            .map_err(storage)?
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        Ok(())
    }
}

use super::storage;
