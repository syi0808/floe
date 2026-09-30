use chrono::{Duration, TimeZone, Utc};
use floe_context_contract::CalendarProvider;
use floe_day::{
    CalendarBatch, CalendarFailure, CalendarMirrorInput, CalendarRange, CalendarRecord,
    CalendarSelection, DayErrorCode, DayService, EventSchedule, PersonId, TimedSchedule,
    TimelineItem, TimelineRepository,
};

use crate::support;
use support::TestTimelineRepository;

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

fn input() -> CalendarMirrorInput {
    CalendarMirrorInput {
        source_connection_id: "source-1".into(),
        provider: CalendarProvider::Fixture,
        calendars: ["home", "work"]
            .into_iter()
            .map(|id| CalendarSelection {
                calendar_id: id.into(),
                calendar_name: id.into(),
            })
            .collect(),
    }
}

fn batch(calendar: &str, title: &str) -> CalendarBatch {
    CalendarBatch {
        calendar_id: calendar.into(),
        records: vec![CalendarRecord {
            can_modify: false,
            calendar_id: calendar.into(),
            external_id: "shared-provider-id".into(),
            external_revision: title.into(),
            title: title.into(),
            schedule: EventSchedule::Timed(
                TimedSchedule::new(now(), now() + Duration::hours(1), "UTC").unwrap(),
            ),
        }],
        failure: None,
    }
}

async fn setup(
    timeline: &TestTimelineRepository,
) -> (DayService<'_, TestTimelineRepository>, PersonId) {
    let core = DayService::new(timeline);
    let person = PersonId::new();
    core.import_calendar_sources(
        person,
        None,
        input(),
        range(),
        vec![batch("home", "Home"), batch("work", "Work")],
        now(),
    )
    .await
    .unwrap();
    (core, person)
}

#[tokio::test]
async fn partial_success_preserves_failed_source_and_updates_statuses() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = setup(&timeline).await;
    let before = timeline.calendar_mirror(person).await.unwrap().unwrap();
    let updated_at = now() + Duration::minutes(1);
    core.import_calendar_sources(
        person,
        Some(1),
        input(),
        range(),
        vec![
            batch("home", "Updated"),
            CalendarBatch {
                calendar_id: "work".into(),
                records: vec![],
                failure: Some(CalendarFailure::PermissionDenied),
            },
        ],
        updated_at,
    )
    .await
    .unwrap();
    let after = timeline.calendar_mirror(person).await.unwrap().unwrap();
    assert_eq!(after.mirror_revision, 2);
    assert_eq!(after.events.len(), 2);
    let original_work = before
        .events
        .iter()
        .find(|event| event.title == "Work")
        .unwrap();
    assert!(after.events.contains(original_work));
    assert!(after.events.iter().any(|event| event.title == "Updated"));
    assert_eq!(
        after.state.source_statuses["home"].last_success_at,
        Some(updated_at)
    );
    assert_eq!(
        after.state.source_statuses["work"].last_success_at,
        Some(now())
    );
    assert_eq!(
        after.state.source_statuses["work"].error,
        Some(CalendarFailure::PermissionDenied)
    );
    assert_eq!(after.state.error_at, Some(updated_at));
    assert_eq!(after.state.last_success_at, Some(now()));
}

#[tokio::test]
async fn malformed_source_is_preserved_while_healthy_empty_result_removes_only_its_events() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = setup(&timeline).await;
    let mut invalid = batch("work", "Invalid");
    invalid.records[0].calendar_id = "home".into();
    core.import_calendar_sources(
        person,
        Some(1),
        input(),
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
    assert!(matches!(&snapshot.items[0], TimelineItem::Event(event) if event.title == "Work"));
    assert_eq!(
        snapshot.calendar.unwrap().source_statuses["work"].error,
        Some(CalendarFailure::ProviderUnavailable)
    );
}

#[tokio::test]
async fn complete_exact_batch_and_mirror_cas_are_required() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = setup(&timeline).await;
    let before = timeline.calendar_mirror(person).await.unwrap().unwrap();
    for batches in [
        vec![batch("home", "No")],
        vec![batch("home", "No"), batch("home", "No")],
    ] {
        assert_eq!(
            core.import_calendar_sources(person, Some(1), input(), range(), batches, now())
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
        core.import_calendar_sources(
            person,
            None,
            input(),
            range(),
            vec![batch("home", "No"), batch("work", "No")],
            now()
        )
        .await
        .unwrap_err()
        .code,
        DayErrorCode::Conflict
    );
    assert_eq!(
        timeline.calendar_mirror(person).await.unwrap(),
        Some(before)
    );
}

#[tokio::test]
async fn current_input_prunes_removed_resource_without_storing_an_authority_list() {
    let timeline = TestTimelineRepository::new();
    let (core, person) = setup(&timeline).await;
    let mut current = input();
    current
        .calendars
        .retain(|calendar| calendar.calendar_id == "home");
    core.import_calendar_sources(
        person,
        Some(1),
        current,
        range(),
        vec![batch("home", "Home")],
        now(),
    )
    .await
    .unwrap();
    let mirror = timeline.calendar_mirror(person).await.unwrap().unwrap();
    assert_eq!(mirror.events.len(), 1);
    assert!(!mirror.state.source_statuses.contains_key("work"));
    assert_eq!(mirror.state.source_connection_id, "source-1");
}
