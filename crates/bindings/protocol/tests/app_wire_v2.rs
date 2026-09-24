use floe_protocol::{
    APP_WIRE_VERSION, AppCommandRequestDto, AppCommandResultDto, AppEventsResultDto,
    AppQueryRequestDto, AppQueryResultDto, AppResponseDto, PROTOCOL_VERSION,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn fixture(name: &str) -> Value {
    let source = match name {
        "start_turn" => include_str!("fixtures/app_wire_v2/start_turn.json"),
        "cancel_run" => include_str!("fixtures/app_wire_v2/cancel_run.json"),
        "cancel_run_receipt" => include_str!("fixtures/app_wire_v2/cancel_run_receipt.json"),
        "command_receipt" => include_str!("fixtures/app_wire_v2/command_receipt.json"),
        "get_run" => include_str!("fixtures/app_wire_v2/get_run.json"),
        "get_message" => include_str!("fixtures/app_wire_v2/get_message.json"),
        "message" => include_str!("fixtures/app_wire_v2/message.json"),
        "resync_required" => include_str!("fixtures/app_wire_v2/resync_required.json"),
        "events" => include_str!("fixtures/app_wire_v2/events.json"),
        "interaction_resolve" => include_str!("fixtures/app_wire_v2/interaction_resolve.json"),
        "interaction_refresh" => include_str!("fixtures/app_wire_v2/interaction_refresh.json"),
        "interaction_resume" => include_str!("fixtures/app_wire_v2/interaction_resume.json"),
        "interaction_get" => include_str!("fixtures/app_wire_v2/interaction_get.json"),
        "interaction_list" => include_str!("fixtures/app_wire_v2/interaction_list.json"),
        "interaction_snapshot" => include_str!("fixtures/app_wire_v2/interaction_snapshot.json"),
        "interaction_resolved" => include_str!("fixtures/app_wire_v2/interaction_resolved.json"),
        _ => panic!("unknown fixture"),
    };
    serde_json::from_str(source).unwrap()
}

fn assert_round_trip<T>(name: &str)
where
    T: DeserializeOwned + Serialize,
{
    let expected = fixture(name);
    let decoded: T = serde_json::from_value(expected.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
}

#[test]
fn app_wire_v2_golden_fixtures_are_stable() {
    assert_round_trip::<AppCommandRequestDto>("start_turn");
    assert_round_trip::<AppCommandRequestDto>("cancel_run");
    assert_round_trip::<AppResponseDto<AppCommandResultDto>>("command_receipt");
    assert_round_trip::<AppResponseDto<AppCommandResultDto>>("cancel_run_receipt");
    assert_round_trip::<AppQueryRequestDto>("get_run");
    assert_round_trip::<AppQueryRequestDto>("get_message");
    assert_round_trip::<AppResponseDto<AppQueryResultDto>>("message");
    assert_round_trip::<AppResponseDto<AppEventsResultDto>>("resync_required");
    assert_round_trip::<AppResponseDto<AppEventsResultDto>>("events");
}

#[test]
fn app_wire_version_is_distinct_from_legacy_protocol() {
    assert_eq!(PROTOCOL_VERSION, 1);
    assert_eq!(APP_WIRE_VERSION, 2);

    let request: AppCommandRequestDto = serde_json::from_value(fixture("start_turn")).unwrap();
    assert_eq!(request.validate(), Ok(()));

    let mut legacy = fixture("start_turn");
    legacy["schema_version"] = json!(PROTOCOL_VERSION);
    let request: AppCommandRequestDto = serde_json::from_value(legacy).unwrap();
    assert_eq!(request.validate(), Err("schema_version"));
}

#[test]
fn app_wire_rejects_invalid_identity_and_unknown_fields() {
    let mut invalid_id = fixture("start_turn");
    invalid_id["command_id"] = json!("not-a-uuid");
    assert!(serde_json::from_value::<AppCommandRequestDto>(invalid_id).is_err());

    let mut nil_id = fixture("start_turn");
    nil_id["request_id"] = json!("00000000-0000-0000-0000-000000000000");
    let request: AppCommandRequestDto = serde_json::from_value(nil_id).unwrap();
    assert_eq!(request.validate(), Err("request_id"));

    let mut unknown = fixture("start_turn");
    unknown["command"]["bearer_token"] = json!("must-not-cross-app-wire");
    assert!(serde_json::from_value::<AppCommandRequestDto>(unknown).is_err());
}

#[test]
fn app_wire_text_validation_uses_normalized_text_and_keeps_payload_defense() {
    let mut padded = fixture("start_turn");
    padded["command"]["text"] = json!("\u{0085}\thello\t\u{0085}");
    let request: AppCommandRequestDto = serde_json::from_value(padded).unwrap();
    assert_eq!(request.validate(), Ok(()));

    for text in ["hello\tworld", "hello\rworld", "hello\u{000B}world"] {
        let mut invalid = fixture("start_turn");
        invalid["command"]["text"] = json!(text);
        let request: AppCommandRequestDto = serde_json::from_value(invalid).unwrap();
        assert_eq!(request.validate(), Err("command.text"));
    }

    let mut exact = fixture("start_turn");
    exact["command"]["text"] = json!("한".repeat(2_730) + "ab");
    let request: AppCommandRequestDto = serde_json::from_value(exact).unwrap();
    assert_eq!(request.validate(), Ok(()));

    let mut over = fixture("start_turn");
    over["command"]["text"] = json!("한".repeat(2_730) + "abc");
    let request: AppCommandRequestDto = serde_json::from_value(over).unwrap();
    assert_eq!(request.validate(), Err("command.text"));

    let mut oversized_payload = fixture("start_turn");
    oversized_payload["command"]["text"] = json!(format!("a{}", " ".repeat(64 * 1024)));
    let request: AppCommandRequestDto = serde_json::from_value(oversized_payload).unwrap();
    assert_eq!(request.validate(), Err("command.text"));
}

#[test]
fn app_wire_validates_continuation_and_event_bounds() {
    let mut continuation = fixture("start_turn");
    continuation["command"]["mode"] = json!({
        "kind": "continue",
        "continuation_ref": {
            "run_id": "00000000-0000-0000-0000-000000000004",
            "executor_generation": 0,
            "level": 1
        }
    });
    let request: AppCommandRequestDto = serde_json::from_value(continuation).unwrap();
    assert_eq!(request.validate(), Err("command.mode.continuation_ref"));

    let mut retry_continuation = fixture("start_turn");
    retry_continuation["command"]["retry_of"] = json!("00000000-0000-4000-8000-000000000005");
    retry_continuation["command"]["mode"] = json!({
        "kind": "continue",
        "continuation_ref": {
            "run_id": "00000000-0000-4000-8000-000000000004",
            "executor_generation": 1,
            "level": 1
        }
    });
    let request: AppCommandRequestDto = serde_json::from_value(retry_continuation).unwrap();
    assert_eq!(request.validate(), Err("command.retry_of"));

    let request: floe_protocol::AppEventsRequestDto = serde_json::from_value(json!({
        "schema_version": 2,
        "request_id": "00000000-0000-0000-0000-000000000006",
        "limit": 257
    }))
    .unwrap();
    assert_eq!(request.validate(), Err("limit"));

    let request: floe_protocol::AppEventsRequestDto = serde_json::from_value(json!({
        "schema_version": 2,
        "request_id": "00000000-0000-0000-0000-000000000006",
        "runtime_epoch": 7,
        "limit": 16
    }))
    .unwrap();
    assert_eq!(request.validate(), Err("cursor"));
}

#[test]
fn app_wire_interaction_fixtures_are_stable() {
    assert_round_trip::<AppCommandRequestDto>("interaction_resolve");
    assert_round_trip::<AppCommandRequestDto>("interaction_refresh");
    assert_round_trip::<AppCommandRequestDto>("interaction_resume");
    assert_round_trip::<AppQueryRequestDto>("interaction_get");
    assert_round_trip::<AppQueryRequestDto>("interaction_list");
    assert_round_trip::<AppResponseDto<AppQueryResultDto>>("interaction_snapshot");
    assert_round_trip::<AppResponseDto<AppCommandResultDto>>("interaction_resolved");

    let request: AppCommandRequestDto =
        serde_json::from_value(fixture("interaction_resolve")).unwrap();
    assert_eq!(request.validate(), Ok(()));
    let request: AppCommandRequestDto =
        serde_json::from_value(fixture("interaction_refresh")).unwrap();
    assert_eq!(request.validate(), Ok(()));
    let request: AppCommandRequestDto =
        serde_json::from_value(fixture("interaction_resume")).unwrap();
    assert_eq!(request.validate(), Ok(()));
    let request: AppQueryRequestDto = serde_json::from_value(fixture("interaction_get")).unwrap();
    assert_eq!(request.validate(), Ok(()));
    let request: AppQueryRequestDto = serde_json::from_value(fixture("interaction_list")).unwrap();
    assert_eq!(request.validate(), Ok(()));
}

#[test]
fn app_wire_interaction_rejects_forged_and_authority_fields() {
    // Unknown decision variants never parse.
    let mut unknown_decision = fixture("interaction_resolve");
    unknown_decision["command"]["decision"] = json!("auto_approve");
    assert!(serde_json::from_value::<AppCommandRequestDto>(unknown_decision).is_err());

    // Unknown snapshot states never parse.
    let mut unknown_state = fixture("interaction_snapshot");
    unknown_state["result"]["state"] = json!("waiting_on_user");
    assert!(serde_json::from_value::<AppResponseDto<AppQueryResultDto>>(unknown_state).is_err());

    // Authority smuggling is rejected: no recipient, grant, resource,
    // consumer, purpose, profile, text or parent override on the command.
    for field in [
        "recipient",
        "grant_scope",
        "resources",
        "consumer",
        "purpose",
        "profile_id",
        "original_text",
        "parent_run_id",
    ] {
        let mut smuggled = fixture("interaction_resolve");
        smuggled["command"][field] = json!("model.evil");
        assert!(
            serde_json::from_value::<AppCommandRequestDto>(smuggled).is_err(),
            "{field} must not cross the wire"
        );
    }

    // Nil ids and zero revisions fail validation, not parsing.
    let mut nil_interaction = fixture("interaction_resolve");
    nil_interaction["command"]["interaction_id"] = json!("00000000-0000-0000-0000-000000000000");
    let request: AppCommandRequestDto = serde_json::from_value(nil_interaction).unwrap();
    assert_eq!(request.validate(), Err("command.interaction_id"));

    let mut zero_revision = fixture("interaction_refresh");
    zero_revision["command"]["expected_revision"] = json!(0);
    let request: AppCommandRequestDto = serde_json::from_value(zero_revision).unwrap();
    assert_eq!(request.validate(), Err("command.expected_revision"));

    let mut zero_digest = fixture("interaction_resolve");
    zero_digest["command"]["target_digest"] = serde_json::to_value([0u8; 32]).unwrap();
    let request: AppCommandRequestDto = serde_json::from_value(zero_digest).unwrap();
    assert_eq!(request.validate(), Err("command.target_digest"));

    let mut short_digest = fixture("interaction_resolve");
    short_digest["command"]["target_digest"] = json!([1, 2, 3]);
    assert!(serde_json::from_value::<AppCommandRequestDto>(short_digest).is_err());

    let mut nil_origin = fixture("interaction_resume");
    nil_origin["command"]["origin_run_id"] = json!("00000000-0000-0000-0000-000000000000");
    let request: AppCommandRequestDto = serde_json::from_value(nil_origin).unwrap();
    assert_eq!(request.validate(), Err("command.origin_run_id"));
}

#[test]
fn app_wire_preserves_explicit_profile_selection() {
    let mut request = fixture("start_turn");
    request["command"]["profile"] = json!({
        "kind": "explicit",
        "profile_id": "local-fast",
    });
    let decoded: AppCommandRequestDto = serde_json::from_value(request.clone()).unwrap();
    assert_eq!(decoded.validate(), Ok(()));
    assert_eq!(serde_json::to_value(decoded).unwrap(), request);
}
