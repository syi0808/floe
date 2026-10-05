use super::database_failure;
use floe_access::{DependencyCoverage, MAX_CONTEXT_DEPENDENCY_BYTES};
use floe_agent_contract::AgentFailure;
use turso::{Row, transaction::Transaction};
use uuid::Uuid;

use super::access_grants::AccessGrantCleanup;
use super::*;

const MAX_CONTEXT_CLEANUP_BATCH: usize = 16;
const MAX_CONTEXT_CLEANUP_ROWS: i64 = 4096;
const MAX_CONTEXT_CLEANUP_BYTES: i64 = 4 * 1024 * 1024;
const MAX_CONTEXT_CLEANUP_SUPPRESSION_ROWS: i64 = 100_000;
const MAX_CONTEXT_CLEANUP_WORK_ROWS: usize = 1024;
const MAX_CONTEXT_CLEANUP_WORK_BYTES: usize = 4 * 1024 * 1024;

struct CleanupBudget {
    rows: usize,
    bytes: usize,
}

impl CleanupBudget {
    fn new() -> Self {
        Self {
            rows: MAX_CONTEXT_CLEANUP_WORK_ROWS,
            bytes: MAX_CONTEXT_CLEANUP_WORK_BYTES,
        }
    }

    fn exhausted(&self) -> bool {
        self.rows == 0 || self.bytes == 0
    }

    fn take(&mut self, bytes: usize) -> bool {
        if self.rows == 0 || bytes > self.bytes {
            return false;
        }
        self.rows -= 1;
        self.bytes -= bytes;
        true
    }
}

async fn validate_context_cleanup_store(
    transaction: &Transaction<'_>,
    person_id: PersonId,
) -> Result<(), AgentFailure> {
    crate::schema::inspect_family(transaction, crate::schema::Family::Cleanup)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)?;
    let mut applied = transaction
        .query(
            "SELECT cleanup_id, person_id, grant_id, invalidated_incarnation, invalidated_epoch, payload, coverage_cursor, coverage_complete FROM agent_context_cleanup_applied LIMIT ?",
            [MAX_CONTEXT_CLEANUP_ROWS + 1],
        )
        .await
        .map_err(database_failure)?;
    let mut count = 0;
    while let Some(row) = applied.next().await.map_err(database_failure)? {
        count += 1;
        if count > MAX_CONTEXT_CLEANUP_ROWS {
            return Err(AgentFailure::BudgetExceeded);
        }
        decode_applied_row(&row, person_id)?;
    }
    let mut suppression = transaction
        .query(
            "SELECT cleanup_id, person_id, session_id, turn_id, grant_id, invalidated_incarnation, invalidated_epoch, payload FROM agent_context_cleanup_suppression LIMIT ?",
            [MAX_CONTEXT_CLEANUP_SUPPRESSION_ROWS + 1],
        )
        .await
        .map_err(database_failure)?;
    let mut count = 0;
    while let Some(row) = suppression.next().await.map_err(database_failure)? {
        count += 1;
        if count > MAX_CONTEXT_CLEANUP_SUPPRESSION_ROWS {
            return Err(AgentFailure::BudgetExceeded);
        }
        decode_suppression_row(&row, person_id)?;
    }
    Ok(())
}

async fn validate_context_cleanup_runtime(
    transaction: &Transaction<'_>,
) -> Result<(), AgentFailure> {
    crate::schema::inspect_family(transaction, crate::schema::Family::Cleanup)
        .await
        .map_err(crate::schema::SchemaFailure::into_agent)
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn validate_context_cleanup(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let result = validate_context_cleanup_store(&transaction, self.person_id).await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    pub async fn drain_context_cleanup(&self, batch_limit: usize) -> Result<usize, AgentFailure> {
        if batch_limit == 0 || batch_limit > MAX_CONTEXT_CLEANUP_BATCH {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(database_failure)?;
        let result = async {
            validate_context_cleanup_runtime(&transaction).await?;
            let items = self
                .pending_data_access_grant_cleanup_in_transaction(&transaction, batch_limit)
                .await?;
            let mut budget = CleanupBudget::new();
            let mut processed = 0;
            for item in items {
                if budget.exhausted() {
                    break;
                }
                if self.apply_cleanup(&transaction, &item, &mut budget).await? {
                    self.acknowledge_data_access_grant_cleanup_in_transaction(&transaction, &item)
                        .await?;
                    processed += 1;
                }
                if budget.exhausted() {
                    break;
                }
            }
            Ok(processed)
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }

    async fn apply_cleanup(
        &self,
        transaction: &Transaction<'_>,
        item: &AccessGrantCleanup,
        budget: &mut CleanupBudget,
    ) -> Result<bool, AgentFailure> {
        let cleanup_id = cleanup_id(item);
        let payload = serde_json::to_string(item).map_err(storage)?;
        let mut existing = transaction
            .query(
                "SELECT person_id, grant_id, invalidated_incarnation, invalidated_epoch, payload, coverage_cursor, coverage_complete FROM agent_context_cleanup_applied WHERE cleanup_id = ?",
                [cleanup_id.clone()],
            )
            .await
            .map_err(database_failure)?;
        let (mut coverage_cursor, mut coverage_complete);
        if let Some(row) = existing.next().await.map_err(database_failure)? {
            if existing.next().await.map_err(database_failure)?.is_some()
                || row.get::<String>(4).map_err(storage)? != payload
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            coverage_cursor = row.get::<i64>(5).map_err(storage)?;
            let stored_complete = row.get::<i64>(6).map_err(storage)?;
            coverage_complete = stored_complete != 0;
            if coverage_cursor < 0 || ![0, 1].contains(&stored_complete) {
                return Err(AgentFailure::VaultUnavailable);
            }
        } else {
            let mut count = transaction
                .query(
                    "SELECT COUNT(*), COALESCE(SUM(length(CAST(payload AS BLOB))), 0) FROM agent_context_cleanup_applied WHERE person_id = ?",
                    [self.person_id.to_string()],
                )
                .await
                .map_err(database_failure)?;
            let row = count
                .next()
                .await
                .map_err(database_failure)?
                .ok_or(AgentFailure::VaultUnavailable)?;
            let rows = row.get::<i64>(0).map_err(storage)?;
            let bytes = row.get::<i64>(1).map_err(storage)?;
            if rows >= MAX_CONTEXT_CLEANUP_ROWS || bytes > MAX_CONTEXT_CLEANUP_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            transaction
                .execute(
                    "INSERT INTO agent_context_cleanup_applied (cleanup_id, person_id, grant_id, invalidated_incarnation, invalidated_epoch, payload, coverage_cursor, coverage_complete) VALUES (?, ?, ?, ?, ?, ?, 0, 0)",
                    (
                        cleanup_id.clone(),
                        self.person_id.to_string(),
                        item.grant_id.as_uuid().to_string(),
                        item.invalidated_authority.incarnation().to_string(),
                        i64::try_from(item.invalidated_authority.access_epoch().get())
                            .map_err(|_| AgentFailure::BudgetExceeded)?,
                        payload.clone(),
                    ),
                )
                .await
                .map_err(database_failure)?;
            coverage_cursor = 0;
            coverage_complete = false;
        }

        if !coverage_complete {
            let (next_cursor, complete) = self
                .index_cleanup_coverage(
                    transaction,
                    item,
                    cleanup_id.clone(),
                    payload.clone(),
                    coverage_cursor,
                    budget,
                )
                .await?;
            coverage_cursor = next_cursor;
            coverage_complete = complete;
            transaction
                .execute(
                    "UPDATE agent_context_cleanup_applied SET coverage_cursor = ?, coverage_complete = ? WHERE cleanup_id = ?",
                    (coverage_cursor, i64::from(coverage_complete), cleanup_id.clone()),
                )
                .await
                .map_err(database_failure)?;
            if !coverage_complete {
                return Ok(false);
            }
        }

        Ok(coverage_complete)
    }

    async fn index_cleanup_coverage(
        &self,
        transaction: &Transaction<'_>,
        item: &AccessGrantCleanup,
        cleanup_id: String,
        payload: String,
        cursor: i64,
        budget: &mut CleanupBudget,
    ) -> Result<(i64, bool), AgentFailure> {
        let limit = i64::try_from(budget.rows.min(MAX_CONTEXT_CLEANUP_ROWS as usize))
            .map_err(|_| AgentFailure::BudgetExceeded)?;
        let mut coverage = transaction
            .query(
                "SELECT rowid, session_id, turn_id, payload FROM agent_context_dependency_coverage WHERE person_id = ? AND rowid > ? ORDER BY rowid LIMIT ?",
                (self.person_id.to_string(), cursor, limit),
            )
            .await
            .map_err(database_failure)?;
        let mut last_cursor = cursor;
        let mut scanned = 0;
        let mut stopped_for_budget = false;
        while let Some(row) = coverage.next().await.map_err(database_failure)? {
            let coverage_payload = row.get::<String>(3).map_err(storage)?;
            if coverage_payload.len() > MAX_CONTEXT_DEPENDENCY_BYTES {
                return Err(AgentFailure::VaultUnavailable);
            }
            if !budget.take(coverage_payload.len()) {
                stopped_for_budget = true;
                break;
            }
            scanned += 1;
            last_cursor = row.get::<i64>(0).map_err(storage)?;
            let parsed = DependencyCoverage::from_persisted_bytes(coverage_payload.as_bytes())
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            let dependent = match parsed {
                DependencyCoverage::Dependent { dependencies } => dependencies,
                _ => continue,
            };
            if dependent.iter().any(|dependency| {
                dependency.grant_id() == item.grant_id
                    && dependency.grant_authority() == item.invalidated_authority
            }) {
                transaction
                    .execute(
                        "INSERT OR IGNORE INTO agent_context_cleanup_suppression (cleanup_id, person_id, session_id, turn_id, grant_id, invalidated_incarnation, invalidated_epoch, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                        (
                            cleanup_id.clone(),
                            self.person_id.to_string(),
                            row.get::<String>(1).map_err(storage)?,
                            row.get::<String>(2).map_err(storage)?,
                            item.grant_id.as_uuid().to_string(),
                            item.invalidated_authority.incarnation().to_string(),
                            i64::try_from(item.invalidated_authority.access_epoch().get())
                                .map_err(|_| AgentFailure::BudgetExceeded)?,
                            payload.clone(),
                        ),
                    )
                    .await
                    .map_err(database_failure)?;
            }
        }
        Ok((last_cursor, !stopped_for_budget && scanned < limit))
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

fn decode_applied_row(
    row: &Row,
    expected_person: PersonId,
) -> Result<AccessGrantCleanup, AgentFailure> {
    let payload = row.get::<String>(5).map_err(storage)?;
    if payload.is_empty() || payload.len() > MAX_CONTEXT_DEPENDENCY_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    let item: AccessGrantCleanup =
        serde_json::from_str(&payload).map_err(|_| AgentFailure::VaultUnavailable)?;
    if row.get::<String>(0).map_err(storage)? != cleanup_id(&item)
        || row.get::<String>(1).map_err(storage)? != expected_person.to_string()
        || row.get::<String>(2).map_err(storage)? != item.grant_id.as_uuid().to_string()
        || row.get::<String>(3).map_err(storage)?
            != item.invalidated_authority.incarnation().to_string()
        || row.get::<i64>(4).map_err(storage)?
            != i64::try_from(item.invalidated_authority.access_epoch().get())
                .map_err(|_| AgentFailure::VaultUnavailable)?
        || row.get::<i64>(6).map_err(storage)? < 0
        || ![0, 1].contains(&row.get::<i64>(7).map_err(storage)?)
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(item)
}

fn decode_suppression_row(row: &Row, expected_person: PersonId) -> Result<(), AgentFailure> {
    let payload = row.get::<String>(7).map_err(storage)?;
    if payload.is_empty() || payload.len() > MAX_CONTEXT_DEPENDENCY_BYTES {
        return Err(AgentFailure::VaultUnavailable);
    }
    let item: AccessGrantCleanup =
        serde_json::from_str(&payload).map_err(|_| AgentFailure::VaultUnavailable)?;
    let session = Uuid::parse_str(&row.get::<String>(2).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    let turn = Uuid::parse_str(&row.get::<String>(3).map_err(storage)?)
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if session.is_nil()
        || turn.is_nil()
        || row.get::<String>(0).map_err(storage)? != cleanup_id(&item)
        || row.get::<String>(1).map_err(storage)? != expected_person.to_string()
        || row.get::<String>(4).map_err(storage)? != item.grant_id.as_uuid().to_string()
        || row.get::<String>(5).map_err(storage)?
            != item.invalidated_authority.incarnation().to_string()
        || row.get::<i64>(6).map_err(storage)?
            != i64::try_from(item.invalidated_authority.access_epoch().get())
                .map_err(|_| AgentFailure::VaultUnavailable)?
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

fn storage(_: impl std::fmt::Debug) -> AgentFailure {
    AgentFailure::StorageUnavailable
}
