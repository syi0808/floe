use super::*;
use serde::de::DeserializeOwned;

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
    if !error_json_out.is_null() {
        unsafe { *error_json_out = ptr::null_mut() };
    }
    let operation = || -> BridgeResult<*mut FloeHandle> {
        let path = c_input(path, "path")?;
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|value| error(ErrorCodeDto::Internal, value.to_string()))?;
        let core = Arc::new(runtime.block_on(FloeCore::open(path)).map_err(core_error)?);
        let local_context = Arc::new(local_context::LocalContextStore::default());
        Ok(Box::into_raw(Box::new(FloeHandle {
            runtime,
            core: core.clone(),
            agent_runs: Default::default(),
            local_context: local_context.clone(),
            #[cfg(unix)]
            agent_vault: vault_host::VaultBridge::new(path, core, local_context),
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
    invoke_json(handle_ptr, request_json, agent_run::run)
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
