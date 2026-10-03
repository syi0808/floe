use chrono::{DateTime, Duration, NaiveDate, Utc};
use floe_context_contract::CalendarProvider;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{Event, EventSchedule};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSource {
    pub can_modify: bool,
    pub connection_id: floe_context_contract::ConnectionId,
    pub provider: CalendarProvider,
    pub calendar_id: String,
    pub calendar_name: String,
    pub external_id: String,
    pub external_revision: CalendarExternalRevision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CalendarExternalRevision {
    ProviderOpaque(String),
    ObservationFingerprint([u8; 32]),
}
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ExternalRevisionWire { ProviderOpaque { value: String }, ObservationFingerprint { sha256: String } }
impl Serialize for CalendarExternalRevision {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = match self { Self::ProviderOpaque(value) => ExternalRevisionWire::ProviderOpaque { value: value.clone() }, Self::ObservationFingerprint(digest) => ExternalRevisionWire::ObservationFingerprint { sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect() } };
        value.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for CalendarExternalRevision {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = match ExternalRevisionWire::deserialize(deserializer)? { ExternalRevisionWire::ProviderOpaque { value } => Self::ProviderOpaque(value), ExternalRevisionWire::ObservationFingerprint { sha256 } => {
            if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) { return Err(serde::de::Error::custom("invalid observation fingerprint")); }
            let mut digest = [0; 32]; for (index, byte) in digest.iter_mut().enumerate() { *byte = u8::from_str_radix(&sha256[index * 2..index * 2 + 2], 16).map_err(serde::de::Error::custom)?; } Self::ObservationFingerprint(digest)
        } };
        if !value.is_valid() { return Err(serde::de::Error::custom("invalid external revision")); } Ok(value)
    }
}
impl CalendarExternalRevision {
    pub fn from_observation_fingerprint_hex(value: &str) -> Option<Self> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) { return None; }
        let mut digest = [0; 32]; for (index, byte) in digest.iter_mut().enumerate() { *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?; }
        let revision = Self::ObservationFingerprint(digest); revision.is_valid().then_some(revision)
    }
    pub fn is_valid(&self) -> bool { match self { Self::ProviderOpaque(value) => !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control), Self::ObservationFingerprint(value) => *value != [0; 32] } }
    pub fn provider_precondition(&self) -> Option<&str> { match self { Self::ProviderOpaque(value) if self.is_valid() => Some(value), _ => None } }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarFailure {
    PermissionDenied,
    CalendarUnavailable,
    ProviderUnavailable,
    SourceChanged,
    SourceFenced,
    VaultLocked,
    BudgetExceeded,
    DeadlineExceeded,
    Cancelled,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarMirrorState {
    pub sources: Vec<CalendarMirrorSourceState>,
}
impl CalendarMirrorState {
    pub fn source_state(&self, connection_id: &str) -> Option<&CalendarMirrorSourceState> { self.sources.iter().find(|state| state.source.source.connection_id().as_str() == connection_id) }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarMirrorSourceState {
    pub source: super::refresh::CalendarSourceVersion,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_range: Option<CalendarRange>,
    pub error: Option<CalendarFailure>,
    pub error_at: Option<DateTime<Utc>>,
    pub calendar_statuses: BTreeMap<String, CalendarSyncStatus>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSyncStatus {
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_range: Option<CalendarRange>,
    pub error: Option<CalendarFailure>,
    pub error_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarBatch {
    pub calendar_id: String,
    pub records: Vec<CalendarRecord>,
    pub failure: Option<CalendarFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarMirror {
    pub mirror_revision: u64,
    pub state: CalendarMirrorState,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarRecord {
    pub can_modify: bool,
    pub calendar_id: String,
    pub external_id: String,
    pub external_revision: CalendarExternalRevision,
    pub title: String,
    pub schedule: EventSchedule,
}
