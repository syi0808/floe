use std::path::Path;

use floe_domain::{
    Capture, CaptureId, Event, EventId, Note, NoteId, PersonId, Task, TaskId, TimelineItem,
};
use serde_json::{from_str, to_string};
use turso::{Builder, Connection};

use crate::{CoreError, ErrorCode};

pub struct TursoStore {
    database: turso::Database,
}

impl TursoStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let path = path.as_ref().to_string_lossy();
        let database = Builder::new_local(path.as_ref())
            .build()
            .await
            .map_err(storage_error)?;
        let store = Self { database };
        store.migrate().await?;
        Ok(store)
    }

    async fn connection(&self) -> Result<Connection, CoreError> {
        self.database.connect().map_err(storage_error)
    }

    async fn migrate(&self) -> Result<(), CoreError> {
        let connection = self.connection().await?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY)",
                (),
            )
            .await
            .map_err(storage_error)?;
        for table in ["captures", "events", "tasks", "notes"] {
            connection.execute(
                &format!("CREATE TABLE IF NOT EXISTS {table} (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)"),
                (),
            ).await.map_err(storage_error)?;
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
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (1)",
                (),
            )
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    async fn put<T: serde::Serialize>(
        &self,
        table: &str,
        id: String,
        person_id: PersonId,
        value: &T,
    ) -> Result<(), CoreError> {
        let payload = to_string(value).map_err(storage_error)?;
        self.connection().await?.execute(
            &format!("INSERT INTO {table}(id, person_id, payload) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET person_id=excluded.person_id, payload=excluded.payload"),
            (id, person_id.to_string(), payload),
        ).await.map_err(storage_error)?;
        Ok(())
    }

    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        id: String,
    ) -> Result<Option<T>, CoreError> {
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
        from_str(&payload).map(Some).map_err(storage_error)
    }

    async fn list<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        person_id: PersonId,
    ) -> Result<Vec<T>, CoreError> {
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
        while let Some(row) = rows.next().await.map_err(storage_error)? {
            let payload: String = row.get(0).map_err(storage_error)?;
            values.push(from_str(&payload).map_err(storage_error)?);
        }
        Ok(values)
    }

    pub async fn put_capture(&self, value: &Capture) -> Result<(), CoreError> {
        self.put("captures", value.id.to_string(), value.person_id, value)
            .await
    }
    pub async fn put_event(&self, value: &Event) -> Result<(), CoreError> {
        self.put("events", value.id.to_string(), value.person_id, value)
            .await
    }
    pub async fn put_task(&self, value: &Task) -> Result<(), CoreError> {
        self.put("tasks", value.id.to_string(), value.person_id, value)
            .await
    }
    pub async fn put_note(&self, value: &Note) -> Result<(), CoreError> {
        self.put("notes", value.id.to_string(), value.person_id, value)
            .await
    }
    pub async fn get_capture(&self, id: CaptureId) -> Result<Option<Capture>, CoreError> {
        self.get("captures", id.to_string()).await
    }
    pub async fn get_event(&self, id: EventId) -> Result<Option<Event>, CoreError> {
        self.get("events", id.to_string()).await
    }
    pub async fn get_task(&self, id: TaskId) -> Result<Option<Task>, CoreError> {
        self.get("tasks", id.to_string()).await
    }
    pub async fn get_note(&self, id: NoteId) -> Result<Option<Note>, CoreError> {
        self.get("notes", id.to_string()).await
    }
    pub async fn list_events(&self, person_id: PersonId) -> Result<Vec<Event>, CoreError> {
        self.list("events", person_id).await
    }
    pub async fn list_tasks(&self, person_id: PersonId) -> Result<Vec<Task>, CoreError> {
        self.list("tasks", person_id).await
    }
    pub async fn list_notes(&self, person_id: PersonId) -> Result<Vec<Note>, CoreError> {
        self.list("notes", person_id).await
    }

    pub async fn classify(&self, capture: &Capture, item: &TimelineItem) -> Result<(), CoreError> {
        let (table, id, person_id, payload) = match item {
            TimelineItem::Event(value) => (
                "events",
                value.id.to_string(),
                value.person_id,
                to_string(value),
            ),
            TimelineItem::Task(value) => (
                "tasks",
                value.id.to_string(),
                value.person_id,
                to_string(value),
            ),
            TimelineItem::Note(value) => (
                "notes",
                value.id.to_string(),
                value.person_id,
                to_string(value),
            ),
        };
        if person_id != capture.person_id {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "capture and classified item must belong to the same person",
            ));
        }
        let capture_payload = to_string(capture).map_err(storage_error)?;
        let payload = payload.map_err(storage_error)?;
        let connection = self.connection().await?;
        connection
            .execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(storage_error)?;
        let result = async {
            connection.execute(
                &format!("INSERT INTO {table}(id, person_id, payload) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload"),
                (id, person_id.to_string(), payload),
            ).await.map_err(storage_error)?;
            connection.execute(
                "INSERT INTO captures(id, person_id, payload) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
                (capture.id.to_string(), capture.person_id.to_string(), capture_payload),
            ).await.map_err(storage_error)?;
            connection.execute("COMMIT", ()).await.map_err(storage_error)?;
            Ok::<_, CoreError>(())
        }.await;
        if result.is_err() {
            let _ = connection.execute("ROLLBACK", ()).await;
        }
        result
    }
}

fn storage_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::new(ErrorCode::Storage, error.to_string())
}
