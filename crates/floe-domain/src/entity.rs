use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CaptureId, EventId, NoteId, PersonId, Revision, TaskId};

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DomainError {
    #[error("{field} must not be empty")]
    EmptyText { field: &'static str },
    #[error("event end must be after its start")]
    InvalidEventInterval,
}

fn required(value: impl Into<String>, field: &'static str) -> Result<String, DomainError> {
    let value = value.into().trim().to_owned();
    if value.is_empty() {
        Err(DomainError::EmptyText { field })
    } else {
        Ok(value)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SourceRef {
    Manual,
    Capture(CaptureId),
    Calendar(crate::CalendarSource),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TimedSchedule {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub timezone: String,
}

impl TimedSchedule {
    pub fn new(
        starts_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
        timezone: impl Into<String>,
    ) -> Result<Self, DomainError> {
        if ends_at <= starts_at {
            return Err(DomainError::InvalidEventInterval);
        }
        Ok(Self {
            starts_at,
            ends_at,
            timezone: required(timezone, "timezone")?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AllDaySchedule {
    pub start_date: NaiveDate,
    pub end_date_exclusive: NaiveDate,
}

impl AllDaySchedule {
    pub fn new(start_date: NaiveDate, end_date_exclusive: NaiveDate) -> Result<Self, DomainError> {
        if end_date_exclusive <= start_date {
            return Err(DomainError::InvalidEventInterval);
        }
        Ok(Self {
            start_date,
            end_date_exclusive,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EventSchedule {
    Timed(TimedSchedule),
    AllDay(AllDaySchedule),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Event {
    pub id: EventId,
    pub person_id: PersonId,
    pub title: String,
    pub schedule: EventSchedule,
    pub source: SourceRef,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: Revision,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Event {
    pub fn new(
        person_id: PersonId,
        title: impl Into<String>,
        schedule: EventSchedule,
        source: SourceRef,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: EventId::new(),
            person_id,
            title: required(title, "title")?,
            schedule,
            source,
            created_at: now,
            updated_at: now,
            revision: Revision::default(),
            deleted_at: None,
        })
    }
    pub fn update(
        &mut self,
        title: impl Into<String>,
        schedule: EventSchedule,
        now: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        self.title = required(title, "title")?;
        self.schedule = schedule;
        self.updated_at = now;
        self.revision = self.revision.next();
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Task {
    pub id: TaskId,
    pub person_id: PersonId,
    pub title: String,
    pub deadline: Option<DateTime<Utc>>,
    pub priority: Priority,
    pub completed_at: Option<DateTime<Utc>>,
    pub source: SourceRef,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: Revision,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Task {
    pub fn new(
        person_id: PersonId,
        title: impl Into<String>,
        deadline: Option<DateTime<Utc>>,
        priority: Priority,
        source: SourceRef,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: TaskId::new(),
            person_id,
            title: required(title, "title")?,
            deadline,
            priority,
            completed_at: None,
            source,
            created_at: now,
            updated_at: now,
            revision: Revision::default(),
            deleted_at: None,
        })
    }
    pub fn complete(&mut self, now: DateTime<Utc>) {
        self.completed_at = Some(now);
        self.updated_at = now;
        self.revision = self.revision.next();
    }
    pub fn reopen(&mut self, now: DateTime<Utc>) {
        self.completed_at = None;
        self.updated_at = now;
        self.revision = self.revision.next();
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Note {
    pub id: NoteId,
    pub person_id: PersonId,
    pub content: String,
    pub source: SourceRef,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: Revision,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Note {
    pub fn new(
        person_id: PersonId,
        content: impl Into<String>,
        source: SourceRef,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: NoteId::new(),
            person_id,
            content: required(content, "content")?,
            source,
            created_at: now,
            updated_at: now,
            revision: Revision::default(),
            deleted_at: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_text_is_rejected() {
        assert!(matches!(
            required("  ", "title"),
            Err(DomainError::EmptyText { .. })
        ));
    }
    #[test]
    fn invalid_interval_is_rejected() {
        let now = Utc::now();
        assert_eq!(
            TimedSchedule::new(now, now, "UTC"),
            Err(DomainError::InvalidEventInterval)
        );
    }
}
