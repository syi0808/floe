use chrono::{TimeZone, Utc};
use floe_context::{
    FLOE_NOTE_VIEW_ID, FLOE_TASK_VIEW_ID, MAX_NATIVE_CONTEXT_BYTES, NativeContextItem,
    TaskContextPriority, acquire_optional_source, note_context_view, task_context_view,
    validate_native_context_view,
};
use floe_context_contract::{ContextIssueReason, ContextSource};
use floe_day::{DayService, Priority};
use floe_kernel::{AgentFailure, PersonId};
use uuid::Uuid;

mod support;
use support::TestTimelineRepository;

#[tokio::test]
async fn optional_context_keeps_budget_issues_distinct_from_unreadable_storage() {
    let timeline = TestTimelineRepository::new();
    let day = DayService::new(&timeline);
    let person = PersonId::new();
    let now = Utc::now();
    day.create_task(person, "one", None, Priority::Normal, now)
        .await
        .unwrap();
    day.create_task(person, "two", None, Priority::Normal, now)
        .await
        .unwrap();
    let note = day.create_note(person, "one", now).await.unwrap();
    day.create_note(person, "two", now).await.unwrap();

    // Two items will not fit a one-item budget: that is an optional-source
    // issue the turn records, not a failure.
    let tasks = acquire_optional_source(
        ContextSource::Tasks,
        task_context_view(
            &timeline,
            person,
            Uuid::new_v4(),
            now,
            1,
            MAX_NATIVE_CONTEXT_BYTES,
        ),
    )
    .await
    .unwrap();
    assert!(tasks.value.is_none());
    assert_eq!(
        tasks.issue.unwrap().reason,
        ContextIssueReason::BudgetExceeded
    );
    let notes = acquire_optional_source(
        ContextSource::Notes,
        note_context_view(
            &timeline,
            person,
            Uuid::new_v4(),
            now,
            1,
            MAX_NATIVE_CONTEXT_BYTES,
        ),
    )
    .await
    .unwrap();
    assert!(notes.value.is_none());
    assert_eq!(
        notes.issue.unwrap().reason,
        ContextIssueReason::BudgetExceeded
    );

    // A row belonging to someone else is not a budget issue and never degrades
    // into a partial view: the read fails.
    let mut stranger = day
        .create_task(person, "three", None, Priority::Normal, now)
        .await
        .unwrap();
    stranger.person_id = PersonId::new();
    timeline.force_put_task(person, &stranger);
    let mut stranger_note = note.clone();
    stranger_note.person_id = PersonId::new();
    timeline.force_put_note(person, &stranger_note);
    assert_eq!(
        task_context_view(
            &timeline,
            person,
            Uuid::new_v4(),
            now,
            8,
            MAX_NATIVE_CONTEXT_BYTES
        )
        .await,
        Err(AgentFailure::StorageUnavailable)
    );
    assert_eq!(
        note_context_view(
            &timeline,
            person,
            Uuid::new_v4(),
            now,
            8,
            MAX_NATIVE_CONTEXT_BYTES
        )
        .await,
        Err(AgentFailure::StorageUnavailable)
    );

    // An unreadable store is a failure the optional source propagates rather
    // than reporting as a missing-but-healthy view.
    timeline.fail_reads();
    assert_eq!(
        acquire_optional_source(
            ContextSource::Tasks,
            task_context_view(
                &timeline,
                person,
                Uuid::new_v4(),
                now,
                8,
                MAX_NATIVE_CONTEXT_BYTES
            ),
        )
        .await,
        Err(AgentFailure::StorageUnavailable)
    );
    assert_eq!(
        acquire_optional_source(
            ContextSource::Notes,
            note_context_view(
                &timeline,
                person,
                Uuid::new_v4(),
                now,
                8,
                MAX_NATIVE_CONTEXT_BYTES
            ),
        )
        .await,
        Err(AgentFailure::StorageUnavailable)
    );
}

#[tokio::test]
async fn projects_bounded_floe_native_task_and_note_views() {
    let timeline = TestTimelineRepository::new();
    let core = DayService::new(&timeline);
    let person = PersonId::new();
    let other = PersonId::new();
    let now = Utc.with_ymd_and_hms(2026, 9, 10, 9, 0, 0).unwrap();
    let deadline = Utc.with_ymd_and_hms(2026, 9, 10, 17, 0, 0).unwrap();
    let task = core
        .create_task(
            person,
            "Ship the proposal",
            Some(deadline),
            Priority::High,
            now,
        )
        .await
        .unwrap();
    let completed = core
        .create_task(person, "Already done", None, Priority::Normal, now)
        .await
        .unwrap();
    core.set_task_completed(completed.id, completed.revision, true, now)
        .await
        .unwrap();
    core.create_task(other, "Private to another person", None, Priority::Low, now)
        .await
        .unwrap();
    let note = core
        .create_note(person, "Remember the launch constraint", now)
        .await
        .unwrap();

    let task_handle = Uuid::new_v4();
    let task_view = task_context_view(
        &timeline,
        person,
        task_handle,
        now,
        8,
        MAX_NATIVE_CONTEXT_BYTES,
    )
    .await
    .unwrap();
    assert_eq!(task_view.view_id, FLOE_TASK_VIEW_ID);
    assert_eq!(task_view.items.len(), 1);
    assert_eq!(
        task_view.items[0],
        NativeContextItem::Task {
            evidence_handle: task.id.0,
            untrusted_title: "Ship the proposal".into(),
            deadline_unix_ms: Some(u64::try_from(deadline.timestamp_millis()).unwrap()),
            priority: TaskContextPriority::High,
        }
    );
    validate_native_context_view(
        &task_view,
        person,
        task_handle,
        u64::try_from(now.timestamp_millis()).unwrap(),
        8,
        MAX_NATIVE_CONTEXT_BYTES,
    )
    .unwrap();

    let note_handle = Uuid::new_v4();
    let note_view = note_context_view(
        &timeline,
        person,
        note_handle,
        now,
        8,
        MAX_NATIVE_CONTEXT_BYTES,
    )
    .await
    .unwrap();
    assert_eq!(note_view.view_id, FLOE_NOTE_VIEW_ID);
    assert_eq!(
        note_view.items[0],
        NativeContextItem::Note {
            evidence_handle: note.id.0,
            untrusted_excerpt: "Remember the launch constraint".into(),
            updated_at_unix_ms: u64::try_from(now.timestamp_millis()).unwrap(),
        }
    );

    assert_eq!(
        task_context_view(
            &timeline,
            person,
            task_handle,
            now,
            8,
            MAX_NATIVE_CONTEXT_BYTES
        )
        .await
        .unwrap(),
        task_view
    );
}

#[tokio::test]
async fn enforces_item_and_byte_budgets_before_exposure() {
    let timeline = TestTimelineRepository::new();
    let core = DayService::new(&timeline);
    let person = PersonId::new();
    let now = Utc.with_ymd_and_hms(2026, 9, 10, 9, 0, 0).unwrap();
    core.create_task(person, "one", None, Priority::Normal, now)
        .await
        .unwrap();
    core.create_task(person, "two", None, Priority::Normal, now)
        .await
        .unwrap();

    assert!(
        task_context_view(
            &timeline,
            person,
            Uuid::new_v4(),
            now,
            1,
            MAX_NATIVE_CONTEXT_BYTES
        )
        .await
        .is_err()
    );
    assert!(
        task_context_view(&timeline, person, Uuid::new_v4(), now, 8, 64)
            .await
            .is_err()
    );
}
