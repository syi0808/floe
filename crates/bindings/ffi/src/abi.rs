use uuid::Uuid;

use super::*;
use serde::de::DeserializeOwned;

#[cfg(unix)]
fn invoke_json_v2<Request, Response>(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
    operation: impl FnOnce(&FloeHandle, Request) -> app_wire::AppWireResult<Response>,
) -> *mut c_char
where
    Request: DeserializeOwned,
    Response: Serialize,
{
    diagnostics::initialize();
    let raw = match c_input(request_json, "request_json") {
        Ok(raw) => raw,
        Err(_) => {
            return c_output(AppResponseDto::<Value>::error(
                Uuid::nil(),
                app_wire::validation("request_json"),
            ));
        }
    };
    let request_id = serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| value.get("request_id")?.as_str()?.parse().ok())
        .unwrap_or_else(Uuid::nil);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let handle = handle(handle_ptr).map_err(|_| app_wire::validation("handle"))?;
        let request =
            serde_json::from_str(raw).map_err(|_| app_wire::validation("request_json"))?;
        operation(handle, request)
    }));
    match result {
        Ok(Ok(result)) => c_output(AppResponseDto::ok(request_id, result)),
        Ok(Err(error)) => c_output(AppResponseDto::<Value>::error(request_id, error)),
        Err(payload) => {
            let _ = diagnostics::panic_error(payload);
            c_output(AppResponseDto::<Value>::error(
                request_id,
                app_wire::internal_error(),
            ))
        }
    }
}

fn invoke_json<Request, Response>(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
    operation: impl FnOnce(&FloeHandle, Request) -> BridgeResult<Response>,
) -> *mut c_char
where
    Request: DeserializeOwned,
    Response: Serialize,
{
    guarded(|| {
        let handle = handle(handle_ptr)?;
        let request = serde_json::from_str(c_input(request_json, "request_json")?)
            .map_err(|value| invalid("request_json", value.to_string()))?;
        operation(handle, request)
    })
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_open(
    path: *const c_char,
    error_json_out: *mut *mut c_char,
) -> *mut FloeHandle {
    diagnostics::initialize();
    if !error_json_out.is_null() {
        unsafe { *error_json_out = ptr::null_mut() };
    }
    let operation = || -> BridgeResult<*mut FloeHandle> {
        let path = c_input(path, "path")?;
        let app = floe_app::open(path).map_err(open_error)?;
        Ok(Box::into_raw(Box::new(FloeHandle::new(app))))
    };
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => value,
        Ok(Err(value)) => {
            if !error_json_out.is_null() {
                unsafe { *error_json_out = c_output(ResponseEnvelopeDto::<Value>::error(value)) };
            }
            ptr::null_mut()
        }
        Err(payload) => {
            let value = diagnostics::panic_error(payload);
            if !error_json_out.is_null() {
                unsafe { *error_json_out = c_output(ResponseEnvelopeDto::<Value>::error(value)) };
            }
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_local_context(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json(handle_ptr, request_json, local_context)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_load_day(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json(handle_ptr, request_json, load_day)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_execute(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json(handle_ptr, request_json, execute)
}

#[unsafe(no_mangle)]
pub extern "C" fn floe_protocol_version() -> u32 {
    PROTOCOL_VERSION
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
#[cfg(unix)]
pub unsafe extern "C" fn floe_core_command_v2(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json_v2(handle_ptr, request_json, app_wire::command)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
#[cfg(unix)]
pub unsafe extern "C" fn floe_core_query_v2(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json_v2(handle_ptr, request_json, app_wire::query)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
#[cfg(unix)]
pub unsafe extern "C" fn floe_core_events_v2(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json_v2(handle_ptr, request_json, app_wire::events)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_calendar_actions(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json(handle_ptr, request_json, calendar_actions)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_agent_fixture(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json(handle_ptr, request_json, agent_fixture)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_agent_fixture_run(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json(handle_ptr, request_json, floe_app::agent_run::run)
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
        let handle = handle.services();
        #[cfg(unix)]
        {
            handle.agent_vault().request(request)
        }
        #[cfg(not(unix))]
        {
            let _ = (handle, request);
            Err::<AgentVaultResultDto, _>(agent_failure(floe_app::modules::agent_contract::AgentFailure::VaultUnavailable))
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

fn open_error(value: AppOpenError) -> ErrorDto {
    match value {
        AppOpenError::Host(host) => host_error(host),
        AppOpenError::Runtime(message) | AppOpenError::Store(message) => {
            error(ErrorCodeDto::Internal, message)
        }
    }
}
