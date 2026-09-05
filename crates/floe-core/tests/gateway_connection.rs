use floe_core::{GatewayConnection, GatewayScheduleModel};

#[test]
fn explicit_gateway_addresses_remain_literal_loopback_only() {
    for address in [
        "https://example.com",
        "http://192.168.1.2:8431",
        "http://127.0.0.1:8431/other",
        "http://user@127.0.0.1:8431",
        "http://127.0.0.1:8431?secret=1",
        "http://127.0.0.1:8431#fragment",
        "http://localhost:8431",
        "http://127.0.0.1:0",
    ] {
        assert!(
            GatewayScheduleModel::new("high_effort".into(), false)
                .unwrap()
                .with_connection(GatewayConnection {
                    base_url: address.into(),
                    token: "a".repeat(52),
                })
                .is_err(),
            "{address}"
        );
    }
    assert!(
        GatewayScheduleModel::new("high_effort".into(), false)
            .unwrap()
            .with_connection(GatewayConnection {
                base_url: "http://127.0.0.1:9543".into(),
                token: "a".repeat(52),
            })
            .is_ok()
    );
}

#[test]
fn explicit_gateway_rejects_invalid_credentials() {
    for token in [
        "short".to_owned(),
        "a".repeat(257),
        format!("{}\n", "a".repeat(52)),
    ] {
        assert!(
            GatewayScheduleModel::new("high_effort".into(), false)
                .unwrap()
                .with_connection(GatewayConnection {
                    base_url: "http://127.0.0.1:8431".into(),
                    token,
                })
                .is_err()
        );
    }
}
