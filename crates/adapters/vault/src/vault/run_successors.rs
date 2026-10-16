//! Immutable typed Run edges; Work never owns a second execution-state cache.
use super::*;
use floe_conversation::{
    MAX_RUN_LINEAGE_RECORDS, RunRecord, RunSuccessorKind, validate_run_successor,
};
use floe_kernel::RunId;
use turso::{Connection, transaction::TransactionBehavior};

fn parse_run(value: String) -> Result<RunId, AgentFailure> {
    Uuid::parse_str(&value)
        .ok()
        .and_then(RunId::from_uuid)
        .ok_or(AgentFailure::StorageUnavailable)
}
fn kind_name(run: &RunRecord) -> Option<(&'static str, RunId)> {
    if let Some(parent) = run.continuation_of {
        Some(("continue", parent))
    } else {
        run.resume_of.map(|parent| ("resume", parent))
    }
}
impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// A point read verifies this row's index without recursively reading parents.
    pub(super) async fn validate_run_backlink_on(
        &self,
        connection: &Connection,
        run: &RunRecord,
    ) -> Result<(), AgentFailure> {
        let mut rows=connection.query("SELECT origin_run_id, child_command_id, session_id, person_id, kind FROM agent_conversation_run_successors WHERE child_run_id = ?", [run.run_id.as_uuid().to_string()]).await.map_err(database_failure)?;
        let row = rows.next().await.map_err(database_failure)?;
        match (kind_name(run), row) {
            (None, None) => Ok(()),
            (Some((kind, parent)), Some(row)) => {
                if row.get::<String>(0).map_err(storage)? != parent.as_uuid().to_string()
                    || row.get::<String>(1).map_err(storage)?
                        != run.command_id.as_uuid().to_string()
                    || row.get::<String>(2).map_err(storage)? != run.session_id.to_string()
                    || row.get::<String>(3).map_err(storage)? != run.person_id.to_string()
                    || row.get::<String>(4).map_err(storage)? != kind
                    || rows.next().await.map_err(database_failure)?.is_some()
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
                Ok(())
            }
            _ => Err(AgentFailure::StorageUnavailable),
        }
    }
    pub(super) async fn run_successor_on(
        &self,
        connection: &Connection,
        parent: &RunRecord,
    ) -> Result<Option<RunRecord>, AgentFailure> {
        // Match the reference receipt against the canonical payload's indexed
        // parent. A deleted edge cannot silently regress Work to an old root.
        let mut rows = connection.query(
            "SELECT child_run_id, kind FROM agent_conversation_run_successors WHERE origin_run_id = ?",
            [parent.run_id.as_uuid().to_string()],
        ).await.map_err(database_failure)?;
        let edge = match rows.next().await.map_err(database_failure)? {
            None => None,
            Some(row) => {
                let id = parse_run(row.get(0).map_err(storage)?)?;
                let kind = match row.get::<String>(1).map_err(storage)?.as_str() {
                    "continue" => RunSuccessorKind::Continue,
                    "resume" => RunSuccessorKind::Resume,
                    _ => return Err(AgentFailure::StorageUnavailable),
                };
                Some((id, kind))
            }
        };
        if rows.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(rows);
        let mut candidates = connection.query(
            "SELECT run_id FROM agent_conversation_runs WHERE COALESCE(json_extract(payload, '$.continuation_of'), json_extract(payload, '$.resume_of')) = ? LIMIT 2",
            [parent.run_id.as_uuid().to_string()],
        ).await.map_err(database_failure)?;
        let candidate = match candidates.next().await.map_err(database_failure)? {
            None => None,
            Some(row) => Some(parse_run(row.get(0).map_err(storage)?)?),
        };
        if candidates.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(candidates);
        let (id, kind) = match (edge, candidate) {
            (None, None) => return Ok(None),
            (Some((id, kind)), Some(actual)) if id == actual => (id, kind),
            _ => return Err(AgentFailure::StorageUnavailable),
        };
        let child = self
            .conversation_run_on(connection, id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        validate_run_successor(parent, &child, kind)?;
        Ok(Some(child))
    }
    pub(super) async fn current_run_descendant_on(
        &self,
        connection: &Connection,
        root: RunId,
    ) -> Result<RunId, AgentFailure> {
        let mut current = self
            .conversation_run_on(connection, root)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..MAX_RUN_LINEAGE_RECORDS {
            if !seen.insert(current.run_id) {
                return Err(AgentFailure::StorageUnavailable);
            }
            match self.run_successor_on(connection, &current).await? {
                None => return Ok(current.run_id),
                Some(child) => current = child,
            }
        }
        Err(AgentFailure::StorageUnavailable)
    }
    pub(super) async fn validate_run_successor_store(&self) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let mut orphans=tx.query("SELECT 1 FROM agent_conversation_run_successors e LEFT JOIN agent_conversation_runs p ON p.run_id = e.origin_run_id LEFT JOIN agent_conversation_runs c ON c.run_id = e.child_run_id WHERE p.run_id IS NULL OR c.run_id IS NULL LIMIT 1", ()).await.map_err(database_failure)?;
        if orphans.next().await.map_err(database_failure)?.is_some() {
            return Err(AgentFailure::StorageUnavailable);
        }
        drop(orphans);
        let mut after = String::new();
        loop {
            let mut rows=tx.query("SELECT run_id FROM agent_conversation_runs WHERE run_id > ? ORDER BY run_id LIMIT 128", [after.clone()]).await.map_err(database_failure)?;
            let mut ids = vec![];
            while let Some(row) = rows.next().await.map_err(database_failure)? {
                ids.push(row.get::<String>(0).map_err(storage)?);
            }
            drop(rows);
            if ids.is_empty() {
                break;
            }
            for id in &ids {
                let record = self
                    .conversation_run_on(&tx, parse_run(id.clone())?)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if let Some((_, parent)) = kind_name(&record) {
                    let parent = self
                        .conversation_run_on(&tx, parent)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    let child = self
                        .run_successor_on(&tx, &parent)
                        .await?
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    if child != record {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
            }
            after = ids.last().unwrap().clone();
            if ids.len() < 128 {
                break;
            }
        }
        tx.commit().await.map_err(database_failure)?;
        self.check_access()
    }
}
