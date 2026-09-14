use chrono::{DateTime, NaiveDate, Utc};
use floe_context::SourceLeaseRegistry;
use floe_domain::*;
use std::sync::Arc;

use crate::{CoreError, ErrorCode, TursoStore};

pub struct FloeCore {
    pub(crate) store: TursoStore,
    pub(crate) lease_registry: Arc<SourceLeaseRegistry>,
}

pub use floe_day::Classification;

impl FloeCore {
    pub async fn open(path: impl AsRef<std::path::Path>) -> Result<Self, CoreError> {
        Ok(Self {
            store: TursoStore::open(path).await?,
            lease_registry: Arc::new(SourceLeaseRegistry::new()),
        })
    }

    pub fn day_service(&self) -> floe_day::DayService<'_, TursoStore> {
        floe_day::DayService::new(&self.store)
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

    #[tokio::test]
    async fn stale_task_write_is_rejected_by_repository_cas() {
        let path = std::env::temp_dir().join(format!("floe-{}.db", uuid::Uuid::new_v4()));
        let person_id = PersonId::new();
        let now = Utc.with_ymd_and_hms(2026, 9, 2, 9, 0, 0).unwrap();
        let core = FloeCore::open(&path).await.unwrap();
        let task = core
            .create_task(person_id, "CAS", None, Priority::Normal, now)
            .await
            .unwrap();
        let mut completed = task.clone();
        completed.complete(now);
        let mut reopened = task.clone();
        reopened.reopen(now);
        floe_day::TimelineRepository::put_task_if_revision(&core.store, &completed, task.revision)
            .await
            .unwrap();
        let error = floe_day::TimelineRepository::put_task_if_revision(
            &core.store,
            &reopened,
            task.revision,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, floe_day::DayErrorCode::Conflict);
        assert_eq!(
            error.metadata.get("expected"),
            Some(&task.revision.0.to_string())
        );
        assert_eq!(
            error.metadata.get("actual"),
            Some(&completed.revision.0.to_string())
        );
        let stored = core.store.get_task(task.id).await.unwrap().unwrap();
        assert_eq!(stored.completed_at, completed.completed_at);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn stale_observation_identity_is_rejected_at_day_boundary() {
        let path = std::env::temp_dir().join(format!("floe-{}.db", uuid::Uuid::new_v4()));
        let person_id = PersonId::new();
        let now = Utc.with_ymd_and_hms(2026, 9, 2, 9, 0, 0).unwrap();
        let core = FloeCore::open(&path).await.unwrap();
        core.select_calendar(
            person_id,
            CalendarProvider::Fixture,
            "home".into(),
            "Home".into(),
        )
        .await
        .unwrap();
        let connection = core.calendar_connection(person_id).await.unwrap().unwrap();
        let error = core
            .day_service()
            .apply_observation(
                person_id,
                floe_day::CalendarObservation {
                    connection_id: "calendar.fixture.rebound".into(),
                    provider: connection.provider,
                    source_authority: connection.source_authority,
                    revision: connection.revision,
                    range: CalendarRange {
                        start_date: now.date_naive(),
                        end_date_exclusive: now.date_naive().succ_opt().unwrap(),
                        timezone_offset_seconds: 0,
                        end_timezone_offset_seconds: None,
                    },
                    batches: vec![CalendarBatch {
                        calendar_id: "home".into(),
                        records: vec![],
                        failure: None,
                    }],
                },
                now,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, floe_day::DayErrorCode::Conflict);
        assert_eq!(
            core.calendar_connection(person_id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            connection.revision
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn competing_capture_classification_cannot_leave_an_orphan_item() {
        let path = std::env::temp_dir().join(format!("floe-{}.db", uuid::Uuid::new_v4()));
        let person_id = PersonId::new();
        let now = Utc.with_ymd_and_hms(2026, 9, 2, 9, 0, 0).unwrap();
        let core = FloeCore::open(&path).await.unwrap();
        let capture = core
            .submit_capture(person_id, "classify", now)
            .await
            .unwrap();
        let first = Task::new(
            person_id,
            "first",
            None,
            Priority::Normal,
            SourceRef::Capture(capture.id),
            now,
        )
        .unwrap();
        let second = Task::new(
            person_id,
            "second",
            None,
            Priority::Normal,
            SourceRef::Capture(capture.id),
            now,
        )
        .unwrap();
        let mut first_capture = capture.clone();
        first_capture.classify(DomainRef::Task(first.id), now);
        let mut second_capture = capture.clone();
        second_capture.classify(DomainRef::Task(second.id), now);
        floe_day::TimelineRepository::classify(
            &core.store,
            &first_capture,
            &TimelineItem::Task(first),
        )
        .await
        .unwrap();
        let error = floe_day::TimelineRepository::classify(
            &core.store,
            &second_capture,
            &TimelineItem::Task(second),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, floe_day::DayErrorCode::Conflict);
        assert_eq!(error.metadata.get("expected"), Some(&"0".to_owned()));
        assert_eq!(error.metadata.get("actual"), Some(&"1".to_owned()));
        assert_eq!(core.store.list_tasks(person_id).await.unwrap().len(), 1);
        let _ = std::fs::remove_file(path);
    }
}
