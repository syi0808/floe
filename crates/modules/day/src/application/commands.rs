use crate::{DayError, DayRepository, Event, Note, Task, TimelineItem};
use floe_kernel::PersonId;

#[derive(Clone)]
pub struct DayService {
    pub(crate) repository: std::sync::Arc<dyn DayRepository>,
    pub(crate) acquisition: std::sync::Arc<dyn crate::CalendarAcquisitionPort>,
    pub(crate) clock: std::sync::Arc<dyn crate::DayClock>,
    pub(crate) lifecycle: std::sync::Arc<super::refresh::DayLifecycle>,
}

impl DayService {
    pub fn new(
        repository: std::sync::Arc<dyn DayRepository>,
        acquisition: std::sync::Arc<dyn crate::CalendarAcquisitionPort>,
        clock: std::sync::Arc<dyn crate::DayClock>,
    ) -> Self {
        Self {
            repository,
            acquisition,
            clock,
            lifecycle: std::sync::Arc::new(super::refresh::DayLifecycle::new()),
        }
    }

    pub(crate) async fn selected_day_items(
        &self,
        person_id: PersonId,
        query: &crate::DayQuery,
    ) -> Result<(Vec<Event>, Vec<Task>, Vec<Note>), DayError> {
        let values = self
            .repository
            .read_items(crate::DayReadQuery::display(person_id, query)?)
            .await?;
        let mut events = Vec::new();
        let mut tasks = Vec::new();
        let mut notes = Vec::new();
        for item in values {
            match item {
                TimelineItem::Event(value) => events.push(value),
                TimelineItem::Task(value) => tasks.push(value),
                TimelineItem::Note(value) => notes.push(value),
            }
        }
        Ok((events, tasks, notes))
    }
}
