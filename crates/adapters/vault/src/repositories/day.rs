//! Day-owned timeline and calendar-mirror persistence.

use crate::{StoreError, StoreErrorCode, TursoStore};
use floe_day::{CalendarMirror, Event, Note, Task, TimelineItem};
use floe_kernel::{CommandFailure, PersonId};
use turso::Connection;

impl TursoStore {
    pub async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarMirror>, StoreError> {
        let connection = self.connection().await?;
        super::day_refresh::mirror_on(&connection, person_id)
            .await
            .map(|value| value.map(|(mirror, _)| mirror))
            .map_err(|error| StoreError::new(StoreErrorCode::Storage, error.to_string()))
    }
}
impl floe_day::DayRepository for TursoStore {
    fn mutate<'a>(
        &'a self,
        command: floe_day::DayMutationCommand,
        fence: &'a floe_day::DayWriteFence,
    ) -> floe_execution::BoxFuture<
        'a,
        Result<floe_day::DayMutationResult, CommandFailure<floe_day::DayError>>,
    > {
        Box::pin(async move { super::day_mutation::mutate(self, command, fence).await })
    }
    fn collect_action<'a>(
        &'a self,
        commit: floe_day::DayCollectionCommit,
        fence: &'a floe_day::DayWriteFence,
    ) -> floe_execution::BoxFuture<'a, Result<floe_day::DayCollectionReceipt, floe_day::DayError>>
    {
        Box::pin(async move { super::day_collection::collect(self, commit, fence).await })
    }
    fn read_items<'a>(
        &'a self,
        query: floe_day::DayReadQuery,
    ) -> floe_execution::BoxFuture<'a, Result<Vec<TimelineItem>, floe_day::DayError>> {
        Box::pin(async move { read_items(self, query).await })
    }
    fn calendar_mirror<'a>(
        &'a self,
        person_id: PersonId,
    ) -> floe_execution::BoxFuture<'a, Result<Option<CalendarMirror>, floe_day::DayError>> {
        Box::pin(async move {
            TursoStore::calendar_mirror(self, person_id)
                .await
                .map_err(day_error)
        })
    }
}
fn day_error(error: StoreError) -> floe_day::DayError {
    let code = match error.code {
        StoreErrorCode::Validation => floe_day::DayErrorCode::Validation,
        StoreErrorCode::NotFound => floe_day::DayErrorCode::NotFound,
        StoreErrorCode::Conflict => floe_day::DayErrorCode::Conflict,
        _ => floe_day::DayErrorCode::Storage,
    };
    floe_day::DayError {
        code,
        message: error.message,
        metadata: error.metadata,
    }
}

// SQL only preselects a conservative superset of the owner query. Fractional
// UTC timestamps sharing a boundary second are admitted here and decided by
// Day's exact predicate after decoding. Unrelated history never enters a Vec.
async fn read_items(
    store: &TursoStore,
    query: floe_day::DayReadQuery,
) -> Result<Vec<TimelineItem>, floe_day::DayError> {
    query.validate()?;
    let connection = store.connection().await.map_err(day_error)?;
    connection
        .execute("BEGIN", ())
        .await
        .map_err(|error| floe_day::DayError::storage(error.to_string()))?;
    let result = read_items_on(&connection, &query).await;
    super::day_refresh::finish_transaction(&connection, result).await
}
pub(super) async fn read_items_on(
    connection: &Connection,
    query: &floe_day::DayReadQuery,
) -> Result<Vec<TimelineItem>, floe_day::DayError> {
    read_items_on_with_decoder(connection, query, decode_item).await
}

fn decode_item(table: &str, payload: &str) -> Result<TimelineItem, floe_day::DayError> {
    match table {
        "events" => serde_json::from_str::<Event>(payload)
            .map(TimelineItem::Event)
            .map_err(|error| floe_day::DayError::storage(error.to_string())),
        "tasks" => serde_json::from_str::<Task>(payload)
            .map(TimelineItem::Task)
            .map_err(|error| floe_day::DayError::storage(error.to_string())),
        "notes" => serde_json::from_str::<Note>(payload)
            .map(TimelineItem::Note)
            .map_err(|error| floe_day::DayError::storage(error.to_string())),
        _ => Err(floe_day::DayError::storage("unknown Day record kind")),
    }
}

async fn read_items_on_with_decoder<F>(
    connection: &Connection,
    query: &floe_day::DayReadQuery,
    mut decoder: F,
) -> Result<Vec<TimelineItem>, floe_day::DayError>
where
    F: FnMut(&str, &str) -> Result<TimelineItem, floe_day::DayError>,
{
    use floe_day::DayReadSelection;
    query.validate()?;
    let mut values = Vec::new();
    let mut bytes = 0usize;
    let mut candidate_items = 0usize;
    let mut serialized_payload_bytes = 0usize;
    let tables: &[&str] = match &query.selection {
        DayReadSelection::DisplayDay { .. } => &["events", "tasks", "notes"],
        DayReadSelection::ActionWindow { .. } => &["events"],
        DayReadSelection::OpenTasks => &["tasks"],
        DayReadSelection::CurrentNotes => &["notes"],
    };
    for table in tables {
        let mut parameters: Vec<turso::Value> = vec![query.person_id.to_string().into()];
        let predicate = match (&query.selection, *table) {
            (DayReadSelection::OpenTasks, "tasks") => {
                "json_extract(payload,'$.completed_at') IS NULL".to_owned()
            }
            (DayReadSelection::CurrentNotes, "notes") => "1=1".to_owned(),
            (DayReadSelection::DisplayDay { range }, _) => {
                let (start, end) = floe_day::range_bounds(range)
                    .map_err(|_| floe_day::DayError::validation("invalid Day range"))?;
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
                parameters.push(starts_at.format("%Y-%m-%dT%H:%M:%S").to_string().into());
                parameters.push(ends_at.format("%Y-%m-%dT%H:%M:%S").to_string().into());
                let start_date = starts_at
                    .date_naive()
                    .pred_opt()
                    .ok_or_else(|| floe_day::DayError::validation("invalid Action range"))?;
                let end_date = ends_at
                    .date_naive()
                    .succ_opt()
                    .ok_or_else(|| floe_day::DayError::validation("invalid Action range"))?;
                parameters.push(start_date.to_string().into());
                parameters.push(end_date.to_string().into());
                "((json_extract(payload,'$.schedule.kind')='timed' AND substr(json_extract(payload,'$.schedule.ends_at'),1,19)>=?2 AND substr(json_extract(payload,'$.schedule.starts_at'),1,19)<=?3) OR (json_extract(payload,'$.schedule.kind')='all_day' AND json_extract(payload,'$.schedule.start_date')<=?5 AND json_extract(payload,'$.schedule.end_date_exclusive')>=?4))".to_owned()
            }
            _ => return Err(floe_day::DayError::storage("invalid Day selection kind")),
        };
        // Acquisition budgets are used only for the exact OpenTasks and
        // CurrentNotes predicates validated by DayReadQuery. Fetching at most
        // one row beyond the cap lets us fail closed before decoding that row.
        let row_limit = query.acquisition.map_or_else(String::new, |budget| {
            format!(" LIMIT {}", budget.max_candidate_items().saturating_add(1))
        });
        let mut rows = connection.query(&format!("SELECT id,person_id,payload FROM {table} WHERE person_id=?1 AND json_extract(payload,'$.deleted_at') IS NULL AND ({predicate}) ORDER BY id{row_limit}"), parameters).await.map_err(|error| floe_day::DayError::storage(error.to_string()))?;
        while let Some(row) = rows
            .next()
            .await
            .map_err(|error| floe_day::DayError::storage(error.to_string()))?
        {
            // Count physical candidates before reading or deserializing payload.
            // An overflow returns no evidence; the extra row is not asserted to
            // be valid because its payload is intentionally not decoded.
            if let Some(budget) = query.acquisition {
                candidate_items = candidate_items
                    .checked_add(1)
                    .filter(|count| *count <= budget.max_candidate_items())
                    .ok_or_else(|| floe_day::DayError::budget("Day acquisition item budget"))?;
            }
            let payload: String = row
                .get(2)
                .map_err(|error| floe_day::DayError::storage(error.to_string()))?;
            // String::len is the UTF-8 byte length of the stored JSON payload.
            // Enforce the aggregate payload budget before serde hydration.
            if let Some(budget) = query.acquisition {
                serialized_payload_bytes = serialized_payload_bytes
                    .checked_add(payload.len())
                    .filter(|count| *count <= budget.max_serialized_payload_bytes())
                    .ok_or_else(|| {
                        floe_day::DayError::budget("Day acquisition payload byte budget")
                    })?;
            }
            if payload.len() > floe_day::MAX_DAY_SNAPSHOT_BYTES {
                return Err(floe_day::DayError::budget(
                    "selected Day record byte budget",
                ));
            }
            let item = decoder(table, &payload)?;
            let (id, person) = match &item {
                TimelineItem::Event(value) => (value.id.to_string(), value.person_id),
                TimelineItem::Task(value) => (value.id.to_string(), value.person_id),
                TimelineItem::Note(value) => (value.id.to_string(), value.person_id),
            };
            if row
                .get::<String>(0)
                .map_err(|error| floe_day::DayError::storage(error.to_string()))?
                != id
                || row
                    .get::<String>(1)
                    .map_err(|error| floe_day::DayError::storage(error.to_string()))?
                    != person.to_string()
            {
                return Err(floe_day::DayError::storage(
                    "Day physical identity mismatch",
                ));
            }
            if !query.selects(&item)? {
                continue;
            }
            if values.len() >= query.max_items {
                return Err(floe_day::DayError::budget("Day selected item budget"));
            }
            bytes = bytes
                .checked_add(query.item_bytes(&item)?)
                .filter(|count| *count <= query.max_projected_item_bytes)
                .ok_or_else(|| floe_day::DayError::budget("Day selected byte budget"))?;
            values.push(item);
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::read_items_on_with_decoder;
    use floe_day::{
        DayReadAcquisitionBudget, DayReadQuery, DayReadSelection, MAX_DAY_SNAPSHOT_BYTES, Note,
        Priority, SourceRef, Task,
    };
    use floe_kernel::{NoteId, PersonId, TaskId};
    use std::{cell::Cell, rc::Rc};
    use uuid::Uuid;

    async fn memory_db() -> (turso::Database, turso::Connection) {
        let database = turso::Builder::new_local(":memory:").build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE TABLE tasks (id TEXT, person_id TEXT, payload TEXT)",
                (),
            )
            .await
            .unwrap();
        connection
            .execute(
                "CREATE TABLE notes (id TEXT, person_id TEXT, payload TEXT)",
                (),
            )
            .await
            .unwrap();
        (database, connection)
    }

    async fn insert_task(connection: &turso::Connection, task: &Task) {
        connection
            .execute(
                "INSERT INTO tasks(id,person_id,payload) VALUES(?1,?2,?3)",
                (
                    task.id.to_string(),
                    task.person_id.to_string(),
                    serde_json::to_string(task).unwrap(),
                ),
            )
            .await
            .unwrap();
    }

    async fn insert_note(connection: &turso::Connection, note: &Note) {
        connection
            .execute(
                "INSERT INTO notes(id,person_id,payload) VALUES(?1,?2,?3)",
                (
                    note.id.to_string(),
                    note.person_id.to_string(),
                    serde_json::to_string(note).unwrap(),
                ),
            )
            .await
            .unwrap();
    }

    fn task(person_id: PersonId, id: u128) -> Task {
        let now = chrono::DateTime::from_timestamp_millis(1_000).unwrap();
        let mut task = Task::new(
            person_id,
            "task",
            None,
            Priority::Normal,
            SourceRef::Manual,
            now,
        )
        .unwrap();
        task.id = TaskId(Uuid::from_u128(id));
        task
    }

    fn note(person_id: PersonId, id: u128, content: String) -> Note {
        let now = chrono::DateTime::from_timestamp_millis(1_000).unwrap();
        let mut note = Note::new(person_id, content, SourceRef::Manual, now).unwrap();
        note.id = NoteId(Uuid::from_u128(id));
        note
    }

    fn query(
        person_id: PersonId,
        selection: DayReadSelection,
        max_items: usize,
        max_payload_bytes: usize,
    ) -> DayReadQuery {
        DayReadQuery::context_evidence(
            person_id,
            selection,
            DayReadAcquisitionBudget::try_new(max_items, max_payload_bytes).unwrap(),
            MAX_DAY_SNAPSHOT_BYTES,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn candidate_item_overflow_is_rejected_before_decoding_the_extra_payload() {
        let (_database, connection) = memory_db().await;
        let person_id = PersonId::new();
        insert_task(&connection, &task(person_id, 1)).await;
        insert_task(&connection, &task(person_id, 2)).await;
        let query = query(
            person_id,
            DayReadSelection::OpenTasks,
            1,
            MAX_DAY_SNAPSHOT_BYTES,
        );
        let decodes = Rc::new(Cell::new(0));
        let observed_decodes = decodes.clone();

        let result = read_items_on_with_decoder(&connection, &query, move |table, payload| {
            observed_decodes.set(observed_decodes.get() + 1);
            super::decode_item(table, payload)
        })
        .await;

        let error = result.unwrap_err();
        assert_eq!(error.code, floe_day::DayErrorCode::Validation);
        assert_eq!(
            error.metadata.get("reason_code").map(String::as_str),
            Some("budget_exceeded")
        );
        assert_eq!(decodes.get(), 1);
    }

    #[tokio::test]
    async fn aggregate_payload_overflow_is_rejected_before_decoding_the_overflowing_payload() {
        let (_database, connection) = memory_db().await;
        let person_id = PersonId::new();
        insert_note(&connection, &note(person_id, 1, "n".repeat(4_500))).await;
        insert_note(&connection, &note(person_id, 2, "n".repeat(4_500))).await;
        let query = query(person_id, DayReadSelection::CurrentNotes, 2, 8 * 1024);
        let decodes = Rc::new(Cell::new(0));
        let observed_decodes = decodes.clone();

        let result = read_items_on_with_decoder(&connection, &query, move |table, payload| {
            observed_decodes.set(observed_decodes.get() + 1);
            super::decode_item(table, payload)
        })
        .await;

        let error = result.unwrap_err();
        assert_eq!(error.code, floe_day::DayErrorCode::Validation);
        assert_eq!(
            error.metadata.get("reason_code").map(String::as_str),
            Some("budget_exceeded")
        );
        assert_eq!(decodes.get(), 1);
    }
}
