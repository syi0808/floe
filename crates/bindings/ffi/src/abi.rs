use uuid::Uuid;

use super::*;
use crate::bridge::{host_error, open_error};
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

#[cfg(unix)]
fn invoke_command_json_v2(handle_ptr: *mut FloeHandle, request_json: *const c_char) -> *mut c_char {
    diagnostics::initialize();
    let raw = match c_input(request_json, "request_json") {
        Ok(raw) => raw,
        Err(_) => {
            return command_output::<Value>(
                Uuid::nil(),
                Err(app_wire::AppCommandFailure::NotAdmitted(
                    app_wire::validation("request_json"),
                )),
            );
        }
    };
    let request_id = serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| value.get("request_id")?.as_str()?.parse().ok())
        .unwrap_or_else(Uuid::nil);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let handle = handle(handle_ptr).map_err(|_| {
            app_wire::AppCommandFailure::NotAdmitted(app_wire::validation("handle"))
        })?;
        let request: AppCommandRequestDto = serde_json::from_str(raw).map_err(|_| {
            app_wire::AppCommandFailure::NotAdmitted(app_wire::validation("request_json"))
        })?;
        app_wire::command(handle, request)
    }));
    let result = match result {
        Ok(result) => result,
        Err(payload) => {
            let _ = diagnostics::panic_error(payload);
            Err(app_wire::AppCommandFailure::Indeterminate(
                app_wire::internal_error(),
            ))
        }
    };
    command_output(request_id, result)
}

#[cfg(unix)]
fn command_output<T: Serialize>(
    request_id: Uuid,
    result: app_wire::AppCommandResult<T>,
) -> *mut c_char {
    let response = match result {
        Ok(result) => AppResponseDto::ok(request_id, result),
        Err(failure) => {
            let (disposition, error) = failure.into_parts();
            AppResponseDto::command_error(request_id, disposition, error)
        }
    };
    // A lost acknowledgement during result encoding is not proof of rejection.
    let encoded = catch_unwind(AssertUnwindSafe(|| serde_json::to_string(&response)));
    match encoded {
        Ok(Ok(encoded)) => CString::new(encoded)
            .expect("JSON cannot contain NUL")
            .into_raw(),
        result => {
            if let Err(payload) = result {
                let _ = diagnostics::panic_error(payload);
            }
            c_output(AppResponseDto::<Value>::command_error(
                request_id,
                AppCommandDispositionDto::Indeterminate,
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
    open_core(path, "path", error_json_out, floe_app::open)
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_open_default(
    support_directory: *const c_char,
    error_json_out: *mut *mut c_char,
) -> *mut FloeHandle {
    open_core(
        support_directory,
        "support_directory",
        error_json_out,
        floe_app::open_default,
    )
}

fn open_core(
    path: *const c_char,
    field: &'static str,
    error_json_out: *mut *mut c_char,
    open: impl FnOnce(
        &str,
    )
        -> Result<floe_app::AppHost<floe_app::AppComposition>, floe_app::AppOpenError>,
) -> *mut FloeHandle {
    diagnostics::initialize();
    if !error_json_out.is_null() {
        unsafe { *error_json_out = ptr::null_mut() };
    }
    let operation = || -> WireResult<*mut FloeHandle> {
        let path = c_input(path, field)?;
        let app = open(path).map_err(open_error)?;
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

#[derive(Serialize)]
struct CoreIdentityDto {
    person_id: Uuid,
    device_id: String,
    runtime_epoch: u64,
}

#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_core_identity(handle_ptr: *mut FloeHandle) -> *mut c_char {
    diagnostics::initialize();
    let operation = || -> WireResult<CoreIdentityDto> {
        let handle = handle(handle_ptr)?;
        let request = handle.app().request(Uuid::new_v4()).map_err(host_error)?;
        let caller = request.caller();
        Ok(CoreIdentityDto {
            person_id: caller.person_id(),
            device_id: caller.device_id().to_owned(),
            runtime_epoch: caller.runtime_epoch(),
        })
    };
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(value)) => c_output(ResponseEnvelopeDto::ok(value)),
        Ok(Err(error)) => c_output(ResponseEnvelopeDto::<Value>::error(error)),
        Err(payload) => c_output(ResponseEnvelopeDto::<Value>::error(
            diagnostics::panic_error(payload),
        )),
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
    invoke_command_json_v2(handle_ptr, request_json)
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
        // The host's retirement worker retains service state after a failed or
        // timed-out drain. No borrowed core pointer enters that worker.
        let result = catch_unwind(AssertUnwindSafe(|| unsafe { drop(Box::from_raw(handle)) }));
        if let Err(payload) = result {
            report_free_panic(payload);
        }
    }
}

/// Acquire on the core's owning thread. Only the returned independent lane may
/// be transferred to a callback worker. It may safely outlive the core handle.
#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
#[cfg(unix)]
pub unsafe extern "C" fn floe_native_host_acquire(
    handle_ptr: *mut FloeHandle,
    error_json_out: *mut *mut c_char,
) -> *mut FloeNativeHostLane {
    diagnostics::initialize();
    if !error_json_out.is_null() {
        unsafe { *error_json_out = ptr::null_mut() };
    }
    let operation = || -> app_wire::AppWireResult<*mut FloeNativeHostLane> {
        let handle = handle(handle_ptr).map_err(|_| app_wire::validation("handle"))?;
        let request = handle
            .app()
            .request(Uuid::new_v4())
            .map_err(app_wire::host_failure)?;
        let lane = request
            .acquire_native_host_lane()
            .map_err(app_wire::host_failure)?;
        Ok(Box::into_raw(Box::new(FloeNativeHostLane { lane })))
    };
    let error = match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(lane)) => return lane,
        Ok(Err(error)) => error,
        Err(payload) => {
            let _ = diagnostics::panic_error(payload);
            app_wire::internal_error()
        }
    };
    if !error_json_out.is_null() {
        unsafe { *error_json_out = c_output(AppResponseDto::<Value>::error(Uuid::nil(), error)) };
    }
    ptr::null_mut()
}

#[cfg(unix)]
fn invoke_native_json_v2<Request, Response>(
    lane_ptr: *mut FloeNativeHostLane,
    request_json: *const c_char,
    operation: impl FnOnce(&FloeNativeHostLane, Request) -> app_wire::AppWireResult<Response>,
) -> *mut c_char
where
    Request: DeserializeOwned,
    Response: Serialize,
{
    diagnostics::initialize();
    let raw = match c_input(request_json, "request_json") {
        Ok(raw) if raw.len() <= 128 * 1024 => raw,
        _ => {
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
        let lane = unsafe { lane_ptr.as_ref() }.ok_or_else(|| app_wire::validation("lane"))?;
        let request =
            serde_json::from_str(raw).map_err(|_| app_wire::validation("request_json"))?;
        operation(lane, request)
    }));
    match result {
        Ok(Ok(value)) => c_output(AppResponseDto::ok(request_id, value)),
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
#[cfg(unix)]
pub unsafe extern "C" fn floe_native_host_command_v2(
    lane: *mut FloeNativeHostLane,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_native_json_v2(lane, request_json, native_lane::command)
}
#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
#[cfg(unix)]
pub unsafe extern "C" fn floe_native_host_query_v2(
    lane: *mut FloeNativeHostLane,
    request_json: *const c_char,
) -> *mut c_char {
    invoke_native_json_v2(lane, request_json, native_lane::query)
}
/// Free exactly once, after the callback worker's final call. Closing the lane
/// interrupts only its own outstanding registrations before releasing memory.
#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn floe_native_host_free(lane: *mut FloeNativeHostLane) {
    if !lane.is_null() {
        let result = catch_unwind(AssertUnwindSafe(|| unsafe { drop(Box::from_raw(lane)) }));
        if let Err(payload) = result {
            report_free_panic(payload);
        }
    }
}

fn report_free_panic(payload: Box<dyn std::any::Any + Send>) {
    // Diagnostics records only a type and incident ID. Its own sink, or a
    // custom panic payload destructor, must not unwind across an extern ABI.
    if let Err(secondary) = catch_unwind(AssertUnwindSafe(|| {
        let _ = diagnostics::panic_error(payload);
    })) {
        std::mem::forget(secondary);
    }
}
