use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use floe_kernel::AgentFailure;

use crate::ContextEvidence;
use crate::DataClass;
use floe_kernel::AGENT_VERSION;

pub const CALENDAR_CONTEXT_VIEW_ID: &str = "calendar.timeline";
pub const MAX_CALENDAR_CONTEXT_ITEMS: usize = 128;
pub const MAX_CALENDAR_CONTEXT_BYTES: usize = 65_536;
const MAX_CALENDAR_CONTEXT_FRESHNESS_MS: i64 = 300_000;
const MAX_CALENDAR_CONTEXT_RANGE_MS: i64 = 32 * 86_400_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarViewQuery {
    range_start_unix_ms: i64,
    range_end_unix_ms: i64,
    cursor: Option<String>,
    limit: usize,
}

#[cfg(test)]
mod query_tests {
    use super::*;

    #[test]
    fn query_preserves_request_scoped_range_and_bounds() {
        let today = CalendarViewQuery::try_new(1_000, 86_401_000, None, 128).unwrap();
        let week = CalendarViewQuery::try_new(1_000, 604_801_000, None, 128).unwrap();
        assert_ne!(today.range_end_unix_ms(), week.range_end_unix_ms());
        assert!(CalendarViewQuery::try_new(1_000, 1_000, None, 128).is_err());
        assert!(CalendarViewQuery::try_new(1_000, 33 * 86_400_000, None, 128).is_err());
        assert!(CalendarViewQuery::try_new(1_000, 2_000, None, 129).is_err());
        assert!(CalendarViewQuery::try_new(1_000, 2_000, Some("".into()), 1).is_err());
    }
}

impl CalendarViewQuery {
    pub fn try_new(
        range_start_unix_ms: i64,
        range_end_unix_ms: i64,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Self, AgentFailure> {
        let query = Self {
            range_start_unix_ms,
            range_end_unix_ms,
            cursor,
            limit,
        };
        query.validate()?;
        Ok(query)
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.range_start_unix_ms < 0
            || self.range_end_unix_ms <= self.range_start_unix_ms
            || self.range_end_unix_ms - self.range_start_unix_ms > MAX_CALENDAR_CONTEXT_RANGE_MS
            || !(1..=MAX_CALENDAR_CONTEXT_ITEMS).contains(&self.limit)
            || self.cursor.as_ref().is_some_and(|cursor| {
                cursor.trim().is_empty()
                    || cursor.len() > 2048
                    || cursor.chars().any(char::is_control)
            })
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn range_start_unix_ms(&self) -> i64 {
        self.range_start_unix_ms
    }

    pub fn range_end_unix_ms(&self) -> i64 {
        self.range_end_unix_ms
    }

    pub fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    pub fn limit(&self) -> usize {
        self.limit
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarContextItem {
    pub evidence_handle: String,
    pub untrusted_title: String,
    pub starts_at_unix_ms: i64,
    pub ends_at_unix_ms: i64,
    pub all_day: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarContextView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub range_start_unix_ms: i64,
    pub range_end_unix_ms: i64,
    pub coverage_complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub items: Vec<CalendarContextItem>,
}

pub fn validate_calendar_context_view(
    view: &CalendarContextView,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    if view.schema_version != AGENT_VERSION
        || view.view_id != CALENDAR_CONTEXT_VIEW_ID
        || !valid_handle(&view.source_handle)
        || view.observed_at_unix_ms > now_unix_ms
        || view.expires_at_unix_ms <= now_unix_ms
        || view.expires_at_unix_ms <= view.observed_at_unix_ms
        || view.expires_at_unix_ms - view.observed_at_unix_ms > MAX_CALENDAR_CONTEXT_FRESHNESS_MS
        || view.range_start_unix_ms < 0
        || view.range_end_unix_ms <= view.range_start_unix_ms
        || view.range_end_unix_ms - view.range_start_unix_ms > MAX_CALENDAR_CONTEXT_RANGE_MS
        || view.coverage_complete == view.next_cursor.is_some()
        || view.next_cursor.as_ref().is_some_and(|cursor| {
            cursor.trim().is_empty()
                || cursor.len() > 2048
                || cursor.chars().any(|character| character.is_control())
        })
        || view.items.len() > MAX_CALENDAR_CONTEXT_ITEMS
        || serde_json::to_vec(view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > MAX_CALENDAR_CONTEXT_BYTES
    {
        return Err(AgentFailure::InvalidInput);
    }
    let mut handles = HashSet::new();
    for item in &view.items {
        if !valid_handle(&item.evidence_handle)
            || !handles.insert(item.evidence_handle.as_str())
            || item.untrusted_title.len() > 1024
            || item.starts_at_unix_ms < 0
            || item.ends_at_unix_ms <= item.starts_at_unix_ms
            || item.starts_at_unix_ms >= view.range_end_unix_ms
            || item.ends_at_unix_ms <= view.range_start_unix_ms
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    Ok(())
}

pub fn calendar_context_evidence(
    view: &CalendarContextView,
) -> Result<ContextEvidence, AgentFailure> {
    Ok(ContextEvidence {
        source_handle: view.source_handle.clone(),
        data_class: DataClass::Personal,
        untrusted_text: serde_json::to_string(&view.items)
            .map_err(|_| AgentFailure::InvalidInput)?,
        expires_at_unix_ms: u64::try_from(view.expires_at_unix_ms)
            .map_err(|_| AgentFailure::InvalidInput)?,
    })
}

fn valid_handle(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}

pub const MAX_TIMELINE_VIEW_DAYS: i64 = 31;
pub const MAX_TIMELINE_VIEW_ITEMS: usize = 128;
pub const MAX_TIMELINE_VIEW_BYTES: usize = 65_536;

/// One bounded event an Expert may read out of a timeline view.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineViewItem {
    pub evidence_handle: uuid::Uuid,
    pub untrusted_title: String,
    pub starts_at_unix_ms: u64,
    pub ends_at_unix_ms: u64,
}

/// The bounded timeline an Expert reads once under a single grant.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertTimelineView {
    pub schema_version: u32,
    pub handle: uuid::Uuid,
    pub person_id: floe_kernel::PersonId,
    pub data_class: DataClass,
    pub source_handle: String,
    pub range_start_unix_ms: u64,
    pub range_end_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub coverage_complete: bool,
    pub next_cursor: Option<String>,
    pub items: Vec<TimelineViewItem>,
}
