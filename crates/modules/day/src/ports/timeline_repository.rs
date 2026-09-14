use crate::{
    CalendarMirror, Capture, CaptureId, Event, EventId, Note, NoteId, PersonId, Revision, Task,
    TaskId, TimelineItem,
};
use std::{collections::BTreeMap, fmt::Display};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DayErrorCode {
    Validation,
    NotFound,
    Conflict,
    Storage,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{message}")]
pub struct DayError {
    pub code: DayErrorCode,
    pub message: String,
    pub metadata: BTreeMap<String, String>,
}

impl DayError {
    pub fn new(code: DayErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(DayErrorCode::Validation, message)
    }
    pub fn not_found(kind: impl Display, id: impl Display) -> Self {
        Self::new(DayErrorCode::NotFound, format!("{kind} not found"))
            .with_metadata("id", id.to_string())
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(DayErrorCode::Conflict, message)
    }
    pub fn stale(expected: Revision, actual: Revision) -> Self {
        Self::conflict("stale revision")
            .with_metadata("expected", expected.0.to_string())
            .with_metadata("actual", actual.0.to_string())
    }
    pub fn storage(message: impl Into<String>) -> Self {
        Self::new(DayErrorCode::Storage, message)
    }
}

#[allow(async_fn_in_trait)]
pub trait TimelineRepository: Send + Sync {
    async fn put_capture(&self, value: &Capture) -> Result<(), DayError>;
    async fn put_event(&self, value: &Event) -> Result<(), DayError>;
    async fn put_event_if_revision(
        &self,
        value: &Event,
        expected: Revision,
    ) -> Result<(), DayError>;
    async fn put_task(&self, value: &Task) -> Result<(), DayError>;
    async fn put_task_if_revision(&self, value: &Task, expected: Revision) -> Result<(), DayError>;
    async fn put_note(&self, value: &Note) -> Result<(), DayError>;
    async fn put_note_if_revision(&self, value: &Note, expected: Revision) -> Result<(), DayError>;
    async fn get_capture(&self, id: CaptureId) -> Result<Option<Capture>, DayError>;
    async fn get_event(&self, id: EventId) -> Result<Option<Event>, DayError>;
    async fn get_task(&self, id: TaskId) -> Result<Option<Task>, DayError>;
    async fn get_note(&self, id: NoteId) -> Result<Option<Note>, DayError>;
    async fn list_events(&self, person_id: PersonId) -> Result<Vec<Event>, DayError>;
    async fn list_tasks(&self, person_id: PersonId) -> Result<Vec<Task>, DayError>;
    async fn list_notes(&self, person_id: PersonId) -> Result<Vec<Note>, DayError>;
    async fn classify(&self, capture: &Capture, item: &TimelineItem) -> Result<(), DayError>;
    async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarMirror>, DayError>;
    async fn put_calendar_mirror(
        &self,
        person_id: PersonId,
        mirror: &CalendarMirror,
        previous: Option<&CalendarMirror>,
    ) -> Result<(), DayError>;
}

pub use TimelineRepository as DayRepository;
