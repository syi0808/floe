use floe_agent::{
    AGENT_VERSION, AgentFailure, AttentionState, AttentionView, CapacityState, FeasibilityItem,
    FeasibilityView, PeopleIdentity, PeopleView, RecoveryState, WeatherImpact, WellbeingView,
    personal_context_evidence, validate_attention_view, validate_feasibility_view,
    validate_people_view, validate_wellbeing_view,
};

const NOW: i64 = 1_789_000_000_000;

fn people() -> PeopleView {
    PeopleView {
        schema_version: AGENT_VERSION,
        view_id: "people.identity".into(),
        source_handle: "people:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        coverage_complete: true,
        identities: vec![PeopleIdentity {
            identity_handle: "person:alex".into(),
            display_name: "Alex".into(),
            aliases: vec!["alex@example.com".into()],
            confidence_millis: 1000,
            evidence_handles: vec!["contact:alex".into()],
        }],
    }
}

#[test]
fn android_people_fixture_crosses_the_shared_identity_boundary() {
    let view: PeopleView = serde_json::from_str(include_str!(
        "../../../apps/client/android/fixtures/people_view.json"
    ))
    .unwrap();
    validate_people_view(&view, 1_789_128_000_000).unwrap();
    let evidence = personal_context_evidence(&view).unwrap();
    assert!(evidence.untrusted_text.contains("Alex"));
    assert!(!evidence.untrusted_text.contains("content://"));
}

#[test]
fn health_connect_fixture_crosses_the_derived_wellbeing_boundary() {
    let view: WellbeingView = serde_json::from_str(include_str!(
        "../../../apps/client/android/fixtures/wellbeing_view.json"
    ))
    .unwrap();
    validate_wellbeing_view(&view, 1_789_128_000_000).unwrap();
    let evidence = personal_context_evidence(&view).unwrap();
    assert!(!evidence.untrusted_text.contains("heart_rate"));
    assert!(!evidence.untrusted_text.contains("raw"));
}

#[test]
fn bounded_personal_views_expose_derived_context_without_raw_source_data() {
    let people = people();
    validate_people_view(&people, NOW).unwrap();
    let feasibility = FeasibilityView {
        schema_version: AGENT_VERSION,
        view_id: "schedule.feasibility".into(),
        source_handle: "feasibility:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        items: vec![FeasibilityItem {
            event_handle: "calendar:event".into(),
            evidence_handles: vec!["eta:route".into(), "weather:window".into()],
            travel_duration_seconds: 1800,
            leave_by_unix_ms: NOW + 900_000,
            weather_impact: WeatherImpact::Minor,
            confidence_millis: 900,
        }],
    };
    validate_feasibility_view(&feasibility, NOW).unwrap();
    let attention = AttentionView {
        schema_version: AGENT_VERSION,
        view_id: "attention.coarse".into(),
        source_handle: "attention:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        state: AttentionState::Focused,
        confidence_millis: 800,
        evidence_handles: vec!["attention:aggregate".into()],
    };
    validate_attention_view(&attention, NOW).unwrap();
    let wellbeing = WellbeingView {
        schema_version: AGENT_VERSION,
        view_id: "wellbeing.derived".into(),
        source_handle: "wellbeing:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        capacity: CapacityState::Reduced,
        recovery: RecoveryState::NeedsRecovery,
        confidence_millis: 750,
        evidence_handles: vec!["health:derived-window".into()],
    };
    validate_wellbeing_view(&wellbeing, NOW).unwrap();

    for evidence in [
        personal_context_evidence(&people).unwrap(),
        personal_context_evidence(&feasibility).unwrap(),
        personal_context_evidence(&attention).unwrap(),
        personal_context_evidence(&wellbeing).unwrap(),
    ] {
        assert!(!evidence.untrusted_text.contains("latitude"));
        assert!(!evidence.untrusted_text.contains("bundle_id"));
        assert!(!evidence.untrusted_text.contains("heart_rate"));
    }
}

#[test]
fn raw_or_unproven_personal_context_fails_closed() {
    let mut stale = people();
    stale.expires_at_unix_ms = NOW;
    assert_eq!(
        validate_people_view(&stale, NOW),
        Err(AgentFailure::InvalidInput)
    );

    let mut duplicate = people();
    duplicate.identities.push(duplicate.identities[0].clone());
    assert_eq!(
        validate_people_view(&duplicate, NOW),
        Err(AgentFailure::InvalidInput)
    );

    let unknown = AttentionView {
        schema_version: AGENT_VERSION,
        view_id: "attention.coarse".into(),
        source_handle: "attention:none".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + 300_000,
        state: AttentionState::Unknown,
        confidence_millis: 500,
        evidence_handles: vec![],
    };
    assert_eq!(
        validate_attention_view(&unknown, NOW),
        Err(AgentFailure::InvalidInput)
    );

    for field in ["latitude", "bundle_id", "raw_samples", "diagnosis"] {
        let mut value = serde_json::to_value(people()).unwrap();
        value[field] = serde_json::json!("private");
        assert!(serde_json::from_value::<PeopleView>(value).is_err());
    }
}
