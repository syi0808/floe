use crate::{
    FLOE_NOTE_VIEW_ID, FLOE_TASK_VIEW_ID, MAX_NATIVE_CONTEXT_TEXT_BYTES, NativeContextItem,
    NativeContextView, TaskContextPriority, validate_native_context_view,
};
use chrono::{DateTime, Utc};
use floe_agent_contract::AGENT_VERSION;
use floe_agent_contract::{AgentFailure, DataClass};
use floe_context_contract::PersonId;
use uuid::Uuid;

use crate::{
    DayContextEvidence, DayContextEvidenceQuery, DayContextEvidenceReader,
    DayContextEvidenceSelection, DayNoteEvidence, DayTaskEvidence,
};

const NATIVE_CONTEXT_TTL: chrono::Duration = chrono::Duration::minutes(5);

pub async fn task_context_view(
    reader: &(impl DayContextEvidenceReader + ?Sized),
    person_id: PersonId,
    handle: Uuid,
    now: DateTime<Utc>,
    max_items: usize,
    max_bytes: usize,
) -> Result<NativeContextView, AgentFailure> {
    let query = DayContextEvidenceQuery::try_new(
        person_id,
        DayContextEvidenceSelection::OpenTasks,
        max_items,
        max_bytes,
    )?;
    let projection = query.projection_budget();
    let max_items = projection.max_items();
    let max_bytes = projection.max_serialized_view_bytes();
    let mut tasks = reader
        .read(query)
        .await
        .map_err(AgentFailure::from)?
        .into_iter()
        .map(|item| match item {
            DayContextEvidence::Task(value) => Ok(value),
            _ => Err(AgentFailure::StorageUnavailable),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if tasks.iter().any(|task| !valid_task(task, person_id)) {
        return Err(AgentFailure::StorageUnavailable);
    }
    tasks.retain(|task| task.deleted_at.is_none() && task.completed_at.is_none());
    tasks.sort_by_key(|task| {
        (
            task.deadline.is_none(),
            task.deadline,
            match task.priority {
                TaskContextPriority::High => 0,
                TaskContextPriority::Normal => 1,
                TaskContextPriority::Low => 2,
            },
            task.created_at,
            task.id,
        )
    });
    if tasks.len() > max_items {
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
                    evidence_handle: task.id,
                    untrusted_title: bounded_text(&task.title),
                    deadline_unix_ms: task.deadline.map(milliseconds).transpose()?,
                    priority: match task.priority {
                        TaskContextPriority::Low => TaskContextPriority::Low,
                        TaskContextPriority::Normal => TaskContextPriority::Normal,
                        TaskContextPriority::High => TaskContextPriority::High,
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
    reader: &(impl DayContextEvidenceReader + ?Sized),
    person_id: PersonId,
    handle: Uuid,
    now: DateTime<Utc>,
    max_items: usize,
    max_bytes: usize,
) -> Result<NativeContextView, AgentFailure> {
    let query = DayContextEvidenceQuery::try_new(
        person_id,
        DayContextEvidenceSelection::CurrentNotes,
        max_items,
        max_bytes,
    )?;
    let projection = query.projection_budget();
    let max_items = projection.max_items();
    let max_bytes = projection.max_serialized_view_bytes();
    let mut notes = reader
        .read(query)
        .await
        .map_err(AgentFailure::from)?
        .into_iter()
        .map(|item| match item {
            DayContextEvidence::Note(value) => Ok(value),
            _ => Err(AgentFailure::StorageUnavailable),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if notes.iter().any(|note| !valid_note(note, person_id)) {
        return Err(AgentFailure::StorageUnavailable);
    }
    notes.retain(|note| note.deleted_at.is_none());
    notes.sort_by_key(|note| (std::cmp::Reverse(note.updated_at), note.id));
    if notes.len() > max_items {
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
                    evidence_handle: note.id,
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

fn valid_task(task: &DayTaskEvidence, person_id: PersonId) -> bool {
    task.person_id == person_id && !task.id.is_nil() && !task.title.trim().is_empty()
}

fn valid_note(note: &DayNoteEvidence, person_id: PersonId) -> bool {
    note.person_id == person_id && !note.id.is_nil() && !note.content.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::{note_context_view, task_context_view};
    use crate::{
        DayContextEvidence, DayContextEvidenceError, DayContextEvidenceQuery,
        DayContextEvidenceReader, DayContextEvidenceSelection, DayNoteEvidence, DayTaskEvidence,
        NativeContextItem,
    };
    use floe_context_contract::TaskContextPriority;
    use floe_context_contract::{MAX_NATIVE_CONTEXT_BYTES, MAX_NATIVE_CONTEXT_ITEMS};
    use floe_execution::BoxFuture;
    use floe_kernel::{AgentFailure, PersonId};
    use std::sync::Mutex;
    use uuid::Uuid;

    #[derive(Default)]
    struct ScriptedReader {
        queries: Mutex<Vec<DayContextEvidenceQuery>>,
        values: Mutex<Vec<DayContextEvidence>>,
        failure: Mutex<Option<DayContextEvidenceError>>,
    }

    impl DayContextEvidenceReader for ScriptedReader {
        fn read<'a>(
            &'a self,
            query: DayContextEvidenceQuery,
        ) -> BoxFuture<'a, Result<Vec<DayContextEvidence>, DayContextEvidenceError>> {
            self.queries.lock().unwrap().push(query);
            let values = self.values.lock().unwrap().clone();
            let failure = *self.failure.lock().unwrap();
            Box::pin(async move { failure.map_or(Ok(values), Err) })
        }
    }

    #[tokio::test]
    async fn task_and_note_views_keep_the_bounded_owner_evidence_projection() {
        let person_id = PersonId::new();
        let observed_at = chrono::DateTime::from_timestamp_millis(1_000).unwrap();
        let deadline = chrono::DateTime::from_timestamp_millis(2_000).unwrap();
        let task_id = Uuid::new_v4();
        let task_reader = ScriptedReader::default();
        task_reader
            .values
            .lock()
            .unwrap()
            .push(DayContextEvidence::Task(DayTaskEvidence {
                id: task_id,
                person_id,
                title: "Prepare review".into(),
                deadline: Some(deadline),
                priority: TaskContextPriority::High,
                completed_at: None,
                created_at: observed_at,
                deleted_at: None,
            }));

        let task_view = task_context_view(
            &task_reader,
            person_id,
            Uuid::new_v4(),
            observed_at,
            usize::MAX,
            usize::MAX,
        )
        .await
        .unwrap();
        assert_eq!(
            task_view.items,
            vec![NativeContextItem::Task {
                evidence_handle: task_id,
                untrusted_title: "Prepare review".into(),
                deadline_unix_ms: Some(2_000),
                priority: TaskContextPriority::High,
            }]
        );
        let task_query = task_reader.queries.lock().unwrap()[0];
        assert_eq!(
            task_query.selection(),
            DayContextEvidenceSelection::OpenTasks
        );
        assert_eq!(
            task_query.projection_budget().max_items(),
            MAX_NATIVE_CONTEXT_ITEMS
        );
        assert_eq!(
            task_query.projection_budget().max_serialized_view_bytes(),
            MAX_NATIVE_CONTEXT_BYTES
        );
        assert_eq!(task_query.acquisition_budget().max_candidate_items(), 128);
        assert_eq!(
            task_query
                .acquisition_budget()
                .max_serialized_payload_bytes(),
            crate::MAX_DAY_EVIDENCE_ACQUISITION_PAYLOAD_BYTES
        );
        assert_eq!(
            task_query
                .acquisition_budget()
                .max_projected_day_item_bytes(),
            floe_day::MAX_DAY_SNAPSHOT_BYTES
        );

        let note_id = Uuid::new_v4();
        let note_reader = ScriptedReader::default();
        note_reader
            .values
            .lock()
            .unwrap()
            .push(DayContextEvidence::Note(DayNoteEvidence {
                id: note_id,
                person_id,
                content: "Bring the signed form".into(),
                updated_at: observed_at,
                deleted_at: None,
            }));
        let note_view = note_context_view(
            &note_reader,
            person_id,
            Uuid::new_v4(),
            observed_at,
            3,
            4_096,
        )
        .await
        .unwrap();
        assert_eq!(
            note_view.items,
            vec![NativeContextItem::Note {
                evidence_handle: note_id,
                untrusted_excerpt: "Bring the signed form".into(),
                updated_at_unix_ms: 1_000,
            }]
        );
        let note_query = note_reader.queries.lock().unwrap()[0];
        assert_eq!(
            note_query.selection(),
            DayContextEvidenceSelection::CurrentNotes
        );
        assert_eq!(note_query.projection_budget().max_items(), 3);
        assert_eq!(
            note_query.projection_budget().max_serialized_view_bytes(),
            4_096
        );
        assert_eq!(note_query.acquisition_budget().max_candidate_items(), 3);
        assert_eq!(
            note_query
                .acquisition_budget()
                .max_serialized_payload_bytes(),
            floe_day::MAX_DAY_SNAPSHOT_BYTES
        );
    }

    #[tokio::test]
    async fn evidence_reader_budget_and_storage_failures_keep_distinct_outcomes() {
        for (failure, expected) in [
            (
                DayContextEvidenceError::BudgetExceeded,
                AgentFailure::BudgetExceeded,
            ),
            (
                DayContextEvidenceError::StorageUnavailable,
                AgentFailure::StorageUnavailable,
            ),
        ] {
            let reader = ScriptedReader::default();
            *reader.failure.lock().unwrap() = Some(failure);
            assert_eq!(
                task_context_view(
                    &reader,
                    PersonId::new(),
                    Uuid::new_v4(),
                    chrono::DateTime::from_timestamp_millis(1_000).unwrap(),
                    4,
                    4_096,
                )
                .await,
                Err(expected)
            );
        }
    }

    #[tokio::test]
    async fn context_rejects_evidence_exceeding_the_requested_item_bound() {
        let person_id = PersonId::new();
        let now = chrono::DateTime::from_timestamp_millis(1_000).unwrap();
        let reader = ScriptedReader::default();
        *reader.values.lock().unwrap() = (0..2)
            .map(|_| {
                DayContextEvidence::Task(DayTaskEvidence {
                    id: Uuid::new_v4(),
                    person_id,
                    title: "Task".into(),
                    deadline: None,
                    priority: TaskContextPriority::Normal,
                    completed_at: None,
                    created_at: now,
                    deleted_at: None,
                })
            })
            .collect();

        assert_eq!(
            task_context_view(&reader, person_id, Uuid::new_v4(), now, 1, 4_096).await,
            Err(AgentFailure::BudgetExceeded)
        );
        assert_eq!(
            reader.queries.lock().unwrap()[0]
                .projection_budget()
                .max_items(),
            1
        );
    }

    #[tokio::test]
    async fn long_note_is_acquired_under_day_budget_then_truncated_to_native_view_budget() {
        let person_id = PersonId::new();
        let now = chrono::DateTime::from_timestamp_millis(1_000).unwrap();
        let note_id = Uuid::new_v4();
        let reader = ScriptedReader::default();
        reader
            .values
            .lock()
            .unwrap()
            .push(DayContextEvidence::Note(DayNoteEvidence {
                id: note_id,
                person_id,
                content: "n".repeat(128 * 1024),
                updated_at: now,
                deleted_at: None,
            }));

        let view = note_context_view(&reader, person_id, Uuid::new_v4(), now, 4, 4_096)
            .await
            .unwrap();
        let NativeContextItem::Note {
            untrusted_excerpt, ..
        } = &view.items[0]
        else {
            panic!("expected bounded Note projection");
        };
        assert_eq!(
            untrusted_excerpt.len(),
            crate::MAX_NATIVE_CONTEXT_TEXT_BYTES
        );
        assert!(serde_json::to_vec(&view).unwrap().len() <= 4_096);

        let query = reader.queries.lock().unwrap()[0];
        assert_eq!(query.projection_budget().max_serialized_view_bytes(), 4_096);
        assert_eq!(
            query.acquisition_budget().max_serialized_payload_bytes(),
            floe_day::MAX_DAY_SNAPSHOT_BYTES
        );
    }
}
