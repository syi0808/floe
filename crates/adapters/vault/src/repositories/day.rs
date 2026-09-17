//! Day-owned timeline and calendar-mirror persistence.

use crate::{StoreError, StoreErrorCode, TursoStore};

impl floe_day::TimelineRepository for TursoStore {
    async fn put_capture(&self, value: &floe_day::Capture) -> Result<(), floe_day::DayError> {
        TursoStore::put_capture(self, value)
            .await
            .map_err(day_error)
    }

    async fn put_event(&self, value: &floe_day::Event) -> Result<(), floe_day::DayError> {
        TursoStore::put_event(self, value)
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
        TursoStore::put_task(self, value)
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
        TursoStore::put_note(self, value)
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
        TursoStore::get_capture(self, id)
            .await
            .map_err(day_error)
    }

    async fn get_event(
        &self,
        id: floe_day::EventId,
    ) -> Result<Option<floe_day::Event>, floe_day::DayError> {
        TursoStore::get_event(self, id)
            .await
            .map_err(day_error)
    }

    async fn get_task(
        &self,
        id: floe_day::TaskId,
    ) -> Result<Option<floe_day::Task>, floe_day::DayError> {
        TursoStore::get_task(self, id)
            .await
            .map_err(day_error)
    }

    async fn get_note(
        &self,
        id: floe_day::NoteId,
    ) -> Result<Option<floe_day::Note>, floe_day::DayError> {
        TursoStore::get_note(self, id)
            .await
            .map_err(day_error)
    }

    async fn list_events(
        &self,
        person_id: floe_day::PersonId,
    ) -> Result<Vec<floe_day::Event>, floe_day::DayError> {
        TursoStore::list_events(self, person_id)
            .await
            .map_err(day_error)
    }

    async fn list_tasks(
        &self,
        person_id: floe_day::PersonId,
    ) -> Result<Vec<floe_day::Task>, floe_day::DayError> {
        TursoStore::list_tasks(self, person_id)
            .await
            .map_err(day_error)
    }

    async fn list_notes(
        &self,
        person_id: floe_day::PersonId,
    ) -> Result<Vec<floe_day::Note>, floe_day::DayError> {
        TursoStore::list_notes(self, person_id)
            .await
            .map_err(day_error)
    }

    async fn classify(
        &self,
        capture: &floe_day::Capture,
        item: &floe_day::TimelineItem,
    ) -> Result<(), floe_day::DayError> {
        TursoStore::classify(self, capture, item)
            .await
            .map_err(day_error)
    }

    async fn calendar_mirror(
        &self,
        person_id: floe_day::PersonId,
    ) -> Result<Option<floe_day::CalendarMirror>, floe_day::DayError> {
        TursoStore::calendar_mirror(self, person_id)
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

fn day_error(error: StoreError) -> floe_day::DayError {
    let code = match error.code {
        StoreErrorCode::Validation => floe_day::DayErrorCode::Validation,
        StoreErrorCode::NotFound => floe_day::DayErrorCode::NotFound,
        StoreErrorCode::Conflict => floe_day::DayErrorCode::Conflict,
        _ => floe_day::DayErrorCode::Storage,
    };
    let mut result = floe_day::DayError::new(code, error.message);
    for (key, value) in error.metadata {
        result = result.with_metadata(key, value);
    }
    result
}

/// The stored calendar mirror, bounded, as Context's timeline read asks for it.
impl floe_context::CalendarMirrorReader for crate::TursoStore {
    async fn bounded_calendar_mirror(
        &self,
        person_id: floe_kernel::PersonId,
    ) -> Result<floe_day::CalendarMirror, floe_agent_contract::AgentFailure> {
        crate::TursoStore::bounded_calendar_mirror(self, person_id).await
    }
}
