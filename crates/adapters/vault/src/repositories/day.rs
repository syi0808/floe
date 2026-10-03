//! Day-owned timeline and calendar-mirror persistence.

use crate::{
    engine::{require_schema, storage_error, unsupported_profile},
    StoreError, StoreErrorCode, TursoStore,
};
use floe_day::{
    CalendarMirror, Capture, Event, Note, Task, TimelineItem,
};
use floe_kernel::{CaptureId, EventId, NoteId, PersonId, TaskId};
use serde::{de::DeserializeOwned, Serialize};
use turso::Connection;

const DAY_TABLES: &[&str] = &["captures", "events", "tasks", "notes", "calendar_mirrors"];

pub(crate) async fn initialize_day_schema(connection: &Connection) -> Result<(), StoreError> {
    super::day_collection::initialize_collection_schema(connection).await?;
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
    pub async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarMirror>, StoreError> {
        self.get("calendar_mirrors", person_id.to_string()).await
    }

    async fn put<T: Serialize>(
        &self,
        table: &str,
        id: String,
        person_id: PersonId,
        value: &T,
    ) -> Result<(), StoreError> {
        let payload = serde_json::to_string(value).map_err(storage_error)?;
        self.connection()
            .await?
            .execute(
                &format!("INSERT INTO {table}(id, person_id, payload) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET person_id=excluded.person_id, payload=excluded.payload"),
                (id, person_id.to_string(), payload),
            )
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn get<T: DeserializeOwned>(
        &self,
        table: &str,
        id: String,
    ) -> Result<Option<T>, StoreError> {
        let mut rows = self
            .connection()
            .await?
            .query(&format!("SELECT payload FROM {table} WHERE id = ?"), (id,))
            .await
            .map_err(storage_error)?;
        let Some(row) = rows.next().await.map_err(storage_error)? else {
            return Ok(None);
        };
        let payload: String = row.get(0).map_err(storage_error)?;
        if payload.len() > floe_day::MAX_DAY_SNAPSHOT_BYTES { return Err(StoreError::new(crate::StoreErrorCode::Validation, "Day read byte budget")); }
        serde_json::from_str(&payload).map(Some).map_err(storage_error)
    }

    async fn list<T: DeserializeOwned>(
        &self,
        table: &str,
        person_id: PersonId,
    ) -> Result<Vec<T>, StoreError> {
        let mut rows = self
            .connection()
            .await?
            .query(
                &format!("SELECT payload FROM {table} WHERE person_id = ? ORDER BY id"),
                (person_id.to_string(),),
            )
            .await
            .map_err(storage_error)?;
        let mut values = Vec::new();
        let mut bytes = 0usize;
        while let Some(row) = rows.next().await.map_err(storage_error)? {
            if values.len() >= floe_day::MAX_DAY_SNAPSHOT_ITEMS { return Err(StoreError::new(crate::StoreErrorCode::Validation, "Day read item budget")); }
            let payload: String = row.get(0).map_err(storage_error)?;
            bytes = bytes.checked_add(payload.len()).filter(|count| *count <= floe_day::MAX_DAY_SNAPSHOT_BYTES).ok_or_else(|| StoreError::new(crate::StoreErrorCode::Validation, "Day read byte budget"))?;
            values.push(serde_json::from_str(&payload).map_err(storage_error)?);
        }
        Ok(values)
    }

    pub async fn put_capture(&self, value: &Capture) -> Result<(), StoreError> {
        self.put("captures", value.id.to_string(), value.person_id, value)
            .await
    }

    pub async fn put_event(&self, value: &Event) -> Result<(), StoreError> {
        self.put("events", value.id.to_string(), value.person_id, value)
            .await
    }

    pub async fn put_task(&self, value: &Task) -> Result<(), StoreError> {
        self.put("tasks", value.id.to_string(), value.person_id, value)
            .await
    }

    pub async fn put_note(&self, value: &Note) -> Result<(), StoreError> {
        self.put("notes", value.id.to_string(), value.person_id, value)
            .await
    }

    async fn put_if_revision<T, F>(
        &self,
        table: &str,
        id: String,
        person_id: PersonId,
        value: &T,
        expected: floe_kernel::Revision,
        revision: F,
    ) -> Result<(), StoreError>
    where
        T: Serialize + DeserializeOwned,
        F: Fn(&T) -> floe_kernel::Revision,
    {
        let payload = serde_json::to_string(value).map_err(storage_error)?;
        let connection = self.connection().await?;
        connection
            .execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(storage_error)?;
        let result = async {
            let mut rows = connection
                .query(
                    &format!("SELECT payload FROM {table} WHERE id = ? AND person_id = ?"),
                    (id.clone(), person_id.to_string()),
                )
                .await
                .map_err(storage_error)?;
            let stored = rows
                .next()
                .await
                .map_err(storage_error)?
                .map(|row| row.get::<String>(0).map_err(storage_error))
                .transpose()?;
            drop(rows);
            let Some(stored) = stored else {
                return Err(StoreError::new(
                    StoreErrorCode::NotFound,
                    "timeline item not found",
                ));
            };
            let current: T = serde_json::from_str(&stored).map_err(storage_error)?;
            if revision(&current) != expected {
                return Err(
                    StoreError::new(StoreErrorCode::Conflict, "stale revision")
                        .with_metadata("expected", expected.0.to_string())
                        .with_metadata("actual", revision(&current).0.to_string()),
                );
            }
            let changed = connection
                .execute(
                    &format!("UPDATE {table} SET payload = ? WHERE id = ? AND person_id = ? AND payload = ?"),
                    (payload, id, person_id.to_string(), stored),
                )
                .await
                .map_err(storage_error)?;
            if changed != 1 {
                return Err(StoreError::new(
                    StoreErrorCode::Conflict,
                    "timeline item changed; reload and retry",
                ));
            }
            connection
                .execute("COMMIT", ())
                .await
                .map_err(storage_error)?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = connection.execute("ROLLBACK", ()).await;
        }
        result
    }

    pub async fn put_event_if_revision(
        &self,
        value: &Event,
        expected: floe_kernel::Revision,
    ) -> Result<(), StoreError> {
        self.put_if_revision(
            "events",
            value.id.to_string(),
            value.person_id,
            value,
            expected,
            |item| item.revision,
        )
        .await
    }

    pub async fn put_task_if_revision(
        &self,
        value: &Task,
        expected: floe_kernel::Revision,
    ) -> Result<(), StoreError> {
        self.put_if_revision(
            "tasks",
            value.id.to_string(),
            value.person_id,
            value,
            expected,
            |item| item.revision,
        )
        .await
    }

    pub async fn put_note_if_revision(
        &self,
        value: &Note,
        expected: floe_kernel::Revision,
    ) -> Result<(), StoreError> {
        self.put_if_revision(
            "notes",
            value.id.to_string(),
            value.person_id,
            value,
            expected,
            |item| item.revision,
        )
        .await
    }

    pub async fn get_capture(&self, id: CaptureId) -> Result<Option<Capture>, StoreError> {
        self.get("captures", id.to_string()).await
    }

    pub async fn get_event(&self, id: EventId) -> Result<Option<Event>, StoreError> {
        self.get("events", id.to_string()).await
    }

    pub async fn get_task(&self, id: TaskId) -> Result<Option<Task>, StoreError> {
        self.get("tasks", id.to_string()).await
    }

    pub async fn get_note(&self, id: NoteId) -> Result<Option<Note>, StoreError> {
        self.get("notes", id.to_string()).await
    }

    pub async fn list_events(&self, person_id: PersonId) -> Result<Vec<Event>, StoreError> {
        self.list("events", person_id).await
    }

    pub async fn list_tasks(&self, person_id: PersonId) -> Result<Vec<Task>, StoreError> {
        self.list("tasks", person_id).await
    }

    pub async fn list_notes(&self, person_id: PersonId) -> Result<Vec<Note>, StoreError> {
        self.list("notes", person_id).await
    }

    pub async fn classify(
        &self,
        capture: &Capture,
        item: &TimelineItem,
    ) -> Result<(), StoreError> {
        let (table, id, person_id, payload) = match item {
            TimelineItem::Event(value) => (
                "events",
                value.id.to_string(),
                value.person_id,
                serde_json::to_string(value),
            ),
            TimelineItem::Task(value) => (
                "tasks",
                value.id.to_string(),
                value.person_id,
                serde_json::to_string(value),
            ),
            TimelineItem::Note(value) => (
                "notes",
                value.id.to_string(),
                value.person_id,
                serde_json::to_string(value),
            ),
        };
        if person_id != capture.person_id {
            return Err(StoreError::new(
                StoreErrorCode::Validation,
                "capture and classified item must belong to the same person",
            ));
        }
        let capture_payload = serde_json::to_string(capture).map_err(storage_error)?;
        let payload = payload.map_err(storage_error)?;
        let connection = self.connection().await?;
        connection
            .execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(storage_error)?;
        let result = async {
            let mut rows = connection
                .query(
                    "SELECT payload FROM captures WHERE id = ? AND person_id = ?",
                    (capture.id.to_string(), capture.person_id.to_string()),
                )
                .await
                .map_err(storage_error)?;
            let stored = rows
                .next()
                .await
                .map_err(storage_error)?
                .map(|row| row.get::<String>(0).map_err(storage_error))
                .transpose()?;
            drop(rows);
            let Some(stored) = stored else {
                return Err(StoreError::new(
                    StoreErrorCode::NotFound,
                    "capture not found",
                ));
            };
            let current: Capture = serde_json::from_str(&stored).map_err(storage_error)?;
            if current.revision.next() != capture.revision {
                return Err(
                    StoreError::new(StoreErrorCode::Conflict, "stale revision")
                        .with_metadata(
                            "expected",
                            capture.revision.0.saturating_sub(1).to_string(),
                        )
                        .with_metadata("actual", current.revision.0.to_string()),
                );
            }
            if !matches!(current.processing, floe_day::CaptureProcessing::Pending) {
                return Err(StoreError::new(
                    StoreErrorCode::Conflict,
                    "capture has already been resolved",
                ));
            }
            connection
                .execute(
                    &format!("INSERT INTO {table}(id, person_id, payload) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload"),
                    (id, person_id.to_string(), payload),
                )
                .await
                .map_err(storage_error)?;
            connection
                .execute(
                    "INSERT INTO captures(id, person_id, payload) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
                    (capture.id.to_string(), capture.person_id.to_string(), capture_payload),
                )
                .await
                .map_err(storage_error)?;
            connection
                .execute("COMMIT", ())
                .await
                .map_err(storage_error)?;
            Ok::<_, StoreError>(())
        }
        .await;
        if result.is_err() {
            let _ = connection.execute("ROLLBACK", ()).await;
        }
        result
    }
}

impl floe_day::DayRepository for TursoStore {
    fn collect_action<'a>(&'a self, commit: floe_day::DayCollectionCommit) -> floe_execution::BoxFuture<'a, Result<floe_day::DayCollectionReceipt, floe_day::DayError>> {
        Box::pin(async move { super::day_collection::collect(self, commit).await })
    }
    fn put_capture<'a>(
        &'a self,
        value: &'a Capture,
    ) -> floe_execution::BoxFuture<'a, Result<(), floe_day::DayError>> {
        Box::pin(async move {
            TursoStore::put_capture(self, value)
                .await
                .map_err(day_error)
        })
    }

    fn put_event<'a>(
        &'a self,
        value: &'a Event,
    ) -> floe_execution::BoxFuture<'a, Result<(), floe_day::DayError>> {
        Box::pin(async move { TursoStore::put_event(self, value).await.map_err(day_error) })
    }

    fn put_event_if_revision<'a>(
        &'a self,
        value: &'a Event,
        expected: floe_day::Revision,
    ) -> floe_execution::BoxFuture<'a, Result<(), floe_day::DayError>> {
        Box::pin(async move {
            TursoStore::put_event_if_revision(self, value, expected)
                .await
                .map_err(day_error)
        })
    }

    fn put_task<'a>(
        &'a self,
        value: &'a Task,
    ) -> floe_execution::BoxFuture<'a, Result<(), floe_day::DayError>> {
        Box::pin(async move { TursoStore::put_task(self, value).await.map_err(day_error) })
    }

    fn put_task_if_revision<'a>(
        &'a self,
        value: &'a Task,
        expected: floe_day::Revision,
    ) -> floe_execution::BoxFuture<'a, Result<(), floe_day::DayError>> {
        Box::pin(async move {
            TursoStore::put_task_if_revision(self, value, expected)
                .await
                .map_err(day_error)
        })
    }

    fn put_note<'a>(
        &'a self,
        value: &'a Note,
    ) -> floe_execution::BoxFuture<'a, Result<(), floe_day::DayError>> {
        Box::pin(async move { TursoStore::put_note(self, value).await.map_err(day_error) })
    }

    fn put_note_if_revision<'a>(
        &'a self,
        value: &'a Note,
        expected: floe_day::Revision,
    ) -> floe_execution::BoxFuture<'a, Result<(), floe_day::DayError>> {
        Box::pin(async move {
            TursoStore::put_note_if_revision(self, value, expected)
                .await
                .map_err(day_error)
        })
    }

    fn get_capture<'a>(
        &'a self,
        id: CaptureId,
    ) -> floe_execution::BoxFuture<'a, Result<Option<Capture>, floe_day::DayError>> {
        Box::pin(async move { TursoStore::get_capture(self, id).await.map_err(day_error) })
    }

    fn get_event<'a>(
        &'a self,
        id: EventId,
    ) -> floe_execution::BoxFuture<'a, Result<Option<Event>, floe_day::DayError>> {
        Box::pin(async move { TursoStore::get_event(self, id).await.map_err(day_error) })
    }

    fn get_task<'a>(
        &'a self,
        id: TaskId,
    ) -> floe_execution::BoxFuture<'a, Result<Option<Task>, floe_day::DayError>> {
        Box::pin(async move { TursoStore::get_task(self, id).await.map_err(day_error) })
    }

    fn get_note<'a>(
        &'a self,
        id: NoteId,
    ) -> floe_execution::BoxFuture<'a, Result<Option<Note>, floe_day::DayError>> {
        Box::pin(async move { TursoStore::get_note(self, id).await.map_err(day_error) })
    }

    fn list_events<'a>(
        &'a self,
        person_id: PersonId,
    ) -> floe_execution::BoxFuture<'a, Result<Vec<Event>, floe_day::DayError>> {
        Box::pin(async move { TursoStore::list_events(self, person_id).await.map_err(day_error) })
    }

    fn list_tasks<'a>(
        &'a self,
        person_id: PersonId,
    ) -> floe_execution::BoxFuture<'a, Result<Vec<Task>, floe_day::DayError>> {
        Box::pin(async move { TursoStore::list_tasks(self, person_id).await.map_err(day_error) })
    }

    fn list_notes<'a>(
        &'a self,
        person_id: PersonId,
    ) -> floe_execution::BoxFuture<'a, Result<Vec<Note>, floe_day::DayError>> {
        Box::pin(async move { TursoStore::list_notes(self, person_id).await.map_err(day_error) })
    }

    fn classify<'a>(
        &'a self,
        capture: &'a Capture,
        item: &'a TimelineItem,
    ) -> floe_execution::BoxFuture<'a, Result<(), floe_day::DayError>> {
        Box::pin(async move {
            TursoStore::classify(self, capture, item)
                .await
                .map_err(day_error)
        })
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
    let mut result = floe_day::DayError::new(code, error.message);
    for (key, value) in error.metadata {
        result = result.with_metadata(key, value);
    }
    result
}
