use chrono::{Duration, TimeZone, Utc};
use floe_agent::{
    CONNECTED_CONTEXT_VERSION, ConnectionState, SituationDescriptor, SituationTrigger,
    SourceFailureKind, evaluate_situation, validate_connector_snapshot,
};
use floe_core::FloeCore;
use floe_domain::{
    CalendarBatch, CalendarFailure, CalendarProvider, CalendarRange, CalendarRecord,
    CalendarSelection, EventSchedule, PersonId, TimedSchedule,
};

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 10, 10, 0, 0).unwrap()
}

fn range() -> CalendarRange {
    CalendarRange {
        start_date: now().date_naive(),
        end_date_exclusive: (now() + Duration::days(1)).date_naive(),
        timezone_offset_seconds: 0,
        end_timezone_offset_seconds: None,
    }
}

fn batch(calendar_id: &str, title: &str) -> CalendarBatch {
    CalendarBatch {
        calendar_id: calendar_id.into(),
        records: vec![CalendarRecord {
            can_modify: false,
            calendar_id: calendar_id.into(),
            external_id: format!("{calendar_id}-event"),
            external_revision: "1".into(),
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
    let core = FloeCore::open(directory.path().join("connected-calendar.db"))
        .await
        .unwrap();
    let person_id = PersonId::new();
    core.select_calendars(
        person_id,
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
        person_id,
        1,
        range(),
        vec![batch("home", "Home"), batch("work", "Work")],
        now(),
    )
    .await
    .unwrap();
    (directory, core, person_id)
}

#[tokio::test]
async fn durable_calendar_projects_a_conforming_provider_neutral_snapshot() {
    let (directory, core, person_id) = setup().await;
    drop(core);
    let core = FloeCore::open(directory.path().join("connected-calendar.db"))
        .await
        .unwrap();

    let snapshot = core
        .calendar_connector_snapshot(person_id, "local-test-device", now() + Duration::minutes(1))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(snapshot.connection.state, ConnectionState::Ready);
    assert_eq!(snapshot.connection.granted_scopes, ["calendar.events.read"]);
    assert_eq!(snapshot.views.len(), 2);
    assert!(snapshot.views.iter().all(|view| {
        view.view_id == "calendar.timeline"
            && view.item_count == 1
            && view.provenance_count == 1
            && !view.source_handle.contains("home")
            && !view.source_handle.contains("work")
    }));
    assert!(
        validate_connector_snapshot(
            &snapshot,
            u64::try_from((now() + Duration::minutes(1)).timestamp_millis()).unwrap(),
        )
        .is_empty()
    );
    assert!(
        !snapshot
            .connection
            .granted_scopes
            .contains(&"calendar.events.write".into())
    );
}

#[tokio::test]
async fn parity_calendar_providers_project_the_same_conforming_contract() {
    for (provider, connector, provider_name) in [
        (
            CalendarProvider::Google,
            "calendar.google",
            "google_calendar",
        ),
        (
            CalendarProvider::Microsoft,
            "calendar.microsoft",
            "microsoft_calendar",
        ),
        (
            CalendarProvider::Android,
            "calendar.android",
            "android_calendar",
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let core = FloeCore::open(directory.path().join("parity-calendar.db"))
            .await
            .unwrap();
        let person_id = PersonId::new();
        core.select_calendars(
            person_id,
            provider,
            vec![CalendarSelection {
                calendar_id: "selected".into(),
                calendar_name: "Selected".into(),
            }],
        )
        .await
        .unwrap();
        core.import_calendar_sources(
            person_id,
            1,
            range(),
            vec![batch("selected", "Review")],
            now(),
        )
        .await
        .unwrap();
        let snapshot = core
            .calendar_connector_snapshot(person_id, "parity-device", now() + Duration::minutes(1))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(snapshot.descriptor.id, connector);
        assert_eq!(snapshot.descriptor.provider, provider_name);
        assert_eq!(snapshot.connection.connector_id, connector);
        assert!(
            validate_connector_snapshot(
                &snapshot,
                u64::try_from((now() + Duration::minutes(1)).timestamp_millis()).unwrap(),
            )
            .is_empty()
        );
    }
}

#[tokio::test]
async fn partial_source_failure_keeps_the_healthy_calendar_situation_runnable() {
    let (_directory, core, person_id) = setup().await;
    let failure_at = now() + Duration::minutes(1);
    core.import_calendar_sources(
        person_id,
        2,
        range(),
        vec![
            batch("home", "Updated"),
            CalendarBatch {
                calendar_id: "work".into(),
                records: vec![],
                failure: Some(CalendarFailure::PermissionDenied),
            },
        ],
        failure_at,
    )
    .await
    .unwrap();
    let snapshot = core
        .calendar_connector_snapshot(person_id, "local-test-device", failure_at)
        .await
        .unwrap()
        .unwrap();
    let situation = SituationDescriptor {
        schema_version: CONNECTED_CONTEXT_VERSION,
        id: "schedule.feasibility".into(),
        version: "1.0.0".into(),
        trigger: SituationTrigger::ExplicitForegroundRequest,
        required_view_ids: vec!["calendar.timeline".into()],
        optional_view_ids: vec!["weather.hourly".into()],
    };
    let report = evaluate_situation(
        &situation,
        std::slice::from_ref(&snapshot),
        u64::try_from(failure_at.timestamp_millis()).unwrap(),
    );

    assert_eq!(snapshot.connection.state, ConnectionState::Degraded);
    assert_eq!(
        snapshot.connection.last_failure.unwrap().kind,
        SourceFailureKind::PermissionDenied
    );
    assert_eq!(snapshot.views.len(), 1);
    assert!(report.can_run());
    assert_eq!(report.source_issues.len(), 1);
    assert_eq!(report.missing_optional_views, ["weather.hourly"]);
}

#[tokio::test]
async fn disconnect_and_reconnect_are_durable_and_never_restore_old_views() {
    let (directory, core, person_id) = setup().await;
    core.disconnect_calendar(person_id, 2).await.unwrap();
    drop(core);
    let core = FloeCore::open(directory.path().join("connected-calendar.db"))
        .await
        .unwrap();
    let disconnected = core
        .calendar_connector_snapshot(person_id, "local-test-device", now())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(disconnected.connection.state, ConnectionState::Disconnected);
    assert!(disconnected.connection.granted_scopes.is_empty());
    assert!(disconnected.views.is_empty());

    core.select_calendar(
        person_id,
        CalendarProvider::Fixture,
        "home".into(),
        "Home".into(),
    )
    .await
    .unwrap();
    let pending = core
        .calendar_connector_snapshot(person_id, "local-test-device", now())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.connection.state, ConnectionState::Pending);
    assert!(pending.views.is_empty());
    assert_eq!(
        core.calendar_connection(person_id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        4
    );
}

#[tokio::test]
async fn expired_calendar_projection_becomes_typed_unavailable_evidence() {
    let (_directory, core, person_id) = setup().await;
    let observed_at = now() + Duration::minutes(5);
    let snapshot = core
        .calendar_connector_snapshot(person_id, "local-test-device", observed_at)
        .await
        .unwrap()
        .unwrap();
    let situation = SituationDescriptor {
        schema_version: CONNECTED_CONTEXT_VERSION,
        id: "schedule.feasibility".into(),
        version: "1.0.0".into(),
        trigger: SituationTrigger::ExplicitForegroundRequest,
        required_view_ids: vec!["calendar.timeline".into()],
        optional_view_ids: vec![],
    };
    let report = evaluate_situation(
        &situation,
        std::slice::from_ref(&snapshot),
        u64::try_from(observed_at.timestamp_millis()).unwrap(),
    );

    assert_eq!(snapshot.connection.state, ConnectionState::Unavailable);
    assert_eq!(
        snapshot.connection.last_failure.unwrap().kind,
        SourceFailureKind::Stale
    );
    assert!(!report.can_run());
    assert_eq!(report.missing_required_views, ["calendar.timeline"]);
}
