mod agent_run;
mod native_calendar;
#[cfg(unix)]
mod vault_host;

use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    sync::Arc,
};

use chrono::{DateTime, NaiveDate, Utc};
use floe_core::{Classification, CoreError, ErrorCode, FloeCore};
use floe_domain::{DomainRef, PersonId, Revision, TimelineItem};
use floe_protocol::*;
use serde::Serialize;
use serde_json::Value;
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

pub struct FloeHandle {
    runtime: Runtime,
    core: Arc<FloeCore>,
    agent_runs: agent_run::AgentRuns,
    #[cfg(unix)]
    agent_vault: vault_host::VaultBridge,
}

impl Drop for FloeHandle {
    fn drop(&mut self) {
        self.agent_runs.close(&self.runtime);
    }
}

type BridgeResult<T> = Result<T, ErrorDto>;

fn error(code: ErrorCodeDto, message: impl Into<String>) -> ErrorDto {
    ErrorDto {
        code,
        message: message.into(),
        field: None,
        metadata: Default::default(),
    }
}

fn invalid(field: &'static str, message: impl Into<String>) -> ErrorDto {
    ErrorDto {
        code: ErrorCodeDto::Validation,
        message: message.into(),
        field: Some(field.into()),
        metadata: Default::default(),
    }
}

fn core_error(value: CoreError) -> ErrorDto {
    ErrorDto {
        code: match value.code {
            ErrorCode::Validation => ErrorCodeDto::Validation,
            ErrorCode::NotFound => ErrorCodeDto::NotFound,
            ErrorCode::Conflict => ErrorCodeDto::Conflict,
            ErrorCode::Storage => ErrorCodeDto::Storage,
            ErrorCode::NoFocusSlot => ErrorCodeDto::NoFocusSlot,
        },
        message: value.message,
        field: None,
        metadata: value.metadata,
    }
}

fn conversion_error(value: ProtocolConversionError) -> ErrorDto {
    match value {
        ProtocolConversionError::UnsupportedVersion { actual, expected } => {
            let mut value = error(
                ErrorCodeDto::UnsupportedVersion,
                format!("unsupported schema version {actual}; expected {expected}"),
            );
            value.metadata.insert("actual".into(), actual.to_string());
            value
                .metadata
                .insert("expected".into(), expected.to_string());
            value
        }
        ProtocolConversionError::InvalidField { field, message } => invalid(field, message),
        ProtocolConversionError::OutOfRange { field } => invalid(field, "value is out of range"),
    }
}

fn parse_person(value: &str) -> BridgeResult<PersonId> {
    Uuid::parse_str(value)
        .map(PersonId)
        .map_err(|value| invalid("person_id", value.to_string()))
}

fn parse_id<T>(value: &str, field: &'static str, wrap: impl FnOnce(Uuid) -> T) -> BridgeResult<T> {
    Uuid::parse_str(value)
        .map(wrap)
        .map_err(|value| invalid(field, value.to_string()))
}

fn parse_time(value: &str, field: &'static str) -> BridgeResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|value| invalid(field, value.to_string()))
}

fn parse_date(value: &str) -> BridgeResult<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|value| invalid("date", value.to_string()))
}

fn check_version(version: u32) -> BridgeResult<()> {
    if version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(conversion_error(
            ProtocolConversionError::UnsupportedVersion {
                actual: version,
                expected: PROTOCOL_VERSION,
            },
        ))
    }
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
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(value) => envelope(value),
        Err(_) => envelope::<T>(Err(error(ErrorCodeDto::Internal, "Rust core panicked"))),
    }
}

fn handle<'a>(value: *mut FloeHandle) -> BridgeResult<&'a FloeHandle> {
    unsafe { value.as_ref() }.ok_or_else(|| invalid("handle", "must not be null"))
}

fn snapshot(
    handle: &FloeHandle,
    person_id: PersonId,
    day: &DayQueryDto,
) -> BridgeResult<DaySnapshotDto> {
    let date = parse_date(&day.date)?;
    let now = parse_time(&day.now, "day.now")?;
    let value = handle
        .runtime
        .block_on(handle.core.day_snapshot_with_end_offset(
            person_id,
            date,
            day.timezone_offset_seconds,
            day.end_timezone_offset_seconds,
            now,
        ))
        .map_err(core_error)?;
    value.try_into().map_err(conversion_error)
}

pub fn load_day(handle: &FloeHandle, request: LoadDayRequestDto) -> BridgeResult<DaySnapshotDto> {
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    snapshot(handle, person_id, &request.day)
}

pub fn agent_fixture(
    handle: &FloeHandle,
    request: AgentFixtureRequestDto,
) -> BridgeResult<AgentFixtureResultDto> {
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    let session = match request.operation {
        AgentFixtureOperationDto::Resume {} => handle
            .runtime
            .block_on(handle.core.resume_agent_fixture(person_id)),
        AgentFixtureOperationDto::Start {} => handle
            .runtime
            .block_on(handle.core.start_agent_fixture(person_id)),
        AgentFixtureOperationDto::Get { session_id } => {
            let session_id = parse_id(&session_id, "session_id", |value| value)?;
            handle
                .runtime
                .block_on(handle.core.agent_fixture_session(person_id, session_id))
        }
        AgentFixtureOperationDto::Recover {
            session_id,
            expected_revision,
        } => {
            let session_id = parse_id(&session_id, "session_id", |value| value)?;
            handle.agent_runs.ensure_idle(person_id, session_id)?;
            handle.runtime.block_on(handle.core.recover_agent_fixture(
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
            handle.agent_runs.ensure_idle(person_id, session_id)?;
            let prompt = agent_run::fixture_prompt(prompt);
            let result = handle
                .runtime
                .block_on(handle.core.run_agent_fixture(
                    person_id,
                    session_id,
                    expected_revision,
                    prompt,
                ))
                .map_err(agent_failure)?;
            return Ok(AgentFixtureResultDto {
                session: result.session,
                events: result.events,
            });
        }
    }
    .map_err(agent_failure)?;
    Ok(AgentFixtureResultDto {
        session,
        events: vec![],
    })
}

fn agent_failure(failure: floe_agent::AgentFailure) -> ErrorDto {
    use floe_agent::AgentFailure;
    let code = match failure {
        AgentFailure::NotFound => ErrorCodeDto::NotFound,
        AgentFailure::Conflict => ErrorCodeDto::Conflict,
        AgentFailure::StorageUnavailable => ErrorCodeDto::Storage,
        AgentFailure::UnsupportedVersion => ErrorCodeDto::UnsupportedVersion,
        _ => ErrorCodeDto::Validation,
    };
    let mut result = error(code, "Agent request could not complete");
    if let Ok(Value::String(reason)) = serde_json::to_value(failure) {
        result.metadata.insert("agent_failure".into(), reason);
    }
    result
}

#[derive(Serialize)]
pub struct CalendarActionsResult {
    pub actions: Vec<floe_core::CalendarAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writes_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<floe_core::ActionAuthority>,
}

pub fn calendar_actions(
    handle: &FloeHandle,
    request: CalendarActionRequestDto,
) -> BridgeResult<CalendarActionsResult> {
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
                    person_id.to_string() == native_calendar::LOCAL_PERSON
                        && native_calendar::NativeCalendar::enabled(),
                ),
                authority: None,
            });
        }
        CalendarActionOperationDto::GetAuthority {} => {
            let authority = handle
                .runtime
                .block_on(handle.core.action_authority(person_id))
                .map_err(core_error)?;
            return Ok(CalendarActionsResult {
                actions: vec![],
                writes_enabled: None,
                authority: Some(authority),
            });
        }
        CalendarActionOperationDto::SetAuthority { calendar_create } => {
            let mode = match calendar_create {
                ActionAuthorityModeDto::Allow => floe_core::ActionAuthorityMode::Allow,
                ActionAuthorityModeDto::Ask => floe_core::ActionAuthorityMode::Ask,
                ActionAuthorityModeDto::Deny => floe_core::ActionAuthorityMode::Deny,
            };
            let authority = handle
                .runtime
                .block_on(handle.core.set_action_authority(person_id, mode))
                .map_err(core_error)?;
            return Ok(CalendarActionsResult {
                actions: vec![],
                writes_enabled: None,
                authority: Some(authority),
            });
        }
        CalendarActionOperationDto::Execute { action_id }
        | CalendarActionOperationDto::Recover { action_id } => {
            let id = parse_id(&action_id, "action_id", |value| value)?;
            if person_id.to_string() != native_calendar::LOCAL_PERSON {
                return Err(invalid(
                    "person_id",
                    "native Calendar is bound to this device's Person",
                ));
            }
            let connection = handle
                .runtime
                .block_on(handle.core.calendar_connection(person_id))
                .map_err(core_error)?;
            let calendar_ids = connection
                .as_ref()
                .map(|connection| {
                    connection
                        .selected_calendars()
                        .into_iter()
                        .map(|calendar| calendar.calendar_id)
                        .collect()
                })
                .unwrap_or_default();
            let provider = native_calendar::NativeCalendar::new(calendar_ids);
            if recover {
                handle.runtime.block_on(
                    handle
                        .core
                        .recover_calendar_action(person_id, id, &provider),
                )
            } else {
                let authority = handle
                    .runtime
                    .block_on(handle.core.action_authority(person_id))
                    .map_err(core_error)?;
                let policy = floe_core::CalendarActionPolicy {
                    person_id,
                    provider: floe_domain::CalendarProvider::EventKit,
                    allowed_calendar_ids: provider.calendar_ids.clone(),
                    allow_create: native_calendar::NativeCalendar::enabled()
                        && (handle
                            .runtime
                            .block_on(handle.core.calendar_action(person_id, id))
                            .map_err(core_error)?
                            .direct
                            || authority.calendar_create != floe_core::ActionAuthorityMode::Deny),
                };
                handle.runtime.block_on(handle.core.execute_calendar_action(
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
                .runtime
                .block_on(handle.core.calendar_actions(person_id))
                .map(|actions| CalendarActionsResult {
                    actions,
                    writes_enabled: None,
                    authority: None,
                })
                .map_err(core_error);
        }
        CalendarActionOperationDto::Get { action_id } => handle.runtime.block_on(
            handle
                .core
                .calendar_action(person_id, parse_id(&action_id, "action_id", |value| value)?),
        ),
        CalendarActionOperationDto::Propose {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
        } => {
            let schedule = floe_domain::TimedSchedule::new(
                parse_time(&starts_at, "starts_at")?,
                parse_time(&ends_at, "ends_at")?,
                &timezone,
            )
            .map_err(|value| invalid("schedule", value.to_string()))?;
            handle.runtime.block_on(handle.core.propose_calendar_action(
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
            if person_id.to_string() != native_calendar::LOCAL_PERSON {
                return Err(invalid(
                    "person_id",
                    "direct Calendar is bound to this device's Person",
                ));
            }
            let schedule = floe_domain::TimedSchedule::new(
                parse_time(&starts_at, "starts_at")?,
                parse_time(&ends_at, "ends_at")?,
                &timezone,
            )
            .map_err(|value| invalid("schedule", value.to_string()))?;
            let event_id = event_id
                .map(|value| parse_id(&value, "event_id", floe_domain::EventId))
                .transpose()?;
            let target = match (event_id, event_revision) {
                (Some(id), Some(revision)) => Some((id, floe_domain::Revision(revision))),
                (None, None) => None,
                _ => {
                    return Err(invalid(
                        "event_revision",
                        "target identity and revision are required together",
                    ));
                }
            };
            handle.runtime.block_on(handle.core.direct_calendar_action(
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
        } => handle.runtime.block_on(handle.core.decide_calendar_action(
            person_id,
            parse_id(&action_id, "action_id", |value| value)?,
            decision == CalendarActionDecisionDto::Approve,
            Utc::now(),
        )),
    }
    .map_err(core_error)?;
    Ok(CalendarActionsResult {
        actions: vec![action],
        writes_enabled: None,
        authority: None,
    })
}

pub fn execute(handle: &FloeHandle, request: CommandRequestDto) -> BridgeResult<MutationResultDto> {
    check_version(request.schema_version)?;
    let person_id = parse_person(&request.person_id)?;
    parse_date(&request.day.date)?;
    parse_time(&request.day.now, "day.now")?;
    let mut changed_item = None;
    let mut capture = None;

    match request.command {
        CommandDto::DisconnectCalendar { expected_revision } => {
            handle
                .runtime
                .block_on(
                    handle
                        .core
                        .disconnect_calendar(person_id, expected_revision),
                )
                .map_err(core_error)?;
        }
        CommandDto::SetCalendarScope {
            provider,
            calendars,
            scope,
        } => {
            handle
                .runtime
                .block_on(
                    handle
                        .core
                        .set_calendar_scope(person_id, provider, calendars, scope),
                )
                .map_err(core_error)?;
        }
        CommandDto::DiscoverCalendars {
            expected_revision,
            calendars,
        } => {
            handle
                .runtime
                .block_on(
                    handle
                        .core
                        .discover_calendars(person_id, expected_revision, calendars),
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
                            Ok(floe_domain::CalendarRecord {
                                can_modify: record.can_modify,
                                calendar_id: record.calendar_id,
                                external_id: record.external_id,
                                external_revision: record.external_revision,
                                title: record.title,
                                schedule: record.schedule.try_into()?,
                            })
                        })
                        .collect::<Result<Vec<_>, floe_protocol::ProtocolConversionError>>();
                    match records {
                        Ok(records) => floe_domain::CalendarBatch {
                            calendar_id: batch.calendar_id,
                            records,
                            failure: batch.failure,
                        },
                        Err(_) => floe_domain::CalendarBatch {
                            calendar_id: batch.calendar_id,
                            records: vec![],
                            failure: Some(floe_domain::CalendarFailure::ProviderUnavailable),
                        },
                    }
                })
                .collect();
            handle
                .runtime
                .block_on(handle.core.import_calendar_sources(
                    person_id,
                    expected_revision,
                    range,
                    batches,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
        }
        CommandDto::SelectCalendars {
            provider,
            calendars,
        } => {
            handle
                .runtime
                .block_on(handle.core.select_calendars(person_id, provider, calendars))
                .map_err(core_error)?;
        }
        CommandDto::SelectCalendar {
            provider,
            calendar_id,
            calendar_name,
        } => {
            handle
                .runtime
                .block_on(handle.core.select_calendar(
                    person_id,
                    provider,
                    calendar_id,
                    calendar_name,
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
                    Ok(floe_domain::CalendarRecord {
                        can_modify: record.can_modify,
                        calendar_id: record.calendar_id,
                        external_id: record.external_id,
                        external_revision: record.external_revision,
                        title: record.title,
                        schedule: record.schedule.try_into().map_err(conversion_error)?,
                    })
                })
                .collect::<BridgeResult<Vec<_>>>()?;
            handle
                .runtime
                .block_on(handle.core.import_calendar(
                    person_id,
                    expected_revision,
                    range,
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
                .runtime
                .block_on(handle.core.record_calendar_failure(
                    person_id,
                    expected_revision,
                    failure,
                ))
                .map_err(core_error)?;
        }
        CommandDto::SubmitCapture { input, occurred_at } => {
            let occurred_at = parse_time(&occurred_at, "occurred_at")?;
            let value = handle
                .runtime
                .block_on(handle.core.submit_capture(person_id, input, occurred_at))
                .map_err(core_error)?;
            capture = Some(value.into());
        }
        CommandDto::ClassifyCapture {
            capture_id,
            expected_revision,
            classification,
            occurred_at,
        } => {
            let capture_id = parse_id(&capture_id, "capture_id", floe_domain::CaptureId)?;
            let classification = match classification {
                ClassificationDto::Event { title, schedule } => Classification::Event {
                    title,
                    schedule: schedule.try_into().map_err(conversion_error)?,
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
                    priority: priority.into(),
                },
                ClassificationDto::Note { content } => Classification::Note { content },
            };
            let value = handle
                .runtime
                .block_on(handle.core.classify_capture(
                    capture_id,
                    Revision(expected_revision),
                    classification,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(value.into());
        }
        CommandDto::CreateEvent {
            title,
            schedule,
            occurred_at,
        } => {
            let value = handle
                .runtime
                .block_on(handle.core.create_event(
                    person_id,
                    title,
                    schedule.try_into().map_err(conversion_error)?,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(TimelineItem::Event(value).into());
        }
        CommandDto::CreateTask {
            title,
            deadline,
            priority,
            occurred_at,
        } => {
            let value = handle
                .runtime
                .block_on(
                    handle.core.create_task(
                        person_id,
                        title,
                        deadline
                            .as_deref()
                            .map(|value| parse_time(value, "deadline"))
                            .transpose()?,
                        priority.into(),
                        parse_time(&occurred_at, "occurred_at")?,
                    ),
                )
                .map_err(core_error)?;
            changed_item = Some(TimelineItem::Task(value).into());
        }
        CommandDto::CreateNote {
            content,
            occurred_at,
        } => {
            let value = handle
                .runtime
                .block_on(handle.core.create_note(
                    person_id,
                    content,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(TimelineItem::Note(value).into());
        }
        CommandDto::UpdateEvent {
            event_id,
            expected_revision,
            title,
            schedule,
            occurred_at,
        } => {
            let value = handle
                .runtime
                .block_on(handle.core.update_event(
                    parse_id(&event_id, "event_id", floe_domain::EventId)?,
                    Revision(expected_revision),
                    title,
                    schedule.try_into().map_err(conversion_error)?,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(TimelineItem::Event(value).into());
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
                .runtime
                .block_on(
                    handle.core.update_task(
                        parse_id(&task_id, "task_id", floe_domain::TaskId)?,
                        Revision(expected_revision),
                        title,
                        deadline
                            .as_deref()
                            .map(|value| parse_time(value, "deadline"))
                            .transpose()?,
                        priority.into(),
                        parse_time(&occurred_at, "occurred_at")?,
                    ),
                )
                .map_err(core_error)?;
            changed_item = Some(TimelineItem::Task(value).into());
        }
        CommandDto::UpdateNote {
            note_id,
            expected_revision,
            content,
            occurred_at,
        } => {
            let value = handle
                .runtime
                .block_on(handle.core.update_note(
                    parse_id(&note_id, "note_id", floe_domain::NoteId)?,
                    Revision(expected_revision),
                    content,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(TimelineItem::Note(value).into());
        }
        CommandDto::SetTaskCompletion {
            task_id,
            expected_revision,
            completed,
            occurred_at,
        } => {
            let value = handle
                .runtime
                .block_on(handle.core.set_task_completed(
                    parse_id(&task_id, "task_id", floe_domain::TaskId)?,
                    Revision(expected_revision),
                    completed,
                    parse_time(&occurred_at, "occurred_at")?,
                ))
                .map_err(core_error)?;
            changed_item = Some(TimelineItem::Task(value).into());
        }
        CommandDto::DeleteItem {
            target,
            expected_revision,
            occurred_at,
        } => {
            let reference: DomainRef = target.try_into().map_err(conversion_error)?;
            handle
                .runtime
                .block_on(handle.core.delete_item(
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

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_open(
    path: *const c_char,
    error_json_out: *mut *mut c_char,
) -> *mut FloeHandle {
    if !error_json_out.is_null() {
        unsafe { *error_json_out = ptr::null_mut() };
    }
    let operation = || -> BridgeResult<*mut FloeHandle> {
        let path = c_input(path, "path")?;
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|value| error(ErrorCodeDto::Internal, value.to_string()))?;
        let core = runtime.block_on(FloeCore::open(path)).map_err(core_error)?;
        Ok(Box::into_raw(Box::new(FloeHandle {
            runtime,
            core: Arc::new(core),
            agent_runs: Default::default(),
            #[cfg(unix)]
            agent_vault: vault_host::VaultBridge::new(path),
        })))
    };
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => value,
        Ok(Err(value)) => {
            if !error_json_out.is_null() {
                unsafe { *error_json_out = c_output(ResponseEnvelopeDto::<Value>::error(value)) };
            }
            ptr::null_mut()
        }
        Err(_) => {
            let value = error(ErrorCodeDto::Internal, "Rust core panicked");
            if !error_json_out.is_null() {
                unsafe { *error_json_out = c_output(ResponseEnvelopeDto::<Value>::error(value)) };
            }
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_load_day(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    guarded(|| {
        let handle = handle(handle_ptr)?;
        let request = serde_json::from_str(c_input(request_json, "request_json")?)
            .map_err(|value| invalid("request_json", value.to_string()))?;
        load_day(handle, request)
    })
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_execute(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    guarded(|| {
        let handle = handle(handle_ptr)?;
        let request = serde_json::from_str(c_input(request_json, "request_json")?)
            .map_err(|value| invalid("request_json", value.to_string()))?;
        execute(handle, request)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn floe_protocol_version() -> u32 {
    PROTOCOL_VERSION
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_calendar_actions(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    guarded(|| {
        let handle = handle(handle_ptr)?;
        let request = serde_json::from_str(c_input(request_json, "request_json")?)
            .map_err(|value| invalid("request_json", value.to_string()))?;
        calendar_actions(handle, request)
    })
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_agent_fixture(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    guarded(|| {
        let handle = handle(handle_ptr)?;
        let request = serde_json::from_str(c_input(request_json, "request_json")?)
            .map_err(|value| invalid("request_json", value.to_string()))?;
        agent_fixture(handle, request)
    })
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_agent_fixture_run(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    guarded(|| {
        let handle = handle(handle_ptr)?;
        let request = serde_json::from_str(c_input(request_json, "request_json")?)
            .map_err(|value| invalid("request_json", value.to_string()))?;
        agent_run::run(handle, request)
    })
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_agent_vault(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    guarded(|| {
        let handle = handle(handle_ptr)?;
        let request: AgentVaultRequestDto =
            serde_json::from_str(c_input(request_json, "request_json")?)
                .map_err(|_| invalid("request_json", "invalid Agent vault request"))?;
        #[cfg(unix)]
        {
            handle.agent_vault.request(request)
        }
        #[cfg(not(unix))]
        {
            let _ = (handle, request);
            Err::<AgentVaultResultDto, _>(agent_failure(floe_agent::AgentFailure::VaultUnavailable))
        }
    })
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_string_free(value: *mut c_char) {
    if !value.is_null() {
        unsafe { drop(CString::from_raw(value)) };
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_free(handle: *mut FloeHandle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle)) };
    }
}
