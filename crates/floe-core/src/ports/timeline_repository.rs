use floe_domain::{
    CalendarMirror, Capture, CaptureId, Event, EventId, Note, NoteId, PersonId, Task, TaskId,
    TimelineItem,
};

use crate::CoreError;

#[allow(async_fn_in_trait)]
pub trait TimelineRepository: Send + Sync {
    async fn put_capture(&self, value: &Capture) -> Result<(), CoreError>;
    async fn put_event(&self, value: &Event) -> Result<(), CoreError>;
    async fn put_task(&self, value: &Task) -> Result<(), CoreError>;
    async fn put_note(&self, value: &Note) -> Result<(), CoreError>;
    async fn get_capture(&self, id: CaptureId) -> Result<Option<Capture>, CoreError>;
    async fn get_event(&self, id: EventId) -> Result<Option<Event>, CoreError>;
    async fn get_task(&self, id: TaskId) -> Result<Option<Task>, CoreError>;
    async fn get_note(&self, id: NoteId) -> Result<Option<Note>, CoreError>;
    async fn list_events(&self, person_id: PersonId) -> Result<Vec<Event>, CoreError>;
    async fn list_tasks(&self, person_id: PersonId) -> Result<Vec<Task>, CoreError>;
    async fn list_notes(&self, person_id: PersonId) -> Result<Vec<Note>, CoreError>;
    async fn classify(&self, capture: &Capture, item: &TimelineItem) -> Result<(), CoreError>;
    async fn calendar_mirror(
        &self,
        person_id: PersonId,
    ) -> Result<Option<CalendarMirror>, CoreError>;
}
