use std::path::Path;

use floe_domain::{
    Capture, CaptureId, Event, EventId, Note, NoteId, PersonId, Task, TaskId, TimelineItem,
};
use serde_json::{from_str, to_string};
use turso::{Builder, Connection};

use crate::ports::TimelineRepository;
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
        store.initialize().await?;
        Ok(store)
    }

    async fn connection(&self) -> Result<Connection, CoreError> {
        self.database.connect().map_err(storage_error)
    }

    async fn initialize(&self) -> Result<(), CoreError> {
        let connection = self.connection().await?;
        for table in [
            "captures",
            "events",
            "tasks",
            "notes",
            "calendar_mirrors",
            "calendar_actions",
            "action_authorities",
            "agent_fixture_sessions",
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
        Ok(())
    }

    pub(crate) async fn agent_fixture_session(
        &self,
        person_id: PersonId,
        session_id: uuid::Uuid,
    ) -> Result<floe_agent::AgentSession, CoreError> {
        let session: floe_agent::AgentSession = self
            .get("agent_fixture_sessions", session_id.to_string())
            .await?
            .ok_or_else(|| {
                CoreError::new(ErrorCode::NotFound, "agent fixture session not found")
            })?;
        if session.person_id != person_id {
            return Err(CoreError::new(
                ErrorCode::NotFound,
                "agent fixture session not found",
            ));
        }
        Ok(session)
    }

    pub(crate) async fn latest_agent_fixture_session(
        &self,
        person_id: PersonId,
    ) -> Result<Option<floe_agent::AgentSession>, CoreError> {
        let connection = self.connection().await?;
        let mut rows = connection.query(
            "SELECT payload FROM agent_fixture_sessions WHERE person_id = ? ORDER BY rowid DESC LIMIT 1",
            [person_id.to_string()],
        ).await.map_err(storage_error)?;
        match rows.next().await.map_err(storage_error)? {
            Some(row) => {
                let payload: String = row.get(0).map_err(storage_error)?;
                Ok(Some(from_str(&payload).map_err(storage_error)?))
            }
            None => Ok(None),
        }
    }

    pub(crate) async fn save_agent_fixture_session(
        &self,
        session: &floe_agent::AgentSession,
        previous: Option<&floe_agent::AgentSession>,
    ) -> Result<(), CoreError> {
        let connection = self.connection().await?;
        let payload = to_string(session).map_err(storage_error)?;
        let changed = if let Some(previous) = previous {
            connection.execute(
                "UPDATE agent_fixture_sessions SET payload = ? WHERE id = ? AND person_id = ? AND payload = ?",
                (payload, session.id.to_string(), session.person_id.to_string(), to_string(previous).map_err(storage_error)?),
            ).await.map_err(storage_error)?
        } else {
            connection.execute(
                "INSERT OR IGNORE INTO agent_fixture_sessions(id, person_id, payload) VALUES (?, ?, ?)",
                (session.id.to_string(), session.person_id.to_string(), payload),
            ).await.map_err(storage_error)?
        };
        if changed != 1 {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "agent fixture session changed; reload",
            ));
        }
        Ok(())
    }

    pub(crate) async fn action_authority(
        &self,
        person_id: PersonId,
    ) -> Result<Option<crate::ActionAuthority>, CoreError> {
        self.get("action_authorities", person_id.to_string()).await
    }

    pub(crate) async fn put_action_authority(
        &self,
        authority: &crate::ActionAuthority,
    ) -> Result<(), CoreError> {
        self.put(
            "action_authorities",
            authority.person_id.to_string(),
            authority.person_id,
            authority,
        )
        .await
    }

    pub(crate) async fn calendar_actions(
        &self,
        person_id: PersonId,
    ) -> Result<Vec<crate::CalendarAction>, CoreError> {
        let mut actions: Vec<crate::CalendarAction> =
            self.list("calendar_actions", person_id).await?;
        actions.sort_by_key(|action| (std::cmp::Reverse(action.created_at), action.id));
        Ok(actions)
    }

    pub(crate) async fn calendar_action(
        &self,
        person_id: PersonId,
        id: uuid::Uuid,
    ) -> Result<crate::CalendarAction, CoreError> {
        let action: crate::CalendarAction = self
            .get("calendar_actions", id.to_string())
            .await?
            .ok_or_else(|| CoreError::new(ErrorCode::NotFound, "calendar action not found"))?;
        if action.person_id != person_id {
            return Err(CoreError::new(
                ErrorCode::NotFound,
                "calendar action not found",
            ));
        }
        Ok(action)
    }

    pub(crate) async fn bounded_expert_calendar_action(
        &self,
        person_id: PersonId,
        id: uuid::Uuid,
    ) -> Result<Option<crate::CalendarAction>, floe_agent::AgentFailure> {
        use floe_agent::AgentFailure;
        let connection = self
            .connection()
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let mut rows = connection.query(
            "SELECT length(CAST(payload AS BLOB)), CASE WHEN length(CAST(payload AS BLOB)) <= 65536 THEN payload ELSE NULL END FROM calendar_actions WHERE id = ? AND person_id = ?",
            (id.to_string(), person_id.to_string()),
        ).await.map_err(|_| AgentFailure::StorageUnavailable)?;
        let Some(row) = rows
            .next()
            .await
            .map_err(|_| AgentFailure::StorageUnavailable)?
        else {
            return Ok(None);
        };
        if row
            .get::<i64>(0)
            .map_err(|_| AgentFailure::StorageUnavailable)?
            > 65_536
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let action: crate::CalendarAction = serde_json::from_str(
            &row.get::<String>(1)
                .map_err(|_| AgentFailure::StorageUnavailable)?,
        )
        .map_err(|_| AgentFailure::StorageUnavailable)?;
        if action.person_id != person_id || action.id != id {
            return Err(AgentFailure::Conflict);
        }
        Ok(Some(action))
    }

    pub(crate) async fn save_calendar_action(
        &self,
        action: &crate::CalendarAction,
        previous: Option<&crate::CalendarAction>,
    ) -> Result<(), CoreError> {
        let connection = self.connection().await?;
        let payload = to_string(action).map_err(storage_error)?;
        let changed = if let Some(previous) = previous {
            connection.execute(
                "UPDATE calendar_actions SET payload = ? WHERE id = ? AND person_id = ? AND payload = ?",
                (payload, action.id.to_string(), action.person_id.to_string(), to_string(previous).map_err(storage_error)?),
            ).await.map_err(storage_error)?
        } else {
            connection.execute(
                "INSERT OR IGNORE INTO calendar_actions(id, person_id, payload) VALUES (?, ?, ?)",
                (action.id.to_string(), action.person_id.to_string(), payload),
            ).await.map_err(storage_error)?
        };
        if changed != 1 {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "calendar action changed; reload its status",
            ));
        }
        Ok(())
    }

    pub async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<floe_domain::CalendarMirror>, CoreError> {
        self.get("calendar_mirrors", person_id.to_string()).await
    }

    pub(crate) async fn bounded_calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<floe_domain::CalendarMirror, floe_agent::AgentFailure> {
        use floe_agent::AgentFailure;
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
        mirror: &floe_domain::CalendarMirror,
        previous: Option<&floe_domain::CalendarMirror>,
    ) -> Result<(), CoreError> {
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
                    return Err(CoreError::new(
                        ErrorCode::Conflict,
                        "calendar changed during sync; reload and retry",
                    ));
                }
            };
            drop(rows);
            let expected: floe_domain::CalendarMirror = from_str(&stored).map_err(storage_error)?;
            if &expected != previous {
                return Err(CoreError::new(
                    ErrorCode::Conflict,
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
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "calendar changed during sync; reload and retry",
            ));
        }
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

    async fn put_if_revision<T, F>(
        &self,
        table: &str,
        id: String,
        person_id: PersonId,
        value: &T,
        expected: floe_domain::Revision,
        revision: F,
    ) -> Result<(), CoreError>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
        F: Fn(&T) -> floe_domain::Revision,
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
                return Err(CoreError::new(ErrorCode::NotFound, "timeline item not found"));
            };
            let current: T = from_str(&stored).map_err(storage_error)?;
            if revision(&current) != expected {
                return Err(CoreError::new(ErrorCode::Conflict, "stale revision")
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
                return Err(CoreError::new(ErrorCode::Conflict, "timeline item changed; reload and retry"));
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
        expected: floe_domain::Revision,
    ) -> Result<(), CoreError> {
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
        expected: floe_domain::Revision,
    ) -> Result<(), CoreError> {
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
        expected: floe_domain::Revision,
    ) -> Result<(), CoreError> {
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
                return Err(CoreError::new(ErrorCode::NotFound, "capture not found"));
            };
            let current: Capture = from_str(&stored).map_err(storage_error)?;
            if current.revision.next() != capture.revision {
                return Err(CoreError::new(ErrorCode::Conflict, "stale revision")
                    .with_metadata(
                        "expected",
                        capture.revision.0.saturating_sub(1).to_string(),
                    )
                    .with_metadata("actual", current.revision.0.to_string()));
            }
            if !matches!(current.processing, floe_domain::CaptureProcessing::Pending) {
                return Err(CoreError::new(ErrorCode::Conflict, "capture has already been resolved"));
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
            Ok::<_, CoreError>(())
        }.await;
        if result.is_err() {
            let _ = connection.execute("ROLLBACK", ()).await;
        }
        result
    }
}

impl TimelineRepository for TursoStore {
    async fn put_capture(&self, value: &Capture) -> Result<(), CoreError> {
        TursoStore::put_capture(self, value).await
    }

    async fn put_event(&self, value: &Event) -> Result<(), CoreError> {
        TursoStore::put_event(self, value).await
    }

    async fn put_task(&self, value: &Task) -> Result<(), CoreError> {
        TursoStore::put_task(self, value).await
    }

    async fn put_note(&self, value: &Note) -> Result<(), CoreError> {
        TursoStore::put_note(self, value).await
    }

    async fn get_capture(&self, id: CaptureId) -> Result<Option<Capture>, CoreError> {
        TursoStore::get_capture(self, id).await
    }

    async fn get_event(&self, id: EventId) -> Result<Option<Event>, CoreError> {
        TursoStore::get_event(self, id).await
    }

    async fn get_task(&self, id: TaskId) -> Result<Option<Task>, CoreError> {
        TursoStore::get_task(self, id).await
    }

    async fn get_note(&self, id: NoteId) -> Result<Option<Note>, CoreError> {
        TursoStore::get_note(self, id).await
    }

    async fn list_events(&self, person_id: PersonId) -> Result<Vec<Event>, CoreError> {
        TursoStore::list_events(self, person_id).await
    }

    async fn list_tasks(&self, person_id: PersonId) -> Result<Vec<Task>, CoreError> {
        TursoStore::list_tasks(self, person_id).await
    }

    async fn list_notes(&self, person_id: PersonId) -> Result<Vec<Note>, CoreError> {
        TursoStore::list_notes(self, person_id).await
    }

    async fn classify(&self, capture: &Capture, item: &TimelineItem) -> Result<(), CoreError> {
        TursoStore::classify(self, capture, item).await
    }

    async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<floe_domain::CalendarMirror>, CoreError> {
        TursoStore::calendar_mirror(self, person_id).await
    }
}

impl floe_day::TimelineRepository for TursoStore {
    async fn put_capture(&self, value: &floe_day::Capture) -> Result<(), floe_day::DayError> {
        TimelineRepository::put_capture(self, value)
            .await
            .map_err(day_error)
    }

    async fn put_event(&self, value: &floe_day::Event) -> Result<(), floe_day::DayError> {
        TimelineRepository::put_event(self, value)
            .await
            .map_err(day_error)
    }

    async fn put_event_if_revision(
        &self,
        value: &floe_day::Event,
        expected: floe_day::Revision,
    ) -> Result<(), floe_day::DayError> {
        TursoStore::put_event_if_revision(self, value, expected)
            .await
            .map_err(day_error)
    }

    async fn put_task(&self, value: &floe_day::Task) -> Result<(), floe_day::DayError> {
        TimelineRepository::put_task(self, value)
            .await
            .map_err(day_error)
    }

    async fn put_task_if_revision(
        &self,
        value: &floe_day::Task,
        expected: floe_day::Revision,
    ) -> Result<(), floe_day::DayError> {
        TursoStore::put_task_if_revision(self, value, expected)
            .await
            .map_err(day_error)
    }

    async fn put_note(&self, value: &floe_day::Note) -> Result<(), floe_day::DayError> {
        TimelineRepository::put_note(self, value)
            .await
            .map_err(day_error)
    }

    async fn put_note_if_revision(
        &self,
        value: &floe_day::Note,
        expected: floe_day::Revision,
    ) -> Result<(), floe_day::DayError> {
        TursoStore::put_note_if_revision(self, value, expected)
            .await
            .map_err(day_error)
    }

    async fn get_capture(
        &self,
        id: floe_day::CaptureId,
    ) -> Result<Option<floe_day::Capture>, floe_day::DayError> {
        TimelineRepository::get_capture(self, id)
            .await
            .map_err(day_error)
    }

    async fn get_event(
        &self,
        id: floe_day::EventId,
    ) -> Result<Option<floe_day::Event>, floe_day::DayError> {
        TimelineRepository::get_event(self, id)
            .await
            .map_err(day_error)
    }

    async fn get_task(
        &self,
        id: floe_day::TaskId,
    ) -> Result<Option<floe_day::Task>, floe_day::DayError> {
        TimelineRepository::get_task(self, id)
            .await
            .map_err(day_error)
    }

    async fn get_note(
        &self,
        id: floe_day::NoteId,
    ) -> Result<Option<floe_day::Note>, floe_day::DayError> {
        TimelineRepository::get_note(self, id)
            .await
            .map_err(day_error)
    }

    async fn list_events(
        &self,
        person_id: floe_day::PersonId,
    ) -> Result<Vec<floe_day::Event>, floe_day::DayError> {
        TimelineRepository::list_events(self, person_id)
            .await
            .map_err(day_error)
    }

    async fn list_tasks(
        &self,
        person_id: floe_day::PersonId,
    ) -> Result<Vec<floe_day::Task>, floe_day::DayError> {
        TimelineRepository::list_tasks(self, person_id)
            .await
            .map_err(day_error)
    }

    async fn list_notes(
        &self,
        person_id: floe_day::PersonId,
    ) -> Result<Vec<floe_day::Note>, floe_day::DayError> {
        TimelineRepository::list_notes(self, person_id)
            .await
            .map_err(day_error)
    }

    async fn classify(
        &self,
        capture: &floe_day::Capture,
        item: &floe_day::TimelineItem,
    ) -> Result<(), floe_day::DayError> {
        TimelineRepository::classify(self, capture, item)
            .await
            .map_err(day_error)
    }

    async fn calendar_mirror(
        &self,
        person_id: floe_day::PersonId,
    ) -> Result<Option<floe_day::CalendarMirror>, floe_day::DayError> {
        TimelineRepository::calendar_mirror(self, person_id)
            .await
            .map_err(day_error)
    }

    async fn put_calendar_mirror(
        &self,
        person_id: floe_day::PersonId,
        mirror: &floe_day::CalendarMirror,
        previous: Option<&floe_day::CalendarMirror>,
    ) -> Result<(), floe_day::DayError> {
        TursoStore::put_calendar_mirror(self, person_id, mirror, previous)
            .await
            .map_err(day_error)
    }
}

fn day_error(error: CoreError) -> floe_day::DayError {
    let code = match error.code {
        ErrorCode::Validation => floe_day::DayErrorCode::Validation,
        ErrorCode::NotFound => floe_day::DayErrorCode::NotFound,
        ErrorCode::Conflict => floe_day::DayErrorCode::Conflict,
        _ => floe_day::DayErrorCode::Storage,
    };
    let mut result = floe_day::DayError::new(code, error.message);
    for (key, value) in error.metadata {
        result = result.with_metadata(key, value);
    }
    result
}

fn storage_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::new(ErrorCode::Storage, error.to_string())
}
