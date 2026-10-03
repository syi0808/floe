//! Day-owned timeline and calendar-mirror persistence.

use crate::{
    engine::{require_schema, storage_error, unsupported_profile},
    StoreError, StoreErrorCode, TursoStore,
};
use floe_day::{CalendarMirror, Event, Note, Task, TimelineItem};
use floe_kernel::PersonId;
use turso::Connection;

const DAY_TABLES: &[&str] = &["captures", "events", "tasks", "notes", "calendar_mirrors"];

pub(crate) async fn initialize_day_schema(connection: &Connection) -> Result<(), StoreError> {
    super::day_collection::initialize_collection_schema(connection).await?;
    super::day_mutation::initialize_mutation_schema(connection).await?;
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS floe_day_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))",
            (),
        )
        .await
        .map_err(storage_error)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO floe_day_schema(id,version) VALUES (1,1)",
            (),
        )
        .await
        .map_err(storage_error)?;

    for table in DAY_TABLES {
        connection
            .execute(
                &format!("CREATE TABLE IF NOT EXISTS {table} (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)"),
                (),
            )
            .await
            .map_err(storage_error)?;
        connection
            .execute(
                &format!("CREATE INDEX IF NOT EXISTS {table}_person ON {table}(person_id)"),
                (),
            )
            .await
            .map_err(storage_error)?;
    }

    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS day_refreshes (operation_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, command_id TEXT NOT NULL, executor_generation TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), payload TEXT NOT NULL, UNIQUE(person_id, command_id))",
            (),
        )
        .await
        .map_err(storage_error)?;
    connection
        .execute(
            "CREATE INDEX IF NOT EXISTS day_refreshes_person_device_generation ON day_refreshes(person_id, device_id, executor_generation)",
            (),
        )
        .await
        .map_err(storage_error)?;
    connection.execute("CREATE TABLE IF NOT EXISTS day_executors (person_id TEXT NOT NULL, device_id TEXT NOT NULL, executor_generation TEXT NOT NULL, active INTEGER NOT NULL CHECK(active IN (0,1)), PRIMARY KEY(person_id,device_id))", ()).await.map_err(storage_error)?;
    Ok(())
}

pub(crate) async fn validate_day_schema(connection: &Connection) -> Result<(), StoreError> {
    super::day_collection::validate_collection_schema(connection).await?;
    super::day_mutation::validate_mutation_schema(connection).await?;
    require_schema(
        connection,
        "floe_day_schema",
        "CREATE TABLE floe_day_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))",
    )
    .await?;
    let mut rows = connection
        .query("SELECT version FROM floe_day_schema WHERE id = 1", ())
        .await
        .map_err(|_| unsupported_profile())?;
    let row = rows
        .next()
        .await
        .map_err(storage_error)?
        .ok_or_else(unsupported_profile)?;
    if row.get::<i64>(0).map_err(storage_error)? != 1
        || rows.next().await.map_err(storage_error)?.is_some()
    {
        return Err(unsupported_profile());
    }
    drop(rows);

    for table in DAY_TABLES {
        require_schema(
            connection,
            table,
            &format!(
                "CREATE TABLE {table} (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)"
            ),
        )
        .await?;
        require_schema(
            connection,
            &format!("{table}_person"),
            &format!("CREATE INDEX {table}_person ON {table}(person_id)"),
        )
        .await?;
    }
    require_schema(
        connection,
        "day_refreshes",
        "CREATE TABLE day_refreshes (operation_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, command_id TEXT NOT NULL, executor_generation TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), payload TEXT NOT NULL, UNIQUE(person_id, command_id))",
    )
    .await?;
    require_schema(
        connection,
        "day_refreshes_person_device_generation",
        "CREATE INDEX day_refreshes_person_device_generation ON day_refreshes(person_id, device_id, executor_generation)",
    )
    .await?;
    require_schema(connection, "day_executors", "CREATE TABLE day_executors (person_id TEXT NOT NULL, device_id TEXT NOT NULL, executor_generation TEXT NOT NULL, active INTEGER NOT NULL CHECK(active IN (0,1)), PRIMARY KEY(person_id,device_id))").await?;
    Ok(())
}

impl TursoStore {
    pub async fn calendar_mirror(&self, person_id: PersonId) -> Result<Option<CalendarMirror>, StoreError> {
        let connection = self.connection().await?;
        super::day_refresh::mirror_on(&connection, person_id).await.map(|value| value.map(|(mirror, _)| mirror)).map_err(|error| StoreError::new(StoreErrorCode::Storage, error.to_string()))
    }
}
impl floe_day::DayRepository for TursoStore {
    fn mutate<'a>(&'a self, command: floe_day::DayMutationCommand, fence: &'a floe_day::DayWriteFence) -> floe_execution::BoxFuture<'a, Result<floe_day::DayMutationResult, floe_day::DayError>> { Box::pin(async move { super::day_mutation::mutate(self, command, fence).await }) }
    fn collect_action<'a>(&'a self, commit: floe_day::DayCollectionCommit, fence: &'a floe_day::DayWriteFence) -> floe_execution::BoxFuture<'a, Result<floe_day::DayCollectionReceipt, floe_day::DayError>> { Box::pin(async move { super::day_collection::collect(self, commit, fence).await }) }
    fn read_items<'a>(&'a self, query: floe_day::DayReadQuery) -> floe_execution::BoxFuture<'a, Result<Vec<TimelineItem>, floe_day::DayError>> { Box::pin(async move { read_items(self, query).await }) }
    fn calendar_mirror<'a>(&'a self, person_id: PersonId) -> floe_execution::BoxFuture<'a, Result<Option<CalendarMirror>, floe_day::DayError>> { Box::pin(async move { TursoStore::calendar_mirror(self, person_id).await.map_err(day_error) }) }
}
fn day_error(error: StoreError) -> floe_day::DayError {
    let code = match error.code { StoreErrorCode::Validation => floe_day::DayErrorCode::Validation, StoreErrorCode::NotFound => floe_day::DayErrorCode::NotFound, StoreErrorCode::Conflict => floe_day::DayErrorCode::Conflict, _ => floe_day::DayErrorCode::Storage };
    floe_day::DayError { code, message: error.message, metadata: error.metadata }
}

// SQL only preselects a conservative superset of the owner query. Fractional
// UTC timestamps sharing a boundary second are admitted here and decided by
// Day's exact predicate after decoding. Unrelated history never enters a Vec.
async fn read_items(store: &TursoStore, query: floe_day::DayReadQuery) -> Result<Vec<TimelineItem>, floe_day::DayError> {
    query.validate()?;
    let connection = store.connection().await.map_err(day_error)?;
    connection.execute("BEGIN", ()).await.map_err(|error| floe_day::DayError::storage(error.to_string()))?;
    let result = read_items_on(&connection, &query).await;
    super::day_refresh::finish_transaction(&connection, result).await
}
pub(super) async fn read_items_on(connection: &Connection, query: &floe_day::DayReadQuery) -> Result<Vec<TimelineItem>, floe_day::DayError> {
    use floe_day::DayReadSelection;
    query.validate()?;
        let mut values = Vec::new(); let mut bytes = 0usize;
        let tables: &[&str] = match &query.selection { DayReadSelection::DisplayDay { .. } => &["events", "tasks", "notes"], DayReadSelection::ActionWindow { .. } => &["events"], DayReadSelection::OpenTasks => &["tasks"], DayReadSelection::CurrentNotes => &["notes"] };
        for table in tables {
            let mut parameters: Vec<turso::Value> = vec![query.person_id.to_string().into()];
            let predicate = match (&query.selection, *table) {
                (DayReadSelection::OpenTasks, "tasks") => "json_extract(payload,'$.completed_at') IS NULL".to_owned(),
                (DayReadSelection::CurrentNotes, "notes") => "1=1".to_owned(),
                (DayReadSelection::DisplayDay { range }, _) => {
                    let (start, end) = floe_day::range_bounds(range).map_err(|_| floe_day::DayError::validation("invalid Day range"))?;
                    parameters.push(start.format("%Y-%m-%dT%H:%M:%S").to_string().into());
                    parameters.push(end.format("%Y-%m-%dT%H:%M:%S").to_string().into());
                    match *table {
                        "events" => {
                            parameters.push(range.start_date.to_string().into()); parameters.push(range.end_date_exclusive.to_string().into());
                            "((json_extract(payload,'$.schedule.kind')='timed' AND substr(json_extract(payload,'$.schedule.ends_at'),1,19)>=?2 AND substr(json_extract(payload,'$.schedule.starts_at'),1,19)<=?3) OR (json_extract(payload,'$.schedule.kind')='all_day' AND json_extract(payload,'$.schedule.start_date')<?5 AND json_extract(payload,'$.schedule.end_date_exclusive')>?4))".to_owned()
                        }
                        "tasks" => "substr(json_extract(payload,'$.created_at'),1,19)<=?3 AND (json_extract(payload,'$.completed_at') IS NULL OR substr(json_extract(payload,'$.completed_at'),1,19)>=?2)".to_owned(),
                        "notes" => "substr(json_extract(payload,'$.created_at'),1,19)>=?2 AND substr(json_extract(payload,'$.created_at'),1,19)<=?3".to_owned(),
                        _ => return Err(floe_day::DayError::storage("unknown Day record kind")),
                    }
                }
                (DayReadSelection::ActionWindow { starts_at, ends_at }, "events") => {
                    parameters.push(starts_at.format("%Y-%m-%dT%H:%M:%S").to_string().into()); parameters.push(ends_at.format("%Y-%m-%dT%H:%M:%S").to_string().into());
                    let start_date = starts_at.date_naive().pred_opt().ok_or_else(|| floe_day::DayError::validation("invalid Action range"))?;
                    let end_date = ends_at.date_naive().succ_opt().ok_or_else(|| floe_day::DayError::validation("invalid Action range"))?;
                    parameters.push(start_date.to_string().into()); parameters.push(end_date.to_string().into());
                    "((json_extract(payload,'$.schedule.kind')='timed' AND substr(json_extract(payload,'$.schedule.ends_at'),1,19)>=?2 AND substr(json_extract(payload,'$.schedule.starts_at'),1,19)<=?3) OR (json_extract(payload,'$.schedule.kind')='all_day' AND json_extract(payload,'$.schedule.start_date')<=?5 AND json_extract(payload,'$.schedule.end_date_exclusive')>=?4))".to_owned()
                }
                _ => return Err(floe_day::DayError::storage("invalid Day selection kind")),
            };
            let mut rows = connection.query(&format!("SELECT id,person_id,payload FROM {table} WHERE person_id=?1 AND json_extract(payload,'$.deleted_at') IS NULL AND ({predicate}) ORDER BY id"), parameters).await.map_err(|error| floe_day::DayError::storage(error.to_string()))?;
            while let Some(row) = rows.next().await.map_err(|error| floe_day::DayError::storage(error.to_string()))? {
                let payload: String = row.get(2).map_err(|error| floe_day::DayError::storage(error.to_string()))?;
                if payload.len() > floe_day::MAX_DAY_SNAPSHOT_BYTES { return Err(floe_day::DayError::budget("selected Day record byte budget")); }
                let item = match *table { "events" => TimelineItem::Event(serde_json::from_str::<Event>(&payload).map_err(|error| floe_day::DayError::storage(error.to_string()))?), "tasks" => TimelineItem::Task(serde_json::from_str::<Task>(&payload).map_err(|error| floe_day::DayError::storage(error.to_string()))?), "notes" => TimelineItem::Note(serde_json::from_str::<Note>(&payload).map_err(|error| floe_day::DayError::storage(error.to_string()))?), _ => return Err(floe_day::DayError::storage("unknown Day record kind")) };
                let (id, person) = match &item { TimelineItem::Event(value) => (value.id.to_string(), value.person_id), TimelineItem::Task(value) => (value.id.to_string(), value.person_id), TimelineItem::Note(value) => (value.id.to_string(), value.person_id) };
                if row.get::<String>(0).map_err(|error| floe_day::DayError::storage(error.to_string()))? != id || row.get::<String>(1).map_err(|error| floe_day::DayError::storage(error.to_string()))? != person.to_string() { return Err(floe_day::DayError::storage("Day physical identity mismatch")); }
                if !query.selects(&item)? { continue; }
                if values.len() >= query.max_items { return Err(floe_day::DayError::budget("Day selected item budget")); }
                bytes = bytes.checked_add(query.item_bytes(&item)?).filter(|count| *count <= query.max_bytes).ok_or_else(|| floe_day::DayError::budget("Day selected byte budget"))?;
                values.push(item);
            }
        }
        Ok(values)
}
