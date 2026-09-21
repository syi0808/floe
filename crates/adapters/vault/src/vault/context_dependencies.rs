use floe_access::{ContextDependencyError, DependencyCoverage, MAX_CONTEXT_DEPENDENCY_BYTES};
use floe_agent_contract::AgentFailure;
use floe_kernel::PersonId;
use turso::transaction::Transaction;
use uuid::Uuid;

const CONTEXT_DEPENDENCY_SCHEMA_VERSION: i64 = 1;
const SCHEMA_TABLE: &str = "agent_context_dependency_schema";
const COVERAGE_TABLE: &str = "agent_context_dependency_coverage";
const MAX_CONTEXT_DEPENDENCY_ROWS: i64 = 100_000;
pub(super) const MAX_CONTEXT_DEPENDENCY_TURNS_PER_SESSION: i64 = 4_096;
pub(super) const MAX_CONTEXT_DEPENDENCY_SESSION_BYTES: i64 = 4 * 1024 * 1024;

pub(super) async fn initialize_context_dependency_store(
    transaction: &Transaction<'_>,
) -> Result<(), AgentFailure> {
    let mut tables = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name IN (?, ?)",
            (SCHEMA_TABLE, COVERAGE_TABLE),
        )
        .await
        .map_err(storage)?;
    let mut found = Vec::new();
    while let Some(row) = tables.next().await.map_err(storage)? {
        found.push(row.get::<String>(0).map_err(storage)?);
    }
    found.sort();
    if found.is_empty() {
        transaction
            .execute(
                "CREATE TABLE agent_context_dependency_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_context_dependency_coverage (person_id TEXT NOT NULL, session_id TEXT NOT NULL, turn_id TEXT NOT NULL, version INTEGER NOT NULL CHECK (version = 1), payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 65536), PRIMARY KEY (person_id, session_id, turn_id))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE INDEX agent_context_dependency_coverage_session_idx ON agent_context_dependency_coverage (person_id, session_id, turn_id)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_context_dependency_schema (id, version) VALUES (1, 1)",
                (),
            )
            .await
            .map_err(storage)?;
        return Ok(());
    }
    if found != [COVERAGE_TABLE.to_owned(), SCHEMA_TABLE.to_owned()] {
        return Err(AgentFailure::VaultUnavailable);
    }
    validate_context_dependency_store(transaction).await
}

pub(super) async fn validate_context_dependency_store(
    transaction: &Transaction<'_>,
) -> Result<(), AgentFailure> {
    let mut marker = transaction
        .query(
            "SELECT id, version FROM agent_context_dependency_schema",
            (),
        )
        .await
        .map_err(storage)?;
    let Some(row) = marker.next().await.map_err(storage)? else {
        return Err(AgentFailure::VaultUnavailable);
    };
    if row.get::<i64>(0).map_err(storage)? != 1
        || row.get::<i64>(1).map_err(storage)? != CONTEXT_DEPENDENCY_SCHEMA_VERSION
        || marker.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut indexes = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'index' AND name = 'agent_context_dependency_coverage_session_idx'",
            (),
        )
        .await
        .map_err(storage)?;
    if indexes.next().await.map_err(storage)?.is_none() {
        return Err(AgentFailure::VaultUnavailable);
    }
    transaction
        .query(
            "SELECT person_id, session_id, turn_id, version, payload FROM agent_context_dependency_coverage LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    let mut rows = transaction
        .query(
            "SELECT person_id, session_id, turn_id, version, payload FROM agent_context_dependency_coverage LIMIT ?",
            [MAX_CONTEXT_DEPENDENCY_ROWS + 1],
        )
        .await
        .map_err(storage)?;
    let mut count = 0i64;
    while let Some(row) = rows.next().await.map_err(storage)? {
        count += 1;
        if count > MAX_CONTEXT_DEPENDENCY_ROWS
            || row.get::<i64>(3).map_err(storage)? != CONTEXT_DEPENDENCY_SCHEMA_VERSION
        {
            return Err(AgentFailure::VaultUnavailable);
        }
        let person = parse_uuid(row.get::<String>(0).map_err(storage)?)?;
        let session = parse_uuid(row.get::<String>(1).map_err(storage)?)?;
        let turn = parse_uuid(row.get::<String>(2).map_err(storage)?)?;
        if person.is_nil() || session.is_nil() || turn.is_nil() {
            return Err(AgentFailure::VaultUnavailable);
        }
        let payload = row.get::<String>(4).map_err(storage)?;
        if payload.is_empty() || payload.len() > MAX_CONTEXT_DEPENDENCY_BYTES {
            return Err(AgentFailure::VaultUnavailable);
        }
        let coverage = DependencyCoverage::from_persisted_bytes(payload.as_bytes())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        ensure_coverage_person(&coverage, PersonId(person))?;
    }
    Ok(())
}

pub(super) async fn read_context_dependency_coverage(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    session_id: Uuid,
    turn_id: Uuid,
) -> Result<DependencyCoverage, AgentFailure> {
    validate_key(person_id, session_id, turn_id)?;
    let mut rows = transaction
        .query(
            "SELECT version, payload FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (person_id.to_string(), session_id.to_string(), turn_id.to_string()),
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(DependencyCoverage::Unknown);
    };
    if rows.next().await.map_err(storage)?.is_some()
        || row.get::<i64>(0).map_err(storage)? != CONTEXT_DEPENDENCY_SCHEMA_VERSION
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let payload = row.get::<String>(1).map_err(storage)?;
    if payload.is_empty() || payload.len() > MAX_CONTEXT_DEPENDENCY_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    let coverage = DependencyCoverage::from_persisted_bytes(payload.as_bytes())
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    ensure_coverage_person(&coverage, person_id)?;
    Ok(coverage)
}

pub(super) async fn merge_context_dependency_coverage(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    session_id: Uuid,
    turn_id: Uuid,
    incoming: DependencyCoverage,
) -> Result<DependencyCoverage, AgentFailure> {
    validate_key(person_id, session_id, turn_id)?;
    incoming
        .validate()
        .map_err(|_| AgentFailure::InvalidInput)?;
    ensure_coverage_person(&incoming, person_id).map_err(|_| AgentFailure::InvalidInput)?;
    let previous = read_context_dependency_row(transaction, person_id, session_id, turn_id).await?;
    let merged = match previous.as_ref() {
        None => incoming.clone(),
        Some(previous) => previous.merge(&incoming).map_err(|error| match error {
            ContextDependencyError::Conflict => AgentFailure::Conflict,
            _ => AgentFailure::InvalidInput,
        })?,
    };
    let payload = merged
        .as_persisted_bytes()
        .map_err(|_| AgentFailure::BudgetExceeded)?;
    let mut quota = transaction
            .query(
                "SELECT COUNT(*), COALESCE(SUM(length(CAST(payload AS BLOB))), 0) FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ?",
                (person_id.to_string(), session_id.to_string()),
            )
            .await
            .map_err(storage)?;
    let quota = quota
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?;
    let count = quota.get::<i64>(0).map_err(storage)?;
    let current_bytes = quota.get::<i64>(1).map_err(storage)?;
    let old_bytes = if previous.is_some() {
        let mut existing = transaction
            .query(
                "SELECT length(CAST(payload AS BLOB)) FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ? AND turn_id = ?",
                (person_id.to_string(), session_id.to_string(), turn_id.to_string()),
            )
            .await
            .map_err(storage)?;
        existing
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<i64>(0)
            .map_err(storage)?
    } else {
        0
    };
    let next_count = count
        .checked_add(if previous.is_none() { 1 } else { 0 })
        .ok_or(AgentFailure::BudgetExceeded)?;
    let next_bytes = current_bytes
        .checked_sub(old_bytes)
        .and_then(|value| value.checked_add(i64::try_from(payload.len()).ok()?))
        .ok_or(AgentFailure::BudgetExceeded)?;
    if next_count > MAX_CONTEXT_DEPENDENCY_TURNS_PER_SESSION
        || next_bytes > MAX_CONTEXT_DEPENDENCY_SESSION_BYTES
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    let changed = transaction
        .execute(
            "UPDATE agent_context_dependency_coverage SET version = ?, payload = ? WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (CONTEXT_DEPENDENCY_SCHEMA_VERSION, String::from_utf8(payload.clone()).map_err(|_| AgentFailure::InvalidInput)?, person_id.to_string(), session_id.to_string(), turn_id.to_string()),
        )
        .await
        .map_err(storage)?;
    if changed == 0 {
        transaction
            .execute(
                "INSERT INTO agent_context_dependency_coverage (person_id, session_id, turn_id, version, payload) VALUES (?, ?, ?, ?, ?)",
                (person_id.to_string(), session_id.to_string(), turn_id.to_string(), CONTEXT_DEPENDENCY_SCHEMA_VERSION, String::from_utf8(payload).map_err(|_| AgentFailure::InvalidInput)?),
            )
            .await
            .map_err(storage)?;
    }
    Ok(merged)
}

async fn read_context_dependency_row(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    session_id: Uuid,
    turn_id: Uuid,
) -> Result<Option<DependencyCoverage>, AgentFailure> {
    validate_key(person_id, session_id, turn_id)?;
    let mut rows = transaction
        .query(
            "SELECT version, payload FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (person_id.to_string(), session_id.to_string(), turn_id.to_string()),
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    if rows.next().await.map_err(storage)?.is_some()
        || row.get::<i64>(0).map_err(storage)? != CONTEXT_DEPENDENCY_SCHEMA_VERSION
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let payload = row.get::<String>(1).map_err(storage)?;
    if payload.is_empty() || payload.len() > MAX_CONTEXT_DEPENDENCY_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    let coverage = DependencyCoverage::from_persisted_bytes(payload.as_bytes())
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    ensure_coverage_person(&coverage, person_id)?;
    Ok(Some(coverage))
}

fn validate_key(person_id: PersonId, session_id: Uuid, turn_id: Uuid) -> Result<(), AgentFailure> {
    if person_id.0.is_nil() || session_id.is_nil() || turn_id.is_nil() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn parse_uuid(value: String) -> Result<Uuid, AgentFailure> {
    Uuid::parse_str(&value).map_err(|_| AgentFailure::VaultUnavailable)
}

fn ensure_coverage_person(
    coverage: &DependencyCoverage,
    person_id: PersonId,
) -> Result<(), AgentFailure> {
    if let DependencyCoverage::Dependent { dependencies } = coverage
        && dependencies
            .iter()
            .any(|dependency| dependency.person_id() != person_id)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

fn storage(_: impl std::fmt::Debug) -> AgentFailure {
    AgentFailure::StorageUnavailable
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
    };

    use super::*;
    use crate::ContextEvidenceReader;
    use chrono::{Duration, Utc};
    use floe_access::{
        ConnectionId, ConnectorId, ConsumerPolicyAuthority, ContextDependency, DependencyCoverage,
        ExecutionOwnerId, GrantAuthority, GrantConsumer, GrantDataCategory, GrantId,
        GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, MAX_CONTEXT_DEPENDENCIES,
        ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_agent_contract::{DataClass, ModelPlacement, TransferConsent};
    use floe_agent_contract::{ModelReplay, ProviderReplay};
    use floe_context::{AgentContext, InferencePolicyDecision};
    use floe_conversation::{
        AgentBudget, AgentMessage, ModelRequest, SessionStore, prompts::manager_prompt,
    };
    use floe_execution::Cancellation;
    use floe_kernel::AGENT_VERSION;

    #[derive(Clone, Default)]
    struct TestKeys(Arc<Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>>);

    impl super::super::VaultKeyProvider for TestKeys {
        fn load(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
        ) -> Result<super::super::VaultKey, AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .get(&(person_id, vault_id))
                .copied()
                .map(super::super::VaultKey::from_bytes)
                .ok_or(AgentFailure::VaultUnavailable)
        }

        fn insert(
            &self,
            person_id: PersonId,
            vault_id: Uuid,
            key: &super::super::VaultKey,
        ) -> Result<(), AgentFailure> {
            self.0
                .lock()
                .unwrap()
                .insert((person_id, vault_id), *key.as_bytes());
            Ok(())
        }
    }

    type Vault = super::super::EncryptedAgentVault<TestKeys>;

    struct AllowDependency;

    impl floe_access::DependencyResolver for AllowDependency {
        fn authorize<'a>(
            &'a self,
            _: &'a ContextDependency,
            _: &'a floe_access::DependencyAuthorization,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), AgentFailure>> + Send + 'a>,
        > {
            Box::pin(async { Ok(()) })
        }
    }

    fn projection_request(
        person_id: PersonId,
        session_id: Uuid,
        turn_id: Uuid,
        messages: Vec<AgentMessage>,
    ) -> ModelRequest {
        ModelRequest {
            usage: floe_inference::UsageLedger::default(),
            replay: vec![ModelReplay {
                call_id: Uuid::new_v4(),
                replay: ProviderReplay {
                    call_ids: vec!["call".into()],
                    preamble: "".into(),
                    gateway: "test".into(),
                    purpose: "test".into(),
                    external: false,
                    source: "test".into(),
                    provider_call_id: "call".into(),
                    items: serde_json::json!([]),
                },
            }],
            schema_version: AGENT_VERSION,
            prompt: manager_prompt(None).unwrap(),
            person_id,
            session_id,
            turn_id,
            policy: InferencePolicyDecision {
                purpose: "test".into(),
                data_classes: vec![DataClass::Personal],
                allowed_placements: vec![ModelPlacement::DeviceLocal],
                performance_class: "test".into(),
                projection_version: 1,
                external_transfer_consent: TransferConsent::NotGranted,
                bounded_sensitive_projection: false,
            },
            context: AgentContext {
                projection_version: 1,
                persona: None,
                optional_context_issues: vec![],
                memories: vec![],
                evidence: vec![],
            },
            messages,
            capabilities: vec![],
            active_agents: vec![],
            remaining_tokens: 100,
            remaining_cost_micros: 100,
            max_output_bytes: 4096,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            cancellation: Cancellation::default(),
        }
    }

    fn root() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        root
    }

    async fn read(
        vault: &Vault,
        person_id: PersonId,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> Result<DependencyCoverage, AgentFailure> {
        let mut connection = vault.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result =
            read_context_dependency_coverage(&transaction, person_id, session_id, turn_id).await;
        vault
            .finish_access_grant_transaction(transaction, result)
            .await
    }

    async fn merge(
        vault: &Vault,
        person_id: PersonId,
        session_id: Uuid,
        turn_id: Uuid,
        coverage: DependencyCoverage,
    ) -> Result<DependencyCoverage, AgentFailure> {
        let mut connection = vault.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let result = merge_context_dependency_coverage(
            &transaction,
            person_id,
            session_id,
            turn_id,
            coverage,
        )
        .await;
        vault
            .finish_access_grant_transaction(transaction, result)
            .await
    }

    fn dependency(person_id: PersonId, marker: &[u8]) -> ContextDependency {
        let source = GrantSourceBinding::try_new(
            person_id,
            ConnectionId::new(),
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("test-host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let observed_at = Utc::now();
        ContextDependency::try_new(
            person_id,
            GrantId::new(),
            GrantAuthority::new(),
            source,
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Scheduling,
            GrantConsumer::builtin("calendar").unwrap(),
            ProcessingRestriction::LocalOnly,
            ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            marker.to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            observed_at,
            observed_at + Duration::minutes(5),
        )
        .unwrap()
    }

    async fn authorized_dependency(vault: &Vault, marker: &[u8]) -> ContextDependency {
        let source = GrantSourceBinding::try_new(
            vault.person_id,
            ConnectionId::new(),
            ConnectorId::try_new("attention.macos").unwrap(),
            ExecutionOwnerId::try_new("test-host").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap();
        let consumer = GrantConsumer::builtin("assistant").unwrap();
        let scope = GrantScope::try_new(
            vec![ResourceHandle::try_new("attention.coarse").unwrap()],
            vec![GrantDataCategory::Derived],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Assistant],
            vec![consumer.clone()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap();
        let grant = vault
            .review_personal_grant(source, scope, &"a".repeat(64), None)
            .await
            .unwrap();
        let policy = vault
            .personal_grant_consumer_policy(grant.id())
            .await
            .unwrap();
        let observed_at = Utc::now();
        ContextDependency::try_new(
            vault.person_id,
            grant.id(),
            grant.authority(),
            grant.source().clone(),
            grant.scope().resources().to_vec(),
            grant.scope().categories().to_vec(),
            GrantOperation::Read,
            GrantPurpose::Assistant,
            consumer,
            ProcessingRestriction::LocalOnly,
            policy,
            Uuid::new_v4(),
            marker.to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            observed_at,
            observed_at + Duration::minutes(2),
        )
        .unwrap()
    }

    #[test]
    fn missing_projection_is_unknown() {
        assert_eq!(DependencyCoverage::default(), DependencyCoverage::Unknown);
    }

    #[test]
    fn schema_limits_are_bounded() {
        assert_eq!(MAX_CONTEXT_DEPENDENCIES, 64);
        assert_eq!(MAX_CONTEXT_DEPENDENCY_BYTES, 64 * 1024);
        assert_eq!(AgentBudget::default().max_iterations, 100);
    }

    #[tokio::test]
    async fn encrypted_store_reopens_and_unknown_is_durable() {
        let root = root();
        let person = PersonId::new();
        let keys = TestKeys::default();
        let vault = Vault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let turn = Uuid::new_v4();
        assert_eq!(
            read(&vault, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Unknown
        );
        merge(
            &vault,
            person,
            session.id,
            turn,
            DependencyCoverage::Independent,
        )
        .await
        .unwrap();
        assert_eq!(
            read(&vault, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Independent
        );
        merge(
            &vault,
            person,
            session.id,
            turn,
            DependencyCoverage::Unknown,
        )
        .await
        .unwrap();
        merge(
            &vault,
            person,
            session.id,
            turn,
            DependencyCoverage::dependent(dependency(person, b"still-unknown")).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(
            read(&vault, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Unknown
        );
        vault.checkpoint().await.unwrap();
        drop(vault);
        let reopened = Vault::open(root.path(), person, keys).await.unwrap();
        assert_eq!(
            read(&reopened, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Unknown
        );
    }

    #[tokio::test]
    async fn explicitly_independent_failed_result_does_not_poison_turn() {
        let root = root();
        let person = PersonId::new();
        let vault = Vault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let turn = Uuid::new_v4();
        let call = Uuid::new_v4();
        session.messages.extend([
            AgentMessage::User {
                turn_id: turn,
                text: "tool request".into(),
            },
            AgentMessage::Capability {
                turn_id: turn,
                call_id: call,
                capability_id: "people.identity.read".into(),
                input: "{}".into(),
                result: Err(AgentFailure::CapabilityUnavailable),
            },
        ]);
        session.revision = 1;
        let store = vault.governed_general_store(session.id);
        store.record_result_independent(turn, call).unwrap();
        store.compare_and_swap(&session, 0).await.unwrap();
        assert_eq!(
            read(&vault, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Independent
        );
    }

    #[tokio::test]
    async fn sidecar_and_session_roll_back_together_on_sql_failure() {
        let root = root();
        let person = PersonId::new();
        let keys = TestKeys::default();
        let vault = Vault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let mut connection = vault.connection().unwrap();
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .unwrap();
        let result = async {
            transaction
                .execute(
                    "UPDATE agent_sessions SET revision = 1 WHERE id = ? AND revision = 0",
                    [session.id.to_string()],
                )
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            merge_context_dependency_coverage(
                &transaction,
                person,
                session.id,
                Uuid::new_v4(),
                DependencyCoverage::Independent,
            )
            .await?;
            transaction
                .execute(
                    "INSERT INTO missing_context_dependency_table VALUES (1)",
                    (),
                )
                .await
                .map_err(|_| AgentFailure::StorageUnavailable)
        }
        .await;
        assert_eq!(
            vault
                .finish_access_grant_transaction(transaction, result)
                .await,
            Err(AgentFailure::StorageUnavailable)
        );
        drop(vault);
        let reopened = Vault::open(root.path(), person, keys).await.unwrap();
        assert_eq!(reopened.load(person, session.id).await.unwrap().revision, 0);
        let mut rows = reopened
            .connection()
            .unwrap()
            .query("SELECT COUNT(*) FROM agent_context_dependency_coverage", ())
            .await
            .unwrap();
        assert_eq!(
            rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn malformed_sidecar_and_missing_index_fail_reopen_closed() {
        let root = root();
        let person = PersonId::new();
        let keys = TestKeys::default();
        let vault = Vault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        vault
            .connection()
            .unwrap()
            .execute(
                "DROP INDEX agent_context_dependency_coverage_session_idx",
                (),
            )
            .await
            .unwrap();
        drop(vault);
        assert!(matches!(
            Vault::open(root.path(), person, keys).await,
            Err(AgentFailure::VaultUnavailable)
        ));
    }

    #[tokio::test]
    async fn malformed_payload_and_wrong_person_fail_closed() {
        let root = root();
        let person = PersonId::new();
        let keys = TestKeys::default();
        let vault = Vault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let turn = Uuid::new_v4();
        assert_eq!(
            merge(
                &vault,
                person,
                session.id,
                turn,
                DependencyCoverage::dependent(dependency(PersonId::new(), b"wrong-person"))
                    .unwrap(),
            )
            .await,
            Err(AgentFailure::InvalidInput)
        );
        vault
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO agent_context_dependency_coverage VALUES (?, ?, ?, 1, ?)",
                (
                    person.to_string(),
                    session.id.to_string(),
                    turn.to_string(),
                    "{}",
                ),
            )
            .await
            .unwrap();
        drop(vault);
        assert!(matches!(
            Vault::open(root.path(), person, keys).await,
            Err(AgentFailure::VaultUnavailable)
        ));
    }

    #[tokio::test]
    async fn encrypted_files_do_not_expose_dependency_payload() {
        let root = root();
        let person = PersonId::new();
        let vault = Vault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let marker = b"opaque-calendar-marker";
        merge(
            &vault,
            person,
            session.id,
            Uuid::new_v4(),
            DependencyCoverage::dependent(dependency(person, marker)).unwrap(),
        )
        .await
        .unwrap();
        vault.checkpoint().await.unwrap();
        let directory = root.path().join(person.to_string());
        for name in ["sessions.db", "sessions.db-wal", "sessions.db-shm"] {
            let path = directory.join(name);
            if let Ok(bytes) = fs::read(path) {
                assert!(!bytes.windows(marker.len()).any(|window| window == marker));
            }
        }
    }

    #[tokio::test]
    async fn key_loss_before_commit_rolls_back_sidecar() {
        let root = root();
        let person = PersonId::new();
        let keys = TestKeys::default();
        let vault = Vault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let turn = Uuid::new_v4();
        let vault_key = keys
            .0
            .lock()
            .unwrap()
            .get(&(person, vault.vault_id))
            .copied()
            .unwrap();
        let mut connection = vault.connection().unwrap();
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .unwrap();
        merge_context_dependency_coverage(
            &transaction,
            person,
            session.id,
            turn,
            DependencyCoverage::Independent,
        )
        .await
        .unwrap();
        keys.0.lock().unwrap().remove(&(person, vault.vault_id));
        let result = vault.check_access();
        assert_eq!(result, Err(AgentFailure::VaultUnavailable));
        assert_eq!(
            vault
                .finish_access_grant_transaction(transaction, result)
                .await,
            Err(AgentFailure::VaultUnavailable)
        );
        keys.0
            .lock()
            .unwrap()
            .insert((person, vault.vault_id), vault_key);
        drop(vault);
        let reopened = Vault::open(root.path(), person, keys).await.unwrap();
        assert_eq!(
            read(&reopened, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Unknown
        );
    }

    #[tokio::test]
    async fn direct_session_cas_cannot_mint_independent_coverage() {
        let root = root();
        let person = PersonId::new();
        let vault = Vault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let turn = Uuid::new_v4();
        session
            .messages
            .push(floe_conversation::AgentMessage::User {
                turn_id: turn,
                text: "direct CAS".into(),
            });
        session.revision = 1;
        vault.compare_and_swap(&session, 0).await.unwrap();
        assert_eq!(
            read(&vault, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Unknown
        );
    }

    #[tokio::test]
    async fn governed_session_cas_records_new_user_turn_as_independent() {
        let root = root();
        let person = PersonId::new();
        let vault = Vault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let turn = Uuid::new_v4();
        session
            .messages
            .push(floe_conversation::AgentMessage::User {
                turn_id: turn,
                text: "ordinary chat".into(),
            });
        session.revision = 1;
        let governed = vault.governed_general_store(session.id);
        governed.compare_and_swap(&session, 0).await.unwrap();
        assert_eq!(
            read(&vault, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Independent
        );
    }

    #[tokio::test]
    async fn governed_session_never_upgrades_persisted_unknown() {
        let root = root();
        let person = PersonId::new();
        let vault = Vault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let turn = Uuid::new_v4();
        session
            .messages
            .push(floe_conversation::AgentMessage::User {
                turn_id: turn,
                text: "unmanaged chat".into(),
            });
        session.revision = 1;
        vault.compare_and_swap(&session, 0).await.unwrap();
        let governed = vault.governed_general_store(session.id);
        governed
            .record_dependency(turn, dependency(person, b"late dependency"))
            .await
            .unwrap();
        session
            .messages
            .push(floe_conversation::AgentMessage::Assistant {
                turn_id: turn,
                text: "reply".into(),
            });
        session.revision = 2;
        governed.compare_and_swap(&session, 1).await.unwrap();
        assert_eq!(
            read(&vault, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Unknown
        );
    }

    #[tokio::test]
    async fn every_capability_result_requires_explicit_result_dependency() {
        let root = root();
        let person = PersonId::new();
        let vault = Vault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let turn = Uuid::new_v4();
        let call_id = Uuid::new_v4();
        session.messages.extend([
            floe_conversation::AgentMessage::Capability {
                turn_id: turn,
                call_id,
                capability_id: "calendar.read".into(),
                input: "{}".into(),
                result: Ok("{}".into()),
            },
            floe_conversation::AgentMessage::User {
                turn_id: turn,
                text: "tool request".into(),
            },
        ]);
        session.revision = 1;
        let governed = vault.governed_general_store(session.id);
        governed
            .record_dependency(turn, dependency(person, b"turn-only"))
            .await
            .unwrap();
        governed.compare_and_swap(&session, 0).await.unwrap();
        assert_eq!(
            read(&vault, person, session.id, turn).await.unwrap(),
            DependencyCoverage::Unknown
        );

        let mut second = vault.create_session().await.unwrap();
        let second_turn = Uuid::new_v4();
        let second_call = Uuid::new_v4();
        second.messages.extend([
            floe_conversation::AgentMessage::User {
                turn_id: second_turn,
                text: "tool request".into(),
            },
            floe_conversation::AgentMessage::Capability {
                turn_id: second_turn,
                call_id: second_call,
                capability_id: "calendar.read".into(),
                input: "{}".into(),
                result: Ok("{}".into()),
            },
        ]);
        second.revision = 1;
        let governed = vault.governed_general_store(second.id);
        governed
            .record_result_independent(second_turn, second_call)
            .unwrap();
        governed
            .record_result_dependency(
                second_turn,
                second_call,
                authorized_dependency(&vault, b"result-bound").await,
            )
            .unwrap();
        governed.compare_and_swap(&second, 0).await.unwrap();
        assert!(matches!(
            read(&vault, person, second.id, second_turn).await.unwrap(),
            DependencyCoverage::Dependent { .. }
        ));
    }

    #[tokio::test]
    async fn storage_release_binds_target_and_rechecks_actual_key_health() {
        let root = root();
        let person = PersonId::new();
        let keys = TestKeys::default();
        let vault = Vault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let mut connection = vault.connection().unwrap();
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .unwrap();
        let authority = super::super::VaultTransactionAuthority {
            vault: &vault,
            transaction: &transaction,
            session_id: session.id,
            session_revision: 1,
        };
        let recipient = floe_access::ReleaseRecipient::Storage {
            person_id: person,
            vault_id: vault.vault_id,
        };
        for (target, session_id, revision) in [
            (
                floe_access::ReleaseRecipient::Storage {
                    person_id: PersonId::new(),
                    vault_id: vault.vault_id,
                },
                session.id,
                1,
            ),
            (
                floe_access::ReleaseRecipient::Storage {
                    person_id: person,
                    vault_id: Uuid::new_v4(),
                },
                session.id,
                1,
            ),
            (recipient, Uuid::new_v4(), 1),
            (recipient, session.id, 2),
        ] {
            assert!(matches!(
                floe_access::admit_release(
                    &DependencyCoverage::Independent,
                    target,
                    session_id,
                    revision,
                    &authority
                )
                .await,
                Err(AgentFailure::PolicyDenied)
            ));
        }
        let permit = floe_access::admit_release(
            &DependencyCoverage::Unknown,
            recipient,
            session.id,
            1,
            &authority,
        )
        .await
        .unwrap();
        keys.0.lock().unwrap().clear();
        assert_eq!(
            floe_access::consume_release(permit).await,
            Err(AgentFailure::VaultUnavailable)
        );
        transaction.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn revoked_grant_cannot_release_a_recorded_answer() {
        let root = root();
        let person = PersonId::new();
        let vault = Vault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let turn_id = Uuid::new_v4();
        let dependency = authorized_dependency(&vault, b"revoked-before-storage").await;
        let store = vault.governed_general_store(session.id);
        store
            .record_dependency(turn_id, dependency.clone())
            .await
            .unwrap();
        vault
            .revoke_data_access_grant(dependency.grant_id(), dependency.grant_authority())
            .await
            .unwrap();
        session.revision = 1;
        session.messages.push(AgentMessage::Assistant {
            turn_id,
            text: "Must not be released".into(),
        });
        assert_eq!(
            store.compare_and_swap(&session, 0).await,
            Err(AgentFailure::PolicyDenied)
        );
        let mut saved = vault.load(person, session.id).await.unwrap();
        assert_eq!(saved.revision, 0);
        assert!(saved.messages.is_empty());
        let independent_turn = Uuid::new_v4();
        saved.revision = 1;
        saved.messages.push(AgentMessage::User {
            turn_id: independent_turn,
            text: "Independent input".into(),
        });
        store.compare_and_swap(&saved, 0).await.unwrap();
        let saved = vault.load(person, session.id).await.unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.messages.len(), 1);
        assert_eq!(
            read(&vault, person, session.id, turn_id).await.unwrap(),
            DependencyCoverage::Unknown
        );
    }

    #[tokio::test]
    async fn dependent_release_rechecks_live_evidence_inside_final_transaction() {
        struct Liveness(std::sync::atomic::AtomicBool);

        impl floe_access::DependencyLiveness for Liveness {
            fn validate(&self, _: &ContextDependency) -> Result<(), AgentFailure> {
                if self.0.load(std::sync::atomic::Ordering::Acquire) {
                    Ok(())
                } else {
                    Err(AgentFailure::PolicyDenied)
                }
            }
        }

        let root = root();
        let person = PersonId::new();
        let vault = Vault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let turn_id = Uuid::new_v4();
        let dependency = authorized_dependency(&vault, b"final-fence").await;
        let liveness = Liveness(std::sync::atomic::AtomicBool::new(true));
        let store = vault.governed_general_store_with_liveness(session.id, &liveness);
        store.record_dependency(turn_id, dependency).await.unwrap();
        session.revision = 1;
        session.messages.push(AgentMessage::User {
            turn_id,
            text: "Question".into(),
        });
        store.compare_and_swap(&session, 0).await.unwrap();
        liveness
            .0
            .store(false, std::sync::atomic::Ordering::Release);
        session.revision = 2;
        session.messages.push(AgentMessage::Assistant {
            turn_id,
            text: "Unreleased answer".into(),
        });
        assert_eq!(
            store.compare_and_swap(&session, 1).await,
            Err(AgentFailure::PolicyDenied)
        );
        let saved = vault.load(person, session.id).await.unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.messages.len(), 1);
        let independent_turn = Uuid::new_v4();
        let mut independent = saved;
        independent.revision = 2;
        independent.messages.push(AgentMessage::User {
            turn_id: independent_turn,
            text: "Hello".into(),
        });
        store.compare_and_swap(&independent, 1).await.unwrap();
    }

    #[tokio::test]
    async fn context_evidence_reader_is_bound_to_its_session() {
        use floe_context::EvidenceReader;

        let root = root();
        let vault = Vault::create(root.path(), PersonId::new(), TestKeys::default())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let reader = crate::ContextEvidenceReader::new(&vault, session.id);
        let turn_id = Uuid::new_v4();
        assert_eq!(
            reader.read_turn_coverage(session.id, turn_id).await,
            Ok(DependencyCoverage::Unknown)
        );
        assert_eq!(
            reader.read_turn_coverage(Uuid::new_v4(), turn_id).await,
            Err(AgentFailure::Conflict)
        );
        assert_eq!(
            reader.read_turn_coverage(session.id, Uuid::nil()).await,
            Err(AgentFailure::InvalidInput)
        );
    }

    #[tokio::test]
    async fn projection_requires_resolver_and_persists_authorized_dependency() {
        let root = root();
        let person = PersonId::new();
        let vault = Vault::create(root.path(), person, TestKeys::default())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let historical_turn = Uuid::new_v4();
        let current_turn = Uuid::new_v4();
        let coverage =
            DependencyCoverage::dependent(authorized_dependency(&vault, b"history").await).unwrap();
        merge(&vault, person, session.id, historical_turn, coverage)
            .await
            .unwrap();
        let historical_messages = vec![
            AgentMessage::User {
                turn_id: historical_turn,
                text: "old request".into(),
            },
            AgentMessage::Assistant {
                turn_id: historical_turn,
                text: "old answer".into(),
            },
        ];
        let mut historical_session = session.clone();
        historical_session.messages = historical_messages.clone();
        historical_session.revision = 1;
        let governed = vault.governed_general_store(session.id);
        governed
            .compare_and_swap(&historical_session, 0)
            .await
            .unwrap();
        let session = vault.load(person, session.id).await.unwrap();
        let messages = vec![
            AgentMessage::User {
                turn_id: historical_turn,
                text: "old request".into(),
            },
            AgentMessage::Assistant {
                turn_id: historical_turn,
                text: "old answer".into(),
            },
            AgentMessage::User {
                turn_id: current_turn,
                text: "new request".into(),
            },
        ];
        let mut denied = projection_request(person, session.id, current_turn, messages.clone());
        governed
            .project_model_request(&mut denied, None)
            .await
            .unwrap();
        assert_eq!(denied.messages.len(), 2);
        assert!(denied.replay.is_empty());
        let mut allowed = projection_request(person, session.id, current_turn, messages);
        governed
            .project_model_request(&mut allowed, Some(&AllowDependency))
            .await
            .unwrap();
        assert_eq!(allowed.messages.len(), 3);
        assert!(allowed.replay.is_empty());

        let mut committed = session;
        committed.messages = allowed.messages;
        committed.revision = 2;
        governed.compare_and_swap(&committed, 1).await.unwrap();
        assert!(matches!(
            read(&vault, person, committed.id, current_turn)
                .await
                .unwrap(),
            DependencyCoverage::Dependent { .. }
        ));
    }

    #[tokio::test]
    async fn compaction_preserves_unknown_and_dependent_lineage_after_reopen() {
        let root = root();
        let person = PersonId::new();
        let keys = TestKeys::default();
        let vault = Vault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        let mut session = vault.create_session().await.unwrap();
        let independent_turn = Uuid::new_v4();
        let unknown_turn = Uuid::new_v4();
        session.messages.extend([
            AgentMessage::User {
                turn_id: independent_turn,
                text: "independent".into(),
            },
            AgentMessage::Assistant {
                turn_id: independent_turn,
                text: "answer".into(),
            },
        ]);
        session.revision = 1;
        let governed = vault.governed_general_store(session.id);
        governed.compare_and_swap(&session, 0).await.unwrap();
        session.messages.extend([
            AgentMessage::User {
                turn_id: unknown_turn,
                text: "tool".into(),
            },
            AgentMessage::Capability {
                turn_id: unknown_turn,
                call_id: Uuid::new_v4(),
                capability_id: "unregistered".into(),
                input: "{}".into(),
                result: Ok("private".into()),
            },
        ]);
        session.revision = 2;
        vault.compare_and_swap(&session, 1).await.unwrap();
        let compacted = vault
            .compact_session(session.id, 2, unknown_turn, "mixed summary".into())
            .await
            .unwrap();
        let compaction = compacted.session.messages[0].clone();
        drop(vault);
        let reopened = Vault::open(root.path(), person, keys).await.unwrap();
        let current_turn = Uuid::new_v4();
        let mut request = projection_request(
            person,
            session.id,
            current_turn,
            vec![
                compaction,
                AgentMessage::User {
                    turn_id: current_turn,
                    text: "next".into(),
                },
            ],
        );
        reopened
            .governed_general_store(session.id)
            .project_model_request(&mut request, None)
            .await
            .unwrap();
        assert_eq!(request.messages.len(), 1);

        let mut dependent_session = reopened.create_session().await.unwrap();
        let archived_independent_turn = Uuid::new_v4();
        let dependent_turn = Uuid::new_v4();
        merge(
            &reopened,
            person,
            dependent_session.id,
            dependent_turn,
            DependencyCoverage::dependent(dependency(person, b"compaction-dependent")).unwrap(),
        )
        .await
        .unwrap();
        dependent_session.messages = vec![
            AgentMessage::User {
                turn_id: archived_independent_turn,
                text: "independent".into(),
            },
            AgentMessage::Assistant {
                turn_id: archived_independent_turn,
                text: "answer".into(),
            },
            AgentMessage::User {
                turn_id: dependent_turn,
                text: "dependent".into(),
            },
            AgentMessage::Assistant {
                turn_id: dependent_turn,
                text: "answer".into(),
            },
        ];
        dependent_session.revision = 1;
        reopened
            .governed_general_store(dependent_session.id)
            .compare_and_swap(&dependent_session, 0)
            .await
            .unwrap();
        let compacted = reopened
            .compact_session(
                dependent_session.id,
                1,
                dependent_turn,
                "dependent summary".into(),
            )
            .await
            .unwrap();
        let dependent_compaction = compacted.session.messages[0].clone();
        let current_turn = Uuid::new_v4();
        let mut request = projection_request(
            person,
            dependent_session.id,
            current_turn,
            vec![
                dependent_compaction,
                AgentMessage::User {
                    turn_id: current_turn,
                    text: "next".into(),
                },
            ],
        );
        reopened
            .governed_general_store(dependent_session.id)
            .project_model_request(&mut request, None)
            .await
            .unwrap();
        assert_eq!(request.messages.len(), 1);
    }
}
