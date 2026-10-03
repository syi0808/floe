//! Mechanical codecs for Day-owned safe display values and manual commands.
use floe_day::{AllDaySchedule, CalendarFailure, CalendarRange, Capture, CaptureProcessing, CaptureSource, Classification, DaySnapshot, DayEvent, DayTask, DayNote, DayItemSource, DayTimelineItem, DayCalendarCoverage, DayCalendarSourceCoverage, DayCalendarResourceCoverage, DayCoverageState, DomainError, DomainRef, EventId, EventSchedule, NoteId, Priority, TaskId, TimedSchedule};
use floe_protocol::conversion::{ProtocolConversionError, parse_date, parse_timestamp, parse_uuid, timestamp};
use floe_protocol::{CalendarRangeDto, CaptureDto, CaptureProcessingDto, CaptureSourceDto, ClassificationDto, DaySnapshotDto, DayEventTargetDto, DayCalendarCoverageDto, DayCalendarSourceCoverageDto, DayCalendarResourceCoverageDto, DayCoverageStateDto, DayCalendarFailureDto, DomainRefDto, EventDto, EventScheduleDto, NoteDto, PROTOCOL_VERSION, PriorityDto, SourceRefDto, TaskDto, TimelineItemDto, UuidRefDto, TaskRefDto};

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

fn uuid_ref(value: uuid::Uuid, field: &'static str) -> Result<UuidRefDto, ProtocolConversionError> { UuidRefDto::new(value).ok_or(ProtocolConversionError::OutOfRange { field }) }
fn wire_error(field: &'static str) -> ProtocolConversionError { ProtocolConversionError::InvalidField { field, message: "invalid owner projection".into() } }

pub fn calendar_range_to_dto(value: CalendarRange) -> CalendarRangeDto {
    CalendarRangeDto {
        start_date: value.start_date,
        end_date_exclusive: value.end_date_exclusive,
        timezone_offset_seconds: value.timezone_offset_seconds,
        end_timezone_offset_seconds: value.end_timezone_offset_seconds,
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

pub fn source_ref_to_dto(value: DayItemSource) -> Result<SourceRefDto, ProtocolConversionError> {
    Ok(match value { DayItemSource::Manual => SourceRefDto::Manual, DayItemSource::Capture { capture_id } => SourceRefDto::Capture { capture_id: uuid_ref(capture_id.0, "capture_id")? }, DayItemSource::Calendar { source_ref, calendar_ref, calendar_label } => SourceRefDto::Calendar { source_ref: uuid_ref(source_ref, "source_ref")?, calendar_ref: uuid_ref(calendar_ref, "calendar_ref")?, calendar_label } })
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

pub fn event_to_dto(value: DayEvent) -> Result<EventDto, ProtocolConversionError> {
    let result = EventDto { id: uuid_ref(value.id.0, "event.id")?, person_id: uuid_ref(value.person_id.0, "person_id")?, title: value.title, schedule: event_schedule_to_dto(value.schedule), source: source_ref_to_dto(value.source)?, created_at: timestamp(value.created_at), updated_at: timestamp(value.updated_at), revision: value.revision.0, deleted_at: value.deleted_at.map(timestamp), action_target: value.action_target.map(|target| Ok::<_, ProtocolConversionError>(DayEventTargetDto { event_id: uuid_ref(target.event_id.0, "event_id")?, expected_revision: target.expected_revision.0 })).transpose()? };
    result.validate().map_err(wire_error)?; Ok(result)
}
pub fn task_to_dto(value: DayTask) -> Result<TaskDto, ProtocolConversionError> {
    let result = TaskDto { id: TaskRefDto::new(value.id.0).ok_or(ProtocolConversionError::OutOfRange { field: "task.id" })?, person_id: uuid_ref(value.person_id.0, "person_id")?, title: value.title, deadline: value.deadline.map(timestamp), priority: priority_to_dto(value.priority), completed_at: value.completed_at.map(timestamp), source: source_ref_to_dto(value.source)?, created_at: timestamp(value.created_at), updated_at: timestamp(value.updated_at), revision: value.revision.0, deleted_at: value.deleted_at.map(timestamp) };
    result.validate().map_err(wire_error)?; Ok(result)
}
pub fn note_to_dto(value: DayNote) -> Result<NoteDto, ProtocolConversionError> {
    let result = NoteDto { id: uuid_ref(value.id.0, "note.id")?, person_id: uuid_ref(value.person_id.0, "person_id")?, content: value.content, source: source_ref_to_dto(value.source)?, created_at: timestamp(value.created_at), updated_at: timestamp(value.updated_at), revision: value.revision.0, deleted_at: value.deleted_at.map(timestamp) };
    result.validate().map_err(wire_error)?; Ok(result)
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

pub fn timeline_item_to_dto(value: DayTimelineItem) -> Result<TimelineItemDto, ProtocolConversionError> {
    Ok(match value { DayTimelineItem::Event(value) => TimelineItemDto::Event(event_to_dto(value)?), DayTimelineItem::Task(value) => TimelineItemDto::Task(task_to_dto(value)?), DayTimelineItem::Note(value) => TimelineItemDto::Note(note_to_dto(value)?) })
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

fn coverage_state(value: DayCoverageState) -> DayCoverageStateDto { match value { DayCoverageState::Current => DayCoverageStateDto::Current, DayCoverageState::Stale => DayCoverageStateDto::Stale, DayCoverageState::Partial => DayCoverageStateDto::Partial, DayCoverageState::Unavailable => DayCoverageStateDto::Unavailable, DayCoverageState::Pending => DayCoverageStateDto::Pending } }
fn calendar_failure(value: CalendarFailure) -> DayCalendarFailureDto { match value { CalendarFailure::PermissionDenied => DayCalendarFailureDto::PermissionDenied, CalendarFailure::CalendarUnavailable => DayCalendarFailureDto::CalendarUnavailable, CalendarFailure::ProviderUnavailable => DayCalendarFailureDto::ProviderUnavailable, CalendarFailure::SourceChanged => DayCalendarFailureDto::SourceChanged, CalendarFailure::SourceFenced => DayCalendarFailureDto::SourceFenced, CalendarFailure::VaultLocked => DayCalendarFailureDto::VaultLocked, CalendarFailure::BudgetExceeded => DayCalendarFailureDto::BudgetExceeded, CalendarFailure::DeadlineExceeded => DayCalendarFailureDto::DeadlineExceeded, CalendarFailure::Cancelled => DayCalendarFailureDto::Cancelled } }
fn resource_coverage(value: DayCalendarResourceCoverage) -> Result<DayCalendarResourceCoverageDto, ProtocolConversionError> { Ok(DayCalendarResourceCoverageDto { resource_ref: uuid_ref(value.resource_ref, "resource_ref")?, label: value.label, state: coverage_state(value.state), last_success_at: value.last_success_at.map(timestamp), last_range: value.last_range.map(calendar_range_to_dto), failure: value.failure.map(calendar_failure), failure_at: value.failure_at.map(timestamp) }) }
fn source_coverage(value: DayCalendarSourceCoverage) -> Result<DayCalendarSourceCoverageDto, ProtocolConversionError> { Ok(DayCalendarSourceCoverageDto { source_ref: uuid_ref(value.source_ref, "source_ref")?, label: value.label, state: coverage_state(value.state), last_success_at: value.last_success_at.map(timestamp), last_range: value.last_range.map(calendar_range_to_dto), failure: value.failure.map(calendar_failure), failure_at: value.failure_at.map(timestamp), resources: value.resources.into_iter().map(resource_coverage).collect::<Result<_, _>>()? }) }
fn calendar_coverage(value: DayCalendarCoverage) -> Result<DayCalendarCoverageDto, ProtocolConversionError> { Ok(DayCalendarCoverageDto { sources: value.sources.into_iter().map(source_coverage).collect::<Result<_, _>>()? }) }
pub fn day_snapshot_to_dto(value: DaySnapshot) -> Result<DaySnapshotDto, ProtocolConversionError> {
    value.validate_bounds().map_err(|_| wire_error("day.snapshot"))?;
    let result = DaySnapshotDto {
        schema_version: PROTOCOL_VERSION, person_id: uuid_ref(value.person_id.0, "person_id")?, date: value.date.to_string(), generated_at: timestamp(value.generated_at), timezone_offset_seconds: value.timezone_offset_seconds,
        now_event_id: value.now_event_id.map(|id| uuid_ref(id.0, "now_event_id")).transpose()?, next_event_id: value.next_event_id.map(|id| uuid_ref(id.0, "next_event_id")).transpose()?, overdue_task_count: value.overdue_task_count.try_into().map_err(|_| ProtocolConversionError::OutOfRange { field: "overdue_task_count" })?,
        items: value.items.into_iter().map(timeline_item_to_dto).collect::<Result<_, _>>()?, calendar: value.calendar.map(calendar_coverage).transpose()?, calendar_mirror_revision: value.calendar_mirror_revision,
    };
    result.validate().map_err(wire_error)?; Ok(result)
}
