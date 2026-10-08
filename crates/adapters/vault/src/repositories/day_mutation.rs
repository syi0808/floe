//! One physical transaction for a Day-owned manual transition and exact replay.
use super::day_refresh::{finish_command_transaction, mirror_on, require_executor};
use crate::TursoStore;
use floe_day::{
    Capture, DayError, DayMutationCommand, DayMutationPrior, DayMutationResult, DayMutationTarget,
    DomainRef, Event, Note, Task, TimelineItem,
};
use floe_kernel::CommandFailure;
use serde::{Deserialize, Serialize};
use turso::Connection;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    command: DayMutationCommand,
    intent_digest: [u8; 32],
    result: DayMutationResult,
}
fn storage(error: impl std::fmt::Display) -> DayError {
    DayError::storage(error.to_string())
}
fn hex(value: &[u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(super) async fn mutate(
    store: &TursoStore,
    command: DayMutationCommand,
    fence: &floe_day::DayWriteFence,
) -> Result<DayMutationResult, CommandFailure<DayError>> {
    if command.request.command_id.is_nil() {
        return Err(CommandFailure::NotApplied(DayError::validation(
            "invalid Day command identity",
        )));
    }
    let connection = store
        .connection()
        .await
        .map_err(|error| CommandFailure::Indeterminate(storage(error)))?;
    connection
        .execute("BEGIN IMMEDIATE", ())
        .await
        .map_err(|error| CommandFailure::Indeterminate(storage(error)))?;
    let mut prior_command = false;
    let mut replay_checked = false;
    let result = async {
        let person = command.person_id.to_string(); let command_id = command.request.command_id.to_string();
        let mut rows = connection.query("SELECT person_id,command_id,device_id,intent_digest,payload FROM day_mutation_receipts WHERE person_id=? AND command_id=?", (person.clone(), command_id.clone())).await.map_err(storage)?;
        if let Some(row) = rows.next().await.map_err(storage)? {
            // Any row occupying this command id is prior durable evidence, even
            // if the receipt payload itself is corrupt or does not match.
            prior_command = true;
            let payload: String = row.get(4).map_err(storage)?;
            if payload.len() > floe_day::MAX_DAY_MUTATION_RECEIPT_BYTES { return Err(storage("Day receipt byte budget")); }
            let receipt: Receipt = serde_json::from_str(&payload).map_err(storage)?;
            receipt.command.validate_result(&receipt.result)?;
            if receipt.command.intent_digest()? != receipt.intent_digest || row.get::<String>(0).map_err(storage)? != receipt.command.person_id.to_string() || row.get::<String>(1).map_err(storage)? != receipt.command.request.command_id.to_string() || row.get::<String>(2).map_err(storage)? != receipt.command.device_id || row.get::<String>(3).map_err(storage)? != hex(&receipt.intent_digest) || rows.next().await.map_err(storage)?.is_some() { return Err(storage("invalid Day receipt identity")); }
            let incoming_digest = command.intent_digest()?;
            if receipt.command.person_id != command.person_id || receipt.command.device_id != command.device_id || receipt.command.request.command_id != command.request.command_id || receipt.intent_digest != incoming_digest { return Err(DayError::conflict("Day command replay changed device or intent")); }
            return Ok(receipt.result);
        }
        drop(rows);
        let mut other = connection.query("SELECT 1 FROM day_refreshes WHERE person_id=? AND command_id=?", (person.clone(),command_id.clone())).await.map_err(storage)?;
        if other.next().await.map_err(storage)?.is_some() { prior_command = true; return Err(DayError::conflict("Day command kind changed")); }
        drop(other);
        replay_checked = true;
        let intent_digest = command.intent_digest()?;
        fence.check(command.person_id, &command.device_id, command.executor_generation)?;
        require_executor(&connection, command.person_id, &command.device_id, command.executor_generation).await?;
        let mut counts = connection.query("SELECT (SELECT COUNT(*) FROM day_mutation_receipts WHERE person_id=?1)+(SELECT COUNT(*) FROM day_refreshes WHERE person_id=?1)", (person.clone(),)).await.map_err(storage)?;
        let count: i64 = counts.next().await.map_err(storage)?.ok_or_else(|| storage("missing Day receipt count"))?.get(0).map_err(storage)?;
        if count < 0 { return Err(storage("invalid Day receipt count")); }
        if count as usize >= floe_day::MAX_DAY_COMMAND_RECEIPTS { return Err(DayError::budget("Day command receipt capacity reached")); }
        drop(counts);
        let (prior, old_capture, old_item) = match command.target() {
            DayMutationTarget::None => (DayMutationPrior::Absent, None, None),
            DayMutationTarget::Capture(id) => {
                let payload = exact_payload(&connection, "captures", &id.to_string(), &person).await?.ok_or_else(|| DayError::not_found("capture", id))?;
                let value: Capture = serde_json::from_str(&payload).map_err(storage)?;
                (DayMutationPrior::Capture(value), Some(payload), None)
            }
            DayMutationTarget::Item(target) => {
                let (table, id) = match target { DomainRef::Event(id) => ("events", id.to_string()), DomainRef::Task(id) => ("tasks", id.to_string()), DomainRef::Note(id) => ("notes", id.to_string()) };
                let payload = exact_payload(&connection, table, &id, &person).await?.ok_or_else(|| DayError::not_found("Day item", &id))?;
                let item = match target { DomainRef::Event(_) => TimelineItem::Event(serde_json::from_str::<Event>(&payload).map_err(storage)?), DomainRef::Task(_) => TimelineItem::Task(serde_json::from_str::<Task>(&payload).map_err(storage)?), DomainRef::Note(_) => TimelineItem::Note(serde_json::from_str::<Note>(&payload).map_err(storage)?) };
                (DayMutationPrior::Item(item), None, Some(payload))
            }
        };
        let applied = command.apply(prior)?;
        if let Some(value) = &applied.capture { write_record(&connection, "captures", value.id.to_string(), &person, serde_json::to_string(value).map_err(storage)?, old_capture.as_deref(), applied.expected_capture_revision.is_some()).await?; }
        if let Some(item) = &applied.item {
            let (table, id, payload) = match item { TimelineItem::Event(value) => ("events", value.id.to_string(), serde_json::to_string(value)), TimelineItem::Task(value) => ("tasks", value.id.to_string(), serde_json::to_string(value)), TimelineItem::Note(value) => ("notes", value.id.to_string(), serde_json::to_string(value)) };
            write_record(&connection, table, id, &person, payload.map_err(storage)?, old_item.as_deref(), applied.expected_item_revision.is_some()).await?;
        }
        let items = super::day::read_items_on(&connection, &floe_day::DayReadQuery::display(command.person_id, &command.request.day)?).await?;
        let mirror = mirror_on(&connection, command.person_id).await?.map(|(mirror, _)| mirror);
        let result = command.result(&applied, items, mirror)?;
        let payload = serde_json::to_string(&Receipt { command: command.clone(), intent_digest, result: result.clone() }).map_err(storage)?;
        if payload.len() > floe_day::MAX_DAY_MUTATION_RECEIPT_BYTES { return Err(DayError::budget("Day command receipt byte budget")); }
        // The active generation is checked in the same transaction as every
        // changed row and its receipt. Retirement/replacement cannot interleave.
        fence.check(command.person_id, &command.device_id, command.executor_generation)?;
        require_executor(&connection, command.person_id, &command.device_id, command.executor_generation).await?;
        let inserted = connection.execute("INSERT OR IGNORE INTO day_mutation_receipts(person_id,command_id,device_id,intent_digest,payload) VALUES (?,?,?,?,?)", (person,command_id,command.device_id.clone(),hex(&intent_digest),payload)).await.map_err(storage)?;
        if inserted != 1 { return Err(DayError::conflict("Day receipt raced another command")); }
        Ok(result)
    }.await;
    let result = result.and_then(|result| {
        fence.check(
            command.person_id,
            &command.device_id,
            command.executor_generation,
        )?;
        Ok(result)
    });
    let result = result.map_err(|error| {
        if prior_command {
            CommandFailure::Indeterminate(error)
        } else if replay_checked {
            CommandFailure::NotApplied(error)
        } else {
            CommandFailure::Indeterminate(error)
        }
    });
    finish_command_transaction(&connection, result).await
}
async fn exact_payload(
    connection: &Connection,
    table: &str,
    id: &str,
    person: &str,
) -> Result<Option<String>, DayError> {
    let mut rows = connection
        .query(
            &format!("SELECT payload FROM {table} WHERE id=? AND person_id=?"),
            (id, person),
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let payload: String = row.get(0).map_err(storage)?;
    if payload.len() > floe_day::MAX_DAY_SNAPSHOT_BYTES
        || rows.next().await.map_err(storage)?.is_some()
    {
        return Err(storage("invalid Day target payload"));
    }
    Ok(Some(payload))
}
async fn write_record(
    connection: &Connection,
    table: &str,
    id: String,
    person: &str,
    payload: String,
    previous: Option<&str>,
    expects_existing: bool,
) -> Result<(), DayError> {
    if previous.is_some() != expects_existing || payload.len() > floe_day::MAX_DAY_SNAPSHOT_BYTES {
        return Err(DayError::validation("invalid Day physical transition"));
    }
    let changed = if let Some(previous) = previous {
        connection
            .execute(
                &format!("UPDATE {table} SET payload=? WHERE id=? AND person_id=? AND payload=?"),
                (payload, id, person.to_owned(), previous.to_owned()),
            )
            .await
            .map_err(storage)?
    } else {
        connection
            .execute(
                &format!("INSERT OR IGNORE INTO {table}(id,person_id,payload) VALUES (?,?,?)"),
                (id, person.to_owned(), payload),
            )
            .await
            .map_err(storage)?
    };
    if changed != 1 {
        return Err(DayError::conflict("Day target changed"));
    }
    Ok(())
}
