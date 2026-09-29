use floe_access::DataAccessGrant;

/// What a feasibility grant admits reading is part of the grant, so Access owns
/// it; this module stores and reads the record.
pub use floe_access::FeasibilityGrantQuery;
use floe_access::{GrantId, SourceAuthority};
use turso::transaction::TransactionBehavior;

use super::access_grants::AccessGrantMutation;
use super::*;

const SCHEMA_VERSION: i64 = 6;

fn decode_source_authority(
    incarnation: String,
    epoch: i64,
) -> Result<SourceAuthority, AgentFailure> {
    let incarnation = uuid::Uuid::parse_str(&incarnation)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let epoch = std::num::NonZeroU64::new(
        u64::try_from(epoch).map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    SourceAuthority::from_parts(incarnation, epoch).ok_or(AgentFailure::VaultUnavailable)
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

    use floe_access::{
        ConnectionId, ConnectorId, ExecutionOwnerId, GrantConsumer, GrantDataCategory, GrantOperation,
        GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle,
    };
    use uuid::Uuid;

    use super::*;

    #[derive(Clone, Default)]
    struct TestKeys(Arc<Mutex<HashMap<(floe_kernel::PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for TestKeys {
        fn load(
            &self,
            person_id: floe_kernel::PersonId,
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
            person_id: floe_kernel::PersonId,
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
        person: floe_kernel::PersonId,
        device: &str,
    ) -> GrantSourceBinding {
        GrantSourceBinding::try_new(
            person,
            ConnectionId::try_new("attention.macos.local").unwrap(),
            ConnectorId::try_new("attention.macos").unwrap(),
            ExecutionOwnerId::try_new(format!("macos:{device}")).unwrap(),
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
        floe_kernel::PersonId,
    ) {
        let root = root();
        let person = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        (root, keys, vault, person)
    }

    #[tokio::test]
    async fn personal_review_creates_active_grant_and_reopens() {
        let (root, keys, vault, person) = vault().await;
        let source = source(person, "device-a");
        let reviewed = vault
            .review_personal_grant(source.clone(), scope(&["assistant"]), &"a".repeat(64), None)
            .await
            .unwrap();
        assert_eq!(reviewed.state(), floe_access::GrantState::Active);
        assert_eq!(reviewed.source(), &source);
        assert_eq!(vault.personal_grant_source_authority(reviewed.id()).await.unwrap().epoch().get(), 1);
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
            floe_access::GrantState::Active
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
                    source(person, "device-a"),
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
        let source = source(person, "device-a");
        let first = vault
            .review_personal_grant(source.clone(), scope(&["assistant"]), &"b".repeat(64), None)
            .await
            .unwrap();
        let first_source_authority = vault.personal_grant_source_authority(first.id()).await.unwrap();
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
            vault.personal_grant_source_authority(first.id()).await.unwrap(),
            first_source_authority
        );
        assert_eq!(stable.scope(), first.scope());
        let changed_subject = vault
            .review_personal_grant(
                source.clone(),
                scope(&["assistant"]),
                &"9".repeat(64),
                Some((stable.id(), stable.authority())),
            )
            .await
            .unwrap();
        assert_eq!(changed_subject.authority(), stable.authority());
        let changed_source_authority = vault.personal_grant_source_authority(first.id()).await.unwrap();
        assert_eq!(
            changed_source_authority,
            first_source_authority.advance().unwrap()
        );
        assert_eq!(changed_subject.scope(), stable.scope());
        let paused = vault
            .pause_personal_grant(changed_subject.id(), changed_subject.authority())
            .await
            .unwrap();
        assert_eq!(paused.state(), floe_access::GrantState::Paused);
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
                &"9".repeat(64),
                Some((paused.id(), paused.authority())),
            )
            .await
            .unwrap();
        assert!(changed.authority().access_epoch() > paused.authority().access_epoch());
        assert_eq!(vault.personal_grant_source_authority(first.id()).await.unwrap(), changed_source_authority);
    }

    #[tokio::test]
    async fn contacts_selection_changes_only_the_personal_source_epoch() {
        let (_root, _keys, vault, person) = vault().await;
        let source = floe_access::contacts_source(person, "device-a", "contacts.apple").unwrap();
        let scope = |consumers: &[&str]| {
            GrantScope::try_new(
                vec![ResourceHandle::try_new(floe_access::PEOPLE_RESOURCE).unwrap()],
                vec![GrantDataCategory::Metadata],
                vec![GrantOperation::Read],
                vec![GrantPurpose::Assistant],
                consumers
                    .iter()
                    .map(|consumer| GrantConsumer::builtin(*consumer).unwrap())
                    .collect(),
                ProcessingRestriction::LocalOnly,
            )
            .unwrap()
        };
        let selected_a = vec!["contact-a".to_owned()];
        let first = vault
            .review_personal_grant_with_selection(
                source.clone(),
                scope(&["assistant"]),
                &"a".repeat(64),
                None,
                &selected_a,
            )
            .await
            .unwrap();
        let first_source = vault.personal_grant_source_authority(first.id()).await.unwrap();
        let selected_b = vec!["contact-b".to_owned()];
        let changed = vault
            .review_personal_grant_with_selection(
                source.clone(),
                scope(&["assistant"]),
                &"a".repeat(64),
                Some((first.id(), first.authority())),
                &selected_b,
            )
            .await
            .unwrap();
        assert_eq!(changed.id(), first.id());
        assert_eq!(changed.authority(), first.authority());
        assert_eq!(changed.source(), &source);
        let second_source = vault.personal_grant_source_authority(first.id()).await.unwrap();
        assert_eq!(second_source, first_source.advance().unwrap());
        let consumer_changed = vault
            .review_personal_grant_with_selection(
                source,
                scope(&["assistant", "calendar.expert"]),
                &"a".repeat(64),
                Some((changed.id(), changed.authority())),
                &selected_b,
            )
            .await
            .unwrap();
        assert_ne!(consumer_changed.authority(), changed.authority());
        assert_eq!(vault.personal_grant_source_authority(first.id()).await.unwrap(), second_source);
        assert_ne!(consumer_changed.scope().consumers(), changed.scope().consumers());
    }

    #[tokio::test]
    async fn feasibility_query_change_advances_only_grant_authority() {
        let (_root, _keys, vault, person) = vault().await;
        let source = floe_access::feasibility_source(person, "device-a").unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new(floe_access::FEASIBILITY_RESOURCE).unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("assistant").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let query_a = FeasibilityGrantQuery {
            event_handle: "event-a".into(),
            evidence_handles: vec!["evidence-a".into()],
            destination_latitude: 37.0,
            destination_longitude: 127.0,
            event_start_unix_ms: 1_800_000_000_000,
            event_end_unix_ms: 1_800_000_060_000,
            travel_mode: "walking".into(),
        };
        let first = vault
            .review_personal_grant_with_feasibility_query(
                source.clone(),
                scope.clone(),
                &"a".repeat(64),
                None,
                query_a.clone(),
            )
            .await
            .unwrap();
        let source_authority = vault.personal_grant_source_authority(first.id()).await.unwrap();
        let observed_at = chrono::Utc::now();
        let old_dependency = floe_access::ContextDependency::try_new(
            person,
            first.id(),
            first.authority(),
            first.source().clone(),
            first.scope().resources().to_vec(),
            source_authority,
            first.scope().resources().to_vec(),
            first.scope().categories().to_vec(),
            GrantOperation::Read,
            GrantPurpose::Assistant,
            GrantConsumer::builtin("assistant").unwrap(),
            ProcessingRestriction::LocalOnly,
            Uuid::new_v4(),
            vec![1],
            Uuid::new_v4(),
            Uuid::new_v4(),
            observed_at,
            observed_at + chrono::Duration::minutes(59),
        )
        .unwrap();
        let mut query_b = query_a.clone();
        query_b.event_handle = "event-b".into();
        let changed = vault
            .review_personal_grant_with_feasibility_query(
                source.clone(),
                scope.clone(),
                &"a".repeat(64),
                Some((first.id(), first.authority())),
                query_b.clone(),
            )
            .await
            .unwrap();
        assert_eq!(changed.authority(), first.authority().advance().unwrap());
        assert_eq!(
            floe_access::validate_grant_dependency(&changed, &old_dependency),
            Err(AgentFailure::PolicyDenied)
        );
        assert_eq!(
            vault.personal_grant_source_authority(first.id()).await.unwrap(),
            source_authority
        );
        assert_eq!(
            vault.personal_feasibility_query(first.id()).await.unwrap(),
            query_b
        );
        assert_eq!(
            vault
                .review_personal_grant_with_feasibility_query(
                    source.clone(),
                    scope.clone(),
                    &"a".repeat(64),
                    Some((first.id(), first.authority())),
                    query_a,
                )
                .await
                .unwrap_err(),
            AgentFailure::Conflict
        );
        let stable = vault
            .review_personal_grant_with_feasibility_query(
                source,
                scope,
                &"a".repeat(64),
                Some((changed.id(), changed.authority())),
                query_b,
            )
            .await
            .unwrap();
        assert_eq!(stable.authority(), changed.authority());
        assert_eq!(
            vault.personal_grant_source_authority(first.id()).await.unwrap(),
            source_authority
        );
    }

    #[tokio::test]
    async fn personal_review_rejects_stale_cas_and_preserves_source_replacement() {
        let (_root, _keys, vault, person) = vault().await;
        let first_source = source(person, "device-a");
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
        let replacement = source(person, "device-b");
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
        assert_eq!(paused_first.state(), floe_access::GrantState::Paused);
    }

    #[tokio::test]
    async fn personal_review_rolls_back_grant_mutation_on_mapping_fault() {
        let (_root, _keys, vault, person) = vault().await;
        let source = source(person, "device-a");
        let first = vault
            .review_personal_grant(source.clone(), scope(&["assistant"]), &"e".repeat(64), None)
            .await
            .unwrap();
        let connection = vault.database.connect().unwrap();
        connection
            .execute(
                "UPDATE personal_grant_policies SET source_epoch = 0 WHERE grant_id = ?",
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
        let source = source(person, "device-a");
        let grant = vault
            .review_personal_grant(source, scope(&["assistant"]), &"1".repeat(64), None)
            .await
            .unwrap();
        let connection = vault.database.connect().unwrap();
        connection
            .execute(
                "UPDATE personal_grant_policies SET person_id = ? WHERE grant_id = ?",
                (
                    floe_kernel::PersonId::new().to_string(),
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

    #[tokio::test]
    async fn obsolete_personal_schema_is_not_migrated_on_reopen() {
        let (root, keys, vault, person) = vault().await;
        let connection = vault.database.connect().unwrap();
        connection.execute("DROP TABLE personal_grant_schema", ()).await.unwrap();
        connection
            .execute(
                "CREATE TABLE personal_grant_schema (id INTEGER PRIMARY KEY, version INTEGER NOT NULL)",
                (),
            )
            .await
            .unwrap();
        connection
            .execute("INSERT INTO personal_grant_schema VALUES (1, 5)", ())
            .await
            .unwrap();
        vault.checkpoint().await.unwrap();
        drop(vault);
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person, keys).await,
            Err(AgentFailure::UnsupportedVersion)
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
    pub async fn pause_personal_grant(
        &self,
        grant_id: GrantId,
        expected: floe_access::GrantAuthority,
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
        source: floe_access::GrantSourceBinding,
        scope: floe_access::GrantScope,
        reviewed_subject_fingerprint: &str,
        expected: Option<(GrantId, floe_access::GrantAuthority)>,
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
        source: floe_access::GrantSourceBinding,
        scope: floe_access::GrantScope,
        reviewed_subject_fingerprint: &str,
        expected: Option<(GrantId, floe_access::GrantAuthority)>,
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
        source: floe_access::GrantSourceBinding,
        scope: floe_access::GrantScope,
        reviewed_subject_fingerprint: &str,
        expected: Option<(GrantId, floe_access::GrantAuthority)>,
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
        source: floe_access::GrantSourceBinding,
        scope: floe_access::GrantScope,
        reviewed_subject_fingerprint: &str,
        expected: Option<(GrantId, floe_access::GrantAuthority)>,
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
                    if grant.id() == expected_id
                        && grant.authority() == expected_authority
                        && grant.source() == &source => {}
                (None, None) => {}
                (Some(_), _) | (None, Some(_)) => return Err(AgentFailure::Conflict),
            }
            let query_changed = if let (Some(grant), Some(query)) =
                (existing.as_ref(), feasibility_query.as_ref())
            {
                self.feasibility_query_in_transaction(&transaction, grant.id())
                    .await?
                    .as_ref()
                    != Some(query)
            } else {
                false
            };
            let grant = match existing {
                Some(grant) if grant.state() == floe_access::GrantState::Revoked => {
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
                        AccessGrantMutation::Activate { scope },
                    )
                    .await?
                }
                Some(grant) => {
                    let mutation = if query_changed
                        && grant.state() == floe_access::GrantState::Active
                        && grant.scope() == &scope
                    {
                        AccessGrantMutation::ReviewActive { scope }
                    } else {
                        AccessGrantMutation::Activate { scope }
                    };
                    self.mutate_data_access_grant_in_transaction(
                        &transaction,
                        grant.id(),
                        grant.authority(),
                        mutation,
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
                        AccessGrantMutation::Activate { scope },
                    )
                    .await?
                }
            };
            self.upsert_personal_source_review_in_transaction(
                &transaction,
                &grant,
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

    async fn feasibility_query_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        grant_id: GrantId,
    ) -> Result<Option<FeasibilityGrantQuery>, AgentFailure> {
        let mut rows = transaction
            .query(
                "SELECT event_handle, evidence_handles, destination_latitude, destination_longitude, event_start_unix_ms, event_end_unix_ms, travel_mode FROM personal_feasibility_queries WHERE grant_id = ? AND person_id = ?",
                (grant_id.as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let Some(row) = rows.next().await.map_err(storage)? else {
            return Ok(None);
        };
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
        query.validate().map_err(|_| AgentFailure::VaultUnavailable)?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(Some(query))
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
        source: &floe_access::GrantSourceBinding,
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

    async fn upsert_personal_source_review_in_transaction(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        grant: &DataAccessGrant,
        fingerprint: &str,
        selected_handles: &[String],
    ) -> Result<(), AgentFailure> {
        let encoded_handles =
            serde_json::to_string(selected_handles).map_err(|_| AgentFailure::InvalidInput)?;
        let mut rows = transaction
            .query(
                "SELECT grant_id, reviewed_subject_fingerprint, selected_handles, source_incarnation, source_epoch FROM personal_grant_policies WHERE person_id = ? AND connector = ? AND connection_id = ? AND execution_owner = ?",
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
            let current_source = decode_source_authority(
                row.get::<String>(3).map_err(storage)?,
                row.get::<i64>(4).map_err(storage)?,
            )?;
            let source_changed = row.get::<String>(1).map_err(storage)? != fingerprint
                || row.get::<String>(2).map_err(storage)? != encoded_handles;
            let next_source = if source_changed {
                current_source.advance().ok_or(AgentFailure::Conflict)?
            } else {
                current_source
            };
            if rows.next().await.map_err(storage)?.is_some() {
                return Err(AgentFailure::VaultUnavailable);
            }
            if old_grant_id != grant.id() || source_changed {
                transaction
                    .execute(
                        "UPDATE personal_grant_policies SET grant_id = ?, reviewed_subject_fingerprint = ?, selected_handles = ?, source_incarnation = ?, source_epoch = ? WHERE person_id = ? AND connector = ? AND connection_id = ? AND execution_owner = ?",
                        (
                            grant.id().as_uuid().to_string(),
                            fingerprint.to_owned(),
                            encoded_handles,
                            next_source.incarnation().to_string(),
                            i64::try_from(next_source.epoch().get()).map_err(|_| AgentFailure::Conflict)?,
                            self.person_id.to_string(),
                            grant.source().connector().as_str().to_owned(),
                            grant.source().connection_id().as_str().to_owned(),
                            grant.source().execution_owner().as_str().to_owned(),
                        ),
                    )
                    .await
                    .map_err(storage)?;
            }
            return Ok(());
        }
        let source_authority = SourceAuthority::new();
        transaction
            .execute(
                "INSERT INTO personal_grant_policies (grant_id, person_id, connector, connection_id, execution_owner, reviewed_subject_fingerprint, selected_handles, source_incarnation, source_epoch) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (grant.id().as_uuid().to_string(), self.person_id.to_string(), grant.source().connector().as_str().to_owned(), grant.source().connection_id().as_str().to_owned(), grant.source().execution_owner().as_str().to_owned(), fingerprint.to_owned(), encoded_handles, source_authority.incarnation().to_string(), i64::try_from(source_authority.epoch().get()).map_err(|_| AgentFailure::Conflict)?),
            )
            .await
            .map_err(storage)?;
        Ok(())
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
                    "CREATE TABLE personal_grant_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 6))",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "CREATE TABLE personal_grant_policies (grant_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, connector TEXT NOT NULL, connection_id TEXT NOT NULL, execution_owner TEXT NOT NULL, reviewed_subject_fingerprint TEXT NOT NULL, selected_handles TEXT NOT NULL, source_incarnation TEXT NOT NULL, source_epoch INTEGER NOT NULL, UNIQUE(person_id, connector, connection_id, execution_owner))",
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
                    "INSERT INTO personal_grant_schema (id, version) VALUES (1, 6)",
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
            return Err(AgentFailure::UnsupportedVersion);
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
                "SELECT grant_id, person_id, connector, connection_id, execution_owner, reviewed_subject_fingerprint, selected_handles, source_incarnation, source_epoch FROM personal_grant_policies ORDER BY grant_id",
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
            floe_access::ConnectorId::try_new(row.get::<String>(2).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            floe_access::ConnectionId::try_new(row.get::<String>(3).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            floe_access::ExecutionOwnerId::try_new(row.get::<String>(4).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            validate_subject_fingerprint(&row.get::<String>(5).map_err(storage)?)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            let selected_handles: Vec<String> =
                serde_json::from_str(&row.get::<String>(6).map_err(storage)?)
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
            decode_source_authority(
                row.get::<String>(7).map_err(storage)?,
                row.get::<i64>(8).map_err(storage)?,
            )?;
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

    /// Bounded current personal-source owner until checkpoint 06 moves standing
    /// Contacts, Attention, and Wellbeing source authority to Connections.
    /// This epoch is not part of DataAccessGrant identity.
    pub async fn personal_grant_source_authority(
        &self,
        grant_id: GrantId,
    ) -> Result<SourceAuthority, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT source_incarnation, source_epoch FROM personal_grant_policies WHERE grant_id = ? AND person_id = ?",
                (grant_id.as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let authority = decode_source_authority(
            row.get::<String>(0).map_err(storage)?,
            row.get::<i64>(1).map_err(storage)?,
        )?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(authority)
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
