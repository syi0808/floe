use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{Event, EventSchedule};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarProvider {
    Fixture,
    EventKit,
    #[serde(rename = "google_calendar")]
    Google,
    #[serde(rename = "microsoft_calendar")]
    Microsoft,
    Android,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSource {
    pub can_modify: bool,
    pub provider: CalendarProvider,
    pub calendar_id: String,
    pub calendar_name: String,
    pub external_id: String,
    pub external_revision: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarFailure {
    PermissionDenied,
    CalendarUnavailable,
    ProviderUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarRange {
    pub start_date: NaiveDate,
    pub end_date_exclusive: NaiveDate,
    pub timezone_offset_seconds: i32,
    pub end_timezone_offset_seconds: Option<i32>,
}

impl CalendarRange {
    pub fn is_valid(&self) -> bool {
        let days = (self.end_date_exclusive - self.start_date).num_days();
        (1..=31).contains(&days)
            && self.timezone_offset_seconds.unsigned_abs() < 86_400
            && self
                .end_timezone_offset_seconds
                .unwrap_or(self.timezone_offset_seconds)
                .unsigned_abs()
                < 86_400
            && days * 86_400 + i64::from(self.timezone_offset_seconds)
                - i64::from(
                    self.end_timezone_offset_seconds
                        .unwrap_or(self.timezone_offset_seconds),
                )
                > 0
    }

    pub fn contains(&self, schedule: &EventSchedule) -> bool {
        match schedule {
            EventSchedule::AllDay(value) => {
                value.start_date < self.end_date_exclusive
                    && value.end_date_exclusive > self.start_date
            }
            EventSchedule::Timed(value) => {
                let offset = Duration::seconds(i64::from(self.timezone_offset_seconds));
                let start = self.start_date.and_hms_opt(0, 0, 0).unwrap().and_utc() - offset;
                let end = self
                    .end_date_exclusive
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
                    - Duration::seconds(i64::from(
                        self.end_timezone_offset_seconds
                            .unwrap_or(self.timezone_offset_seconds),
                    ));
                value.starts_at < end && value.ends_at > start
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSelection {
    pub calendar_id: String,
    pub calendar_name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarScope {
    Selected,
    All,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarConnection {
    pub connection_id: String,
    pub device_id: String,
    pub disconnected: bool,
    pub scope: CalendarScope,
    pub provider: CalendarProvider,
    pub calendars: Vec<CalendarSelection>,
    pub revision: u64,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_range: Option<CalendarRange>,
    pub error: Option<CalendarFailure>,
    pub error_at: Option<DateTime<Utc>>,
    pub source_statuses: BTreeMap<String, CalendarSyncStatus>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSyncStatus {
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_range: Option<CalendarRange>,
    pub error: Option<CalendarFailure>,
    pub error_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CalendarBatch {
    pub calendar_id: String,
    pub records: Vec<CalendarRecord>,
    pub failure: Option<CalendarFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarMirror {
    pub connection: CalendarConnection,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CalendarRecord {
    pub can_modify: bool,
    pub calendar_id: String,
    pub external_id: String,
    pub external_revision: String,
    pub title: String,
    pub schedule: EventSchedule,
}
