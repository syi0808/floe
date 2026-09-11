use floe_agent::{
    CONNECTED_CONTEXT_VERSION, CapabilityAuthority, ConnectionState, ConnectorCapabilityDescriptor,
    ConnectorConnectionSnapshot, ConnectorDescriptor, ConnectorSnapshot, ContextRoutingRuntime,
    ContextTransferClass, DataClass, DeviceClass, DevicePresence, DeviceScope, ExecutionLocation,
    LogicalViewRoute, LogicalViewRoutingPolicy, RetentionClass, RoutedAvailability,
    RuntimeDeviceState, RuntimeSourcePolicy, SourceFailure, SourceFailureKind, ViewDescriptor,
    ViewSnapshot, route_logical_views, route_logical_views_with_runtime,
};

const NOW: u64 = 2_000_000;

fn snapshot(
    connector: &str,
    provider: &str,
    state: ConnectionState,
    observed: u64,
    source: &str,
) -> ConnectorSnapshot {
    ConnectorSnapshot {
        descriptor: ConnectorDescriptor {
            schema_version: CONNECTED_CONTEXT_VERSION,
            id: connector.into(),
            version: "1.0.0".into(),
            provider: provider.into(),
            execution: ExecutionLocation::Server,
            capabilities: vec![ConnectorCapabilityDescriptor {
                schema_version: CONNECTED_CONTEXT_VERSION,
                id: "mail.search".into(),
                version: "1.0.0".into(),
                authority: CapabilityAuthority::Observe,
                required_scopes: vec!["mail.read".into()],
                output_view_id: Some("mail.communication".into()),
            }],
            views: vec![ViewDescriptor {
                schema_version: CONNECTED_CONTEXT_VERSION,
                id: "mail.communication".into(),
                version: "1.0.0".into(),
                data_class: DataClass::Personal,
                retention: RetentionClass::IndexOnDemand,
                freshness_ttl_ms: 300_000,
                max_items: 100,
                max_bytes: 65_536,
                provenance_required: true,
            }],
        },
        connection: ConnectorConnectionSnapshot {
            schema_version: CONNECTED_CONTEXT_VERSION,
            connector_id: connector.into(),
            state,
            granted_scopes: vec!["mail.read".into()],
            observed_at_unix_ms: NOW,
            last_success_at_unix_ms: Some(observed),
            last_failure: (state == ConnectionState::Degraded).then_some(SourceFailure {
                kind: SourceFailureKind::PartialFetch,
                observed_at_unix_ms: NOW,
            }),
        },
        views: vec![ViewSnapshot {
            schema_version: CONNECTED_CONTEXT_VERSION,
            view_id: "mail.communication".into(),
            source_handle: source.into(),
            observed_at_unix_ms: observed,
            expires_at_unix_ms: observed + 300_000,
            item_count: 1,
            byte_count: 512,
            provenance_count: 1,
        }],
    }
}

fn route(logical: &str, connector: &str) -> LogicalViewRoute {
    route_view(logical, connector, "mail.communication")
}

fn route_view(logical: &str, connector: &str, view_id: &str) -> LogicalViewRoute {
    LogicalViewRoute {
        logical_source_id: logical.into(),
        connector_id: connector.into(),
        view_id: view_id.into(),
    }
}

fn device_snapshot(
    connector: &str,
    device_id: &str,
    logical_view: &str,
    observed: u64,
    source: &str,
) -> ConnectorSnapshot {
    let mut snapshot = snapshot(
        connector,
        "native",
        ConnectionState::Ready,
        observed,
        source,
    );
    snapshot.descriptor.execution = ExecutionLocation::Device {
        device_id: device_id.into(),
    };
    snapshot.descriptor.capabilities[0].output_view_id = Some(logical_view.into());
    snapshot.descriptor.views[0].id = logical_view.into();
    snapshot.views[0].view_id = logical_view.into();
    snapshot
}

fn runtime(
    devices: Vec<RuntimeDeviceState>,
    sources: &[(&str, ContextTransferClass)],
    policies: Vec<LogicalViewRoutingPolicy>,
) -> ContextRoutingRuntime {
    ContextRoutingRuntime {
        devices,
        sources: sources
            .iter()
            .map(|(connector_id, transfer)| RuntimeSourcePolicy {
                connector_id: (*connector_id).into(),
                transfer: *transfer,
            })
            .collect(),
        logical_views: policies,
    }
}

fn device(device_id: &str, class: DeviceClass, online: bool, present: bool) -> RuntimeDeviceState {
    RuntimeDeviceState {
        device_id: device_id.into(),
        class,
        online,
        presence: if present {
            DevicePresence::Present
        } else {
            DevicePresence::Absent
        },
        presence_expires_at_unix_ms: NOW + 60_000,
    }
}

#[test]
fn route_arbitration_prefers_health_then_freshness_then_provider_without_prompting() {
    let google = snapshot(
        "gmail.account",
        "google",
        ConnectionState::Degraded,
        NOW - 100,
        "mail:google",
    );
    let microsoft = snapshot(
        "outlook.account",
        "microsoft",
        ConnectionState::Ready,
        NOW - 1_000,
        "mail:microsoft",
    );
    let routes = [
        route("work-mail", "gmail.account"),
        route("work-mail", "outlook.account"),
    ];
    let result = route_logical_views(
        &routes,
        &[google.clone(), microsoft.clone()],
        &["google".into(), "microsoft".into()],
        NOW,
    );
    assert_eq!(result.selected[0].connector_id, "outlook.account");
    assert_eq!(result.deduplicated_candidates, 1);

    let mut microsoft_degraded = microsoft;
    microsoft_degraded.connection.state = ConnectionState::Degraded;
    microsoft_degraded.connection.last_failure = Some(SourceFailure {
        kind: SourceFailureKind::PartialFetch,
        observed_at_unix_ms: NOW,
    });
    let result = route_logical_views(
        &routes,
        &[google, microsoft_degraded],
        &["google".into(), "microsoft".into()],
        NOW,
    );
    assert_eq!(result.selected[0].connector_id, "gmail.account");
}

#[test]
fn nonconforming_and_duplicate_physical_views_are_not_promoted() {
    let android = snapshot(
        "android.mail",
        "android",
        ConnectionState::Ready,
        NOW - 1_000,
        "mail:shared",
    );
    let google = snapshot(
        "gmail.account",
        "google",
        ConnectionState::Ready,
        NOW - 500,
        "mail:shared",
    );
    let mut health_connect = snapshot(
        "health-connect",
        "health_connect",
        ConnectionState::Ready,
        NOW - 100,
        "health:derived",
    );
    health_connect.views[0].byte_count = 100_000;
    let routes = [
        route("a-personal-mail", "android.mail"),
        route("b-duplicate-mail", "gmail.account"),
        route("health", "health-connect"),
    ];
    let result = route_logical_views(&routes, &[android, google, health_connect], &[], NOW);
    assert_eq!(result.selected.len(), 1);
    assert_eq!(result.selected[0].logical_source_id, "a-personal-mail");
    assert_eq!(
        result.unavailable_logical_sources,
        ["b-duplicate-mail", "health"]
    );
}

#[test]
fn calendar_parity_routes_share_one_logical_source_without_prompt_selection() {
    let mut google = snapshot(
        "calendar.google",
        "google_calendar",
        ConnectionState::Ready,
        NOW - 500,
        "calendar.timeline:google",
    );
    let mut microsoft = snapshot(
        "calendar.microsoft",
        "microsoft_calendar",
        ConnectionState::Ready,
        NOW - 500,
        "calendar.timeline:microsoft",
    );
    for candidate in [&mut google, &mut microsoft] {
        candidate.descriptor.capabilities[0].id = "calendar.events.read".into();
        candidate.descriptor.capabilities[0].required_scopes = vec!["calendar.events.read".into()];
        candidate.descriptor.capabilities[0].output_view_id = Some("calendar.timeline".into());
        candidate.descriptor.views[0].id = "calendar.timeline".into();
        candidate.connection.granted_scopes = vec!["calendar.events.read".into()];
        candidate.views[0].view_id = "calendar.timeline".into();
    }
    let routes = [
        LogicalViewRoute {
            logical_source_id: "personal-calendar".into(),
            connector_id: "calendar.google".into(),
            view_id: "calendar.timeline".into(),
        },
        LogicalViewRoute {
            logical_source_id: "personal-calendar".into(),
            connector_id: "calendar.microsoft".into(),
            view_id: "calendar.timeline".into(),
        },
    ];

    let result = route_logical_views(
        &routes,
        &[microsoft, google],
        &["google_calendar".into(), "microsoft_calendar".into()],
        NOW,
    );

    assert_eq!(result.selected.len(), 1);
    assert_eq!(result.selected[0].connector_id, "calendar.google");
    assert_eq!(result.deduplicated_candidates, 1);
}

#[test]
fn location_uses_the_present_invoking_mobile_instead_of_the_newest_device() {
    let phone = device_snapshot(
        "location.phone",
        "phone",
        "location.current",
        NOW - 5_000,
        "location:phone",
    );
    let tablet = device_snapshot(
        "location.tablet",
        "tablet",
        "location.current",
        NOW - 100,
        "location:tablet",
    );
    let desktop = device_snapshot(
        "location.desktop",
        "desktop",
        "location.current",
        NOW - 10,
        "location:desktop",
    );
    let routes = [
        route_view("current-location", "location.phone", "location.current"),
        route_view("current-location", "location.tablet", "location.current"),
        route_view("current-location", "location.desktop", "location.current"),
    ];
    let runtime = runtime(
        vec![
            device("phone", DeviceClass::Mobile, true, true),
            device("tablet", DeviceClass::Mobile, true, false),
            device("desktop", DeviceClass::Desktop, true, true),
        ],
        &[],
        vec![LogicalViewRoutingPolicy {
            logical_source_id: "current-location".into(),
            device_scope: DeviceScope::Location {
                invoking_mobile_device_id: Some("phone".into()),
                designated_carried_device_id: Some("tablet".into()),
            },
            max_age_ms: Some(120_000),
            allowed_transfers: vec![],
        }],
    );

    let result =
        route_logical_views_with_runtime(&routes, &[tablet, desktop, phone], &[], NOW, &runtime);

    assert_eq!(
        result.selected[0].producer_device_id.as_deref(),
        Some("phone")
    );
}

#[test]
fn attention_stays_on_the_interaction_device_instead_of_becoming_person_global() {
    let phone = device_snapshot(
        "attention.phone",
        "phone",
        "attention.coarse",
        NOW - 10,
        "attention:phone",
    );
    let desktop = device_snapshot(
        "attention.desktop",
        "desktop",
        "attention.coarse",
        NOW - 1_000,
        "attention:desktop",
    );
    let routes = [
        route_view("current-attention", "attention.phone", "attention.coarse"),
        route_view("current-attention", "attention.desktop", "attention.coarse"),
    ];
    let runtime = runtime(
        vec![
            device("phone", DeviceClass::Mobile, true, true),
            device("desktop", DeviceClass::Desktop, true, true),
        ],
        &[],
        vec![LogicalViewRoutingPolicy {
            logical_source_id: "current-attention".into(),
            device_scope: DeviceScope::Attention {
                interaction_device_id: "desktop".into(),
                active_desktop_device_id: Some("desktop".into()),
            },
            max_age_ms: Some(120_000),
            allowed_transfers: vec![],
        }],
    );

    let result = route_logical_views_with_runtime(&routes, &[phone, desktop], &[], NOW, &runtime);

    assert_eq!(result.selected.len(), 1);
    assert_eq!(
        result.selected[0].producer_device_id.as_deref(),
        Some("desktop")
    );
}

#[test]
fn stale_and_device_only_offline_context_fail_closed_but_relay_cache_is_degraded() {
    let stale = device_snapshot(
        "attention.stale",
        "tablet",
        "attention.coarse",
        NOW - 121_000,
        "attention:stale",
    );
    let offline = device_snapshot(
        "attention.offline",
        "tablet",
        "attention.coarse",
        NOW - 1_000,
        "attention:offline",
    );
    let policy = LogicalViewRoutingPolicy {
        logical_source_id: "current-attention".into(),
        device_scope: DeviceScope::Attention {
            interaction_device_id: "tablet".into(),
            active_desktop_device_id: None,
        },
        max_age_ms: Some(120_000),
        allowed_transfers: vec![],
    };
    let routes = [
        route_view("current-attention", "attention.stale", "attention.coarse"),
        route_view("current-attention", "attention.offline", "attention.coarse"),
    ];
    let device_only = runtime(
        vec![device("tablet", DeviceClass::Mobile, false, true)],
        &[
            ("attention.stale", ContextTransferClass::DeviceOnly),
            ("attention.offline", ContextTransferClass::DeviceOnly),
        ],
        vec![policy.clone()],
    );
    let result = route_logical_views_with_runtime(
        &routes,
        &[stale.clone(), offline.clone()],
        &[],
        NOW,
        &device_only,
    );
    assert!(result.selected.is_empty());
    assert_eq!(result.unavailable_logical_sources, ["current-attention"]);

    let relayed = runtime(
        vec![device("tablet", DeviceClass::Mobile, false, true)],
        &[
            ("attention.stale", ContextTransferClass::OpaqueRelay),
            ("attention.offline", ContextTransferClass::OpaqueRelay),
        ],
        vec![policy],
    );
    let result = route_logical_views_with_runtime(&routes, &[stale, offline], &[], NOW, &relayed);
    assert_eq!(result.selected.len(), 1);
    assert_eq!(
        result.selected[0].availability,
        RoutedAvailability::DegradedOfflineCache
    );
    assert_eq!(result.selected[0].view.source_handle, "attention:offline");
}

#[test]
fn explicit_source_disagreement_keeps_all_distinct_evidence() {
    let primary = snapshot(
        "health.primary",
        "healthkit",
        ConnectionState::Ready,
        NOW - 1_000,
        "wellbeing:healthkit",
    );
    let mut secondary = snapshot(
        "health.secondary",
        "health_connect",
        ConnectionState::Degraded,
        NOW - 500,
        "wellbeing:health-connect",
    );
    secondary.connection.last_failure = Some(SourceFailure {
        kind: SourceFailureKind::SourceDisagreement,
        observed_at_unix_ms: NOW,
    });
    let routes = [
        route("derived-wellbeing", "health.primary"),
        route("derived-wellbeing", "health.secondary"),
    ];

    let result = route_logical_views(&routes, &[secondary, primary], &[], NOW);

    assert_eq!(result.selected[0].connector_id, "health.primary");
    assert_eq!(result.disagreements.len(), 1);
    assert_eq!(result.disagreements[0].candidates.len(), 2);
    assert_eq!(
        result.disagreements[0]
            .candidates
            .iter()
            .map(|candidate| candidate.view.source_handle.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        ["wellbeing:health-connect", "wellbeing:healthkit"]
            .into_iter()
            .collect()
    );
}
