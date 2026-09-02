use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use floe_domain::{
    Capture, CaptureSource, DaySnapshot, DomainRef, Event, EventId, EventSchedule, Note, PersonId,
    Priority, SourceRef, Task, TimedSchedule, TimelineItem,
};
use floe_protocol::*;
use serde_json::json;
use uuid::Uuid;

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap()
}

#[test]
fn snapshot_has_a_versioned_stable_wire_shape() {
    let snapshot = DaySnapshotDto {
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
