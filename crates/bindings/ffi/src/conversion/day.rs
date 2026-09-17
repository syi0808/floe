//! Codec between the app wire DTOs and the Day values the app's API carries.
//!
//! Day owns the timeline; the wire owns the DTO. Neither of them owns the
//! translation, so it lives here, at the binding that needs both.

#[cfg(test)]
use floe_app::Revision;
use floe_app::{
    AllDaySchedule, CalendarBatch, CalendarConnection, CalendarFailure, CalendarRange,
    CalendarRecord, CalendarSelection, CalendarSource, CalendarSyncStatus, Capture,
    CaptureProcessing, CaptureSource, Classification, DaySnapshot, DomainError, DomainRef, Event,
    EventId, EventSchedule, Note, NoteId, Priority, SourceRef, Task, TaskId, TimedSchedule,
    TimelineItem,
};
use floe_protocol::conversion::{
    ProtocolConversionError, parse_date, parse_timestamp, parse_uuid, timestamp,
};
use floe_protocol::{
    CalendarBatchDto, CalendarConnectionDto, CalendarFailureDto, CalendarRangeDto,
    CalendarRecordDto, CalendarSelectionDto, CalendarSourceDto, CalendarSyncStatusDto, CaptureDto,
    CaptureProcessingDto, CaptureSourceDto, ClassificationDto, DaySnapshotDto, DomainRefDto,
    EventDto, EventScheduleDto, NoteDto, PROTOCOL_VERSION, PriorityDto, SourceRefDto, TaskDto,
    TimelineItemDto,
};

/// The identity and scope codecs stay with the wire; re-exported so a caller
/// converts through one module.
pub use floe_protocol::conversion::{
    calendar_provider_from_dto, calendar_provider_to_dto, calendar_scope_from_dto,
    calendar_scope_to_dto,
};

fn domain_error(error: DomainError) -> ProtocolConversionError {
    ProtocolConversionError::InvalidField {
        field: "domain",
        message: error.to_string(),
    }
}

macro_rules! parse_id {
    ($function:ident, $type:ident) => {
        fn $function(value: &str, field: &'static str) -> Result<$type, ProtocolConversionError> {
            parse_uuid(value, field).map($type)
        }
    };
}

macro_rules! parse_test_id {
    ($function:ident, $type:path) => {
        #[cfg(test)]
        fn $function(value: &str, field: &'static str) -> Result<$type, ProtocolConversionError> {
            parse_uuid(value, field).map($type)
        }
    };
}

parse_test_id!(parse_person_id, floe_app::PersonId);
parse_test_id!(parse_capture_id, floe_app::CaptureId);
parse_id!(parse_event_id, EventId);
parse_id!(parse_task_id, TaskId);
parse_id!(parse_note_id, NoteId);

pub fn calendar_failure_to_dto(value: CalendarFailure) -> CalendarFailureDto {
    match value {
        CalendarFailure::PermissionDenied => CalendarFailureDto::PermissionDenied,
        CalendarFailure::CalendarUnavailable => CalendarFailureDto::CalendarUnavailable,
        CalendarFailure::ProviderUnavailable => CalendarFailureDto::ProviderUnavailable,
    }
}

pub fn calendar_failure_from_dto(value: CalendarFailureDto) -> CalendarFailure {
    match value {
        CalendarFailureDto::PermissionDenied => CalendarFailure::PermissionDenied,
        CalendarFailureDto::CalendarUnavailable => CalendarFailure::CalendarUnavailable,
        CalendarFailureDto::ProviderUnavailable => CalendarFailure::ProviderUnavailable,
    }
}

pub fn calendar_selection_to_dto(value: CalendarSelection) -> CalendarSelectionDto {
    CalendarSelectionDto {
        calendar_id: value.calendar_id,
        calendar_name: value.calendar_name,
    }
}

pub fn calendar_selection_from_dto(value: CalendarSelectionDto) -> CalendarSelection {
    CalendarSelection {
        calendar_id: value.calendar_id,
        calendar_name: value.calendar_name,
    }
}

pub fn calendar_range_to_dto(value: CalendarRange) -> CalendarRangeDto {
    CalendarRangeDto {
        start_date: value.start_date,
        end_date_exclusive: value.end_date_exclusive,
        timezone_offset_seconds: value.timezone_offset_seconds,
        end_timezone_offset_seconds: value.end_timezone_offset_seconds,
    }
}

pub fn calendar_range_from_dto(value: CalendarRangeDto) -> CalendarRange {
    CalendarRange {
        start_date: value.start_date,
        end_date_exclusive: value.end_date_exclusive,
        timezone_offset_seconds: value.timezone_offset_seconds,
        end_timezone_offset_seconds: value.end_timezone_offset_seconds,
    }
}

pub fn calendar_source_to_dto(value: CalendarSource) -> CalendarSourceDto {
    CalendarSourceDto {
        can_modify: value.can_modify,
        provider: calendar_provider_to_dto(value.provider),
        calendar_id: value.calendar_id,
        calendar_name: value.calendar_name,
        external_id: value.external_id,
        external_revision: value.external_revision,
    }
}

#[cfg(test)]
pub fn calendar_source_from_dto(value: CalendarSourceDto) -> CalendarSource {
    CalendarSource {
        can_modify: value.can_modify,
        provider: calendar_provider_from_dto(value.provider),
        calendar_id: value.calendar_id,
        calendar_name: value.calendar_name,
        external_id: value.external_id,
        external_revision: value.external_revision,
    }
}

pub fn calendar_sync_status_to_dto(value: CalendarSyncStatus) -> CalendarSyncStatusDto {
    CalendarSyncStatusDto {
        last_success_at: value.last_success_at,
        last_range: value.last_range.map(calendar_range_to_dto),
        error: value.error.map(calendar_failure_to_dto),
        error_at: value.error_at,
    }
}

#[cfg(test)]
pub fn calendar_sync_status_from_dto(value: CalendarSyncStatusDto) -> CalendarSyncStatus {
    CalendarSyncStatus {
        last_success_at: value.last_success_at,
        last_range: value.last_range.map(calendar_range_from_dto),
        error: value.error.map(calendar_failure_from_dto),
        error_at: value.error_at,
    }
}

pub fn calendar_connection_to_dto(value: CalendarConnection) -> CalendarConnectionDto {
    CalendarConnectionDto {
        connection_id: value.connection_id,
        device_id: value.device_id,
        disconnected: value.disconnected,
        scope: calendar_scope_to_dto(value.scope),
        provider: calendar_provider_to_dto(value.provider),
        calendars: value
            .calendars
            .into_iter()
            .map(calendar_selection_to_dto)
            .collect(),
        revision: value.revision,
        source_authority: value.source_authority,
        last_success_at: value.last_success_at,
        last_range: value.last_range.map(calendar_range_to_dto),
        error: value.error.map(calendar_failure_to_dto),
        error_at: value.error_at,
        source_statuses: value
            .source_statuses
            .into_iter()
            .map(|(key, value)| (key, calendar_sync_status_to_dto(value)))
            .collect(),
    }
}

#[cfg(test)]
pub fn calendar_connection_from_dto(value: CalendarConnectionDto) -> CalendarConnection {
    CalendarConnection {
        connection_id: value.connection_id,
        device_id: value.device_id,
        disconnected: value.disconnected,
        scope: calendar_scope_from_dto(value.scope),
        provider: calendar_provider_from_dto(value.provider),
        calendars: value
            .calendars
            .into_iter()
            .map(calendar_selection_from_dto)
            .collect(),
        revision: value.revision,
        source_authority: value.source_authority,
        last_success_at: value.last_success_at,
        last_range: value.last_range.map(calendar_range_from_dto),
        error: value.error.map(calendar_failure_from_dto),
        error_at: value.error_at,
        source_statuses: value
            .source_statuses
            .into_iter()
            .map(|(key, value)| (key, calendar_sync_status_from_dto(value)))
            .collect(),
    }
}

pub fn priority_to_dto(value: Priority) -> PriorityDto {
    match value {
        Priority::Low => PriorityDto::Low,
        Priority::Normal => PriorityDto::Normal,
        Priority::High => PriorityDto::High,
    }
}

pub fn priority_from_dto(value: PriorityDto) -> Priority {
    match value {
        PriorityDto::Low => Priority::Low,
        PriorityDto::Normal => Priority::Normal,
        PriorityDto::High => Priority::High,
    }
}

pub fn source_ref_to_dto(value: SourceRef) -> SourceRefDto {
    match value {
        SourceRef::Manual => SourceRefDto::Manual,
        SourceRef::Calendar(source) => SourceRefDto::Calendar {
            source: calendar_source_to_dto(source),
        },
        SourceRef::Capture(id) => SourceRefDto::Capture {
            capture_id: id.to_string(),
        },
    }
}

#[cfg(test)]
pub fn source_ref_from_dto(value: SourceRefDto) -> Result<SourceRef, ProtocolConversionError> {
    match value {
        SourceRefDto::Manual => Ok(SourceRef::Manual),
        SourceRefDto::Calendar { source } => {
            Ok(SourceRef::Calendar(calendar_source_from_dto(source)))
        }
        SourceRefDto::Capture { capture_id } => Ok(SourceRef::Capture(parse_capture_id(
            &capture_id,
            "capture_id",
        )?)),
    }
}

pub fn domain_ref_to_dto(value: DomainRef) -> DomainRefDto {
    match value {
        DomainRef::Event(id) => DomainRefDto::Event { id: id.to_string() },
        DomainRef::Task(id) => DomainRefDto::Task { id: id.to_string() },
        DomainRef::Note(id) => DomainRefDto::Note { id: id.to_string() },
    }
}

pub fn domain_ref_from_dto(value: DomainRefDto) -> Result<DomainRef, ProtocolConversionError> {
    match value {
        DomainRefDto::Event { id } => Ok(DomainRef::Event(parse_event_id(&id, "id")?)),
        DomainRefDto::Task { id } => Ok(DomainRef::Task(parse_task_id(&id, "id")?)),
        DomainRefDto::Note { id } => Ok(DomainRef::Note(parse_note_id(&id, "id")?)),
    }
}

pub fn event_schedule_to_dto(value: EventSchedule) -> EventScheduleDto {
    match value {
        EventSchedule::Timed(value) => EventScheduleDto::Timed {
            starts_at: timestamp(value.starts_at),
            ends_at: timestamp(value.ends_at),
            timezone: value.timezone,
        },
        EventSchedule::AllDay(value) => EventScheduleDto::AllDay {
            start_date: value.start_date.to_string(),
            end_date_exclusive: value.end_date_exclusive.to_string(),
        },
    }
}

pub fn event_schedule_from_dto(
    value: EventScheduleDto,
) -> Result<EventSchedule, ProtocolConversionError> {
    match value {
        EventScheduleDto::Timed {
            starts_at,
            ends_at,
            timezone,
        } => Ok(EventSchedule::Timed(
            TimedSchedule::new(
                parse_timestamp(&starts_at, "starts_at")?,
                parse_timestamp(&ends_at, "ends_at")?,
                timezone,
            )
            .map_err(domain_error)?,
        )),
        EventScheduleDto::AllDay {
            start_date,
            end_date_exclusive,
        } => Ok(EventSchedule::AllDay(
            AllDaySchedule::new(
                parse_date(&start_date, "start_date")?,
                parse_date(&end_date_exclusive, "end_date_exclusive")?,
            )
            .map_err(domain_error)?,
        )),
    }
}

pub fn event_to_dto(value: Event) -> EventDto {
    EventDto {
        id: value.id.to_string(),
        person_id: value.person_id.to_string(),
        title: value.title,
        schedule: event_schedule_to_dto(value.schedule),
        source: source_ref_to_dto(value.source),
        created_at: timestamp(value.created_at),
        updated_at: timestamp(value.updated_at),
        revision: value.revision.0,
        deleted_at: value.deleted_at.map(timestamp),
    }
}

#[cfg(test)]
pub fn event_from_dto(value: EventDto) -> Result<Event, ProtocolConversionError> {
    let person_id = parse_person_id(&value.person_id, "person_id")?;
    let schedule = event_schedule_from_dto(value.schedule)?;
    let source = source_ref_from_dto(value.source)?;
    let created_at = parse_timestamp(&value.created_at, "created_at")?;
    let mut event =
        Event::new(person_id, value.title, schedule, source, created_at).map_err(domain_error)?;
    event.id = parse_event_id(&value.id, "id")?;
    event.updated_at = parse_timestamp(&value.updated_at, "updated_at")?;
    event.revision = Revision(value.revision);
    event.deleted_at = value
        .deleted_at
        .as_deref()
        .map(|value| parse_timestamp(value, "deleted_at"))
        .transpose()?;
    Ok(event)
}

pub fn task_to_dto(value: Task) -> TaskDto {
    TaskDto {
        id: value.id.to_string(),
        person_id: value.person_id.to_string(),
        title: value.title,
        deadline: value.deadline.map(timestamp),
        priority: priority_to_dto(value.priority),
        completed_at: value.completed_at.map(timestamp),
        source: source_ref_to_dto(value.source),
        created_at: timestamp(value.created_at),
        updated_at: timestamp(value.updated_at),
        revision: value.revision.0,
        deleted_at: value.deleted_at.map(timestamp),
    }
}

#[cfg(test)]
pub fn task_from_dto(value: TaskDto) -> Result<Task, ProtocolConversionError> {
    let person_id = parse_person_id(&value.person_id, "person_id")?;
    let deadline = value
        .deadline
        .as_deref()
        .map(|value| parse_timestamp(value, "deadline"))
        .transpose()?;
    let source = source_ref_from_dto(value.source)?;
    let created_at = parse_timestamp(&value.created_at, "created_at")?;
    let mut task = Task::new(
        person_id,
        value.title,
        deadline,
        priority_from_dto(value.priority),
        source,
        created_at,
    )
    .map_err(domain_error)?;
    task.id = parse_task_id(&value.id, "id")?;
    task.completed_at = value
        .completed_at
        .as_deref()
        .map(|value| parse_timestamp(value, "completed_at"))
        .transpose()?;
    task.updated_at = parse_timestamp(&value.updated_at, "updated_at")?;
    task.revision = Revision(value.revision);
    task.deleted_at = value
        .deleted_at
        .as_deref()
        .map(|value| parse_timestamp(value, "deleted_at"))
        .transpose()?;
    Ok(task)
}

pub fn note_to_dto(value: Note) -> NoteDto {
    NoteDto {
        id: value.id.to_string(),
        person_id: value.person_id.to_string(),
        content: value.content,
        source: source_ref_to_dto(value.source),
        created_at: timestamp(value.created_at),
        updated_at: timestamp(value.updated_at),
        revision: value.revision.0,
        deleted_at: value.deleted_at.map(timestamp),
    }
}

#[cfg(test)]
pub fn note_from_dto(value: NoteDto) -> Result<Note, ProtocolConversionError> {
    let person_id = parse_person_id(&value.person_id, "person_id")?;
    let source = source_ref_from_dto(value.source)?;
    let created_at = parse_timestamp(&value.created_at, "created_at")?;
    let mut note = Note::new(person_id, value.content, source, created_at).map_err(domain_error)?;
    note.id = parse_note_id(&value.id, "id")?;
    note.updated_at = parse_timestamp(&value.updated_at, "updated_at")?;
    note.revision = Revision(value.revision);
    note.deleted_at = value
        .deleted_at
        .as_deref()
        .map(|value| parse_timestamp(value, "deleted_at"))
        .transpose()?;
    Ok(note)
}

/// The classification a caller chose for one capture.
pub fn classification_from_dto(
    value: ClassificationDto,
) -> Result<Classification, ProtocolConversionError> {
    Ok(match value {
        ClassificationDto::Event { title, schedule } => Classification::Event {
            title,
            schedule: event_schedule_from_dto(schedule)?,
        },
        ClassificationDto::Task {
            title,
            deadline,
            priority,
        } => Classification::Task {
            title,
            deadline: deadline
                .as_deref()
                .map(|value| parse_timestamp(value, "deadline"))
                .transpose()?,
            priority: priority_from_dto(priority),
        },
        ClassificationDto::Note { content } => Classification::Note { content },
    })
}

/// One source record as the calendar mirror stores it.
pub fn calendar_record_from_dto(
    value: CalendarRecordDto,
) -> Result<CalendarRecord, ProtocolConversionError> {
    Ok(CalendarRecord {
        can_modify: value.can_modify,
        calendar_id: value.calendar_id,
        external_id: value.external_id,
        external_revision: value.external_revision,
        title: value.title,
        schedule: event_schedule_from_dto(value.schedule)?,
    })
}

/// One source batch. A batch whose records do not convert is reported as a
/// provider failure rather than a partially imported calendar.
pub fn calendar_batch_from_dto(value: CalendarBatchDto) -> CalendarBatch {
    let calendar_id = value.calendar_id;
    let failure = value.failure.map(calendar_failure_from_dto);
    match value
        .records
        .into_iter()
        .map(calendar_record_from_dto)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(records) => CalendarBatch {
            calendar_id,
            records,
            failure,
        },
        Err(_) => CalendarBatch {
            calendar_id,
            records: vec![],
            failure: Some(CalendarFailure::ProviderUnavailable),
        },
    }
}

pub fn timeline_item_to_dto(value: TimelineItem) -> TimelineItemDto {
    match value {
        TimelineItem::Event(value) => TimelineItemDto::Event(event_to_dto(value)),
        TimelineItem::Task(value) => TimelineItemDto::Task(task_to_dto(value)),
        TimelineItem::Note(value) => TimelineItemDto::Note(note_to_dto(value)),
    }
}

#[cfg(test)]
pub fn timeline_item_from_dto(
    value: TimelineItemDto,
) -> Result<TimelineItem, ProtocolConversionError> {
    match value {
        TimelineItemDto::Event(value) => Ok(TimelineItem::Event(event_from_dto(value)?)),
        TimelineItemDto::Task(value) => Ok(TimelineItem::Task(task_from_dto(value)?)),
        TimelineItemDto::Note(value) => Ok(TimelineItem::Note(note_from_dto(value)?)),
    }
}

pub fn capture_source_to_dto(value: CaptureSource) -> CaptureSourceDto {
    match value {
        CaptureSource::Typed => CaptureSourceDto::Typed,
        CaptureSource::Voice => CaptureSourceDto::Voice,
    }
}

#[cfg(test)]
pub fn capture_source_from_dto(value: CaptureSourceDto) -> CaptureSource {
    match value {
        CaptureSourceDto::Typed => CaptureSource::Typed,
        CaptureSourceDto::Voice => CaptureSource::Voice,
    }
}

pub fn capture_processing_to_dto(value: CaptureProcessing) -> CaptureProcessingDto {
    match value {
        CaptureProcessing::Pending => CaptureProcessingDto::Pending,
        CaptureProcessing::Classified {
            target,
            classified_at,
        } => CaptureProcessingDto::Classified {
            target: domain_ref_to_dto(target),
            classified_at: timestamp(classified_at),
        },
        CaptureProcessing::Dismissed { dismissed_at } => CaptureProcessingDto::Dismissed {
            dismissed_at: timestamp(dismissed_at),
        },
    }
}

#[cfg(test)]
pub fn capture_processing_from_dto(
    value: CaptureProcessingDto,
) -> Result<CaptureProcessing, ProtocolConversionError> {
    match value {
        CaptureProcessingDto::Pending => Ok(CaptureProcessing::Pending),
        CaptureProcessingDto::Classified {
            target,
            classified_at,
        } => Ok(CaptureProcessing::Classified {
            target: domain_ref_from_dto(target)?,
            classified_at: parse_timestamp(&classified_at, "classified_at")?,
        }),
        CaptureProcessingDto::Dismissed { dismissed_at } => Ok(CaptureProcessing::Dismissed {
            dismissed_at: parse_timestamp(&dismissed_at, "dismissed_at")?,
        }),
    }
}

pub fn capture_to_dto(value: Capture) -> CaptureDto {
    CaptureDto {
        id: value.id.to_string(),
        person_id: value.person_id.to_string(),
        original_input: value.original_input,
        captured_at: timestamp(value.captured_at),
        source: capture_source_to_dto(value.source),
        processing: capture_processing_to_dto(value.processing),
        revision: value.revision.0,
    }
}

#[cfg(test)]
pub fn capture_from_dto(value: CaptureDto) -> Result<Capture, ProtocolConversionError> {
    let person_id = parse_person_id(&value.person_id, "person_id")?;
    let captured_at = parse_timestamp(&value.captured_at, "captured_at")?;
    let mut capture = Capture::new(
        person_id,
        value.original_input,
        captured_at,
        capture_source_from_dto(value.source),
    )
    .map_err(domain_error)?;
    capture.id = parse_capture_id(&value.id, "id")?;
    capture.processing = capture_processing_from_dto(value.processing)?;
    capture.revision = Revision(value.revision);
    Ok(capture)
}

pub fn day_snapshot_to_dto(value: DaySnapshot) -> Result<DaySnapshotDto, ProtocolConversionError> {
    Ok(DaySnapshotDto {
        schema_version: PROTOCOL_VERSION,
        person_id: value.person_id.to_string(),
        date: value.date.to_string(),
        generated_at: timestamp(value.generated_at),
        timezone_offset_seconds: value.timezone_offset_seconds,
        now_event_id: value.now_event_id.map(|id| id.to_string()),
        next_event_id: value.next_event_id.map(|id| id.to_string()),
        overdue_task_count: value.overdue_task_count.try_into().map_err(|_| {
            ProtocolConversionError::OutOfRange {
                field: "overdue_task_count",
            }
        })?,
        items: value.items.into_iter().map(timeline_item_to_dto).collect(),
        calendar: value.calendar.map(calendar_connection_to_dto),
    })
}

#[cfg(test)]
pub fn day_snapshot_from_dto(
    value: DaySnapshotDto,
) -> Result<DaySnapshot, ProtocolConversionError> {
    if value.schema_version != PROTOCOL_VERSION {
        return Err(ProtocolConversionError::UnsupportedVersion {
            actual: value.schema_version,
            expected: PROTOCOL_VERSION,
        });
    }
    Ok(DaySnapshot {
        person_id: parse_person_id(&value.person_id, "person_id")?,
        date: parse_date(&value.date, "date")?,
        calendar: value.calendar.map(calendar_connection_from_dto),
        generated_at: parse_timestamp(&value.generated_at, "generated_at")?,
        timezone_offset_seconds: value.timezone_offset_seconds,
        now_event_id: value
            .now_event_id
            .as_deref()
            .map(|value| parse_event_id(value, "now_event_id"))
            .transpose()?,
        next_event_id: value
            .next_event_id
            .as_deref()
            .map(|value| parse_event_id(value, "next_event_id"))
            .transpose()?,
        overdue_task_count: value.overdue_task_count as usize,
        items: value
            .items
            .into_iter()
            .map(timeline_item_from_dto)
            .collect::<Result<_, _>>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use chrono::{TimeZone, Utc};
    use floe_app::{
        CalendarConnection, CalendarFailure, CalendarProvider, CalendarRange, CalendarScope,
        CalendarSelection, CalendarSyncStatus, Capture, CaptureId, CaptureSource, DaySnapshot,
        DomainRef, Event, EventId, EventSchedule, Note, PersonId, Priority, SourceRef, Task,
        TimedSchedule, TimelineItem,
    };
    use floe_protocol::{CalendarProviderDto, DaySnapshotDto, EventScheduleDto, SourceRefDto};
    use uuid::Uuid;

    fn id(value: &str) -> Uuid {
        Uuid::parse_str(value).unwrap()
    }

    #[test]
    fn domain_snapshot_round_trip_preserves_all_item_kinds() {
        let person_id = PersonId(id("00000000-0000-0000-0000-000000000001"));
        let capture_id = CaptureId(id("00000000-0000-0000-0000-000000000004"));
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

        let dto = day_snapshot_to_dto(snapshot.clone()).unwrap();
        assert_eq!(day_snapshot_from_dto(dto).unwrap(), snapshot);
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

        let dto = capture_to_dto(capture.clone());
        assert_eq!(dto.revision, 1);
        assert_eq!(capture_from_dto(dto).unwrap(), capture);
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
            day_snapshot_from_dto(snapshot),
            Err(ProtocolConversionError::UnsupportedVersion { actual: 99, .. })
        ));

        let schedule = EventScheduleDto::Timed {
            starts_at: "2026-09-02T10:30:00Z".into(),
            ends_at: "2026-09-02T10:30:00Z".into(),
            timezone: "UTC".into(),
        };
        assert!(event_schedule_from_dto(schedule).is_err());

        let invalid_id = SourceRefDto::Capture {
            capture_id: "not-a-uuid".into(),
        };
        assert!(source_ref_from_dto(invalid_id).is_err());
    }

    #[test]
    fn calendar_connection_conversion_preserves_wire_json_shape() {
        let now = Utc.with_ymd_and_hms(2026, 9, 2, 10, 30, 0).unwrap();
        let range = CalendarRange {
            start_date: now.date_naive(),
            end_date_exclusive: now.date_naive() + chrono::Duration::days(1),
            timezone_offset_seconds: 32_400,
            end_timezone_offset_seconds: Some(28_800),
        };
        let connection = CalendarConnection {
            connection_id: "connection-1".into(),
            device_id: "mac-local".into(),
            disconnected: false,
            scope: CalendarScope::Selected,
            provider: CalendarProvider::Google,
            calendars: vec![CalendarSelection {
                calendar_id: "primary".into(),
                calendar_name: "Primary".into(),
            }],
            revision: 7,
            source_authority: floe_app::SourceAuthority::new(),
            last_success_at: Some(now),
            last_range: Some(range.clone()),
            error: Some(CalendarFailure::ProviderUnavailable),
            error_at: Some(now),
            source_statuses: [(
                "primary".into(),
                CalendarSyncStatus {
                    last_success_at: Some(now),
                    last_range: Some(range),
                    error: None,
                    error_at: None,
                },
            )]
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
        };
        let dto = calendar_connection_to_dto(connection.clone());
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            serde_json::to_value(&connection).unwrap()
        );
        assert_eq!(dto.provider, CalendarProviderDto::Google);
        assert_eq!(
            serde_json::to_value(CalendarProviderDto::Google).unwrap(),
            serde_json::json!("google_calendar")
        );
        assert_eq!(
            serde_json::to_value(CalendarProviderDto::Microsoft).unwrap(),
            serde_json::json!("microsoft_calendar")
        );
        assert_eq!(calendar_connection_from_dto(dto), connection);
    }
}
