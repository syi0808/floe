//! Idempotent manual Day commands and their pure local transition.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use floe_kernel::{CaptureId, EventId, NoteId, PersonId, Revision, TaskId};
use crate::{Capture, CaptureProcessing, CaptureSource, DayError, DayQuery, DaySnapshot, DayTimelineItem, DomainRef, Event, EventSchedule, Note, Priority, SourceRef, Task, TimelineItem, CalendarMirror};

pub const MAX_DAY_COMMAND_RECEIPTS: usize = 4096;
pub const MAX_DAY_MUTATION_BYTES: usize = 1024 * 1024;
pub const MAX_DAY_MUTATION_RECEIPT_BYTES: usize = crate::MAX_DAY_SNAPSHOT_BYTES + 2 * MAX_DAY_MUTATION_BYTES + 65_536;
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Classification { Event { title: String, schedule: EventSchedule }, Task { title: String, deadline: Option<DateTime<Utc>>, priority: Priority }, Note { content: String } }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayMutationRequest { pub command_id: Uuid, pub day: DayQuery, pub mutation: DayMutation }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DayMutation {
    SubmitCapture { input: String, occurred_at: DateTime<Utc> },
    ClassifyCapture { capture_id: CaptureId, expected_revision: u64, classification: Classification, occurred_at: DateTime<Utc> },
    CreateEvent { title: String, schedule: EventSchedule, occurred_at: DateTime<Utc> },
    CreateTask { title: String, deadline: Option<DateTime<Utc>>, priority: Priority, occurred_at: DateTime<Utc> },
    CreateNote { content: String, occurred_at: DateTime<Utc> },
    UpdateEvent { event_id: EventId, expected_revision: u64, title: String, schedule: EventSchedule, occurred_at: DateTime<Utc> },
    UpdateTask { task_id: TaskId, expected_revision: u64, title: String, deadline: Option<DateTime<Utc>>, priority: Priority, occurred_at: DateTime<Utc> },
    UpdateNote { note_id: NoteId, expected_revision: u64, content: String, occurred_at: DateTime<Utc> },
    SetTaskCompletion { task_id: TaskId, expected_revision: u64, completed: bool, occurred_at: DateTime<Utc> },
    DeleteItem { target: DomainRef, expected_revision: u64, occurred_at: DateTime<Utc> },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayMutationResult { pub command_id: Uuid, pub snapshot: DaySnapshot, pub changed_item: Option<DayTimelineItem>, pub capture: Option<Capture> }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayMutationCommand { pub person_id: PersonId, pub device_id: String, pub executor_generation: Uuid, pub request: DayMutationRequest }
#[derive(Clone, Debug)]
pub enum DayMutationTarget { None, Capture(CaptureId), Item(DomainRef) }
#[derive(Clone, Debug)]
pub enum DayMutationPrior { Absent, Capture(Capture), Item(TimelineItem) }
#[derive(Clone, Debug)]
pub struct DayMutationApplied {
    pub capture: Option<Capture>, pub item: Option<TimelineItem>,
    pub expected_capture_revision: Option<Revision>, pub expected_item_revision: Option<Revision>,
}
impl DayMutationCommand {
    pub fn validate(&self) -> Result<(), DayError> {
        if !self.person_id.is_valid() || self.device_id.is_empty() || self.device_id.len() > 256 || self.executor_generation.is_nil() || self.request.command_id.is_nil() { return Err(DayError::validation("invalid Day command identity")); }
        self.request.day.range()?;
        let schedule = match &self.request.mutation { DayMutation::CreateEvent { schedule, .. } | DayMutation::UpdateEvent { schedule, .. } | DayMutation::ClassifyCapture { classification: Classification::Event { schedule, .. }, .. } => Some(schedule), _ => None };
        if let Some(schedule) = schedule { match schedule { EventSchedule::Timed(value) => { crate::TimedSchedule::new(value.starts_at, value.ends_at, &value.timezone)?; }, EventSchedule::AllDay(value) => { crate::AllDaySchedule::new(value.start_date, value.end_date_exclusive)?; } } }
        struct Counter(usize);
        impl std::io::Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> { self.0 = self.0.checked_add(bytes.len()).filter(|count| *count <= MAX_DAY_MUTATION_BYTES).ok_or_else(|| std::io::Error::other("Day command byte budget"))?; Ok(bytes.len()) }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        serde_json::to_writer(Counter(0), &self.request).map_err(|_| DayError::budget("Day command byte budget"))?;
        Ok(())
    }
    pub fn intent_digest(&self) -> Result<[u8; 32], DayError> {
        self.validate()?;
        let day = &self.request.day;
        crate::digest(&("day_mutation", self.person_id, &self.device_id, self.request.command_id, day.date, day.timezone_offset_seconds, day.end_timezone_offset_seconds.unwrap_or(day.timezone_offset_seconds), &self.request.mutation))
    }
    pub fn target(&self) -> DayMutationTarget {
        match &self.request.mutation {
            DayMutation::ClassifyCapture { capture_id, .. } => DayMutationTarget::Capture(*capture_id),
            DayMutation::UpdateEvent { event_id, .. } => DayMutationTarget::Item(DomainRef::Event(*event_id)),
            DayMutation::UpdateTask { task_id, .. } | DayMutation::SetTaskCompletion { task_id, .. } => DayMutationTarget::Item(DomainRef::Task(*task_id)),
            DayMutation::UpdateNote { note_id, .. } => DayMutationTarget::Item(DomainRef::Note(*note_id)),
            DayMutation::DeleteItem { target, .. } => DayMutationTarget::Item(*target),
            _ => DayMutationTarget::None,
        }
    }
    fn entity_id(&self, kind: &str) -> Uuid { Uuid::new_v5(&self.request.command_id, format!("floe.day.{kind}:{}:{}", self.person_id, self.device_id).as_bytes()) }
    pub fn apply(&self, prior: DayMutationPrior) -> Result<DayMutationApplied, DayError> {
        self.validate()?;
        let mut result = DayMutationApplied { capture: None, item: None, expected_capture_revision: None, expected_item_revision: None };
        match (&self.request.mutation, prior) {
            (DayMutation::SubmitCapture { input, occurred_at }, DayMutationPrior::Absent) => { let mut value = Capture::new(self.person_id, input.clone(), *occurred_at, CaptureSource::Typed)?; value.id = CaptureId(self.entity_id("capture")); result.capture = Some(value); }
            (DayMutation::CreateEvent { title, schedule, occurred_at }, DayMutationPrior::Absent) => { let mut value = Event::new(self.person_id, title.clone(), schedule.clone(), SourceRef::Manual, *occurred_at)?; value.id = EventId(self.entity_id("event")); result.item = Some(TimelineItem::Event(value)); }
            (DayMutation::CreateTask { title, deadline, priority, occurred_at }, DayMutationPrior::Absent) => { let mut value = Task::new(self.person_id, title.clone(), *deadline, *priority, SourceRef::Manual, *occurred_at)?; value.id = TaskId(self.entity_id("task")); result.item = Some(TimelineItem::Task(value)); }
            (DayMutation::CreateNote { content, occurred_at }, DayMutationPrior::Absent) => { let mut value = Note::new(self.person_id, content.clone(), SourceRef::Manual, *occurred_at)?; value.id = NoteId(self.entity_id("note")); result.item = Some(TimelineItem::Note(value)); }
            (DayMutation::ClassifyCapture { capture_id, expected_revision, classification, occurred_at }, DayMutationPrior::Capture(mut capture)) => {
                if capture.id != *capture_id || capture.person_id != self.person_id { return Err(DayError::not_found("capture", capture_id)); }
                revision(capture.revision, *expected_revision)?;
                if capture.processing != CaptureProcessing::Pending { return Err(DayError::conflict("capture has already been resolved")); }
                let source = SourceRef::Capture(capture.id);
                let item = match classification {
                    Classification::Event { title, schedule } => { let mut value = Event::new(self.person_id, title.clone(), schedule.clone(), source, *occurred_at)?; value.id = EventId(self.entity_id("event")); TimelineItem::Event(value) }
                    Classification::Task { title, deadline, priority } => { let mut value = Task::new(self.person_id, title.clone(), *deadline, *priority, source, *occurred_at)?; value.id = TaskId(self.entity_id("task")); TimelineItem::Task(value) }
                    Classification::Note { content } => { let mut value = Note::new(self.person_id, content.clone(), source, *occurred_at)?; value.id = NoteId(self.entity_id("note")); TimelineItem::Note(value) }
                };
                capture.classify(item_target(&item), *occurred_at);
                result.capture = Some(capture); result.item = Some(item); result.expected_capture_revision = Some(Revision(*expected_revision));
            }
            (mutation, DayMutationPrior::Item(mut item)) => {
                let expected = match mutation { DayMutation::UpdateEvent { expected_revision, .. } | DayMutation::UpdateTask { expected_revision, .. } | DayMutation::UpdateNote { expected_revision, .. } | DayMutation::SetTaskCompletion { expected_revision, .. } | DayMutation::DeleteItem { expected_revision, .. } => *expected_revision, _ => return Err(DayError::conflict("unexpected Day target")) };
                let DayMutationTarget::Item(target) = self.target() else { return Err(DayError::conflict("unexpected Day target")); };
                if item_target(&item) != target || item_person(&item) != self.person_id { return Err(DayError::not_found("Day item", "target")); }
                revision(item_revision(&item), expected)?;
                match (mutation, &mut item) {
                    (DayMutation::UpdateEvent { title, schedule, occurred_at, .. }, TimelineItem::Event(value)) => { local_event(value)?; value.update(title.clone(), schedule.clone(), *occurred_at)?; }
                    (DayMutation::UpdateTask { title, deadline, priority, occurred_at, .. }, TimelineItem::Task(value)) => value.update(title.clone(), *deadline, *priority, *occurred_at)?,
                    (DayMutation::UpdateNote { content, occurred_at, .. }, TimelineItem::Note(value)) => value.update(content.clone(), *occurred_at)?,
                    (DayMutation::SetTaskCompletion { completed, occurred_at, .. }, TimelineItem::Task(value)) => if *completed { value.complete(*occurred_at) } else { value.reopen(*occurred_at) },
                    (DayMutation::DeleteItem { occurred_at, .. }, TimelineItem::Event(value)) => { local_event(value)?; value.delete(*occurred_at); }
                    (DayMutation::DeleteItem { occurred_at, .. }, TimelineItem::Task(value)) => value.delete(*occurred_at),
                    (DayMutation::DeleteItem { occurred_at, .. }, TimelineItem::Note(value)) => value.delete(*occurred_at),
                    _ => return Err(DayError::conflict("Day target kind changed")),
                }
                result.expected_item_revision = Some(Revision(expected)); result.item = Some(item);
            }
            _ => return Err(DayError::not_found("Day target", "requested identity")),
        }
        Ok(result)
    }
    pub fn result(&self, applied: &DayMutationApplied, items: Vec<TimelineItem>, mirror: Option<CalendarMirror>) -> Result<DayMutationResult, DayError> {
        let mut events = Vec::new(); let mut tasks = Vec::new(); let mut notes = Vec::new();
        for item in items { match item { TimelineItem::Event(value) => events.push(value), TimelineItem::Task(value) => tasks.push(value), TimelineItem::Note(value) => notes.push(value) } }
        if let Some(mirror) = &mirror { events.extend(mirror.events.clone()); }
        let query = &self.request.day;
        let range = query.range()?;
        let mut snapshot = crate::project_day_with_end_offset(self.person_id, query.date, query.timezone_offset_seconds, query.end_timezone_offset_seconds, query.now, events, tasks, notes)?;
        snapshot.calendar_mirror_revision = mirror.as_ref().map(|value| value.mirror_revision);
        snapshot.calendar = mirror.map(|value| crate::project_unverified_calendar_coverage(&value.state, &range, query.now));
        snapshot.validate_bounds()?;
        let changed_item = if matches!(&self.request.mutation, DayMutation::DeleteItem { .. }) { None } else { applied.item.as_ref().map(|value| match value { TimelineItem::Event(value) => DayTimelineItem::Event(crate::project_event(value)), TimelineItem::Task(value) => DayTimelineItem::Task(crate::project_task(value)), TimelineItem::Note(value) => DayTimelineItem::Note(crate::project_note(value)) }) };
        let capture = matches!(&self.request.mutation, DayMutation::SubmitCapture { .. }).then(|| applied.capture.clone()).flatten();
        let result = DayMutationResult { command_id: self.request.command_id, snapshot, changed_item, capture }; self.validate_result(&result)?; Ok(result)
    }
    pub fn validate_result(&self, result: &DayMutationResult) -> Result<(), DayError> {
        self.validate()?; result.snapshot.validate_bounds()?;
        if result.command_id != self.request.command_id || result.snapshot.person_id != self.person_id || result.snapshot.date != self.request.day.date || result.snapshot.generated_at != self.request.day.now || result.snapshot.timezone_offset_seconds != self.request.day.timezone_offset_seconds { return Err(DayError::storage("Day command result identity mismatch")); }
        if let Some(item) = &result.changed_item {
            let (person, id, revision) = match item { DayTimelineItem::Event(value) => (value.person_id,value.id.0,value.revision), DayTimelineItem::Task(value) => (value.person_id,value.id.0,value.revision), DayTimelineItem::Note(value) => (value.person_id,value.id.0,value.revision) };
            if person != self.person_id || id.is_nil() || revision.0 == 0 || revision.0 > i64::MAX as u64 { return Err(DayError::storage("invalid Day changed item identity")); }
        }
        if let Some(capture) = &result.capture { if capture.person_id != self.person_id || capture.id.0 != self.entity_id("capture") || capture.revision.0 != 1 { return Err(DayError::storage("invalid Day capture result identity")); } }
        match &self.request.mutation {
            DayMutation::SubmitCapture { .. } if result.capture.is_some() && result.changed_item.is_none() => {}
            DayMutation::DeleteItem { .. } if result.capture.is_none() && result.changed_item.is_none() => {}
            DayMutation::SubmitCapture { .. } | DayMutation::DeleteItem { .. } => return Err(DayError::storage("invalid Day mutation result shape")),
            _ if result.capture.is_none() && result.changed_item.is_some() => {}
            _ => return Err(DayError::storage("invalid Day mutation result shape")),
        }
        Ok(())
    }
}
fn revision(actual: Revision, expected: u64) -> Result<(), DayError> { if actual.0 == 0 || actual.0 >= i64::MAX as u64 || expected == 0 || actual.0 != expected { Err(DayError::stale(Revision(expected), actual)) } else { Ok(()) } }
fn local_event(value: &Event) -> Result<(), DayError> { if matches!(&value.source, SourceRef::Calendar(_)) { Err(DayError::validation("external Calendar events require Actions")) } else { Ok(()) } }
fn item_target(value: &TimelineItem) -> DomainRef { match value { TimelineItem::Event(value) => DomainRef::Event(value.id), TimelineItem::Task(value) => DomainRef::Task(value.id), TimelineItem::Note(value) => DomainRef::Note(value.id) } }
fn item_person(value: &TimelineItem) -> PersonId { match value { TimelineItem::Event(value) => value.person_id, TimelineItem::Task(value) => value.person_id, TimelineItem::Note(value) => value.person_id } }
fn item_revision(value: &TimelineItem) -> Revision { match value { TimelineItem::Event(value) => value.revision, TimelineItem::Task(value) => value.revision, TimelineItem::Note(value) => value.revision } }
