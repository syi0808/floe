use floe_agent::AgentFailure;
use floe_domain::{
    ConsumerPolicyAuthority, DataAccessGrant, GrantAuthority, GrantId, GrantScope,
    GrantSourceBinding,
};
use serde::{Deserialize, Serialize};
use turso::transaction::TransactionBehavior;

use super::{EncryptedAgentVault, VaultKeyProvider, access_grants::AccessGrantMutation};

const SCHEMA_VERSION: i64 = 1;
const MAX_MAPPINGS: usize = 128;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RemoteViewGrantMapping {
    grant_id: GrantId,
    view_id: String,
    source: GrantSourceBinding,
    scope: GrantScope,
    policy_incarnation: uuid::Uuid,
    policy_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteViewGrantBinding {
    pub grant: DataAccessGrant,
    pub consumer_policy: ConsumerPolicyAuthority,
}

fn valid_view_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.bytes().all(|byte| byte.is_ascii_graphic())
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn get_remote_view_grant(
        &self,
        grant_id: GrantId,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !grant_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT grant_id FROM remote_view_grant_mappings WHERE person_id = ? AND grant_id = ?",
                (self.person_id.to_string(), grant_id.as_uuid().to_string()),
            )
            .await
            .map_err(super::storage)?;
        let Some(row) = rows.next().await.map_err(super::storage)? else {
            return Err(AgentFailure::NotFound);
        };
        if rows.next().await.map_err(super::storage)?.is_some()
            || row.get::<String>(0).map_err(super::storage)? != grant_id.as_uuid().to_string()
        {
            return Err(AgentFailure::PolicyDenied);
        }
        self.get_data_access_grant(grant_id).await
    }

    pub async fn find_remote_view_grant(
        &self,
        view_id: &str,
        source: &GrantSourceBinding,
        consumer: &str,
    ) -> Result<Option<DataAccessGrant>, AgentFailure> {
        if !valid_view_id(view_id) || source.person_id() != self.person_id || consumer.is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT grant_id, payload FROM remote_view_grant_mappings WHERE person_id = ? AND view_id = ? AND connector = ? AND connection_id = ? AND source_incarnation = ? AND source_epoch = ?",
                (
                    self.person_id.to_string(),
                    view_id,
                    source.connector().as_str(),
                    source.connection_id().as_str(),
                    source.source_authority().incarnation().to_string(),
                    source.source_authority().epoch().get() as i64,
                ),
            )
            .await
            .map_err(super::storage)?;
        let mut found = None;
        while let Some(row) = rows.next().await.map_err(super::storage)? {
            let encoded = row.get::<String>(1).map_err(super::storage)?;
            let mapping: RemoteViewGrantMapping =
                serde_json::from_str(&encoded).map_err(|_| AgentFailure::VaultUnavailable)?;
            if mapping.source != *source
                || !mapping
                    .scope
                    .consumers()
                    .iter()
                    .any(|item| item.identifier() == consumer)
            {
                continue;
            }
            if found.is_some() {
                return Err(AgentFailure::Conflict);
            }
            let grant_id = mapping.grant_id;
            found = Some(self.get_data_access_grant(grant_id).await?);
        }
        Ok(found)
    }

    pub async fn pause_remote_view_grant(
        &self,
        grant_id: GrantId,
        expected: GrantAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        let grant = self.get_remote_view_grant(grant_id).await?;
        if grant.authority() != expected {
            return Err(AgentFailure::Conflict);
        }
        self.pause_data_access_grant(grant_id, expected).await
    }

    pub(super) async fn validate_remote_view_dependency_policy_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        dependency: &floe_domain::ContextDependency,
    ) -> Result<(), AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT policy_incarnation, policy_epoch, source_incarnation, source_epoch FROM remote_view_grant_mappings WHERE grant_id = ? AND person_id = ?",
                (dependency.grant_id().as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(super::storage)?;
        let row = rows
            .next()
            .await
            .map_err(super::storage)?
            .ok_or(AgentFailure::PolicyDenied)?;
        if rows.next().await.map_err(super::storage)?.is_some()
            || row.get::<String>(0).map_err(super::storage)?
                != dependency.consumer_policy().incarnation().to_string()
            || row.get::<i64>(1).map_err(super::storage)?
                != i64::try_from(dependency.consumer_policy().epoch().get())
                    .map_err(|_| AgentFailure::PolicyDenied)?
            || row.get::<String>(2).map_err(super::storage)?
                != dependency
                    .source()
                    .source_authority()
                    .incarnation()
                    .to_string()
            || row.get::<i64>(3).map_err(super::storage)?
                != i64::try_from(dependency.source().source_authority().epoch().get())
                    .map_err(|_| AgentFailure::PolicyDenied)?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    pub async fn remote_view_grant_binding(
        &self,
        view_id: &str,
        connector: &str,
        connection_id: &str,
        source_authority: floe_domain::SourceAuthority,
    ) -> Result<RemoteViewGrantBinding, AgentFailure> {
        if !valid_view_id(view_id) || connector.is_empty() || connection_id.is_empty() {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT grant_id, policy_incarnation, policy_epoch, payload FROM remote_view_grant_mappings WHERE person_id = ? AND view_id = ? AND connector = ? AND connection_id = ? AND source_incarnation = ? AND source_epoch = ?",
                (
                    self.person_id.to_string(),
                    view_id,
                    connector,
                    connection_id,
                    source_authority.incarnation().to_string(),
                    source_authority.epoch().get() as i64,
                ),
            )
            .await
            .map_err(super::storage)?;
        let row = rows.next().await.map_err(super::storage)?;
        let Some(row) = row else {
            return Err(AgentFailure::AccessReviewRequired);
        };
        if rows.next().await.map_err(super::storage)?.is_some() {
            return Err(AgentFailure::Conflict);
        }
        let encoded = row.get::<String>(3).map_err(super::storage)?;
        if encoded.len() > 16 * 1024 {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mapping: RemoteViewGrantMapping =
            serde_json::from_str(&encoded).map_err(|_| AgentFailure::VaultUnavailable)?;
        if mapping.grant_id.as_uuid().to_string() != row.get::<String>(0).map_err(super::storage)?
            || mapping.policy_incarnation.to_string()
                != row.get::<String>(1).map_err(super::storage)?
            || mapping.policy_epoch as i64 != row.get::<i64>(2).map_err(super::storage)?
            || mapping.view_id != view_id
            || mapping.source.person_id() != self.person_id
            || mapping.source.connector().as_str() != connector
            || mapping.source.connection_id().as_str() != connection_id
            || mapping.source.source_authority() != source_authority
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let grant = self
            .get_data_access_grant(mapping.grant_id)
            .await
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        if grant.authority_owner() != self.vault_id
            || grant.state() != floe_domain::GrantState::Active
            || grant.review_required()
            || grant.source() != &mapping.source
            || grant.scope() != &mapping.scope
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(RemoteViewGrantBinding {
            grant,
            consumer_policy: ConsumerPolicyAuthority::from_parts(
                mapping.policy_incarnation,
                std::num::NonZeroU64::new(mapping.policy_epoch)
                    .ok_or(AgentFailure::VaultUnavailable)?,
            )
            .ok_or(AgentFailure::VaultUnavailable)?,
        })
    }

    pub async fn review_and_activate_remote_view_grant(
        &self,
        view_id: &str,
        grant_id: GrantId,
        expected: Option<GrantAuthority>,
        source: GrantSourceBinding,
        scope: GrantScope,
        expected_policy: Option<ConsumerPolicyAuthority>,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if !valid_view_id(view_id) || !grant_id.is_valid() || source.person_id() != self.person_id {
            return Err(AgentFailure::InvalidInput);
        }
        let policy = expected_policy.unwrap_or_default();
        if !policy.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let mapping = RemoteViewGrantMapping {
            grant_id,
            view_id: view_id.to_owned(),
            source: source.clone(),
            scope: scope.clone(),
            policy_incarnation: policy.incarnation(),
            policy_epoch: policy.epoch().get(),
        };
        let payload = serde_json::to_string(&mapping).map_err(|_| AgentFailure::InvalidInput)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(super::storage)?;
        let result = async {
            self.ensure_remote_view_schema(&transaction).await?;
            let existing = transaction
                .query(
                    "SELECT policy_incarnation, policy_epoch FROM remote_view_grant_mappings WHERE grant_id = ? AND person_id = ?",
                    (grant_id.as_uuid().to_string(), self.person_id.to_string()),
                )
                .await
                .map_err(super::storage)?
                .next()
                .await
                .map_err(super::storage)?;
            if let Some(row) = existing {
                if expected.is_none()
                    || row.get::<String>(0).map_err(super::storage)? != policy.incarnation().to_string()
                    || row.get::<i64>(1).map_err(super::storage)? != policy.epoch().get() as i64
                {
                    return Err(AgentFailure::Conflict);
                }
            } else if expected.is_some() {
                return Err(AgentFailure::NotFound);
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
                    let created = self
                        .create_data_access_grant_in_transaction(
                            &transaction,
                            grant_id,
                            source.clone(),
                            scope.clone(),
                        )
                        .await?;
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        grant_id,
                        created.authority(),
                        AccessGrantMutation::Activate { source, scope },
                    )
                    .await?
                }
            };
            let count = transaction
                .query("SELECT COUNT(*) FROM remote_view_grant_mappings", ())
                .await
                .map_err(super::storage)?
                .next()
                .await
                .map_err(super::storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(super::storage)?;
            let updated = transaction
                .execute(
                    "UPDATE remote_view_grant_mappings SET view_id = ?, connector = ?, connection_id = ?, execution_owner = ?, source_incarnation = ?, source_epoch = ?, policy_incarnation = ?, policy_epoch = ?, payload = ? WHERE grant_id = ? AND person_id = ?",
                    (view_id, mapping.source.connector().as_str(), mapping.source.connection_id().as_str(), mapping.source.execution_owner().as_str(), mapping.source.source_authority().incarnation().to_string(), mapping.source.source_authority().epoch().get() as i64, mapping.policy_incarnation.to_string(), mapping.policy_epoch as i64, payload.clone(), grant_id.as_uuid().to_string(), self.person_id.to_string()),
                )
                .await
                .map_err(super::storage)?;
            if updated == 0 {
                if usize::try_from(count).map_err(|_| AgentFailure::VaultUnavailable)? >= MAX_MAPPINGS {
                    return Err(AgentFailure::BudgetExceeded);
                }
                transaction
                    .execute(
                        "INSERT INTO remote_view_grant_mappings (grant_id, person_id, view_id, connector, connection_id, execution_owner, source_incarnation, source_epoch, policy_incarnation, policy_epoch, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        (grant_id.as_uuid().to_string(), self.person_id.to_string(), view_id, mapping.source.connector().as_str(), mapping.source.connection_id().as_str(), mapping.source.execution_owner().as_str(), mapping.source.source_authority().incarnation().to_string(), mapping.source.source_authority().epoch().get() as i64, mapping.policy_incarnation.to_string(), mapping.policy_epoch as i64, payload),
                    )
                    .await
                    .map_err(super::storage)?;
            }
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub(super) async fn initialize_remote_view_grant_store(
        &self,
        fresh: bool,
    ) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(super::storage)?;
        let result = async {
            if fresh {
                transaction.execute("CREATE TABLE remote_view_grant_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))", ()).await.map_err(super::storage)?;
                transaction.execute("INSERT INTO remote_view_grant_schema (id, version) VALUES (1, ?)", [SCHEMA_VERSION]).await.map_err(super::storage)?;
                transaction.execute("CREATE TABLE remote_view_grant_mappings (grant_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, view_id TEXT NOT NULL, connector TEXT NOT NULL, connection_id TEXT NOT NULL, execution_owner TEXT NOT NULL, source_incarnation TEXT NOT NULL, source_epoch INTEGER NOT NULL, policy_incarnation TEXT NOT NULL, policy_epoch INTEGER NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, view_id, connector, connection_id, execution_owner, source_incarnation, source_epoch))", ()).await.map_err(super::storage)?;
                transaction.execute("CREATE INDEX remote_view_grant_source ON remote_view_grant_mappings (person_id, view_id, connector, connection_id, source_incarnation, source_epoch)", ()).await.map_err(super::storage)?;
            } else {
                let mut marker = transaction.query("SELECT version FROM remote_view_grant_schema WHERE id = 1", ()).await.map_err(super::storage)?;
                if marker.next().await.map_err(super::storage)?.ok_or(AgentFailure::VaultUnavailable)?.get::<i64>(0).map_err(super::storage)? != SCHEMA_VERSION { return Err(AgentFailure::UnsupportedVersion); }
                transaction.query("SELECT * FROM remote_view_grant_mappings LIMIT 0", ()).await.map_err(super::storage)?;
            }
            Ok(())
        }.await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    async fn ensure_remote_view_schema(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
    ) -> Result<(), AgentFailure> {
        transaction
            .query(
                "SELECT version FROM remote_view_grant_schema WHERE id = 1",
                (),
            )
            .await
            .map_err(super::storage)?
            .next()
            .await
            .map_err(super::storage)?
            .ok_or(AgentFailure::VaultUnavailable)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
    };

    use floe_domain::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer, GrantDataCategory,
        GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, PersonId,
        ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use uuid::Uuid;

    use super::*;
    use crate::agent_vault::VaultKey;

    #[derive(Clone, Default)]
    struct TestKeys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for TestKeys {
        fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person, vault))
                .copied()
                .map(VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }
        fn insert(
            &self,
            person: PersonId,
            vault: Uuid,
            key: &VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person, vault), *key.as_bytes());
            Ok(())
        }
    }

    fn source(person: PersonId, authority: SourceAuthority) -> GrantSourceBinding {
        GrantSourceBinding::try_new(
            person,
            ConnectionId::try_new("mail.connection").unwrap(),
            ConnectorId::try_new("gmail").unwrap(),
            ExecutionOwnerId::try_new("server:mail").unwrap(),
            authority,
        )
        .unwrap()
    }

    fn scope() -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new("mail.communication:mail.connection").unwrap()],
            vec![GrantDataCategory::Content],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("assistant").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    async fn vault() -> (tempfile::TempDir, EncryptedAgentVault<TestKeys>, PersonId) {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let person = PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        (root, vault, person)
    }

    #[tokio::test]
    async fn reviewed_mapping_binds_exact_source_and_rejects_selection_tampering() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let grant = vault
            .review_and_activate_remote_view_grant(
                "mail.communication",
                GrantId::new(),
                None,
                source(person, authority),
                scope(),
                None,
            )
            .await
            .unwrap();
        let binding = vault
            .remote_view_grant_binding("mail.communication", "gmail", "mail.connection", authority)
            .await
            .unwrap();
        assert_eq!(binding.grant.id(), grant.id());
        assert_eq!(binding.grant.source().source_authority(), authority);
        assert!(matches!(
            vault
                .remote_view_grant_binding(
                    "mail.communication",
                    "gmail",
                    "other.connection",
                    authority,
                )
                .await,
            Err(AgentFailure::AccessReviewRequired)
        ));
        assert!(matches!(
            vault
                .remote_view_grant_binding(
                    "mail.communication",
                    "gmail",
                    "mail.connection",
                    SourceAuthority::new(),
                )
                .await,
            Err(AgentFailure::AccessReviewRequired | AgentFailure::Conflict)
        ));
    }

    #[tokio::test]
    async fn paused_or_revoked_review_mapping_cannot_be_read() {
        let (_root, vault, person) = vault().await;
        let authority = SourceAuthority::new();
        let grant = vault
            .review_and_activate_remote_view_grant(
                "work.context",
                GrantId::new(),
                None,
                GrantSourceBinding::try_new(
                    person,
                    ConnectionId::try_new("work.connection").unwrap(),
                    ConnectorId::try_new("linear").unwrap(),
                    ExecutionOwnerId::try_new("server:work").unwrap(),
                    authority,
                )
                .unwrap(),
                GrantScope::try_new(
                    vec![ResourceHandle::try_new("work.context:work.connection").unwrap()],
                    vec![GrantDataCategory::Derived],
                    vec![GrantOperation::Read],
                    vec![GrantPurpose::Assistant],
                    vec![GrantConsumer::builtin("assistant").unwrap()],
                    ProcessingRestriction::LocalOnly,
                )
                .unwrap(),
                None,
            )
            .await
            .unwrap();
        let _ = vault
            .pause_data_access_grant(grant.id(), grant.authority())
            .await
            .unwrap();
        assert!(matches!(
            vault
                .remote_view_grant_binding("work.context", "linear", "work.connection", authority)
                .await,
            Err(AgentFailure::PolicyDenied)
        ));
    }
}
