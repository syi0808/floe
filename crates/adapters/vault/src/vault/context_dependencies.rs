use super::database_failure;
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
        .map_err(database_failure)?;
    let mut count = 0i64;
    while let Some(row) = rows.next().await.map_err(database_failure)? {
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
    read_context_dependency_coverage_inner(connection, person_id, session_id, turn_id, None).await
}

#[cfg(test)]
pub(super) async fn read_context_dependency_coverage_counted(
    connection: &turso::Connection,
    person_id: PersonId,
    session_id: Uuid,
    turn_id: Uuid,
    query_count: &std::sync::atomic::AtomicU64,
) -> Result<DependencyCoverage, AgentFailure> {
    read_context_dependency_coverage_inner(
        connection,
        person_id,
        session_id,
        turn_id,
        Some(query_count),
    )
    .await
}

async fn read_context_dependency_coverage_inner(
    connection: &turso::Connection,
    person_id: PersonId,
    session_id: Uuid,
    turn_id: Uuid,
    query_count: Option<&std::sync::atomic::AtomicU64>,
) -> Result<DependencyCoverage, AgentFailure> {
    validate_key(person_id, session_id, turn_id)?;
    if let Some(query_count) = query_count {
        query_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    let mut rows = connection
        .query(
            "SELECT version, payload FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (person_id.to_string(), session_id.to_string(), turn_id.to_string()),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(DependencyCoverage::Unknown);
    };
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    decode_context_dependency_coverage(
        person_id,
        row.get::<i64>(0).map_err(storage)?,
        row.get::<String>(1).map_err(storage)?,
    )
}

/// Metadata for the transaction-scoped second-phase owner read. The normal
/// learning read above deliberately remains a single SELECT of version and
/// body so it cannot race between separate metadata and hydration queries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ContextDependencyCoverageHeader {
    stored_bytes: Option<usize>,
}

impl ContextDependencyCoverageHeader {
    pub(super) fn accounted_bytes(self) -> Result<usize, AgentFailure> {
        match self.stored_bytes {
            Some(bytes) => Ok(bytes),
            None => DependencyCoverage::Unknown
                .as_persisted_bytes()
                .map(|bytes| bytes.len())
                .map_err(|_| AgentFailure::VaultUnavailable),
        }
    }
}

/// Preflight the live coverage row without selecting its body. This helper
/// requires the caller's explicit transaction so the later hydration observes
/// the same snapshot.
pub(super) async fn preflight_context_dependency_coverage_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    session_id: Uuid,
    turn_id: Uuid,
) -> Result<ContextDependencyCoverageHeader, AgentFailure> {
    validate_key(person_id, session_id, turn_id)?;
    let mut rows = transaction
        .query(
            "SELECT version, length(CAST(payload AS BLOB)) FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (person_id.to_string(), session_id.to_string(), turn_id.to_string()),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(ContextDependencyCoverageHeader { stored_bytes: None });
    };
    let version = row.get::<i64>(0).map_err(storage)?;
    let length = row.get::<i64>(1).map_err(storage)?;
    if rows.next().await.map_err(database_failure)?.is_some()
        || version != CONTEXT_DEPENDENCY_RECORD_VERSION
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let length = usize::try_from(length).map_err(|_| AgentFailure::VaultUnavailable)?;
    if length == 0 || length > MAX_CONTEXT_DEPENDENCY_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(ContextDependencyCoverageHeader {
        stored_bytes: Some(length),
    })
}

/// Hydrate the body corresponding to a transaction-scoped preflight header.
/// A missing row is represented by encoded `Unknown` and does not trigger a
/// second query.
pub(super) async fn hydrate_context_dependency_coverage_on(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    session_id: Uuid,
    turn_id: Uuid,
    header: ContextDependencyCoverageHeader,
    #[cfg(test)] payload_hydrations: &std::sync::atomic::AtomicU64,
) -> Result<DependencyCoverage, AgentFailure> {
    validate_key(person_id, session_id, turn_id)?;
    let Some(expected_bytes) = header.stored_bytes else {
        return Ok(DependencyCoverage::Unknown);
    };
    #[cfg(test)]
    payload_hydrations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut rows = transaction
        .query(
            "SELECT version, payload FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ? AND turn_id = ?",
            (person_id.to_string(), session_id.to_string(), turn_id.to_string()),
        )
        .await
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Err(AgentFailure::VaultUnavailable);
    };
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    let version = row.get::<i64>(0).map_err(storage)?;
    let payload = row.get::<String>(1).map_err(storage)?;
    if payload.len() != expected_bytes {
        return Err(AgentFailure::VaultUnavailable);
    }
    decode_context_dependency_coverage(person_id, version, payload)
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
            .map_err(database_failure)?;
    let quota = quota
        .next()
        .await
        .map_err(database_failure)?
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
            .map_err(database_failure)?;
        existing
            .next()
            .await
            .map_err(database_failure)?
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
        .map_err(database_failure)?;
    if changed == 0 {
        transaction
            .execute(
                "INSERT INTO agent_context_dependency_coverage (person_id, session_id, turn_id, version, payload) VALUES (?, ?, ?, ?, ?)",
                (person_id.to_string(), session_id.to_string(), turn_id.to_string(), CONTEXT_DEPENDENCY_RECORD_VERSION, String::from_utf8(payload).map_err(|_| AgentFailure::InvalidInput)?),
            )
            .await
            .map_err(database_failure)?;
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
        .map_err(database_failure)?;
    let Some(row) = rows.next().await.map_err(database_failure)? else {
        return Ok(None);
    };
    if rows.next().await.map_err(database_failure)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    let version = row.get::<i64>(0).map_err(storage)?;
    let payload = row.get::<String>(1).map_err(storage)?;
    decode_context_dependency_coverage(person_id, version, payload).map(Some)
}

fn decode_context_dependency_coverage(
    person_id: PersonId,
    version: i64,
    payload: String,
) -> Result<DependencyCoverage, AgentFailure> {
    if version != CONTEXT_DEPENDENCY_RECORD_VERSION
        || payload.is_empty()
        || payload.len() > MAX_CONTEXT_DEPENDENCY_BYTES
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let coverage = DependencyCoverage::from_persisted_bytes(payload.as_bytes())
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    ensure_coverage_person(&coverage, person_id)?;
    Ok(coverage)
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
