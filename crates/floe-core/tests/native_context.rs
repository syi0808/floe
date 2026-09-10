use chrono::{TimeZone, Utc};
use floe_agent::{
    FLOE_NOTE_VIEW_ID, FLOE_TASK_VIEW_ID, MAX_NATIVE_CONTEXT_BYTES, NativeContextItem,
    TaskContextPriority, validate_native_context_view,
};
use floe_core::FloeCore;
use floe_domain::{PersonId, Priority};
use uuid::Uuid;

#[tokio::test]
async fn projects_bounded_floe_native_task_and_note_views() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("native-context.db");
    let core = FloeCore::open(&path).await.unwrap();
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
    core.complete_task(completed.id, completed.revision, now)
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
    let task_view = core
        .task_context_view(person, task_handle, now, 8, MAX_NATIVE_CONTEXT_BYTES)
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
    let note_view = core
        .note_context_view(person, note_handle, now, 8, MAX_NATIVE_CONTEXT_BYTES)
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

    drop(core);
    let reopened = FloeCore::open(path).await.unwrap();
    assert_eq!(
        reopened
            .task_context_view(person, task_handle, now, 8, MAX_NATIVE_CONTEXT_BYTES,)
            .await
            .unwrap(),
        task_view
    );
}

#[tokio::test]
async fn enforces_item_and_byte_budgets_before_exposure() {
    let directory = tempfile::tempdir().unwrap();
    let core = FloeCore::open(directory.path().join("native-context.db"))
        .await
        .unwrap();
    let person = PersonId::new();
    let now = Utc.with_ymd_and_hms(2026, 9, 10, 9, 0, 0).unwrap();
    core.create_task(person, "one", None, Priority::Normal, now)
        .await
        .unwrap();
    core.create_task(person, "two", None, Priority::Normal, now)
        .await
        .unwrap();

    assert!(
        core.task_context_view(person, Uuid::new_v4(), now, 1, MAX_NATIVE_CONTEXT_BYTES)
            .await
            .is_err()
    );
    assert!(
        core.task_context_view(person, Uuid::new_v4(), now, 8, 64)
            .await
            .is_err()
    );
}
