use chrono::{Duration, TimeZone, Utc};
use floe_context_contract::{CalendarProvider, CalendarScope};
use floe_day::{
    AllDaySchedule, CalendarConnection, CalendarFailure, CalendarRange, CalendarRecord,
    CalendarSelection, DayErrorCode as ErrorCode, DayService, DaySnapshot, EventSchedule, PersonId,
    SourceAuthority, SourceRef, TimedSchedule, TimelineItem, TimelineRepository,
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

async fn fixture(
    timeline: &TestTimelineRepository,
) -> (DayService<'_, TestTimelineRepository>, PersonId) {
    let core = DayService::new(timeline);
    let person = PersonId::new();
    core.select_calendar(
        person,
        CalendarProvider::Fixture,
        "calendar-1".into(),
        "Test calendar".into(),
    )
    .await
    .unwrap();
    (core, person)
}

#[tokio::test]
async fn authority_survives_sync_but_not_permission_or_scope_changes() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = fixture(&timeline).await;
    let initial = core.calendar_connection(person).await.unwrap().unwrap();
    assert!(initial.source_authority.is_valid());
    let mut obsolete = serde_json::to_value(&initial).unwrap();
    obsolete.as_object_mut().unwrap().remove("source_authority");
    assert!(serde_json::from_value::<CalendarConnection>(obsolete).is_err());
    core.import_calendar(person, initial.revision, range(0), vec![], now())
        .await
        .unwrap();
    let synced = core.calendar_connection(person).await.unwrap().unwrap();
    assert!(synced.revision > initial.revision);
    assert_eq!(synced.source_authority, initial.source_authority);
    core.record_calendar_failure(
        person,
        synced.revision,
        CalendarFailure::ProviderUnavailable,
        now(),
    )
    .await
    .unwrap();
    let unavailable = core.calendar_connection(person).await.unwrap().unwrap();
    assert_eq!(unavailable.source_authority, initial.source_authority);
    core.record_calendar_failure(
        person,
        unavailable.revision,
        CalendarFailure::PermissionDenied,
        now(),
    )
    .await
    .unwrap();
    let revoked = core.calendar_connection(person).await.unwrap().unwrap();
    assert_ne!(revoked.source_authority, initial.source_authority);
    core.record_calendar_failure(
        person,
        revoked.revision,
        CalendarFailure::PermissionDenied,
        now(),
    )
    .await
    .unwrap();
    let still_revoked = core.calendar_connection(person).await.unwrap().unwrap();
    assert_eq!(still_revoked.source_authority, revoked.source_authority);
    core.import_calendar(person, still_revoked.revision, range(0), vec![], now())
        .await
        .unwrap();
    let restored = core.calendar_connection(person).await.unwrap().unwrap();
    assert_eq!(restored.source_authority, revoked.source_authority);
    core.set_calendar_scope(
        person,
        restored.connection_id.clone(),
        restored.revision + 1,
        restored.device_id.clone(),
        restored.provider,
        vec![CalendarSelection {
            calendar_id: "calendar-1".into(),
            calendar_name: "Renamed".into(),
        }],
        restored.scope,
    )
    .await
    .unwrap();
    let renamed = core.calendar_connection(person).await.unwrap().unwrap();
    assert_eq!(renamed.source_authority, restored.source_authority);
    core.set_calendar_scope(
        person,
        renamed.connection_id.clone(),
        renamed.revision + 1,
        renamed.device_id.clone(),
        renamed.provider,
        selection("calendar-2"),
        renamed.scope,
    )
    .await
    .unwrap();
    let changed = core.calendar_connection(person).await.unwrap().unwrap();
    assert_ne!(changed.source_authority, renamed.source_authority);
}

#[test]
fn authority_rejects_zero_epoch() {
    assert!(
        serde_json::from_value::<SourceAuthority>(serde_json::json!({
            "incarnation": uuid::Uuid::new_v4(), "epoch": 0
        }))
        .is_err()
    );
}

fn selection(identifier: &str) -> Vec<CalendarSelection> {
    vec![CalendarSelection {
        calendar_id: identifier.into(),
        calendar_name: identifier.into(),
    }]
}

async fn snapshot<R: TimelineRepository + ?Sized>(
    core: &DayService<'_, R>,
    person: PersonId,
    day: i64,
) -> DaySnapshot {
    core.day_snapshot(person, range(day).start_date, 32_400, now())
        .await
        .unwrap()
}

#[tokio::test]
async fn multiple_selection_rejects_invalid_sources_and_stale_reads() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = fixture(&timeline).await;
    let calendars = vec![
        CalendarSelection {
            calendar_id: "calendar-1".into(),
            calendar_name: "Home".into(),
        },
        CalendarSelection {
            calendar_id: "calendar-2".into(),
            calendar_name: "Work".into(),
        },
    ];
    for invalid in [vec![], vec![calendars[0].clone(), calendars[0].clone()]] {
        assert_eq!(
            core.select_calendars(person, CalendarProvider::Fixture, invalid)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Validation
        );
    }
    core.select_calendars(person, CalendarProvider::Fixture, calendars.clone())
        .await
        .unwrap();
    let selected = snapshot(&core, person, 0).await;
    let revision = selected.calendar.as_ref().unwrap().revision;
    let mut unknown = record("event", 0);
    unknown.calendar_id = "unknown".into();
    assert_eq!(
        core.import_calendar(person, revision, range(0), vec![unknown], now())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Validation
    );
    assert_eq!(snapshot(&core, person, 0).await, selected);
    core.select_calendars(
        person,
        CalendarProvider::Fixture,
        calendars.into_iter().rev().collect(),
    )
    .await
    .unwrap();
    assert_eq!(snapshot(&core, person, 0).await, selected);
    assert_eq!(
        core.import_calendar(person, revision - 1, range(0), vec![], now())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
}

#[tokio::test]
async fn connection_revision_is_monotonic_and_equal_revision_is_exactly_idempotent() {
    let timeline = TestTimelineRepository::new();
    let core = DayService::new(&timeline);
    let person = PersonId::new();
    let connection_id = "00000000-0000-4000-8000-000000000010";
    core.set_calendar_scope(
        person,
        connection_id.into(),
        7,
        "device-a".into(),
        CalendarProvider::EventKit,
        selection("primary"),
        CalendarScope::Selected,
    )
    .await
    .unwrap();
    core.set_calendar_scope(
        person,
        connection_id.into(),
        7,
        "device-a".into(),
        CalendarProvider::EventKit,
        selection("primary"),
        CalendarScope::Selected,
    )
    .await
    .unwrap();

    for (revision, calendars, scope) in [
        (6, selection("primary"), CalendarScope::Selected),
        (7, selection("secondary"), CalendarScope::Selected),
        (7, selection("primary"), CalendarScope::All),
    ] {
        assert_eq!(
            core.set_calendar_scope(
                person,
                connection_id.into(),
                revision,
                "device-a".into(),
                CalendarProvider::EventKit,
                calendars,
                scope,
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::Conflict
        );
    }

    core.set_calendar_scope(
        person,
        connection_id.into(),
        8,
        "device-a".into(),
        CalendarProvider::EventKit,
        selection("primary"),
        CalendarScope::All,
    )
    .await
    .unwrap();
    assert_eq!(
        core.set_calendar_scope(
            person,
            connection_id.into(),
            7,
            "device-a".into(),
            CalendarProvider::EventKit,
            selection("primary"),
            CalendarScope::Selected,
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::Conflict
    );
    let connection = core.calendar_connection(person).await.unwrap().unwrap();
    assert_eq!(connection.revision, 8);
    assert_eq!(connection.scope, CalendarScope::All);
}

#[tokio::test]
async fn repeated_import_is_idempotent_and_updates_preserve_identity() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = fixture(&timeline).await;
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
}

#[tokio::test]
async fn deletion_is_range_scoped_and_person_scoped() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = fixture(&timeline).await;
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
}

#[tokio::test]
async fn invalid_and_stale_batches_never_partially_replace_cache() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = fixture(&timeline).await;
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
}

#[tokio::test]
async fn recorded_failure_keeps_cached_events_and_a_later_import_clears_it() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = fixture(&timeline).await;
    core.import_calendar(person, 1, range(0), vec![record("today", 0)], now())
        .await
        .unwrap();
    core.record_calendar_failure(person, 2, CalendarFailure::PermissionDenied, now())
        .await
        .unwrap();
    let cached = snapshot(&core, person, 0).await;
    assert_eq!(cached.items.len(), 1);
    let connection = cached.calendar.unwrap();
    assert_eq!(connection.error, Some(CalendarFailure::PermissionDenied));
    assert_eq!(connection.error_at, Some(now()));
    core.import_calendar(person, 3, range(0), vec![record("today", 0)], now())
        .await
        .unwrap();
    let connection = snapshot(&core, person, 0).await.calendar.unwrap();
    assert_eq!(connection.error, None);
    assert_eq!(connection.error_at, None);
}

#[tokio::test]
async fn all_day_exclusive_end_and_utc_boundary_project_correctly() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = fixture(&timeline).await;
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
}
