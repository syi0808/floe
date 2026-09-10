use serde::{Deserialize, Serialize};

use crate::{AGENT_VERSION, AgentFailure, ContextEvidence, DataClass};

pub const COMMUNICATION_VIEW_ID: &str = "mail.communication";
pub const MAX_COMMUNICATION_ITEMS: usize = 100;
pub const MAX_COMMUNICATION_BYTES: usize = 65_536;
pub const MAX_COMMUNICATION_FRESHNESS_MS: u64 = 300_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommunicationItem {
    pub evidence_handle: String,
    pub thread_handle: String,
    pub received_unix_ms: i64,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub snippet: String,
    pub labels: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommunicationView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub coverage_complete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<usize>,
    pub items: Vec<CommunicationItem>,
}

pub fn validate_communication_view(
    view: &CommunicationView,
    now_unix_ms: i64,
    maximum_items: usize,
    maximum_bytes: usize,
) -> Result<(), AgentFailure> {
    if view.schema_version != AGENT_VERSION {
        return Err(AgentFailure::UnsupportedVersion);
    }
    if view.view_id != COMMUNICATION_VIEW_ID
        || !valid_handle(&view.source_handle)
        || view.observed_at_unix_ms > now_unix_ms
        || view.expires_at_unix_ms <= now_unix_ms
        || view.expires_at_unix_ms <= view.observed_at_unix_ms
        || u64::try_from(view.expires_at_unix_ms - view.observed_at_unix_ms)
            .map_err(|_| AgentFailure::InvalidInput)?
            > MAX_COMMUNICATION_FRESHNESS_MS
        || view.coverage_complete == view.next_cursor.is_some()
        || view.next_cursor.is_some_and(|cursor| cursor > 10_000)
    {
        return Err(AgentFailure::InvalidInput);
    }
    if view.items.len() > maximum_items.min(MAX_COMMUNICATION_ITEMS)
        || serde_json::to_vec(view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > maximum_bytes.min(MAX_COMMUNICATION_BYTES)
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, item) in view.items.iter().enumerate() {
        if !valid_handle(&item.evidence_handle)
            || !valid_handle(&item.thread_handle)
            || item.received_unix_ms < 0
            || item.received_unix_ms > view.observed_at_unix_ms
            || item.from.len() > 4096
            || item.to.len() > 4096
            || item.subject.len() > 4096
            || item.snippet.len() > 1024
            || item.labels.len() > 128
            || item.labels.iter().any(|label| !valid_label(label))
            || item
                .labels
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != item.labels.len()
            || view.items[..index]
                .iter()
                .any(|other| other.evidence_handle == item.evidence_handle)
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    Ok(())
}

pub fn communication_context_evidence(
    view: &CommunicationView,
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

fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}
