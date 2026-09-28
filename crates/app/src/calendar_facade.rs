use chrono::{DateTime, Utc};
use floe_connections::{ConnectionId, SourceState};
use floe_context_contract::CalendarProvider;
use floe_day::{
    CalendarBatch, CalendarFailure, CalendarMirrorInput, CalendarRange, CalendarRecord,
    CalendarSelection,
};
use floe_kernel::PersonId;

use crate::{CoreError, ErrorCode, FloeCore, connection_services::source_error};

impl FloeCore {
    async fn calendar_mirror_input(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
        execution_owner_id: &str,
    ) -> Result<CalendarMirrorInput, CoreError> {
        let source = self
            .source_service()
            .load(person_id, connection_id)
            .await
            .map_err(source_error)?
            .ok_or_else(|| CoreError::new(ErrorCode::NotFound, "Calendar source not found"))?;
        if source.execution_owner_id().as_str() != execution_owner_id {
            return Err(CoreError::new(
                ErrorCode::NotFound,
                "Calendar source not found",
            ));
        }
        if source.state() == SourceState::Disconnected {
            return Err(CoreError::new(
                ErrorCode::Conflict,
                "Calendar source is disconnected",
            ));
        }
        let provider = match source.connector_id().as_str() {
            "calendar.fixture" => CalendarProvider::Fixture,
            "calendar.event_kit" => CalendarProvider::EventKit,
            "calendar.google" => CalendarProvider::Google,
            "calendar.microsoft" => CalendarProvider::Microsoft,
            "calendar.android" => CalendarProvider::Android,
            _ => {
                return Err(CoreError::new(
                    ErrorCode::Validation,
                    "source is not a Calendar connector",
                ));
            }
        };
        Ok(CalendarMirrorInput {
            source_connection_id: source.connection_id().as_str().into(),
            provider,
            calendars: source
                .resources()
                .iter()
                .map(|resource| CalendarSelection {
                    calendar_id: resource.handle().as_str().into(),
                    calendar_name: resource.label().into(),
                })
                .collect(),
        })
    }

    pub async fn record_calendar_failure(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
        execution_owner_id: &str,
        expected_mirror_revision: Option<u64>,
        failure: CalendarFailure,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let input = self
            .calendar_mirror_input(person_id, connection_id, execution_owner_id)
            .await?;
        self.day_service()
            .record_calendar_failure(person_id, expected_mirror_revision, input, failure, now)
            .await
            .map_err(crate::core::day_error)
    }

    pub async fn import_calendar(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
        execution_owner_id: &str,
        expected_mirror_revision: Option<u64>,
        range: CalendarRange,
        records: Vec<CalendarRecord>,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let input = self
            .calendar_mirror_input(person_id, connection_id, execution_owner_id)
            .await?;
        self.day_service()
            .import_calendar(
                person_id,
                expected_mirror_revision,
                input,
                range,
                records,
                now,
            )
            .await
            .map_err(crate::core::day_error)
    }

    pub async fn import_calendar_sources(
        &self,
        person_id: PersonId,
        connection_id: &ConnectionId,
        execution_owner_id: &str,
        expected_mirror_revision: Option<u64>,
        range: CalendarRange,
        batches: Vec<CalendarBatch>,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let input = self
            .calendar_mirror_input(person_id, connection_id, execution_owner_id)
            .await?;
        self.day_service()
            .import_calendar_sources(
                person_id,
                expected_mirror_revision,
                input,
                range,
                batches,
                now,
            )
            .await
            .map_err(crate::core::day_error)
    }
}
