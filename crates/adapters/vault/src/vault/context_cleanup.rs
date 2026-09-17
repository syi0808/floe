use floe_access::{DependencyCoverage, MAX_CONTEXT_DEPENDENCY_BYTES};
use floe_agent_contract::AgentFailure;
use floe_conversation::AgentSession;
use turso::{Row, transaction::Transaction};
use uuid::Uuid;

use super::access_grants::AccessGrantCleanup;
use super::*;

const CONTEXT_CLEANUP_SCHEMA_VERSION: i64 = 1;
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

pub(super) async fn initialize_context_cleanup_store(
    transaction: &Transaction<'_>,
    person_id: PersonId,
    create_if_missing: bool,
) -> Result<(), AgentFailure> {
    let mut tables = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name IN (?, ?, ?)",
            (
                "agent_context_cleanup_schema",
                "agent_context_cleanup_applied",
                "agent_context_cleanup_suppression",
            ),
        )
        .await
        .map_err(storage)?;
    let mut found = Vec::new();
    while let Some(row) = tables.next().await.map_err(storage)? {
        found.push(row.get::<String>(0).map_err(storage)?);
    }
    found.sort();
    let expected = vec![
        "agent_context_cleanup_applied".to_owned(),
        "agent_context_cleanup_schema".to_owned(),
        "agent_context_cleanup_suppression".to_owned(),
    ];
    if found.is_empty() {
        if !create_if_missing {
            return Err(AgentFailure::VaultUnavailable);
        }
        transaction
            .execute(
                "CREATE TABLE agent_context_cleanup_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_context_cleanup_applied (cleanup_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, grant_id TEXT NOT NULL, invalidated_incarnation TEXT NOT NULL, invalidated_epoch INTEGER NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 65536), coverage_cursor INTEGER NOT NULL DEFAULT 0, session_cursor INTEGER NOT NULL DEFAULT 0, coverage_complete INTEGER NOT NULL CHECK (coverage_complete IN (0, 1)), session_complete INTEGER NOT NULL CHECK (session_complete IN (0, 1)))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_context_cleanup_suppression (cleanup_id TEXT NOT NULL, person_id TEXT NOT NULL, session_id TEXT NOT NULL, turn_id TEXT NOT NULL, grant_id TEXT NOT NULL, invalidated_incarnation TEXT NOT NULL, invalidated_epoch INTEGER NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 65536), PRIMARY KEY (cleanup_id, session_id, turn_id))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE INDEX agent_context_cleanup_suppression_turn_idx ON agent_context_cleanup_suppression (person_id, session_id, turn_id)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_context_cleanup_schema (id, version) VALUES (1, 1)",
                (),
            )
            .await
            .map_err(storage)?;
        return Ok(());
    }
    if found != expected {
        return Err(AgentFailure::VaultUnavailable);
    }
    validate_context_cleanup_store(transaction, person_id).await
}

async fn validate_context_cleanup_store(
    transaction: &Transaction<'_>,
    person_id: PersonId,
) -> Result<(), AgentFailure> {
    let mut marker = transaction
        .query("SELECT id, version FROM agent_context_cleanup_schema", ())
        .await
        .map_err(storage)?;
    let Some(row) = marker.next().await.map_err(storage)? else {
        return Err(AgentFailure::VaultUnavailable);
    };
    if row.get::<i64>(0).map_err(storage)? != 1
        || row.get::<i64>(1).map_err(storage)? != CONTEXT_CLEANUP_SCHEMA_VERSION
        || marker.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut indexes = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'index' AND name = 'agent_context_cleanup_suppression_turn_idx'",
            (),
        )
        .await
        .map_err(storage)?;
    if indexes.next().await.map_err(storage)?.is_none() {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut applied = transaction
        .query(
            "SELECT cleanup_id, person_id, grant_id, invalidated_incarnation, invalidated_epoch, payload, coverage_cursor, session_cursor, coverage_complete, session_complete FROM agent_context_cleanup_applied LIMIT ?",
            [MAX_CONTEXT_CLEANUP_ROWS + 1],
        )
        .await
        .map_err(storage)?;
    let mut count = 0;
    while let Some(row) = applied.next().await.map_err(storage)? {
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
        .map_err(storage)?;
    let mut count = 0;
    while let Some(row) = suppression.next().await.map_err(storage)? {
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
    let mut marker = transaction
        .query(
            "SELECT id, version FROM agent_context_cleanup_schema WHERE id = 1",
            (),
        )
        .await
        .map_err(storage)?;
    let Some(row) = marker.next().await.map_err(storage)? else {
        return Err(AgentFailure::VaultUnavailable);
    };
    if row.get::<i64>(0).map_err(storage)? != 1
        || row.get::<i64>(1).map_err(storage)? != CONTEXT_CLEANUP_SCHEMA_VERSION
        || marker.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    for query in [
        "SELECT cleanup_id, person_id, grant_id, invalidated_incarnation, invalidated_epoch, payload, coverage_cursor, session_cursor, coverage_complete, session_complete FROM agent_context_cleanup_applied LIMIT 0",
        "SELECT cleanup_id, person_id, session_id, turn_id, grant_id, invalidated_incarnation, invalidated_epoch, payload FROM agent_context_cleanup_suppression LIMIT 0",
    ] {
        transaction.query(query, ()).await.map_err(storage)?;
    }
    Ok(())
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn initialize_context_cleanup(
        &self,
        create_if_missing: bool,
    ) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .map_err(|error| match error {
                turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => AgentFailure::Conflict,
                _ => AgentFailure::StorageUnavailable,
            })?;
        let result =
            initialize_context_cleanup_store(&transaction, self.person_id, create_if_missing).await;
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
            .map_err(|error| match error {
                turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => AgentFailure::Conflict,
                _ => AgentFailure::StorageUnavailable,
            })?;
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

    pub(super) async fn context_cleanup_applies_to_turn(
        &self,
        transaction: &Transaction<'_>,
        session_id: Uuid,
        turn_id: Uuid,
    ) -> Result<bool, AgentFailure> {
        validate_context_cleanup_runtime(transaction).await?;
        let mut rows = transaction
            .query(
                "SELECT 1 FROM agent_context_cleanup_suppression WHERE person_id = ? AND session_id = ? AND turn_id = ? LIMIT 1",
                (
                    self.person_id.to_string(),
                    session_id.to_string(),
                    turn_id.to_string(),
                ),
            )
            .await
            .map_err(storage)?;
        if rows.next().await.map_err(storage)?.is_some() {
            return Ok(true);
        }
        let mut coverage = transaction
            .query(
                "SELECT payload FROM agent_context_dependency_coverage WHERE person_id = ? AND session_id = ? AND turn_id = ?",
                (
                    self.person_id.to_string(),
                    session_id.to_string(),
                    turn_id.to_string(),
                ),
            )
            .await
            .map_err(storage)?;
        let Some(row) = coverage.next().await.map_err(storage)? else {
            return Ok(true);
        };
        if coverage.next().await.map_err(storage)?.is_some() {
            return Err(AgentFailure::VaultUnavailable);
        }
        let payload = row.get::<String>(0).map_err(storage)?;
        let parsed = DependencyCoverage::from_persisted_bytes(payload.as_bytes())
            .map_err(|_| AgentFailure::VaultUnavailable)?;
        let dependencies = match parsed {
            DependencyCoverage::Dependent { dependencies } => dependencies,
            DependencyCoverage::Independent => return Ok(false),
            DependencyCoverage::Unknown => return Ok(true),
        };
        for dependency in dependencies {
            let mut invalidations = transaction
                .query(
                    "SELECT 1 FROM agent_context_cleanup_applied WHERE person_id = ? AND grant_id = ? AND invalidated_incarnation = ? AND invalidated_epoch = ? LIMIT 1",
                    (
                        self.person_id.to_string(),
                        dependency.grant_id().as_uuid().to_string(),
                        dependency.grant_authority().incarnation().to_string(),
                        i64::try_from(dependency.grant_authority().access_epoch().get())
                            .map_err(|_| AgentFailure::VaultUnavailable)?,
                    ),
                )
                .await
                .map_err(storage)?;
            if invalidations.next().await.map_err(storage)?.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) async fn sanitize_session_for_context_cleanup(
        &self,
        transaction: &Transaction<'_>,
        session: &mut AgentSession,
    ) -> Result<(), AgentFailure> {
        for execution in &mut session.capability_executions {
            if execution.replay.is_some()
                && self
                    .context_cleanup_applies_to_turn(transaction, session.id, execution.turn_id)
                    .await?
            {
                execution.replay = None;
            }
        }
        for execution in &mut session.delegation_executions {
            if execution.replay.is_some()
                && self
                    .context_cleanup_applies_to_turn(transaction, session.id, execution.turn_id)
                    .await?
            {
                execution.replay = None;
            }
        }
        Ok(())
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
                "SELECT person_id, grant_id, invalidated_incarnation, invalidated_epoch, payload, coverage_cursor, session_cursor, coverage_complete, session_complete FROM agent_context_cleanup_applied WHERE cleanup_id = ?",
                [cleanup_id.clone()],
            )
            .await
            .map_err(storage)?;
        let (mut coverage_cursor, mut session_cursor, mut coverage_complete, mut session_complete);
        if let Some(row) = existing.next().await.map_err(storage)? {
            if existing.next().await.map_err(storage)?.is_some()
                || row.get::<String>(4).map_err(storage)? != payload
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            coverage_cursor = row.get::<i64>(5).map_err(storage)?;
            session_cursor = row.get::<i64>(6).map_err(storage)?;
            coverage_complete = row.get::<i64>(7).map_err(storage)? != 0;
            session_complete = row.get::<i64>(8).map_err(storage)? != 0;
            if coverage_cursor < 0
                || session_cursor < 0
                || row.get::<i64>(7).map_err(storage)? > 1
                || row.get::<i64>(8).map_err(storage)? > 1
            {
                return Err(AgentFailure::VaultUnavailable);
            }
        } else {
            let mut count = transaction
                .query(
                    "SELECT COUNT(*), COALESCE(SUM(length(CAST(payload AS BLOB))), 0) FROM agent_context_cleanup_applied WHERE person_id = ?",
                    [self.person_id.to_string()],
                )
                .await
                .map_err(storage)?;
            let row = count
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?;
            let rows = row.get::<i64>(0).map_err(storage)?;
            let bytes = row.get::<i64>(1).map_err(storage)?;
            if rows >= MAX_CONTEXT_CLEANUP_ROWS || bytes > MAX_CONTEXT_CLEANUP_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            transaction
                .execute(
                    "INSERT INTO agent_context_cleanup_applied (cleanup_id, person_id, grant_id, invalidated_incarnation, invalidated_epoch, payload, coverage_cursor, session_cursor, coverage_complete, session_complete) VALUES (?, ?, ?, ?, ?, ?, 0, 0, 0, 0)",
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
                .map_err(storage)?;
            coverage_cursor = 0;
            session_cursor = 0;
            coverage_complete = false;
            session_complete = false;
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
                .map_err(storage)?;
            if !coverage_complete {
                return Ok(false);
            }
        }

        if !session_complete {
            let (next_cursor, complete) = self
                .scrub_session_replays(transaction, session_cursor, budget)
                .await?;
            session_cursor = next_cursor;
            session_complete = complete;
            transaction
                .execute(
                    "UPDATE agent_context_cleanup_applied SET session_cursor = ?, session_complete = ? WHERE cleanup_id = ?",
                    (session_cursor, i64::from(session_complete), cleanup_id),
                )
                .await
                .map_err(storage)?;
        }
        Ok(coverage_complete && session_complete)
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
            .map_err(storage)?;
        let mut last_cursor = cursor;
        let mut scanned = 0;
        let mut stopped_for_budget = false;
        while let Some(row) = coverage.next().await.map_err(storage)? {
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
                    .map_err(storage)?;
            }
        }
        Ok((last_cursor, !stopped_for_budget && scanned < limit))
    }

    async fn scrub_session_replays(
        &self,
        transaction: &Transaction<'_>,
        cursor: i64,
        budget: &mut CleanupBudget,
    ) -> Result<(i64, bool), AgentFailure> {
        let limit = i64::try_from(budget.rows.min(MAX_CONTEXT_CLEANUP_ROWS as usize))
            .map_err(|_| AgentFailure::BudgetExceeded)?;
        let mut rows = transaction
            .query(
                "SELECT rowid, id, revision, payload FROM agent_sessions WHERE rowid > ? ORDER BY rowid LIMIT ?",
                (
                    cursor,
                    limit,
                ),
            )
            .await
            .map_err(storage)?;
        let mut scanned = 0;
        let mut last_cursor = cursor;
        let mut stopped_for_budget = false;
        while let Some(row) = rows.next().await.map_err(storage)? {
            let payload = row.get::<String>(3).map_err(storage)?;
            if !budget.take(payload.len()) {
                stopped_for_budget = true;
                break;
            }
            scanned += 1;
            last_cursor = row.get::<i64>(0).map_err(storage)?;
            if payload.len() > AgentBudget::default().max_session_bytes {
                return Err(AgentFailure::BudgetExceeded);
            }
            let mut session: AgentSession = serde_json::from_str(&payload).map_err(storage)?;
            let mut changed = false;
            for execution in &mut session.capability_executions {
                if execution.replay.is_some()
                    && self
                        .context_cleanup_applies_to_turn(transaction, session.id, execution.turn_id)
                        .await?
                {
                    execution.replay = None;
                    changed = true;
                }
            }
            for execution in &mut session.delegation_executions {
                if execution.replay.is_some()
                    && self
                        .context_cleanup_applies_to_turn(transaction, session.id, execution.turn_id)
                        .await?
                {
                    execution.replay = None;
                    changed = true;
                }
            }
            if changed {
                let next_payload = self.payload(&session)?;
                transaction
                    .execute(
                        "UPDATE agent_sessions SET payload = ? WHERE id = ? AND revision = ?",
                        (
                            next_payload,
                            row.get::<String>(1).map_err(storage)?,
                            row.get::<i64>(2).map_err(storage)?,
                        ),
                    )
                    .await
                    .map_err(storage)?;
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
        || row.get::<i64>(7).map_err(storage)? < 0
        || ![0, 1].contains(&row.get::<i64>(8).map_err(storage)?)
        || ![0, 1].contains(&row.get::<i64>(9).map_err(storage)?)
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

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        fs,
        os::unix::fs::PermissionsExt,
        sync::{Arc, Mutex},
    };

    use chrono::{Duration, Utc};
    use floe_access::DataAccessGrant;
    use floe_access::{
        ConnectionId, ConnectorId, ContextDependency, DependencyCoverage, ExecutionOwnerId,
        GrantConsumer, GrantDataCategory, GrantOperation, GrantPurpose, GrantScope,
        GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority,
    };
    use floe_agent_contract::AgentFailure;
    use floe_conversation::{
        AgentMessage, CapabilityExecution, CapabilityExecutionState, ProviderReplay,
    };

    use super::super::context_dependencies::merge_context_dependency_coverage;
    use super::*;
    use crate::VaultKeyProvider;

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

    fn source_binding(person_id: floe_kernel::PersonId) -> GrantSourceBinding {
        GrantSourceBinding::try_new(
            person_id,
            ConnectionId::new(),
            ConnectorId::try_new("calendar").unwrap(),
            ExecutionOwnerId::try_new("cleanup-test").unwrap(),
            SourceAuthority::new(),
        )
        .unwrap()
    }

    fn scope() -> GrantScope {
        GrantScope::try_new(
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            vec![GrantOperation::Read],
            vec![GrantPurpose::Scheduling],
            vec![GrantConsumer::builtin("calendar").unwrap()],
            ProcessingRestriction::LocalOnly,
        )
        .unwrap()
    }

    async fn cleanup_fixture(
        root: &std::path::Path,
        person: floe_kernel::PersonId,
        keys: TestKeys,
    ) -> (
        EncryptedAgentVault<TestKeys>,
        DataAccessGrant,
        floe_conversation::AgentSession,
    ) {
        let vault = EncryptedAgentVault::create(root, person, keys)
            .await
            .unwrap();
        let source = source_binding(person);
        let grant = vault
            .create_data_access_grant(source.clone(), scope())
            .await
            .unwrap();
        let active = vault
            .activate_data_access_grant(grant.id(), grant.authority(), source, scope())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        (vault, active, session)
    }

    fn dependency_for(
        person: floe_kernel::PersonId,
        grant: &DataAccessGrant,
        query_fingerprint: Vec<u8>,
    ) -> ContextDependency {
        ContextDependency::try_new(
            person,
            grant.id(),
            grant.authority(),
            source_binding(person),
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Scheduling,
            GrantConsumer::builtin("calendar").unwrap(),
            ProcessingRestriction::LocalOnly,
            floe_access::ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            query_fingerprint,
            Uuid::new_v4(),
            Uuid::new_v4(),
            Utc::now(),
            Utc::now() + Duration::minutes(5),
        )
        .unwrap()
    }

    async fn insert_coverage(
        vault: &EncryptedAgentVault<TestKeys>,
        person: floe_kernel::PersonId,
        session_id: Uuid,
        turn_id: Uuid,
        coverage: DependencyCoverage,
    ) {
        let mut connection = vault.connection().unwrap();
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .unwrap();
        merge_context_dependency_coverage(&transaction, person, session_id, turn_id, coverage)
            .await
            .unwrap();
        vault
            .finish_access_grant_transaction(transaction, Ok(()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn drain_scrubs_exact_invalidated_turn_without_revision_or_history_change() {
        let root = root();
        let person = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        let source = source_binding(person);
        let grant = vault
            .create_data_access_grant(source.clone(), scope())
            .await
            .unwrap();
        let active = vault
            .activate_data_access_grant(grant.id(), grant.authority(), source.clone(), scope())
            .await
            .unwrap();
        let session = vault.create_session().await.unwrap();
        let turn_id = Uuid::new_v4();
        let now = Utc::now();
        let dependency = ContextDependency::try_new(
            person,
            active.id(),
            active.authority(),
            source,
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Scheduling,
            GrantConsumer::builtin("calendar").unwrap(),
            ProcessingRestriction::LocalOnly,
            floe_access::ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"cleanup-query".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now + Duration::minutes(5),
        )
        .unwrap();
        let repeated_payload = String::from_utf8(
            DependencyCoverage::dependent(dependency.clone())
                .unwrap()
                .as_persisted_bytes()
                .unwrap(),
        )
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
            turn_id,
            DependencyCoverage::dependent(dependency).unwrap(),
        )
        .await
        .unwrap();
        vault
            .finish_access_grant_transaction(transaction, Ok(()))
            .await
            .unwrap();
        let mut connection = vault.connection().unwrap();
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .unwrap();
        for _ in 0..1025 {
            transaction
                .execute(
                    "INSERT INTO agent_context_dependency_coverage (person_id, session_id, turn_id, version, payload) VALUES (?, ?, ?, 1, ?)",
                    (
                        person.to_string(),
                        Uuid::new_v4().to_string(),
                        Uuid::new_v4().to_string(),
                        repeated_payload.clone(),
                    ),
                )
                .await
                .unwrap();
        }
        vault
            .finish_access_grant_transaction(transaction, Ok(()))
            .await
            .unwrap();
        let connection = vault.connection().unwrap();
        let mut rows = connection
            .query(
                "SELECT COUNT(*) FROM agent_context_dependency_coverage WHERE person_id = ? AND instr(payload, ?) > 0",
                (person.to_string(), active.id().as_uuid().to_string()),
            )
            .await
            .unwrap();
        assert_eq!(
            rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
            1026
        );

        let source_b = source_binding(person);
        let grant_b = vault
            .create_data_access_grant(source_b.clone(), scope())
            .await
            .unwrap();
        let active_b = vault
            .activate_data_access_grant(
                grant_b.id(),
                grant_b.authority(),
                source_b.clone(),
                scope(),
            )
            .await
            .unwrap();
        let session_b = vault.create_session().await.unwrap();
        let turn_b = Uuid::new_v4();
        let dependency_b = ContextDependency::try_new(
            person,
            active_b.id(),
            active_b.authority(),
            source_b,
            vec![ResourceHandle::try_new("calendar/main").unwrap()],
            vec![GrantDataCategory::Metadata],
            GrantOperation::Read,
            GrantPurpose::Scheduling,
            GrantConsumer::builtin("calendar").unwrap(),
            ProcessingRestriction::LocalOnly,
            floe_access::ConsumerPolicyAuthority::new(),
            Uuid::new_v4(),
            b"cleanup-query-b".to_vec(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now + Duration::minutes(5),
        )
        .unwrap();
        let mut connection = vault.connection().unwrap();
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .unwrap();
        merge_context_dependency_coverage(
            &transaction,
            person,
            session_b.id,
            turn_b,
            DependencyCoverage::dependent(dependency_b).unwrap(),
        )
        .await
        .unwrap();
        vault
            .finish_access_grant_transaction(transaction, Ok(()))
            .await
            .unwrap();
        let replay_b = ProviderReplay {
            call_ids: vec!["call-b".into()],
            preamble: String::new(),
            gateway: "fixture".into(),
            purpose: "test".into(),
            external: false,
            source: "calendar".into(),
            provider_call_id: "provider-b".into(),
            items: serde_json::json!(["opaque"]),
        };
        let mut with_replay_b = vault.load(person, session_b.id).await.unwrap();
        let previous_b = with_replay_b.revision;
        with_replay_b.revision += 1;
        with_replay_b
            .capability_executions
            .push(CapabilityExecution {
                scope_id: session_b.id,
                turn_id: turn_b,
                call_id: Uuid::new_v4(),
                capability_id: "calendar.read".into(),
                input: "bounded".into(),
                state: CapabilityExecutionState::Settled,
                result: Some(Ok("displayed".into())),
                replay: Some(replay_b),
            });
        vault
            .compare_and_swap(&with_replay_b, previous_b)
            .await
            .unwrap();

        let replay = ProviderReplay {
            call_ids: vec!["call-a".into()],
            preamble: String::new(),
            gateway: "fixture".into(),
            purpose: "test".into(),
            external: false,
            source: "calendar".into(),
            provider_call_id: "provider-a".into(),
            items: serde_json::json!(["opaque"]),
        };
        let mut with_replay = vault.load(person, session.id).await.unwrap();
        with_replay.revision += 1;
        with_replay.capability_executions.push(CapabilityExecution {
            scope_id: session.id,
            turn_id,
            call_id: Uuid::new_v4(),
            capability_id: "calendar.read".into(),
            input: "bounded".into(),
            state: CapabilityExecutionState::Settled,
            result: Some(Ok("displayed".into())),
            replay: Some(replay),
        });
        vault
            .compare_and_swap(&with_replay, session.revision)
            .await
            .unwrap();
        let revision_before_cleanup = with_replay.revision;
        let message_count = with_replay.messages.len();
        assert_eq!(
            vault.drain_context_cleanup(0).await,
            Err(AgentFailure::BudgetExceeded)
        );
        assert_eq!(
            vault
                .drain_context_cleanup(MAX_CONTEXT_CLEANUP_BATCH + 1)
                .await,
            Err(AgentFailure::BudgetExceeded)
        );
        vault
            .revoke_data_access_grant(active.id(), active.authority())
            .await
            .unwrap();

        assert_eq!(vault.drain_context_cleanup(16).await.unwrap(), 0);
        let mut late = vault.load(person, session.id).await.unwrap();
        let late_previous = late.revision;
        late.revision += 1;
        late.capability_executions[0].replay = Some(ProviderReplay {
            call_ids: vec!["late".into()],
            preamble: String::new(),
            gateway: "fixture".into(),
            purpose: "test".into(),
            external: false,
            source: "calendar".into(),
            provider_call_id: "late".into(),
            items: serde_json::json!([]),
        });
        vault.compare_and_swap(&late, late_previous).await.unwrap();
        assert!(
            vault
                .load(person, session.id)
                .await
                .unwrap()
                .capability_executions[0]
                .replay
                .is_none()
        );
        assert_eq!(vault.drain_context_cleanup(16).await.unwrap(), 1);
        assert_eq!(vault.drain_context_cleanup(16).await.unwrap(), 0);
        let cleaned = vault.load(person, session.id).await.unwrap();
        assert_eq!(cleaned.revision, revision_before_cleanup + 1);
        assert_eq!(cleaned.messages.len(), message_count);
        assert!(cleaned.capability_executions[0].replay.is_none());
        assert!(
            vault
                .load(person, session_b.id)
                .await
                .unwrap()
                .capability_executions[0]
                .replay
                .is_some()
        );

        drop(vault);
        let vault = EncryptedAgentVault::open(root.path(), person, keys.clone())
            .await
            .unwrap();
        assert_eq!(vault.drain_context_cleanup(16).await.unwrap(), 0);
        let cleaned = vault.load(person, session.id).await.unwrap();
        assert!(cleaned.capability_executions[0].replay.is_none());
        let mut stale = cleaned.clone();
        stale.revision += 1;
        stale.capability_executions[0].replay = Some(ProviderReplay {
            call_ids: vec!["stale".into()],
            preamble: String::new(),
            gateway: "fixture".into(),
            purpose: "test".into(),
            external: false,
            source: "calendar".into(),
            provider_call_id: "stale".into(),
            items: serde_json::json!([]),
        });
        vault
            .compare_and_swap(&stale, cleaned.revision)
            .await
            .unwrap();
        assert!(
            vault
                .load(person, session.id)
                .await
                .unwrap()
                .capability_executions[0]
                .replay
                .is_none()
        );
    }

    #[tokio::test]
    async fn drain_resumes_after_byte_budget_before_row_budget() {
        let root = root();
        let person = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let (vault, active, _) = cleanup_fixture(root.path(), person, keys.clone()).await;
        let mut dependencies = Vec::new();
        for _ in 0..3 {
            dependencies.push(dependency_for(person, &active, vec![b'q'; 2800]));
        }
        let first = dependencies.remove(0);
        let mut coverage = DependencyCoverage::dependent(first).unwrap();
        for dependency in dependencies {
            coverage = coverage
                .merge(&DependencyCoverage::dependent(dependency).unwrap())
                .unwrap();
        }
        let payload = coverage.as_persisted_bytes().unwrap();
        assert!(payload.len() > 4_096);
        assert!(payload.len() <= MAX_CONTEXT_DEPENDENCY_BYTES);
        for _ in 0..300 {
            insert_coverage(
                &vault,
                person,
                Uuid::new_v4(),
                Uuid::new_v4(),
                DependencyCoverage::from_persisted_bytes(&payload).unwrap(),
            )
            .await;
        }
        vault
            .revoke_data_access_grant(active.id(), active.authority())
            .await
            .unwrap();

        assert_eq!(vault.drain_context_cleanup(16).await.unwrap(), 0);
        let connection = vault.connection().unwrap();
        let mut rows = connection
            .query(
                "SELECT coverage_complete, coverage_cursor FROM agent_context_cleanup_applied LIMIT 1",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        assert_eq!(row.get::<i64>(0).unwrap(), 0);
        assert!(row.get::<i64>(1).unwrap() > 0);
        let mut drained = 0;
        for _ in 0..16 {
            drained += vault.drain_context_cleanup(16).await.unwrap();
            if drained == 1 {
                break;
            }
        }
        assert_eq!(drained, 1);
    }

    #[tokio::test]
    async fn cleanup_transaction_rolls_back_scrub_marker_and_ack_on_sql_failure() {
        let root = root();
        let person = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let (vault, active, session) = cleanup_fixture(root.path(), person, keys.clone()).await;
        let turn_id = Uuid::new_v4();
        insert_coverage(
            &vault,
            person,
            session.id,
            turn_id,
            DependencyCoverage::dependent(dependency_for(person, &active, b"cleanup".to_vec()))
                .unwrap(),
        )
        .await;
        let replay = ProviderReplay {
            call_ids: vec!["call".into()],
            preamble: String::new(),
            gateway: "fixture".into(),
            purpose: "test".into(),
            external: false,
            source: "calendar".into(),
            provider_call_id: "provider".into(),
            items: serde_json::json!(["opaque"]),
        };
        let mut with_replay = vault.load(person, session.id).await.unwrap();
        let previous_revision = with_replay.revision;
        with_replay.revision += 1;
        with_replay.capability_executions.push(CapabilityExecution {
            scope_id: session.id,
            turn_id,
            call_id: Uuid::new_v4(),
            capability_id: "calendar.read".into(),
            input: "bounded".into(),
            state: CapabilityExecutionState::Settled,
            result: Some(Ok("displayed".into())),
            replay: Some(replay),
        });
        vault
            .compare_and_swap(&with_replay, previous_revision)
            .await
            .unwrap();
        vault
            .revoke_data_access_grant(active.id(), active.authority())
            .await
            .unwrap();
        let mut connection = vault.connection().unwrap();
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .unwrap();
        transaction
            .execute(
                "CREATE TRIGGER cleanup_test_ack_failure BEFORE DELETE ON data_access_grant_cleanup BEGIN SELECT RAISE(ABORT, 'cleanup test'); END",
                (),
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        assert_eq!(
            vault.drain_context_cleanup(16).await,
            Err(AgentFailure::StorageUnavailable)
        );
        drop(vault);
        let vault = EncryptedAgentVault::open(root.path(), person, keys)
            .await
            .unwrap();
        let restored = vault.load(person, session.id).await.unwrap();
        assert_eq!(restored.revision, with_replay.revision);
        assert!(restored.capability_executions[0].replay.is_some());
        let pending = vault.pending_data_access_grant_cleanup(16).await.unwrap();
        assert_eq!(pending.len(), 1);
        let mut connection = vault.connection().unwrap();
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .unwrap();
        transaction
            .execute("DROP TRIGGER cleanup_test_ack_failure", ())
            .await
            .unwrap();
        transaction.commit().await.unwrap();
    }

    #[tokio::test]
    async fn raw_cas_sanitizes_after_incoming_revoked_coverage_merge() {
        let root = root();
        let person = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let (vault, active, session) = cleanup_fixture(root.path(), person, keys).await;
        let turn_id = Uuid::new_v4();
        let dependency = dependency_for(person, &active, b"revoked".to_vec());
        insert_coverage(
            &vault,
            person,
            session.id,
            turn_id,
            DependencyCoverage::Independent,
        )
        .await;
        let mut candidate = vault.load(person, session.id).await.unwrap();
        let previous_revision = candidate.revision;
        candidate.revision += 1;
        candidate.messages.push(AgentMessage::User {
            turn_id,
            text: "independent history".into(),
        });
        candidate.capability_executions.push(CapabilityExecution {
            scope_id: session.id,
            turn_id,
            call_id: Uuid::new_v4(),
            capability_id: "calendar.read".into(),
            input: "bounded".into(),
            state: CapabilityExecutionState::Settled,
            result: Some(Ok("displayed".into())),
            replay: Some(ProviderReplay {
                call_ids: vec!["call".into()],
                preamble: String::new(),
                gateway: "fixture".into(),
                purpose: "test".into(),
                external: false,
                source: "calendar".into(),
                provider_call_id: "provider".into(),
                items: serde_json::json!(["opaque"]),
            }),
        });
        let mut independent = BTreeMap::new();
        independent.insert(turn_id, DependencyCoverage::Independent);
        vault
            .compare_and_swap_checked(&candidate, previous_revision, &independent)
            .await
            .unwrap();
        vault
            .revoke_data_access_grant(active.id(), active.authority())
            .await
            .unwrap();
        assert_eq!(vault.drain_context_cleanup(16).await.unwrap(), 1);
        let mut candidate = vault.load(person, session.id).await.unwrap();
        let previous_revision = candidate.revision;
        candidate.revision += 1;
        candidate.messages.push(AgentMessage::Assistant {
            turn_id,
            text: "continued".into(),
        });
        let mut revoked = BTreeMap::new();
        revoked.insert(turn_id, DependencyCoverage::dependent(dependency).unwrap());
        vault
            .compare_and_swap_checked(&candidate, previous_revision, &revoked)
            .await
            .unwrap();
        let persisted = vault.load(person, session.id).await.unwrap();
        assert!(
            persisted
                .capability_executions
                .iter()
                .all(|execution| execution.replay.is_none())
        );
    }

    #[tokio::test]
    async fn cleanup_fails_closed_when_key_is_lost() {
        let root = root();
        let person = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let (vault, _, _) = cleanup_fixture(root.path(), person, keys.clone()).await;
        keys.0.lock().unwrap().clear();
        assert_eq!(
            vault.drain_context_cleanup(16).await,
            Err(AgentFailure::VaultUnavailable)
        );
    }

    #[tokio::test]
    async fn reopening_without_context_cleanup_schema_fails_closed() {
        let root = root();
        let person = floe_kernel::PersonId::new();
        let keys = TestKeys::default();
        let vault = EncryptedAgentVault::create(root.path(), person, keys.clone())
            .await
            .unwrap();
        let mut connection = vault.connection().unwrap();
        let transaction = connection
            .transaction_with_behavior(turso::transaction::TransactionBehavior::Immediate)
            .await
            .unwrap();
        transaction
            .execute("DROP TABLE agent_context_cleanup_suppression", ())
            .await
            .unwrap();
        transaction
            .execute("DROP TABLE agent_context_cleanup_applied", ())
            .await
            .unwrap();
        transaction
            .execute("DROP TABLE agent_context_cleanup_schema", ())
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        drop(vault);
        assert!(matches!(
            EncryptedAgentVault::open(root.path(), person, keys).await,
            Err(AgentFailure::VaultUnavailable)
        ));
    }
}
