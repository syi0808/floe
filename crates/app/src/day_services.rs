use crate::{
    AppComposition, CalendarBatch, CalendarFailure, CalendarProvider, CalendarRange,
    CalendarRecord, CalendarScope, CalendarSelection, CallerContext, CaptureId, Classification,
    CoreError, DomainRef, ErrorCode, EventId, EventSchedule, NoteId, PersonId, Priority, Revision,
    TaskId, TimelineItem,
};
use chrono::{DateTime, NaiveDate, Utc};
use floe_day::TimelineRepository;
use uuid::Uuid;

pub struct DayRead {
    pub date: NaiveDate,
    pub timezone_offset_seconds: i32,
    pub end_timezone_offset_seconds: Option<i32>,
    pub now: DateTime<Utc>,
}

pub struct DayMutationRequest {
    pub command_id: Uuid,
    pub day: DayRead,
    pub mutation: DayMutation,
}

pub enum DayMutation {
    DisconnectCalendar {
        expected_revision: u64,
    },
    SetCalendarScope {
        connection_id: String,
        connection_revision: u64,
        provider: CalendarProvider,
        calendars: Vec<CalendarSelection>,
        scope: CalendarScope,
    },
    DiscoverCalendars {
        expected_revision: u64,
        calendars: Vec<CalendarSelection>,
    },
    ImportCalendarSources {
        expected_revision: u64,
        range: CalendarRange,
        batches: Vec<CalendarBatch>,
        occurred_at: DateTime<Utc>,
    },
    ImportCalendar {
        expected_revision: u64,
        range: CalendarRange,
        records: Vec<CalendarRecord>,
        occurred_at: DateTime<Utc>,
    },
    CalendarFailed {
        expected_revision: u64,
        failure: CalendarFailure,
    },
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
    pub snapshot: crate::DaySnapshot,
    pub changed_item: Option<TimelineItem>,
    pub capture: Option<crate::Capture>,
}

pub trait DayCommands {
    fn mutate_day(
        &self,
        caller: &CallerContext,
        request: DayMutationRequest,
    ) -> Result<DayMutationResult, CoreError>;
}

pub trait DayQueries {
    fn read_day(
        &self,
        caller: &CallerContext,
        request: DayRead,
    ) -> Result<crate::DaySnapshot, CoreError>;
}

impl DayQueries for AppComposition {
    fn read_day(
        &self,
        caller: &CallerContext,
        request: DayRead,
    ) -> Result<crate::DaySnapshot, CoreError> {
        self.runtime
            .block_on(self.core.day_snapshot_with_end_offset(
                PersonId(caller.person_id()),
                request.date,
                request.timezone_offset_seconds,
                request.end_timezone_offset_seconds,
                request.now,
            ))
    }
}

impl DayCommands for AppComposition {
    fn mutate_day(
        &self,
        caller: &CallerContext,
        request: DayMutationRequest,
    ) -> Result<DayMutationResult, CoreError> {
        if request.command_id.is_nil() {
            return Err(CoreError::new(
                ErrorCode::Validation,
                "invalid command identity",
            ));
        }
        let end_date = request
            .day
            .date
            .succ_opt()
            .ok_or_else(|| CoreError::new(ErrorCode::Validation, "invalid day"))?;
        if !(CalendarRange {
            start_date: request.day.date,
            end_date_exclusive: end_date,
            timezone_offset_seconds: request.day.timezone_offset_seconds,
            end_timezone_offset_seconds: request.day.end_timezone_offset_seconds,
        })
        .is_valid()
        {
            return Err(CoreError::new(ErrorCode::Validation, "invalid day offsets"));
        }
        let person = PersonId(caller.person_id());
        let core = &self.core;
        self.runtime.block_on(async {
            let mut changed_item = None;
            let mut capture = None;
            match request.mutation {
                DayMutation::DisconnectCalendar { expected_revision } => {
                    core.disconnect_calendar(person, expected_revision).await?;
                }
                DayMutation::SetCalendarScope {
                    connection_id,
                    connection_revision,
                    provider,
                    calendars,
                    scope,
                } => {
                    core.set_calendar_scope(
                        person,
                        connection_id,
                        connection_revision,
                        caller.device_id().into(),
                        provider,
                        calendars,
                        scope,
                    )
                    .await?;
                }
                DayMutation::DiscoverCalendars {
                    expected_revision,
                    calendars,
                } => {
                    core.discover_calendars(person, expected_revision, calendars)
                        .await?;
                }
                DayMutation::ImportCalendarSources {
                    expected_revision,
                    range,
                    batches,
                    occurred_at,
                } => {
                    core.import_calendar_sources(
                        person,
                        expected_revision,
                        range,
                        batches,
                        occurred_at,
                    )
                    .await?;
                }
                DayMutation::ImportCalendar {
                    expected_revision,
                    range,
                    records,
                    occurred_at,
                } => {
                    core.import_calendar(person, expected_revision, range, records, occurred_at)
                        .await?;
                }
                DayMutation::CalendarFailed {
                    expected_revision,
                    failure,
                } => {
                    core.record_calendar_failure(
                        person,
                        expected_revision,
                        failure,
                        request.day.now,
                    )
                    .await?;
                }
                DayMutation::SubmitCapture { input, occurred_at } => {
                    capture = Some(core.submit_capture(person, input, occurred_at).await?);
                }
                DayMutation::ClassifyCapture {
                    capture_id,
                    expected_revision,
                    classification,
                    occurred_at,
                } => {
                    let stored = TimelineRepository::get_capture(&core.store, capture_id)
                        .await
                        .map_err(crate::core::day_error)?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(
                        core.classify_capture(
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
                        core.create_event(person, title, schedule, occurred_at)
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
                        core.create_task(person, title, deadline, priority, occurred_at)
                            .await?,
                    ));
                }
                DayMutation::CreateNote {
                    content,
                    occurred_at,
                } => {
                    changed_item = Some(TimelineItem::Note(
                        core.create_note(person, content, occurred_at).await?,
                    ));
                }
                DayMutation::UpdateEvent {
                    event_id,
                    expected_revision,
                    title,
                    schedule,
                    occurred_at,
                } => {
                    let stored = TimelineRepository::get_event(&core.store, event_id)
                        .await
                        .map_err(crate::core::day_error)?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(TimelineItem::Event(
                        core.update_event(
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
                    let stored = TimelineRepository::get_task(&core.store, task_id)
                        .await
                        .map_err(crate::core::day_error)?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(TimelineItem::Task(
                        core.update_task(
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
                    let stored = TimelineRepository::get_note(&core.store, note_id)
                        .await
                        .map_err(crate::core::day_error)?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(TimelineItem::Note(
                        core.update_note(
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
                    let stored = TimelineRepository::get_task(&core.store, task_id)
                        .await
                        .map_err(crate::core::day_error)?;
                    check_person(stored.map(|value| value.person_id), person)?;
                    changed_item = Some(TimelineItem::Task(
                        core.set_task_completed(
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
                            TimelineRepository::get_event(&core.store, *identifier)
                                .await
                                .map_err(crate::core::day_error)?
                                .map(|value| value.person_id)
                        }
                        DomainRef::Task(identifier) => {
                            TimelineRepository::get_task(&core.store, *identifier)
                                .await
                                .map_err(crate::core::day_error)?
                                .map(|value| value.person_id)
                        }
                        DomainRef::Note(identifier) => {
                            TimelineRepository::get_note(&core.store, *identifier)
                                .await
                                .map_err(crate::core::day_error)?
                                .map(|value| value.person_id)
                        }
                    };
                    check_person(owner, person)?;
                    core.delete_item(target, Revision(expected_revision), occurred_at)
                        .await?;
                }
            }
            Ok(DayMutationResult {
                command_id: request.command_id,
                snapshot: core
                    .day_snapshot_with_end_offset(
                        person,
                        request.day.date,
                        request.day.timezone_offset_seconds,
                        request.day.end_timezone_offset_seconds,
                        request.day.now,
                    )
                    .await?,
                changed_item,
                capture,
            })
        })
    }
}

fn check_person(actual: Option<PersonId>, expected: PersonId) -> Result<(), CoreError> {
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(CoreError::new(ErrorCode::NotFound, "item not found"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admitted_day_preserves_person_and_revision_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let person = PersonId::new();
        let path = directory
            .path()
            .join("people")
            .join(person.to_string())
            .join("floe.db");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(directory.path().join("local_device_id"), "verified-mac").unwrap();
        let host = crate::composition::open(path.to_str().unwrap()).unwrap();
        let admitted = host.request(Uuid::new_v4()).unwrap();
        let caller = admitted.caller();
        let services = admitted.services();
        let now = Utc::now();
        let read = || DayRead {
            date: now.date_naive(),
            timezone_offset_seconds: 0,
            end_timezone_offset_seconds: None,
            now,
        };
        let created = services
            .mutate_day(
                caller,
                DayMutationRequest {
                    command_id: Uuid::new_v4(),
                    day: read(),
                    mutation: DayMutation::CreateNote {
                        content: "owned".into(),
                        occurred_at: now,
                    },
                },
            )
            .unwrap();
        let TimelineItem::Note(note) = created.changed_item.unwrap() else {
            panic!("expected Note")
        };
        assert_eq!(note.person_id, person);
        let foreign = CallerContext::verified(
            crate::LocalIdentityClaim {
                person_id: Uuid::new_v4(),
                device_id: caller.device_id().into(),
            },
            caller.runtime_epoch(),
        )
        .unwrap();
        for (identity, revision, code) in [
            (&foreign, note.revision.0, ErrorCode::NotFound),
            (caller, note.revision.0 + 1, ErrorCode::Conflict),
        ] {
            let result = services.mutate_day(
                identity,
                DayMutationRequest {
                    command_id: Uuid::new_v4(),
                    day: read(),
                    mutation: DayMutation::UpdateNote {
                        note_id: note.id,
                        expected_revision: revision,
                        content: "unauthorized".into(),
                        occurred_at: now,
                    },
                },
            );
            assert_eq!(result.err().unwrap().code, code);
        }
        let stored = services
            .runtime
            .block_on(TimelineRepository::get_note(&services.core.store, note.id))
            .unwrap()
            .unwrap();
        assert_eq!(stored, note);
    }
}
