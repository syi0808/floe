use chrono::{Duration, TimeZone, Utc};
use floe_core::{ErrorCode, FloeCore};
use floe_domain::*;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 5, 1, 0, 0).unwrap()
}

fn range() -> CalendarRange {
    CalendarRange {
        start_date: now().date_naive(),
        end_date_exclusive: (now() + Duration::days(1)).date_naive(),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
    }
}

fn batch(calendar: &str, title: &str) -> CalendarBatch {
    CalendarBatch {
        calendar_id: calendar.into(),
        records: vec![CalendarRecord {
            can_modify: false,
            calendar_id: Some(calendar.into()),
            external_id: "same-provider-id".into(),
            external_revision: title.into(),
            title: title.into(),
            schedule: EventSchedule::Timed(
                TimedSchedule::new(now(), now() + Duration::hours(1), "UTC").unwrap(),
            ),
        }],
        failure: None,
    }
}

async fn setup() -> (tempfile::TempDir, FloeCore, PersonId) {
    let directory = tempfile::tempdir().unwrap();
    let core = FloeCore::open(directory.path().join("sources.db"))
        .await
        .unwrap();
    let person = PersonId::new();
    core.select_calendars(
        person,
        CalendarProvider::Fixture,
        vec![
            CalendarSelection {
                calendar_id: "home".into(),
                calendar_name: "Home".into(),
            },
            CalendarSelection {
                calendar_id: "work".into(),
                calendar_name: "Work".into(),
            },
        ],
    )
    .await
    .unwrap();
    core.import_calendar_sources(
        person,
        1,
        range(),
        vec![batch("home", "Home"), batch("work", "Work")],
        now(),
    )
    .await
    .unwrap();
    (directory, core, person)
}

#[tokio::test]
async fn disconnect_removes_imports_and_reconnect_never_reuses_a_revision() {
    let (_directory, core, person) = setup().await;
    core.disconnect_calendar(person, 2).await.unwrap();
    let snapshot = core
        .day_snapshot(person, now().date_naive(), 0, now())
        .await
        .unwrap();
    assert!(snapshot.calendar.is_none());
    assert!(snapshot.items.is_empty());
    assert!(
        core.import_calendar_sources(
            person,
            2,
            range(),
            vec![batch("home", "Late"), batch("work", "Late")],
            now()
        )
        .await
        .is_err()
    );
    core.select_calendar(
        person,
        CalendarProvider::Fixture,
        "home".into(),
        "Home".into(),
    )
    .await
    .unwrap();
    let snapshot = core
        .day_snapshot(person, now().date_naive(), 0, now())
        .await
        .unwrap();
    assert_eq!(snapshot.calendar.unwrap().revision, 4);
    assert_eq!(
        core.import_calendar_sources(person, 2, range(), vec![batch("home", "Late")], now())
            .await
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
    core.import_calendar_sources(person, 4, range(), vec![batch("home", "Fresh")], now())
        .await
        .unwrap();
}

#[tokio::test]
async fn partial_success_commits_only_healthy_source_and_survives_restart() {
    let (directory, core, person) = setup().await;
    let baseline = core
        .day_snapshot(person, now().date_naive(), 0, now())
        .await
        .unwrap();
    let failure = CalendarBatch {
        calendar_id: "work".into(),
        records: vec![],
        failure: Some(CalendarFailure::PermissionDenied),
    };
    let updated_at = now() + Duration::minutes(1);
    core.import_calendar_sources(
        person,
        2,
        range(),
        vec![batch("home", "Updated"), failure],
        updated_at,
    )
    .await
    .unwrap();
    drop(core);
    let core = FloeCore::open(directory.path().join("sources.db"))
        .await
        .unwrap();
    let after = core
        .day_snapshot(person, now().date_naive(), 0, now())
        .await
        .unwrap();
    assert_eq!(after.items.len(), 2);
    for old in &baseline.items {
        let TimelineItem::Event(old) = old else {
            panic!()
        };
        let new = after
            .items
            .iter()
            .find_map(|item| match item {
                TimelineItem::Event(event) if event.id == old.id => Some(event),
                _ => None,
            })
            .unwrap();
        if old.title == "Work" {
            assert_eq!(old, new);
        } else {
            assert_eq!(new.title, "Updated");
        }
    }
    let connection = after.calendar.unwrap();
    assert_eq!(
        connection.source_statuses["home"].last_success_at,
        Some(updated_at)
    );
    assert_eq!(
        connection.source_statuses["work"].last_success_at,
        Some(now())
    );
    assert_eq!(
        connection.source_statuses["work"].error,
        Some(CalendarFailure::PermissionDenied)
    );
    assert_eq!(connection.last_success_at, Some(now()));
}

#[tokio::test]
async fn invalid_source_is_preserved_while_healthy_empty_result_deletes_only_its_source() {
    let (_directory, core, person) = setup().await;
    let mut invalid = batch("work", "Invalid");
    invalid.records[0].calendar_id = Some("home".into());
    core.import_calendar_sources(
        person,
        2,
        range(),
        vec![
            CalendarBatch {
                calendar_id: "home".into(),
                records: vec![],
                failure: None,
            },
            invalid,
        ],
        now(),
    )
    .await
    .unwrap();
    let snapshot = core
        .day_snapshot(person, now().date_naive(), 0, now())
        .await
        .unwrap();
    assert_eq!(snapshot.items.len(), 1);
    let TimelineItem::Event(event) = &snapshot.items[0] else {
        panic!()
    };
    assert_eq!(event.title, "Work");
    assert_eq!(
        snapshot.calendar.unwrap().source_statuses["work"].error,
        Some(CalendarFailure::ProviderUnavailable)
    );
}

#[tokio::test]
async fn missing_source_is_not_empty_and_stale_or_incomplete_batches_are_rejected() {
    let (_directory, core, person) = setup().await;
    for batches in [
        vec![batch("home", "No")],
        vec![batch("home", "No"), batch("home", "No")],
    ] {
        assert_eq!(
            core.import_calendar_sources(person, 2, range(), batches, now())
                .await
                .unwrap_err()
                .code,
            ErrorCode::Validation
        );
    }
    core.import_calendar_sources(
        person,
        2,
        range(),
        vec![
            batch("home", "Home"),
            CalendarBatch {
                calendar_id: "work".into(),
                records: vec![],
                failure: Some(CalendarFailure::CalendarUnavailable),
            },
        ],
        now(),
    )
    .await
    .unwrap();
    assert_eq!(
        core.import_calendar_sources(
            person,
            2,
            range(),
            vec![batch("home", "No"), batch("work", "No")],
            now()
        )
        .await
        .unwrap_err()
        .code,
        ErrorCode::Conflict
    );
    let snapshot = core
        .day_snapshot(person, now().date_naive(), 0, now())
        .await
        .unwrap();
    assert_eq!(snapshot.items.len(), 2);
    assert_eq!(
        snapshot.calendar.unwrap().source_statuses["work"].error,
        Some(CalendarFailure::CalendarUnavailable)
    );
    core.import_calendar_sources(
        person,
        3,
        range(),
        vec![batch("home", "Home"), batch("work", "Work")],
        now(),
    )
    .await
    .unwrap();
    assert!(
        core.day_snapshot(person, now().date_naive(), 0, now())
            .await
            .unwrap()
            .calendar
            .unwrap()
            .error
            .is_none()
    );
}

#[tokio::test]
async fn only_explicit_all_scope_discovers_new_sources_and_mode_survives_restart() {
    let (directory, core, person) = setup().await;
    let new_calendar = CalendarSelection {
        calendar_id: "new".into(),
        calendar_name: "New".into(),
    };
    assert_eq!(
        core.discover_calendars(person, 2, vec![new_calendar.clone()])
            .await
            .unwrap_err()
            .code,
        ErrorCode::Validation
    );
    let old = core
        .day_snapshot(person, now().date_naive(), 0, now())
        .await
        .unwrap()
        .calendar
        .unwrap();
    core.set_calendar_scope(
        person,
        CalendarProvider::Fixture,
        old.selected_calendars(),
        CalendarScope::All,
    )
    .await
    .unwrap();
    core.discover_calendars(person, 3, vec![new_calendar.clone()])
        .await
        .unwrap();
    drop(core);
    let core = FloeCore::open(directory.path().join("sources.db"))
        .await
        .unwrap();
    let snapshot = core
        .day_snapshot(person, now().date_naive(), 0, now())
        .await
        .unwrap();
    let connection = snapshot.calendar.unwrap();
    assert_eq!(connection.scope, CalendarScope::All);
    assert_eq!(connection.selected_calendars().len(), 3);
    assert!(connection.source_statuses["new"].last_success_at.is_none());
    assert_eq!(snapshot.items.len(), 2);
    core.set_calendar_scope(
        person,
        CalendarProvider::Fixture,
        vec![new_calendar.clone()],
        CalendarScope::Selected,
    )
    .await
    .unwrap();
    assert_eq!(
        core.discover_calendars(person, 4, vec![new_calendar])
            .await
            .unwrap_err()
            .code,
        ErrorCode::Conflict
    );
}

#[tokio::test]
async fn nonrecurring_eventkit_identity_migrates_and_follows_a_move_outside_the_old_range() {
    let directory = tempfile::tempdir().unwrap();
    let core = FloeCore::open(directory.path().join("identity.db"))
        .await
        .unwrap();
    let person = PersonId::new();
    core.select_calendar(
        person,
        CalendarProvider::EventKit,
        "home".into(),
        "Home".into(),
    )
    .await
    .unwrap();
    let mut record = batch("home", "Moving").records.remove(0);
    record.external_id = "native-item|2026-09-05T01:00:00.000Z".into();
    core.import_calendar(person, 1, range(), vec![record.clone()], now())
        .await
        .unwrap();
    let previous = core
        .day_snapshot(person, now().date_naive(), 0, now())
        .await
        .unwrap();
    let TimelineItem::Event(previous) = &previous.items[0] else {
        panic!()
    };
    record.external_id = "native-item|".into();
    record.schedule = EventSchedule::Timed(
        TimedSchedule::new(
            now() + Duration::days(1),
            now() + Duration::days(1) + Duration::hours(1),
            "UTC",
        )
        .unwrap(),
    );
    let mut next_range = range();
    next_range.start_date += Duration::days(1);
    next_range.end_date_exclusive += Duration::days(1);
    core.import_calendar(person, 2, next_range.clone(), vec![record], now())
        .await
        .unwrap();
    let next = core
        .day_snapshot(person, next_range.start_date, 0, now())
        .await
        .unwrap();
    let TimelineItem::Event(updated) = &next.items[0] else {
        panic!()
    };
    assert_eq!(previous.id, updated.id);
    assert!(
        core.day_snapshot(person, range().start_date, 0, now())
            .await
            .unwrap()
            .items
            .is_empty()
    );
}
