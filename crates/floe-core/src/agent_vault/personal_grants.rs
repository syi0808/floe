use chrono::Utc;
use floe_domain::{
    ConsumerPolicyAuthority, DataAccessGrant, GrantConsumer, GrantId, SourceAuthority,
};
use serde::{Deserialize, Serialize};
use turso::transaction::TransactionBehavior;

use super::access_grants::AccessGrantMutation;
use super::*;

const SCHEMA_VERSION: i64 = 4;
const ATTENTION_CONNECTOR: &str = "attention.macos";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FeasibilityGrantQuery {
    pub event_handle: String,
    pub evidence_handles: Vec<String>,
    pub destination_latitude: f64,
    pub destination_longitude: f64,
    pub event_start_unix_ms: i64,
    pub event_end_unix_ms: i64,
    pub travel_mode: String,
}

impl FeasibilityGrantQuery {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.event_handle.is_empty()
            || self.event_handle.len() > 128
            || self.event_handle.chars().any(char::is_whitespace)
            || self.evidence_handles.is_empty()
            || self.evidence_handles.len() > 8
            || self
                .evidence_handles
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || self.evidence_handles.iter().any(|handle| {
                handle.is_empty() || handle.len() > 128 || handle.chars().any(char::is_whitespace)
            })
            || !self.destination_latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.destination_latitude)
            || !self.destination_longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.destination_longitude)
            || self.event_start_unix_ms < 0
            || self.event_end_unix_ms <= self.event_start_unix_ms
            || self.event_end_unix_ms - self.event_start_unix_ms > 86_400_000
            || !matches!(
                self.travel_mode.as_str(),
                "automobile" | "transit" | "walking"
            )
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

async fn transaction_start<'a>(
    connection: &'a mut turso::Connection,
) -> Result<turso::transaction::Transaction<'a>, AgentFailure> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(storage)
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
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantDataCategory, GrantOperation,
        GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle,
        SourceAuthority,
    };
    use uuid::Uuid;

    use super::*;

    #[derive(Clone, Default)]
    struct TestKeys(Arc<Mutex<HashMap<(floe_domain::PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for TestKeys {
        fn load(
            &self,
            person_id: floe_domain::PersonId,
            vault_id: Uuid,
        ) -> Result<VaultKey, AgentFailure> {
            self.0
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

    fn source(
        person: floe_domain::PersonId,
        device: &str,
        authority: SourceAuthority,
    ) -> GrantSourceBinding {
        GrantSourceBinding::try_new(
            person,
            ConnectionId::try_new("attention.macos.local").unwrap(),
            ConnectorId::try_new("attention.macos").unwrap(),
            ExecutionOwnerId::try_new(format!("macos:{device}")).unwrap(),
            authority,
        )
        .unwrap()
    }

    fn scope(consumers: &[&str]) -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new("attention.coarse").unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            consumers
                .iter()
                .map(|consumer| GrantConsumer::builtin(*consumer).unwrap())
                .collect(),
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    async fn vault() -> (
        tempfile::TempDir,
        TestKeys,
        EncryptedAgentVault<TestKeys>,
        floe_domain::PersonId,
    ) {
        let root = root();
        let person = floe_domain::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        (root, keys, vault, person)
    }

    #[tokio::test]
    async fn personal_review_creates_active_grant_and_reopens() {
        let (root, keys, vault, person) = vault().await;
        let source = source(person, "device-a", SourceAuthority::new());
        let reviewed = vault
            .review_personal_grant(source.clone(), scope(&["assistant"]), &"a".repeat(64), None)
            .await
            .unwrap();
        assert_eq!(reviewed.state(), floe_domain::GrantState::Active);
        assert_eq!(reviewed.source(), &source);
        assert_eq!(
            vault
                .personal_grant_consumer_policy(reviewed.id())
                .await
                .unwrap()
                .epoch()
                .get(),
            1
        );
        vault.checkpoint().await.unwrap();
        drop(vault);
        let reopened = EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .unwrap();
        assert_eq!(
            reopened
                .get_data_access_grant(reviewed.id())
                .await
                .unwrap()
                .state(),
            floe_domain::GrantState::Active
        );
        assert_eq!(
            reopened
                .personal_grant_subject_fingerprint(reviewed.id())
                .await
                .unwrap(),
            "a".repeat(64)
        );
    }

    #[tokio::test]
    async fn personal_review_rejects_malformed_subject_before_transaction() {
        let (_root, _keys, vault, person) = vault().await;
        for fingerprint in ["z".repeat(64), "A".repeat(64)] {
            let failure = vault
                .review_personal_grant(
                    source(person, "device-a", SourceAuthority::new()),
                    scope(&["assistant"]),
                    &fingerprint,
                    None,
                )
                .await
                .unwrap_err();
            assert_eq!(failure, AgentFailure::InvalidInput);
        }
        assert!(vault.list_data_access_grants(128).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn personal_review_noop_pause_and_selected_consumers_are_cas_checked() {
        let (_root, _keys, vault, person) = vault().await;
        let source = source(person, "device-a", SourceAuthority::new());
        let first = vault
            .review_personal_grant(source.clone(), scope(&["assistant"]), &"b".repeat(64), None)
            .await
            .unwrap();
        let policy = vault
            .personal_grant_consumer_policy(first.id())
            .await
            .unwrap();
        let stable = vault
            .review_personal_grant(
                source.clone(),
                scope(&["assistant"]),
                &"b".repeat(64),
                Some((first.id(), first.authority())),
            )
            .await
            .unwrap();
        assert_eq!(stable.authority(), first.authority());
        assert_eq!(
            vault
                .personal_grant_consumer_policy(first.id())
                .await
                .unwrap(),
            policy
        );
        let changed_subject = vault
            .review_personal_grant(
                source.clone(),
                scope(&["assistant"]),
                &"9".repeat(64),
                Some((stable.id(), stable.authority())),
            )
            .await
            .unwrap();
        assert!(changed_subject.authority().access_epoch() > stable.authority().access_epoch());
        assert_eq!(
            vault
                .personal_grant_consumer_policy(first.id())
                .await
                .unwrap(),
            policy
        );
        let paused = vault
            .pause_personal_grant(changed_subject.id(), changed_subject.authority())
            .await
            .unwrap();
        assert_eq!(paused.state(), floe_domain::GrantState::Paused);
        assert_eq!(
            vault
                .pause_personal_grant(first.id(), first.authority())
                .await
                .unwrap_err(),
            AgentFailure::Conflict
        );
        let changed = vault
            .review_personal_grant(
                source,
                scope(&["attention.expert"]),
                &"b".repeat(64),
                Some((paused.id(), paused.authority())),
            )
            .await
            .unwrap();
        assert!(changed.authority().access_epoch() > paused.authority().access_epoch());
        assert_ne!(
            vault
                .personal_grant_consumer_policy(first.id())
                .await
                .unwrap(),
            policy
        );
    }

    #[tokio::test]
    async fn personal_review_rejects_stale_cas_and_preserves_source_replacement() {
        let (_root, _keys, vault, person) = vault().await;
        let first_source = source(person, "device-a", SourceAuthority::new());
        let first = vault
            .review_personal_grant(
                first_source.clone(),
                scope(&["assistant"]),
                &"c".repeat(64),
                None,
            )
            .await
            .unwrap();
        let paused_first = vault
            .pause_personal_grant(first.id(), first.authority())
            .await
            .unwrap();
        assert_eq!(
            vault
                .review_personal_grant(
                    first_source.clone(),
                    scope(&["assistant"]),
                    &"c".repeat(64),
                    Some((first.id(), first.authority())),
                )
                .await
                .unwrap_err(),
            AgentFailure::Conflict
        );
        assert_eq!(
            vault
                .pause_personal_grant(first.id(), first.authority())
                .await
                .unwrap_err(),
            AgentFailure::Conflict
        );
        let replacement = source(person, "device-b", SourceAuthority::new());
        let second = vault
            .review_personal_grant(
                replacement.clone(),
                scope(&["assistant"]),
                &"d".repeat(64),
                None,
            )
            .await
            .unwrap();
        assert_ne!(first.id(), second.id());
        assert_eq!(vault.list_data_access_grants(128).await.unwrap().len(), 2);
        assert_eq!(paused_first.state(), floe_domain::GrantState::Paused);
    }

    #[tokio::test]
    async fn personal_review_rolls_back_grant_mutation_on_mapping_fault() {
        let (_root, _keys, vault, person) = vault().await;
        let source = source(person, "device-a", SourceAuthority::new());
        let first = vault
            .review_personal_grant(source.clone(), scope(&["assistant"]), &"e".repeat(64), None)
            .await
            .unwrap();
        let connection = vault.database.connect().unwrap();
        connection
            .execute(
                "UPDATE personal_grant_policies SET policy_epoch = 0 WHERE grant_id = ?",
                [first.id().as_uuid().to_string()],
            )
            .await
            .unwrap();
        let failure = vault
            .review_personal_grant(
                source,
                scope(&["attention.expert"]),
                &"f".repeat(64),
                Some((first.id(), first.authority())),
            )
            .await
            .unwrap_err();
        assert_eq!(failure, AgentFailure::VaultUnavailable);
        let mut rows = connection
            .query(
                "SELECT access_epoch FROM data_access_grants WHERE grant_id = ?",
                [first.id().as_uuid().to_string()],
            )
            .await
            .unwrap();
        assert_eq!(
            rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn personal_review_reopen_rejects_corrupt_mapping() {
        let (root, keys, vault, person) = vault().await;
        let source = source(person, "device-a", SourceAuthority::new());
        let grant = vault
            .review_personal_grant(source, scope(&["assistant"]), &"1".repeat(64), None)
            .await
            .unwrap();
        let connection = vault.database.connect().unwrap();
        connection
            .execute(
                "UPDATE personal_grant_policies SET person_id = ? WHERE grant_id = ?",
                (
                    floe_domain::PersonId::new().to_string(),
                    grant.id().as_uuid().to_string(),
                ),
            )
            .await
            .unwrap();
        vault.checkpoint().await.unwrap();
        drop(vault);
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person, keys).await,
            Err(AgentFailure::VaultUnavailable)
        ));
    }
}

fn validate_subject_fingerprint(fingerprint: &str) -> Result<(), AgentFailure> {
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Err(AgentFailure::InvalidInput)
    } else {
        Ok(())
    }
}

fn storage(_: impl std::fmt::Debug) -> AgentFailure {
    AgentFailure::StorageUnavailable
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn validate_current_authority_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        dependency: &floe_domain::ContextDependency,
    ) -> Result<(), AgentFailure> {
        let connector = dependency.source().connector().as_str();
        if connector == ATTENTION_CONNECTOR {
            self.validate_attention_dependency_in_transaction(transaction, dependency)
                .await?;
        } else if connector.starts_with("calendar.") {
            self.validate_access_grant_dependency_in_transaction(transaction, dependency)
                .await?;
            self.validate_calendar_dependency_policy_in_transaction(transaction, dependency)
                .await?;
        } else {
            self.validate_access_grant_dependency_in_transaction(transaction, dependency)
                .await?;
            self.validate_remote_view_dependency_policy_in_transaction(transaction, dependency)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn validate_context_dependency_coverage_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        coverage: &floe_domain::DependencyCoverage,
    ) -> Result<(), AgentFailure> {
        let floe_domain::DependencyCoverage::Dependent { dependencies } = coverage else {
            return Ok(());
        };
        for dependency in dependencies {
            if dependency.person_id() != self.person_id {
                return Err(AgentFailure::PolicyDenied);
            }
            if dependency.expires_at() <= Utc::now() {
                return Err(AgentFailure::PolicyDenied);
            }
            self.validate_current_authority_in_transaction(transaction, dependency)
                .await?;
        }
        Ok(())
    }

    async fn validate_calendar_dependency_policy_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        dependency: &floe_domain::ContextDependency,
    ) -> Result<(), AgentFailure> {
        let grant_id = dependency.grant_id().as_uuid().to_string();
        let person_id = self.person_id.to_string();
        let mut rows = transaction.query(
            "SELECT policy_incarnation, policy_epoch FROM calendar_grant_mappings WHERE grant_id = ? AND person_id = ? UNION ALL SELECT policy_incarnation, policy_epoch FROM remote_calendar_grant_mappings WHERE grant_id = ? AND person_id = ?",
            (grant_id.clone(), person_id.clone(), grant_id, person_id),
        ).await.map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::PolicyDenied)?;
        if rows.next().await.map_err(storage)?.is_some()
            || row.get::<String>(0).map_err(storage)?
                != dependency.consumer_policy().incarnation().to_string()
            || row.get::<i64>(1).map_err(storage)?
                != i64::try_from(dependency.consumer_policy().epoch().get())
                    .map_err(|_| AgentFailure::PolicyDenied)?
        {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    async fn validate_access_grant_dependency_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        dependency: &floe_domain::ContextDependency,
    ) -> Result<(), AgentFailure> {
        let grant = self
            .read_data_access_grant_in_transaction(transaction, dependency.grant_id())
            .await
            .map_err(|error| match error {
                AgentFailure::NotFound => AgentFailure::PolicyDenied,
                other => other,
            })?;
        floe_access::validate_grant_dependency(&grant, dependency)
    }

    async fn validate_attention_dependency_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        dependency: &floe_domain::ContextDependency,
    ) -> Result<(), AgentFailure> {
        self.validate_access_grant_dependency_in_transaction(transaction, dependency)
            .await?;
        let mut policy_rows = transaction
                .query(
                    "SELECT policy_incarnation, policy_epoch FROM personal_grant_policies WHERE grant_id = ? AND person_id = ?",
                    (dependency.grant_id().as_uuid().to_string(), self.person_id.to_string()),
                )
                .await
                .map_err(storage)?;
        let row = policy_rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::PolicyDenied)?;
        let incarnation = uuid::Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let epoch = std::num::NonZeroU64::new(
            u64::try_from(row.get::<i64>(1).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?,
        )
        .ok_or(AgentFailure::VaultUnavailable)?;
        let policy = ConsumerPolicyAuthority::from_parts(incarnation, epoch)
            .ok_or(AgentFailure::VaultUnavailable)?;
        if dependency.consumer_policy() != policy {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(())
    }

    pub async fn pause_personal_grant(
        &self,
        grant_id: GrantId,
        expected: floe_domain::GrantAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = transaction_start(&mut connection).await?;
        let result = async {
            let mut rows = transaction
                .query(
                    "SELECT 1 FROM personal_grant_policies WHERE grant_id = ? AND person_id = ?",
                    (grant_id.as_uuid().to_string(), self.person_id.to_string()),
                )
                .await
                .map_err(storage)?;
            if rows.next().await.map_err(storage)?.is_none() {
                return Err(AgentFailure::NotFound);
            }
            self.mutate_data_access_grant_in_transaction(
                &transaction,
                grant_id,
                expected,
                AccessGrantMutation::Pause,
            )
            .await
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub async fn review_personal_grant(
        &self,
        source: floe_domain::GrantSourceBinding,
        scope: floe_domain::GrantScope,
        reviewed_subject_fingerprint: &str,
        expected: Option<(GrantId, floe_domain::GrantAuthority)>,
    ) -> Result<DataAccessGrant, AgentFailure> {
        self.review_personal_grant_with_selection(
            source,
            scope,
            reviewed_subject_fingerprint,
            expected,
            &[],
        )
        .await
    }

    pub async fn review_personal_grant_with_selection(
        &self,
        source: floe_domain::GrantSourceBinding,
        scope: floe_domain::GrantScope,
        reviewed_subject_fingerprint: &str,
        expected: Option<(GrantId, floe_domain::GrantAuthority)>,
        selected_handles: &[String],
    ) -> Result<DataAccessGrant, AgentFailure> {
        self.review_personal_grant_with_selection_and_query(
            source,
            scope,
            reviewed_subject_fingerprint,
            expected,
            selected_handles,
            None,
        )
        .await
    }

    pub async fn review_personal_grant_with_feasibility_query(
        &self,
        source: floe_domain::GrantSourceBinding,
        scope: floe_domain::GrantScope,
        reviewed_subject_fingerprint: &str,
        expected: Option<(GrantId, floe_domain::GrantAuthority)>,
        query: FeasibilityGrantQuery,
    ) -> Result<DataAccessGrant, AgentFailure> {
        query.validate()?;
        self.review_personal_grant_with_selection_and_query(
            source,
            scope,
            reviewed_subject_fingerprint,
            expected,
            &[],
            Some(query),
        )
        .await
    }

    async fn review_personal_grant_with_selection_and_query(
        &self,
        source: floe_domain::GrantSourceBinding,
        scope: floe_domain::GrantScope,
        reviewed_subject_fingerprint: &str,
        expected: Option<(GrantId, floe_domain::GrantAuthority)>,
        selected_handles: &[String],
        feasibility_query: Option<FeasibilityGrantQuery>,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if source.person_id() != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        if selected_handles.len() > 64
            || selected_handles.windows(2).any(|pair| pair[0] >= pair[1])
            || selected_handles.iter().any(|handle| {
                handle.is_empty() || handle.len() > 512 || handle.chars().any(char::is_whitespace)
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
        let consumers = scope.consumers().to_vec();
        validate_subject_fingerprint(reviewed_subject_fingerprint)?;
        let mut connection = self.connection()?;
        let transaction = transaction_start(&mut connection).await?;
        let result = async {
            let existing = if let Some(grant_id) = self
                .personal_grant_mapping_in_transaction(&transaction, &source)
                .await?
            {
                Some(
                    self.read_data_access_grant_in_transaction(&transaction, grant_id)
                        .await?,
                )
            } else {
                self.find_data_access_grant_by_source_in_transaction(&transaction, &source)
                    .await?
            };
            match (existing.as_ref(), expected) {
                (Some(grant), Some((expected_id, expected_authority)))
                    if grant.id() == expected_id && grant.authority() == expected_authority => {}
                (None, None) => {}
                (Some(_), _) | (None, Some(_)) => return Err(AgentFailure::Conflict),
            }
            let grant = match existing {
                Some(grant) if grant.state() == floe_domain::GrantState::Revoked => {
                    let created = self
                        .create_data_access_grant_in_transaction(
                            &transaction,
                            GrantId::new(),
                            source.clone(),
                            scope.clone(),
                        )
                        .await?;
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        created.id(),
                        created.authority(),
                        AccessGrantMutation::Activate { source, scope },
                    )
                    .await?
                }
                Some(grant) => {
                    let same_review = self
                        .personal_grant_subject_in_transaction(&transaction, grant.id())
                        .await?
                        .is_some_and(|fingerprint| fingerprint == reviewed_subject_fingerprint);
                    let source = if grant.state() == floe_domain::GrantState::Active
                        && !same_review
                        && grant.source().source_authority() == source.source_authority()
                    {
                        floe_domain::GrantSourceBinding::try_new(
                            source.person_id(),
                            source.connection_id(),
                            source.connector().clone(),
                            source.execution_owner().clone(),
                            SourceAuthority::new(),
                        )
                        .map_err(|_| AgentFailure::InvalidInput)?
                    } else {
                        source.clone()
                    };
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        grant.id(),
                        grant.authority(),
                        AccessGrantMutation::Activate { source, scope },
                    )
                    .await?
                }
                None => {
                    let created = self
                        .create_data_access_grant_in_transaction(
                            &transaction,
                            GrantId::new(),
                            source.clone(),
                            scope.clone(),
                        )
                        .await?;
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        created.id(),
                        created.authority(),
                        AccessGrantMutation::Activate { source, scope },
                    )
                    .await?
                }
            };
            self.upsert_personal_policy_in_transaction(
                &transaction,
                &grant,
                &consumers,
                reviewed_subject_fingerprint,
                selected_handles,
            )
            .await?;
            if let Some(query) = feasibility_query.as_ref() {
                self.upsert_feasibility_query_in_transaction(&transaction, &grant, query)
                    .await?;
            }
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    async fn personal_grant_subject_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        grant_id: GrantId,
    ) -> Result<Option<String>, AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT reviewed_subject_fingerprint FROM personal_grant_policies WHERE grant_id = ? AND person_id = ?",
                (grant_id.as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        rows.next()
            .await
            .map_err(storage)?
            .map(|row| row.get::<String>(0).map_err(storage))
            .transpose()
    }

    pub async fn personal_grant_selected_handles(
        &self,
        grant_id: GrantId,
    ) -> Result<Vec<String>, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT selected_handles FROM personal_grant_policies WHERE grant_id = ? AND person_id = ?",
                (grant_id.as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        let values = serde_json::from_str(&row.get::<String>(0).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        Ok(values)
    }

    pub async fn personal_feasibility_query(
        &self,
        grant_id: GrantId,
    ) -> Result<FeasibilityGrantQuery, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT event_handle, evidence_handles, destination_latitude, destination_longitude, event_start_unix_ms, event_end_unix_ms, travel_mode FROM personal_feasibility_queries WHERE grant_id = ? AND person_id = ?",
                (grant_id.as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::NotFound)?;
        let query = FeasibilityGrantQuery {
            event_handle: row.get::<String>(0).map_err(storage)?,
            evidence_handles: serde_json::from_str(&row.get::<String>(1).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?,
            destination_latitude: row.get::<f64>(2).map_err(storage)?,
            destination_longitude: row.get::<f64>(3).map_err(storage)?,
            event_start_unix_ms: row.get::<i64>(4).map_err(storage)?,
            event_end_unix_ms: row.get::<i64>(5).map_err(storage)?,
            travel_mode: row.get::<String>(6).map_err(storage)?,
        };
        query.validate()?;
        Ok(query)
    }

    async fn upsert_feasibility_query_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        grant: &DataAccessGrant,
        query: &FeasibilityGrantQuery,
    ) -> Result<(), AgentFailure> {
        query.validate()?;
        let evidence_handles = serde_json::to_string(&query.evidence_handles)
            .map_err(|_| AgentFailure::InvalidInput)?;
        transaction
            .execute(
                "DELETE FROM personal_feasibility_queries WHERE grant_id = ? AND person_id = ?",
                (grant.id().as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO personal_feasibility_queries (grant_id, person_id, event_handle, evidence_handles, destination_latitude, destination_longitude, event_start_unix_ms, event_end_unix_ms, travel_mode) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (grant.id().as_uuid().to_string(), self.person_id.to_string(), query.event_handle.clone(), evidence_handles, query.destination_latitude, query.destination_longitude, query.event_start_unix_ms, query.event_end_unix_ms, query.travel_mode.clone()),
            )
            .await
            .map_err(storage)?;
        Ok(())
    }

    async fn personal_grant_mapping_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        source: &floe_domain::GrantSourceBinding,
    ) -> Result<Option<GrantId>, AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT grant_id FROM personal_grant_policies WHERE person_id = ? AND connector = ? AND connection_id = ? AND execution_owner = ?",
                (
                    self.person_id.to_string(),
                    source.connector().as_str().to_owned(),
                    source.connection_id().as_str().to_owned(),
                    source.execution_owner().as_str().to_owned(),
                ),
            )
            .await
            .map_err(storage)?;
        let first = rows.next().await.map_err(storage)?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        first
            .map(|row| {
                uuid::Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                    .ok()
                    .and_then(GrantId::from_uuid)
                    .ok_or(AgentFailure::VaultUnavailable)
            })
            .transpose()
    }

    async fn upsert_personal_policy_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        grant: &DataAccessGrant,
        consumers: &[GrantConsumer],
        fingerprint: &str,
        selected_handles: &[String],
    ) -> Result<ConsumerPolicyAuthority, AgentFailure> {
        let encoded = serde_json::to_string(consumers).map_err(|_| AgentFailure::InvalidInput)?;
        let encoded_handles =
            serde_json::to_string(selected_handles).map_err(|_| AgentFailure::InvalidInput)?;
        let mut rows = transaction
            .query(
                "SELECT grant_id, policy_incarnation, policy_epoch, consumers, reviewed_subject_fingerprint, selected_handles FROM personal_grant_policies WHERE person_id = ? AND connector = ? AND connection_id = ? AND execution_owner = ?",
                (
                    self.person_id.to_string(),
                    grant.source().connector().as_str().to_owned(),
                    grant.source().connection_id().as_str().to_owned(),
                    grant.source().execution_owner().as_str().to_owned(),
                ),
            )
            .await
            .map_err(storage)?;
        if let Some(row) = rows.next().await.map_err(storage)? {
            let old_grant_id = uuid::Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                .ok()
                .and_then(GrantId::from_uuid)
                .ok_or(AgentFailure::VaultUnavailable)?;
            let incarnation = uuid::Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            let epoch = std::num::NonZeroU64::new(
                u64::try_from(row.get::<i64>(2).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?,
            )
            .ok_or(AgentFailure::VaultUnavailable)?;
            let current = ConsumerPolicyAuthority::from_parts(incarnation, epoch)
                .ok_or(AgentFailure::VaultUnavailable)?;
            if row.get::<String>(3).map_err(storage)? == encoded {
                if old_grant_id != grant.id()
                    || row.get::<String>(4).map_err(storage)? != fingerprint
                    || row.get::<String>(5).map_err(storage)? != encoded_handles
                {
                    transaction
                        .execute(
                            "UPDATE personal_grant_policies SET grant_id = ?, reviewed_subject_fingerprint = ?, selected_handles = ? WHERE person_id = ? AND connector = ? AND connection_id = ? AND execution_owner = ?",
                            (
                                grant.id().as_uuid().to_string(),
                                fingerprint.to_owned(),
                                encoded_handles.clone(),
                                self.person_id.to_string(),
                                grant.source().connector().as_str().to_owned(),
                                grant.source().connection_id().as_str().to_owned(),
                                grant.source().execution_owner().as_str().to_owned(),
                            ),
                        )
                        .await
                        .map_err(storage)?;
                }
                return Ok(current);
            }
            let next = current.advance().ok_or(AgentFailure::Conflict)?;
            transaction
                .execute(
                    "UPDATE personal_grant_policies SET grant_id = ?, policy_incarnation = ?, policy_epoch = ?, consumers = ?, reviewed_subject_fingerprint = ?, selected_handles = ? WHERE person_id = ? AND connector = ? AND connection_id = ? AND execution_owner = ?",
                    (grant.id().as_uuid().to_string(), next.incarnation().to_string(), i64::try_from(next.epoch().get()).map_err(|_| AgentFailure::Conflict)?, encoded, fingerprint.to_owned(), encoded_handles, self.person_id.to_string(), grant.source().connector().as_str().to_owned(), grant.source().connection_id().as_str().to_owned(), grant.source().execution_owner().as_str().to_owned()),
                )
                .await
                .map_err(storage)?;
            return Ok(next);
        }
        let policy = ConsumerPolicyAuthority::new();
        transaction
            .execute(
                "INSERT INTO personal_grant_policies (grant_id, person_id, connector, connection_id, execution_owner, reviewed_subject_fingerprint, policy_incarnation, policy_epoch, consumers, selected_handles) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (grant.id().as_uuid().to_string(), self.person_id.to_string(), grant.source().connector().as_str().to_owned(), grant.source().connection_id().as_str().to_owned(), grant.source().execution_owner().as_str().to_owned(), fingerprint.to_owned(), policy.incarnation().to_string(), i64::try_from(policy.epoch().get()).map_err(|_| AgentFailure::Conflict)?, encoded, encoded_handles),
            )
            .await
            .map_err(storage)?;
        Ok(policy)
    }

    pub(super) async fn initialize_personal_grant_store(
        &self,
        fresh: bool,
    ) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('personal_grant_schema', 'personal_grant_policies', 'personal_feasibility_queries')",
                (),
            )
            .await
            .map_err(storage)?;
        let mut names = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            names.push(row.get::<String>(0).map_err(storage)?);
        }
        if names.is_empty() {
            if !fresh {
                return Err(AgentFailure::VaultUnavailable);
            }
            let mut connection = self.connection()?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "CREATE TABLE personal_grant_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 4))",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "CREATE TABLE personal_grant_policies (grant_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, connector TEXT NOT NULL, connection_id TEXT NOT NULL, execution_owner TEXT NOT NULL, reviewed_subject_fingerprint TEXT NOT NULL, policy_incarnation TEXT NOT NULL, policy_epoch INTEGER NOT NULL, consumers TEXT NOT NULL, selected_handles TEXT NOT NULL DEFAULT '[]', UNIQUE(person_id, connector, connection_id, execution_owner))",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "CREATE TABLE personal_feasibility_queries (grant_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, event_handle TEXT NOT NULL, evidence_handles TEXT NOT NULL, destination_latitude REAL NOT NULL, destination_longitude REAL NOT NULL, event_start_unix_ms INTEGER NOT NULL, event_end_unix_ms INTEGER NOT NULL, travel_mode TEXT NOT NULL, UNIQUE(grant_id, person_id))",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "INSERT INTO personal_grant_schema (id, version) VALUES (1, 4)",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction.commit().await.map_err(storage)?;
            return Ok(());
        }
        if names.len() != 3
            || !names.iter().any(|name| name == "personal_grant_schema")
            || !names.iter().any(|name| name == "personal_grant_policies")
            || !names
                .iter()
                .any(|name| name == "personal_feasibility_queries")
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut marker = connection
            .query("SELECT version FROM personal_grant_schema WHERE id = 1", ())
            .await
            .map_err(storage)?;
        let version = marker
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<i64>(0)
            .map_err(storage)?;
        if version != SCHEMA_VERSION {
            return Err(AgentFailure::VaultUnavailable);
        }
        if marker.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        self.validate_personal_policy_rows(&connection).await
    }

    async fn validate_personal_policy_rows(
        &self,
        connection: &turso::Connection,
    ) -> Result<(), AgentFailure> {
        let mut rows = connection
            .query(
                "SELECT grant_id, person_id, connector, connection_id, execution_owner, reviewed_subject_fingerprint, policy_incarnation, policy_epoch, consumers, selected_handles FROM personal_grant_policies ORDER BY grant_id",
                (),
            )
            .await
            .map_err(storage)?;
        let mut count = 0usize;
        while let Some(row) = rows.next().await.map_err(storage)? {
            count += 1;
            if count > 128 {
                return Err(AgentFailure::BudgetExceeded);
            }
            let grant_id = uuid::Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
                .ok()
                .and_then(GrantId::from_uuid)
                .ok_or(AgentFailure::VaultUnavailable)?;
            if row.get::<String>(1).map_err(storage)? != self.person_id.to_string() {
                return Err(AgentFailure::VaultUnavailable);
            }
            floe_domain::ConnectorId::try_new(row.get::<String>(2).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            floe_domain::ConnectionId::try_new(row.get::<String>(3).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            floe_domain::ExecutionOwnerId::try_new(row.get::<String>(4).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            validate_subject_fingerprint(&row.get::<String>(5).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            let incarnation = uuid::Uuid::parse_str(&row.get::<String>(6).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            let epoch = std::num::NonZeroU64::new(
                u64::try_from(row.get::<i64>(7).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?,
            )
            .ok_or(AgentFailure::VaultUnavailable)?;
            ConsumerPolicyAuthority::from_parts(incarnation, epoch)
                .ok_or(AgentFailure::VaultUnavailable)?;
            let consumers: Vec<GrantConsumer> =
                serde_json::from_str(&row.get::<String>(8).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
            if consumers.is_empty() || consumers.len() > 32 {
                return Err(AgentFailure::VaultUnavailable);
            }
            for consumer in &consumers {
                let valid = match consumer {
                    GrantConsumer::Builtin(value) => GrantConsumer::builtin(value.clone()),
                    GrantConsumer::Extension(value) => GrantConsumer::extension(value.clone()),
                };
                valid.map_err(|_| AgentFailure::VaultUnavailable)?;
            }
            let mut canonical = consumers.clone();
            canonical.sort();
            canonical.dedup();
            if canonical != consumers {
                return Err(AgentFailure::VaultUnavailable);
            }
            let selected_handles: Vec<String> =
                serde_json::from_str(&row.get::<String>(9).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
            if selected_handles.len() > 64
                || selected_handles.iter().any(|handle| {
                    handle.is_empty()
                        || handle.len() > 512
                        || handle.chars().any(char::is_whitespace)
                })
                || selected_handles.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            let mut grant_rows = connection
                .query(
                    "SELECT person_id, connection_id, connector, execution_owner FROM data_access_grants WHERE grant_id = ? AND authority_owner = ?",
                    (grant_id.as_uuid().to_string(), self.vault_id.to_string()),
                )
                .await
                .map_err(storage)?;
            let grant_row = grant_rows
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?;
            if grant_rows.next().await.map_err(storage)?.is_some()
                || grant_row.get::<String>(0).map_err(storage)? != self.person_id.to_string()
                || grant_row.get::<String>(1).map_err(storage)?
                    != row.get::<String>(3).map_err(storage)?
                || grant_row.get::<String>(2).map_err(storage)?
                    != row.get::<String>(2).map_err(storage)?
                || grant_row.get::<String>(3).map_err(storage)?
                    != row.get::<String>(4).map_err(storage)?
            {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
        Ok(())
    }

    pub async fn personal_grant_consumer_policy(
        &self,
        grant_id: GrantId,
    ) -> Result<ConsumerPolicyAuthority, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT policy_incarnation, policy_epoch FROM personal_grant_policies WHERE grant_id = ? AND person_id = ?",
                (grant_id.as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let incarnation = uuid::Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let epoch = std::num::NonZeroU64::new(
            u64::try_from(row.get::<i64>(1).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?,
        )
        .ok_or(AgentFailure::VaultUnavailable)?;
        ConsumerPolicyAuthority::from_parts(incarnation, epoch)
            .ok_or(AgentFailure::VaultUnavailable)
    }

    pub async fn personal_grant_subject_fingerprint(
        &self,
        grant_id: GrantId,
    ) -> Result<String, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT reviewed_subject_fingerprint FROM personal_grant_policies WHERE grant_id = ? AND person_id = ?",
                (grant_id.as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        rows.next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?
            .get::<String>(0)
            .map_err(storage)
    }
}
