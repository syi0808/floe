use std::collections::HashSet;

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{CalendarRangeDto, OperationRefDto, TaskRefDto, UuidRefDto};

// Matches the Day owner snapshot ceilings; over-budget output fails explicitly.
pub const MAX_DAY_SNAPSHOT_ITEMS: usize = 10_000;
pub const MAX_DAY_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;
const MAX_DAY_COVERAGE_SOURCES: usize = 64;
const MAX_DAY_COVERAGE_RESOURCES: usize = 256;
const MAX_DAY_DISPLAY_TEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_DAY_TIMESTAMP_BYTES: usize = 64;
const MAX_DAY_REVISION: u64 = i64::MAX as u64;
const MAX_CIVIL_RANGE_DAYS: i64 = 31;
const MAX_RANGE_OFFSET_SECONDS: u32 = 86_400;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayQueryDto {
    pub date: String,
    pub timezone_offset_seconds: i32,
    pub end_timezone_offset_seconds: Option<i32>,
    pub now: String,
}

impl DayQueryDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        let date = parse_civil_date(&self.date, "day.date")?;
        if date.succ_opt().is_none() {
            return Err("day.date");
        }
        parse_instant(&self.now, "day.now")?;
        let end_offset = self
            .end_timezone_offset_seconds
            .unwrap_or(self.timezone_offset_seconds);
        if self.timezone_offset_seconds.unsigned_abs() >= MAX_RANGE_OFFSET_SECONDS
            || end_offset.unsigned_abs() >= MAX_RANGE_OFFSET_SECONDS
            || 86_400 + i64::from(self.timezone_offset_seconds) - i64::from(end_offset) <= 0
        {
            return Err("day.timezone_offset_seconds");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimedScheduleDto {
    pub starts_at: String,
    pub ends_at: String,
    pub timezone: String,
}

impl TimedScheduleDto {
    fn parsed_bounds(
        &self,
    ) -> Result<(DateTime<FixedOffset>, DateTime<FixedOffset>), &'static str> {
        if self.starts_at.len() > MAX_DAY_TIMESTAMP_BYTES
            || self.ends_at.len() > MAX_DAY_TIMESTAMP_BYTES
        {
            return Err("day.schedule.timestamp");
        }
        let starts_at =
            DateTime::parse_from_rfc3339(&self.starts_at).map_err(|_| "day.schedule.starts_at")?;
        let ends_at =
            DateTime::parse_from_rfc3339(&self.ends_at).map_err(|_| "day.schedule.ends_at")?;
        if starts_at.offset().local_minus_utc() != 0
            || ends_at.offset().local_minus_utc() != 0
            || ends_at <= starts_at
        {
            return Err("day.schedule.interval");
        }
        Ok((starts_at, ends_at))
    }

    /// New Calendar operation inputs use the bounded owner schedule contract.
    pub fn validate_new_action(&self) -> Result<(), &'static str> {
        let (starts_at, ends_at) = self.parsed_bounds()?;
        if !bounded_trimmed_text(&self.timezone, 128)
            || ends_at - starts_at > chrono::Duration::hours(24)
        {
            return Err("day.schedule");
        }
        Ok(())
    }

    /// Existing Day schedules are observations. Do not apply new-write limits.
    pub fn validate_historical(&self) -> Result<(), &'static str> {
        self.parsed_bounds()?;
        if self.timezone.len() > MAX_DAY_DISPLAY_TEXT_BYTES {
            return Err("day.schedule.timezone");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayEventTargetDto {
    pub event_id: UuidRefDto,
    pub expected_revision: u64,
}

impl DayEventTargetDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        positive_day_revision(
            self.expected_revision,
            "day.event.action_target.expected_revision",
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceRefDto {
    Manual,
    Capture {
        capture_id: UuidRefDto,
    },
    Calendar {
        source_ref: UuidRefDto,
        calendar_ref: UuidRefDto,
        calendar_label: String,
    },
}

impl SourceRefDto {
    fn validate(&self) -> Result<(), &'static str> {
        if let Self::Calendar { calendar_label, .. } = self {
            if calendar_label.is_empty() || calendar_label.len() > 256 {
                return Err("day.item.source.calendar_label");
            }
        }
        Ok(())
    }

    fn is_calendar(&self) -> bool {
        matches!(self, Self::Calendar { .. })
    }
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

impl EventScheduleDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Timed {
                starts_at,
                ends_at,
                timezone,
            } => TimedScheduleDto {
                starts_at: starts_at.clone(),
                ends_at: ends_at.clone(),
                timezone: timezone.clone(),
            }
            .validate_historical(),
            Self::AllDay {
                start_date,
                end_date_exclusive,
            } => {
                let start = parse_civil_date(start_date, "day.schedule.start_date")?;
                let end = parse_civil_date(end_date_exclusive, "day.schedule.end_date_exclusive")?;
                if end <= start {
                    return Err("day.schedule.interval");
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityDto {
    Low,
    Normal,
    High,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventDto {
    pub id: UuidRefDto,
    pub person_id: UuidRefDto,
    pub title: String,
    pub schedule: EventScheduleDto,
    pub source: SourceRefDto,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub deleted_at: Option<String>,
    pub action_target: Option<DayEventTargetDto>,
}

impl EventDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_day_display_text(&self.title, "day.event.title")?;
        self.schedule.validate()?;
        self.source.validate()?;
        parse_instant(&self.created_at, "day.event.created_at")?;
        parse_instant(&self.updated_at, "day.event.updated_at")?;
        if self
            .deleted_at
            .as_deref()
            .is_some_and(|value| parse_instant(value, "day.event.deleted_at").is_err())
        {
            return Err("day.event.deleted_at");
        }
        positive_day_revision(self.revision, "day.event.revision")?;
        if let Some(target) = &self.action_target {
            target.validate()?;
            if target.event_id != self.id || target.expected_revision != self.revision {
                return Err("day.event.action_target");
            }
        }
        if self.source.is_calendar() != self.action_target.is_some() {
            return Err("day.event.action_target");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDto {
    pub id: TaskRefDto,
    pub person_id: UuidRefDto,
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

impl TaskDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_day_display_text(&self.title, "day.task.title")?;
        self.source.validate()?;
        parse_optional_instant(self.deadline.as_deref(), "day.task.deadline")?;
        parse_optional_instant(self.completed_at.as_deref(), "day.task.completed_at")?;
        parse_instant(&self.created_at, "day.task.created_at")?;
        parse_instant(&self.updated_at, "day.task.updated_at")?;
        parse_optional_instant(self.deleted_at.as_deref(), "day.task.deleted_at")?;
        positive_day_revision(self.revision, "day.task.revision")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NoteDto {
    pub id: UuidRefDto,
    pub person_id: UuidRefDto,
    pub content: String,
    pub source: SourceRefDto,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub deleted_at: Option<String>,
}

impl NoteDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_day_display_text(&self.content, "day.note.content")?;
        self.source.validate()?;
        parse_instant(&self.created_at, "day.note.created_at")?;
        parse_instant(&self.updated_at, "day.note.updated_at")?;
        parse_optional_instant(self.deleted_at.as_deref(), "day.note.deleted_at")?;
        positive_day_revision(self.revision, "day.note.revision")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TimelineItemDto {
    Event(EventDto),
    Task(TaskDto),
    Note(NoteDto),
}

impl TimelineItemDto {
    fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Event(value) => value.validate(),
            Self::Task(value) => value.validate(),
            Self::Note(value) => value.validate(),
        }
    }

    fn person_id(&self) -> UuidRefDto {
        match self {
            Self::Event(value) => value.person_id,
            Self::Task(value) => value.person_id,
            Self::Note(value) => value.person_id,
        }
    }

    fn identity(&self) -> (u8, Uuid) {
        match self {
            Self::Event(value) => (0, value.id.get()),
            Self::Task(value) => (1, value.id.get()),
            Self::Note(value) => (2, value.id.get()),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DayCoverageStateDto {
    Current,
    Stale,
    Partial,
    Unavailable,
    Pending,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DayCalendarFailureDto {
    PermissionDenied,
    CalendarUnavailable,
    ProviderUnavailable,
    SourceChanged,
    SourceFenced,
    VaultLocked,
    BudgetExceeded,
    DeadlineExceeded,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayCalendarResourceCoverageDto {
    pub resource_ref: UuidRefDto,
    pub label: String,
    pub state: DayCoverageStateDto,
    pub last_success_at: Option<String>,
    pub last_range: Option<CalendarRangeDto>,
    pub failure: Option<DayCalendarFailureDto>,
    pub failure_at: Option<String>,
}

impl DayCalendarResourceCoverageDto {
    fn validate(&self) -> Result<(), &'static str> {
        validate_calendar_label(&self.label)?;
        parse_optional_instant(
            self.last_success_at.as_deref(),
            "day.calendar.resource.last_success_at",
        )?;
        parse_optional_instant(
            self.failure_at.as_deref(),
            "day.calendar.resource.failure_at",
        )?;
        if self
            .last_range
            .as_ref()
            .is_some_and(|range| validate_calendar_range(range).is_err())
        {
            return Err("day.calendar.resource.last_range");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayCalendarSourceCoverageDto {
    pub source_ref: UuidRefDto,
    pub label: String,
    pub state: DayCoverageStateDto,
    pub last_success_at: Option<String>,
    pub last_range: Option<CalendarRangeDto>,
    pub failure: Option<DayCalendarFailureDto>,
    pub failure_at: Option<String>,
    pub resources: Vec<DayCalendarResourceCoverageDto>,
}

impl DayCalendarSourceCoverageDto {
    fn validate(&self) -> Result<(), &'static str> {
        validate_calendar_label(&self.label)?;
        parse_optional_instant(
            self.last_success_at.as_deref(),
            "day.calendar.source.last_success_at",
        )?;
        parse_optional_instant(self.failure_at.as_deref(), "day.calendar.source.failure_at")?;
        if self
            .last_range
            .as_ref()
            .is_some_and(|range| validate_calendar_range(range).is_err())
        {
            return Err("day.calendar.source.last_range");
        }
        for resource in &self.resources {
            resource.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayCalendarCoverageDto {
    pub sources: Vec<DayCalendarSourceCoverageDto>,
}

impl DayCalendarCoverageDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.sources.len() > MAX_DAY_COVERAGE_SOURCES {
            return Err("day.calendar.sources");
        }
        let mut source_refs = HashSet::with_capacity(self.sources.len());
        let mut resource_refs = HashSet::new();
        let mut resource_count = 0usize;
        for source in &self.sources {
            source.validate()?;
            if !source_refs.insert(source.source_ref) {
                return Err("day.calendar.duplicate_source_ref");
            }
            resource_count = resource_count
                .checked_add(source.resources.len())
                .ok_or("day.calendar.resources")?;
            for resource in &source.resources {
                if !resource_refs.insert(resource.resource_ref) {
                    return Err("day.calendar.duplicate_resource_ref");
                }
            }
        }
        if resource_count > MAX_DAY_COVERAGE_RESOURCES {
            return Err("day.calendar.resources");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DaySnapshotDto {
    pub schema_version: u32,
    pub person_id: UuidRefDto,
    pub date: String,
    pub generated_at: String,
    pub timezone_offset_seconds: i32,
    pub now_event_id: Option<UuidRefDto>,
    pub next_event_id: Option<UuidRefDto>,
    pub overdue_task_count: u32,
    pub items: Vec<TimelineItemDto>,
    pub calendar: Option<DayCalendarCoverageDto>,
    pub calendar_mirror_revision: Option<u64>,
}

impl DaySnapshotDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1 {
            return Err("day.snapshot.schema_version");
        }
        parse_civil_date(&self.date, "day.snapshot.date")?;
        parse_instant(&self.generated_at, "day.snapshot.generated_at")?;
        if self.timezone_offset_seconds.unsigned_abs() >= MAX_RANGE_OFFSET_SECONDS
            || self.items.len() > MAX_DAY_SNAPSHOT_ITEMS
            || self
                .calendar_mirror_revision
                .is_some_and(|revision| revision == 0 || revision > MAX_DAY_REVISION)
        {
            return Err("day.snapshot");
        }

        let mut item_ids = HashSet::with_capacity(self.items.len());
        let mut event_ids = HashSet::new();
        for item in &self.items {
            item.validate()?;
            if item.person_id() != self.person_id || !item_ids.insert(item.identity()) {
                return Err("day.snapshot.items");
            }
            if let TimelineItemDto::Event(event) = item {
                event_ids.insert(event.id);
            }
        }
        if self
            .now_event_id
            .is_some_and(|event_id| !event_ids.contains(&event_id))
            || self
                .next_event_id
                .is_some_and(|event_id| !event_ids.contains(&event_id))
        {
            return Err("day.snapshot.event_reference");
        }
        if let Some(calendar) = &self.calendar {
            calendar.validate()?;
        }
        validate_snapshot_bytes(self)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DayRefreshFailureDto {
    SourceChanged,
    PermissionDenied,
    Unavailable,
    VaultLocked,
    BudgetExceeded,
    DeadlineExceeded,
    Cancelled,
    HostInterrupted,
    StorageUnavailable,
    InvalidAcquisition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum DayRefreshStateDto {
    Pending {
        operation_ref: OperationRefDto,
        revision: u64,
    },
    Running {
        operation_ref: OperationRefDto,
        revision: u64,
    },
    Completed {
        operation_ref: OperationRefDto,
        revision: u64,
        day: DaySnapshotDto,
    },
    Failed {
        operation_ref: OperationRefDto,
        revision: u64,
        failure: DayRefreshFailureDto,
    },
    Interrupted {
        operation_ref: OperationRefDto,
        revision: u64,
        failure: DayRefreshFailureDto,
    },
}

impl DayRefreshStateDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        let revision = match self {
            Self::Pending { revision, .. }
            | Self::Running { revision, .. }
            | Self::Completed { revision, .. }
            | Self::Failed { revision, .. }
            | Self::Interrupted { revision, .. } => *revision,
        };
        if revision == 0 || revision > MAX_DAY_REVISION {
            return Err("day_refresh.revision");
        }
        if let Self::Completed { day, .. } = self {
            day.validate()?;
        }
        Ok(())
    }
}

// These local mutation/capture DTO names and payloads remain available to the
// existing Day mutation path. Product snapshots above use only safe Day values.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureDto {
    pub id: String,
    pub person_id: String,
    pub original_input: String,
    pub captured_at: String,
    pub source: CaptureSourceDto,
    pub processing: CaptureProcessingDto,
    pub revision: u64,
}

impl CaptureDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_uuid_text(&self.id)
            || !valid_uuid_text(&self.person_id)
            || self.original_input.len() > 1_048_576
        {
            return Err("day.capture");
        }
        parse_instant(&self.captured_at, "day.capture.captured_at")?;
        positive_day_revision(self.revision, "day.capture.revision")?;
        match &self.processing {
            CaptureProcessingDto::Pending => {}
            CaptureProcessingDto::Classified {
                target,
                classified_at,
            } => {
                target.validate()?;
                parse_instant(classified_at, "day.capture.classified_at")?;
            }
            CaptureProcessingDto::Dismissed { dismissed_at } => {
                parse_instant(dismissed_at, "day.capture.dismissed_at")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSourceDto {
    Typed,
    Voice,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
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

impl DomainRefDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        let id = match self {
            Self::Event { id } | Self::Task { id } | Self::Note { id } => id,
        };
        if valid_uuid_text(id) {
            Ok(())
        } else {
            Err("day.domain_ref.id")
        }
    }
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
#[serde(deny_unknown_fields)]
pub struct MutationResultDto {
    pub snapshot: DaySnapshotDto,
    pub changed_item: Option<TimelineItemDto>,
    pub capture: Option<CaptureDto>,
}

impl MutationResultDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.snapshot.validate()?;
        if let Some(item) = &self.changed_item {
            item.validate()?;
            if item.person_id() != self.snapshot.person_id {
                return Err("day.mutation_result.changed_item");
            }
        }
        if let Some(capture) = &self.capture {
            capture.validate()?;
        }
        Ok(())
    }
}

pub fn validate_calendar_range(range: &CalendarRangeDto) -> Result<(), &'static str> {
    let days = (range.end_date_exclusive - range.start_date).num_days();
    let end_offset = range
        .end_timezone_offset_seconds
        .unwrap_or(range.timezone_offset_seconds);
    if !(1..=MAX_CIVIL_RANGE_DAYS).contains(&days)
        || range.timezone_offset_seconds.unsigned_abs() >= MAX_RANGE_OFFSET_SECONDS
        || end_offset.unsigned_abs() >= MAX_RANGE_OFFSET_SECONDS
        || days * 86_400 + i64::from(range.timezone_offset_seconds) - i64::from(end_offset) <= 0
    {
        return Err("day.calendar_range");
    }
    Ok(())
}

pub(crate) fn validate_instant(value: &str) -> Result<DateTime<FixedOffset>, &'static str> {
    parse_instant(value, "day.instant")
}

fn validate_calendar_label(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > 256 {
        Err("day.calendar.label")
    } else {
        Ok(())
    }
}

fn validate_day_display_text(value: &str, field: &'static str) -> Result<(), &'static str> {
    if value.len() <= MAX_DAY_DISPLAY_TEXT_BYTES {
        Ok(())
    } else {
        Err(field)
    }
}

fn positive_day_revision(value: u64, field: &'static str) -> Result<(), &'static str> {
    if value == 0 || value > MAX_DAY_REVISION {
        Err(field)
    } else {
        Ok(())
    }
}

fn parse_civil_date(value: &str, field: &'static str) -> Result<NaiveDate, &'static str> {
    if value.len() != 10 {
        return Err(field);
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| field)?;
    if date.format("%Y-%m-%d").to_string() != value {
        return Err(field);
    }
    Ok(date)
}

fn parse_instant(value: &str, field: &'static str) -> Result<DateTime<FixedOffset>, &'static str> {
    if value.is_empty() || value.len() > MAX_DAY_TIMESTAMP_BYTES {
        return Err(field);
    }
    DateTime::parse_from_rfc3339(value)
        .map_err(|_| field)
        .and_then(|instant| {
            if instant.offset().local_minus_utc() == 0 {
                Ok(instant)
            } else {
                Err(field)
            }
        })
}

fn parse_optional_instant(value: Option<&str>, field: &'static str) -> Result<(), &'static str> {
    if let Some(value) = value {
        parse_instant(value, field)?;
    }
    Ok(())
}

fn valid_uuid_text(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil())
}

fn bounded_trimmed_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn validate_snapshot_bytes(value: &DaySnapshotDto) -> Result<(), &'static str> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|size| *size <= MAX_DAY_SNAPSHOT_BYTES)
                .ok_or_else(|| std::io::Error::other("Day snapshot byte budget"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(0), value).map_err(|_| "day.snapshot.byte_budget")
}
