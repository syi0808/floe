use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use floe_context_contract::SourceAuthority;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarProviderDto {
    Fixture,
    EventKit,
    #[serde(rename = "google_calendar")]
    Google,
    #[serde(rename = "microsoft_calendar")]
    Microsoft,
    Android,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSourceDto {
    pub can_modify: bool,
    pub provider: CalendarProviderDto,
    pub calendar_id: String,
    pub calendar_name: String,
    pub external_id: String,
    pub external_revision: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarFailureDto {
    PermissionDenied,
    CalendarUnavailable,
    ProviderUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarRangeDto {
    pub start_date: NaiveDate,
    pub end_date_exclusive: NaiveDate,
    pub timezone_offset_seconds: i32,
    pub end_timezone_offset_seconds: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSelectionDto {
    pub calendar_id: String,
    pub calendar_name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarScopeDto {
    Selected,
    All,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarConnectionDto {
    pub connection_id: String,
    pub device_id: String,
    pub disconnected: bool,
    pub scope: CalendarScopeDto,
    pub provider: CalendarProviderDto,
    pub calendars: Vec<CalendarSelectionDto>,
    pub revision: u64,
    pub source_authority: SourceAuthority,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_range: Option<CalendarRangeDto>,
    pub error: Option<CalendarFailureDto>,
    pub error_at: Option<DateTime<Utc>>,
    pub source_statuses: BTreeMap<String, CalendarSyncStatusDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSyncStatusDto {
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_range: Option<CalendarRangeDto>,
    pub error: Option<CalendarFailureDto>,
    pub error_at: Option<DateTime<Utc>>,
}
