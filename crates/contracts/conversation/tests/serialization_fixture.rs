use floe_conversation_contract::{AdmissionTarget, MessageAdmissionRequest};
use serde_json::Value;

#[test]
fn new_message_contract_round_trips_without_becoming_a_storage_schema() {
    let fixture = include_str!("fixtures/new-message-admission.json");
    let expected: Value = serde_json::from_str(fixture).expect("valid fixture JSON");
    let request: MessageAdmissionRequest =
        serde_json::from_value(expected.clone()).expect("valid contract request");

    let AdmissionTarget::New {
        identity,
        conversation_id,
        branch_id,
    } = &request.target
    else {
        panic!("fixture explicitly admits a new conversation");
    };
    assert_eq!(identity.definition_id, "expert.schedule");
    assert_eq!(identity.definition_revision, 3);
    assert!(conversation_id.is_valid());
    assert!(branch_id.is_valid());
    assert_eq!(request.message.text, "Review my next appointment.");

    let actual = serde_json::to_value(request).expect("contract serialization");
    assert_eq!(actual, expected);
}
