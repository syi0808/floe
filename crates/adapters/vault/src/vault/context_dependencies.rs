use floe_access::{ContextDependencyError, DependencyCoverage, MAX_CONTEXT_DEPENDENCY_BYTES};
use floe_agent_contract::AgentFailure;
use floe_kernel::PersonId;
use turso::transaction::Transaction;
use uuid::Uuid;

// Version of the serialized coverage row, distinct from schema lifecycle.
const CONTEXT_DEPENDENCY_RECORD_VERSION: i64 = 1;
const MAX_CONTEXT_DEPENDENCY_ROWS: i64 = 100_000;
pub(super) const MAX_CONTEXT_DEPENDENCY_TURNS_PER_SESSION: i64 = 4_096;
pub(super) const MAX_CONTEXT_DEPENDENCY_SESSION_BYTES: i64 = 4 * 1024 * 1024;

pub(super) async fn validate_context_dependency_store(
    transaction: &Transaction<'_>,
) -> Result<(), AgentFailure> {
    crate::schema::inspect_family(transaction, crate::schema::Family::Context)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)?;
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
            || row.get::<i64>(3).map_err(storage)? != CONTEXT_DEPENDENCY_RECORD_VERSION
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
    connection: &turso::Connection,
    person_id: PersonId,
    session_id: Uuid,
    turn_id: Uuid,
) -> Result<DependencyCoverage, AgentFailure> {
    validate_key(person_id, session_id, turn_id)?;
    let mut rows = connection
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
        || row.get::<i64>(0).map_err(storage)? != CONTEXT_DEPENDENCY_RECORD_VERSION
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
            (CONTEXT_DEPENDENCY_RECORD_VERSION, String::from_utf8(payload.clone()).map_err(|_| AgentFailure::InvalidInput)?, person_id.to_string(), session_id.to_string(), turn_id.to_string()),
        )
        .await
        .map_err(storage)?;
    if changed == 0 {
        transaction
            .execute(
                "INSERT INTO agent_context_dependency_coverage (person_id, session_id, turn_id, version, payload) VALUES (?, ?, ?, ?, ?)",
                (person_id.to_string(), session_id.to_string(), turn_id.to_string(), CONTEXT_DEPENDENCY_RECORD_VERSION, String::from_utf8(payload).map_err(|_| AgentFailure::InvalidInput)?),
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
        || row.get::<i64>(0).map_err(storage)? != CONTEXT_DEPENDENCY_RECORD_VERSION
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
