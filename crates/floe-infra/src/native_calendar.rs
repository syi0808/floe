use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use floe_agent::{AgentFailure, Cancellation};
use floe_core::{
    ActionFailure, CalendarAction, CalendarActionProvider, CalendarCreateReceipt,
    CalendarObservation, CalendarObserveRequest, CalendarPreflight, CalendarReadAccess,
    CalendarReadAccessRequest, CalendarReadAccessStamp,
};
use floe_domain::{
    AllDaySchedule, CalendarBatch, CalendarFailure, CalendarProvider, CalendarRecord,
    ContextDependency, Event, EventSchedule, PersonId, TimedSchedule,
};
use floe_protocol::{CalendarBatchDto, CalendarFailureDto, EventScheduleDto, PROTOCOL_VERSION};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use tokio::time::Instant;

pub const LOCAL_PERSON: &str = "00000000-0000-4000-8000-000000000001";

fn schedule_from_native(value: EventScheduleDto) -> Result<EventSchedule, AgentFailure> {
    match value {
        EventScheduleDto::Timed {
            starts_at,
            ends_at,
            timezone,
        } => {
            let starts_at = DateTime::parse_from_rfc3339(&starts_at)
                .map_err(|_| AgentFailure::InvalidInput)?
                .with_timezone(&Utc);
            let ends_at = DateTime::parse_from_rfc3339(&ends_at)
                .map_err(|_| AgentFailure::InvalidInput)?
                .with_timezone(&Utc);
            TimedSchedule::new(starts_at, ends_at, timezone)
                .map(EventSchedule::Timed)
                .map_err(|_| AgentFailure::InvalidInput)
        }
        EventScheduleDto::AllDay {
            start_date,
            end_date_exclusive,
        } => {
            let start_date = NaiveDate::parse_from_str(&start_date, "%Y-%m-%d")
                .map_err(|_| AgentFailure::InvalidInput)?;
            let end_date_exclusive = NaiveDate::parse_from_str(&end_date_exclusive, "%Y-%m-%d")
                .map_err(|_| AgentFailure::InvalidInput)?;
            AllDaySchedule::new(start_date, end_date_exclusive)
                .map(EventSchedule::AllDay)
                .map_err(|_| AgentFailure::InvalidInput)
        }
    }
}

fn failure_from_native(value: CalendarFailureDto) -> CalendarFailure {
    match value {
        CalendarFailureDto::PermissionDenied => CalendarFailure::PermissionDenied,
        CalendarFailureDto::CalendarUnavailable => CalendarFailure::CalendarUnavailable,
        CalendarFailureDto::ProviderUnavailable => CalendarFailure::ProviderUnavailable,
    }
}

pub struct NativeCalendar {
    pub calendar_ids: Vec<String>,
    local_events: std::sync::Mutex<Vec<Event>>,
    reviewed_source: std::sync::Mutex<Option<NativeActionSource>>,
}

#[derive(Clone)]
struct NativeActionSource {
    execution_id: uuid::Uuid,
    calendar_ids: Vec<String>,
    fingerprint: String,
}

pub struct NativeCalendarReadAccess {
    person_id: PersonId,
    device_id: String,
    provider: CalendarProvider,
    calendar_ids: Vec<String>,
    connection_id: String,
    connection_revision: u64,
}

impl NativeCalendarReadAccess {
    pub fn new(
        person_id: PersonId,
        device_id: String,
        provider: CalendarProvider,
        calendar_ids: Vec<String>,
        connection_id: String,
        connection_revision: u64,
    ) -> Self {
        let mut calendar_ids = calendar_ids;
        calendar_ids.sort();
        Self {
            person_id,
            device_id,
            provider,
            calendar_ids,
            connection_id,
            connection_revision,
        }
    }

    fn validate_request(
        &self,
        person_id: PersonId,
        device_id: &str,
        provider: CalendarProvider,
        calendar_ids: &[String],
    ) -> Result<(), AgentFailure> {
        if provider != CalendarProvider::EventKit {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        if person_id != self.person_id
            || self.person_id.to_string() != LOCAL_PERSON
            || device_id != self.device_id
            || provider != self.provider
            || self.connection_id.trim().is_empty()
            || self.connection_revision == 0
            || self.device_id.len() > 128
            || self.device_id.chars().any(char::is_control)
            || self.connection_id.len() > 128
            || self.connection_id.chars().any(char::is_control)
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut expected = self.calendar_ids.clone();
        let mut actual = calendar_ids.to_vec();
        expected.sort();
        actual.sort();
        if actual != expected
            || actual.len() != expected.len()
            || actual.iter().collect::<HashSet<_>>().len() != actual.len()
            || expected.iter().collect::<HashSet<_>>().len() != expected.len()
            || actual.iter().any(|id| {
                id.trim().is_empty() || id.len() > 512 || id.chars().any(char::is_control)
            })
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        if actual.is_empty() || actual.len() > 4 || actual.iter().any(|id| id.trim().is_empty()) {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(())
    }

    fn request_base(&self, operation: &str, deadline: Instant) -> Result<Value, AgentFailure> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let deadline = Utc::now()
            .checked_add_signed(
                chrono::Duration::from_std(remaining.min(Duration::from_secs(30)))
                    .map_err(|_| AgentFailure::InvalidInput)?,
            )
            .ok_or(AgentFailure::InvalidInput)?;
        Ok(json!({
            "operation": operation,
            "schema_version": PROTOCOL_VERSION,
            "person_id": self.person_id,
            "device_id": self.device_id,
            "provider": self.provider,
            "connection_id": self.connection_id,
            "connection_revision": self.connection_revision,
            "calendar_ids": self.calendar_ids,
            "item_limit": 128,
            "byte_limit": 65_536,
            "deadline": deadline.to_rfc3339(),
        }))
    }

    async fn native<T: DeserializeOwned + Send + 'static>(
        request: Value,
        deadline: Instant,
        cancellation: Cancellation,
    ) -> Result<T, AgentFailure> {
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        static READ_ADMISSION: OnceLock<Arc<Semaphore>> = OnceLock::new();
        let permit = READ_ADMISSION
            .get_or_init(|| Arc::new(Semaphore::new(1)))
            .clone()
            .try_acquire_owned()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?;
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            call_read(request)
        });
        let result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(AgentFailure::DeadlineExceeded),
            result = task => result.map_err(|_| AgentFailure::Interrupted)?,
        };
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        result.map_err(native_read_failure)
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeObservation {
    stamp: CalendarReadAccessStamp,
    observed_at: DateTime<Utc>,
    batches: Vec<CalendarBatchDto>,
}

impl CalendarReadAccess for NativeCalendarReadAccess {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        self.validate_request(
            request.person_id,
            &request.device_id,
            request.provider,
            &request.calendar_ids,
        )?;
        let input = self.request_base("view_access", request.deadline)?;
        let stamp: CalendarReadAccessStamp =
            Self::native(input, request.deadline, request.cancellation).await?;
        if stamp.schema_version != PROTOCOL_VERSION
            || stamp.person_id != self.person_id
            || stamp.device_id != self.device_id
            || stamp.provider != CalendarProvider::EventKit
            || stamp.calendar_ids != self.calendar_ids
            || stamp.native_subject_fingerprint.len() != 64
            || stamp
                .native_subject_fingerprint
                .bytes()
                .any(|byte| !byte.is_ascii_hexdigit())
            || stamp.generation.trim().is_empty()
            || stamp.generation.len() > 128
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        Ok(stamp)
    }

    async fn observe(
        &self,
        request: CalendarObserveRequest,
    ) -> Result<Option<CalendarObservation>, AgentFailure> {
        self.validate_request(
            request.person_id,
            &request.device_id,
            request.provider,
            &request.calendar_ids,
        )?;
        if request.starts_at >= request.ends_at
            || request.ends_at - request.starts_at > chrono::Duration::days(32)
        {
            return Err(AgentFailure::InvalidInput);
        }
        let before = self
            .check(CalendarReadAccessRequest {
                person_id: request.person_id,
                device_id: request.device_id.clone(),
                provider: request.provider,
                calendar_ids: request.calendar_ids.clone(),
                expected_native_subject_fingerprint: None,
                deadline: request.deadline,
                cancellation: request.cancellation.clone(),
            })
            .await?;
        if let Some(expected) = request.expected_native_subject_fingerprint.as_deref()
            && expected != before.native_subject_fingerprint
        {
            return Err(AgentFailure::AccessReviewRequired);
        }
        let cancellation = request.cancellation.clone();
        let mut input = self.request_base("observe", request.deadline)?;
        if let Some(expected) = request.expected_native_subject_fingerprint.as_deref() {
            input["expected_native_subject_fingerprint"] = json!(expected);
        }
        input["starts_at"] = json!(request.starts_at.to_rfc3339());
        input["ends_at"] = json!(request.ends_at.to_rfc3339());
        let observation: NativeObservation =
            Self::native(input, request.deadline, cancellation.clone()).await?;
        let after = self
            .check(CalendarReadAccessRequest {
                person_id: request.person_id,
                device_id: request.device_id,
                provider: request.provider,
                calendar_ids: request.calendar_ids,
                expected_native_subject_fingerprint: None,
                deadline: request.deadline,
                cancellation: cancellation.clone(),
            })
            .await?;
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= request.deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        if observation.stamp != before || before != after {
            return Err(AgentFailure::StaleContext);
        }
        if observation.observed_at > Utc::now()
            || Utc::now() - observation.observed_at > chrono::Duration::minutes(5)
        {
            return Err(AgentFailure::StaleContext);
        }
        let expected: HashSet<_> = self.calendar_ids.iter().map(String::as_str).collect();
        if observation.batches.len() != expected.len()
            || observation
                .batches
                .iter()
                .map(|batch| batch.calendar_id.as_str())
                .collect::<HashSet<_>>()
                != expected
            || observation
                .batches
                .iter()
                .any(|batch| batch.failure.is_some())
        {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let raw_count = observation
            .batches
            .iter()
            .map(|batch| batch.records.len())
            .sum::<usize>();
        if raw_count > 128 {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut record_ids = HashSet::new();
        if observation.batches.iter().any(|batch| {
            batch.records.iter().any(|record| {
                record.calendar_id != batch.calendar_id
                    || record.external_id.trim().is_empty()
                    || record.external_id.len() > 512
                    || record.external_revision.trim().is_empty()
                    || record.external_revision.len() > 512
                    || record.title.len() > 4096
                    || !record_ids.insert((batch.calendar_id.as_str(), record.external_id.as_str()))
            })
        }) {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        let batches = observation
            .batches
            .into_iter()
            .map(|batch| {
                let records = batch
                    .records
                    .into_iter()
                    .map(|record| {
                        Ok(CalendarRecord {
                            can_modify: record.can_modify,
                            calendar_id: record.calendar_id,
                            external_id: record.external_id,
                            external_revision: record.external_revision,
                            title: record.title,
                            schedule: schedule_from_native(record.schedule)?,
                        })
                    })
                    .collect::<Result<Vec<_>, AgentFailure>>()?;
                Ok(CalendarBatch {
                    calendar_id: batch.calendar_id,
                    records,
                    failure: batch.failure.map(failure_from_native),
                })
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        let count = batches
            .iter()
            .map(|batch| batch.records.len())
            .sum::<usize>();
        if count > 128 {
            return Err(AgentFailure::BudgetExceeded);
        }
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if Instant::now() >= request.deadline {
            return Err(AgentFailure::DeadlineExceeded);
        }
        Ok(Some(CalendarObservation {
            stamp: before,
            observed_at: observation.observed_at,
            batches,
        }))
    }
}

fn native_failure(failure: ActionFailure) -> AgentFailure {
    match failure {
        ActionFailure::PermissionDenied => AgentFailure::CapabilityDenied,
        ActionFailure::ProviderUnavailable => AgentFailure::CapabilityUnavailable,
        ActionFailure::Timeout => AgentFailure::DeadlineExceeded,
        ActionFailure::UncertainResult => AgentFailure::CapabilityUnavailable,
    }
}

enum ReadCallFailure {
    Action(ActionFailure),
    BudgetExceeded,
}

fn native_read_failure(failure: ReadCallFailure) -> AgentFailure {
    match failure {
        ReadCallFailure::BudgetExceeded => AgentFailure::BudgetExceeded,
        ReadCallFailure::Action(ActionFailure::UncertainResult) => {
            AgentFailure::CapabilityUnavailable
        }
        ReadCallFailure::Action(other) => native_failure(other),
    }
}

impl NativeCalendar {
    pub fn new(calendar_ids: Vec<String>) -> Self {
        Self {
            calendar_ids,
            local_events: Default::default(),
            reviewed_source: Default::default(),
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
        let mut request = json!({
            "operation": operation, "action": action, "calendar_ids": self.calendar_ids,
            "local_events": local_events,
            "deadline": (chrono::Utc::now() + chrono::Duration::seconds(12)).to_rfc3339(),
        });
        if action.agent_origin.is_some() && matches!(operation, "preflight" | "create") {
            let source = self
                .reviewed_source
                .lock()
                .map_err(|_| ActionFailure::ProviderUnavailable)?;
            let source = source
                .as_ref()
                .filter(|source| source.execution_id == action.execution_id)
                .ok_or(ActionFailure::PermissionDenied)?;
            request["reviewed_calendar_ids"] = json!(source.calendar_ids);
            request["expected_native_subject_fingerprint"] = json!(source.fingerprint);
        }
        call(request)
    }
}

impl CalendarActionProvider for NativeCalendar {
    async fn validate_source(
        &self,
        action: &CalendarAction,
        dependency: &ContextDependency,
        native_subject_fingerprint: &str,
    ) -> Result<(), ActionFailure> {
        let calendar_ids: Vec<String> = dependency
            .resources()
            .iter()
            .map(|resource| resource.as_str().to_owned())
            .collect();
        if action.provider != CalendarProvider::EventKit
            || action.person_id.to_string() != LOCAL_PERSON
            || dependency.person_id() != action.person_id
            || dependency.source().connector().as_str() != "calendar.event_kit"
            || !calendar_ids.contains(&action.calendar_id)
            || self.calendar_ids != [action.calendar_id.clone()]
        {
            return Err(ActionFailure::PermissionDenied);
        }
        let device_id = dependency.source().execution_owner().as_str().to_owned();
        let access = NativeCalendarReadAccess::new(
            action.person_id,
            device_id.clone(),
            action.provider,
            calendar_ids.clone(),
            dependency.source().connection_id().as_str().to_owned(),
            action.connection_revision,
        );
        let stamp = access
            .check(CalendarReadAccessRequest {
                person_id: action.person_id,
                device_id,
                provider: action.provider,
                calendar_ids: calendar_ids.clone(),
                expected_native_subject_fingerprint: Some(native_subject_fingerprint.to_owned()),
                deadline: Instant::now() + Duration::from_secs(12),
                cancellation: Cancellation::default(),
            })
            .await
            .map_err(|failure| match failure {
                AgentFailure::DeadlineExceeded => ActionFailure::Timeout,
                AgentFailure::CapabilityDenied | AgentFailure::AccessReviewRequired => {
                    ActionFailure::PermissionDenied
                }
                _ => ActionFailure::ProviderUnavailable,
            })?;
        if stamp.native_subject_fingerprint != native_subject_fingerprint {
            return Err(ActionFailure::PermissionDenied);
        }
        *self
            .reviewed_source
            .lock()
            .map_err(|_| ActionFailure::ProviderUnavailable)? = Some(NativeActionSource {
            execution_id: action.execution_id,
            calendar_ids,
            fingerprint: native_subject_fingerprint.to_owned(),
        });
        Ok(())
    }

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

fn call_read<T: DeserializeOwned>(request: Value) -> Result<T, ReadCallFailure> {
    let value = invoke_with_limit(request, Some(65_536)).map_err(|failure| match failure {
        InvokeFailure::Action(action) => ReadCallFailure::Action(action),
        InvokeFailure::ResponseTooLarge => ReadCallFailure::BudgetExceeded,
    })?;
    if let Some(reason) = value.get("error") {
        if reason.as_str() == Some("budget_exceeded") {
            return Err(ReadCallFailure::BudgetExceeded);
        }
        return Err(ReadCallFailure::Action(
            serde_json::from_value(reason.clone()).unwrap_or(ActionFailure::ProviderUnavailable),
        ));
    }
    serde_json::from_value(value["data"].clone())
        .map_err(|_| ReadCallFailure::Action(ActionFailure::UncertainResult))
}

enum InvokeFailure {
    Action(ActionFailure),
    ResponseTooLarge,
}

#[cfg(not(target_os = "macos"))]
fn invoke(_: Value) -> Result<Value, ActionFailure> {
    Err(ActionFailure::ProviderUnavailable)
}

#[cfg(not(target_os = "macos"))]
fn invoke_with_limit(_: Value, _: Option<usize>) -> Result<Value, InvokeFailure> {
    Err(InvokeFailure::Action(ActionFailure::ProviderUnavailable))
}

#[cfg(target_os = "macos")]
fn invoke(request: Value) -> Result<Value, ActionFailure> {
    invoke_with_limit(request, None).map_err(|failure| match failure {
        InvokeFailure::Action(action) => action,
        InvokeFailure::ResponseTooLarge => ActionFailure::UncertainResult,
    })
}

#[cfg(target_os = "macos")]
fn invoke_with_limit(
    request: Value,
    max_response_bytes: Option<usize>,
) -> Result<Value, InvokeFailure> {
    use std::ffi::{CStr, CString, c_char, c_int, c_void};
    unsafe extern "C" {
        fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> c_int;
    }
    static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = GATE
        .try_lock()
        .map_err(|_| InvokeFailure::Action(ActionFailure::ProviderUnavailable))?;
    let executable = std::env::current_exe()
        .map_err(|_| InvokeFailure::Action(ActionFailure::ProviderUnavailable))?;
    let directory = executable
        .parent()
        .and_then(|path| path.parent())
        .ok_or(InvokeFailure::Action(ActionFailure::ProviderUnavailable))?;
    let path = CString::new(
        directory
            .join("Frameworks/libfloe_eventkit.dylib")
            .to_string_lossy()
            .as_bytes(),
    )
    .map_err(|_| InvokeFailure::Action(ActionFailure::ProviderUnavailable))?;
    let input = CString::new(request.to_string())
        .map_err(|_| InvokeFailure::Action(ActionFailure::ProviderUnavailable))?;
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
        .map_err(|reason| InvokeFailure::Action(*reason))?;
    unsafe {
        let output = invoke(input.as_ptr());
        if output.is_null() {
            Err(InvokeFailure::Action(ActionFailure::ProviderUnavailable))
        } else {
            let bytes = if let Some(limit) = max_response_bytes {
                let mut length = 0;
                while length <= limit && *output.add(length) != 0 {
                    length += 1;
                }
                if length > limit {
                    release(output);
                    return Err(InvokeFailure::ResponseTooLarge);
                }
                std::slice::from_raw_parts(output.cast::<u8>(), length)
            } else {
                CStr::from_ptr(output).to_bytes()
            };
            let result = serde_json::from_slice(bytes)
                .map_err(|_| InvokeFailure::Action(ActionFailure::UncertainResult));
            release(output);
            result
        }
    }
}

#[cfg(test)]
mod conversion_tests {
    use super::*;

    #[test]
    fn native_schedule_conversion_preserves_domain_interval_validation() {
        let timed = |ends_at: &str| EventScheduleDto::Timed {
            starts_at: "2026-09-14T10:00:00+09:00".into(),
            ends_at: ends_at.into(),
            timezone: " Asia/Seoul ".into(),
        };
        let EventSchedule::Timed(schedule) =
            schedule_from_native(timed("2026-09-14T11:00:00+09:00")).unwrap()
        else {
            panic!("expected timed schedule");
        };
        assert_eq!(schedule.timezone, "Asia/Seoul");
        assert_eq!(schedule.starts_at.to_rfc3339(), "2026-09-14T01:00:00+00:00");
        assert_eq!(
            schedule_from_native(timed("2026-09-14T10:00:00+09:00")),
            Err(AgentFailure::InvalidInput)
        );
        assert_eq!(
            schedule_from_native(EventScheduleDto::AllDay {
                start_date: "2026-09-14".into(),
                end_date_exclusive: "2026-09-14".into(),
            }),
            Err(AgentFailure::InvalidInput)
        );
        assert!(
            schedule_from_native(EventScheduleDto::AllDay {
                start_date: "2026-09-14".into(),
                end_date_exclusive: "2026-09-15".into(),
            })
            .is_ok()
        );
    }
}
