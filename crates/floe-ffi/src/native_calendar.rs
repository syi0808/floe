use floe_agent::AgentFailure;
use floe_core::{
    ActionFailure, CalendarAction, CalendarActionProvider, CalendarCreateReceipt, CalendarPreflight,
};
use floe_domain::Event;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

pub const LOCAL_PERSON: &str = "00000000-0000-4000-8000-000000000001";

pub struct NativeCalendar {
    pub calendar_ids: Vec<String>,
    local_events: std::sync::Mutex<Vec<Event>>,
}

impl NativeCalendar {
    pub fn new(calendar_ids: Vec<String>) -> Self {
        Self {
            calendar_ids,
            local_events: Default::default(),
        }
    }

    pub fn enabled() -> bool {
        call::<Value>(json!({"operation": "capabilities"}))
            .is_ok_and(|value| value["writes_enabled"] == true)
    }

    fn request<T: DeserializeOwned>(
        &self,
        operation: &str,
        action: &CalendarAction,
    ) -> Result<T, ActionFailure> {
        let local_events = self
            .local_events
            .lock()
            .map_err(|_| ActionFailure::ProviderUnavailable)?
            .clone();
        call(json!({
            "operation": operation, "action": action, "calendar_ids": self.calendar_ids,
            "local_events": local_events,
            "deadline": (chrono::Utc::now() + chrono::Duration::seconds(12)).to_rfc3339(),
        }))
    }
}

impl CalendarActionProvider for NativeCalendar {
    async fn preflight(
        &self,
        action: &CalendarAction,
        local_events: &[Event],
    ) -> Result<CalendarPreflight, ActionFailure> {
        *self
            .local_events
            .lock()
            .map_err(|_| ActionFailure::ProviderUnavailable)? = local_events.to_vec();
        self.request("preflight", action)
    }

    async fn create(
        &self,
        action: &CalendarAction,
    ) -> Result<CalendarCreateReceipt, ActionFailure> {
        self.request("create", action)
    }

    async fn lookup(
        &self,
        action: &CalendarAction,
    ) -> Result<Vec<CalendarCreateReceipt>, ActionFailure> {
        self.request("lookup", action)
    }
}

impl floe_core::CalendarReadAccess for NativeCalendar {
    async fn check(
        &self,
        request: floe_core::CalendarReadAccessRequest,
    ) -> Result<floe_core::CalendarReadAccessStamp, AgentFailure> {
        check_calendar_read(
            request,
            &self.calendar_ids,
            call::<floe_core::CalendarReadAccessStamp>,
        )
        .await
    }
}

async fn check_calendar_read(
    request: floe_core::CalendarReadAccessRequest,
    included: &[String],
    invoke: impl FnOnce(Value) -> Result<floe_core::CalendarReadAccessStamp, ActionFailure>
    + Send
    + 'static,
) -> Result<floe_core::CalendarReadAccessStamp, AgentFailure> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    struct Permit;
    impl Drop for Permit {
        fn drop(&mut self) {
            ACTIVE.store(false, Ordering::Release);
        }
    }
    if request.person_id.to_string() != LOCAL_PERSON
        || request.provider != floe_domain::CalendarProvider::EventKit
        || request.calendar_ids.is_empty()
        || request.calendar_ids.len() > 4
        || request.calendar_ids.iter().any(|identifier| {
            identifier.trim().is_empty() || identifier.len() > 512 || !included.contains(identifier)
        })
        || request
            .calendar_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != request.calendar_ids.len()
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    if request.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    let remaining = request
        .deadline
        .saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    if ACTIVE
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(AgentFailure::CapabilityUnavailable);
    }
    let permit = Permit;
    let native_deadline = chrono::Utc::now()
        + chrono::Duration::from_std(remaining.min(std::time::Duration::from_secs(12)))
            .map_err(|_| AgentFailure::InvalidInput)?;
    let input = json!({"operation": "view_access", "schema_version": 1,
            "person_id": request.person_id, "provider": request.provider,
            "calendar_ids": request.calendar_ids, "deadline": native_deadline.to_rfc3339()});
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("floe-calendar-access".into())
        .spawn(move || {
            let result = invoke(input);
            drop(permit);
            let _ = sender.send(result);
        })
        .map_err(|_| AgentFailure::CapabilityUnavailable)?;
    let result = tokio::select! {
        biased;
        _ = request.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
        _ = tokio::time::sleep_until(request.deadline) => return Err(AgentFailure::DeadlineExceeded),
        result = receiver => result.map_err(|_| AgentFailure::CapabilityUnavailable)?,
    };
    if request.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if tokio::time::Instant::now() >= request.deadline {
        return Err(AgentFailure::DeadlineExceeded);
    }
    result.map_err(|error| match error {
        ActionFailure::PermissionDenied => AgentFailure::CapabilityDenied,
        ActionFailure::Timeout => AgentFailure::DeadlineExceeded,
        _ => AgentFailure::CapabilityUnavailable,
    })
}

fn call<T: DeserializeOwned>(request: Value) -> Result<T, ActionFailure> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = sender.send(invoke(request));
    });
    let value = receiver
        .recv_timeout(std::time::Duration::from_secs(15))
        .map_err(|_| ActionFailure::Timeout)??;
    if let Some(reason) = value.get("error") {
        return Err(
            serde_json::from_value(reason.clone()).unwrap_or(ActionFailure::ProviderUnavailable)
        );
    }
    serde_json::from_value(value["data"].clone()).map_err(|_| ActionFailure::UncertainResult)
}

#[cfg(not(target_os = "macos"))]
fn invoke(_: Value) -> Result<Value, ActionFailure> {
    Err(ActionFailure::ProviderUnavailable)
}

#[cfg(target_os = "macos")]
fn invoke(request: Value) -> Result<Value, ActionFailure> {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    unsafe extern "C" {
        fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> c_int;
    }
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = GATE
        .try_lock()
        .map_err(|_| ActionFailure::ProviderUnavailable)?;
    let executable = std::env::current_exe().map_err(|_| ActionFailure::ProviderUnavailable)?;
    let directory = executable
        .parent()
        .and_then(|path| path.parent())
        .ok_or(ActionFailure::ProviderUnavailable)?;
    let path = CString::new(
        directory
            .join("Frameworks/libfloe_eventkit.dylib")
            .to_string_lossy()
            .as_bytes(),
    )
    .map_err(|_| ActionFailure::ProviderUnavailable)?;
    let input =
        CString::new(request.to_string()).map_err(|_| ActionFailure::ProviderUnavailable)?;
    type Invoke = unsafe extern "C" fn(*const c_char) -> *mut c_char;
    type Release = unsafe extern "C" fn(*mut c_char);
    static FUNCTIONS: std::sync::OnceLock<Result<(Invoke, Release), ActionFailure>> =
        std::sync::OnceLock::new();
    let (invoke, release) = *FUNCTIONS
        .get_or_init(|| unsafe {
            let library = dlopen(path.as_ptr(), 2);
            if library.is_null() {
                return Err(ActionFailure::ProviderUnavailable);
            }
            let invoke = dlsym(library, c"floe_eventkit_action".as_ptr());
            let release = dlsym(library, c"floe_eventkit_free".as_ptr());
            if invoke.is_null() || release.is_null() {
                dlclose(library);
                return Err(ActionFailure::ProviderUnavailable);
            }
            Ok((
                std::mem::transmute::<*mut c_void, Invoke>(invoke),
                std::mem::transmute::<*mut c_void, Release>(release),
            ))
        })
        .as_ref()
        .map_err(|reason| *reason)?;
    unsafe {
        let output = invoke(input.as_ptr());
        if output.is_null() {
            Err(ActionFailure::ProviderUnavailable)
        } else {
            let result = serde_json::from_slice(CStr::from_ptr(output).to_bytes())
                .map_err(|_| ActionFailure::UncertainResult);
            release(output);
            result
        }
    }
}

#[cfg(test)]
mod access_tests;
