use uuid::Uuid;

use super::*;
use crate::bridge::open_error;
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
    let operation = || -> WireResult<*mut FloeHandle> {
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

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
#[cfg(unix)]
pub unsafe extern "C" fn floe_core_remote_pairing_v2(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json_v2(handle_ptr, request_json, crate::remote_wire::pairing)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
#[cfg(unix)]
pub unsafe extern "C" fn floe_core_remote_access_v2(
    handle_ptr: *mut FloeHandle,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_json_v2(handle_ptr, request_json, crate::remote_wire::access)
}
