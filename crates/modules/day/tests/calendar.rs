use chrono::{Duration, TimeZone, Utc};
use floe_context_contract::CalendarProvider;
use floe_day::{
    AllDaySchedule, CalendarFailure, CalendarMirrorInput, CalendarRange, CalendarRecord,
    CalendarSelection, DayErrorCode, DayService, EventSchedule, PersonId, SourceRef, TimedSchedule,
    TimelineItem, TimelineRepository,
};

mod support;
use support::TestTimelineRepository;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 4, 0, 0, 0).unwrap()
}

fn range(day: i64) -> CalendarRange {
    CalendarRange {
        start_date: (now() + Duration::days(day)).date_naive(),
        end_date_exclusive: (now() + Duration::days(day + 1)).date_naive(),
        timezone_offset_seconds: 32_400,
        end_timezone_offset_seconds: None,
    }
}

fn input(connection_id: &str, calendar_id: &str) -> CalendarMirrorInput {
    CalendarMirrorInput {
        source_connection_id: connection_id.into(),
        provider: CalendarProvider::Fixture,
        calendars: vec![CalendarSelection {
            calendar_id: calendar_id.into(),
            calendar_name: "Test calendar".into(),
        }],
    }
}

fn record(identifier: &str, day: i64) -> CalendarRecord {
    CalendarRecord {
        can_modify: false,
        calendar_id: "calendar-1".into(),
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

#[tokio::test]
async fn imports_preserve_event_identity_and_advance_only_mirror_revision() {
    let timeline = TestTimelineRepository::new();
    let core = DayService::new(&timeline);
    let person = PersonId::new();
    let source = input("source-1", "calendar-1");
    core.import_calendar(
        person,
        None,
        source.clone(),
        range(0),
        vec![record("event", 0)],
        now(),
    )
    .await
    .unwrap();
    let initial = core
        .day_snapshot(person, range(0).start_date, 32_400, now())
        .await
        .unwrap();
    let TimelineItem::Event(original) = &initial.items[0] else {
        panic!()
    };
    assert_eq!(initial.calendar_mirror_revision, Some(1));
    assert_eq!(
        initial.calendar.as_ref().unwrap().source_connection_id,
        "source-1"
    );
    core.import_calendar(
        person,
        Some(1),
        source.clone(),
        range(0),
        vec![record("event", 0)],
        now() + Duration::minutes(1),
    )
    .await
    .unwrap();
    let repeated = core
        .day_snapshot(person, range(0).start_date, 32_400, now())
        .await
        .unwrap();
    assert_eq!(repeated.items, initial.items);
    assert_eq!(repeated.calendar_mirror_revision, Some(2));
    let mut changed = record("event", 0);
    changed.title = "Updated".into();
    changed.external_revision = "v2".into();
    core.import_calendar(person, Some(2), source, range(0), vec![changed], now())
        .await
        .unwrap();
    let updated = core
        .day_snapshot(person, range(0).start_date, 32_400, now())
        .await
        .unwrap();
    let TimelineItem::Event(event) = &updated.items[0] else {
        panic!()
    };
    assert_eq!(event.id, original.id);
    assert_eq!(event.revision, original.revision.next());
    assert!(
        matches!(&event.source, SourceRef::Calendar(source) if source.external_revision == "v2")
    );
    assert_eq!(updated.calendar_mirror_revision, Some(3));
}

#[tokio::test]
async fn invalid_or_stale_import_does_not_replace_mirror() {
    let timeline = TestTimelineRepository::new();
    let core = DayService::new(&timeline);
    let person = PersonId::new();
    let source = input("source-1", "calendar-1");
    core.import_calendar(
        person,
        None,
        source.clone(),
        range(0),
        vec![record("event", 0)],
        now(),
    )
    .await
    .unwrap();
    let before = timeline.calendar_mirror(person).await.unwrap().unwrap();
    for records in [
        vec![record("duplicate", 0), record("duplicate", 0)],
        vec![record("outside", 1)],
    ] {
        assert_eq!(
            core.import_calendar(person, Some(1), source.clone(), range(0), records, now())
                .await
                .unwrap_err()
                .code,
            DayErrorCode::Validation
        );
        assert_eq!(
            timeline.calendar_mirror(person).await.unwrap(),
            Some(before.clone())
        );
    }
    assert_eq!(
        core.import_calendar(person, None, source, range(0), vec![], now())
            .await
            .unwrap_err()
            .code,
        DayErrorCode::Conflict
    );
}

#[tokio::test]
async fn range_deletion_and_failure_preserve_unrelated_items() {
    let timeline = TestTimelineRepository::new();
    let core = DayService::new(&timeline);
    let person = PersonId::new();
    let source = input("source-1", "calendar-1");
    core.create_note(person, "Local note", now()).await.unwrap();
    core.import_calendar(
        person,
        None,
        source.clone(),
        range(0),
        vec![record("today", 0)],
        now(),
    )
    .await
    .unwrap();
    core.import_calendar(
        person,
        Some(1),
        source.clone(),
        range(1),
        vec![record("tomorrow", 1)],
        now(),
    )
    .await
    .unwrap();
    core.record_calendar_failure(
        person,
        Some(2),
        source.clone(),
        CalendarFailure::ProviderUnavailable,
        now(),
    )
    .await
    .unwrap();
    let failed = core
        .day_snapshot(person, range(0).start_date, 32_400, now())
        .await
        .unwrap();
    assert_eq!(failed.items.len(), 2);
    assert_eq!(
        failed.calendar.as_ref().unwrap().error,
        Some(CalendarFailure::ProviderUnavailable)
    );
    core.import_calendar(person, Some(3), source.clone(), range(0), vec![], now())
        .await
        .unwrap();
    let today = core
        .day_snapshot(person, range(0).start_date, 32_400, now())
        .await
        .unwrap();
    assert!(matches!(today.items.as_slice(), [TimelineItem::Note(_)]));
    assert_eq!(today.calendar.unwrap().error, None);
    assert_eq!(
        core.day_snapshot(person, range(1).start_date, 32_400, now())
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(
        core.day_snapshot(PersonId::new(), range(1).start_date, 32_400, now())
            .await
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn new_source_resets_observed_events_and_advances_mirror_revision() {
    let timeline = TestTimelineRepository::new();
    let core = DayService::new(&timeline);
    let person = PersonId::new();
    core.import_calendar(
        person,
        None,
        input("source-1", "calendar-1"),
        range(0),
        vec![record("old", 0)],
        now(),
    )
    .await
    .unwrap();
    core.import_calendar(
        person,
        Some(1),
        input("source-2", "new"),
        range(0),
        vec![],
        now(),
    )
    .await
    .unwrap();
    let snapshot = core
        .day_snapshot(person, range(0).start_date, 32_400, now())
        .await
        .unwrap();
    assert!(snapshot.items.is_empty());
    assert_eq!(snapshot.calendar.unwrap().source_connection_id, "source-2");
    assert_eq!(snapshot.calendar_mirror_revision, Some(2));
}

#[tokio::test]
async fn all_day_exclusive_end_and_utc_boundary_project_correctly() {
    let timeline = TestTimelineRepository::new();
    let core = DayService::new(&timeline);
    let person = PersonId::new();
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
    core.import_calendar(
        person,
        None,
        input("source-1", "calendar-1"),
        range(0),
        vec![all_day, midnight],
        now(),
    )
    .await
    .unwrap();
    assert_eq!(
        core.day_snapshot(person, range(0).start_date, 32_400, now())
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    assert!(
        core.day_snapshot(person, range(-1).start_date, 32_400, now())
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        core.day_snapshot(person, range(1).start_date, 32_400, now())
            .await
            .unwrap()
            .items
            .is_empty()
    );
}
