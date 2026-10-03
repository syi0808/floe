use crate::{CalendarMirror, Capture, Event, Note, Task, TimelineItem};
use floe_kernel::{CaptureId, EventId, NoteId, PersonId, Revision, TaskId};
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
    pub fn budget(message: impl Into<String>) -> Self { Self::validation(message).with_metadata("reason_code", "budget_exceeded") }
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

pub trait DayRepository: super::refresh_repository::DayRefreshRepository + Send + Sync {
    fn collect_action<'a>(&'a self, commit: crate::DayCollectionCommit) -> floe_execution::BoxFuture<'a, Result<crate::DayCollectionReceipt, DayError>>;
    fn put_capture<'a>(&'a self, value: &'a Capture) -> floe_execution::BoxFuture<'a, Result<(), DayError>>;
    fn put_event<'a>(&'a self, value: &'a Event) -> floe_execution::BoxFuture<'a, Result<(), DayError>>;
    fn put_event_if_revision<'a>(&'a self, value: &'a Event, expected: Revision) -> floe_execution::BoxFuture<'a, Result<(), DayError>>;
    fn put_task<'a>(&'a self, value: &'a Task) -> floe_execution::BoxFuture<'a, Result<(), DayError>>;
    fn put_task_if_revision<'a>(&'a self, value: &'a Task, expected: Revision) -> floe_execution::BoxFuture<'a, Result<(), DayError>>;
    fn put_note<'a>(&'a self, value: &'a Note) -> floe_execution::BoxFuture<'a, Result<(), DayError>>;
    fn put_note_if_revision<'a>(&'a self, value: &'a Note, expected: Revision) -> floe_execution::BoxFuture<'a, Result<(), DayError>>;
    fn get_capture<'a>(&'a self, id: CaptureId) -> floe_execution::BoxFuture<'a, Result<Option<Capture>, DayError>>;
    fn get_event<'a>(&'a self, id: EventId) -> floe_execution::BoxFuture<'a, Result<Option<Event>, DayError>>;
    fn get_task<'a>(&'a self, id: TaskId) -> floe_execution::BoxFuture<'a, Result<Option<Task>, DayError>>;
    fn get_note<'a>(&'a self, id: NoteId) -> floe_execution::BoxFuture<'a, Result<Option<Note>, DayError>>;
    fn read_items<'a>(&'a self, query: crate::DayReadQuery) -> floe_execution::BoxFuture<'a, Result<Vec<TimelineItem>, DayError>>;
    fn classify<'a>(&'a self, capture: &'a Capture, item: &'a TimelineItem) -> floe_execution::BoxFuture<'a, Result<(), DayError>>;
    fn calendar_mirror<'a>(&'a self, person_id: PersonId) -> floe_execution::BoxFuture<'a, Result<Option<CalendarMirror>, DayError>>;
}
