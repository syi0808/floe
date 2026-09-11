use floe_agent::{FeasibilityView, validate_feasibility_view};

#[test]
fn swift_feasibility_fixture_matches_rust_contract() {
    let view: FeasibilityView = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/client/apple/FeasibilityProvider/Tests/Fixtures/schedule_feasibility.json"
    )))
    .expect("Swift fixture must deserialize as the Rust projection");

    validate_feasibility_view(&view, 2_000_000_000_001)
        .expect("Swift fixture must satisfy the Rust feasibility bounds");
}
