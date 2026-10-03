//! Manual Day commands are admitted, checked and projected by Day.
use chrono::{DateTime, Utc};
use uuid::Uuid;
use floe_execution::ExecutionScope;
use floe_kernel::{OwnerActor, PersonId};
use crate::{Capture, CaptureId, Classification, DayError, DayQuery, DayService, DaySnapshot, DayTimelineItem, DomainRef, EventId, EventSchedule, NoteId, Priority, Revision, TaskId, TimelineItem};

pub struct DayMutationRequest {
    pub command_id: Uuid,
    pub day: DayQuery,
    pub mutation: DayMutation,
}

pub enum DayMutation {
    SubmitCapture {
        input: String,
        occurred_at: DateTime<Utc>,
    },
    ClassifyCapture {
        capture_id: CaptureId,
        expected_revision: u64,
        classification: Classification,
        occurred_at: DateTime<Utc>,
    },
    CreateEvent {
        title: String,
        schedule: EventSchedule,
        occurred_at: DateTime<Utc>,
    },
    CreateTask {
        title: String,
        deadline: Option<DateTime<Utc>>,
        priority: Priority,
        occurred_at: DateTime<Utc>,
    },
    CreateNote {
        content: String,
        occurred_at: DateTime<Utc>,
    },
    UpdateEvent {
        event_id: EventId,
        expected_revision: u64,
        title: String,
        schedule: EventSchedule,
        occurred_at: DateTime<Utc>,
    },
    UpdateTask {
        task_id: TaskId,
        expected_revision: u64,
        title: String,
        deadline: Option<DateTime<Utc>>,
        priority: Priority,
        occurred_at: DateTime<Utc>,
    },
    UpdateNote {
        note_id: NoteId,
        expected_revision: u64,
        content: String,
        occurred_at: DateTime<Utc>,
    },
    SetTaskCompletion {
        task_id: TaskId,
        expected_revision: u64,
        completed: bool,
        occurred_at: DateTime<Utc>,
    },
    DeleteItem {
        target: DomainRef,
        expected_revision: u64,
        occurred_at: DateTime<Utc>,
    },
}

pub struct DayMutationResult {
    pub command_id: Uuid,
    pub snapshot: DaySnapshot,
    pub changed_item: Option<DayTimelineItem>,
    pub capture: Option<Capture>,
}


impl DayService {
    pub async fn mutate(&self, actor: &OwnerActor, request: DayMutationRequest, scope: &ExecutionScope) -> Result<DayMutationResult, DayError> {
        self.admit_actor(actor)?;
        if request.command_id.is_nil() { return Err(DayError::validation("invalid command identity")); }
        request.day.range()?;
        request.mutation.validate_bounds()?;
        if scope.cancellation().is_cancelled() || tokio::time::Instant::now() >= scope.deadline() { return Err(DayError::storage("Day command expired")); }
        let person = actor.person_id;
        let mut changed_item = None;
        let mut capture = None;
            match request.mutation {
                DayMutation::SubmitCapture { input, occurred_at } => {
                    capture = Some(self.submit_capture(person, input, occurred_at).await?);
                }
                DayMutation::ClassifyCapture {
                    capture_id,
                    expected_revision,
                    classification,
                    occurred_at,
                } => {
                    let stored = self.repository.get_capture(capture_id)
                        .await?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(
                        self.classify_capture(
                            capture_id,
                            Revision(expected_revision),
                            classification,
                            occurred_at,
                        )
                        .await?,
                    );
                }
                DayMutation::CreateEvent {
                    title,
                    schedule,
                    occurred_at,
                } => {
                    changed_item = Some(TimelineItem::Event(
                        self.create_event(person, title, schedule, occurred_at)
                            .await?,
                    ));
                }
                DayMutation::CreateTask {
                    title,
                    deadline,
                    priority,
                    occurred_at,
                } => {
                    changed_item = Some(TimelineItem::Task(
                        self.create_task(person, title, deadline, priority, occurred_at)
                            .await?,
                    ));
                }
                DayMutation::CreateNote {
                    content,
                    occurred_at,
                } => {
                    changed_item = Some(TimelineItem::Note(
                        self.create_note(person, content, occurred_at).await?,
                    ));
                }
                DayMutation::UpdateEvent {
                    event_id,
                    expected_revision,
                    title,
                    schedule,
                    occurred_at,
                } => {
                    let stored = self.repository.get_event(event_id)
                        .await?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(TimelineItem::Event(
                        self.update_event(
                            event_id,
                            Revision(expected_revision),
                            title,
                            schedule,
                            occurred_at,
                        )
                        .await?,
                    ));
                }
                DayMutation::UpdateTask {
                    task_id,
                    expected_revision,
                    title,
                    deadline,
                    priority,
                    occurred_at,
                } => {
                    let stored = self.repository.get_task(task_id)
                        .await?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(TimelineItem::Task(
                        self.update_task(
                            task_id,
                            Revision(expected_revision),
                            title,
                            deadline,
                            priority,
                            occurred_at,
                        )
                        .await?,
                    ));
                }
                DayMutation::UpdateNote {
                    note_id,
                    expected_revision,
                    content,
                    occurred_at,
                } => {
                    let stored = self.repository.get_note(note_id)
                        .await?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(TimelineItem::Note(
                        self.update_note(
                            note_id,
                            Revision(expected_revision),
                            content,
                            occurred_at,
                        )
                        .await?,
                    ));
                }
                DayMutation::SetTaskCompletion {
                    task_id,
                    expected_revision,
                    completed,
                    occurred_at,
                } => {
                    let stored = self.repository.get_task(task_id)
                        .await?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(TimelineItem::Task(
                        self.set_task_completed(
                            task_id,
                            Revision(expected_revision),
                            completed,
                            occurred_at,
                        )
                        .await?,
                    ));
                }
                DayMutation::DeleteItem {
                    target,
                    expected_revision,
                    occurred_at,
                } => {
                    let owner = match &target {
                        DomainRef::Event(identifier) => {
                            self.repository.get_event(*identifier)
                                .await?
                                .map(|value| value.person_id)
                        }
                        DomainRef::Task(identifier) => {
                            self.repository.get_task(*identifier)
                                .await?
                                .map(|value| value.person_id)
                        }
                        DomainRef::Note(identifier) => {
                            self.repository.get_note(*identifier)
                                .await?
                                .map(|value| value.person_id)
                        }
                    };
                    check_person(owner, person)?;
                    self.delete_item(target, Revision(expected_revision), occurred_at)
                        .await?;
                }
            }
        self.admit_actor(actor)?;
        let changed_item = changed_item.as_ref().map(|item| match item {
            TimelineItem::Event(value) => DayTimelineItem::Event(crate::project_event(value)),
            TimelineItem::Task(value) => DayTimelineItem::Task(crate::project_task(value)),
            TimelineItem::Note(value) => DayTimelineItem::Note(crate::project_note(value)),
        });
        Ok(DayMutationResult { command_id: request.command_id, snapshot: self.snapshot(actor, request.day, scope).await?, changed_item, capture })
    }
}
fn check_person(actual: Option<PersonId>, expected: PersonId) -> Result<(), DayError> {
    if actual == Some(expected) { Ok(()) } else { Err(DayError::not_found("Day item", "requested identity")) }
}

impl DayMutation {
    fn validate_bounds(&self) -> Result<(), DayError> {
        let text = |value: &str| if value.len() <= 1024 * 1024 { Ok(()) } else { Err(DayError::budget("Day command text budget")) };
        let schedule = |value: &EventSchedule| if let EventSchedule::Timed(value) = value { text(&value.timezone) } else { Ok(()) };
        match self {
            Self::SubmitCapture { input, .. } => text(input),
            Self::CreateEvent { title, schedule: value, .. } | Self::UpdateEvent { title, schedule: value, .. } => { text(title)?; schedule(value) },
            Self::CreateTask { title, .. } | Self::UpdateTask { title, .. } => text(title),
            Self::CreateNote { content, .. } | Self::UpdateNote { content, .. } => text(content),
            Self::ClassifyCapture { classification, .. } => match classification { Classification::Event { title, schedule: value } => { text(title)?; schedule(value) }, Classification::Task { title, .. } => text(title), Classification::Note { content } => text(content) },
            _ => Ok(()),
        }
    }
}
