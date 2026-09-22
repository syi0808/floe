//! C ABI boundary: raw pointers, JSON envelopes, DTO conversion, memory
//! release and the panic barrier.
//!
//! No product judgment lives here. Every request is parsed into the app's own
//! command and handed to `floe-app`.

mod abi;
mod app_wire;
mod bridge;
mod context_wire;
pub mod conversion;
mod day_wire;
mod diagnostics;
mod remote_wire;

pub use abi::*;
pub use bridge::FloeHandle;

use std::{
    ffi::{CStr, CString, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};

use floe_protocol::wire::{
    WireResult, agent_failure, check_version, invalid, parse_id, parse_person, protocol_payload,
};
use floe_protocol::*;
use serde::Serialize;
use serde_json::Value;

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
