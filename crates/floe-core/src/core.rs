use chrono::{DateTime, NaiveDate, Utc};
use floe_domain::*;

use crate::{CoreError, ErrorCode, TursoStore};

pub struct FloeCore {
    pub(crate) store: TursoStore,
}

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

impl FloeCore {
    pub async fn open(path: impl AsRef<std::path::Path>) -> Result<Self, CoreError> {
        Ok(Self {
            store: TursoStore::open(path).await?,
        })
    }

    pub async fn submit_capture(
        &self,
        person_id: PersonId,
        input: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Capture, CoreError> {
        let capture = Capture::new(person_id, input, now, CaptureSource::Typed)?;
        self.store.put_capture(&capture).await?;
        Ok(capture)
    }

    pub async fn create_event(
        &self,
        person_id: PersonId,
        title: impl Into<String>,
        schedule: EventSchedule,
        now: DateTime<Utc>,
    ) -> Result<Event, CoreError> {
        let event = Event::new(person_id, title, schedule, SourceRef::Manual, now)?;
        self.store.put_event(&event).await?;
        Ok(event)
    }

    pub async fn create_task(
        &self,
        person_id: PersonId,
        title: impl Into<String>,
        deadline: Option<DateTime<Utc>>,
        priority: Priority,
        now: DateTime<Utc>,
    ) -> Result<Task, CoreError> {
        let task = Task::new(person_id, title, deadline, priority, SourceRef::Manual, now)?;
        self.store.put_task(&task).await?;
        Ok(task)
    }

    pub async fn create_note(
        &self,
        person_id: PersonId,
        content: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Note, CoreError> {
        let note = Note::new(person_id, content, SourceRef::Manual, now)?;
        self.store.put_note(&note).await?;
        Ok(note)
    }

    pub async fn classify_capture(
        &self,
        capture_id: CaptureId,
        expected_revision: Revision,
        classification: Classification,
        now: DateTime<Utc>,
    ) -> Result<TimelineItem, CoreError> {
        let mut capture = self
            .store
            .get_capture(capture_id)
            .await?
            .ok_or_else(|| not_found("capture", capture_id))?;
        ensure_revision(capture.revision, expected_revision)?;
        if !matches!(capture.processing, CaptureProcessing::Pending) {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "capture has already been resolved",
            ));
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
        self.store.classify(&capture, &item).await?;
        Ok(item)
    }

    pub async fn set_task_completed(
        &self,
        task_id: TaskId,
        expected_revision: Revision,
        completed: bool,
        now: DateTime<Utc>,
    ) -> Result<Task, CoreError> {
        let mut task = self
            .store
            .get_task(task_id)
            .await?
            .ok_or_else(|| not_found("task", task_id))?;
        ensure_revision(task.revision, expected_revision)?;
        if completed {
            task.complete(now);
        } else {
            task.reopen(now);
        }
        self.store.put_task(&task).await?;
        Ok(task)
    }

    pub async fn complete_task(
        &self,
        task_id: TaskId,
        expected_revision: Revision,
        now: DateTime<Utc>,
    ) -> Result<Task, CoreError> {
        self.set_task_completed(task_id, expected_revision, true, now)
            .await
    }

    pub async fn update_event(
        &self,
        event_id: EventId,
        expected_revision: Revision,
        title: impl Into<String>,
        schedule: EventSchedule,
        now: DateTime<Utc>,
    ) -> Result<Event, CoreError> {
        let mut event = self
            .store
            .get_event(event_id)
            .await?
            .ok_or_else(|| not_found("event", event_id))?;
        ensure_revision(event.revision, expected_revision)?;
        ensure_local_event(&event)?;
        event.update(title, schedule, now)?;
        self.store.put_event(&event).await?;
        Ok(event)
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
        let mut task = self
            .store
            .get_task(task_id)
            .await?
            .ok_or_else(|| not_found("task", task_id))?;
        ensure_revision(task.revision, expected_revision)?;
        let validated = Task::new(
            task.person_id,
            title,
            deadline,
            priority,
            task.source.clone(),
            now,
        )?;
        task.title = validated.title;
        task.deadline = deadline;
        task.priority = priority;
        task.updated_at = now;
        task.revision = task.revision.next();
        self.store.put_task(&task).await?;
        Ok(task)
    }

    pub async fn update_note(
        &self,
        note_id: NoteId,
        expected_revision: Revision,
        content: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<Note, CoreError> {
        let mut note = self
            .store
            .get_note(note_id)
            .await?
            .ok_or_else(|| not_found("note", note_id))?;
        ensure_revision(note.revision, expected_revision)?;
        let validated = Note::new(note.person_id, content, note.source.clone(), now)?;
        note.content = validated.content;
        note.updated_at = now;
        note.revision = note.revision.next();
        self.store.put_note(&note).await?;
        Ok(note)
    }

    pub async fn delete_item(
        &self,
        reference: DomainRef,
        expected_revision: Revision,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        match reference {
            DomainRef::Event(id) => {
                let mut value = self
                    .store
                    .get_event(id)
                    .await?
                    .ok_or_else(|| not_found("event", id))?;
                ensure_revision(value.revision, expected_revision)?;
                ensure_local_event(&value)?;
                value.deleted_at = Some(now);
                value.updated_at = now;
                value.revision = value.revision.next();
                self.store.put_event(&value).await?;
            }
            DomainRef::Task(id) => {
                let mut value = self
                    .store
                    .get_task(id)
                    .await?
                    .ok_or_else(|| not_found("task", id))?;
                ensure_revision(value.revision, expected_revision)?;
                value.deleted_at = Some(now);
                value.updated_at = now;
                value.revision = value.revision.next();
                self.store.put_task(&value).await?;
            }
            DomainRef::Note(id) => {
                let mut value = self
                    .store
                    .get_note(id)
                    .await?
                    .ok_or_else(|| not_found("note", id))?;
                ensure_revision(value.revision, expected_revision)?;
                value.deleted_at = Some(now);
                value.updated_at = now;
                value.revision = value.revision.next();
                self.store.put_note(&value).await?;
            }
        }
        Ok(())
    }

    pub async fn day_snapshot(
        &self,
        person_id: PersonId,
        date: NaiveDate,
        timezone_offset_seconds: i32,
        now: DateTime<Utc>,
    ) -> Result<DaySnapshot, CoreError> {
        let mirror = self.store.calendar_mirror(person_id).await?;
        let mut events = self.store.list_events(person_id).await?;
        if let Some(mirror) = &mirror {
            events.extend(mirror.events.clone());
        }
        let mut snapshot = project_day(
            person_id,
            date,
            timezone_offset_seconds,
            now,
            events,
            self.store.list_tasks(person_id).await?,
            self.store.list_notes(person_id).await?,
        );
        snapshot.calendar = mirror.map(|mirror| mirror.connection);
        Ok(snapshot)
    }
}

fn ensure_revision(actual: Revision, expected: Revision) -> Result<(), CoreError> {
    if actual == expected {
        Ok(())
    } else {
        Err(CoreError::new(ErrorCode::Conflict, "stale revision")
            .with_metadata("expected", expected.0.to_string())
            .with_metadata("actual", actual.0.to_string()))
    }
}

fn ensure_local_event(event: &Event) -> Result<(), CoreError> {
    if matches!(
        event.source,
        SourceRef::Calendar(_) | SourceRef::External(_)
    ) {
        return Err(CoreError::new(
            ErrorCode::Validation,
            "external calendar events are read-only",
        ));
    }
    Ok(())
}

fn not_found(kind: &str, id: impl std::fmt::Display) -> CoreError {
    CoreError::new(ErrorCode::NotFound, format!("{kind} not found"))
        .with_metadata("id", id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[tokio::test]
    async fn capture_classification_persists_across_reopen() {
        let path = std::env::temp_dir().join(format!("floe-{}.db", uuid::Uuid::new_v4()));
        let person_id = PersonId::new();
        let now = Utc.with_ymd_and_hms(2026, 9, 2, 9, 0, 0).unwrap();
        let core = FloeCore::open(&path).await.unwrap();
        let capture = core
            .submit_capture(person_id, "Buy milk", now)
            .await
            .unwrap();
        let item = core
            .classify_capture(
                capture.id,
                capture.revision,
                Classification::Task {
                    title: "Buy milk".into(),
                    deadline: None,
                    priority: Priority::Normal,
                },
                now,
            )
            .await
            .unwrap();
        drop(core);
        let core = FloeCore::open(&path).await.unwrap();
        let snapshot = core
            .day_snapshot(person_id, now.date_naive(), 0, now)
            .await
            .unwrap();
        assert_eq!(snapshot.items, vec![item]);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn stale_classification_does_not_create_an_item() {
        let path = std::env::temp_dir().join(format!("floe-{}.db", uuid::Uuid::new_v4()));
        let person_id = PersonId::new();
        let now = Utc.with_ymd_and_hms(2026, 9, 2, 9, 0, 0).unwrap();
        let core = FloeCore::open(&path).await.unwrap();
        let capture = core
            .submit_capture(person_id, "Remember this", now)
            .await
            .unwrap();
        let error = core
            .classify_capture(
                capture.id,
                Revision(99),
                Classification::Note {
                    content: "Remember this".into(),
                },
                now,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Conflict);
        assert!(
            core.day_snapshot(person_id, now.date_naive(), 0, now)
                .await
                .unwrap()
                .items
                .is_empty()
        );
        let _ = std::fs::remove_file(path);
    }
}
