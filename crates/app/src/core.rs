use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};

use floe_context::SourceLeaseRegistry;
use floe_day::{
    Capture, CaptureId, DaySnapshot, DomainRef, Event, EventId, EventSchedule, Note, NoteId,
    Priority, Revision, Task, TaskId, TimelineItem,
};
use floe_kernel::PersonId;
use floe_vault::TursoStore;

use crate::{CoreError, ErrorCode};

pub struct FloeCore {
    pub(crate) store: Arc<TursoStore>,
    pub(crate) lease_registry: Arc<SourceLeaseRegistry>,
}

pub use floe_day::Classification;

impl FloeCore {
    pub async fn open(path: impl AsRef<std::path::Path>) -> Result<Self, CoreError> {
        Ok(Self {
            store: Arc::new(
                TursoStore::open(path)
                    .await
                    .map_err(|error| CoreError::new(ErrorCode::Storage, error.to_string()))?,
            ),
            lease_registry: Arc::new(SourceLeaseRegistry::new()),
        })
    }

    pub fn day_service(&self) -> floe_day::DayService<'_, TursoStore> {
        floe_day::DayService::new(self.store.as_ref())
    }

    pub fn source_service(&self) -> floe_connections::SourceConnectionService<'_, TursoStore> {
        floe_connections::SourceConnectionService::new(self.store.as_ref())
    }

    pub async fn submit_capture(
        &self,
        person_id: PersonId,
        input: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Capture, CoreError> {
        self.day_service()
            .submit_capture(person_id, input, now)
            .await
            .map_err(day_error)
    }

    pub async fn create_event(
        &self,
        person_id: PersonId,
        title: impl Into<String>,
        schedule: EventSchedule,
        now: DateTime<Utc>,
    ) -> Result<Event, CoreError> {
        self.day_service()
            .create_event(person_id, title, schedule, now)
            .await
            .map_err(day_error)
    }

    pub async fn create_task(
        &self,
        person_id: PersonId,
        title: impl Into<String>,
        deadline: Option<DateTime<Utc>>,
        priority: Priority,
        now: DateTime<Utc>,
    ) -> Result<Task, CoreError> {
        self.day_service()
            .create_task(person_id, title, deadline, priority, now)
            .await
            .map_err(day_error)
    }

    pub async fn create_note(
        &self,
        person_id: PersonId,
        content: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Note, CoreError> {
        self.day_service()
            .create_note(person_id, content, now)
            .await
            .map_err(day_error)
    }

    pub async fn classify_capture(
        &self,
        capture_id: CaptureId,
        expected_revision: Revision,
        classification: Classification,
        now: DateTime<Utc>,
    ) -> Result<TimelineItem, CoreError> {
        self.day_service()
            .classify_capture(capture_id, expected_revision, classification, now)
            .await
            .map_err(day_error)
    }

    pub async fn set_task_completed(
        &self,
        task_id: TaskId,
        expected_revision: Revision,
        completed: bool,
        now: DateTime<Utc>,
    ) -> Result<Task, CoreError> {
        self.day_service()
            .set_task_completed(task_id, expected_revision, completed, now)
            .await
            .map_err(day_error)
    }

    pub async fn update_event(
        &self,
        event_id: EventId,
        expected_revision: Revision,
        title: impl Into<String>,
        schedule: EventSchedule,
        now: DateTime<Utc>,
    ) -> Result<Event, CoreError> {
        self.day_service()
            .update_event(event_id, expected_revision, title, schedule, now)
            .await
            .map_err(day_error)
    }

    pub async fn update_task(
        &self,
        task_id: TaskId,
        expected_revision: Revision,
        title: impl Into<String>,
        deadline: Option<DateTime<Utc>>,
        priority: Priority,
        now: DateTime<Utc>,
    ) -> Result<Task, CoreError> {
        self.day_service()
            .update_task(task_id, expected_revision, title, deadline, priority, now)
            .await
            .map_err(day_error)
    }

    pub async fn update_note(
        &self,
        note_id: NoteId,
        expected_revision: Revision,
        content: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Note, CoreError> {
        self.day_service()
            .update_note(note_id, expected_revision, content, now)
            .await
            .map_err(day_error)
    }

    pub async fn delete_item(
        &self,
        reference: DomainRef,
        expected_revision: Revision,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        self.day_service()
            .delete_item(reference, expected_revision, now)
            .await
            .map_err(day_error)
    }

    pub async fn day_snapshot(
        &self,
        person_id: PersonId,
        date: NaiveDate,
        timezone_offset_seconds: i32,
        now: DateTime<Utc>,
    ) -> Result<DaySnapshot, CoreError> {
        self.day_snapshot_with_end_offset(person_id, date, timezone_offset_seconds, None, now)
            .await
    }

    pub async fn day_snapshot_with_end_offset(
        &self,
        person_id: PersonId,
        date: NaiveDate,
        timezone_offset_seconds: i32,
        end_timezone_offset_seconds: Option<i32>,
        now: DateTime<Utc>,
    ) -> Result<DaySnapshot, CoreError> {
        self.day_service()
            .day_snapshot_with_end_offset(
                person_id,
                date,
                timezone_offset_seconds,
                end_timezone_offset_seconds,
                now,
            )
            .await
            .map_err(day_error)
    }
}

pub(crate) fn day_error(error: floe_day::DayError) -> CoreError {
    let code = match error.code {
        floe_day::DayErrorCode::Validation => ErrorCode::Validation,
        floe_day::DayErrorCode::NotFound => ErrorCode::NotFound,
        floe_day::DayErrorCode::Conflict => ErrorCode::Conflict,
        floe_day::DayErrorCode::Storage => ErrorCode::Storage,
    };
    let mut result = CoreError::new(code, error.message);
    for (key, value) in error.metadata {
        result = result.with_metadata(key, value);
    }
    result
}
