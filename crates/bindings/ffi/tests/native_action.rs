#![cfg(target_os = "macos")]

use floe_agent_contract::{AgentFailure};
use floe_execution::{Cancellation};
use floe_access::{CalendarReadAccess, CalendarReadAccessRequest};
use floe_actions::{ActionFailure, CalendarAction, CalendarActionProvider, CalendarActionState};
use floe_day::{CalendarObserveRequest};
use floe_context_contract::{ConnectionId, ContextDependency, GrantAuthority, GrantConsumer, GrantDataCategory, GrantOperation, GrantPurpose, GrantScope, GrantSourceBinding, ProcessingRestriction, ResourceHandle, SourceAuthority};
use floe_day::{CalendarProvider, TimedSchedule};
use floe_kernel::{PersonId};
use floe_ffi::*;
use floe_provider_adapters::sources::native_calendar::{NativeCalendar, NativeCalendarReadAccess};
use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString},
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    process::Command,
};
use tokio::time::Instant;

const PERSON: &str = "00000000-0000-4000-8000-000000000001";

struct FixtureLock {
    file: File,
}

impl FixtureLock {
    fn acquire() -> Self {
        if std::env::var_os("FLOE_NATIVE_FIXTURE_CHILD").is_some() {
            panic!("fixture child must not acquire the parent fixture lock");
        }
        let path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("native-calendar-fixture.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        const LOCK_EX: i32 = 2;
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        assert_eq!(unsafe { flock(file.as_raw_fd(), LOCK_EX) }, 0);
        Self { file }
    }
}

impl Drop for FixtureLock {
    fn drop(&mut self) {
        const LOCK_UN: i32 = 8;
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        assert_eq!(unsafe { flock(self.file.as_raw_fd(), LOCK_UN) }, 0);
    }
}

fn run_read_fixture_child(test_name: &str, mode: &str) -> bool {
    if std::env::var_os("FLOE_NATIVE_FIXTURE_CHILD").is_some() {
        return false;
    }
    let _lock = FixtureLock::acquire();
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("Contents/MacOS/test-host");
    let frameworks = directory.path().join("Contents/Frameworks");
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&frameworks).unwrap();
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    assert!(
        Command::new("xcrun")
            .args(["swiftc", "-emit-library", "-warnings-as-errors"])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/NativeCalendarFixture.swift"
            ))
            .arg("-o")
            .arg(frameworks.join("libfloe_eventkit.dylib"))
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new(executable)
            .args(["--exact", test_name, "--nocapture"])
            .env("FLOE_NATIVE_FIXTURE_CHILD", "1")
            .env("FLOE_NATIVE_READ_FIXTURE", mode)
            .status()
            .unwrap()
            .success()
    );
    true
}

fn read_access() -> NativeCalendarReadAccess {
    read_access_with_ids(vec!["target".into()])
}

fn read_access_with_ids(calendar_ids: Vec<String>) -> NativeCalendarReadAccess {
    NativeCalendarReadAccess::new(
        PersonId(uuid::Uuid::parse_str(PERSON).unwrap()),
        "test-device".into(),
        CalendarProvider::EventKit,
        calendar_ids,
        "00000000-0000-4000-8000-000000000010".into(),
        1,
    )
}

fn observe_request(deadline: Instant, cancellation: Cancellation) -> CalendarObserveRequest {
    observe_request_with_ids(deadline, cancellation, vec!["target".into()])
}

fn observe_request_with_ids(
    deadline: Instant,
    cancellation: Cancellation,
    calendar_ids: Vec<String>,
) -> CalendarObserveRequest {
    CalendarObserveRequest {
        person_id: PersonId(uuid::Uuid::parse_str(PERSON).unwrap()),
        device_id: "test-device".into(),
        provider: CalendarProvider::EventKit,
        calendar_ids,
        expected_native_subject_fingerprint: None,
        starts_at: chrono::Utc::now(),
        ends_at: chrono::Utc::now() + chrono::Duration::hours(1),
        deadline,
        cancellation,
    }
}

fn check_request(deadline: Instant, cancellation: Cancellation) -> CalendarReadAccessRequest {
    CalendarReadAccessRequest {
        person_id: PersonId(uuid::Uuid::parse_str(PERSON).unwrap()),
        device_id: "test-device".into(),
        provider: CalendarProvider::EventKit,
        calendar_ids: vec!["target".into()],
        expected_native_subject_fingerprint: None,
        deadline,
        cancellation,
    }
}

struct Core(*mut FloeHandle);

impl Core {
    fn open(path: &std::path::Path) -> Self {
        let path = CString::new(path.to_str().unwrap()).unwrap();
        let mut error = std::ptr::null_mut();
        let handle = unsafe { floe_core_open(path.as_ptr(), &mut error) };
        assert!(!handle.is_null());
        assert!(error.is_null());
        Self(handle)
    }

    fn call(&self, request: Value, action: bool) -> Value {
        let input = CString::new(request.to_string()).unwrap();
        unsafe {
            let output = if action {
                floe_core_calendar_actions(self.0, input.as_ptr())
            } else {
                floe_core_execute(self.0, input.as_ptr())
            };
            let value = serde_json::from_slice(CStr::from_ptr(output).to_bytes()).unwrap();
            floe_string_free(output);
            value
        }
    }

    fn action(&self, operation: Value) -> Value {
        self.call(
            json!({"schema_version": 1, "person_id": PERSON, "operation": operation}),
            true,
        )
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        unsafe { floe_core_free(self.0) };
    }
}

fn data(response: Value) -> Value {
    assert_eq!(response["status"], "ok", "{response}");
    response["data"].clone()
}

#[test]
fn native_executor_uses_rust_ledger_and_lookup_only_after_response_loss() {
    if std::env::var_os("FLOE_NATIVE_FIXTURE_CHILD").is_none() {
        let _lock = FixtureLock::acquire();
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("Contents/MacOS/test-host");
        let frameworks = directory.path().join("Contents/Frameworks");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&frameworks).unwrap();
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        assert!(
            Command::new("xcrun")
                .args(["swiftc", "-emit-library", "-warnings-as-errors"])
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/NativeCalendarFixture.swift"
                ))
                .arg("-o")
                .arg(frameworks.join("libfloe_eventkit.dylib"))
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new(executable)
                .args([
                    "--exact",
                    "native_executor_uses_rust_ledger_and_lookup_only_after_response_loss",
                    "--nocapture"
                ])
                .env("FLOE_NATIVE_FIXTURE_CHILD", "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let core = Core::open(&path);
    let now = chrono::Utc::now();
    let day = json!({"date": now.format("%Y-%m-%d").to_string(), "timezone_offset_seconds": 0, "now": now.to_rfc3339()});
    data(core.call(
        json!({"schema_version": 1, "person_id": PERSON, "day": day,
        "command": {"type": "set_calendar_scope",
            "connection_id": "00000000-0000-4000-8000-000000000010",
            "connection_revision": 1, "device_id": "test-device",
            "provider": "event_kit", "scope": "selected",
            "calendars": [{"calendar_id": "target", "calendar_name": "Fixture · Target"}]}}),
        false,
    ));
    assert_eq!(
        data(core.action(json!({"kind": "capabilities"})))["writes_enabled"],
        true
    );
    let propose = |title: &str| {
        json!({"kind": "propose", "calendar_id": "target", "title": title,
        "starts_at": (now + chrono::Duration::hours(1)).to_rfc3339(),
        "ends_at": (now + chrono::Duration::hours(2)).to_rfc3339(), "timezone": "Etc/UTC"})
    };
    let proposal = data(core.action(propose("Lost response")))["actions"][0].clone();
    let id = &proposal["id"];
    assert_eq!(
        core.action(json!({"kind": "execute", "action_id": id}))["error"]["code"],
        "conflict"
    );
    data(core.action(json!({"kind": "decide", "action_id": id, "decision": "approve"})));
    let result = data(core.action(json!({"kind": "execute", "action_id": id})));
    assert_eq!(
        result["actions"][0]["state"],
        json!({"status": "unknown", "reason": "timeout"})
    );
    drop(core);
    let core = Core::open(&path);
    assert_eq!(
        core.action(json!({"kind": "execute", "action_id": id}))["error"]["code"],
        "conflict"
    );
    let recovered = data(core.action(json!({"kind": "recover", "action_id": id})));
    assert_eq!(
        recovered["actions"][0]["state"],
        json!({"status": "succeeded", "external_id": "native-fixture-event|"})
    );
    assert_eq!(
        recovered["actions"][0]["execution_id"],
        proposal["execution_id"]
    );
    let blocked = data(core.action(propose("Conflict")))["actions"][0]["id"].clone();
    data(core.action(json!({"kind": "decide", "action_id": blocked, "decision": "approve"})));
    let result = data(core.action(json!({"kind": "execute", "action_id": blocked})));
    assert_eq!(
        result["actions"][0]["state"],
        json!({"status": "blocked", "reason": "schedule_conflict"})
    );
    let trace = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("creates.txt");
    assert_eq!(std::fs::read_to_string(trace).unwrap(), "create\n");
}

#[tokio::test]
async fn native_read_accepts_exact_subset_and_fences_generation() {
    if run_read_fixture_child(
        "native_read_accepts_exact_subset_and_fences_generation",
        "valid",
    ) {
        return;
    }
    let access = read_access();
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    let stamp = access
        .check(check_request(deadline, Cancellation::default()))
        .await
        .unwrap();
    assert_eq!(stamp.calendar_ids, vec!["target"]);
    let observation = access
        .observe(observe_request(deadline, Cancellation::default()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(observation.stamp, stamp);
    assert_eq!(observation.batches.len(), 1);
    assert_eq!(observation.batches[0].records.len(), 1);
}

#[tokio::test]
async fn native_read_rejects_changed_generation() {
    if run_read_fixture_child(
        "native_read_rejects_changed_generation",
        "changed_generation",
    ) {
        return;
    }
    let result = read_access()
        .observe(observe_request(
            Instant::now() + std::time::Duration::from_secs(5),
            Cancellation::default(),
        ))
        .await;
    assert!(matches!(result, Err(AgentFailure::StaleContext)));
}

#[tokio::test]
async fn native_action_source_validation_rejects_wrong_subject() {
    if run_read_fixture_child(
        "native_action_source_validation_rejects_wrong_subject",
        "valid",
    ) {
        return;
    }
    let person = PersonId(uuid::Uuid::parse_str(PERSON).unwrap());
    let provider = NativeCalendar::new(vec!["target".into()]);
    let now = chrono::Utc::now();
    let action = CalendarAction {
        agent_origin: None,
        direct: false,
        mutation: None,
        id: uuid::Uuid::new_v4(),
        person_id: person,
        provider: CalendarProvider::EventKit,
        calendar_id: "target".into(),
        calendar_name: "Target".into(),
        title: "Focus time".into(),
        schedule: TimedSchedule::new(
            now + chrono::Duration::hours(1),
            now + chrono::Duration::hours(2),
            "Etc/UTC",
        )
        .unwrap(),
        connection_revision: 1,
        created_at: now,
        expires_at: now + chrono::Duration::minutes(10),
        approved_at: Some(now),
        execution_id: uuid::Uuid::new_v4(),
        state: CalendarActionState::Approved,
    };
    let source = GrantSourceBinding::try_new(
        person,
        ConnectionId::try_new("00000000-0000-4000-8000-000000000010").unwrap(),
        floe_context_contract::ConnectorId::try_new("calendar.event_kit").unwrap(),
        floe_context_contract::ExecutionOwnerId::try_new("test-device").unwrap(),
        SourceAuthority::new(),
    )
    .unwrap();
    let consumer = GrantConsumer::builtin("calendar.expert").unwrap();
    let scope = GrantScope::try_new(
        vec![ResourceHandle::try_new("target").unwrap()],
        vec![GrantDataCategory::Metadata],
        vec![GrantOperation::Read],
        vec![GrantPurpose::Assistant],
        vec![consumer.clone()],
        ProcessingRestriction::LocalOnly,
    )
    .unwrap();
    let dependency = ContextDependency::try_new(
        person,
        floe_context_contract::GrantId::new(),
        GrantAuthority::new(),
        source,
        scope.resources().to_vec(),
        scope.categories().to_vec(),
        GrantOperation::Read,
        GrantPurpose::Assistant,
        consumer,
        ProcessingRestriction::LocalOnly,
        floe_context_contract::ConsumerPolicyAuthority::new(),
        uuid::Uuid::new_v4(),
        vec![1],
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        now,
        now + chrono::Duration::minutes(5),
    )
    .unwrap();
    provider
        .validate_source(&action, &dependency, &"a".repeat(64))
        .await
        .unwrap();
    assert_eq!(
        provider
            .validate_source(&action, &dependency, &"b".repeat(64))
            .await,
        Err(ActionFailure::PermissionDenied)
    );
}

#[tokio::test]
async fn native_read_rejects_subject_change_before_events_query() {
    if run_read_fixture_child(
        "native_read_rejects_subject_change_before_events_query",
        "changed_subject",
    ) {
        return;
    }
    let mut request = observe_request(
        Instant::now() + std::time::Duration::from_secs(5),
        Cancellation::default(),
    );
    request.expected_native_subject_fingerprint = Some("a".repeat(64));
    let result = read_access().observe(request).await;
    assert!(matches!(result, Err(AgentFailure::CapabilityDenied)));
}

#[tokio::test]
async fn native_read_rejects_malformed_and_oversized_results() {
    if run_read_fixture_child(
        "native_read_rejects_malformed_and_oversized_results",
        "malformed",
    ) {
        return;
    }
    let malformed = read_access()
        .check(check_request(
            Instant::now() + std::time::Duration::from_secs(5),
            Cancellation::default(),
        ))
        .await;
    assert_eq!(malformed, Err(AgentFailure::CapabilityUnavailable));
}

#[tokio::test]
async fn native_read_rejects_oversized_results() {
    if run_read_fixture_child("native_read_rejects_oversized_results", "oversized") {
        return;
    }
    let result = read_access()
        .observe(observe_request(
            Instant::now() + std::time::Duration::from_secs(5),
            Cancellation::default(),
        ))
        .await;
    assert!(matches!(result, Err(AgentFailure::BudgetExceeded)));
}

#[tokio::test]
async fn native_read_rejects_raw_oversized_results() {
    if run_read_fixture_child("native_read_rejects_raw_oversized_results", "raw_oversized") {
        return;
    }
    let result = read_access()
        .observe(observe_request(
            Instant::now() + std::time::Duration::from_secs(5),
            Cancellation::default(),
        ))
        .await;
    assert!(matches!(result, Err(AgentFailure::BudgetExceeded)));
}

#[tokio::test]
async fn native_read_rejects_aggregate_oversized_results() {
    if run_read_fixture_child(
        "native_read_rejects_aggregate_oversized_results",
        "aggregate_oversized",
    ) {
        return;
    }
    let result = read_access_with_ids(vec!["first".into(), "second".into()])
        .observe(observe_request_with_ids(
            Instant::now() + std::time::Duration::from_secs(5),
            Cancellation::default(),
            vec!["first".into(), "second".into()],
        ))
        .await;
    assert!(matches!(result, Err(AgentFailure::BudgetExceeded)));
}

#[tokio::test]
async fn native_read_rejects_duplicate_batches() {
    if run_read_fixture_child("native_read_rejects_duplicate_batches", "duplicate_batch") {
        return;
    }
    let result = read_access()
        .observe(observe_request(
            Instant::now() + std::time::Duration::from_secs(5),
            Cancellation::default(),
        ))
        .await;
    assert!(matches!(result, Err(AgentFailure::CapabilityUnavailable)));
}

#[tokio::test]
async fn native_read_rejects_duplicate_records() {
    if run_read_fixture_child("native_read_rejects_duplicate_records", "duplicate_record") {
        return;
    }
    let result = read_access()
        .observe(observe_request(
            Instant::now() + std::time::Duration::from_secs(5),
            Cancellation::default(),
        ))
        .await;
    assert!(matches!(result, Err(AgentFailure::CapabilityUnavailable)));
}

#[tokio::test]
async fn native_read_rejects_partial_batches() {
    if run_read_fixture_child("native_read_rejects_partial_batches", "partial") {
        return;
    }
    let result = read_access()
        .observe(observe_request(
            Instant::now() + std::time::Duration::from_secs(5),
            Cancellation::default(),
        ))
        .await;
    assert!(matches!(result, Err(AgentFailure::CapabilityUnavailable)));
}

#[tokio::test]
async fn native_read_cancellation_and_deadline_do_not_accept_late_results() {
    if run_read_fixture_child(
        "native_read_cancellation_and_deadline_do_not_accept_late_results",
        "late",
    ) {
        return;
    }
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert!(matches!(
        read_access()
            .observe(observe_request(
                Instant::now() + std::time::Duration::from_secs(5),
                cancellation,
            ))
            .await,
        Err(AgentFailure::Cancelled)
    ));
    let result = read_access()
        .check(check_request(
            Instant::now() + std::time::Duration::from_millis(50),
            Cancellation::default(),
        ))
        .await;
    assert_eq!(result, Err(AgentFailure::DeadlineExceeded));
}

#[tokio::test]
async fn native_read_rejects_wrong_identity_before_provider_call() {
    if run_read_fixture_child(
        "native_read_rejects_wrong_identity_before_provider_call",
        "valid",
    ) {
        return;
    }
    let mut request = check_request(
        Instant::now() + std::time::Duration::from_secs(5),
        Cancellation::default(),
    );
    request.device_id = "other-device".into();
    assert_eq!(
        read_access().check(request).await,
        Err(AgentFailure::CapabilityDenied)
    );
    let mut request = check_request(
        Instant::now() + std::time::Duration::from_secs(5),
        Cancellation::default(),
    );
    request.calendar_ids = vec!["target".into(), "target".into()];
    assert_eq!(
        read_access().check(request).await,
        Err(AgentFailure::CapabilityDenied)
    );
    let mut request = check_request(
        Instant::now() + std::time::Duration::from_secs(5),
        Cancellation::default(),
    );
    request.provider = CalendarProvider::Android;
    assert_eq!(
        read_access().check(request).await,
        Err(AgentFailure::CapabilityUnavailable)
    );
}
