use serde::{Deserialize, Serialize};

use super::calendar::{CalendarConnectionDto, CalendarFailureDto, CalendarSourceDto};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayQueryDto {
    pub date: String,
    pub timezone_offset_seconds: i32,
    pub end_timezone_offset_seconds: Option<i32>,
    pub now: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DaySnapshotDto {
    pub schema_version: u32,
    pub person_id: String,
    pub date: String,
    pub generated_at: String,
    pub timezone_offset_seconds: i32,
    pub now_event_id: Option<String>,
    pub next_event_id: Option<String>,
    pub overdue_task_count: u32,
    pub items: Vec<TimelineItemDto>,
    pub calendar: Option<CalendarConnectionDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimelineItemDto {
    Event(EventDto),
    Task(TaskDto),
    Note(NoteDto),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventDto {
    pub id: String,
    pub person_id: String,
    pub title: String,
    pub schedule: EventScheduleDto,
    pub source: SourceRefDto,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventScheduleDto {
    Timed {
        starts_at: String,
        ends_at: String,
        timezone: String,
    },
    AllDay {
        start_date: String,
        end_date_exclusive: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceRefDto {
    Manual,
    Capture { capture_id: String },
    Calendar { source: CalendarSourceDto },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityDto {
    Low,
    Normal,
    High,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskDto {
    pub id: String,
    pub person_id: String,
    pub title: String,
    pub deadline: Option<String>,
    pub priority: PriorityDto,
    pub completed_at: Option<String>,
    pub source: SourceRefDto,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NoteDto {
    pub id: String,
    pub person_id: String,
    pub content: String,
    pub source: SourceRefDto,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CaptureDto {
    pub id: String,
    pub person_id: String,
    pub original_input: String,
    pub captured_at: String,
    pub source: CaptureSourceDto,
    pub processing: CaptureProcessingDto,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSourceDto {
    Typed,
    Voice,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CaptureProcessingDto {
    Pending,
    Classified {
        target: DomainRefDto,
        classified_at: String,
    },
    Dismissed {
        dismissed_at: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DomainRefDto {
    Event { id: String },
    Task { id: String },
    Note { id: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarRecordDto {
    pub can_modify: bool,
    pub calendar_id: String,
    pub external_id: String,
    pub external_revision: String,
    pub title: String,
    pub schedule: EventScheduleDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarBatchDto {
    pub calendar_id: String,
    pub records: Vec<CalendarRecordDto>,
    pub failure: Option<CalendarFailureDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClassificationDto {
    Event {
        title: String,
        schedule: EventScheduleDto,
    },
    Task {
        title: String,
        deadline: Option<String>,
        priority: PriorityDto,
    },
    Note {
        content: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MutationResultDto {
    pub snapshot: DaySnapshotDto,
    pub changed_item: Option<TimelineItemDto>,
    pub capture: Option<CaptureDto>,
}
