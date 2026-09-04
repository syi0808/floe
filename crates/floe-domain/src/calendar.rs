use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::{Event, EventSchedule};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarProvider {
    Fixture,
    EventKit,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSource {
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
}

impl CalendarRange {
    pub fn is_valid(&self) -> bool {
        let days = (self.end_date_exclusive - self.start_date).num_days();
        (1..=31).contains(&days) && self.timezone_offset_seconds.unsigned_abs() <= 86_400
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
                    - offset;
                value.starts_at < end && value.ends_at > start
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarConnection {
    pub provider: CalendarProvider,
    pub calendar_id: String,
    pub calendar_name: String,
    pub revision: u64,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_range: Option<CalendarRange>,
    pub error: Option<CalendarFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarMirror {
    pub connection: CalendarConnection,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug)]
pub struct CalendarRecord {
    pub external_id: String,
    pub external_revision: String,
    pub title: String,
    pub schedule: EventSchedule,
}
