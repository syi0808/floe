use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AGENT_VERSION, AgentFailure, ContextEvidence, DataClass};

pub const FLOE_TASK_VIEW_ID: &str = "floe.tasks";
pub const FLOE_NOTE_VIEW_ID: &str = "floe.notes";
pub const MAX_NATIVE_CONTEXT_ITEMS: usize = 128;
pub const MAX_NATIVE_CONTEXT_BYTES: usize = 65_536;
pub const MAX_NATIVE_CONTEXT_TEXT_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskContextPriority {
    Low,
    Normal,
    High,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeContextItem {
    Task {
        evidence_handle: Uuid,
        untrusted_title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deadline_unix_ms: Option<u64>,
        priority: TaskContextPriority,
    },
    Note {
        evidence_handle: Uuid,
        untrusted_excerpt: String,
        updated_at_unix_ms: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeContextView {
    pub schema_version: u32,
    pub handle: Uuid,
    pub person_id: PersonId,
    pub view_id: String,
    pub data_class: DataClass,
    pub source_handle: String,
    pub observed_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub coverage_complete: bool,
    pub next_cursor: Option<String>,
    pub items: Vec<NativeContextItem>,
}

pub fn validate_native_context_view(
    view: &NativeContextView,
    expected_person_id: PersonId,
    expected_handle: Uuid,
    now_unix_ms: u64,
    max_items: usize,
    max_bytes: usize,
) -> Result<(), AgentFailure> {
    if view.schema_version != AGENT_VERSION {
        return Err(AgentFailure::UnsupportedVersion);
    }
    if view.person_id != expected_person_id
        || view.handle != expected_handle
        || view.data_class != DataClass::Personal
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    if !matches!(view.view_id.as_str(), FLOE_TASK_VIEW_ID | FLOE_NOTE_VIEW_ID)
        || view.source_handle.trim().is_empty()
        || view.source_handle.len() > 128
        || view.observed_at_unix_ms > now_unix_ms
        || view.expires_at_unix_ms <= now_unix_ms
        || view.expires_at_unix_ms - view.observed_at_unix_ms > 300_000
        || view.coverage_complete == view.next_cursor.is_some()
        || view
            .next_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 256)
    {
        return Err(AgentFailure::InvalidInput);
    }
    if view.items.len() > max_items.min(MAX_NATIVE_CONTEXT_ITEMS)
        || serde_json::to_vec(view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > max_bytes.min(MAX_NATIVE_CONTEXT_BYTES)
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    if view.items.iter().enumerate().any(|(index, item)| {
        let (handle, text, matching_kind) = match item {
            NativeContextItem::Task {
                evidence_handle,
                untrusted_title,
                ..
            } => (
                evidence_handle,
                untrusted_title,
                view.view_id == FLOE_TASK_VIEW_ID,
            ),
            NativeContextItem::Note {
                evidence_handle,
                untrusted_excerpt,
                updated_at_unix_ms,
            } => (
                evidence_handle,
                untrusted_excerpt,
                view.view_id == FLOE_NOTE_VIEW_ID
                    && *updated_at_unix_ms <= view.observed_at_unix_ms,
            ),
        };
        !matching_kind
            || handle.is_nil()
            || text.trim().is_empty()
            || text.len() > MAX_NATIVE_CONTEXT_TEXT_BYTES
            || view.items[..index]
                .iter()
                .any(|other| item_handle(other) == *handle)
    }) {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

pub fn native_context_evidence(view: &NativeContextView) -> Result<ContextEvidence, AgentFailure> {
    Ok(ContextEvidence {
        source_handle: view.source_handle.clone(),
        data_class: view.data_class,
        untrusted_text: serde_json::to_string(&view.items)
            .map_err(|_| AgentFailure::InvalidInput)?,
        expires_at_unix_ms: view.expires_at_unix_ms,
    })
}

fn item_handle(item: &NativeContextItem) -> Uuid {
    match item {
        NativeContextItem::Task {
            evidence_handle, ..
        }
        | NativeContextItem::Note {
            evidence_handle, ..
        } => *evidence_handle,
    }
}
