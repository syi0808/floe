use chrono::{DateTime, NaiveDate, Utc};

use crate::{
    Capture, CaptureProcessing, CaptureSource, DayError, DaySnapshot, DomainError, DomainRef,
    Event, EventSchedule, Note, Priority, SourceRef, Task, TimelineItem, DayRepository,
    project_day_with_end_offset,
};
use floe_kernel::{CaptureId, EventId, NoteId, PersonId, Revision, TaskId};

#[derive(Clone, Debug)]
pub enum Classification {
    Event {
        title: String,
        schedule: EventSchedule,
    },
    Task {
        title: String,
        deadline: Option<DateTime<Utc>>,
        priority: Priority,
    },
    Note {
        content: String,
    },
}

#[derive(Clone)]
pub struct DayService {
    pub(crate) repository: std::sync::Arc<dyn DayRepository>,
    pub(crate) acquisition: std::sync::Arc<dyn crate::CalendarAcquisitionPort>,
    pub(crate) clock: std::sync::Arc<dyn crate::DayClock>,
    pub(crate) lifecycle: std::sync::Arc<super::refresh::DayLifecycle>,
}

impl DayService {
    pub fn new(repository: std::sync::Arc<dyn DayRepository>, acquisition: std::sync::Arc<dyn crate::CalendarAcquisitionPort>, clock: std::sync::Arc<dyn crate::DayClock>) -> Self {
        Self { repository, acquisition, clock, lifecycle: std::sync::Arc::new(super::refresh::DayLifecycle::new()) }
    }

    pub(crate) async fn submit_capture(
        &self,
        person_id: PersonId,
        input: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Capture, DayError> {
        self.admit_person(person_id)?;
        let capture = Capture::new(person_id, input, now, CaptureSource::Typed)?;
        self.repository.put_capture(&capture).await?;
        Ok(capture)
    }

    pub(crate) async fn create_event(
        &self,
        person_id: PersonId,
        title: impl Into<String>,
        schedule: EventSchedule,
        now: DateTime<Utc>,
    ) -> Result<Event, DayError> {
        self.admit_person(person_id)?;
        let event = Event::new(person_id, title, schedule, SourceRef::Manual, now)?;
        self.repository.put_event(&event).await?;
        Ok(event)
    }

    pub(crate) async fn create_task(
        &self,
        person_id: PersonId,
        title: impl Into<String>,
        deadline: Option<DateTime<Utc>>,
        priority: Priority,
        now: DateTime<Utc>,
    ) -> Result<Task, DayError> {
        self.admit_person(person_id)?;
        let task = Task::new(person_id, title, deadline, priority, SourceRef::Manual, now)?;
        self.repository.put_task(&task).await?;
        Ok(task)
    }

    pub(crate) async fn create_note(
        &self,
        person_id: PersonId,
        content: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Note, DayError> {
        self.admit_person(person_id)?;
        let note = Note::new(person_id, content, SourceRef::Manual, now)?;
        self.repository.put_note(&note).await?;
        Ok(note)
    }

    pub(crate) async fn classify_capture(
        &self,
        capture_id: CaptureId,
        expected_revision: Revision,
        classification: Classification,
        now: DateTime<Utc>,
    ) -> Result<TimelineItem, DayError> {
        let mut capture = self
            .repository
            .get_capture(capture_id)
            .await?
            .ok_or_else(|| DayError::not_found("capture", capture_id))?;
        self.admit_person(capture.person_id)?;
        ensure_revision(capture.revision, expected_revision)?;
        if !matches!(capture.processing, CaptureProcessing::Pending) {
            return Err(DayError::conflict("capture has already been resolved"));
        }
        let source = SourceRef::Capture(capture.id);
        let item = match classification {
            Classification::Event { title, schedule } => {
                TimelineItem::Event(Event::new(capture.person_id, title, schedule, source, now)?)
            }
            Classification::Task {
                title,
                deadline,
                priority,
            } => TimelineItem::Task(Task::new(
                capture.person_id,
                title,
                deadline,
                priority,
                source,
                now,
            )?),
            Classification::Note { content } => {
                TimelineItem::Note(Note::new(capture.person_id, content, source, now)?)
            }
        };
        let target = match &item {
            TimelineItem::Event(value) => DomainRef::Event(value.id),
            TimelineItem::Task(value) => DomainRef::Task(value.id),
            TimelineItem::Note(value) => DomainRef::Note(value.id),
        };
        capture.classify(target, now);
        self.repository.classify(&capture, &item).await?;
        Ok(item)
    }

    pub(crate) async fn set_task_completed(
        &self,
        task_id: TaskId,
        expected_revision: Revision,
        completed: bool,
        now: DateTime<Utc>,
    ) -> Result<Task, DayError> {
        let mut task = self
            .repository
            .get_task(task_id)
            .await?
            .ok_or_else(|| DayError::not_found("task", task_id))?;
        self.admit_person(task.person_id)?;
        ensure_revision(task.revision, expected_revision)?;
        if completed {
            task.complete(now);
        } else {
            task.reopen(now);
        }
        self.repository
            .put_task_if_revision(&task, expected_revision)
            .await?;
        Ok(task)
    }

    pub(crate) async fn update_event(
        &self,
        event_id: EventId,
        expected_revision: Revision,
        title: impl Into<String>,
        schedule: EventSchedule,
        now: DateTime<Utc>,
    ) -> Result<Event, DayError> {
        let mut event = self
            .repository
            .get_event(event_id)
            .await?
            .ok_or_else(|| DayError::not_found("event", event_id))?;
        self.admit_person(event.person_id)?;
        ensure_revision(event.revision, expected_revision)?;
        ensure_local_event(&event)?;
        event.update(title, schedule, now)?;
        self.repository
            .put_event_if_revision(&event, expected_revision)
            .await?;
        Ok(event)
    }

    pub(crate) async fn update_task(
        &self,
        task_id: TaskId,
        expected_revision: Revision,
        title: impl Into<String>,
        deadline: Option<DateTime<Utc>>,
        priority: Priority,
        now: DateTime<Utc>,
    ) -> Result<Task, DayError> {
        let mut task = self
            .repository
            .get_task(task_id)
            .await?
            .ok_or_else(|| DayError::not_found("task", task_id))?;
        self.admit_person(task.person_id)?;
        ensure_revision(task.revision, expected_revision)?;
        task.update(title, deadline, priority, now)?;
        self.repository
            .put_task_if_revision(&task, expected_revision)
            .await?;
        Ok(task)
    }

    pub(crate) async fn update_note(
        &self,
        note_id: NoteId,
        expected_revision: Revision,
        content: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Note, DayError> {
        let mut note = self
            .repository
            .get_note(note_id)
            .await?
            .ok_or_else(|| DayError::not_found("note", note_id))?;
        self.admit_person(note.person_id)?;
        ensure_revision(note.revision, expected_revision)?;
        note.update(content, now)?;
        self.repository
            .put_note_if_revision(&note, expected_revision)
            .await?;
        Ok(note)
    }

    pub(crate) async fn delete_item(
        &self,
        reference: DomainRef,
        expected_revision: Revision,
        now: DateTime<Utc>,
    ) -> Result<(), DayError> {
        match reference {
            DomainRef::Event(id) => {
                let mut value = self
                    .repository
                    .get_event(id)
                    .await?
                    .ok_or_else(|| DayError::not_found("event", id))?;
                self.admit_person(value.person_id)?;
                ensure_revision(value.revision, expected_revision)?;
                ensure_local_event(&value)?;
                value.delete(now);
                self.repository
                    .put_event_if_revision(&value, expected_revision)
                    .await?;
            }
            DomainRef::Task(id) => {
                let mut value = self
                    .repository
                    .get_task(id)
                    .await?
                    .ok_or_else(|| DayError::not_found("task", id))?;
                self.admit_person(value.person_id)?;
                ensure_revision(value.revision, expected_revision)?;
                value.delete(now);
                self.repository
                    .put_task_if_revision(&value, expected_revision)
                    .await?;
            }
            DomainRef::Note(id) => {
                let mut value = self
                    .repository
                    .get_note(id)
                    .await?
                    .ok_or_else(|| DayError::not_found("note", id))?;
                self.admit_person(value.person_id)?;
                ensure_revision(value.revision, expected_revision)?;
                value.delete(now);
                self.repository
                    .put_note_if_revision(&value, expected_revision)
                    .await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn selected_day_items(&self, person_id: PersonId, query: &crate::DayQuery) -> Result<(Vec<Event>, Vec<Task>, Vec<Note>), DayError> {
        let values = self.repository.read_items(crate::DayReadQuery::display(person_id, query)?).await?;
        let mut events = Vec::new(); let mut tasks = Vec::new(); let mut notes = Vec::new();
        for item in values { match item { TimelineItem::Event(value) => events.push(value), TimelineItem::Task(value) => tasks.push(value), TimelineItem::Note(value) => notes.push(value) } }
        Ok((events, tasks, notes))
    }

    pub(crate) async fn day_snapshot(
        &self,
        person_id: PersonId,
        date: NaiveDate,
        timezone_offset_seconds: i32,
        now: DateTime<Utc>,
    ) -> Result<DaySnapshot, DayError> {
        self.admit_person(person_id)?;
        self.day_snapshot_with_end_offset(person_id, date, timezone_offset_seconds, None, now)
            .await
    }

    pub(crate) async fn day_snapshot_with_end_offset(
        &self,
        person_id: PersonId,
        date: NaiveDate,
        timezone_offset_seconds: i32,
        end_timezone_offset_seconds: Option<i32>,
        now: DateTime<Utc>,
    ) -> Result<DaySnapshot, DayError> {
        self.admit_person(person_id)?;
        let end_date_exclusive = date
            .succ_opt()
            .ok_or_else(|| DayError::validation("date out of range"))?;
        let range = crate::CalendarRange {
            start_date: date,
            end_date_exclusive,
            timezone_offset_seconds,
            end_timezone_offset_seconds,
        };
        if !range.is_valid() {
            return Err(DayError::validation("invalid day offsets"));
        }
        let mirror = self.repository.calendar_mirror(person_id).await?;
        let query = crate::DayQuery { date, timezone_offset_seconds, end_timezone_offset_seconds, now };
        let (mut events, tasks, notes) = self.selected_day_items(person_id, &query).await?;
        if let Some(mirror) = &mirror { events.extend(mirror.events.clone()); }
        let mut snapshot = project_day_with_end_offset(person_id, date, timezone_offset_seconds, end_timezone_offset_seconds, now, events, tasks, notes)?;
        snapshot.calendar_mirror_revision = mirror.as_ref().map(|mirror| mirror.mirror_revision);
        snapshot.calendar = mirror.map(|mirror| crate::project_calendar_coverage(&mirror.state, &range, now));
        snapshot.validate_bounds()?;
        Ok(snapshot)
    }
}

fn ensure_revision(actual: Revision, expected: Revision) -> Result<(), DayError> {
    if actual.0 == 0 || actual.0 >= i64::MAX as u64 || expected.0 == 0 { return Err(DayError::conflict("Day revision unavailable or exhausted")); }
    if actual == expected {
        Ok(())
    } else {
        Err(DayError::stale(expected, actual))
    }
}

fn ensure_local_event(event: &Event) -> Result<(), DayError> {
    if matches!(event.source, SourceRef::Calendar(_)) {
        Err(DayError::validation(
            "external calendar events are read-only",
        ))
    } else {
        Ok(())
    }
}

impl From<DomainError> for DayError {
    fn from(error: DomainError) -> Self {
        Self::validation(error.to_string())
    }
}
