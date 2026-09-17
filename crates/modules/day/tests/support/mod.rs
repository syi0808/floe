//! An in-memory [`TimelineRepository`] for Day's own tests.
//!
//! Day owns calendar, task and note state; it does not own storage. These tests
//! drive [`DayService`](floe_day::DayService) against this map so that what they
//! assert is Day's state semantics and not a database's. Transactions, CAS and
//! encryption are the Vault adapter's tests to make.

// Each integration test binary compiles this module on its own, so a helper only
// some of them need would otherwise read as dead code.
#![allow(dead_code)]

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use floe_day::{
    CalendarMirror, Capture, CaptureId, DayError, Event, EventId, Note, NoteId, PersonId, Revision,
    Task, TaskId, TimelineItem, TimelineRepository,
};

#[derive(Default)]
struct State {
    captures: HashMap<CaptureId, Capture>,
    events: HashMap<EventId, Event>,
    tasks: HashMap<TaskId, Task>,
    notes: HashMap<NoteId, Note>,
    mirrors: HashMap<PersonId, CalendarMirror>,
    classifications: Vec<(CaptureId, TimelineItem)>,
}

/// The stored timeline one test owns.
#[derive(Default)]
pub struct TestTimelineRepository {
    state: Mutex<State>,
}

impl TestTimelineRepository {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> Result<MutexGuard<'_, State>, DayError> {
        self.state
            .lock()
            .map_err(|_| DayError::storage("test timeline poisoned"))
    }

    /// What `classify_capture` linked, in the order it linked them.
    pub fn classifications(&self) -> Vec<(CaptureId, TimelineItem)> {
        self.state().expect("timeline").classifications.clone()
    }
}

/// Reject a write whose expected revision is not the stored one, the way a
/// compare-and-swap store does, so Day's stale-revision paths are exercised.
fn check_revision(stored: Option<Revision>, expected: Revision) -> Result<(), DayError> {
    match stored {
        Some(actual) if actual != expected => Err(DayError::stale(expected, actual)),
        _ => Ok(()),
    }
}

impl TimelineRepository for TestTimelineRepository {
    async fn put_capture(&self, value: &Capture) -> Result<(), DayError> {
        self.state()?.captures.insert(value.id, value.clone());
        Ok(())
    }

    async fn put_event(&self, value: &Event) -> Result<(), DayError> {
        self.state()?.events.insert(value.id, value.clone());
        Ok(())
    }

    async fn put_event_if_revision(
        &self,
        value: &Event,
        expected: Revision,
    ) -> Result<(), DayError> {
        let mut state = self.state()?;
        check_revision(
            state.events.get(&value.id).map(|event| event.revision),
            expected,
        )?;
        state.events.insert(value.id, value.clone());
        Ok(())
    }

    async fn put_task(&self, value: &Task) -> Result<(), DayError> {
        self.state()?.tasks.insert(value.id, value.clone());
        Ok(())
    }

    async fn put_task_if_revision(&self, value: &Task, expected: Revision) -> Result<(), DayError> {
        let mut state = self.state()?;
        check_revision(
            state.tasks.get(&value.id).map(|task| task.revision),
            expected,
        )?;
        state.tasks.insert(value.id, value.clone());
        Ok(())
    }

    async fn put_note(&self, value: &Note) -> Result<(), DayError> {
        self.state()?.notes.insert(value.id, value.clone());
        Ok(())
    }

    async fn put_note_if_revision(&self, value: &Note, expected: Revision) -> Result<(), DayError> {
        let mut state = self.state()?;
        check_revision(
            state.notes.get(&value.id).map(|note| note.revision),
            expected,
        )?;
        state.notes.insert(value.id, value.clone());
        Ok(())
    }

    async fn get_capture(&self, id: CaptureId) -> Result<Option<Capture>, DayError> {
        Ok(self.state()?.captures.get(&id).cloned())
    }

    async fn get_event(&self, id: EventId) -> Result<Option<Event>, DayError> {
        Ok(self.state()?.events.get(&id).cloned())
    }

    async fn get_task(&self, id: TaskId) -> Result<Option<Task>, DayError> {
        Ok(self.state()?.tasks.get(&id).cloned())
    }

    async fn get_note(&self, id: NoteId) -> Result<Option<Note>, DayError> {
        Ok(self.state()?.notes.get(&id).cloned())
    }

    async fn list_events(&self, person_id: PersonId) -> Result<Vec<Event>, DayError> {
        Ok(self
            .state()?
            .events
            .values()
            .filter(|event| event.person_id == person_id)
            .cloned()
            .collect())
    }

    async fn list_tasks(&self, person_id: PersonId) -> Result<Vec<Task>, DayError> {
        Ok(self
            .state()?
            .tasks
            .values()
            .filter(|task| task.person_id == person_id)
            .cloned()
            .collect())
    }

    async fn list_notes(&self, person_id: PersonId) -> Result<Vec<Note>, DayError> {
        Ok(self
            .state()?
            .notes
            .values()
            .filter(|note| note.person_id == person_id)
            .cloned()
            .collect())
    }

    async fn classify(&self, capture: &Capture, item: &TimelineItem) -> Result<(), DayError> {
        let mut state = self.state()?;
        state.captures.insert(capture.id, capture.clone());
        match item {
            TimelineItem::Event(event) => {
                state.events.insert(event.id, event.clone());
            }
            TimelineItem::Task(task) => {
                state.tasks.insert(task.id, task.clone());
            }
            TimelineItem::Note(note) => {
                state.notes.insert(note.id, note.clone());
            }
        }
        state.classifications.push((capture.id, item.clone()));
        Ok(())
    }

    async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarMirror>, DayError> {
        Ok(self.state()?.mirrors.get(&person_id).cloned())
    }

    async fn put_calendar_mirror(
        &self,
        person_id: PersonId,
        mirror: &CalendarMirror,
        previous: Option<&CalendarMirror>,
    ) -> Result<(), DayError> {
        let mut state = self.state()?;
        let stored = state.mirrors.get(&person_id);
        // The store admits the write only against the mirror the caller read.
        match (stored, previous) {
            (Some(stored), Some(previous))
                if stored.connection.revision != previous.connection.revision =>
            {
                return Err(DayError::conflict("stale calendar mirror"));
            }
            (Some(_), None) => return Err(DayError::conflict("calendar mirror already exists")),
            (None, Some(_)) => return Err(DayError::not_found("calendar mirror", person_id.0)),
            _ => {}
        }
        state.mirrors.insert(person_id, mirror.clone());
        Ok(())
    }
}
