//! App adapter from Context's narrow evidence port to Day's bounded query.

use std::sync::Arc;

use floe_context::{
    DayContextEvidence, DayContextEvidenceError, DayContextEvidenceQuery, DayContextEvidenceReader,
    DayContextEvidenceSelection, DayNoteEvidence, DayTaskEvidence,
};
use floe_day::{
    DayReadAcquisitionBudget, DayReadQuery, DayReadSelection, DayRepository, TimelineItem,
};
use floe_execution::BoxFuture;

pub(crate) struct DayContextEvidenceAdapter {
    repository: Arc<dyn DayRepository>,
}

impl DayContextEvidenceAdapter {
    pub(crate) fn new(repository: Arc<dyn DayRepository>) -> Self {
        Self { repository }
    }
}

impl DayContextEvidenceReader for DayContextEvidenceAdapter {
    fn read<'a>(
        &'a self,
        query: DayContextEvidenceQuery,
    ) -> BoxFuture<'a, Result<Vec<DayContextEvidence>, DayContextEvidenceError>> {
        Box::pin(async move {
            let selection = query.selection();
            let day_query = day_read_query(&query).map_err(map_day_error)?;
            let items = self
                .repository
                .read_items(day_query)
                .await
                .map_err(map_day_error)?;

            map_day_items(selection, items)
        })
    }
}

fn day_read_query(query: &DayContextEvidenceQuery) -> Result<DayReadQuery, floe_day::DayError> {
    let acquisition = query.acquisition_budget();
    let day_acquisition = DayReadAcquisitionBudget::try_new(
        acquisition.max_candidate_items(),
        acquisition.max_serialized_payload_bytes(),
    )?;
    DayReadQuery::context_evidence(
        query.person_id(),
        match query.selection() {
            DayContextEvidenceSelection::OpenTasks => DayReadSelection::OpenTasks,
            DayContextEvidenceSelection::CurrentNotes => DayReadSelection::CurrentNotes,
        },
        day_acquisition,
        acquisition.max_projected_day_item_bytes(),
    )
}

fn map_day_items(
    selection: DayContextEvidenceSelection,
    items: Vec<TimelineItem>,
) -> Result<Vec<DayContextEvidence>, DayContextEvidenceError> {
    items
        .into_iter()
        .map(|item| match (selection, item) {
            (DayContextEvidenceSelection::OpenTasks, TimelineItem::Task(task)) => {
                Ok(DayContextEvidence::Task(DayTaskEvidence {
                    id: task.id.0,
                    person_id: task.person_id,
                    title: task.title,
                    deadline: task.deadline,
                    priority: match task.priority {
                        floe_day::Priority::Low => floe_context::TaskContextPriority::Low,
                        floe_day::Priority::Normal => floe_context::TaskContextPriority::Normal,
                        floe_day::Priority::High => floe_context::TaskContextPriority::High,
                    },
                    completed_at: task.completed_at,
                    created_at: task.created_at,
                    deleted_at: task.deleted_at,
                }))
            }
            (DayContextEvidenceSelection::CurrentNotes, TimelineItem::Note(note)) => {
                Ok(DayContextEvidence::Note(DayNoteEvidence {
                    id: note.id.0,
                    person_id: note.person_id,
                    content: note.content,
                    updated_at: note.updated_at,
                    deleted_at: note.deleted_at,
                }))
            }
            _ => Err(DayContextEvidenceError::StorageUnavailable),
        })
        .collect()
}

fn map_day_error(error: floe_day::DayError) -> DayContextEvidenceError {
    if error.code == floe_day::DayErrorCode::Validation {
        DayContextEvidenceError::BudgetExceeded
    } else {
        DayContextEvidenceError::StorageUnavailable
    }
}

#[cfg(test)]
mod tests {
    use super::{day_read_query, map_day_error, map_day_items};
    use floe_context::{
        DayContextEvidence, DayContextEvidenceError, DayContextEvidenceQuery,
        DayContextEvidenceSelection, DayNoteEvidence, DayTaskEvidence,
    };
    use floe_day::{DayError, DayReadSelection, Note, Priority, SourceRef, Task, TimelineItem};
    use floe_kernel::PersonId;

    #[test]
    fn bounded_queries_map_tasks_and_notes_without_changing_evidence() {
        let person_id = PersonId::new();
        let now = chrono::DateTime::from_timestamp_millis(1_000).unwrap();
        let task = Task::new(
            person_id,
            "Prepare review",
            Some(chrono::DateTime::from_timestamp_millis(2_000).unwrap()),
            Priority::High,
            SourceRef::Manual,
            now,
        )
        .unwrap();
        let note = Note::new(person_id, "Bring the signed form", SourceRef::Manual, now).unwrap();
        let task_query = DayContextEvidenceQuery::try_new(
            person_id,
            DayContextEvidenceSelection::OpenTasks,
            5,
            4_096,
        )
        .unwrap();
        let bounded_task_query = day_read_query(&task_query).unwrap();
        assert_eq!(bounded_task_query.max_items, 5);
        assert_eq!(
            bounded_task_query.max_projected_item_bytes,
            floe_day::MAX_DAY_SNAPSHOT_BYTES
        );
        let bounded_task_acquisition = bounded_task_query.acquisition.unwrap();
        assert_eq!(bounded_task_acquisition.max_candidate_items(), 5);
        assert_eq!(
            bounded_task_acquisition.max_serialized_payload_bytes(),
            floe_day::MAX_DAY_SNAPSHOT_BYTES
        );
        assert!(matches!(
            bounded_task_query.selection,
            DayReadSelection::OpenTasks
        ));
        let task_result = map_day_items(
            task_query.selection(),
            vec![TimelineItem::Task(task.clone())],
        )
        .unwrap();
        assert_eq!(
            task_result,
            vec![DayContextEvidence::Task(DayTaskEvidence {
                id: task.id.0,
                person_id,
                title: task.title,
                deadline: task.deadline,
                priority: floe_context::TaskContextPriority::High,
                completed_at: task.completed_at,
                created_at: task.created_at,
                deleted_at: task.deleted_at,
            })]
        );

        let note_query = DayContextEvidenceQuery::try_new(
            person_id,
            DayContextEvidenceSelection::CurrentNotes,
            3,
            2_048,
        )
        .unwrap();
        let bounded_note_query = day_read_query(&note_query).unwrap();
        assert_eq!(bounded_note_query.max_items, 3);
        assert_eq!(
            bounded_note_query.max_projected_item_bytes,
            floe_day::MAX_DAY_SNAPSHOT_BYTES
        );
        let bounded_note_acquisition = bounded_note_query.acquisition.unwrap();
        assert_eq!(bounded_note_acquisition.max_candidate_items(), 3);
        assert_eq!(
            bounded_note_acquisition.max_serialized_payload_bytes(),
            floe_day::MAX_DAY_SNAPSHOT_BYTES
        );
        assert!(matches!(
            bounded_note_query.selection,
            DayReadSelection::CurrentNotes
        ));
        let note_result = map_day_items(
            note_query.selection(),
            vec![TimelineItem::Note(note.clone())],
        )
        .unwrap();
        assert_eq!(
            note_result,
            vec![DayContextEvidence::Note(DayNoteEvidence {
                id: note.id.0,
                person_id,
                content: note.content,
                updated_at: note.updated_at,
                deleted_at: note.deleted_at,
            })]
        );
    }

    #[test]
    fn day_budget_and_storage_failures_keep_distinct_context_errors() {
        for (failure, expected) in [
            (
                DayError::budget("bounded evidence exceeded"),
                DayContextEvidenceError::BudgetExceeded,
            ),
            (
                DayError::storage("Day read unavailable"),
                DayContextEvidenceError::StorageUnavailable,
            ),
        ] {
            assert_eq!(map_day_error(failure), expected);
        }
    }
}
