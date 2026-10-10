//! Exact idempotent projection of a Calendar Operations causal receipt into Day.
use super::day_refresh::{
    current_calendar_sources_on, finish_transaction, mirror_on, persist_mirror_on,
    require_executor, versions_of,
};
use crate::TursoStore;
use floe_day::{DayCollectionCommit, DayCollectionReceipt, DayError, MirrorExpectation};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CollectionRecord {
    command: DayCollectionCommit,
    receipt: DayCollectionReceipt,
}

fn storage(error: impl std::fmt::Display) -> DayError {
    DayError::storage(error.to_string())
}
fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) async fn collect(
    store: &TursoStore,
    commit: DayCollectionCommit,
    fence: &floe_day::DayWriteFence,
) -> Result<DayCollectionReceipt, DayError> {
    commit.validate()?;
    let connection = store.connection().await.map_err(storage)?;
    connection
        .execute("BEGIN IMMEDIATE", ())
        .await
        .map_err(storage)?;
    let result = async {
        let mut rows = connection.query("SELECT person_id,device_id,receipt_digest,intent_digest,payload FROM day_action_collections WHERE execution_id=?", (commit.execution_id.to_string(),)).await.map_err(storage)?;
        if let Some(row) = rows.next().await.map_err(storage)? {
            let payload: String = row.get(4).map_err(storage)?;
            if payload.len() > 64 * 1024 { return Err(storage("invalid Day collection receipt size")); }
            let stored: CollectionRecord = serde_json::from_str(&payload).map_err(storage)?; stored.command.validate()?;
            if row.get::<String>(0).map_err(storage)? != stored.command.person_id.to_string() || row.get::<String>(1).map_err(storage)? != stored.command.device_id || row.get::<String>(2).map_err(storage)? != hex(&stored.command.receipt_digest) || row.get::<String>(3).map_err(storage)? != hex(&stored.command.intent_digest) || stored.receipt.execution_id != stored.command.execution_id || stored.receipt.receipt_digest != stored.command.receipt_digest || stored.receipt.day_projection_ref != format!("day.collection:{}", stored.command.execution_id) { return Err(storage("invalid Day collection receipt binding")); }
            if stored.command.execution_id != commit.execution_id || stored.command.person_id != commit.person_id || stored.command.device_id != commit.device_id || stored.command.receipt_digest != commit.receipt_digest || stored.command.intent_digest != commit.intent_digest || stored.command.collection != commit.collection { return Err(DayError::conflict("Day collection replay changed intent")); }
            return Ok(stored.receipt);
        }
        drop(rows);
        fence.check(commit.person_id, &commit.device_id, commit.executor_generation)?;
        require_executor(&connection, commit.person_id, &commit.device_id, commit.executor_generation).await?;
        let sources = current_calendar_sources_on(&connection, commit.person_id).await?;
        let target = sources.iter().find(|source| source.version.source.connection_id() == commit.collection.source().connection_id).ok_or_else(|| DayError::conflict("Calendar collection source missing"))?;
        if !target.source.is_serving() { return Err(DayError::conflict("Calendar collection source unavailable")); }
        let mut fences = connection.query("SELECT person_id FROM source_operations WHERE connection_id=? AND fence=1", (commit.collection.source().connection_id.as_str(),)).await.map_err(storage)?;
        if fences.next().await.map_err(storage)?.is_some() { return Err(DayError::conflict("Calendar collection source is fenced")); }
        drop(fences);
        let current = mirror_on(&connection, commit.person_id).await?;
        let expectation = MirrorExpectation::of(current.as_ref().map(|(mirror, _)| mirror))?;
        let (mirror, receipt) = commit.apply(current.as_ref().map(|(mirror, _)| mirror), &versions_of(&sources))?;
        let payload = serde_json::to_string(&CollectionRecord { command: commit.clone(), receipt: receipt.clone() }).map_err(storage)?;
        if payload.len() > 64 * 1024 { return Err(DayError::validation("Calendar collection receipt budget")); }
        persist_mirror_on(&connection, commit.person_id, expectation, current, &mirror, serde_json::to_string(&mirror).map_err(storage)?).await?;
        let changed = connection.execute("INSERT OR IGNORE INTO day_action_collections(execution_id,person_id,device_id,receipt_digest,intent_digest,payload) VALUES (?,?,?,?,?,?)", (commit.execution_id.to_string(),commit.person_id.to_string(),commit.device_id.clone(),hex(&commit.receipt_digest),hex(&commit.intent_digest),payload)).await.map_err(storage)?;
        if changed != 1 { return Err(DayError::conflict("Day collection receipt raced")); }
        Ok(receipt)
    }.await;
    let result = result.and_then(|result| {
        fence.check(
            commit.person_id,
            &commit.device_id,
            commit.executor_generation,
        )?;
        Ok(result)
    });
    finish_transaction(&connection, result).await
}
