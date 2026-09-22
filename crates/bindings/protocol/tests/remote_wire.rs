use floe_protocol::*;
use serde_json::{Value, json};
use uuid::Uuid;

fn producer() -> Value {
    json!({
        "schema_version": 1,
        "instance_id": Uuid::new_v4(),
        "execution_owner": Uuid::new_v4(),
        "audience": "floe.server:test",
        "key_id": Uuid::new_v4(),
        "public_key": "public",
        "fingerprint": "fingerprint"
    })
}

fn challenge() -> Value {
    json!({
        "schema_version": 1,
        "pairing_id": Uuid::new_v4(),
        "challenge_id": Uuid::new_v4(),
        "challenge_b64url": "signed-evidence",
        "producer_signature": "signature",
        "producer": producer(),
        "issuer": {"key_id": Uuid::new_v4(), "public_key": "public", "fingerprint": "fingerprint"},
        "expires_at_unix_ms": 100
    })
}

fn envelope(operation: Value) -> Value {
    json!({"schema_version": APP_WIRE_VERSION, "request_id": Uuid::new_v4(), "operation": operation})
}

#[test]
fn pairing_contract_roundtrips_only_setup_intent_and_signed_evidence() {
    let target = json!({"base_url": "http://127.0.0.1:8431"});
    let commands = [
        json!({"kind": "prepare"}),
        json!({"kind": "confirm", "target": target, "challenge": challenge(), "polling_proof": "proof"}),
        json!({"kind": "status", "target": target, "pairing_id": Uuid::new_v4(), "polling_proof": "proof"}),
        json!({"kind": "finalize", "target": target, "pairing_id": Uuid::new_v4(), "challenge": challenge(), "polling_proof": "proof"}),
        json!({"kind": "read_result", "operation_id": Uuid::new_v4(), "release": false}),
    ];
    for command in commands {
        let request = envelope(command);
        let decoded: RemotePairingRequestDto = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(decoded.validate(), Ok(()));
        assert_eq!(serde_json::to_value(decoded).unwrap(), request);
        for field in [
            "person_id",
            "device_id",
            "route",
            "remote_route",
            "bearer_token",
            "token",
            "purpose",
            "recipient",
            "external",
            "allow_external",
            "calendar_connections",
            "pairing",
        ] {
            let mut forged = request.clone();
            forged["operation"][field] = json!("forged");
            assert!(
                serde_json::from_value::<RemotePairingRequestDto>(forged).is_err(),
                "{field}"
            );
            let mut forged = request.clone();
            forged[field] = json!("forged");
            assert!(
                serde_json::from_value::<RemotePairingRequestDto>(forged).is_err(),
                "{field}"
            );
            if request["operation"].get("target").is_some() {
                let mut forged = request.clone();
                forged["operation"]["target"][field] = json!("forged");
                assert!(
                    serde_json::from_value::<RemotePairingRequestDto>(forged).is_err(),
                    "{field}"
                );
            }
        }
    }
}

#[test]
fn remote_access_rejects_all_transport_and_identity_fields() {
    let connection = Uuid::new_v4();
    let source = json!({"connector_id": "google", "connection_id": connection, "resource": "calendar.timeline:test"});
    let mut commands = vec![
        json!({"kind": "inspect_producer"}),
        json!({"kind": "review_and_enroll", "producer": producer()}),
        json!({"kind": "enrollment_status", "enrollment_id": Uuid::new_v4()}),
        json!({"kind": "read_result", "operation_id": Uuid::new_v4(), "release": true}),
    ];
    for kind in [
        "calendar_grant_preview",
        "calendar_grant_review",
        "view_grant_preview",
        "view_grant_review",
    ] {
        let mut command = source.clone();
        command["kind"] = json!(kind);
        if kind.starts_with("view") {
            command["view_id"] = json!("mail.communication");
            command["consumer"] = json!("communication");
        }
        if kind.ends_with("_review") {
            command["expected_producer_fingerprint"] = json!("fingerprint");
        }
        if kind == "view_grant_review" {
            command["expected_source_authority"] =
                json!({"incarnation": Uuid::new_v4(), "epoch": 1});
            command["expected_connection_revision"] = json!(2);
            command["expected_provider_identity"] = json!("provider");
            command["expected_recipient"] = json!("recipient");
        }
        commands.push(command);
    }
    for kind in [
        "calendar_grant_status",
        "calendar_grant_pause",
        "view_grant_status",
        "view_grant_pause",
    ] {
        let mut command = json!({"kind": kind, "grant_id": Uuid::new_v4()});
        if kind.ends_with("pause") {
            command["expected_authority"] =
                json!({"incarnation": Uuid::new_v4(), "access_epoch": 1});
        }
        commands.push(command);
    }
    for command in commands {
        let request = envelope(command);
        let decoded: RemoteAccessRequestDto = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(decoded.validate(), Ok(()));
        assert_eq!(serde_json::to_value(decoded).unwrap(), request);
        for field in [
            "person_id",
            "device_id",
            "base_url",
            "target",
            "route",
            "bearer_token",
            "token",
            "purpose",
            "recipient",
            "external",
            "allow_external",
            "calendar_connections",
            "pairing",
        ] {
            let mut forged = request.clone();
            forged["operation"][field] = json!("forged");
            assert!(
                serde_json::from_value::<RemoteAccessRequestDto>(forged).is_err(),
                "{field}"
            );
            let mut forged = request.clone();
            forged[field] = json!("forged");
            assert!(
                serde_json::from_value::<RemoteAccessRequestDto>(forged).is_err(),
                "{field}"
            );
        }
    }
}

#[test]
fn remote_requests_reject_invalid_versions_ids_and_bounds() {
    let request = envelope(
        json!({"kind": "status", "target": {"base_url": "http://localhost:8431"}, "pairing_id": Uuid::new_v4(), "polling_proof": "proof"}),
    );
    for (field, value) in [
        ("schema_version", json!(3)),
        ("request_id", json!(Uuid::nil())),
    ] {
        let mut invalid = request.clone();
        invalid[field] = value;
        let decoded: RemotePairingRequestDto = serde_json::from_value(invalid).unwrap();
        assert!(decoded.validate().is_err());
    }
    for value in [
        "".to_owned(),
        "x".repeat(2049),
        " http://localhost".to_owned(),
        "http://localhost\n".to_owned(),
    ] {
        let mut invalid = request.clone();
        invalid["operation"]["target"]["base_url"] = json!(value);
        let decoded: RemotePairingRequestDto = serde_json::from_value(invalid).unwrap();
        assert!(decoded.validate().is_err());
    }
    let mut invalid = request;
    invalid["operation"]["polling_proof"] = json!("x".repeat(257));
    assert!(
        serde_json::from_value::<RemotePairingRequestDto>(invalid)
            .unwrap()
            .validate()
            .is_err()
    );
    for operation in [
        json!({"kind": "enrollment_status", "enrollment_id": "invalid"}),
        json!({"kind": "calendar_grant_preview", "connector_id": "google", "connection_id": Uuid::new_v4(), "resource": "x".repeat(2049)}),
        json!({"kind": "read_result", "operation_id": Uuid::nil(), "release": false}),
    ] {
        assert!(
            serde_json::from_value::<RemoteAccessRequestDto>(envelope(operation))
                .unwrap()
                .validate()
                .is_err()
        );
    }
    assert_eq!(APP_WIRE_VERSION, 2);
    assert_eq!(PROTOCOL_VERSION, 1);
}

#[test]
fn pairing_credential_is_only_approved_output_and_never_debug_output() {
    let token = "newly_issued_secret_for_keychain";
    let approved: PairingOutcomeDto = serde_json::from_value(
        json!({"status": "approved", "client_id": "client", "token": token}),
    )
    .unwrap();
    assert_eq!(serde_json::to_value(&approved).unwrap()["token"], token);
    assert!(!format!("{approved:?}").contains(token));
    for status in [
        "pending",
        "local_confirmed",
        "rejected",
        "expired",
        "repair_required",
    ] {
        assert!(
            serde_json::from_value::<PairingOutcomeDto>(json!({"status": status, "token": token}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<PairingOutcomeDto>(
                json!({"status": status, "client_id": "client"})
            )
            .is_err()
        );
    }
    assert!(
        serde_json::from_value::<PairingOutcomeDto>(
            json!({"status": "approved", "client_id": "client"})
        )
        .is_err()
    );
}

#[test]
fn obsolete_remote_agent_vault_operations_are_not_aliases() {
    for kind in [
        "remote_pairing_prepare",
        "remote_pairing_confirm",
        "remote_pairing_status",
        "remote_pairing_finalize",
        "remote_authority_inspect_producer",
        "remote_authority_review_and_enroll",
        "remote_authority_enrollment_status",
        "remote_calendar_grant_preview",
        "remote_calendar_grant_review",
        "remote_calendar_grant_status",
        "remote_calendar_grant_pause",
        "remote_view_grant_preview",
        "remote_view_grant_review",
        "remote_view_grant_status",
        "remote_view_grant_pause",
    ] {
        assert!(serde_json::from_value::<AppCommandDto>(json!({"kind": kind})).is_err());
    }
}
