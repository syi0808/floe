use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::{Event, EventId, EventSchedule, Note, PersonId, Task};

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
    pub items: Vec<TimelineItem>,
}

pub fn project_day(
    person_id: PersonId,
    date: NaiveDate,
    timezone_offset_seconds: i32,
    now: DateTime<Utc>,
    mut events: Vec<Event>,
    mut tasks: Vec<Task>,
    mut notes: Vec<Note>,
) -> DaySnapshot {
    let day_start =
        DateTime::<Utc>::from_naive_utc_and_offset(date.and_hms_opt(0, 0, 0).unwrap(), Utc)
            - chrono::Duration::seconds(i64::from(timezone_offset_seconds));
    let day_end = day_start + chrono::Duration::days(1);
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
    let mut items = Vec::with_capacity(events.len() + tasks.len() + notes.len());
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
    DaySnapshot {
        person_id,
        date,
        generated_at: now,
        timezone_offset_seconds,
        now_event_id,
        next_event_id,
        overdue_task_count,
        items,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::{EventSchedule, Priority, SourceRef, TimedSchedule};

    #[test]
    fn now_next_and_overdue_are_deterministic() {
        let person_id = PersonId::new();
        let now = Utc.with_ymd_and_hms(2026, 9, 2, 10, 30, 0).unwrap();
        let current = Event::new(
            person_id,
            "Current",
            EventSchedule::Timed(
                TimedSchedule::new(
                    now - chrono::Duration::minutes(30),
                    now + chrono::Duration::minutes(30),
                    "UTC",
                )
                .unwrap(),
            ),
            SourceRef::Manual,
            now,
        )
        .unwrap();
        let next = Event::new(
            person_id,
            "Next",
            EventSchedule::Timed(
                TimedSchedule::new(
                    now + chrono::Duration::hours(1),
                    now + chrono::Duration::hours(2),
                    "UTC",
                )
                .unwrap(),
            ),
            SourceRef::Manual,
            now,
        )
        .unwrap();
        let overdue = Task::new(
            person_id,
            "Late",
            Some(now - chrono::Duration::days(1)),
            Priority::Normal,
            SourceRef::Manual,
            now,
        )
        .unwrap();
        let snapshot = project_day(
            person_id,
            now.date_naive(),
            0,
            now,
            vec![next.clone(), current.clone()],
            vec![overdue],
            vec![],
        );
        assert_eq!(snapshot.now_event_id, Some(current.id));
        assert_eq!(snapshot.next_event_id, Some(next.id));
        assert_eq!(snapshot.overdue_task_count, 1);
    }
}
