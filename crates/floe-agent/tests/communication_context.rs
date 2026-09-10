use floe_agent::{
    AGENT_VERSION, AgentFailure, CommunicationView, DataClass, MAX_COMMUNICATION_BYTES,
    MAX_COMMUNICATION_ITEMS, communication_context_evidence, validate_communication_view,
};

const NOW: i64 = 1_789_000_000_000;

fn fixture() -> CommunicationView {
    serde_json::from_str(include_str!(
        "../../../server/internal/connectors/gmail/testdata/communication_view.json"
    ))
    .unwrap()
}

fn microsoft_fixture() -> CommunicationView {
    serde_json::from_str(include_str!(
        "../../../server/internal/connectors/microsoftmail/testdata/communication_view.json"
    ))
    .unwrap()
}

#[test]
fn server_communication_view_crosses_the_common_context_boundary() {
    let view = fixture();
    validate_communication_view(&view, NOW, MAX_COMMUNICATION_ITEMS, MAX_COMMUNICATION_BYTES)
        .unwrap();
    let evidence = communication_context_evidence(&view).unwrap();
    assert_eq!(evidence.source_handle, view.source_handle);
    assert_eq!(evidence.data_class, DataClass::Personal);
    assert!(evidence.untrusted_text.contains("Please confirm"));
    assert!(!evidence.untrusted_text.contains("access_token"));
}

#[test]
fn microsoft_communication_view_crosses_the_common_context_boundary() {
    let view = microsoft_fixture();
    validate_communication_view(
        &view,
        1_789_128_000_000,
        MAX_COMMUNICATION_ITEMS,
        MAX_COMMUNICATION_BYTES,
    )
    .unwrap();
    let evidence = communication_context_evidence(&view).unwrap();
    assert_eq!(evidence.source_handle, view.source_handle);
    assert_eq!(evidence.data_class, DataClass::Personal);
    assert!(evidence.untrusted_text.contains("Please confirm"));
    assert!(!evidence.untrusted_text.contains("message_123"));
}

#[test]
fn communication_view_rejects_stale_oversized_and_escalated_shapes() {
    let mut stale = fixture();
    stale.expires_at_unix_ms = NOW;
    assert_eq!(
        validate_communication_view(
            &stale,
            NOW,
            MAX_COMMUNICATION_ITEMS,
            MAX_COMMUNICATION_BYTES
        ),
        Err(AgentFailure::InvalidInput)
    );

    let mut duplicate = fixture();
    duplicate.items.push(duplicate.items[0].clone());
    assert_eq!(
        validate_communication_view(
            &duplicate,
            NOW,
            MAX_COMMUNICATION_ITEMS,
            MAX_COMMUNICATION_BYTES
        ),
        Err(AgentFailure::InvalidInput)
    );

    let mut bounded = fixture();
    bounded.items[0].snippet = "x".repeat(1025);
    assert_eq!(
        validate_communication_view(
            &bounded,
            NOW,
            MAX_COMMUNICATION_ITEMS,
            MAX_COMMUNICATION_BYTES
        ),
        Err(AgentFailure::InvalidInput)
    );

    let mut json = serde_json::to_value(fixture()).unwrap();
    json["authority"] = serde_json::json!("send");
    assert!(serde_json::from_value::<CommunicationView>(json).is_err());

    let mut unsupported = fixture();
    unsupported.schema_version = AGENT_VERSION + 1;
    assert_eq!(
        validate_communication_view(
            &unsupported,
            NOW,
            MAX_COMMUNICATION_ITEMS,
            MAX_COMMUNICATION_BYTES
        ),
        Err(AgentFailure::UnsupportedVersion)
    );
}
