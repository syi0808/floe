//! C ABI boundary: raw pointers, JSON envelopes, DTO conversion, memory
//! release and the panic barrier.
//!
//! No product judgment lives here. Every request is handed to `floe-app`.

mod abi;
mod app_wire;
mod diagnostics;

pub use abi::*;

use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use chrono::{DateTime, Utc};
use floe_app::modules::day::{Classification, DomainRef, Revision, TimelineItem};
use floe_app::modules::kernel::PersonId;
use uuid::Uuid;

use floe_app::{
    AppComposition, AppOpenError, BridgeResult, FloeHandle, agent_failure, check_version, conversion_error, core_error, error,
    host_error, invalid, action_error, parse_date, parse_id, parse_person, parse_time, protocol_payload,
    unsupported_version,
};
use floe_protocol::*;
use serde::Serialize;
use serde_json::Value;


pub fn local_context(
    handle: &FloeHandle,
    request: LocalContextRequestDto,
) -> BridgeResult<LocalContextResultDto> {
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
                .ok_or_else(|| agent_failure(floe_app::modules::agent_contract::AgentFailure::CapabilityUnavailable))?,
        )
    } else {
        None
    };
    handle
        .local_context()
        .request_bound(person_id, request.operation, connection.as_ref())
}

fn c_input<'a>(value: *const c_char, field: &'static str) -> BridgeResult<&'a str> {
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

fn envelope<T: Serialize>(value: BridgeResult<T>) -> *mut c_char {
    match value {
        Ok(value) => c_output(ResponseEnvelopeDto::ok(value)),
        Err(value) => c_output(ResponseEnvelopeDto::<Value>::error(value)),
    }
}

fn guarded<T: Serialize>(operation: impl FnOnce() -> BridgeResult<T>) -> *mut c_char {
    diagnostics::initialize();
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(value) => envelope(value),
        Err(payload) => envelope::<T>(Err(diagnostics::panic_error(payload))),
    }
}

fn handle<'a>(value: *mut FloeHandle) -> BridgeResult<&'a FloeHandle> {
    unsafe { value.as_ref() }.ok_or_else(|| invalid("handle", "must not be null"))
}

fn snapshot(
    handle: &AppComposition,
    person_id: PersonId,
    day: &DayQueryDto,
) -> BridgeResult<DaySnapshotDto> {
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

pub fn load_day(handle: &FloeHandle, request: LoadDayRequestDto) -> BridgeResult<DaySnapshotDto> {
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    snapshot(handle.services(), person_id, &request.day)
}

pub fn agent_fixture(
    handle: &FloeHandle,
    request: AgentFixtureRequestDto,
) -> BridgeResult<AgentFixtureResultDto> {
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
            handle.agent_runs().ensure_idle(person_id, session_id)?;
            handle.runtime().block_on(handle.core().recover_agent_fixture(
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
            handle.agent_runs().ensure_idle(person_id, session_id)?;
            let prompt = floe_app::agent_run::fixture_prompt(prompt);
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

#[derive(Serialize)]
pub struct CalendarActionsResult {
    pub actions: Vec<floe_app::modules::actions::CalendarAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writes_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<floe_app::modules::actions::ActionAuthority>,
}

pub fn calendar_actions(
    handle: &FloeHandle,
    request: CalendarActionRequestDto,
) -> BridgeResult<CalendarActionsResult> {
    let handle = handle.services();
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    let recover = matches!(
        &request.operation,
        CalendarActionOperationDto::Recover { .. }
    );
    let action = match request.operation {
        CalendarActionOperationDto::Capabilities {} => {
            return Ok(CalendarActionsResult {
                actions: vec![],
                writes_enabled: Some(
                    person_id.to_string() == floe_app::modules::native_calendar::LOCAL_PERSON
                        && floe_app::modules::native_calendar::NativeCalendar::enabled(),
                ),
                authority: None,
            });
        }
        CalendarActionOperationDto::GetAuthority {} => {
            let authority = handle
                .runtime()
                .block_on(handle.core().actions()
                .action_authority(person_id))
                .map_err(action_error)?;
            return Ok(CalendarActionsResult {
                actions: vec![],
                writes_enabled: None,
                authority: Some(authority),
            });
        }
        CalendarActionOperationDto::SetAuthority { .. } => {
            return Err(agent_failure(floe_app::modules::agent_contract::AgentFailure::PolicyDenied));
        }
        CalendarActionOperationDto::Execute { action_id }
        | CalendarActionOperationDto::Recover { action_id } => {
            let id = parse_id(&action_id, "action_id", |value| value)?;
            if person_id.to_string() != floe_app::modules::native_calendar::LOCAL_PERSON {
                return Err(invalid(
                    "person_id",
                    "native Calendar is bound to this device's Person",
                ));
            }
            let connection = handle
                .runtime()
                .block_on(handle.core().calendar_connection(person_id))
                .map_err(core_error)?;
            let calendar_ids = connection
                .as_ref()
                .map(|connection| {
                    connection
                        .calendars
                        .iter()
                        .map(|calendar| calendar.calendar_id.clone())
                        .collect()
                })
                .unwrap_or_default();
            let provider = floe_app::modules::native_calendar::NativeCalendar::new(calendar_ids);
            if recover {
                handle.runtime().block_on(
                    handle
                        .core()
                        .actions()
                .recover_calendar_action(person_id, id, &provider),
                )
            } else {
                let authority = handle
                    .runtime()
                    .block_on(handle.core().actions()
                .action_authority(person_id))
                    .map_err(action_error)?;
                let policy = floe_app::modules::actions::CalendarActionPolicy {
                    person_id,
                    provider: floe_app::modules::day::CalendarProvider::EventKit,
                    allowed_calendar_ids: provider.calendar_ids.clone(),
                    allow_create: floe_app::modules::native_calendar::NativeCalendar::enabled()
                        && (handle
                            .runtime()
                            .block_on(handle.core().actions()
                .calendar_action(person_id, id))
                            .map_err(action_error)?
                            .direct
                            || authority.calendar_create != floe_app::modules::actions::ActionAuthorityMode::Deny),
                };
                handle.runtime().block_on(handle.core().actions()
                .execute_calendar_action(
                    person_id,
                    id,
                    &policy,
                    &provider,
                    Utc::now,
                ))
            }
        }
        CalendarActionOperationDto::List {} => {
            return handle
                .runtime()
                .block_on(handle.core().actions()
                .calendar_actions(person_id))
                .map(|actions| CalendarActionsResult {
                    actions,
                    writes_enabled: None,
                    authority: None,
                })
                .map_err(action_error);
        }
        CalendarActionOperationDto::Get { action_id } => handle.runtime().block_on(
            handle
                .core()
                .actions()
                .calendar_action(person_id, parse_id(&action_id, "action_id", |value| value)?),
        ),
        CalendarActionOperationDto::Propose {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
        } => {
            let schedule = floe_app::modules::day::TimedSchedule::new(
                parse_time(&starts_at, "starts_at")?,
                parse_time(&ends_at, "ends_at")?,
                &timezone,
            )
            .map_err(|value| invalid("schedule", value.to_string()))?;
            handle.runtime().block_on(handle.core().actions()
                .propose_calendar_action(
                person_id,
                calendar_id,
                title,
                schedule,
                Utc::now(),
            ))
        }
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
            if person_id.to_string() != floe_app::modules::native_calendar::LOCAL_PERSON {
                return Err(invalid(
                    "person_id",
                    "direct Calendar is bound to this device's Person",
                ));
            }
            let schedule = floe_app::modules::day::TimedSchedule::new(
                parse_time(&starts_at, "starts_at")?,
                parse_time(&ends_at, "ends_at")?,
                &timezone,
            )
            .map_err(|value| invalid("schedule", value.to_string()))?;
            let event_id = event_id
                .map(|value| parse_id(&value, "event_id", floe_app::modules::kernel::EventId))
                .transpose()?;
            let target = match (event_id, event_revision) {
                (Some(id), Some(revision)) => Some((id, floe_app::modules::kernel::Revision(revision))),
                (None, None) => None,
                _ => {
                    return Err(invalid(
                        "event_revision",
                        "target identity and revision are required together",
                    ));
                }
            };
            handle.runtime().block_on(handle.core().actions().direct_calendar_action(
                person_id,
                calendar_id,
                title,
                schedule,
                target,
                delete,
                Utc::now(),
            ))
        }
        CalendarActionOperationDto::Decide {
            action_id,
            decision,
        } => handle.runtime().block_on(handle.core().actions()
                .decide_calendar_action(
            person_id,
            parse_id(&action_id, "action_id", |value| value)?,
            decision == CalendarActionDecisionDto::Approve,
            Utc::now(),
        )),
    }
    .map_err(action_error)?;
    Ok(CalendarActionsResult {
        actions: vec![action],
        writes_enabled: None,
        authority: None,
    })
}

pub fn execute(handle: &FloeHandle, request: CommandRequestDto) -> BridgeResult<MutationResultDto> {
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
                .map(|batch| {
                    let records = batch
                        .records
                        .into_iter()
                        .map(|record| {
                            Ok(floe_app::modules::day::CalendarRecord {
                                can_modify: record.can_modify,
                                calendar_id: record.calendar_id,
                                external_id: record.external_id,
                                external_revision: record.external_revision,
                                title: record.title,
                                schedule: conversion::event_schedule_from_dto(record.schedule)?,
                            })
                        })
                        .collect::<Result<Vec<_>, conversion::ProtocolConversionError>>();
                    match records {
                        Ok(records) => floe_app::modules::day::CalendarBatch {
                            calendar_id: batch.calendar_id,
                            records,
                            failure: batch.failure.map(conversion::calendar_failure_from_dto),
                        },
                        Err(_) => floe_app::modules::day::CalendarBatch {
                            calendar_id: batch.calendar_id,
                            records: vec![],
                            failure: Some(floe_app::modules::day::CalendarFailure::ProviderUnavailable),
                        },
                    }
                })
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
                .map(|record| {
                    Ok(floe_app::modules::day::CalendarRecord {
                        can_modify: record.can_modify,
                        calendar_id: record.calendar_id,
                        external_id: record.external_id,
                        external_revision: record.external_revision,
                        title: record.title,
                        schedule: conversion::event_schedule_from_dto(record.schedule)
                            .map_err(conversion_error)?,
                    })
                })
                .collect::<BridgeResult<Vec<_>>>()?;
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
            let capture_id = parse_id(&capture_id, "capture_id", floe_app::modules::kernel::CaptureId)?;
            let classification = match classification {
                ClassificationDto::Event { title, schedule } => Classification::Event {
                    title,
                    schedule: conversion::event_schedule_from_dto(schedule)
                        .map_err(conversion_error)?,
                },
                ClassificationDto::Task {
                    title,
                    deadline,
                    priority,
                } => Classification::Task {
                    title,
                    deadline: deadline
                        .as_deref()
                        .map(|value| parse_time(value, "deadline"))
                        .transpose()?,
                    priority: conversion::priority_from_dto(priority),
                },
                ClassificationDto::Note { content } => Classification::Note { content },
            };
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
                    parse_id(&event_id, "event_id", floe_app::modules::kernel::EventId)?,
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
                        parse_id(&task_id, "task_id", floe_app::modules::kernel::TaskId)?,
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
                    parse_id(&note_id, "note_id", floe_app::modules::kernel::NoteId)?,
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
                    parse_id(&task_id, "task_id", floe_app::modules::kernel::TaskId)?,
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
