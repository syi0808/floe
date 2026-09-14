use floe_protocol::{
    APP_WIRE_VERSION, AppCommandRequestDto, AppCommandResultDto, AppEventsResultDto,
    AppQueryRequestDto, AppQueryResultDto, AppResponseDto, PROTOCOL_VERSION,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn fixture(name: &str) -> Value {
    let source = match name {
        "start_turn" => include_str!("fixtures/app_wire_v2/start_turn.json"),
        "command_receipt" => include_str!("fixtures/app_wire_v2/command_receipt.json"),
        "get_run" => include_str!("fixtures/app_wire_v2/get_run.json"),
        "get_message" => include_str!("fixtures/app_wire_v2/get_message.json"),
        "message" => include_str!("fixtures/app_wire_v2/message.json"),
        "resync_required" => include_str!("fixtures/app_wire_v2/resync_required.json"),
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
    assert_round_trip::<AppResponseDto<AppCommandResultDto>>("command_receipt");
    assert_round_trip::<AppQueryRequestDto>("get_run");
    assert_round_trip::<AppQueryRequestDto>("get_message");
    assert_round_trip::<AppResponseDto<AppQueryResultDto>>("message");
    assert_round_trip::<AppResponseDto<AppEventsResultDto>>("resync_required");
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

    let request: floe_protocol::AppEventsRequestDto = serde_json::from_value(json!({
        "schema_version": 2,
        "request_id": "00000000-0000-0000-0000-000000000006",
        "limit": 257
    }))
    .unwrap();
    assert_eq!(request.validate(), Err("limit"));
}
