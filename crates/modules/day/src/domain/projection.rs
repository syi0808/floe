use chrono::{DateTime, NaiveDate, Utc};
use floe_kernel::{EventId, PersonId};
use serde::{Deserialize, Serialize};

use super::{DayCalendarCoverage, DayTimelineItem, Event, EventSchedule, Note, Task};

pub const MAX_DAY_SNAPSHOT_ITEMS: usize = 10_000;
pub const MAX_DAY_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TimelineItem {
    Event(Event),
    Task(Task),
    Note(Note),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DaySnapshot {
    pub person_id: PersonId,
    pub date: NaiveDate,
    pub generated_at: DateTime<Utc>,
    pub timezone_offset_seconds: i32,
    pub now_event_id: Option<EventId>,
    pub next_event_id: Option<EventId>,
    pub overdue_task_count: usize,
    pub items: Vec<DayTimelineItem>,
    #[serde(default)]
    pub calendar: Option<DayCalendarCoverage>,
    pub calendar_mirror_revision: Option<u64>,
}

pub fn project_day(
    person_id: PersonId,
    date: NaiveDate,
    timezone_offset_seconds: i32,
    now: DateTime<Utc>,
    events: Vec<Event>,
    tasks: Vec<Task>,
    notes: Vec<Note>,
) -> Result<DaySnapshot, crate::DayError> {
    project_day_with_end_offset(
        person_id,
        date,
        timezone_offset_seconds,
        None,
        now,
        events,
        tasks,
        notes,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn project_day_with_end_offset(
    person_id: PersonId,
    date: NaiveDate,
    timezone_offset_seconds: i32,
    end_timezone_offset_seconds: Option<i32>,
    now: DateTime<Utc>,
    mut events: Vec<Event>,
    mut tasks: Vec<Task>,
    mut notes: Vec<Note>,
) -> Result<DaySnapshot, crate::DayError> {
    let day_start =
        DateTime::<Utc>::from_naive_utc_and_offset(date.and_hms_opt(0, 0, 0).unwrap(), Utc)
            - chrono::Duration::seconds(i64::from(timezone_offset_seconds));
    let day_end = day_start
        + chrono::Duration::days(1)
        + chrono::Duration::seconds(
            i64::from(timezone_offset_seconds)
                - i64::from(end_timezone_offset_seconds.unwrap_or(timezone_offset_seconds)),
        );
    events.retain(|item| {
        item.person_id == person_id
            && item.deleted_at.is_none()
            && match &item.schedule {
                EventSchedule::Timed(value) => {
                    value.ends_at > day_start && value.starts_at < day_end
                }
                EventSchedule::AllDay(value) => {
                    value.start_date <= date && date < value.end_date_exclusive
                }
            }
    });
    tasks.retain(|item| {
        item.person_id == person_id
            && item.deleted_at.is_none()
            && item.created_at < day_end
            && item
                .completed_at
                .is_none_or(|completed_at| completed_at >= day_start)
    });
    notes.retain(|item| {
        item.person_id == person_id
            && item.deleted_at.is_none()
            && day_start <= item.created_at
            && item.created_at < day_end
    });
    let mut timed: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event.schedule, EventSchedule::Timed(_)))
        .collect();
    timed.sort_by_key(|event| match &event.schedule {
        EventSchedule::Timed(value) => (value.starts_at, event.id),
        _ => unreachable!(),
    });
    let now_event_id = timed
        .iter()
        .find(|event| match &event.schedule {
            EventSchedule::Timed(value) => value.starts_at <= now && now < value.ends_at,
            _ => false,
        })
        .map(|event| event.id);
    let next_event_id = timed
        .iter()
        .find(|event| match &event.schedule {
            EventSchedule::Timed(value) => value.starts_at > now,
            _ => false,
        })
        .map(|event| event.id);
    let overdue_task_count = tasks
        .iter()
        .filter(|task| {
            task.completed_at.is_none() && task.deadline.is_some_and(|deadline| deadline < now)
        })
        .count();
    let total = events
        .len()
        .checked_add(tasks.len())
        .and_then(|count| count.checked_add(notes.len()))
        .ok_or_else(|| crate::DayError::budget("Day snapshot item budget"))?;
    if total > MAX_DAY_SNAPSHOT_ITEMS {
        return Err(crate::DayError::budget("Day snapshot item budget"));
    }
    let mut items = Vec::with_capacity(total);
    items.extend(events.into_iter().map(TimelineItem::Event));
    items.extend(tasks.into_iter().map(TimelineItem::Task));
    items.extend(notes.into_iter().map(TimelineItem::Note));
    items.sort_by_key(|item| match item {
        TimelineItem::Event(event) => {
            let effective = match &event.schedule {
                EventSchedule::Timed(value) => value.starts_at.timestamp(),
                EventSchedule::AllDay(_) => day_start.timestamp(),
            };
            (
                effective,
                0,
                event.created_at.timestamp(),
                event.id.to_string(),
            )
        }
        TimelineItem::Task(task) => (
            task.deadline
                .map_or(i64::MAX - 1, |value| value.timestamp()),
            1,
            task.created_at.timestamp(),
            task.id.to_string(),
        ),
        TimelineItem::Note(note) => (
            i64::MAX,
            2,
            note.created_at.timestamp(),
            note.id.to_string(),
        ),
    });
    let snapshot = DaySnapshot {
        calendar: None,
        calendar_mirror_revision: None,
        person_id,
        date,
        generated_at: now,
        timezone_offset_seconds,
        now_event_id,
        next_event_id,
        overdue_task_count,
        items: items
            .into_iter()
            .map(|item| match item {
                TimelineItem::Event(event) => DayTimelineItem::Event(super::project_event(&event)),
                TimelineItem::Task(task) => DayTimelineItem::Task(super::project_task(&task)),
                TimelineItem::Note(note) => DayTimelineItem::Note(super::project_note(&note)),
            })
            .collect(),
    };
    snapshot.validate_bounds()?;
    Ok(snapshot)
}

impl DaySnapshot {
    pub fn validate_bounds(&self) -> Result<(), crate::DayError> {
        if self.items.len() > MAX_DAY_SNAPSHOT_ITEMS
            || self.calendar.as_ref().is_some_and(|calendar| {
                calendar.sources.len() > crate::MAX_REFRESH_SOURCES
                    || calendar
                        .sources
                        .iter()
                        .map(|source| source.resources.len())
                        .sum::<usize>()
                        > crate::MAX_REFRESH_CALENDARS
            })
        {
            return Err(crate::DayError::budget("Day snapshot item budget"));
        }
        struct Counter(usize);
        impl std::io::Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .filter(|count| *count <= MAX_DAY_SNAPSHOT_BYTES)
                    .ok_or_else(|| std::io::Error::other("Day snapshot byte budget"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        // Reserve bounded product envelope metadata (including schema version).
        serde_json::to_writer(Counter(64), self)
            .map_err(|_| crate::DayError::budget("Day snapshot byte budget"))
    }
}
