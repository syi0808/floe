use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use floe_domain::{
    AllDaySchedule, Capture, CaptureId, CaptureProcessing, CaptureSource, DaySnapshot, DomainError,
    DomainRef, Event, EventId, EventSchedule, Note, NoteId, PersonId, Priority, Revision,
    SourceRef, Task, TaskId, TimedSchedule, TimelineItem,
};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    CaptureDto, CaptureProcessingDto, CaptureSourceDto, DaySnapshotDto, DomainRefDto, EventDto,
    EventScheduleDto, NoteDto, PROTOCOL_VERSION, PriorityDto, SourceRefDto, TaskDto,
    TimelineItemDto,
};

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProtocolConversionError {
    #[error("invalid {field}: {message}")]
    InvalidField {
        field: &'static str,
        message: String,
    },
    #[error("unsupported schema version {actual}; expected {expected}")]
    UnsupportedVersion { actual: u32, expected: u32 },
    #[error("{field} exceeds the protocol range")]
    OutOfRange { field: &'static str },
}

impl From<DomainError> for ProtocolConversionError {
    fn from(error: DomainError) -> Self {
        Self::InvalidField {
            field: "domain",
            message: error.to_string(),
        }
    }
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

fn parse_timestamp(
    value: &str,
    field: &'static str,
) -> Result<DateTime<Utc>, ProtocolConversionError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| ProtocolConversionError::InvalidField {
            field,
            message: error.to_string(),
        })
}

fn parse_date(value: &str, field: &'static str) -> Result<NaiveDate, ProtocolConversionError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|error| {
        ProtocolConversionError::InvalidField {
            field,
            message: error.to_string(),
        }
    })
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, ProtocolConversionError> {
    Uuid::parse_str(value).map_err(|error| ProtocolConversionError::InvalidField {
        field,
        message: error.to_string(),
    })
}

macro_rules! parse_id {
    ($function:ident, $type:ident) => {
        fn $function(value: &str, field: &'static str) -> Result<$type, ProtocolConversionError> {
            parse_uuid(value, field).map($type)
        }
    };
}

parse_id!(parse_person_id, PersonId);
parse_id!(parse_capture_id, CaptureId);
parse_id!(parse_event_id, EventId);
parse_id!(parse_task_id, TaskId);
parse_id!(parse_note_id, NoteId);

impl From<Priority> for PriorityDto {
    fn from(value: Priority) -> Self {
        match value {
            Priority::Low => Self::Low,
            Priority::Normal => Self::Normal,
            Priority::High => Self::High,
        }
    }
}

impl From<PriorityDto> for Priority {
    fn from(value: PriorityDto) -> Self {
        match value {
            PriorityDto::Low => Self::Low,
            PriorityDto::Normal => Self::Normal,
            PriorityDto::High => Self::High,
        }
    }
}

impl From<SourceRef> for SourceRefDto {
    fn from(value: SourceRef) -> Self {
        match value {
            SourceRef::Manual => Self::Manual,
            SourceRef::Calendar(source) => Self::Calendar { source },
            SourceRef::Capture(id) => Self::Capture {
                capture_id: id.to_string(),
            },
        }
    }
}

impl TryFrom<SourceRefDto> for SourceRef {
    type Error = ProtocolConversionError;

    fn try_from(value: SourceRefDto) -> Result<Self, Self::Error> {
        match value {
            SourceRefDto::Manual => Ok(Self::Manual),
            SourceRefDto::Calendar { source } => Ok(Self::Calendar(source)),
            SourceRefDto::Capture { capture_id } => {
                Ok(Self::Capture(parse_capture_id(&capture_id, "capture_id")?))
            }
        }
    }
}

impl From<DomainRef> for DomainRefDto {
    fn from(value: DomainRef) -> Self {
        match value {
            DomainRef::Event(id) => Self::Event { id: id.to_string() },
            DomainRef::Task(id) => Self::Task { id: id.to_string() },
            DomainRef::Note(id) => Self::Note { id: id.to_string() },
        }
    }
}

impl TryFrom<DomainRefDto> for DomainRef {
    type Error = ProtocolConversionError;

    fn try_from(value: DomainRefDto) -> Result<Self, Self::Error> {
        match value {
            DomainRefDto::Event { id } => Ok(Self::Event(parse_event_id(&id, "id")?)),
            DomainRefDto::Task { id } => Ok(Self::Task(parse_task_id(&id, "id")?)),
            DomainRefDto::Note { id } => Ok(Self::Note(parse_note_id(&id, "id")?)),
        }
    }
}

impl From<EventSchedule> for EventScheduleDto {
    fn from(value: EventSchedule) -> Self {
        match value {
            EventSchedule::Timed(value) => Self::Timed {
                starts_at: timestamp(value.starts_at),
                ends_at: timestamp(value.ends_at),
                timezone: value.timezone,
            },
            EventSchedule::AllDay(value) => Self::AllDay {
                start_date: value.start_date.to_string(),
                end_date_exclusive: value.end_date_exclusive.to_string(),
            },
        }
    }
}

impl TryFrom<EventScheduleDto> for EventSchedule {
    type Error = ProtocolConversionError;

    fn try_from(value: EventScheduleDto) -> Result<Self, Self::Error> {
        match value {
            EventScheduleDto::Timed {
                starts_at,
                ends_at,
                timezone,
            } => Ok(Self::Timed(TimedSchedule::new(
                parse_timestamp(&starts_at, "starts_at")?,
                parse_timestamp(&ends_at, "ends_at")?,
                timezone,
            )?)),
            EventScheduleDto::AllDay {
                start_date,
                end_date_exclusive,
            } => Ok(Self::AllDay(AllDaySchedule::new(
                parse_date(&start_date, "start_date")?,
                parse_date(&end_date_exclusive, "end_date_exclusive")?,
            )?)),
        }
    }
}

impl From<Event> for EventDto {
    fn from(value: Event) -> Self {
        Self {
            id: value.id.to_string(),
            person_id: value.person_id.to_string(),
            title: value.title,
            schedule: value.schedule.into(),
            source: value.source.into(),
            created_at: timestamp(value.created_at),
            updated_at: timestamp(value.updated_at),
            revision: value.revision.0,
            deleted_at: value.deleted_at.map(timestamp),
        }
    }
}

impl TryFrom<EventDto> for Event {
    type Error = ProtocolConversionError;

    fn try_from(value: EventDto) -> Result<Self, Self::Error> {
        let person_id = parse_person_id(&value.person_id, "person_id")?;
        let schedule = value.schedule.try_into()?;
        let source = value.source.try_into()?;
        let created_at = parse_timestamp(&value.created_at, "created_at")?;
        let mut event = Self::new(person_id, value.title, schedule, source, created_at)?;
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
}

impl From<Task> for TaskDto {
    fn from(value: Task) -> Self {
        Self {
            id: value.id.to_string(),
            person_id: value.person_id.to_string(),
            title: value.title,
            deadline: value.deadline.map(timestamp),
            priority: value.priority.into(),
            completed_at: value.completed_at.map(timestamp),
            source: value.source.into(),
            created_at: timestamp(value.created_at),
            updated_at: timestamp(value.updated_at),
            revision: value.revision.0,
            deleted_at: value.deleted_at.map(timestamp),
        }
    }
}

impl TryFrom<TaskDto> for Task {
    type Error = ProtocolConversionError;

    fn try_from(value: TaskDto) -> Result<Self, Self::Error> {
        let person_id = parse_person_id(&value.person_id, "person_id")?;
        let deadline = value
            .deadline
            .as_deref()
            .map(|value| parse_timestamp(value, "deadline"))
            .transpose()?;
        let source = value.source.try_into()?;
        let created_at = parse_timestamp(&value.created_at, "created_at")?;
        let mut task = Self::new(
            person_id,
            value.title,
            deadline,
            value.priority.into(),
            source,
            created_at,
        )?;
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
}

impl From<Note> for NoteDto {
    fn from(value: Note) -> Self {
        Self {
            id: value.id.to_string(),
            person_id: value.person_id.to_string(),
            content: value.content,
            source: value.source.into(),
            created_at: timestamp(value.created_at),
            updated_at: timestamp(value.updated_at),
            revision: value.revision.0,
            deleted_at: value.deleted_at.map(timestamp),
        }
    }
}

impl TryFrom<NoteDto> for Note {
    type Error = ProtocolConversionError;

    fn try_from(value: NoteDto) -> Result<Self, Self::Error> {
        let person_id = parse_person_id(&value.person_id, "person_id")?;
        let source = value.source.try_into()?;
        let created_at = parse_timestamp(&value.created_at, "created_at")?;
        let mut note = Self::new(person_id, value.content, source, created_at)?;
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
}

impl From<TimelineItem> for TimelineItemDto {
    fn from(value: TimelineItem) -> Self {
        match value {
            TimelineItem::Event(value) => Self::Event(value.into()),
            TimelineItem::Task(value) => Self::Task(value.into()),
            TimelineItem::Note(value) => Self::Note(value.into()),
        }
    }
}

impl TryFrom<TimelineItemDto> for TimelineItem {
    type Error = ProtocolConversionError;

    fn try_from(value: TimelineItemDto) -> Result<Self, Self::Error> {
        match value {
            TimelineItemDto::Event(value) => Ok(Self::Event(value.try_into()?)),
            TimelineItemDto::Task(value) => Ok(Self::Task(value.try_into()?)),
            TimelineItemDto::Note(value) => Ok(Self::Note(value.try_into()?)),
        }
    }
}

impl From<CaptureSource> for CaptureSourceDto {
    fn from(value: CaptureSource) -> Self {
        match value {
            CaptureSource::Typed => Self::Typed,
            CaptureSource::Voice => Self::Voice,
        }
    }
}

impl From<CaptureSourceDto> for CaptureSource {
    fn from(value: CaptureSourceDto) -> Self {
        match value {
            CaptureSourceDto::Typed => Self::Typed,
            CaptureSourceDto::Voice => Self::Voice,
        }
    }
}

impl From<CaptureProcessing> for CaptureProcessingDto {
    fn from(value: CaptureProcessing) -> Self {
        match value {
            CaptureProcessing::Pending => Self::Pending,
            CaptureProcessing::Classified {
                target,
                classified_at,
            } => Self::Classified {
                target: target.into(),
                classified_at: timestamp(classified_at),
            },
            CaptureProcessing::Dismissed { dismissed_at } => Self::Dismissed {
                dismissed_at: timestamp(dismissed_at),
            },
        }
    }
}

impl TryFrom<CaptureProcessingDto> for CaptureProcessing {
    type Error = ProtocolConversionError;

    fn try_from(value: CaptureProcessingDto) -> Result<Self, Self::Error> {
        match value {
            CaptureProcessingDto::Pending => Ok(Self::Pending),
            CaptureProcessingDto::Classified {
                target,
                classified_at,
            } => Ok(Self::Classified {
                target: target.try_into()?,
                classified_at: parse_timestamp(&classified_at, "classified_at")?,
            }),
            CaptureProcessingDto::Dismissed { dismissed_at } => Ok(Self::Dismissed {
                dismissed_at: parse_timestamp(&dismissed_at, "dismissed_at")?,
            }),
        }
    }
}

impl From<Capture> for CaptureDto {
    fn from(value: Capture) -> Self {
        Self {
            id: value.id.to_string(),
            person_id: value.person_id.to_string(),
            original_input: value.original_input,
            captured_at: timestamp(value.captured_at),
            source: value.source.into(),
            processing: value.processing.into(),
            revision: value.revision.0,
        }
    }
}

impl TryFrom<CaptureDto> for Capture {
    type Error = ProtocolConversionError;

    fn try_from(value: CaptureDto) -> Result<Self, Self::Error> {
        let person_id = parse_person_id(&value.person_id, "person_id")?;
        let captured_at = parse_timestamp(&value.captured_at, "captured_at")?;
        let mut capture = Self::new(
            person_id,
            value.original_input,
            captured_at,
            value.source.into(),
        )?;
        capture.id = parse_capture_id(&value.id, "id")?;
        capture.processing = value.processing.try_into()?;
        capture.revision = Revision(value.revision);
        Ok(capture)
    }
}

impl TryFrom<DaySnapshot> for DaySnapshotDto {
    type Error = ProtocolConversionError;

    fn try_from(value: DaySnapshot) -> Result<Self, Self::Error> {
        Ok(Self {
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
            items: value.items.into_iter().map(Into::into).collect(),
            calendar: value.calendar,
        })
    }
}

impl TryFrom<DaySnapshotDto> for DaySnapshot {
    type Error = ProtocolConversionError;

    fn try_from(value: DaySnapshotDto) -> Result<Self, Self::Error> {
        if value.schema_version != PROTOCOL_VERSION {
            return Err(ProtocolConversionError::UnsupportedVersion {
                actual: value.schema_version,
                expected: PROTOCOL_VERSION,
            });
        }
        Ok(Self {
            person_id: parse_person_id(&value.person_id, "person_id")?,
            date: parse_date(&value.date, "date")?,
            calendar: value.calendar,
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
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}
