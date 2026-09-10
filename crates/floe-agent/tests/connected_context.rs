use floe_agent::{
    CONNECTED_CONTEXT_VERSION, CapabilityAuthority, ConformanceCode, ConnectionState,
    ConnectorCapabilityDescriptor, ConnectorConnectionSnapshot, ConnectorDescriptor,
    ConnectorSnapshot, DataClass, ExecutionLocation, RetentionClass, SituationDescriptor,
    SituationTrigger, SourceFailure, SourceFailureKind, ViewDescriptor, ViewSnapshot,
    evaluate_situation, validate_connector_snapshot,
};

const NOW: u64 = 2_000_000;

fn fixture(connector_id: &str, view_id: &str, source_handle: &str) -> ConnectorSnapshot {
    ConnectorSnapshot {
        descriptor: ConnectorDescriptor {
            schema_version: CONNECTED_CONTEXT_VERSION,
            id: connector_id.into(),
            version: "1.0.0".into(),
            provider: "fixture".into(),
            execution: ExecutionLocation::Device {
                device_id: "test-device".into(),
            },
            capabilities: vec![ConnectorCapabilityDescriptor {
                schema_version: CONNECTED_CONTEXT_VERSION,
                id: format!("{view_id}.read"),
                version: "1.0.0".into(),
                authority: CapabilityAuthority::Observe,
                required_scopes: vec![format!("{view_id}.read")],
                output_view_id: Some(view_id.into()),
            }],
            views: vec![ViewDescriptor {
                schema_version: CONNECTED_CONTEXT_VERSION,
                id: view_id.into(),
                version: "1.0.0".into(),
                data_class: DataClass::Personal,
                retention: RetentionClass::Mirror,
                freshness_ttl_ms: 60_000,
                max_items: 100,
                max_bytes: 16_384,
                provenance_required: true,
            }],
        },
        connection: ConnectorConnectionSnapshot {
            schema_version: CONNECTED_CONTEXT_VERSION,
            connector_id: connector_id.into(),
            state: ConnectionState::Ready,
            granted_scopes: vec![format!("{view_id}.read")],
            observed_at_unix_ms: NOW,
            last_success_at_unix_ms: Some(NOW),
            last_failure: None,
        },
        views: vec![ViewSnapshot {
            schema_version: CONNECTED_CONTEXT_VERSION,
            view_id: view_id.into(),
            source_handle: source_handle.into(),
            observed_at_unix_ms: NOW - 1_000,
            expires_at_unix_ms: NOW + 30_000,
            item_count: 2,
            byte_count: 512,
            provenance_count: 2,
        }],
    }
}

fn briefing() -> SituationDescriptor {
    SituationDescriptor {
        schema_version: CONNECTED_CONTEXT_VERSION,
        id: "briefing.today".into(),
        version: "1.0.0".into(),
        trigger: SituationTrigger::ExplicitForegroundRequest,
        required_view_ids: vec!["view.timeline".into()],
        optional_view_ids: vec!["view.weather".into(), "view.communication".into()],
    }
}

#[test]
fn conforming_connector_keeps_observe_separate_from_act() {
    let mut fixture = fixture("calendar.fixture", "view.timeline", "calendar:home");
    fixture
        .descriptor
        .capabilities
        .push(ConnectorCapabilityDescriptor {
            schema_version: CONNECTED_CONTEXT_VERSION,
            id: "calendar.event.create".into(),
            version: "1.0.0".into(),
            authority: CapabilityAuthority::Act,
            required_scopes: vec!["calendar.events.write".into()],
            output_view_id: None,
        });

    assert!(validate_connector_snapshot(&fixture, NOW).is_empty());

    fixture.descriptor.capabilities[1].output_view_id = Some("view.timeline".into());
    assert!(
        validate_connector_snapshot(&fixture, NOW)
            .iter()
            .any(|violation| {
                violation.code == ConformanceCode::InvalidAuthority
                    && violation.subject == "calendar.event.create"
            })
    );
}

#[test]
fn missing_scope_and_provenance_fail_conformance() {
    let mut fixture = fixture("mail.fixture", "view.communication", "mail:inbox");
    fixture.connection.granted_scopes.clear();
    fixture.views[0].provenance_count = 1;

    let violations = validate_connector_snapshot(&fixture, NOW);
    assert!(violations.iter().any(|violation| {
        violation.code == ConformanceCode::MissingScope
            && violation.subject == "view.communication.read"
    }));
    assert!(violations.iter().any(|violation| {
        violation.code == ConformanceCode::MissingProvenance
            && violation.subject == "view.communication"
    }));
}

#[test]
fn degraded_optional_source_does_not_block_required_view() {
    let calendar = fixture("calendar.fixture", "view.timeline", "calendar:home");
    let mut mail = fixture("mail.fixture", "view.communication", "mail:inbox");
    mail.connection.state = ConnectionState::Degraded;
    mail.connection.last_failure = Some(SourceFailure {
        kind: SourceFailureKind::PartialFetch,
        observed_at_unix_ms: NOW,
    });
    mail.views.clear();

    let report = evaluate_situation(&briefing(), &[calendar, mail], NOW);

    assert!(report.can_run());
    assert_eq!(report.available_views["view.timeline"], ["calendar:home"]);
    assert_eq!(
        report.missing_optional_views,
        ["view.weather", "view.communication"]
    );
    assert_eq!(report.source_issues.len(), 1);
    assert_eq!(
        report.source_issues[0].failure,
        Some(SourceFailureKind::PartialFetch)
    );
}

#[test]
fn stale_or_nonconforming_views_are_not_reported_available() {
    let mut weather = fixture("weather.fixture", "view.weather", "weather:seoul");
    weather.views[0].expires_at_unix_ms = NOW;
    let mut calendar = fixture("calendar.fixture", "view.timeline", "calendar:home");
    calendar.views[0].byte_count = 20_000;

    let report = evaluate_situation(&briefing(), &[weather, calendar], NOW);

    assert!(!report.can_run());
    assert_eq!(report.missing_required_views, ["view.timeline"]);
    assert_eq!(
        report.missing_optional_views,
        ["view.weather", "view.communication"]
    );
    assert!(report.violations.iter().any(|violation| {
        violation.code == ConformanceCode::ViewLimitExceeded && violation.subject == "view.timeline"
    }));
}

#[test]
fn strict_wire_contract_rejects_unknown_fields() {
    let value = serde_json::json!({
        "schema_version": CONNECTED_CONTEXT_VERSION,
        "id": "briefing.today",
        "version": "1.0.0",
        "trigger": "explicit_foreground_request",
        "required_view_ids": ["view.timeline"],
        "optional_view_ids": [],
        "background_delivery": true
    });

    assert!(serde_json::from_value::<SituationDescriptor>(value).is_err());
}

#[test]
fn go_gmail_descriptor_conforms_to_the_shared_rust_contract() {
    let snapshot: ConnectorSnapshot = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../server/internal/connectors/gmail/testdata/ready_snapshot.json"
    )))
    .unwrap();
    assert!(validate_connector_snapshot(&snapshot, 1_789_000_000_000).is_empty());
    assert_eq!(snapshot.descriptor.id, "gmail");
    assert!(
        snapshot
            .descriptor
            .capabilities
            .iter()
            .all(|capability| capability.authority == CapabilityAuthority::Observe)
    );
}

#[test]
fn go_github_descriptor_conforms_to_the_shared_rust_contract() {
    let snapshot: ConnectorSnapshot = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../server/internal/connectors/github/testdata/ready_snapshot.json"
    )))
    .unwrap();
    assert!(validate_connector_snapshot(&snapshot, 1_789_012_800_000).is_empty());
    assert_eq!(snapshot.descriptor.provider, "github");
    assert_eq!(snapshot.views[0].view_id, "work.context");
}
