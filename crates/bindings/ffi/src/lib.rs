//! C ABI boundary: raw pointers, JSON envelopes, DTO conversion, memory
//! release and the panic barrier.
//!
//! No product judgment lives here. Every request is parsed into the app's own
//! command and handed to `floe-app`.

mod abi;
mod app_wire;
mod bridge;
pub mod conversion;
mod diagnostics;
mod remote_wire;

pub use abi::*;
pub use bridge::FloeHandle;

use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use chrono::Utc;

use bridge::core_error;
use floe_app::{
    AppComposition, CalendarActionCommand, CalendarActionsResult, CaptureId, DomainRef, EventId,
    NoteId, Revision, TaskId, TimedSchedule, TimelineItem,
};
use floe_protocol::wire::{
    WireResult, agent_failure, check_version, conversion_error, invalid, parse_date, parse_id,
    parse_person, parse_time, protocol_payload,
};
use floe_protocol::*;
use serde::Serialize;
use serde_json::Value;

pub fn local_context(
    handle: &FloeHandle,
    request: LocalContextRequestDto,
) -> WireResult<LocalContextResultDto> {
    let handle = handle.services();
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    let connection = if matches!(
        &request.operation,
        LocalContextOperationDto::PublishCalendarObservation { .. }
    ) {
        Some(
            handle
                .runtime()
                .block_on(handle.core().calendar_connection(person_id))
                .map_err(core_error)?
                .ok_or_else(|| agent_failure(AgentFailure::CapabilityUnavailable))?,
        )
    } else {
        None
    };
    let command = conversion::native::local_context_command(request.operation)?;
    let outcome = handle
        .local_context()
        .execute(person_id, command, connection.as_ref())
        .map_err(agent_failure)?;
    Ok(conversion::native::local_context_result(person_id, outcome))
}

fn c_input<'a>(value: *const c_char, field: &'static str) -> WireResult<&'a str> {
    if value.is_null() {
        return Err(invalid(field, "must not be null"));
    }
    unsafe { CStr::from_ptr(value) }
        .to_str()
        .map_err(|value| invalid(field, value.to_string()))
}

fn c_output(value: impl Serialize) -> *mut c_char {
    let encoded = serde_json::to_string(&value).unwrap_or_else(|_| {
        "{\"schema_version\":1,\"status\":\"error\",\"error\":{\"code\":\"internal\",\"message\":\"response serialization failed\"}}".into()
    });
    CString::new(encoded)
        .expect("JSON cannot contain NUL")
        .into_raw()
}

fn envelope<T: Serialize>(value: WireResult<T>) -> *mut c_char {
    match value {
        Ok(value) => c_output(ResponseEnvelopeDto::ok(value)),
        Err(value) => c_output(ResponseEnvelopeDto::<Value>::error(value)),
    }
}

fn guarded<T: Serialize>(operation: impl FnOnce() -> WireResult<T>) -> *mut c_char {
    diagnostics::initialize();
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(value) => envelope(value),
        Err(payload) => envelope::<T>(Err(diagnostics::panic_error(payload))),
    }
}

fn handle<'a>(value: *mut FloeHandle) -> WireResult<&'a FloeHandle> {
    unsafe { value.as_ref() }.ok_or_else(|| invalid("handle", "must not be null"))
}

fn snapshot(
    handle: &AppComposition,
    person_id: PersonId,
    day: &DayQueryDto,
) -> WireResult<DaySnapshotDto> {
    let date = parse_date(&day.date)?;
    let now = parse_time(&day.now, "day.now")?;
    let value = handle
        .runtime()
        .block_on(handle.core().day_snapshot_with_end_offset(
            person_id,
            date,
            day.timezone_offset_seconds,
            day.end_timezone_offset_seconds,
            now,
        ))
        .map_err(core_error)?;
    conversion::day_snapshot_to_dto(value).map_err(conversion_error)
}

pub fn load_day(handle: &FloeHandle, request: LoadDayRequestDto) -> WireResult<DaySnapshotDto> {
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    snapshot(handle.services(), person_id, &request.day)
}

pub fn agent_fixture(
    handle: &FloeHandle,
    request: AgentFixtureRequestDto,
) -> WireResult<AgentFixtureResultDto> {
    let handle = handle.services();
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    let session = match request.operation {
        AgentFixtureOperationDto::Resume {} => handle
            .runtime()
            .block_on(handle.core().resume_agent_fixture(person_id)),
        AgentFixtureOperationDto::Start {} => handle
            .runtime()
            .block_on(handle.core().start_agent_fixture(person_id)),
        AgentFixtureOperationDto::Get { session_id } => {
            let session_id = parse_id(&session_id, "session_id", |value| value)?;
            handle
                .runtime()
                .block_on(handle.core().agent_fixture_session(person_id, session_id))
        }
        AgentFixtureOperationDto::Recover {
            session_id,
            expected_revision,
        } => {
            let session_id = parse_id(&session_id, "session_id", |value| value)?;
            handle
                .agent_runs()
                .ensure_idle(person_id, session_id)
                .map_err(agent_failure)?;
            handle
                .runtime()
                .block_on(handle.core().recover_agent_fixture(
                    person_id,
                    session_id,
                    expected_revision,
                ))
        }
        AgentFixtureOperationDto::Turn {
            session_id,
            expected_revision,
            prompt,
        } => {
            let session_id = parse_id(&session_id, "session_id", |value| value)?;
            handle
                .agent_runs()
                .ensure_idle(person_id, session_id)
                .map_err(agent_failure)?;
            let prompt = fixture_prompt(prompt);
            let result = handle
                .runtime()
                .block_on(handle.core().run_agent_fixture(
                    person_id,
                    session_id,
                    expected_revision,
                    prompt,
                ))
                .map_err(agent_failure)?;
            return Ok(AgentFixtureResultDto {
                session: protocol_payload(&result.session)?,
                events: result
                    .events
                    .iter()
                    .map(protocol_payload)
                    .collect::<Result<Vec<_>, _>>()?,
            });
        }
    }
    .map_err(agent_failure)?;
    Ok(AgentFixtureResultDto {
        session: protocol_payload(&session)?,
        events: vec![],
    })
}

/// Read one calendar-action request off the wire and hand the app the command
/// it states. Which provider carries it out is the app's to resolve.
pub fn calendar_actions(
    handle: &FloeHandle,
    request: CalendarActionRequestDto,
) -> WireResult<CalendarActionsResult> {
    let handle = handle.services();
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    let command = match request.operation {
        CalendarActionOperationDto::Capabilities {} => CalendarActionCommand::Capabilities,
        CalendarActionOperationDto::GetAuthority {} => CalendarActionCommand::GetAuthority,
        CalendarActionOperationDto::SetAuthority { .. } => CalendarActionCommand::SetAuthority,
        CalendarActionOperationDto::List {} => CalendarActionCommand::List,
        CalendarActionOperationDto::Get { action_id } => CalendarActionCommand::Get {
            action_id: parse_id(&action_id, "action_id", |value| value)?,
        },
        CalendarActionOperationDto::Execute { action_id } => CalendarActionCommand::Execute {
            action_id: parse_id(&action_id, "action_id", |value| value)?,
        },
        CalendarActionOperationDto::Recover { action_id } => CalendarActionCommand::Recover {
            action_id: parse_id(&action_id, "action_id", |value| value)?,
        },
        CalendarActionOperationDto::Decide {
            action_id,
            decision,
        } => CalendarActionCommand::Decide {
            action_id: parse_id(&action_id, "action_id", |value| value)?,
            approve: decision == CalendarActionDecisionDto::Approve,
        },
        CalendarActionOperationDto::Propose {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
        } => CalendarActionCommand::Propose {
            calendar_id,
            title,
            schedule: timed_schedule(&starts_at, &ends_at, &timezone)?,
        },
        CalendarActionOperationDto::Direct {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
            event_id,
            event_revision,
            delete,
        } => {
            let event_id = event_id
                .map(|value| parse_id(&value, "event_id", EventId))
                .transpose()?;
            let target = match (event_id, event_revision) {
                (Some(id), Some(revision)) => Some((id, Revision(revision))),
                (None, None) => None,
                _ => {
                    return Err(invalid(
                        "event_revision",
                        "target identity and revision are required together",
                    ));
                }
            };
            CalendarActionCommand::Direct {
                calendar_id,
                title,
                schedule: timed_schedule(&starts_at, &ends_at, &timezone)?,
                target,
                delete,
            }
        }
    };
    handle
        .runtime()
        .block_on(
            handle
                .core()
                .calendar_action_command(person_id, command, Utc::now()),
        )
        .map_err(core_error)
}

fn timed_schedule(starts_at: &str, ends_at: &str, timezone: &str) -> WireResult<TimedSchedule> {
    TimedSchedule::new(
        parse_time(starts_at, "starts_at")?,
        parse_time(ends_at, "ends_at")?,
        timezone,
    )
    .map_err(|value| invalid("schedule", value.to_string()))
}

pub fn execute(handle: &FloeHandle, request: CommandRequestDto) -> WireResult<MutationResultDto> {
    let handle = handle.services();
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    parse_date(&request.day.date)?;
    let request_now = parse_time(&request.day.now, "day.now")?;
    let mut changed_item = None;
    let mut capture = None;

    match request.command {
        CommandDto::DisconnectCalendar { expected_revision } => {
            handle
                .runtime()
                .block_on(
                    handle
                        .core()
                        .disconnect_calendar(person_id, expected_revision),
                )
                .map_err(core_error)?;
        }
        CommandDto::SetCalendarScope {
            connection_id,
            connection_revision,
            device_id,
            provider,
            calendars,
            scope,
        } => {
            handle
                .runtime()
                .block_on(
                    handle.core().set_calendar_scope(
                        person_id,
                        connection_id,
                        connection_revision,
                        device_id,
                        conversion::calendar_provider_from_dto(provider),
                        calendars
                            .into_iter()
                            .map(conversion::calendar_selection_from_dto)
                            .collect(),
                        conversion::calendar_scope_from_dto(scope),
                    ),
                )
                .map_err(core_error)?;
        }
        CommandDto::DiscoverCalendars {
            expected_revision,
            calendars,
        } => {
            handle
                .runtime()
                .block_on(
                    handle.core().discover_calendars(
                        person_id,
                        expected_revision,
                        calendars
                            .into_iter()
                            .map(conversion::calendar_selection_from_dto)
                            .collect(),
                    ),
                )
                .map_err(core_error)?;
        }
        CommandDto::ImportCalendarSources {
            expected_revision,
            range,
            batches,
            occurred_at,
        } => {
            let batches = batches
                .into_iter()
                .map(conversion::calendar_batch_from_dto)
                .collect();
            handle
                .runtime()
                .block_on(handle.core().import_calendar_sources(
                    person_id,
                    expected_revision,
                    conversion::calendar_range_from_dto(range),
                    batches,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
        }
        CommandDto::ImportCalendar {
            expected_revision,
            range,
            records,
            occurred_at,
        } => {
            let records = records
                .into_iter()
                .map(conversion::calendar_record_from_dto)
                .collect::<Result<Vec<_>, _>>()
                .map_err(conversion_error)?;
            handle
                .runtime()
                .block_on(handle.core().import_calendar(
                    person_id,
                    expected_revision,
                    conversion::calendar_range_from_dto(range),
                    records,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
        }
        CommandDto::CalendarFailed {
            expected_revision,
            failure,
        } => {
            handle
                .runtime()
                .block_on(handle.core().record_calendar_failure(
                    person_id,
                    expected_revision,
                    conversion::calendar_failure_from_dto(failure),
                    request_now,
                ))
                .map_err(core_error)?;
        }
        CommandDto::SubmitCapture { input, occurred_at } => {
            let occurred_at = parse_time(&occurred_at, "occurred_at")?;
            let value = handle
                .runtime()
                .block_on(handle.core().submit_capture(person_id, input, occurred_at))
                .map_err(core_error)?;
            capture = Some(conversion::capture_to_dto(value));
        }
        CommandDto::ClassifyCapture {
            capture_id,
            expected_revision,
            classification,
            occurred_at,
        } => {
            let capture_id = parse_id(&capture_id, "capture_id", CaptureId)?;
            let classification =
                conversion::classification_from_dto(classification).map_err(conversion_error)?;
            let value = handle
                .runtime()
                .block_on(handle.core().classify_capture(
                    capture_id,
                    Revision(expected_revision),
                    classification,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(conversion::timeline_item_to_dto(value));
        }
        CommandDto::CreateEvent {
            title,
            schedule,
            occurred_at,
        } => {
            let value = handle
                .runtime()
                .block_on(handle.core().create_event(
                    person_id,
                    title,
                    conversion::event_schedule_from_dto(schedule).map_err(conversion_error)?,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(conversion::timeline_item_to_dto(TimelineItem::Event(value)));
        }
        CommandDto::CreateTask {
            title,
            deadline,
            priority,
            occurred_at,
        } => {
            let value = handle
                .runtime()
                .block_on(
                    handle.core().create_task(
                        person_id,
                        title,
                        deadline
                            .as_deref()
                            .map(|value| parse_time(value, "deadline"))
                            .transpose()?,
                        conversion::priority_from_dto(priority),
                        parse_time(&occurred_at, "occurred_at")?,
                    ),
                )
                .map_err(core_error)?;
            changed_item = Some(conversion::timeline_item_to_dto(TimelineItem::Task(value)));
        }
        CommandDto::CreateNote {
            content,
            occurred_at,
        } => {
            let value = handle
                .runtime()
                .block_on(handle.core().create_note(
                    person_id,
                    content,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(conversion::timeline_item_to_dto(TimelineItem::Note(value)));
        }
        CommandDto::UpdateEvent {
            event_id,
            expected_revision,
            title,
            schedule,
            occurred_at,
        } => {
            let value = handle
                .runtime()
                .block_on(handle.core().update_event(
                    parse_id(&event_id, "event_id", EventId)?,
                    Revision(expected_revision),
                    title,
                    conversion::event_schedule_from_dto(schedule).map_err(conversion_error)?,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(conversion::timeline_item_to_dto(TimelineItem::Event(value)));
        }
        CommandDto::UpdateTask {
            task_id,
            expected_revision,
            title,
            deadline,
            priority,
            occurred_at,
        } => {
            let value = handle
                .runtime()
                .block_on(
                    handle.core().update_task(
                        parse_id(&task_id, "task_id", TaskId)?,
                        Revision(expected_revision),
                        title,
                        deadline
                            .as_deref()
                            .map(|value| parse_time(value, "deadline"))
                            .transpose()?,
                        conversion::priority_from_dto(priority),
                        parse_time(&occurred_at, "occurred_at")?,
                    ),
                )
                .map_err(core_error)?;
            changed_item = Some(conversion::timeline_item_to_dto(TimelineItem::Task(value)));
        }
        CommandDto::UpdateNote {
            note_id,
            expected_revision,
            content,
            occurred_at,
        } => {
            let value = handle
                .runtime()
                .block_on(handle.core().update_note(
                    parse_id(&note_id, "note_id", NoteId)?,
                    Revision(expected_revision),
                    content,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(conversion::timeline_item_to_dto(TimelineItem::Note(value)));
        }
        CommandDto::SetTaskCompletion {
            task_id,
            expected_revision,
            completed,
            occurred_at,
        } => {
            let value = handle
                .runtime()
                .block_on(handle.core().set_task_completed(
                    parse_id(&task_id, "task_id", TaskId)?,
                    Revision(expected_revision),
                    completed,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(conversion::timeline_item_to_dto(TimelineItem::Task(value)));
        }
        CommandDto::DeleteItem {
            target,
            expected_revision,
            occurred_at,
        } => {
            let reference: DomainRef =
                conversion::domain_ref_from_dto(target).map_err(conversion_error)?;
            handle
                .runtime()
                .block_on(handle.core().delete_item(
                    reference,
                    Revision(expected_revision),
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
        }
    }

    Ok(MutationResultDto {
        snapshot: snapshot(handle, person_id, &request.day)?,
        changed_item,
        capture,
    })
}

/// The scripted prompt one fixture request names.
fn fixture_prompt(prompt: AgentFixturePromptDto) -> floe_app::AgentFixturePrompt {
    match prompt {
        AgentFixturePromptDto::Today => floe_app::AgentFixturePrompt::Today,
        AgentFixturePromptDto::FollowUp => floe_app::AgentFixturePrompt::FollowUp,
        AgentFixturePromptDto::RepeatedCall => floe_app::AgentFixturePrompt::RepeatedCall,
        AgentFixturePromptDto::Unavailable => floe_app::AgentFixturePrompt::Unavailable,
    }
}

/// Read one scripted-run request off the wire, and state where the run got to.
pub fn agent_fixture_run(
    handle: &FloeHandle,
    request: AgentFixtureRunRequestDto,
) -> WireResult<AgentFixtureRunDto> {
    check_version(request.schema_version)?;
    let command = match request.operation {
        AgentFixtureRunOperationDto::Begin { prompt } => floe_app::AgentFixtureRunCommand::Begin {
            prompt: fixture_prompt(prompt),
        },
        AgentFixtureRunOperationDto::Poll { after_sequence } => {
            floe_app::AgentFixtureRunCommand::Poll { after_sequence }
        }
        AgentFixtureRunOperationDto::Stop {} => floe_app::AgentFixtureRunCommand::Stop,
        AgentFixtureRunOperationDto::Release {} => floe_app::AgentFixtureRunCommand::Release,
    };
    let snapshot = floe_app::agent_run::run(
        handle.services(),
        floe_app::AgentFixtureRunRequest {
            person_id: parse_person(&request.person_id)?,
            session_id: parse_id(&request.session_id, "session_id", |value| value)?,
            expected_revision: request.expected_revision,
            command,
        },
    )
    .map_err(agent_failure)?;
    Ok(AgentFixtureRunDto {
        session_id: snapshot.session_id.to_string(),
        expected_revision: snapshot.expected_revision,
        events: snapshot
            .events
            .iter()
            .map(protocol_payload)
            .collect::<Result<Vec<_>, _>>()?,
        next_sequence: snapshot.next_sequence,
        done: snapshot.done,
        session: snapshot
            .session
            .as_ref()
            .map(protocol_payload)
            .transpose()?,
        failure: snapshot
            .failure
            .as_ref()
            .map(protocol_payload)
            .transpose()?,
    })
}
