use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use floe_domain::{
    Capture, CaptureSource, DaySnapshot, DomainRef, Event, EventId, EventSchedule, Note, PersonId,
    Priority, SourceRef, Task, TimedSchedule, TimelineItem,
};
use floe_protocol::*;

#[test]
fn calendar_expert_setup_transport_is_typed_and_never_accepts_ambient_grants_or_keys() {
    let action = serde_json::json!({
        "kind": "calendar_experts",
        "setup": {
            "instance_id": "00000000-0000-4000-8000-000000000001",
            "expected_revision": 0,
            "setup_id": "00000000-0000-4000-8000-000000000002",
            "provider": "event_kit",
            "calendar_ids": ["explicit-calendar"]
        }
    });
    let parsed: AgentVaultActionDto = serde_json::from_value(action.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), action);
    for field in [
        "enabled",
        "person_id",
        "key",
        "data_class",
        "private_state",
        "view_handle",
    ] {
        let mut forged = action.clone();
        forged["setup"][field] = serde_json::json!(true);
        assert!(serde_json::from_value::<AgentVaultActionDto>(forged).is_err());
    }
    let view_change = serde_json::json!({
        "kind": "registry", "change": {
            "instance_id": "00000000-0000-4000-8000-000000000001",
            "expected_revision": 1,
            "target": {"kind": "calendar_view", "id": "00000000-0000-4000-8000-000000000002", "enabled": false}
        }
    });
    let parsed: AgentVaultActionDto = serde_json::from_value(view_change.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), view_change);
    let legacy = serde_json::json!({
        "request_id": "00000000-0000-4000-8000-000000000001", "events": [],
        "next_sequence": 0, "done": true, "state": "ready", "session": null, "failure": null
    });
    let result: AgentVaultResultDto = serde_json::from_value(legacy.clone()).unwrap();
    assert!(result.calendar_experts.is_none());
    assert_eq!(serde_json::to_value(result).unwrap(), legacy);
}

#[test]
fn registry_configuration_transport_rejects_raw_grants_state_and_unknown_operations() {
    let change = serde_json::json!({
        "instance_id": "00000000-0000-4000-8000-000000000001",
        "expected_revision": 7,
        "target": {"kind": "assignment", "id": "00000000-0000-4000-8000-000000000002", "enabled": false}
    });
    let action = serde_json::json!({"kind": "registry", "change": change});
    let parsed: AgentVaultActionDto = serde_json::from_value(action.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), action);
    for field in ["granted_view_handles", "private_state", "person_id"] {
        let mut forged = action.clone();
        forged["change"]["target"][field] = serde_json::json!([]);
        assert!(serde_json::from_value::<AgentVaultActionDto>(forged).is_err());
    }
    let mut forged = action;
    forged["change"]["target"]["kind"] = serde_json::json!("grant_calendar");
    assert!(serde_json::from_value::<AgentVaultActionDto>(forged).is_err());
}
use serde_json::json;
use uuid::Uuid;

#[test]
fn calendar_session_transport_cannot_supply_classification_grants_or_model_input() {
    let action = json!({"kind": "calendar_session", "operation": {
        "kind": "start", "setup_id": Uuid::new_v4(),
    }});
    let parsed: AgentVaultActionDto = serde_json::from_value(action.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), action);
    for field in [
        "person_id",
        "provider",
        "data_classes",
        "calendar_ids",
        "view_handle",
        "prompt",
        "model",
    ] {
        let mut forged = action.clone();
        forged["operation"][field] = json!("untrusted");
        assert!(serde_json::from_value::<AgentVaultActionDto>(forged).is_err());
    }
    let legacy = floe_agent::AgentSession::new(floe_domain::PersonId::new());
    let wire = serde_json::to_value(&legacy).unwrap();
    assert!(wire.get("scope").is_none());
    assert_eq!(
        serde_json::from_value::<floe_agent::AgentSession>(wire).unwrap(),
        legacy
    );
}

#[test]
fn calendar_turn_transport_requires_explicit_bounded_model_and_destination() {
    let action = json!({
        "kind": "calendar_turn",
        "request": {
            "session_id": Uuid::new_v4(),
            "expected_revision": 3,
            "prompt": {"kind": "propose_focus", "focus_minutes": 60},
            "model": "foundation_models",
            "day": {
                "start_date": "2026-09-08",
                "end_date_exclusive": "2026-09-09",
                "timezone_offset_seconds": 32400
            },
            "starts_at": "2026-09-08T00:00:00Z",
            "ends_at": "2026-09-08T12:00:00Z",
            "destination": {
                "provider": "event_kit",
                "calendar_id": "explicit-calendar",
                "connection_revision": 9,
                "timezone": "Asia/Seoul"
            }
        }
    });
    let parsed: AgentVaultActionDto = serde_json::from_value(action.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), action);
    for field in [
        "person_id",
        "view_handle",
        "assignment_id",
        "allowed_placements",
        "external_transfer_consent",
        "credential",
    ] {
        let mut forged = action.clone();
        forged["request"][field] = json!("untrusted");
        assert!(serde_json::from_value::<AgentVaultActionDto>(forged).is_err());
    }
}

#[test]
fn free_text_calendar_turn_accepts_only_a_redacted_server_route() {
    let action = json!({
        "kind": "calendar_turn",
        "request": {
            "session_id": Uuid::new_v4(),
            "expected_revision": 3,
            "prompt": {"kind": "free_text", "text": "What is next?"},
            "model": "foundation_models",
            "day": {
                "start_date": "2026-09-08",
                "end_date_exclusive": "2026-09-09",
                "timezone_offset_seconds": 32400
            },
            "starts_at": "2026-09-08T00:00:00Z",
            "ends_at": "2026-09-08T12:00:00Z",
            "destination": null,
            "remote_route": {
                "base_url": "http://127.0.0.1:8431",
                "bearer_token": "secret_token_value_that_is_long_enough",
                "purpose": "everyday_assistance",
                "external": true,
                "allow_external": false
            }
        }
    });
    let parsed: AgentVaultActionDto = serde_json::from_value(action.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), action);
    let rendered = format!("{parsed:?}");
    assert!(!rendered.contains("secret_token_value_that_is_long_enough"));
    assert!(rendered.contains("[REDACTED]"));
}

#[test]
fn proposal_inspection_transport_accepts_only_a_recorded_reference() {
    let action = json!({
        "kind": "inspect_proposal",
        "session_id": Uuid::new_v4(),
        "invocation_id": Uuid::new_v4(),
    });
    let parsed: AgentVaultActionDto = serde_json::from_value(action.clone()).unwrap();
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
        assert!(serde_json::from_value::<AgentVaultActionDto>(forged).is_err());
    }
}

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap()
}

#[test]
fn legacy_external_provenance_round_trips_without_assuming_a_calendar_selection() {
    let stored = json!({"External": {
        "connection_id": "legacy", "provider": "fixture", "resource_type": "calendar_event",
        "external_id": "event", "external_revision": "revision"
    }});
    let source: SourceRef = serde_json::from_value(stored.clone()).unwrap();
    let dto: SourceRefDto = source.into();
    let wire = serde_json::to_value(&dto).unwrap();
    assert_eq!(wire["kind"], "external");
    assert_eq!(wire["source"]["connection_id"], "legacy");
    let restored: SourceRef = dto.try_into().unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), stored);
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
fn command_and_nested_union_tags_are_stable() {
    let request = CommandRequestDto {
        schema_version: PROTOCOL_VERSION,
        person_id: "00000000-0000-0000-0000-000000000001".into(),
        day: DayQueryDto {
            date: "2026-09-02".into(),
            timezone_offset_seconds: 32_400,
            end_timezone_offset_seconds: None,
            now: "2026-09-02T10:30:00Z".into(),
        },
        command: CommandDto::ClassifyCapture {
            capture_id: "00000000-0000-0000-0000-000000000004".into(),
            expected_revision: 0,
            classification: ClassificationDto::Event {
                title: "Review".into(),
                schedule: EventScheduleDto::Timed {
                    starts_at: "2026-09-02T11:00:00Z".into(),
                    ends_at: "2026-09-02T12:00:00Z".into(),
                    timezone: "Asia/Seoul".into(),
                },
            },
            occurred_at: "2026-09-02T10:30:00Z".into(),
        },
    };

    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({
            "schema_version": 1,
            "person_id": "00000000-0000-0000-0000-000000000001",
            "day": {
                "date": "2026-09-02",
                "timezone_offset_seconds": 32400,
                "now": "2026-09-02T10:30:00Z"
            },
            "command": {
                "type": "classify_capture",
                "capture_id": "00000000-0000-0000-0000-000000000004",
                "expected_revision": 0,
                "classification": {
                    "kind": "event",
                    "title": "Review",
                    "schedule": {
                        "kind": "timed",
                        "starts_at": "2026-09-02T11:00:00Z",
                        "ends_at": "2026-09-02T12:00:00Z",
                        "timezone": "Asia/Seoul"
                    }
                },
                "occurred_at": "2026-09-02T10:30:00Z"
            }
        })
    );
}

#[test]
fn load_day_request_has_a_stable_wire_shape() {
    let request = LoadDayRequestDto {
        schema_version: PROTOCOL_VERSION,
        person_id: "00000000-0000-0000-0000-000000000001".into(),
        day: DayQueryDto {
            date: "2026-09-02".into(),
            timezone_offset_seconds: 32_400,
            end_timezone_offset_seconds: None,
            now: "2026-09-02T10:30:00Z".into(),
        },
    };

    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({
            "schema_version": 1,
            "person_id": "00000000-0000-0000-0000-000000000001",
            "day": {
                "date": "2026-09-02",
                "timezone_offset_seconds": 32400,
                "now": "2026-09-02T10:30:00Z"
            }
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
fn domain_snapshot_round_trip_preserves_all_item_kinds() {
    let person_id = PersonId(id("00000000-0000-0000-0000-000000000001"));
    let capture_id = floe_domain::CaptureId(id("00000000-0000-0000-0000-000000000004"));
    let now = Utc.with_ymd_and_hms(2026, 9, 2, 10, 30, 0).unwrap();
    let event = Event::new(
        person_id,
        "Review",
        EventSchedule::Timed(
            TimedSchedule::new(now, now + chrono::Duration::hours(1), "Asia/Seoul").unwrap(),
        ),
        SourceRef::Capture(capture_id),
        now,
    )
    .unwrap();
    let task = Task::new(
        person_id,
        "Ship",
        Some(now),
        Priority::High,
        SourceRef::Manual,
        now,
    )
    .unwrap();
    let note = Note::new(person_id, "Remember", SourceRef::Manual, now).unwrap();
    let snapshot = DaySnapshot {
        calendar: None,
        person_id,
        date: now.date_naive(),
        generated_at: now,
        timezone_offset_seconds: 32_400,
        now_event_id: Some(event.id),
        next_event_id: None,
        overdue_task_count: 1,
        items: vec![
            TimelineItem::Event(event),
            TimelineItem::Task(task),
            TimelineItem::Note(note),
        ],
    };

    let dto = DaySnapshotDto::try_from(snapshot.clone()).unwrap();
    assert_eq!(DaySnapshot::try_from(dto).unwrap(), snapshot);
}

#[test]
fn capture_round_trip_preserves_processing_and_revision() {
    let person_id = PersonId(id("00000000-0000-0000-0000-000000000001"));
    let now = Utc.with_ymd_and_hms(2026, 9, 2, 10, 30, 0).unwrap();
    let mut capture = Capture::new(person_id, "Ship", now, CaptureSource::Typed).unwrap();
    capture.classify(
        DomainRef::Event(EventId(id("00000000-0000-0000-0000-000000000002"))),
        now,
    );

    let dto = CaptureDto::from(capture.clone());
    assert_eq!(dto.revision, 1);
    assert_eq!(Capture::try_from(dto).unwrap(), capture);
}

#[test]
fn conversion_rejects_invalid_versions_and_domain_values() {
    let snapshot = DaySnapshotDto {
        calendar: None,
        schema_version: 99,
        person_id: "00000000-0000-0000-0000-000000000001".into(),
        date: "2026-09-02".into(),
        generated_at: "2026-09-02T10:30:00Z".into(),
        timezone_offset_seconds: 0,
        now_event_id: None,
        next_event_id: None,
        overdue_task_count: 0,
        items: vec![],
    };
    assert!(matches!(
        DaySnapshot::try_from(snapshot),
        Err(ProtocolConversionError::UnsupportedVersion { actual: 99, .. })
    ));

    let schedule = EventScheduleDto::Timed {
        starts_at: "2026-09-02T10:30:00Z".into(),
        ends_at: "2026-09-02T10:30:00Z".into(),
        timezone: "UTC".into(),
    };
    assert!(EventSchedule::try_from(schedule).is_err());

    let invalid_id = SourceRefDto::Capture {
        capture_id: "not-a-uuid".into(),
    };
    assert!(SourceRef::try_from(invalid_id).is_err());
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
        CommandDto::SubmitCapture {
            input: "input".into(),
            occurred_at: at.clone(),
        },
        CommandDto::ClassifyCapture {
            capture_id: "capture".into(),
            expected_revision: 0,
            classification: ClassificationDto::Note {
                content: "note".into(),
            },
            occurred_at: at.clone(),
        },
        CommandDto::CreateEvent {
            title: "event".into(),
            schedule: timed.clone(),
            occurred_at: at.clone(),
        },
        CommandDto::CreateTask {
            title: "task".into(),
            deadline: None,
            priority: PriorityDto::Normal,
            occurred_at: at.clone(),
        },
        CommandDto::CreateNote {
            content: "note".into(),
            occurred_at: at.clone(),
        },
        CommandDto::UpdateEvent {
            event_id: "event".into(),
            expected_revision: 1,
            title: "event".into(),
            schedule: timed,
            occurred_at: at.clone(),
        },
        CommandDto::UpdateTask {
            task_id: "task".into(),
            expected_revision: 1,
            title: "task".into(),
            deadline: Some(at.clone()),
            priority: PriorityDto::High,
            occurred_at: at.clone(),
        },
        CommandDto::UpdateNote {
            note_id: "note".into(),
            expected_revision: 1,
            content: "note".into(),
            occurred_at: at.clone(),
        },
        CommandDto::SetTaskCompletion {
            task_id: "task".into(),
            expected_revision: 1,
            completed: true,
            occurred_at: at.clone(),
        },
        CommandDto::DeleteItem {
            target: DomainRefDto::Note { id: "note".into() },
            expected_revision: 1,
            occurred_at: at,
        },
    ];

    for command in commands {
        let encoded = serde_json::to_string(&command).unwrap();
        assert_eq!(
            serde_json::from_str::<CommandDto>(&encoded).unwrap(),
            command
        );
    }
}
