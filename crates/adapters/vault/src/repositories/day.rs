//! Day-owned timeline and calendar-mirror persistence.

use crate::{StoreError, StoreErrorCode, TursoStore};
use floe_day::{CalendarMirror, Event, Note, Task, TimelineItem};
use floe_kernel::PersonId;
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
    ) -> floe_execution::BoxFuture<'a, Result<floe_day::DayMutationResult, floe_day::DayError>>
    {
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
    use floe_day::DayReadSelection;
    query.validate()?;
    let mut values = Vec::new();
    let mut bytes = 0usize;
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
        let mut rows = connection.query(&format!("SELECT id,person_id,payload FROM {table} WHERE person_id=?1 AND json_extract(payload,'$.deleted_at') IS NULL AND ({predicate}) ORDER BY id"), parameters).await.map_err(|error| floe_day::DayError::storage(error.to_string()))?;
        while let Some(row) = rows
            .next()
            .await
            .map_err(|error| floe_day::DayError::storage(error.to_string()))?
        {
            let payload: String = row
                .get(2)
                .map_err(|error| floe_day::DayError::storage(error.to_string()))?;
            if payload.len() > floe_day::MAX_DAY_SNAPSHOT_BYTES {
                return Err(floe_day::DayError::budget(
                    "selected Day record byte budget",
                ));
            }
            let item = match *table {
                "events" => TimelineItem::Event(
                    serde_json::from_str::<Event>(&payload)
                        .map_err(|error| floe_day::DayError::storage(error.to_string()))?,
                ),
                "tasks" => TimelineItem::Task(
                    serde_json::from_str::<Task>(&payload)
                        .map_err(|error| floe_day::DayError::storage(error.to_string()))?,
                ),
                "notes" => TimelineItem::Note(
                    serde_json::from_str::<Note>(&payload)
                        .map_err(|error| floe_day::DayError::storage(error.to_string()))?,
                ),
                _ => return Err(floe_day::DayError::storage("unknown Day record kind")),
            };
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
                .filter(|count| *count <= query.max_bytes)
                .ok_or_else(|| floe_day::DayError::budget("Day selected byte budget"))?;
            values.push(item);
        }
    }
    Ok(values)
}
