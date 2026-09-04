use chrono::{Duration, TimeZone, Utc};
use floe_core::{ErrorCode, FloeCore};
use floe_domain::*;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 4, 0, 0, 0).unwrap()
}

fn range(day: i64) -> CalendarRange {
    CalendarRange {
        start_date: (now() + Duration::days(day)).date_naive(),
        end_date_exclusive: (now() + Duration::days(day + 1)).date_naive(),
        timezone_offset_seconds: 32_400,
    }
}

fn record(identifier: &str, day: i64) -> CalendarRecord {
    CalendarRecord {
        external_id: identifier.into(),
        external_revision: "v1".into(),
        title: "Fixture event".into(),
        schedule: EventSchedule::Timed(
            TimedSchedule::new(
                now() + Duration::days(day),
                now() + Duration::days(day) + Duration::hours(1),
                "Asia/Seoul",
            )
            .unwrap(),
        ),
    }
}

async fn fixture() -> (FloeCore, PersonId, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("floe-calendar-{}.db", uuid::Uuid::new_v4()));
    let core = FloeCore::open(&path).await.unwrap();
    let person = PersonId::new();
    core.select_calendar(
        person,
        CalendarProvider::Fixture,
        "calendar-1".into(),
        "Test calendar".into(),
    )
    .await
    .unwrap();
    (core, person, path)
}

async fn snapshot(core: &FloeCore, person: PersonId, day: i64) -> DaySnapshot {
    core.day_snapshot(person, range(day).start_date, 32_400, now())
        .await
        .unwrap()
}

#[tokio::test]
async fn repeated_import_is_idempotent_and_updates_preserve_identity() {
    let (core, person, path) = fixture().await;
    core.import_calendar(person, 1, range(0), vec![record("external", 0)], now())
        .await
        .unwrap();
    let first = snapshot(&core, person, 0).await.items;
    core.import_calendar(
        person,
        2,
        range(0),
        vec![record("external", 0)],
        now() + Duration::minutes(1),
    )
    .await
    .unwrap();
    assert_eq!(snapshot(&core, person, 0).await.items, first);
    let mut changed = record("external", 0);
    changed.title = "Updated".into();
    changed.external_revision = "v2".into();
    core.import_calendar(person, 3, range(0), vec![changed], now())
        .await
        .unwrap();
    let updated = snapshot(&core, person, 0).await;
    let TimelineItem::Event(event) = &updated.items[0] else {
        panic!()
    };
    let TimelineItem::Event(original) = &first[0] else {
        panic!()
    };
    assert_eq!(event.id, original.id);
    assert_eq!(event.revision, original.revision.next());
    assert!(
        matches!(&event.source, SourceRef::Calendar(source) if source.external_revision == "v2")
    );
    drop(core);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn deletion_is_range_scoped_and_person_scoped() {
    let (core, person, path) = fixture().await;
    core.create_note(person, "Local note", now()).await.unwrap();
    core.import_calendar(person, 1, range(0), vec![record("today", 0)], now())
        .await
        .unwrap();
    core.import_calendar(person, 2, range(1), vec![record("tomorrow", 1)], now())
        .await
        .unwrap();
    core.import_calendar(person, 3, range(0), vec![], now())
        .await
        .unwrap();
    let today = snapshot(&core, person, 0).await;
    assert!(matches!(today.items.as_slice(), [TimelineItem::Note(_)]));
    assert_eq!(snapshot(&core, person, 1).await.items.len(), 1);
    assert!(snapshot(&core, PersonId::new(), 1).await.items.is_empty());
    drop(core);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn invalid_and_stale_batches_never_partially_replace_cache() {
    let (core, person, path) = fixture().await;
    core.import_calendar(person, 1, range(0), vec![record("today", 0)], now())
        .await
        .unwrap();
    let original = snapshot(&core, person, 0).await;
    for records in [
        vec![record("duplicate", 0), record("duplicate", 0)],
        vec![record("outside", 1)],
    ] {
        let error = core
            .import_calendar(person, 2, range(0), records, now())
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Validation);
        assert_eq!(snapshot(&core, person, 0).await, original);
    }
    assert_eq!(
        core.import_calendar(person, 1, range(0), vec![], now())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    core.select_calendar(
        person,
        CalendarProvider::Fixture,
        "new".into(),
        "Other".into(),
    )
    .await
    .unwrap();
    assert_eq!(
        core.import_calendar(person, 2, range(0), vec![record("old", 0)], now())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    assert!(snapshot(&core, person, 0).await.items.is_empty());
    drop(core);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn failure_and_cached_events_survive_reopen_and_retry() {
    let (core, person, path) = fixture().await;
    core.import_calendar(person, 1, range(0), vec![record("today", 0)], now())
        .await
        .unwrap();
    core.record_calendar_failure(person, 2, CalendarFailure::PermissionDenied)
        .await
        .unwrap();
    drop(core);
    let core = FloeCore::open(&path).await.unwrap();
    let cached = snapshot(&core, person, 0).await;
    assert_eq!(cached.items.len(), 1);
    assert_eq!(
        cached.calendar.unwrap().error,
        Some(CalendarFailure::PermissionDenied)
    );
    core.import_calendar(person, 3, range(0), vec![record("today", 0)], now())
        .await
        .unwrap();
    assert_eq!(
        snapshot(&core, person, 0).await.calendar.unwrap().error,
        None
    );
    drop(core);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn all_day_exclusive_end_and_utc_boundary_project_correctly() {
    let (core, person, path) = fixture().await;
    let mut all_day = record("all-day", 0);
    all_day.schedule = EventSchedule::AllDay(
        AllDaySchedule::new(range(0).start_date, range(0).end_date_exclusive).unwrap(),
    );
    let mut midnight = record("midnight", 0);
    midnight.schedule = EventSchedule::Timed(
        TimedSchedule::new(
            now() - Duration::hours(9),
            now() - Duration::hours(8),
            "Asia/Seoul",
        )
        .unwrap(),
    );
    core.import_calendar(person, 1, range(0), vec![all_day, midnight], now())
        .await
        .unwrap();
    assert_eq!(snapshot(&core, person, 0).await.items.len(), 2);
    assert!(snapshot(&core, person, -1).await.items.is_empty());
    assert!(snapshot(&core, person, 1).await.items.is_empty());
    drop(core);
    let _ = std::fs::remove_file(path);
}
