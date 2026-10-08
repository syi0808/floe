//! QA regression through the exported C ABI and AppWire envelopes.

use super::{
    FloeHandle, floe_core_command_v2, floe_core_free, floe_core_open_default, floe_core_query_v2,
    floe_string_free,
};
use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString, c_char},
    fs,
    path::PathBuf,
    ptr, thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

struct TestProfile(PathBuf);

impl TestProfile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("floe-day-refresh-ffi-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create disposable QA support directory");
        Self(path)
    }
}

impl Drop for TestProfile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct TestCore(*mut FloeHandle);

impl TestCore {
    fn open(profile: &TestProfile) -> Self {
        let path = CString::new(profile.0.to_string_lossy().as_bytes())
            .expect("temporary profile path contains no NUL");
        let mut error = ptr::null_mut();
        let handle = unsafe { floe_core_open_default(path.as_ptr(), &mut error) };
        if handle.is_null() {
            let response = response_from_ptr(error);
            panic!(
                "open disposable QA profile failed: {:?}",
                response["status"]
            );
        }
        assert!(
            error.is_null(),
            "successful open returned an error envelope"
        );
        Self(handle)
    }
}

impl Drop for TestCore {
    fn drop(&mut self) {
        unsafe { floe_core_free(self.0) };
    }
}

fn response_from_ptr(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null(), "FFI returned a null JSON response");
    let encoded = unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned();
    unsafe { floe_string_free(pointer) };
    serde_json::from_str(&encoded).expect("FFI response is valid JSON")
}

fn query(core: &TestCore, query: Value) -> Value {
    let request = json!({
        "schema_version": 2,
        "request_id": Uuid::new_v4().to_string(),
        "query": query,
    });
    let encoded = CString::new(request.to_string()).expect("request JSON contains no NUL");
    unsafe { response_from_ptr(floe_core_query_v2(core.0, encoded.as_ptr())) }
}

fn command(core: &TestCore, command_id: Uuid, command: Value) -> Value {
    let request = json!({
        "schema_version": 2,
        "request_id": Uuid::new_v4().to_string(),
        "command_id": command_id.to_string(),
        "command": command,
    });
    let encoded = CString::new(request.to_string()).expect("request JSON contains no NUL");
    unsafe { response_from_ptr(floe_core_command_v2(core.0, encoded.as_ptr())) }
}

fn result(response: &Value) -> &Value {
    assert_eq!(response["status"], "ok", "AppWire request did not succeed");
    &response["result"]
}

fn day_query() -> Value {
    json!({
        "date": "2026-10-08",
        "timezone_offset_seconds": 32_400,
        "end_timezone_offset_seconds": 32_400,
        "now": "2026-10-08T00:00:00Z",
    })
}

fn day_snapshot(core: &TestCore, day: &Value) -> Value {
    let response = query(core, json!({ "kind": "day.snapshot", "day": day }));
    let result = result(&response);
    assert_eq!(result["kind"], "day_snapshot");
    result["snapshot"].clone()
}

fn day_refresh(core: &TestCore, command_id: Uuid, day: &Value) -> Value {
    command(
        core,
        command_id,
        json!({ "kind": "day.refresh", "day": day }),
    )
}

fn poll_day_refresh(core: &TestCore, operation_ref: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let response = query(
            core,
            json!({
                "kind": "day.refresh.get",
                "operation_ref": operation_ref,
            }),
        );
        let state = &result(&response)["refresh"];
        match state["state"].as_str() {
            Some("completed") | Some("failed") | Some("interrupted") => {
                return state.clone();
            }
            _ => {}
        }
        assert!(
            Instant::now() < deadline,
            "Day refresh did not reach a terminal state"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn ensure_runtime_ready(core: &TestCore) {
    let readiness = query(core, json!({ "kind": "runtime.readiness" }));
    match result(&readiness)["state"].as_str() {
        Some("ready") => return,
        Some("preparation_required") => {}
        state => panic!("unexpected Runtime readiness state: {state:?}"),
    }

    let operation_id = Uuid::new_v4();
    let prepare = command(core, operation_id, json!({ "kind": "runtime.prepare" }));
    let mut preparation = result(&prepare).clone();
    assert_eq!(preparation["kind"], "runtime.preparation");
    assert_eq!(preparation["operation_id"], operation_id.to_string());
    let deadline = Instant::now() + Duration::from_secs(60);
    while preparation["done"] != true {
        assert!(
            Instant::now() < deadline,
            "Runtime preparation did not finish"
        );
        let observed = query(
            core,
            json!({
                "kind": "runtime.preparation.get",
                "operation_id": operation_id,
            }),
        );
        preparation = result(&observed).clone();
        assert_eq!(preparation["operation_id"], operation_id.to_string());
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        preparation["failure"].is_null(),
        "Runtime preparation returned a failure"
    );
    let acknowledge = command(
        core,
        operation_id,
        json!({ "kind": "runtime.preparation.acknowledge" }),
    );
    assert_eq!(
        result(&acknowledge)["operation_id"],
        operation_id.to_string()
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let readiness = query(core, json!({ "kind": "runtime.readiness" }));
        match result(&readiness)["state"].as_str() {
            Some("ready") => return,
            Some("preparation_required") => {}
            state => panic!("unexpected Runtime readiness state: {state:?}"),
        }
        assert!(Instant::now() < deadline, "Runtime did not become ready");
        thread::sleep(Duration::from_millis(20));
    }
}

fn poll_connection_operation(core: &TestCore, operation_ref: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let response = query(
            core,
            json!({
                "kind": "connections.operation.get",
                "operation_ref": operation_ref,
            }),
        );
        let operation = &result(&response)["operation"];
        match operation["state"].as_str() {
            Some("completed") | Some("failed") | Some("cancelled") => {
                return operation.clone();
            }
            _ => {}
        }
        assert!(
            Instant::now() < deadline,
            "Synthetic integration setup did not reach a terminal state"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn connect_synthetic_team_calendar(core: &TestCore) {
    let overview = query(core, json!({ "kind": "connections.overview" }));
    let overview = result(&overview);
    let integration = overview["overview"]["integrations"]
        .as_array()
        .expect("Connections overview has an integration list")
        .iter()
        .find(|value| value["service_kind"] == "synthetic_qa_calendar")
        .expect("QA fixture integration is available");
    let integration_ref = integration["integration_ref"]
        .as_str()
        .expect("integration ref is a string")
        .to_owned();
    let integration_revision = integration["revision"]
        .as_u64()
        .expect("integration revision is a number");

    let review_response = command(
        core,
        Uuid::new_v4(),
        json!({
            "kind": "connections.integration.prepare_review",
            "integration_ref": integration_ref,
            "expected_revision": integration_revision,
        }),
    );
    let review = result(&review_response)["review"].clone();
    assert_eq!(review["integration_ref"], integration_ref);

    let start_command_id = Uuid::new_v4();
    let start_response = command(
        core,
        start_command_id,
        json!({
            "kind": "connections.integration.start",
            "integration_ref": integration_ref,
            "review_ref": review["review_ref"],
            "expected_revision": integration_revision,
        }),
    );
    let pending = &result(&start_response)["operation"];
    let operation_ref = pending["operation_ref"]
        .as_str()
        .expect("setup operation ref is a string")
        .to_owned();
    let operation = poll_connection_operation(core, &operation_ref);
    assert_eq!(operation["state"], "completed");
    let source = &operation["source"];
    let source_ref = source["source_ref"]
        .as_str()
        .expect("source ref is a string")
        .to_owned();
    let source_revision = source["revision"]
        .as_u64()
        .expect("source revision is a number");

    let source_review_response = command(
        core,
        Uuid::new_v4(),
        json!({
            "kind": "connections.source.prepare_review",
            "source_ref": source_ref,
            "expected_revision": source_revision,
        }),
    );
    let source_review = result(&source_review_response)["review"].clone();
    let team_resource = source_review["permitted_choices"]
        .as_array()
        .expect("source review has permitted resources")
        .iter()
        .find(|choice| choice["label"] == "Synthetic team calendar")
        .expect("synthetic team calendar is selectable")["resource_ref"]
        .clone();

    let configure = command(
        core,
        Uuid::new_v4(),
        json!({
            "kind": "connections.source.configure",
            "source_ref": source_ref,
            "review_ref": source_review["review_ref"],
            "selected_resource_refs": [team_resource],
            "expected_revision": source_review["source_revision"],
        }),
    );
    assert!(result(&configure).is_object());

    let overview = query(core, json!({ "kind": "connections.overview" }));
    let sources = result(&overview)["overview"]["sources"]
        .as_array()
        .expect("Connections overview has a source list");
    let selected = sources
        .iter()
        .find(|value| value["source_ref"] == source_ref)
        .expect("configured synthetic source remains visible");
    assert_eq!(
        selected["selected_resources"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        selected["selected_resources"][0]["resource_ref"],
        team_resource
    );
}

#[test]
fn day_refresh_uses_distinct_operation_identity_through_appwire_ffi() {
    let profile = TestProfile::new();
    let core = TestCore::open(&profile);
    let day = day_query();

    // A fresh profile exposes a valid empty Day before Runtime preparation.
    let empty = day_snapshot(&core, &day);
    assert_eq!(empty["date"], "2026-10-08");
    assert_eq!(empty["timezone_offset_seconds"], 32_400);
    assert!(empty["items"].as_array().is_some_and(Vec::is_empty));

    ensure_runtime_ready(&core);

    let zero_source_command_id = Uuid::new_v4();
    let first = day_refresh(&core, zero_source_command_id, &day);
    let first_result = result(&first);
    assert_eq!(first_result["kind"], "day.refresh");
    let first_refresh = first_result["refresh"].clone();
    let zero_source_operation_ref = first_refresh["operation_ref"]
        .as_str()
        .expect("refresh returns an operation ref")
        .to_owned();
    assert_ne!(
        zero_source_operation_ref,
        zero_source_command_id.to_string()
    );

    let completed = poll_day_refresh(&core, &zero_source_operation_ref);
    assert_eq!(completed["state"], "completed");
    assert!(
        completed["day"]["items"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );

    // Exact replay returns the original owner operation and does not admit a
    // second acquisition, even after the first operation has completed.
    let replay = day_refresh(&core, zero_source_command_id, &day);
    let replay_refresh = &result(&replay)["refresh"];
    assert_eq!(replay_refresh["operation_ref"], zero_source_operation_ref);
    assert_eq!(replay_refresh["state"], "completed");

    let mut changed_body = day.clone();
    changed_body["date"] = json!("2026-10-09");
    let changed = day_refresh(&core, zero_source_command_id, &changed_body);
    assert_eq!(changed["status"], "command_error");
    assert_eq!(changed["disposition"], "indeterminate");
    assert_eq!(changed["error"]["code"], "conflict");

    let unknown_operation = query(
        &core,
        json!({
            "kind": "day.refresh.get",
            "operation_ref": zero_source_command_id.to_string(),
        }),
    );
    assert_eq!(unknown_operation["status"], "error");

    connect_synthetic_team_calendar(&core);
    let selected_command_id = Uuid::new_v4();
    let selected = day_refresh(&core, selected_command_id, &day);
    let selected_result = result(&selected);
    assert_eq!(selected_result["kind"], "day.refresh");
    let selected_operation_ref = selected_result["refresh"]["operation_ref"]
        .as_str()
        .expect("selected refresh returns an operation ref")
        .to_owned();
    assert_ne!(selected_operation_ref, selected_command_id.to_string());

    let completed = poll_day_refresh(&core, &selected_operation_ref);
    assert_eq!(completed["state"], "completed");
    assert_eq!(completed["day"]["date"], "2026-10-08");
    assert_eq!(completed["day"]["timezone_offset_seconds"], 32_400);
    assert_eq!(completed["day"]["items"].as_array().map(Vec::len), Some(1));
    let calendar = &completed["day"]["calendar"];
    assert_eq!(calendar["sources"].as_array().map(Vec::len), Some(1));
    assert_eq!(calendar["sources"][0]["state"], "current");
    assert_eq!(calendar["sources"][0]["resources"][0]["state"], "current");
}
