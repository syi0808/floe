//! Calendar timeline grants: what a bounded calendar read is allowed to cover.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, DataClass};
use floe_kernel::PersonId;
use uuid::Uuid;

use crate::{CalendarProvider, CalendarRange};

/// The longest span a single timeline grant may cover.
pub const MAX_TIMELINE_GRANT_DAYS: i64 = 31;

#[derive(Clone)]
pub struct CalendarTimelineGrant {
    pub person_id: PersonId,
    pub handle: Uuid,
    pub provider: CalendarProvider,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_revision: u64,
    pub day: CalendarRange,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl CalendarTimelineGrant {
    pub fn data_class(&self) -> DataClass {
        match self.provider {
            CalendarProvider::Fixture => DataClass::Synthetic,
            CalendarProvider::EventKit
            | CalendarProvider::Google
            | CalendarProvider::Microsoft
            | CalendarProvider::Android => DataClass::Personal,
        }
    }

    fn validate(&self, now: DateTime<Utc>) -> Result<(), AgentFailure> {
        let identifiers: HashSet<_> = self.calendar_ids.iter().collect();
        let (day_start, day_end) = range_bounds(&self.day)?;
        let range_days = (self.day.end_date_exclusive - self.day.start_date).num_days();
        if !(1..=MAX_TIMELINE_GRANT_DAYS).contains(&range_days)
            || self.device_id.trim().is_empty()
            || self.device_id.len() > 128
            || self.calendar_ids.is_empty()
            || self.calendar_ids.len() > 4
            || identifiers.len() != self.calendar_ids.len()
            || self
                .calendar_ids
                .iter()
                .any(|identifier| identifier.trim().is_empty() || identifier.len() > 512)
            || self.starts_at < day_start
            || self.ends_at > day_end
            || self.starts_at >= self.ends_at
            || self.expires_at - now > chrono::Duration::minutes(5)
        {
            return Err(AgentFailure::InvalidInput);
        }
        if self.expires_at <= now {
            return Err(AgentFailure::StaleContext);
        }
        Ok(())
    }
}

/// The UTC interval a calendar range covers, honouring its timezone offsets.
pub fn range_bounds(range: &CalendarRange) -> Result<(DateTime<Utc>, DateTime<Utc>), AgentFailure> {
    if !range.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let start = range
        .start_date
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            range.timezone_offset_seconds,
        )))
        .ok_or(AgentFailure::InvalidInput)?;
    let end = range
        .end_date_exclusive
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        .checked_sub_signed(chrono::Duration::seconds(i64::from(
            range
                .end_timezone_offset_seconds
                .unwrap_or(range.timezone_offset_seconds),
        )))
        .ok_or(AgentFailure::InvalidInput)?;
    Ok((start, end))
}
