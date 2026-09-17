use crate::{
    FLOE_NOTE_VIEW_ID, FLOE_TASK_VIEW_ID, MAX_NATIVE_CONTEXT_ITEMS, MAX_NATIVE_CONTEXT_TEXT_BYTES,
    NativeContextItem, NativeContextView, TaskContextPriority, validate_native_context_view,
};
use chrono::{DateTime, Utc};
use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::{AgentFailure, DataClass};
use floe_context_contract::PersonId;
use floe_day::{Priority, TimelineRepository};
use uuid::Uuid;

const NATIVE_CONTEXT_TTL: chrono::Duration = chrono::Duration::minutes(5);

pub async fn task_context_view(
    repository: &impl TimelineRepository,
    person_id: PersonId,
    handle: Uuid,
    now: DateTime<Utc>,
    max_items: usize,
    max_bytes: usize,
) -> Result<NativeContextView, AgentFailure> {
    let mut tasks = repository
        .list_tasks(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    if tasks.iter().any(|task| {
        task.person_id != person_id || task.id.0.is_nil() || task.title.trim().is_empty()
    }) {
        return Err(AgentFailure::StorageUnavailable);
    }
    tasks.retain(|task| task.deleted_at.is_none() && task.completed_at.is_none());
    tasks.sort_by_key(|task| {
        (
            task.deadline.is_none(),
            task.deadline,
            match task.priority {
                Priority::High => 0,
                Priority::Normal => 1,
                Priority::Low => 2,
            },
            task.created_at,
            task.id,
        )
    });
    if tasks.len() > max_items.min(MAX_NATIVE_CONTEXT_ITEMS) {
        return Err(AgentFailure::BudgetExceeded);
    }
    let observed_at_unix_ms = milliseconds(now)?;
    let view = NativeContextView {
        schema_version: AGENT_VERSION,
        handle,
        person_id,
        view_id: FLOE_TASK_VIEW_ID.into(),
        data_class: DataClass::Personal,
        source_handle: format!("floe.tasks:{handle}"),
        observed_at_unix_ms,
        expires_at_unix_ms: milliseconds(now + NATIVE_CONTEXT_TTL)?,
        coverage_complete: true,
        next_cursor: None,
        items: tasks
            .into_iter()
            .map(|task| {
                Ok(NativeContextItem::Task {
                    evidence_handle: task.id.0,
                    untrusted_title: bounded_text(&task.title),
                    deadline_unix_ms: task.deadline.map(milliseconds).transpose()?,
                    priority: match task.priority {
                        Priority::Low => TaskContextPriority::Low,
                        Priority::Normal => TaskContextPriority::Normal,
                        Priority::High => TaskContextPriority::High,
                    },
                })
            })
            .collect::<Result<_, AgentFailure>>()?,
    };
    validate_native_context_view(
        &view,
        person_id,
        handle,
        observed_at_unix_ms,
        max_items,
        max_bytes,
    )?;
    Ok(view)
}

pub async fn note_context_view(
    repository: &impl TimelineRepository,
    person_id: PersonId,
    handle: Uuid,
    now: DateTime<Utc>,
    max_items: usize,
    max_bytes: usize,
) -> Result<NativeContextView, AgentFailure> {
    let mut notes = repository
        .list_notes(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?;
    if notes.iter().any(|note| {
        note.person_id != person_id || note.id.0.is_nil() || note.content.trim().is_empty()
    }) {
        return Err(AgentFailure::StorageUnavailable);
    }
    notes.retain(|note| note.deleted_at.is_none());
    notes.sort_by_key(|note| (std::cmp::Reverse(note.updated_at), note.id));
    if notes.len() > max_items.min(MAX_NATIVE_CONTEXT_ITEMS) {
        return Err(AgentFailure::BudgetExceeded);
    }
    let observed_at_unix_ms = milliseconds(now)?;
    let view = NativeContextView {
        schema_version: AGENT_VERSION,
        handle,
        person_id,
        view_id: FLOE_NOTE_VIEW_ID.into(),
        data_class: DataClass::Personal,
        source_handle: format!("floe.notes:{handle}"),
        observed_at_unix_ms,
        expires_at_unix_ms: milliseconds(now + NATIVE_CONTEXT_TTL)?,
        coverage_complete: true,
        next_cursor: None,
        items: notes
            .into_iter()
            .map(|note| {
                Ok(NativeContextItem::Note {
                    evidence_handle: note.id.0,
                    untrusted_excerpt: bounded_text(&note.content),
                    updated_at_unix_ms: milliseconds(note.updated_at)?,
                })
            })
            .collect::<Result<_, AgentFailure>>()?,
    };
    validate_native_context_view(
        &view,
        person_id,
        handle,
        observed_at_unix_ms,
        max_items,
        max_bytes,
    )?;
    Ok(view)
}

fn bounded_text(value: &str) -> String {
    let mut end = value.len().min(MAX_NATIVE_CONTEXT_TEXT_BYTES);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn milliseconds(value: DateTime<Utc>) -> Result<u64, AgentFailure> {
    u64::try_from(value.timestamp_millis()).map_err(|_| AgentFailure::InvalidInput)
}
