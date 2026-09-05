use chrono::{Duration, TimeZone, Utc};
use floe_core::FloeCore;
use floe_domain::*;

#[tokio::test]
async fn dst_days_import_and_project_the_exact_23_or_25_hour_interval() {
    for (month, day, start_offset, end_offset, hours) in
        [(3, 8, -28800, -25200, 23), (11, 1, -25200, -28800, 25)]
    {
        let directory = tempfile::tempdir().unwrap();
        let core = FloeCore::open(directory.path().join("dst.db"))
            .await
            .unwrap();
        let person = PersonId::new();
        core.select_calendar(
            person,
            CalendarProvider::Fixture,
            "dst".into(),
            "DST".into(),
        )
        .await
        .unwrap();
        let date = Utc.with_ymd_and_hms(2026, month, day, 0, 0, 0).unwrap();
        let start = date - Duration::seconds(start_offset.into());
        let end = start + Duration::hours(hours);
        let range = CalendarRange {
            start_date: date.date_naive(),
            end_date_exclusive: (date + Duration::days(1)).date_naive(),
            timezone_offset_seconds: start_offset,
            end_timezone_offset_seconds: Some(end_offset),
        };
        let schedule =
            TimedSchedule::new(end - Duration::minutes(15), end, "America/Los_Angeles").unwrap();
        core.import_calendar(
            person,
            1,
            range.clone(),
            vec![CalendarRecord {
                calendar_id: Some("dst".into()),
                external_id: "last-quarter".into(),
                external_revision: "1".into(),
                title: "Last quarter hour".into(),
                schedule: EventSchedule::Timed(schedule),
            }],
            start,
        )
        .await
        .unwrap();
        let snapshot = core
            .day_snapshot_with_end_offset(
                person,
                date.date_naive(),
                start_offset,
                Some(end_offset),
                start,
            )
            .await
            .unwrap();
        assert_eq!(snapshot.items.len(), 1);
        let outside = EventSchedule::Timed(
            TimedSchedule::new(end, end + Duration::minutes(15), "America/Los_Angeles").unwrap(),
        );
        assert!(!range.contains(&outside));
        let next = core
            .day_snapshot(person, range.end_date_exclusive, end_offset, end)
            .await
            .unwrap();
        assert!(next.items.is_empty());
    }
}
