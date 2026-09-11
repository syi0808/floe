use floe_agent::{
    AGENT_VERSION, AgentFailure, AttentionState, AttentionView, CapacityState, FeasibilityView,
    RecoveryState, WellbeingView, validate_attention_view, validate_feasibility_view,
    validate_wellbeing_view,
};

const NOW: i64 = 1_789_000_000_000;

fn attention(lifetime_ms: i64) -> AttentionView {
    AttentionView {
        schema_version: AGENT_VERSION,
        view_id: "attention.coarse".into(),
        source_handle: "attention:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + lifetime_ms,
        state: AttentionState::Focused,
        confidence_millis: 800,
        evidence_handles: vec!["attention:aggregate".into()],
    }
}

fn feasibility(lifetime_ms: i64) -> FeasibilityView {
    FeasibilityView {
        schema_version: AGENT_VERSION,
        view_id: "schedule.feasibility".into(),
        source_handle: "feasibility:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + lifetime_ms,
        items: vec![],
    }
}

fn wellbeing(lifetime_ms: i64) -> WellbeingView {
    WellbeingView {
        schema_version: AGENT_VERSION,
        view_id: "wellbeing.derived".into(),
        source_handle: "wellbeing:fixture".into(),
        observed_at_unix_ms: NOW,
        expires_at_unix_ms: NOW + lifetime_ms,
        capacity: CapacityState::Typical,
        recovery: RecoveryState::Recovered,
        confidence_millis: 800,
        evidence_handles: vec!["health:derived-window".into()],
    }
}

#[test]
fn personal_views_enforce_their_purpose_specific_lifetimes() {
    validate_attention_view(&attention(120_000), NOW).unwrap();
    assert_eq!(
        validate_attention_view(&attention(120_001), NOW),
        Err(AgentFailure::InvalidInput)
    );

    validate_feasibility_view(&feasibility(300_000), NOW).unwrap();
    assert_eq!(
        validate_feasibility_view(&feasibility(300_001), NOW),
        Err(AgentFailure::InvalidInput)
    );

    validate_wellbeing_view(&wellbeing(1_800_000), NOW).unwrap();
    assert_eq!(
        validate_wellbeing_view(&wellbeing(1_800_001), NOW),
        Err(AgentFailure::InvalidInput)
    );
}
