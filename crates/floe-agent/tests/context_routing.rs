use floe_agent::{
    CONNECTED_CONTEXT_VERSION, CapabilityAuthority, ConnectionState, ConnectorCapabilityDescriptor,
    ConnectorConnectionSnapshot, ConnectorDescriptor, ConnectorSnapshot, DataClass,
    ExecutionLocation, LogicalViewRoute, RetentionClass, SourceFailure, SourceFailureKind,
    ViewDescriptor, ViewSnapshot, route_logical_views,
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
    LogicalViewRoute {
        logical_source_id: logical.into(),
        connector_id: connector.into(),
        view_id: "mail.communication".into(),
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
