use serde::{Deserialize, Serialize};

use crate::{AGENT_VERSION, AgentFailure, ContextEvidence, DataClass};

pub const WORK_CONTEXT_VIEW_ID: &str = "work.context";
pub const LOGISTICS_VIEW_ID: &str = "life.logistics";
pub const MAX_PORTFOLIO_VIEW_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemKind {
    SelectedFile,
    Project,
    MeetingDecision,
    Communication,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkContextItem {
    pub evidence_handle: String,
    pub kind: WorkItemKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    pub observed_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkContextView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub coverage_complete: bool,
    pub scope_handle: String,
    pub items: Vec<WorkContextItem>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogisticsItemKind {
    Reservation,
    Travel,
    Delivery,
    Errand,
    HomeState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogisticsItem {
    pub evidence_handle: String,
    pub kind: LogisticsItemKind,
    pub summary: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurs_at_unix_ms: Option<i64>,
    pub needs_attention: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogisticsView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub coverage_complete: bool,
    pub items: Vec<LogisticsItem>,
}

pub fn validate_work_context_view(
    view: &WorkContextView,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    validate_envelope(
        view.schema_version,
        &view.view_id,
        WORK_CONTEXT_VIEW_ID,
        &view.source_handle,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
        now_unix_ms,
    )?;
    if !valid_handle(&view.scope_handle) {
        return Err(AgentFailure::InvalidInput);
    }
    if view.items.len() > 64 {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, item) in view.items.iter().enumerate() {
        if !valid_handle(&item.evidence_handle)
            || !valid_text(&item.title, 512)
            || item
                .excerpt
                .as_ref()
                .is_some_and(|value| !valid_text(value, 2048))
            || item
                .status
                .as_ref()
                .is_some_and(|value| !valid_text(value, 256))
            || item
                .blocker
                .as_ref()
                .is_some_and(|value| !valid_text(value, 512))
            || item
                .next_action
                .as_ref()
                .is_some_and(|value| !valid_text(value, 512))
            || item.observed_at_unix_ms < 0
            || item.observed_at_unix_ms > view.observed_at_unix_ms
            || view.items[..index]
                .iter()
                .any(|other| other.evidence_handle == item.evidence_handle)
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    validate_size(view)
}

pub fn validate_logistics_view(view: &LogisticsView, now_unix_ms: i64) -> Result<(), AgentFailure> {
    validate_envelope(
        view.schema_version,
        &view.view_id,
        LOGISTICS_VIEW_ID,
        &view.source_handle,
        view.observed_at_unix_ms,
        view.expires_at_unix_ms,
        now_unix_ms,
    )?;
    if view.items.len() > 64 {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, item) in view.items.iter().enumerate() {
        if !valid_handle(&item.evidence_handle)
            || !valid_text(&item.summary, 512)
            || !valid_text(&item.status, 128)
            || item.occurs_at_unix_ms.is_some_and(|value| value < 0)
            || view.items[..index]
                .iter()
                .any(|other| other.evidence_handle == item.evidence_handle)
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    validate_size(view)
}

pub fn work_context_evidence(view: &WorkContextView) -> Result<ContextEvidence, AgentFailure> {
    evidence(&view.source_handle, view.expires_at_unix_ms, view)
}

pub fn logistics_context_evidence(view: &LogisticsView) -> Result<ContextEvidence, AgentFailure> {
    evidence(&view.source_handle, view.expires_at_unix_ms, view)
}

fn evidence(
    source_handle: &str,
    expires_at_unix_ms: i64,
    value: &impl Serialize,
) -> Result<ContextEvidence, AgentFailure> {
    Ok(ContextEvidence {
        source_handle: source_handle.into(),
        data_class: DataClass::Personal,
        untrusted_text: serde_json::to_string(value).map_err(|_| AgentFailure::InvalidInput)?,
        expires_at_unix_ms: u64::try_from(expires_at_unix_ms)
            .map_err(|_| AgentFailure::InvalidInput)?,
    })
}

fn validate_envelope(
    schema_version: u32,
    view_id: &str,
    expected_view_id: &str,
    source_handle: &str,
    observed_at_unix_ms: i64,
    expires_at_unix_ms: i64,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    if schema_version != AGENT_VERSION {
        return Err(AgentFailure::UnsupportedVersion);
    }
    if view_id != expected_view_id
        || !valid_handle(source_handle)
        || observed_at_unix_ms > now_unix_ms
        || expires_at_unix_ms <= now_unix_ms
        || expires_at_unix_ms <= observed_at_unix_ms
        || expires_at_unix_ms - observed_at_unix_ms > 300_000
    {
        Err(AgentFailure::InvalidInput)
    } else {
        Ok(())
    }
}

fn validate_size(value: &impl Serialize) -> Result<(), AgentFailure> {
    if serde_json::to_vec(value)
        .map_err(|_| AgentFailure::InvalidInput)?
        .len()
        > MAX_PORTFOLIO_VIEW_BYTES
    {
        Err(AgentFailure::BudgetExceeded)
    } else {
        Ok(())
    }
}

fn valid_handle(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum
}
