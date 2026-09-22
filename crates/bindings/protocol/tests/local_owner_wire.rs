use floe_protocol::{AppCommandRequestDto, AppQueryRequestDto};
use serde_json::json;
use uuid::Uuid;

#[test]
fn vault_commands_are_strict_owner_intent() {
    for kind in ["vault.create", "vault.unlock", "vault.lock"] {
        let request = json!({"schema_version": 2, "request_id": Uuid::new_v4(), "command_id": Uuid::new_v4(), "command": {"kind": kind}});
        serde_json::from_value::<AppCommandRequestDto>(request.clone())
            .unwrap()
            .validate()
            .unwrap();
        for field in [
            "person_id",
            "device_id",
            "bearer",
            "token",
            "base_url",
            "endpoint",
            "route",
            "unknown",
        ] {
            for nested in [false, true] {
                let mut invalid = request.clone();
                let target = if nested {
                    &mut invalid["command"]
                } else {
                    &mut invalid
                };
                target[field] = json!("foreign");
                assert!(
                    serde_json::from_value::<AppCommandRequestDto>(invalid).is_err(),
                    "{kind}: {field}"
                );
            }
        }
        for field in ["request_id", "command_id"] {
            let mut invalid = request.clone();
            invalid[field] = json!(Uuid::nil());
            assert!(
                serde_json::from_value::<AppCommandRequestDto>(invalid)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
    }
}

#[test]
fn vault_queries_reject_identity_and_invalid_operation_ids() {
    for query in [
        json!({"kind":"vault.status"}),
        json!({"kind":"vault.read_result", "operation_id":Uuid::new_v4(), "release":false}),
    ] {
        let request = json!({"schema_version":2, "request_id":Uuid::new_v4(), "query":query});
        serde_json::from_value::<AppQueryRequestDto>(request.clone())
            .unwrap()
            .validate()
            .unwrap();
        for field in [
            "person_id",
            "device_id",
            "bearer",
            "base_url",
            "route",
            "unknown",
        ] {
            let mut invalid = request.clone();
            invalid["query"][field] = json!("foreign");
            assert!(serde_json::from_value::<AppQueryRequestDto>(invalid).is_err());
        }
    }
    let invalid = json!({"schema_version":2, "request_id":Uuid::new_v4(), "query":{"kind":"vault.read_result", "operation_id":Uuid::nil(), "release":false}});
    assert!(
        serde_json::from_value::<AppQueryRequestDto>(invalid)
            .unwrap()
            .validate()
            .is_err()
    );
}

#[test]
fn local_owner_commands_reject_identity_topology_and_malformed_ids() {
    let commands = [
        json!({"kind":"conversation.session.start"}),
        json!({"kind":"conversation.session.resume"}),
        json!({"kind":"conversation.session.recover", "session_id":Uuid::new_v4(), "expected_revision":1}),
        json!({"kind":"experts.registry.configure", "change":{"instance_id":Uuid::new_v4(), "expected_revision":1, "target":{"kind":"installation", "id":Uuid::new_v4(), "enabled":true}}}),
        json!({"kind":"experts.calendar.install", "setup":{"instance_id":Uuid::new_v4(), "expected_revision":0, "setup_id":Uuid::new_v4(), "provider":"event_kit", "calendar_ids":["calendar"], "connection_scope":"selected", "connection_revision":1, "source_authority":null, "reviewed_native_subject_fingerprint":null}}),
        json!({"kind":"access.calendar.configure", "change":{"instance_id":Uuid::new_v4(), "expected_revision":1, "setup_id":Uuid::new_v4(), "change":{"kind":"set_enabled", "enabled":false}}}),
        json!({"kind":"access.personal.configure", "connector":"attention.macos", "change":{"kind":"set_enabled", "enabled":false}}),
        json!({"kind":"access.contacts.configure", "connector":"contacts.apple", "change":{"kind":"set_enabled", "enabled":false}}),
        json!({"kind":"knowledge.memory.decide", "candidate_id":Uuid::new_v4(), "decision":"approve"}),
    ];
    for command in commands {
        let request = json!({"schema_version":2, "request_id":Uuid::new_v4(), "command_id":Uuid::new_v4(), "command":command});
        serde_json::from_value::<AppCommandRequestDto>(request.clone())
            .unwrap()
            .validate()
            .unwrap();
        for field in [
            "person_id",
            "device_id",
            "bearer",
            "token",
            "base_url",
            "endpoint",
            "route",
            "unknown",
        ] {
            for path in ["", "command", "command.setup", "command.change"] {
                let mut invalid = request.clone();
                let mut target = &mut invalid;
                for segment in path.split('.').filter(|segment| !segment.is_empty()) {
                    target = &mut target[segment];
                }
                if !target.is_object() {
                    continue;
                }
                target[field] = json!("foreign");
                assert!(
                    serde_json::from_value::<AppCommandRequestDto>(invalid).is_err(),
                    "{path}.{field}: {command}"
                );
            }
        }
    }
    for command in [
        json!({"kind":"conversation.session.recover", "session_id":Uuid::nil(), "expected_revision":1}),
        json!({"kind":"knowledge.memory.decide", "candidate_id":Uuid::nil(), "decision":"approve"}),
        json!({"kind":"experts.registry.configure", "change":{"instance_id":Uuid::nil(), "expected_revision":1, "target":{"kind":"installation", "id":Uuid::new_v4(), "enabled":true}}}),
    ] {
        let request = json!({"schema_version":2, "request_id":Uuid::new_v4(), "command_id":Uuid::new_v4(), "command":command});
        assert!(
            serde_json::from_value::<AppCommandRequestDto>(request)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[test]
fn every_owner_result_query_requires_non_nil_correlation_and_rejects_authority_fields() {
    for kind in [
        "conversation.session.read_result",
        "experts.read_result",
        "access.local.read_result",
        "knowledge.read_result",
        "connections.read_result",
        "actions.read_result",
    ] {
        let request = json!({"schema_version":2, "request_id":Uuid::new_v4(), "query":{"kind":kind, "operation_id":Uuid::new_v4(), "release":false}});
        serde_json::from_value::<AppQueryRequestDto>(request.clone())
            .unwrap()
            .validate()
            .unwrap();
        let mut invalid = request.clone();
        invalid["query"]["operation_id"] = json!(Uuid::nil());
        assert!(
            serde_json::from_value::<AppQueryRequestDto>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
        for field in [
            "person_id",
            "device_id",
            "bearer",
            "endpoint",
            "route",
            "unknown",
        ] {
            let mut invalid = request.clone();
            invalid["query"][field] = json!("foreign");
            assert!(serde_json::from_value::<AppQueryRequestDto>(invalid).is_err());
        }
    }
}

#[test]
fn day_actions_and_native_completions_accept_intent_not_authority() {
    for command in [
        json!({"kind":"day.mutate", "day":{"date":"2026-09-22", "timezone_offset_seconds":0, "now":"2026-09-22T00:00:00Z"}, "mutation":{"type":"create_note", "content":"note", "occurred_at":"2026-09-22T00:00:00Z"}}),
        json!({"kind":"actions.calendar", "operation":{"kind":"execute", "action_id":Uuid::new_v4()}}),
        json!({"kind":"context.apply", "command":{"kind":"complete_attention_acquisition", "host_epoch":"native-host", "result":{"request_id":Uuid::new_v4(), "host_epoch":"native-host", "mode":"inspect_subject", "native_subject_fingerprint_before":"a".repeat(64), "native_subject_fingerprint_after":"a".repeat(64), "permission_class":"authorized"}}}),
    ] {
        let request = json!({"schema_version":2, "request_id":Uuid::new_v4(), "command_id":Uuid::new_v4(), "command":command});
        serde_json::from_value::<AppCommandRequestDto>(request.clone())
            .unwrap()
            .validate()
            .unwrap();
        for path in [
            "command",
            "command.day",
            "command.mutation",
            "command.operation",
            "command.command",
            "command.command.result",
        ] {
            for field in [
                "person_id",
                "device_id",
                "bearer",
                "endpoint",
                "route",
                "unknown",
            ] {
                let mut invalid = request.clone();
                let mut target = &mut invalid;
                for segment in path.split('.') {
                    target = &mut target[segment];
                }
                if !target.is_object() {
                    continue;
                }
                target[field] = json!("foreign");
                assert!(
                    serde_json::from_value::<AppCommandRequestDto>(invalid).is_err(),
                    "{path}.{field}"
                );
            }
        }
    }
    for operation in [
        json!({"kind":"execute", "action_id":Uuid::nil()}),
        json!({"kind":"list"}),
    ] {
        let request = json!({"schema_version":2, "request_id":Uuid::new_v4(), "command_id":Uuid::new_v4(), "command":{"kind":"actions.calendar", "operation":operation}});
        assert!(
            serde_json::from_value::<AppCommandRequestDto>(request)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}
