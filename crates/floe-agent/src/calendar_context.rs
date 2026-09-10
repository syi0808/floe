use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{AGENT_VERSION, AgentFailure, ContextEvidence, DataClass};

pub const CALENDAR_CONTEXT_VIEW_ID: &str = "calendar.timeline";
pub const MAX_CALENDAR_CONTEXT_ITEMS: usize = 128;
pub const MAX_CALENDAR_CONTEXT_BYTES: usize = 65_536;
const MAX_CALENDAR_CONTEXT_FRESHNESS_MS: i64 = 300_000;
const MAX_CALENDAR_CONTEXT_RANGE_MS: i64 = 32 * 86_400_000;

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
