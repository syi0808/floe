use floe_access::{
    DataAccessGrant, FeasibilityGrantQuery, FeasibilityReviewRecord, GrantAuthority, GrantId,
    GrantScope, GrantSourceBinding, GrantState, SourceAuthority,
};
use turso::transaction::{Transaction, TransactionBehavior};

use super::access_grants::AccessGrantMutation;
use super::*;

const SCHEMA_VERSION: i64 = 1;

fn storage(_: impl std::fmt::Debug) -> AgentFailure {
    AgentFailure::StorageUnavailable
}

fn decode_authority(incarnation: String, epoch: i64) -> Result<SourceAuthority, AgentFailure> {
    let incarnation = Uuid::parse_str(&incarnation).map_err(|_| AgentFailure::VaultUnavailable)?;
    let epoch = std::num::NonZeroU64::new(
        u64::try_from(epoch).map_err(|_| AgentFailure::VaultUnavailable)?,
    )
    .ok_or(AgentFailure::VaultUnavailable)?;
    SourceAuthority::from_parts(incarnation, epoch).ok_or(AgentFailure::VaultUnavailable)
}

fn validate_subject(subject: &str) -> Result<(), AgentFailure> {
    if subject.len() == 64
        && subject
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(AgentFailure::InvalidInput)
    }
}

fn decode_review(row: &turso::Row, offset: usize) -> Result<FeasibilityReviewRecord, AgentFailure> {
    let reviewed_subject = row.get::<String>(offset).map_err(storage)?;
    validate_subject(&reviewed_subject).map_err(|_| AgentFailure::VaultUnavailable)?;
    let source_authority = decode_authority(
        row.get::<String>(offset + 1).map_err(storage)?,
        row.get::<i64>(offset + 2).map_err(storage)?,
    )?;
    let query = FeasibilityGrantQuery {
        event_handle: row.get::<String>(offset + 3).map_err(storage)?,
        evidence_handles: serde_json::from_str(&row.get::<String>(offset + 4).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
        destination_latitude: row.get::<f64>(offset + 5).map_err(storage)?,
        destination_longitude: row.get::<f64>(offset + 6).map_err(storage)?,
        event_start_unix_ms: row.get::<i64>(offset + 7).map_err(storage)?,
        event_end_unix_ms: row.get::<i64>(offset + 8).map_err(storage)?,
        travel_mode: row.get::<String>(offset + 9).map_err(storage)?,
    };
    query
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    Ok(FeasibilityReviewRecord {
        query,
        reviewed_subject,
        source_authority,
    })
}

const REVIEW_COLUMNS: &str = "reviewed_subject_fingerprint, source_incarnation, source_epoch, event_handle, evidence_handles, destination_latitude, destination_longitude, event_start_unix_ms, event_end_unix_ms, travel_mode";

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn feasibility_review(
        &self,
        grant_id: GrantId,
    ) -> Result<FeasibilityReviewRecord, AgentFailure> {
        let connection = self.connection()?;
        let query = format!(
            "SELECT {REVIEW_COLUMNS} FROM personal_feasibility_reviews WHERE grant_id = ? AND person_id = ?"
        );
        let mut rows = connection
            .query(
                &query,
                (grant_id.as_uuid().to_string(), self.person_id.to_string()),
            )
            .await
            .map_err(storage)?;
        let row = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::AccessReviewRequired)?;
        let review = decode_review(&row, 0)?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(review)
    }

    async fn source_review_in_transaction(
        &self,
        transaction: &Transaction<'_>,
        source: &GrantSourceBinding,
    ) -> Result<Option<(GrantId, FeasibilityReviewRecord)>, AgentFailure> {
        let query = format!(
            "SELECT grant_id, {REVIEW_COLUMNS} FROM personal_feasibility_reviews WHERE person_id = ? AND connector = ? AND connection_id = ? AND execution_owner = ?"
        );
        let mut rows = transaction
            .query(
                &query,
                (
                    self.person_id.to_string(),
                    source.connector().as_str().to_owned(),
                    source.connection_id().as_str().to_owned(),
                    source.execution_owner().as_str().to_owned(),
                ),
            )
            .await
            .map_err(storage)?;
        let Some(row) = rows.next().await.map_err(storage)? else {
            return Ok(None);
        };
        let grant_id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
            .ok()
            .and_then(GrantId::from_uuid)
            .ok_or(AgentFailure::VaultUnavailable)?;
        let review = decode_review(&row, 1)?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        Ok(Some((grant_id, review)))
    }

    pub async fn review_feasibility_grant(
        &self,
        source: GrantSourceBinding,
        scope: GrantScope,
        reviewed_subject: &str,
        expected: Option<(GrantId, GrantAuthority)>,
        query: FeasibilityGrantQuery,
    ) -> Result<DataAccessGrant, AgentFailure> {
        if source.person_id() != self.person_id
            || source.connector().as_str() != floe_access::FEASIBILITY_CONNECTOR
        {
            return Err(AgentFailure::InvalidInput);
        }
        validate_subject(reviewed_subject)?;
        query.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let previous = self.source_review_in_transaction(&transaction, &source).await?;
            let existing = if let Some((id, _)) = &previous {
                Some(self.read_data_access_grant_in_transaction(&transaction, *id).await?)
            } else {
                self.find_data_access_grant_by_source_in_transaction(&transaction, &source).await?
            };
            match (existing.as_ref(), expected) {
                (Some(grant), Some((id, authority)))
                    if grant.id() == id && grant.authority() == authority && grant.source() == &source => {}
                (None, None) => {}
                (Some(grant), None) if grant.state() == GrantState::Revoked => {}
                _ => return Err(AgentFailure::Conflict),
            }
            let query_changed = previous.as_ref().is_some_and(|(_, review)| review.query != query);
            let grant = match existing {
                Some(grant) if grant.state() != GrantState::Revoked => {
                    let mutation = if query_changed
                        && grant.state() == GrantState::Active
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
                _ => {
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
            let authority = match previous.as_ref() {
                Some((_, review)) if review.reviewed_subject == reviewed_subject => review.source_authority,
                Some((_, review)) => review.source_authority.advance().ok_or(AgentFailure::Conflict)?,
                None => SourceAuthority::new(),
            };
            transaction
                .execute(
                    "INSERT INTO personal_feasibility_reviews (person_id, connector, connection_id, execution_owner, grant_id, reviewed_subject_fingerprint, source_incarnation, source_epoch, event_handle, evidence_handles, destination_latitude, destination_longitude, event_start_unix_ms, event_end_unix_ms, travel_mode) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(person_id, connector, connection_id, execution_owner) DO UPDATE SET grant_id = excluded.grant_id, reviewed_subject_fingerprint = excluded.reviewed_subject_fingerprint, source_incarnation = excluded.source_incarnation, source_epoch = excluded.source_epoch, event_handle = excluded.event_handle, evidence_handles = excluded.evidence_handles, destination_latitude = excluded.destination_latitude, destination_longitude = excluded.destination_longitude, event_start_unix_ms = excluded.event_start_unix_ms, event_end_unix_ms = excluded.event_end_unix_ms, travel_mode = excluded.travel_mode",
                    (
                        self.person_id.to_string(),
                        source.connector().as_str().to_owned(),
                        source.connection_id().as_str().to_owned(),
                        source.execution_owner().as_str().to_owned(),
                        grant.id().as_uuid().to_string(),
                        reviewed_subject.to_owned(),
                        authority.incarnation().to_string(),
                        i64::try_from(authority.epoch().get()).map_err(|_| AgentFailure::Conflict)?,
                        query.event_handle,
                        serde_json::to_string(&query.evidence_handles).map_err(|_| AgentFailure::InvalidInput)?,
                        query.destination_latitude,
                        query.destination_longitude,
                        query.event_start_unix_ms,
                        query.event_end_unix_ms,
                        query.travel_mode,
                    ),
                )
                .await
                .map_err(storage)?;
            Ok(grant)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub async fn pause_feasibility_grant(
        &self,
        grant_id: GrantId,
        expected: GrantAuthority,
    ) -> Result<DataAccessGrant, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let mut rows = transaction
                .query(
                    "SELECT 1 FROM personal_feasibility_reviews WHERE grant_id = ? AND person_id = ?",
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

    pub(super) async fn initialize_feasibility_review_store(
        &self,
        fresh: bool,
    ) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('personal_grant_schema', 'personal_grant_policies', 'personal_feasibility_queries', 'personal_feasibility_review_schema', 'personal_feasibility_reviews')",
                (),
            )
            .await
            .map_err(storage)?;
        let mut names = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            names.push(row.get::<String>(0).map_err(storage)?);
        }
        if names.iter().any(|name| {
            matches!(
                name.as_str(),
                "personal_grant_schema"
                    | "personal_grant_policies"
                    | "personal_feasibility_queries"
            )
        }) {
            return Err(AgentFailure::UnsupportedVersion);
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
                .execute("CREATE TABLE personal_feasibility_review_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))", ())
                .await
                .map_err(storage)?;
            transaction
                .execute("CREATE TABLE personal_feasibility_reviews (person_id TEXT NOT NULL, connector TEXT NOT NULL, connection_id TEXT NOT NULL, execution_owner TEXT NOT NULL, grant_id TEXT NOT NULL, reviewed_subject_fingerprint TEXT NOT NULL, source_incarnation TEXT NOT NULL, source_epoch INTEGER NOT NULL, event_handle TEXT NOT NULL, evidence_handles TEXT NOT NULL, destination_latitude REAL NOT NULL, destination_longitude REAL NOT NULL, event_start_unix_ms INTEGER NOT NULL, event_end_unix_ms INTEGER NOT NULL, travel_mode TEXT NOT NULL, PRIMARY KEY(person_id, connector, connection_id, execution_owner), UNIQUE(grant_id, person_id))", ())
                .await
                .map_err(storage)?;
            transaction
                .execute(
                    "INSERT INTO personal_feasibility_review_schema (id, version) VALUES (1, 1)",
                    (),
                )
                .await
                .map_err(storage)?;
            transaction.commit().await.map_err(storage)?;
            return Ok(());
        }
        if names.len() != 2
            || !names
                .iter()
                .any(|name| name == "personal_feasibility_review_schema")
            || !names
                .iter()
                .any(|name| name == "personal_feasibility_reviews")
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut marker = connection
            .query(
                "SELECT version FROM personal_feasibility_review_schema WHERE id = 1",
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
        if version != SCHEMA_VERSION {
            return Err(AgentFailure::UnsupportedVersion);
        }
        if marker.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        let mut rows = connection
            .query(&format!("SELECT person_id, connector, connection_id, execution_owner, grant_id, {REVIEW_COLUMNS} FROM personal_feasibility_reviews"), ())
            .await
            .map_err(storage)?;
        let mut count = 0usize;
        while let Some(row) = rows.next().await.map_err(storage)? {
            count += 1;
            if count > 128 {
                return Err(AgentFailure::BudgetExceeded);
            }
            if row.get::<String>(0).map_err(storage)? != self.person_id.to_string()
                || row.get::<String>(1).map_err(storage)? != floe_access::FEASIBILITY_CONNECTOR
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            let connection_id =
                floe_access::ConnectionId::try_new(row.get::<String>(2).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
            let connector =
                floe_access::ConnectorId::try_new(row.get::<String>(1).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
            let owner =
                floe_access::ExecutionOwnerId::try_new(row.get::<String>(3).map_err(storage)?)
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
            let source =
                GrantSourceBinding::try_new(self.person_id, connection_id, connector, owner)
                    .map_err(|_| AgentFailure::VaultUnavailable)?;
            let grant_id = Uuid::parse_str(&row.get::<String>(4).map_err(storage)?)
                .ok()
                .and_then(GrantId::from_uuid)
                .ok_or(AgentFailure::VaultUnavailable)?;
            decode_review(&row, 5)?;
            let grant = self.get_data_access_grant(grant_id).await?;
            if grant.source() != &source {
                return Err(AgentFailure::VaultUnavailable);
            }
        }
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

    use floe_access::{
        GrantConsumer, GrantDataCategory, GrantOperation, GrantPurpose, ProcessingRestriction,
        ResourceHandle,
    };

    use super::*;

    #[derive(Clone, Default)]
    struct TestKeys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl VaultKeyProvider for TestKeys {
        fn load(&self, person_id: PersonId, vault_id: Uuid) -> Result<VaultKey, AgentFailure> {
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
            person_id: PersonId,
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

    fn query() -> FeasibilityGrantQuery {
        FeasibilityGrantQuery {
            event_handle: "event:one".into(),
            evidence_handles: vec!["calendar:one".into()],
            destination_latitude: 37.5,
            destination_longitude: 127.0,
            event_start_unix_ms: 1_000,
            event_end_unix_ms: 2_000,
            travel_mode: "transit".into(),
        }
    }

    fn scope() -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new(floe_access::FEASIBILITY_RESOURCE).unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![GrantConsumer::builtin("assistant").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn contextual_review_separates_subject_and_query_authorities() {
        let root = root();
        let person_id = PersonId::new();
        let vault = EncryptedAgentVault::create(root.path(), person_id, TestKeys::default())
            .await
            .unwrap();
        let source = floe_access::feasibility_source(person_id, "device").unwrap();
        let first = vault
            .review_feasibility_grant(source.clone(), scope(), &"a".repeat(64), None, query())
            .await
            .unwrap();
        let first_review = vault.feasibility_review(first.id()).await.unwrap();
        let unchanged = vault
            .review_feasibility_grant(
                source.clone(),
                scope(),
                &"a".repeat(64),
                Some((first.id(), first.authority())),
                query(),
            )
            .await
            .unwrap();
        assert_eq!(unchanged.authority(), first.authority());
        assert_eq!(
            vault.feasibility_review(first.id()).await.unwrap(),
            first_review
        );
        let mut changed_query = query();
        changed_query.destination_latitude = 38.0;
        let query_review = vault
            .review_feasibility_grant(
                source.clone(),
                scope(),
                &"a".repeat(64),
                Some((first.id(), unchanged.authority())),
                changed_query.clone(),
            )
            .await
            .unwrap();
        assert_ne!(query_review.authority(), unchanged.authority());
        let after_query = vault.feasibility_review(first.id()).await.unwrap();
        assert_eq!(after_query.source_authority, first_review.source_authority);
        assert_eq!(after_query.query, changed_query);
        let subject_review = vault
            .review_feasibility_grant(
                source,
                scope(),
                &"b".repeat(64),
                Some((first.id(), query_review.authority())),
                changed_query,
            )
            .await
            .unwrap();
        assert_eq!(subject_review.authority(), query_review.authority());
        assert_eq!(
            vault
                .feasibility_review(first.id())
                .await
                .unwrap()
                .source_authority,
            first_review.source_authority.advance().unwrap()
        );
        let revoked = vault
            .revoke_data_access_grant(subject_review.id(), subject_review.authority())
            .await
            .unwrap();
        assert_eq!(revoked.state(), GrantState::Revoked);
        let replacement = vault
            .review_feasibility_grant(
                floe_access::feasibility_source(person_id, "device").unwrap(),
                scope(),
                &"b".repeat(64),
                None,
                query(),
            )
            .await
            .unwrap();
        assert_ne!(replacement.id(), revoked.id());
        assert_eq!(
            vault
                .feasibility_review(replacement.id())
                .await
                .unwrap()
                .source_authority,
            first_review.source_authority.advance().unwrap()
        );
    }

    #[tokio::test]
    async fn obsolete_personal_schema_fails_closed_on_reopen() {
        let root = root();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        vault
            .connection()
            .unwrap()
            .execute("CREATE TABLE personal_grant_schema (id INTEGER)", ())
            .await
            .unwrap();
        drop(vault);
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person_id, keys).await,
            Err(AgentFailure::UnsupportedVersion)
        ));
    }

    #[tokio::test]
    async fn corrupt_contextual_review_fails_closed_on_reopen() {
        let root = root();
        let person_id = PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person_id, keys.clone())
            .await
            .unwrap();
        vault
            .review_feasibility_grant(
                floe_access::feasibility_source(person_id, "device").unwrap(),
                scope(),
                &"a".repeat(64),
                None,
                query(),
            )
            .await
            .unwrap();
        vault
            .connection()
            .unwrap()
            .execute(
                "UPDATE personal_feasibility_reviews SET source_epoch = 0",
                (),
            )
            .await
            .unwrap();
        drop(vault);
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person_id, keys).await,
            Err(AgentFailure::VaultUnavailable)
        ));
    }
}
