use floe_agent::{
    CapacityState, RecoveryState, WellbeingView, personal_context_evidence, validate_wellbeing_view,
};

#[test]
fn apple_fixture_crosses_the_derived_wellbeing_boundary() {
    let view: WellbeingView = serde_json::from_str(include_str!(
        "../../../apps/client/apple/FloeAppleHealth/Tests/FloeAppleHealthTests/Fixtures/wellbeing_view.json"
    ))
    .unwrap();

    validate_wellbeing_view(&view, view.observed_at_unix_ms).unwrap();
    assert_eq!(view.capacity, CapacityState::Strong);
    assert_eq!(view.recovery, RecoveryState::Recovered);
    assert_eq!(
        view.expires_at_unix_ms - view.observed_at_unix_ms,
        30 * 60 * 1_000
    );

    let evidence = personal_context_evidence(&view).unwrap();
    for forbidden in [
        "sleep_hours",
        "steps",
        "exercise_minutes",
        "raw_samples",
        "provider_id",
        "metadata",
    ] {
        assert!(!evidence.untrusted_text.contains(forbidden));
    }
}
