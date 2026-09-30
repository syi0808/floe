use chrono::{Duration, TimeZone, Utc};
use floe_app::FloeCore;
use floe_connections::{
    CONNECTED_CONTEXT_VERSION, ConnectionId, ConnectionResource, ConnectionState, ConnectorId,
    ResourceMode, SituationDescriptor, SituationTrigger, SourceFailureKind, evaluate_situation,
    validate_connector_snapshot,
};
use floe_context_contract::{ExecutionOwnerId, ResourceHandle};
use floe_day::{
    CalendarBatch, CalendarFailure, CalendarRange, CalendarRecord, EventSchedule, TimedSchedule,
};
use floe_kernel::PersonId;

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

async fn establish(
    core: &FloeCore,
    person_id: PersonId,
    connector: &str,
    connection_id: &str,
    device_id: &str,
    calendars: &[(&str, &str)],
) -> floe_connections::SourceConnection {
    core.source_service()
        .establish(
            person_id,
            ConnectorId::try_new(connector).unwrap(),
            ConnectionId::try_new(connection_id).unwrap(),
            ExecutionOwnerId::try_new(device_id).unwrap(),
            ResourceMode::Selected,
            calendars
                .iter()
                .map(|(handle, label)| {
                    ConnectionResource::new(
                        ResourceHandle::try_new(*handle).unwrap(),
                        (*label).into(),
                    )
                    .unwrap()
                })
                .collect(),
        )
        .await
        .unwrap()
}

async fn setup() -> (tempfile::TempDir, FloeCore, PersonId, ConnectionId) {
    let directory = tempfile::tempdir().unwrap();
    let core = FloeCore::open(directory.path().join("connected-calendar.db"))
        .await
        .unwrap();
    let person_id = PersonId::new();
    let source = establish(
        &core,
        person_id,
        "calendar.fixture",
        "calendar.fixture",
        "local-test-device",
        &[("home", "Home"), ("work", "Work")],
    )
    .await;
    core.import_calendar_sources(
        person_id,
        source.connection_id(),
        "local-test-device",
        None,
        range(),
        vec![batch("home", "Home"), batch("work", "Work")],
        now(),
    )
    .await
    .unwrap();
    (directory, core, person_id, source.connection_id().clone())
}

#[tokio::test]
async fn durable_calendar_projects_a_conforming_provider_neutral_snapshot() {
    let (directory, core, person_id, _) = setup().await;
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
    for (connector, provider_name) in [
        ("calendar.google", "google_calendar"),
        ("calendar.microsoft", "microsoft_calendar"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let core = FloeCore::open(directory.path().join("parity-calendar.db"))
            .await
            .unwrap();
        let person_id = PersonId::new();
        let source = establish(
            &core,
            person_id,
            connector,
            connector,
            "parity-device",
            &[("selected", "Selected")],
        )
        .await;
        assert_eq!(source.connector_id().as_str(), connector);
        core.import_calendar_sources(
            person_id,
            source.connection_id(),
            "parity-device",
            None,
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
    let (_directory, core, person_id, connection_id) = setup().await;
    let failure_at = now() + Duration::minutes(1);
    core.import_calendar_sources(
        person_id,
        &connection_id,
        "local-test-device",
        Some(1),
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
        id: "schedule.availability".into(),
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
async fn source_edit_advances_authority_while_mirror_sync_only_advances_mirror_revision() {
    let (_directory, core, person_id, connection_id) = setup().await;
    let source = core
        .source_service()
        .load(person_id, &connection_id)
        .await
        .unwrap()
        .unwrap();
    core.import_calendar_sources(
        person_id,
        &connection_id,
        "local-test-device",
        Some(1),
        range(),
        vec![batch("home", "Home"), batch("work", "Work")],
        now() + Duration::minutes(1),
    )
    .await
    .unwrap();
    assert_eq!(
        core.source_service()
            .load(person_id, &connection_id)
            .await
            .unwrap(),
        Some(source.clone())
    );
    let edited = core
        .source_service()
        .configure(
            person_id,
            &connection_id,
            source.revision(),
            ResourceMode::Selected,
            vec![
                ConnectionResource::new(ResourceHandle::try_new("home").unwrap(), "Home".into())
                    .unwrap(),
            ],
        )
        .await
        .unwrap();
    assert_eq!(edited.revision(), source.revision() + 1);
    assert_ne!(edited.source_authority(), source.source_authority());
    let snapshot = core
        .calendar_connector_snapshot(person_id, "local-test-device", now() + Duration::minutes(1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.views.len(), 1);
    core.import_calendar_sources(
        person_id,
        &connection_id,
        "local-test-device",
        Some(2),
        range(),
        vec![batch("home", "Home")],
        now() + Duration::minutes(2),
    )
    .await
    .unwrap();
    assert_eq!(
        core.source_service()
            .load(person_id, &connection_id)
            .await
            .unwrap(),
        Some(edited)
    );
    let day = core
        .day_snapshot(person_id, now().date_naive(), 0, now())
        .await
        .unwrap();
    assert_eq!(day.calendar_mirror_revision, Some(3));
    assert_eq!(day.items.len(), 1);
}

#[tokio::test]
async fn disconnect_and_reconnect_are_durable_and_never_restore_old_views() {
    let (directory, core, person_id, connection_id) = setup().await;
    let original = core
        .source_service()
        .load(person_id, &connection_id)
        .await
        .unwrap()
        .unwrap();
    core.source_service()
        .disconnect(person_id, &connection_id, original.revision())
        .await
        .unwrap();
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

    let replacement = establish(
        &core,
        person_id,
        "calendar.fixture",
        "calendar.fixture.reconnected",
        "local-test-device",
        &[("home", "Home")],
    )
    .await;
    let pending = core
        .calendar_connector_snapshot(person_id, "local-test-device", now())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.connection.state, ConnectionState::Pending);
    assert!(pending.views.is_empty());
    assert_eq!(replacement.revision(), 1);
    assert_ne!(replacement.connection_id(), &connection_id);
}

#[tokio::test]
async fn expired_calendar_projection_becomes_typed_unavailable_evidence() {
    let (_directory, core, person_id, _) = setup().await;
    let observed_at = now() + Duration::minutes(5);
    let snapshot = core
        .calendar_connector_snapshot(person_id, "local-test-device", observed_at)
        .await
        .unwrap()
        .unwrap();
    let situation = SituationDescriptor {
        schema_version: CONNECTED_CONTEXT_VERSION,
        id: "schedule.availability".into(),
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
