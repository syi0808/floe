use std::collections::BTreeMap;

use floe_protocol::*;

#[test]
fn registry_configuration_transport_rejects_raw_grants_state_and_unknown_operations() {
    let change = serde_json::json!({
        "instance_id": "00000000-0000-4000-8000-000000000001",
        "expected_revision": 7,
        "target": {"kind": "assignment", "id": "00000000-0000-4000-8000-000000000002", "enabled": false}
    });
    let action = serde_json::json!({"kind": "experts.registry.configure", "change": change});
    let parsed: AppCommandDto = serde_json::from_value(action.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), action);
    for field in ["granted_views", "private_state", "person_id"] {
        let mut forged = action.clone();
        forged["change"]["target"][field] = serde_json::json!([]);
        assert!(serde_json::from_value::<AppCommandDto>(forged).is_err());
    }
    let mut forged = action;
    forged["change"]["target"]["kind"] = serde_json::json!("grant_calendar");
    assert!(serde_json::from_value::<AppCommandDto>(forged).is_err());
}
use serde_json::json;
use uuid::Uuid;

#[test]
fn calendar_scoped_conversation_actions_are_not_part_of_the_protocol() {
    for action in [
        json!({"kind": "calendar_session", "operation": {"kind": "start", "setup_id": Uuid::new_v4()}}),
        json!({"kind": "calendar_turn", "request": {"session_id": Uuid::new_v4()}}),
    ] {
        assert!(serde_json::from_value::<AppCommandDto>(action).is_err());
    }
}

#[test]
fn obsolete_conversation_turn_wire_is_rejected() {
    let action = json!({
        "kind": "conversation_turn",
        "request": {
            "session_id": Uuid::new_v4(),
            "expected_revision": 2,
            "text": "Help me plan the afternoon"
        }
    });
    assert!(serde_json::from_value::<AppCommandDto>(action).is_err());
}

#[test]
fn proposal_inspection_transport_accepts_only_a_recorded_reference() {
    let action = json!({
        "kind": "actions.proposal.inspect",
        "session_id": Uuid::new_v4(),
        "invocation_id": Uuid::new_v4(),
    });
    let parsed: AppQueryDto = serde_json::from_value(action.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), action);
    for field in [
        "person_id",
        "destination",
        "approve",
        "execute",
        "retry",
        "output",
        "key",
    ] {
        let mut forged = action.clone();
        forged[field] = json!(true);
        assert!(serde_json::from_value::<AppQueryDto>(forged).is_err());
    }
}

#[test]
fn memory_review_transport_accepts_only_user_decisions_on_candidate_ids() {
    let inspect = json!({"kind": "knowledge.memory.review"});
    let parsed: AppQueryDto = serde_json::from_value(inspect.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), inspect);

    let decide = json!({
        "kind": "knowledge.memory.decide",
        "candidate_id": Uuid::new_v4(),
        "decision": "approve"
    });
    let parsed: AppCommandDto = serde_json::from_value(decide.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), decide);

    for field in ["actor", "payload", "statement", "person_id", "execute"] {
        let mut forged = decide.clone();
        forged[field] = json!("untrusted");
        assert!(serde_json::from_value::<AppCommandDto>(forged).is_err());
    }
}

#[test]
fn memory_overview_transport_is_read_only() {
    let inspect = json!({"kind": "knowledge.memory.overview"});
    let parsed: AppQueryDto = serde_json::from_value(inspect.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), inspect);

    for field in ["target_id", "statement", "delete", "actor", "person_id"] {
        let mut forged = inspect.clone();
        forged[field] = json!("untrusted");
        assert!(serde_json::from_value::<AppQueryDto>(forged).is_err());
    }
}

#[test]
fn snapshot_has_a_versioned_stable_wire_shape() {
    let snapshot = DaySnapshotDto {
        calendar: None,
        schema_version: PROTOCOL_VERSION,
        person_id: "00000000-0000-0000-0000-000000000001".into(),
        date: "2026-09-02".into(),
        generated_at: "2026-09-02T10:30:00Z".into(),
        timezone_offset_seconds: 32_400,
        now_event_id: Some("00000000-0000-0000-0000-000000000002".into()),
        next_event_id: None,
        overdue_task_count: 2,
        items: vec![TimelineItemDto::Note(NoteDto {
            id: "00000000-0000-0000-0000-000000000003".into(),
            person_id: "00000000-0000-0000-0000-000000000001".into(),
            content: "Protocol boundary".into(),
            source: SourceRefDto::Capture {
                capture_id: "00000000-0000-0000-0000-000000000004".into(),
            },
            created_at: "2026-09-02T09:00:00Z".into(),
            updated_at: "2026-09-02T09:05:00Z".into(),
            revision: 1,
            deleted_at: None,
        })],
    };

    assert_eq!(
        serde_json::to_value(snapshot).unwrap(),
        json!({
            "schema_version": 1,
            "person_id": "00000000-0000-0000-0000-000000000001",
            "date": "2026-09-02",
            "generated_at": "2026-09-02T10:30:00Z",
            "timezone_offset_seconds": 32400,
            "now_event_id": "00000000-0000-0000-0000-000000000002",
            "next_event_id": null,
            "overdue_task_count": 2,
            "calendar": null,
            "items": [{
                "kind": "note",
                "id": "00000000-0000-0000-0000-000000000003",
                "person_id": "00000000-0000-0000-0000-000000000001",
                "content": "Protocol boundary",
                "source": {
                    "kind": "capture",
                    "capture_id": "00000000-0000-0000-0000-000000000004"
                },
                "created_at": "2026-09-02T09:00:00Z",
                "updated_at": "2026-09-02T09:05:00Z",
                "revision": 1,
                "deleted_at": null
            }]
        })
    );
}

#[test]
fn capture_and_error_envelopes_have_explicit_tags() {
    let processing = CaptureProcessingDto::Classified {
        target: DomainRefDto::Task {
            id: "task-id".into(),
        },
        classified_at: "2026-09-02T10:30:00Z".into(),
    };
    assert_eq!(
        serde_json::to_value(processing).unwrap(),
        json!({
            "status": "classified",
            "target": {"kind": "task", "id": "task-id"},
            "classified_at": "2026-09-02T10:30:00Z"
        })
    );

    let mut metadata = BTreeMap::new();
    metadata.insert("actual".into(), "2".into());
    metadata.insert("expected".into(), "1".into());
    let response: ResponseEnvelopeDto<DaySnapshotDto> = ResponseEnvelopeDto::error(ErrorDto {
        code: ErrorCodeDto::Conflict,
        message: "stale revision".into(),
        field: None,
        metadata,
    });
    assert_eq!(
        serde_json::to_value(response).unwrap(),
        json!({
            "schema_version": 1,
            "status": "error",
            "error": {
                "code": "conflict",
                "message": "stale revision",
                "metadata": {"actual": "2", "expected": "1"}
            }
        })
    );
}

#[test]
fn all_command_variants_round_trip_through_json() {
    let at = "2026-09-02T10:30:00Z".to_owned();
    let timed = EventScheduleDto::Timed {
        starts_at: at.clone(),
        ends_at: "2026-09-02T11:30:00Z".into(),
        timezone: "UTC".into(),
    };
    let commands = vec![
        DayMutationDto::SubmitCapture {
            input: "input".into(),
            occurred_at: at.clone(),
        },
        DayMutationDto::ClassifyCapture {
            capture_id: "capture".into(),
            expected_revision: 0,
            classification: ClassificationDto::Note {
                content: "note".into(),
            },
            occurred_at: at.clone(),
        },
        DayMutationDto::CreateEvent {
            title: "event".into(),
            schedule: timed.clone(),
            occurred_at: at.clone(),
        },
        DayMutationDto::CreateTask {
            title: "task".into(),
            deadline: None,
            priority: PriorityDto::Normal,
            occurred_at: at.clone(),
        },
        DayMutationDto::CreateNote {
            content: "note".into(),
            occurred_at: at.clone(),
        },
        DayMutationDto::UpdateEvent {
            event_id: "event".into(),
            expected_revision: 1,
            title: "event".into(),
            schedule: timed,
            occurred_at: at.clone(),
        },
        DayMutationDto::UpdateTask {
            task_id: "task".into(),
            expected_revision: 1,
            title: "task".into(),
            deadline: Some(at.clone()),
            priority: PriorityDto::High,
            occurred_at: at.clone(),
        },
        DayMutationDto::UpdateNote {
            note_id: "note".into(),
            expected_revision: 1,
            content: "note".into(),
            occurred_at: at.clone(),
        },
        DayMutationDto::SetTaskCompletion {
            task_id: "task".into(),
            expected_revision: 1,
            completed: true,
            occurred_at: at.clone(),
        },
        DayMutationDto::DeleteItem {
            target: DomainRefDto::Note { id: "note".into() },
            expected_revision: 1,
            occurred_at: at,
        },
    ];

    for command in commands {
        let encoded = serde_json::to_string(&command).unwrap();
        assert_eq!(
            serde_json::from_str::<DayMutationDto>(&encoded).unwrap(),
            command
        );
    }
}
#[test]
fn connections_transport_is_read_only() {
    let action = json!({"kind": "connections.overview"});
    assert_eq!(
        serde_json::from_value::<AppQueryDto>(action).unwrap(),
        AppQueryDto::ConnectionsOverview {}
    );
    for forged in [
        json!({"kind": "connections.overview", "disconnect": true}),
        json!({"kind": "connections.overview", "credential": "secret"}),
    ] {
        assert!(serde_json::from_value::<AppQueryDto>(forged).is_err());
    }
}

#[test]
fn admitted_day_envelopes_and_nested_tags_are_stable() {
    let request_id = Uuid::new_v4();
    let command_id = Uuid::new_v4();
    let day = json!({"date":"2026-09-02", "timezone_offset_seconds":32400, "end_timezone_offset_seconds":null, "now":"2026-09-02T10:30:00Z"});
    let mutation = json!({"type":"classify_capture", "capture_id":Uuid::new_v4(), "expected_revision":0, "classification":{"kind":"event", "title":"Review", "schedule":{"kind":"timed", "starts_at":"2026-09-02T11:00:00Z", "ends_at":"2026-09-02T12:00:00Z", "timezone":"Asia/Seoul"}}, "occurred_at":"2026-09-02T10:30:00Z"});
    let command = json!({"schema_version":2, "request_id":request_id, "command_id":command_id, "command":{"kind":"day.mutate", "day":day, "mutation":mutation}});
    let decoded: AppCommandRequestDto = serde_json::from_value(command.clone()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), command);
    let query = json!({"schema_version":2, "request_id":request_id, "query":{"kind":"day.snapshot", "day":day}});
    let decoded: AppQueryRequestDto = serde_json::from_value(query.clone()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), query);
}
