//! Codec between the app wire DTOs and the Day values the app's API carries.
//!
//! Day owns the timeline; the wire owns the DTO. Neither of them owns the
//! translation, so it lives here, at the binding that needs both.

use floe_app::{
    AllDaySchedule, CalendarBatch, CalendarFailure, CalendarMirrorState, CalendarRange,
    CalendarRecord, CalendarSelection, CalendarSource, CalendarSyncStatus, Capture,
    CaptureProcessing, CaptureSource, Classification, DaySnapshot, DomainError, DomainRef, Event,
    EventId, EventSchedule, Note, NoteId, Priority, SourceRef, Task, TaskId, TimedSchedule,
    TimelineItem,
};
use floe_protocol::conversion::{
    ProtocolConversionError, parse_date, parse_timestamp, parse_uuid, timestamp,
};
use floe_protocol::{
    CalendarBatchDto, CalendarFailureDto, CalendarMirrorStateDto, CalendarRangeDto,
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

pub fn calendar_sync_status_to_dto(value: CalendarSyncStatus) -> CalendarSyncStatusDto {
    CalendarSyncStatusDto {
        last_success_at: value.last_success_at,
        last_range: value.last_range.map(calendar_range_to_dto),
        error: value.error.map(calendar_failure_to_dto),
        error_at: value.error_at,
    }
}

pub fn calendar_mirror_state_to_dto(value: CalendarMirrorState) -> CalendarMirrorStateDto {
    CalendarMirrorStateDto {
        source_connection_id: value.source_connection_id,
        provider: calendar_provider_to_dto(value.provider),
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

pub fn capture_source_to_dto(value: CaptureSource) -> CaptureSourceDto {
    match value {
        CaptureSource::Typed => CaptureSourceDto::Typed,
        CaptureSource::Voice => CaptureSourceDto::Voice,
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
        calendar: value.calendar.map(calendar_mirror_state_to_dto),
        calendar_mirror_revision: value.calendar_mirror_revision,
    })
}
