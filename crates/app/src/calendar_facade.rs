use chrono::{DateTime, Utc};

use floe_day::{
    CalendarBatch, CalendarConnection, CalendarFailure, CalendarProvider, CalendarRange,
    CalendarRecord, CalendarScope,
    CalendarSelection, CalendarSource, CalendarSyncStatus,
};
use floe_kernel::PersonId;

use crate::{CoreError, FloeCore};

impl FloeCore {
    pub async fn calendar_connection(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarConnection>, CoreError> {
        self.day_service()
            .calendar_connection(person_id)
            .await
            .map_err(crate::core::day_error)
    }

    pub async fn select_calendar(
        &self,
        person_id: PersonId,
        provider: CalendarProvider,
        calendar_id: String,
        calendar_name: String,
    ) -> Result<(), CoreError> {
        self.day_service()
            .select_calendar(person_id, provider, calendar_id, calendar_name)
            .await
            .map_err(crate::core::day_error)
    }

    pub async fn select_calendars(
        &self,
        person_id: PersonId,
        provider: CalendarProvider,
        calendars: Vec<CalendarSelection>,
    ) -> Result<(), CoreError> {
        self.day_service()
            .select_calendars(person_id, provider, calendars)
            .await
            .map_err(crate::core::day_error)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn set_calendar_scope(
        &self,
        person_id: PersonId,
        connection_id: String,
        connection_revision: u64,
        device_id: String,
        provider: CalendarProvider,
        calendars: Vec<CalendarSelection>,
        scope: CalendarScope,
    ) -> Result<(), CoreError> {
        self.day_service()
            .set_calendar_scope(
                person_id,
                connection_id,
                connection_revision,
                device_id,
                provider,
                calendars,
                scope,
            )
            .await
            .map_err(crate::core::day_error)
    }

    pub async fn disconnect_calendar(
        &self,
        person_id: PersonId,
        expected_revision: u64,
    ) -> Result<(), CoreError> {
        self.day_service()
            .disconnect_calendar(person_id, expected_revision)
            .await
            .map_err(crate::core::day_error)
    }

    pub async fn discover_calendars(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        calendars: Vec<CalendarSelection>,
    ) -> Result<(), CoreError> {
        self.day_service()
            .discover_calendars(person_id, expected_revision, calendars)
            .await
            .map_err(crate::core::day_error)
    }

    pub async fn record_calendar_failure(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        failure: CalendarFailure,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        self.day_service()
            .record_calendar_failure(person_id, expected_revision, failure, now)
            .await
            .map_err(crate::core::day_error)
    }

    pub async fn import_calendar(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        range: CalendarRange,
        records: Vec<CalendarRecord>,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        self.day_service()
            .import_calendar(person_id, expected_revision, range, records, now)
            .await
            .map_err(crate::core::day_error)
    }

    pub async fn import_calendar_sources(
        &self,
        person_id: PersonId,
        expected_revision: u64,
        range: CalendarRange,
        batches: Vec<CalendarBatch>,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        self.day_service()
            .import_calendar_sources(person_id, expected_revision, range, batches, now)
            .await
            .map_err(crate::core::day_error)
    }
}
