//! Safe Day display values. Provider evidence remains in private owner records.
use chrono::{DateTime, Utc};
use floe_kernel::{CaptureId, EventId, NoteId, PersonId, Revision, TaskId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use crate::{CalendarFailure, CalendarMirrorState, CalendarRange, Event, EventSchedule, Note, Priority, SourceRef, Task};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DayItemSource {
    Manual,
    Capture { capture_id: CaptureId },
    Calendar { source_ref: Uuid, calendar_ref: Uuid, calendar_label: String },
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayEventTarget { pub event_id: EventId, pub expected_revision: Revision }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayEvent {
    pub id: EventId, pub person_id: PersonId, pub title: String, pub schedule: EventSchedule,
    pub source: DayItemSource, pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc>,
    pub revision: Revision, pub deleted_at: Option<DateTime<Utc>>,
    /// Correlation for Actions to reload private evidence, never permission.
    pub action_target: Option<DayEventTarget>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayTask {
    pub id: TaskId, pub person_id: PersonId, pub title: String, pub deadline: Option<DateTime<Utc>>,
    pub priority: Priority, pub completed_at: Option<DateTime<Utc>>, pub source: DayItemSource,
    pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc>, pub revision: Revision,
    pub deleted_at: Option<DateTime<Utc>>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayNote { pub id: NoteId, pub person_id: PersonId, pub content: String, pub source: DayItemSource, pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc>, pub revision: Revision, pub deleted_at: Option<DateTime<Utc>> }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DayTimelineItem { Event(DayEvent), Task(DayTask), Note(DayNote) }

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DayCoverageState { Current, Stale, Partial, Unavailable, Pending }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayCalendarResourceCoverage { pub resource_ref: Uuid, pub label: String, pub state: DayCoverageState, pub last_success_at: Option<DateTime<Utc>>, pub last_range: Option<CalendarRange>, pub failure: Option<CalendarFailure>, pub failure_at: Option<DateTime<Utc>> }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayCalendarSourceCoverage { pub source_ref: Uuid, pub label: String, pub state: DayCoverageState, pub last_success_at: Option<DateTime<Utc>>, pub last_range: Option<CalendarRange>, pub failure: Option<CalendarFailure>, pub failure_at: Option<DateTime<Utc>>, pub resources: Vec<DayCalendarResourceCoverage> }
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayCalendarCoverage { pub sources: Vec<DayCalendarSourceCoverage> }

pub fn project_item_source(person_id: PersonId, source: &SourceRef) -> DayItemSource {
    match source { SourceRef::Manual => DayItemSource::Manual, SourceRef::Capture(capture_id) => DayItemSource::Capture { capture_id: *capture_id }, SourceRef::Calendar(source) => DayItemSource::Calendar { source_ref: display_reference(person_id, source.connection_id.as_str(), ""), calendar_ref: display_reference(person_id, source.connection_id.as_str(), &source.calendar_id), calendar_label: source.calendar_name.clone() } }
}
pub fn project_event(event: &Event) -> DayEvent { DayEvent { id: event.id, person_id: event.person_id, title: event.title.clone(), schedule: event.schedule.clone(), source: project_item_source(event.person_id, &event.source), created_at: event.created_at, updated_at: event.updated_at, revision: event.revision, deleted_at: event.deleted_at, action_target: matches!(&event.source, SourceRef::Calendar(_)).then_some(DayEventTarget { event_id: event.id, expected_revision: event.revision }) } }
pub fn project_task(task: &Task) -> DayTask { DayTask { id: task.id, person_id: task.person_id, title: task.title.clone(), deadline: task.deadline, priority: task.priority, completed_at: task.completed_at, source: project_item_source(task.person_id, &task.source), created_at: task.created_at, updated_at: task.updated_at, revision: task.revision, deleted_at: task.deleted_at } }
pub fn project_note(note: &Note) -> DayNote { DayNote { id: note.id, person_id: note.person_id, content: note.content.clone(), source: project_item_source(note.person_id, &note.source), created_at: note.created_at, updated_at: note.updated_at, revision: note.revision, deleted_at: note.deleted_at } }
pub fn project_calendar_coverage(mirror: &CalendarMirrorState, now: DateTime<Utc>) -> DayCalendarCoverage {
    let sources = mirror.sources.iter().map(|source| {
        let person = source.source.source.person_id(); let connection = source.source.source.connection_id();
        let resources = source.source.calendars.iter().map(|calendar| {
            let status = source.calendar_statuses.get(&calendar.calendar_id);
            DayCalendarResourceCoverage { resource_ref: display_reference(person, connection.as_str(), &calendar.calendar_id), label: calendar.calendar_name.clone(), state: coverage_state(status.and_then(|status| status.last_success_at), status.and_then(|status| status.error), now), last_success_at: status.and_then(|status| status.last_success_at), last_range: status.and_then(|status| status.last_range.clone()), failure: status.and_then(|status| status.error), failure_at: status.and_then(|status| status.error_at) }
        }).collect::<Vec<_>>();
        let current = resources.iter().filter(|resource| resource.state == DayCoverageState::Current).count();
        let state = if current > 0 && current < resources.len() { DayCoverageState::Partial } else { coverage_state(source.last_success_at, source.error, now) };
        let label = match source.source.provider { floe_context_contract::CalendarProvider::EventKit => "Apple Calendar", floe_context_contract::CalendarProvider::Google => "Google Calendar", floe_context_contract::CalendarProvider::Microsoft => "Microsoft Calendar", floe_context_contract::CalendarProvider::Fixture => "Example Calendar", floe_context_contract::CalendarProvider::Android => "Calendar" }.to_owned();
        DayCalendarSourceCoverage { source_ref: display_reference(person, connection.as_str(), ""), label, state, last_success_at: source.last_success_at, last_range: source.last_range.clone(), failure: source.error, failure_at: source.error_at, resources }
    }).collect(); DayCalendarCoverage { sources }
}
fn coverage_state(success: Option<DateTime<Utc>>, failure: Option<CalendarFailure>, now: DateTime<Utc>) -> DayCoverageState { if failure.is_some() { DayCoverageState::Unavailable } else { match success { Some(success) if success <= now && now.signed_duration_since(success) < chrono::Duration::minutes(5) => DayCoverageState::Current, Some(_) => DayCoverageState::Stale, None => DayCoverageState::Pending } } }
fn display_reference(person: PersonId, connection: &str, resource: &str) -> Uuid {
    let mut hash = Sha256::new(); for value in ["floe.day.display", &person.to_string(), connection, resource] { hash.update((value.len() as u64).to_be_bytes()); hash.update(value.as_bytes()); }
    let digest = hash.finalize(); let mut bytes = [0; 16]; bytes.copy_from_slice(&digest[..16]); bytes[6] = (bytes[6] & 15) | 80; bytes[8] = (bytes[8] & 63) | 128; Uuid::from_bytes(bytes)
}
