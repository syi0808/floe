use std::{io::Read, path::Path, sync::Arc};
use turso::core::{Clock, IO};

use floe_day::{Capture, Event, Note, Task, TimelineItem};
use floe_kernel::{CaptureId, EventId, NoteId, PersonId, TaskId};
use serde_json::{from_str, to_string};
use turso::{Builder, Connection};

use crate::{StoreError, StoreErrorCode};

pub struct TursoStore {
    database: turso::Database,
}

impl TursoStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_string_lossy();
        let database = Builder::new_local(path.as_ref())
            .build()
            .await
            .map_err(storage_error)?;
        let store = Self { database };
        store.initialize().await?;
        Ok(store)
    }

    /// Open an explicitly selected, already supported profile. The main file
    /// is opened without Create and retained by the IO adapter through both
    /// validation and ordinary use; no missing-file race can create a profile.
    pub async fn open_existing(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path=std::fs::canonicalize(path.as_ref()).map_err(|_|StoreError::new(StoreErrorCode::NotFound,"selected profile database is missing"))?;
        let mut probe=std::fs::File::open(&path).map_err(storage_error)?;
        if !probe.metadata().map_err(storage_error)?.is_file(){return Err(unsupported_profile())}
        let mut header=[0u8;16];probe.read_exact(&mut header).map_err(|_|unsupported_profile())?;
        if &header!=b"SQLite format 3\0"{return Err(unsupported_profile())}
        let path=path.to_str().ok_or_else(unsupported_profile)?.to_owned();
        let io=Arc::new(ExistingProfileIo::new(path.clone()).map_err(storage_error)?);
        let readonly=Self{database:Builder::new_local(&path).with_io_impl(io.clone()).read_only(true).build().await.map_err(storage_error)?};
        readonly.validate_existing().await?;
        drop(readonly);
        let store=Self{database:Builder::new_local(&path).with_io_impl(io).build().await.map_err(storage_error)?};
        // Recheck the same pinned file after the writable engine is opened.
        store.validate_existing().await?;
        Ok(store)
    }
    async fn validate_existing(&self)->Result<(),StoreError>{
        let connection=self.connection().await?;
        let mut rows=connection.query("SELECT version FROM floe_source_schema WHERE id = 1",()).await.map_err(|_|unsupported_profile())?;
        let row=rows.next().await.map_err(storage_error)?.ok_or_else(unsupported_profile)?;
        if row.get::<i64>(0).map_err(storage_error)?!=1||rows.next().await.map_err(storage_error)?.is_some(){return Err(unsupported_profile())}
        drop(rows);
        for table in ["captures","events","tasks","notes","calendar_mirrors","calendar_actions","action_authorities"]{
            require_schema(&connection,table,&format!("CREATE TABLE {table} (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, payload TEXT NOT NULL)")).await?;
            require_schema(&connection,&format!("{table}_person"),&format!("CREATE INDEX {table}_person ON {table}(person_id)")).await?;
        }
        require_schema(&connection,"floe_source_schema","CREATE TABLE floe_source_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))").await?;
        require_schema(&connection,"source_connections","CREATE TABLE source_connections (connection_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, connector_id TEXT NOT NULL, revision INTEGER NOT NULL, payload TEXT NOT NULL)").await?;
        require_schema(&connection,"source_connections_person_connector","CREATE INDEX source_connections_person_connector ON source_connections(person_id, connector_id)").await?;
        require_schema(&connection,"source_operations","CREATE TABLE source_operations (operation_id TEXT PRIMARY KEY, command_id TEXT NOT NULL, person_id TEXT NOT NULL, connection_id TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision > 0), fence INTEGER NOT NULL CHECK(fence IN (0,1)), payload TEXT NOT NULL, UNIQUE(person_id,command_id))").await?;
        require_schema(&connection,"source_operation_fence","CREATE UNIQUE INDEX source_operation_fence ON source_operations(connection_id) WHERE fence = 1").await?;
        Ok(())
    }

    pub(crate) async fn connection(&self) -> Result<Connection, StoreError> {
        self.database.connect().map_err(storage_error)
    }

    async fn initialize(&self) -> Result<(), StoreError> {
        let connection = self.connection().await?;
        connection.execute("CREATE TABLE IF NOT EXISTS floe_source_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))",()).await.map_err(storage_error)?;
        connection.execute("INSERT OR IGNORE INTO floe_source_schema(id,version) VALUES (1,1)",()).await.map_err(storage_error)?;
        for table in [
            "captures",
            "events",
            "tasks",
            "notes",
            "calendar_mirrors",
            "calendar_actions",
            "action_authorities",
        ] {
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
        connection.execute(
            "CREATE TABLE IF NOT EXISTS source_connections (connection_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, connector_id TEXT NOT NULL, revision INTEGER NOT NULL, payload TEXT NOT NULL)",
            (),
        ).await.map_err(storage_error)?;
        connection.execute(
            "CREATE INDEX IF NOT EXISTS source_connections_person_connector ON source_connections(person_id, connector_id)",
            (),
        ).await.map_err(storage_error)?;
        crate::repositories::initialize_source_operations(&connection)
            .await
            .map_err(storage_error)?;
        Ok(())
    }

    pub async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<floe_day::CalendarMirror>, StoreError> {
        self.get("calendar_mirrors", person_id.to_string()).await
    }

    pub async fn bounded_calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<floe_day::CalendarMirror, floe_agent_contract::AgentFailure> {
        use floe_agent_contract::AgentFailure;
        let connection = self
            .connection()
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let mut rows = connection.query(
            "SELECT length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= 4194304 THEN payload ELSE NULL END FROM calendar_mirrors WHERE id = ? AND person_id = ?",
            (person_id.to_string(), person_id.to_string()),
        ).await.map_err(|_| AgentFailure::StorageUnavailable)?;
        let row = rows
            .next()
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .ok_or(AgentFailure::CapabilityUnavailable)?;
        if row
            .get::<i64>(0)
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            > 4_194_304
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        serde_json::from_str(
            &row.get::<String>(1)
                .map_err(|_| AgentFailure::CapabilityUnavailable)?,
        )
        .map_err(|_| AgentFailure::CapabilityUnavailable)
    }

    pub async fn put_calendar_mirror(
        &self,
        person_id: PersonId,
        mirror: &floe_day::CalendarMirror,
        previous: Option<&floe_day::CalendarMirror>,
    ) -> Result<(), StoreError> {
        let payload = to_string(mirror).map_err(storage_error)?;
        let connection = self.connection().await?;
        let changed = if let Some(previous) = previous {
            let mut rows = connection
                .query(
                    "SELECT payload FROM calendar_mirrors WHERE id = ? AND person_id = ?",
                    (person_id.to_string(), person_id.to_string()),
                )
                .await
                .map_err(storage_error)?;
            let stored = match rows.next().await.map_err(storage_error)? {
                Some(row) => row.get::<String>(0).map_err(storage_error)?,
                None => {
                    return Err(StoreError::new(
                        StoreErrorCode::Conflict,
                        "calendar changed during sync; reload and retry",
                    ));
                }
            };
            drop(rows);
            let expected: floe_day::CalendarMirror = from_str(&stored).map_err(storage_error)?;
            if &expected != previous {
                return Err(StoreError::new(
                    StoreErrorCode::Conflict,
                    "calendar changed during sync; reload and retry",
                ));
            }
            connection.execute(
                "UPDATE calendar_mirrors SET payload = ? WHERE id = ? AND person_id = ? AND payload = ?",
                (payload, person_id.to_string(), person_id.to_string(), stored),
            ).await.map_err(storage_error)?
        } else {
            connection.execute(
                "INSERT OR IGNORE INTO calendar_mirrors(id, person_id, payload) VALUES (?, ?, ?)",
                (person_id.to_string(), person_id.to_string(), payload),
            ).await.map_err(storage_error)?
        };
        if changed != 1 {
            return Err(StoreError::new(
                StoreErrorCode::Conflict,
                "calendar changed during sync; reload and retry",
            ));
        }
        Ok(())
    }

    pub(crate) async fn put<T: serde::Serialize>(
        &self,
        table: &str,
        id: String,
        person_id: PersonId,
        value: &T,
    ) -> Result<(), StoreError> {
        let payload = to_string(value).map_err(storage_error)?;
        self.connection().await?.execute(
            &format!("INSERT INTO {table}(id, person_id, payload) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET person_id=excluded.person_id, payload=excluded.payload"),
            (id, person_id.to_string(), payload),
        ).await.map_err(storage_error)?;
        Ok(())
    }

    pub(crate) async fn get<T: serde::de::DeserializeOwned>(
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
        from_str(&payload).map(Some).map_err(storage_error)
    }

    pub(crate) async fn list<T: serde::de::DeserializeOwned>(
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
        while let Some(row) = rows.next().await.map_err(storage_error)? {
            let payload: String = row.get(0).map_err(storage_error)?;
            values.push(from_str(&payload).map_err(storage_error)?);
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
        T: serde::Serialize + serde::de::DeserializeOwned,
        F: Fn(&T) -> floe_kernel::Revision,
    {
        let payload = to_string(value).map_err(storage_error)?;
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
                return Err(StoreError::new(StoreErrorCode::NotFound, "timeline item not found"));
            };
            let current: T = from_str(&stored).map_err(storage_error)?;
            if revision(&current) != expected {
                return Err(StoreError::new(StoreErrorCode::Conflict, "stale revision")
                    .with_metadata("expected", expected.0.to_string())
                    .with_metadata("actual", revision(&current).0.to_string()));
            }
            let changed = connection
                .execute(
                    &format!("UPDATE {table} SET payload = ? WHERE id = ? AND person_id = ? AND payload = ?"),
                    (payload, id, person_id.to_string(), stored),
                )
                .await
                .map_err(storage_error)?;
            if changed != 1 {
                return Err(StoreError::new(StoreErrorCode::Conflict, "timeline item changed; reload and retry"));
            }
            connection.execute("COMMIT", ()).await.map_err(storage_error)?;
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

    pub async fn classify(&self, capture: &Capture, item: &TimelineItem) -> Result<(), StoreError> {
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
            return Err(StoreError::new(
                StoreErrorCode::Validation,
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
                return Err(StoreError::new(StoreErrorCode::NotFound, "capture not found"));
            };
            let current: Capture = from_str(&stored).map_err(storage_error)?;
            if current.revision.next() != capture.revision {
                return Err(StoreError::new(StoreErrorCode::Conflict, "stale revision")
                    .with_metadata(
                        "expected",
                        capture.revision.0.saturating_sub(1).to_string(),
                    )
                    .with_metadata("actual", current.revision.0.to_string()));
            }
            if !matches!(current.processing, floe_day::CaptureProcessing::Pending) {
                return Err(StoreError::new(StoreErrorCode::Conflict, "capture has already been resolved"));
            }
            connection.execute(
                &format!("INSERT INTO {table}(id, person_id, payload) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload"),
                (id, person_id.to_string(), payload),
            ).await.map_err(storage_error)?;
            connection.execute(
                "INSERT INTO captures(id, person_id, payload) VALUES (?, ?, ?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",
                (capture.id.to_string(), capture.person_id.to_string(), capture_payload),
            ).await.map_err(storage_error)?;
            connection.execute("COMMIT", ()).await.map_err(storage_error)?;
            Ok::<_, StoreError>(())
        }.await;
        if result.is_err() {
            let _ = connection.execute("ROLLBACK", ()).await;
        }
        result
    }
}

pub(crate) fn storage_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::new(StoreErrorCode::Storage, error.to_string())
}

fn unsupported_profile()->StoreError{StoreError::new(StoreErrorCode::Validation,"selected profile database schema is unsupported")}
async fn require_schema(connection:&Connection,name:&str,expected:&str)->Result<(),StoreError>{
    let mut rows=connection.query("SELECT sql FROM sqlite_master WHERE name = ? AND type IN ('table','index')",(name,)).await.map_err(|_|unsupported_profile())?;
    let row=rows.next().await.map_err(storage_error)?.ok_or_else(unsupported_profile)?;
    let sql:String=row.get(0).map_err(storage_error)?;
    let canonical=|value:&str|value.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase().replace("if not exists ","");
    if canonical(&sql)!=canonical(expected)||rows.next().await.map_err(storage_error)?.is_some(){return Err(unsupported_profile())}Ok(())
}
/// Turso's high-level Builder defaults to Create. Retaining a preopened main
/// file makes existing-profile admission independent of that default.
struct ExistingProfileIo{path:String,inner:turso::core::PlatformIO,main:Arc<dyn turso::core::File>,identity:turso::core::io::FileId}
impl ExistingProfileIo{
    fn new(path:String)->turso::core::Result<Self>{
        let inner=turso::core::PlatformIO::new()?;
        let identity=inner.file_id(&path)?;
        let main=inner.open_file(&path,turso::core::OpenFlags::None,false)?;
        if identity!=inner.file_id(&path)?{return Err(turso::core::LimboError::InternalError("selected profile changed during open".into()))}
        Ok(Self{path,inner,main,identity})
    }
}
impl Clock for ExistingProfileIo{
    fn current_time_monotonic(&self)->turso::core::MonotonicInstant{self.inner.current_time_monotonic()}
    fn current_time_wall_clock(&self)->turso::core::WallClockInstant{self.inner.current_time_wall_clock()}
}
impl IO for ExistingProfileIo{
    fn open_file(&self,path:&str,flags:turso::core::OpenFlags,direct:bool)->turso::core::Result<Arc<dyn turso::core::File>>{
        if path==self.path{return Ok(self.main.clone())}self.inner.open_file(path,flags,direct)
    }
    fn remove_file(&self,path:&str)->turso::core::Result<()>{
        if path==self.path{return Err(turso::core::LimboError::InternalError("cannot remove selected profile".into()))}self.inner.remove_file(path)
    }
    fn file_id(&self,path:&str)->turso::core::Result<turso::core::io::FileId>{if path==self.path{Ok(self.identity)}else{self.inner.file_id(path)}}
    fn supports_shared_wal_coordination(&self)->bool{self.inner.supports_shared_wal_coordination()}
    fn step(&self)->turso::core::Result<()>{self.inner.step()}
    fn cancel(&self,completions:&[turso::core::Completion])->turso::core::Result<()>{self.inner.cancel(completions)}
    fn drain_completions(&self,completions:&[turso::core::Completion])->turso::core::Result<()>{self.inner.drain_completions(completions)}
}
