use std::ffi::{CStr, CString, c_char};

use floe_ffi::*;
use serde_json::{Value, json};
use uuid::Uuid;

const PERSON: &str = "00000000-0000-4000-8000-000000000001";

struct Core(*mut FloeHandle);

impl Core {
    fn open(path: &str) -> Self {
        let supplied = std::path::Path::new(path);
        let product_path = supplied
            .parent()
            .and_then(std::path::Path::parent)
            .and_then(std::path::Path::file_name)
            .is_some_and(|name| name == "people");
        let profile;
        let path = if product_path {
            supplied
        } else {
            let root = supplied.parent().unwrap();
            let identity = root.join("local_device_id");
            if !identity.exists() {
                std::fs::write(identity, "mac-local").unwrap();
            }
            profile = root.join("people").join(PERSON).join("floe.db");
            std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
            &profile
        };
        let path = CString::new(path.to_str().unwrap()).unwrap();
        let mut error = std::ptr::null_mut();
        let handle = unsafe { floe_core_open(path.as_ptr(), &mut error) };
        if !error.is_null() {
            panic!("open failed: {}", take_json(error));
        }
        assert!(!handle.is_null());
        Self(handle)
    }

    fn execute(&self, request: Value) -> Value {
        let request = CString::new(request.to_string()).unwrap();
        take_json(unsafe { floe_core_command_v2(self.0, request.as_ptr()) })
    }

    fn load(&self, day: Value) -> Value {
        self.query_v2(query(json!({"kind":"day.snapshot", "day":day})))
    }
    fn context(&self, operation: Value, read: bool) -> Value {
        if read {
            self.query_v2(query(json!({"kind":"context.read", "query":operation})))
        } else {
            self.command_v2(intent(json!({"kind":"context.apply", "command":operation})))
        }
    }
    fn observe(&self, mut response: Value, read_kind: &str) -> Value {
        let operation_id = response["result"]["operation_id"].clone();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while response["status"] == "ok" && response["result"]["done"] != true {
            assert!(std::time::Instant::now() < deadline, "{response}");
            std::thread::sleep(std::time::Duration::from_millis(2));
            response = self.query_v2(query(
                json!({"kind":read_kind, "operation_id":operation_id, "release":false}),
            ));
        }
        response
    }

    fn query_v2(&self, request: Value) -> Value {
        let request = CString::new(request.to_string()).unwrap();
        take_json(unsafe { floe_core_query_v2(self.0, request.as_ptr()) })
    }

    fn command_v2(&self, request: Value) -> Value {
        let request = CString::new(request.to_string()).unwrap();
        take_json(unsafe { floe_core_command_v2(self.0, request.as_ptr()) })
    }

    fn events_v2(&self, request: Value) -> Value {
        let request = CString::new(request.to_string()).unwrap();
        take_json(unsafe { floe_core_events_v2(self.0, request.as_ptr()) })
    }
}

#[test]
fn product_open_binds_native_identity_before_database_side_effects() {
    let directory = tempfile::tempdir().unwrap();
    let person = Uuid::new_v4();
    let person_directory = directory.path().join("people").join(person.to_string());
    std::fs::create_dir_all(&person_directory).unwrap();
    std::fs::write(directory.path().join("local_device_id"), "local-device-1").unwrap();
    let database = person_directory.join("floe.db");
    let core = Core::open(database.to_str().unwrap());
    assert!(database.exists());
    let request_id = Uuid::new_v4();
    let command_id = Uuid::new_v4();
    let query = json!({
        "schema_version": 2,
        "request_id": request_id,
        "query": {"kind": "conversation.get_command", "command_id": command_id}
    });
    let locked = core.query_v2(query.clone());
    assert_eq!(locked["schema_version"], 2);
    assert_eq!(locked["request_id"], request_id.to_string());
    assert_eq!(locked["error"]["code"], "unavailable");
    let mut wrong_version = query.clone();
    wrong_version["schema_version"] = json!(1);
    let wrong_version = core.query_v2(wrong_version);
    assert_eq!(wrong_version["error"]["code"], "unsupported_version");
    assert_eq!(wrong_version["error"]["field"], "schema_version");
    let mut injected = query;
    injected["query"]["bearer_token"] = json!("must-not-cross-app-wire");
    assert_eq!(core.query_v2(injected)["error"]["code"], "validation");

    let events_request_id = Uuid::new_v4();
    let initial_events = core.events_v2(json!({
        "schema_version": 2,
        "request_id": events_request_id,
        "limit": 16
    }));
    assert_eq!(initial_events["request_id"], events_request_id.to_string());
    assert_eq!(initial_events["result"]["kind"], "resync_required");
    assert_eq!(initial_events["result"]["snapshot_cursor"], 0);
    assert!(initial_events["result"]["runtime_epoch"].as_u64().unwrap() > 0);

    let start_request_id = Uuid::new_v4();
    let start = json!({
        "schema_version": 2,
        "request_id": start_request_id,
        "command_id": Uuid::new_v4(),
        "command": {
            "kind": "conversation.start_turn",
            "session_id": Uuid::new_v4(),
            "expected_revision": 0,
            "text": "Use only the host-selected route",
            "mode": {"kind": "new_turn"}
        }
    });
    let locked = core.command_v2(start.clone());
    assert_eq!(locked["request_id"], start_request_id.to_string());
    assert_eq!(locked["error"]["code"], "unavailable");
    let mut injected = start;
    injected["command"]["remote_route"] = json!({"bearer_token": "must-not-cross-app-wire"});
    assert_eq!(core.command_v2(injected)["error"]["code"], "validation");
    drop(core);

    let missing_directory = tempfile::tempdir().unwrap();
    let missing_person_directory = missing_directory
        .path()
        .join("people")
        .join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&missing_person_directory).unwrap();
    let missing_database = missing_person_directory.join("floe.db");
    let path = CString::new(missing_database.to_str().unwrap()).unwrap();
    let mut error = std::ptr::null_mut();
    let handle = unsafe { floe_core_open(path.as_ptr(), &mut error) };
    assert!(handle.is_null());
    assert!(!error.is_null());
    let error = take_json(error);
    assert_eq!(error["status"], "error");
    assert_eq!(error["error"]["code"], "internal");
    assert!(!missing_database.exists());
}

impl Drop for Core {
    fn drop(&mut self) {
        unsafe { floe_core_free(self.0) };
    }
}

fn take_json(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null());
    let encoded = unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .unwrap()
        .to_owned();
    unsafe { floe_string_free(pointer) };
    serde_json::from_str(&encoded).unwrap()
}

fn day() -> Value {
    json!({
        "date": "2026-09-02",
        "timezone_offset_seconds": 0,
        "now": "2026-09-02T10:30:00Z"
    })
}

fn command(mutation: Value) -> Value {
    intent(json!({"kind":"day.mutate", "day":day(), "mutation":mutation}))
}

fn intent(command: Value) -> Value {
    json!({"schema_version":2, "request_id":Uuid::new_v4(), "command_id":Uuid::new_v4(), "command":command})
}

fn query(query: Value) -> Value {
    json!({"schema_version":2, "request_id":Uuid::new_v4(), "query":query})
}

fn data(response: &Value) -> &Value {
    assert_eq!(response["status"], "ok", "{response}");
    assert_eq!(response["schema_version"], 2);
    let result = &response["result"];
    match result["kind"].as_str() {
        Some("day_mutation") => &result["mutation"],
        Some("day_snapshot") => &result["snapshot"],
        Some("context_applied" | "context_read") => &result["context"],
        _ => result,
    }
}

fn changed(response: &Value) -> (&str, u64) {
    let item = &data(response)["changed_item"];
    (
        item["id"].as_str().unwrap(),
        item["revision"].as_u64().unwrap(),
    )
}

#[test]
fn complete_command_surface_round_trips_and_persists() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("person.db");
    let core = Core::open(path.to_str().unwrap());

    let response = core.execute(command(json!({
        "type": "submit_capture",
        "input": "Captured note",
        "occurred_at": "2026-09-02T09:00:00Z"
    })));
    let capture = &data(&response)["capture"];
    let capture_id = capture["id"].as_str().unwrap();
    let capture_revision = capture["revision"].as_u64().unwrap();

    let response = core.execute(command(json!({
        "type": "classify_capture",
        "capture_id": capture_id,
        "expected_revision": capture_revision,
        "classification": {"kind": "note", "content": "Captured note"},
        "occurred_at": "2026-09-02T09:01:00Z"
    })));
    assert_eq!(data(&response)["changed_item"]["source"]["kind"], "capture");

    let response = core.execute(command(json!({
        "type": "create_event",
        "title": "Review",
        "schedule": {
            "kind": "timed",
            "starts_at": "2026-09-02T10:00:00Z",
            "ends_at": "2026-09-02T11:00:00Z",
            "timezone": "UTC"
        },
        "occurred_at": "2026-09-02T08:00:00Z"
    })));
    let (event_id, event_revision) = changed(&response);
    let event_id = event_id.to_owned();
    let response = core.execute(command(json!({
        "type": "update_event",
        "event_id": event_id,
        "expected_revision": event_revision,
        "title": "Review updated",
        "schedule": {
            "kind": "timed",
            "starts_at": "2026-09-02T10:00:00Z",
            "ends_at": "2026-09-02T11:30:00Z",
            "timezone": "UTC"
        },
        "occurred_at": "2026-09-02T08:10:00Z"
    })));
    assert_eq!(data(&response)["changed_item"]["title"], "Review updated");

    let response = core.execute(command(json!({
        "type": "create_task",
        "title": "Ship",
        "deadline": "2026-09-02T10:00:00Z",
        "priority": "normal",
        "occurred_at": "2026-09-02T08:00:00Z"
    })));
    let (task_id, task_revision) = changed(&response);
    let task_id = task_id.to_owned();
    let response = core.execute(command(json!({
        "type": "set_task_completion",
        "task_id": task_id,
        "expected_revision": task_revision,
        "completed": true,
        "occurred_at": "2026-09-02T10:35:00Z"
    })));
    let completed_revision = data(&response)["changed_item"]["revision"]
        .as_u64()
        .unwrap();
    assert!(!data(&response)["changed_item"]["completed_at"].is_null());
    let response = core.execute(command(json!({
        "type": "set_task_completion",
        "task_id": task_id,
        "expected_revision": completed_revision,
        "completed": false,
        "occurred_at": "2026-09-02T10:36:00Z"
    })));
    let reopened_revision = data(&response)["changed_item"]["revision"]
        .as_u64()
        .unwrap();
    assert!(data(&response)["changed_item"]["completed_at"].is_null());
    let response = core.execute(command(json!({
        "type": "update_task",
        "task_id": task_id,
        "expected_revision": reopened_revision,
        "title": "Ship updated",
        "deadline": null,
        "priority": "high",
        "occurred_at": "2026-09-02T10:37:00Z"
    })));
    let updated_task_revision = data(&response)["changed_item"]["revision"]
        .as_u64()
        .unwrap();
    let response = core.execute(command(json!({
        "type": "delete_item",
        "target": {"kind": "task", "id": task_id},
        "expected_revision": updated_task_revision,
        "occurred_at": "2026-09-02T10:38:00Z"
    })));
    assert!(data(&response)["changed_item"].is_null());

    let response = core.execute(command(json!({
        "type": "create_note",
        "content": "Manual note",
        "occurred_at": "2026-09-02T08:00:00Z"
    })));
    let (note_id, note_revision) = changed(&response);
    let note_id = note_id.to_owned();
    let response = core.execute(command(json!({
        "type": "update_note",
        "note_id": note_id,
        "expected_revision": note_revision,
        "content": "Manual note updated",
        "occurred_at": "2026-09-02T08:05:00Z"
    })));
    assert_eq!(
        data(&response)["changed_item"]["content"],
        "Manual note updated"
    );

    drop(core);
    let core = Core::open(path.to_str().unwrap());
    let response = core.load(day());
    let items = data(&response)["items"].as_array().unwrap();
    assert!(items.iter().any(|item| item["title"] == "Review updated"));
    assert!(items.iter().any(|item| item["content"] == "Captured note"));
    assert!(!items.iter().any(|item| item["id"] == task_id));
}

#[test]
fn remote_owner_abis_validate_envelopes_and_preserve_request_correlation() {
    let directory = tempfile::tempdir().unwrap();
    let person = Uuid::new_v4();
    let person_directory = directory.path().join("people").join(person.to_string());
    std::fs::create_dir_all(&person_directory).unwrap();
    std::fs::write(directory.path().join("local_device_id"), "verified-mac").unwrap();
    let core = Core::open(person_directory.join("floe.db").to_str().unwrap());
    for (endpoint, kind) in [
        (
            floe_core_remote_pairing_v2
                as unsafe extern "C" fn(*mut FloeHandle, *const c_char) -> *mut c_char,
            "read_result",
        ),
        (floe_core_remote_access_v2, "read_result"),
    ] {
        let request_id = Uuid::new_v4();
        let call = |body: Value| {
            let request = CString::new(body.to_string()).unwrap();
            take_json(unsafe { endpoint(core.0, request.as_ptr()) })
        };
        let request = json!({"schema_version": 2, "request_id": request_id, "operation": {"kind": kind, "operation_id": Uuid::new_v4(), "release": false}});
        let accepted = call(request.clone());
        assert_eq!(accepted["request_id"], request_id.to_string());
        assert_eq!(accepted["error"]["code"], "not_found");
        let mut wrong = request.clone();
        wrong["schema_version"] = json!(3);
        assert_eq!(call(wrong)["error"]["code"], "unsupported_version");
        for field in [
            "person_id",
            "device_id",
            "route",
            "base_url",
            "bearer_token",
            "allow_external",
            "calendar_connections",
        ] {
            let mut forged = request.clone();
            forged["operation"][field] = json!("must_not_appear_in_diagnostics");
            let rejected = call(forged);
            assert_eq!(rejected["request_id"], request_id.to_string());
            assert_eq!(rejected["error"]["code"], "validation");
            assert!(
                !rejected
                    .to_string()
                    .contains("must_not_appear_in_diagnostics")
            );
        }
        let malformed = CString::new("{").unwrap();
        assert_eq!(
            take_json(unsafe { endpoint(core.0, malformed.as_ptr()) })["error"]["code"],
            "validation"
        );
    }
}

#[test]
fn context_identity_is_admitted_and_publications_are_ephemeral() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("context.db");
    let core = Core::open(database.to_str().unwrap());
    let now = chrono::Utc::now().timestamp_millis();
    let publication = json!({"kind":"publish", "view":{
        "schema_version":1, "view_id":"attention.coarse", "source_handle":"attention:macos_local",
        "observed_at_unix_ms":now-1, "expires_at_unix_ms":now+60_000, "state":"focused",
        "confidence_millis":800, "evidence_handles":["activity:coarse"]
    }});
    data(&core.context(publication.clone(), false));
    let read = core.context(json!({"kind":"read", "view_id":"attention.coarse"}), true);
    assert_eq!(data(&read)["device_id"], "mac-local");
    assert_eq!(data(&read)["person_id"], PERSON);
    assert_eq!(data(&read)["view"]["state"], "focused");
    for field in ["person_id", "device_id", "bearer", "route"] {
        let mut forged = publication.clone();
        forged[field] = json!("foreign");
        assert_eq!(core.context(forged, false)["error"]["code"], "validation");
    }
    let revoked = core.context(
        json!({"kind":"revoke", "view_id":"attention.coarse"}),
        false,
    );
    assert_eq!(data(&revoked)["removed_count"], 1);
    assert_eq!(
        core.context(json!({"kind":"read", "view_id":"attention.coarse"}), true)["error"]["metadata"]
            ["agent_failure"],
        "capability_unavailable"
    );
    data(&core.context(publication, false));
    drop(core);
    let reopened = Core::open(database.to_str().unwrap());
    assert_eq!(
        reopened.context(json!({"kind":"read", "view_id":"attention.coarse"}), true)["error"]["metadata"]
            ["agent_failure"],
        "capability_unavailable"
    );
}

#[test]
fn native_calendar_publication_requires_the_current_exact_connection() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().join("calendar.db").to_str().unwrap());
    let connection_id = Uuid::new_v4().to_string();
    data(&core.execute(command(json!({"type":"set_calendar_scope", "connection_id":connection_id, "connection_revision":1, "provider":"event_kit", "scope":"selected", "calendars":[{"calendar_id":"home", "calendar_name":"Home"}]}))));
    let now = chrono::Utc::now().timestamp_millis();
    let publication = json!({"kind":"publish_calendar_observation", "connection_id":connection_id, "connection_revision":1, "provider":"event_kit", "calendar_ids":["home"], "observed_at_unix_ms":now-1, "expires_at_unix_ms":now+60_000, "range_start_unix_ms":now-60_000, "range_end_unix_ms":now+60_000, "batches":[{"calendar_id":"home", "records":[], "failure":null}]});
    data(&core.context(publication.clone(), false));
    for (field, value) in [
        ("connection_id", json!(Uuid::new_v4())),
        ("connection_revision", json!(2)),
    ] {
        let mut stale = publication.clone();
        stale[field] = value;
        assert_eq!(
            core.context(stale, false)["error"]["metadata"]["agent_failure"],
            "stale_context"
        );
    }
    let mut foreign = publication;
    foreign["person_id"] = json!(Uuid::new_v4());
    assert_eq!(core.context(foreign, false)["error"]["code"], "validation");
}

#[test]
fn owner_results_are_correlated_and_status_never_provisions_keys() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().join("status.db").to_str().unwrap());
    let response = core.observe(
        core.query_v2(query(json!({"kind":"vault.status"}))),
        "vault.read_result",
    );
    let result = data(&response);
    assert_eq!(result["state"], "missing");
    let operation_id = result["operation_id"].clone();
    let observed = core.query_v2(query(
        json!({"kind":"vault.read_result", "operation_id":operation_id, "release":false}),
    ));
    assert_eq!(data(&observed), result);
    let wrong_owner = core.query_v2(query(
        json!({"kind":"actions.read_result", "operation_id":operation_id, "release":false}),
    ));
    assert_eq!(wrong_owner["error"]["code"], "not_found");
    let released = core.query_v2(query(
        json!({"kind":"vault.read_result", "operation_id":operation_id, "release":true}),
    ));
    assert_eq!(data(&released), result);
    assert!(
        !directory
            .path()
            .join("people")
            .join(PERSON)
            .join("floe.db.agent-vaults")
            .exists()
    );
    for field in [
        "person_id",
        "device_id",
        "bearer_token",
        "endpoint",
        "route",
        "text",
    ] {
        let mut request = intent(json!({"kind":"vault.create"}));
        request["command"][field] = json!("secret-marker-do-not-echo");
        let denied = core.command_v2(request);
        assert_eq!(denied["error"]["code"], "validation");
        assert!(!denied.to_string().contains("secret-marker"));
    }
}

#[test]
fn actions_accept_intent_not_execution_authority() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().join("actions.db").to_str().unwrap());
    for operation in [
        json!({"kind":"decide", "action_id":Uuid::new_v4(), "decision":"maybe"}),
        json!({"kind":"execute", "action_id":"invalid"}),
        json!({"kind":"set_authority", "calendar_create":"allow", "person_id":Uuid::new_v4()}),
        json!({"kind":"direct", "allow_create":true}),
    ] {
        assert_eq!(
            core.command_v2(intent(
                json!({"kind":"actions.calendar", "operation":operation})
            ))["error"]["code"],
            "validation"
        );
    }
    for field in ["now", "allow_create", "device_id", "person_id", "endpoint"] {
        let mut request = query(json!({"kind":"actions.list"}));
        request["query"][field] = json!(true);
        assert_eq!(core.query_v2(request)["error"]["code"], "validation");
    }
    let authority = core.observe(
        core.query_v2(query(json!({"kind":"actions.authority"}))),
        "actions.read_result",
    );
    assert_eq!(data(&authority)["failure"]["kind"], "vault_unavailable");
}

#[test]
fn abi_returns_typed_errors_for_bad_boundary_input_without_writes() {
    assert_eq!(floe_protocol_version(), 1);
    unsafe {
        floe_string_free(std::ptr::null_mut());
        floe_core_free(std::ptr::null_mut());
    }
    let malformed = CString::new("{").unwrap();
    let response =
        take_json(unsafe { floe_core_query_v2(std::ptr::null_mut(), malformed.as_ptr()) });
    assert_eq!(response["error"]["field"], "handle");
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().join("person.db").to_str().unwrap());
    let response = take_json(unsafe { floe_core_command_v2(core.0, malformed.as_ptr()) });
    assert_eq!(response["error"]["field"], "request_json");
    let mut wrong_version = query(json!({"kind":"day.snapshot", "day":day()}));
    wrong_version["schema_version"] = json!(99);
    assert_eq!(
        core.query_v2(wrong_version)["error"]["code"],
        "unsupported_version"
    );
    let mut invalid = command(
        json!({"type":"create_note", "content":"must not persist", "occurred_at":"2026-09-02T10:30:00Z"}),
    );
    invalid["command"]["day"]["date"] = json!("not-a-date");
    assert_eq!(core.execute(invalid)["status"], "error");
    assert!(
        data(&core.load(day()))["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let created = core.execute(command(json!({"type":"create_task", "title":"Revision check", "deadline":null, "priority":"normal", "occurred_at":"2026-09-02T10:30:00Z"})));
    let task_id = data(&created)["changed_item"]["id"].clone();
    let conflict = core.execute(command(json!({"type":"set_task_completion", "task_id":task_id, "expected_revision":99, "completed":true, "occurred_at":"2026-09-02T10:31:00Z"})));
    assert_eq!(conflict["error"]["code"], "conflict");
    assert_eq!(conflict["error"]["metadata"]["expected"], "99");
    let invalid_utf8 = [0xff_u8, 0];
    let mut error = std::ptr::null_mut();
    let handle = unsafe { floe_core_open(invalid_utf8.as_ptr().cast(), &mut error) };
    assert!(handle.is_null());
    assert_eq!(take_json(error)["error"]["field"], "path");
    let unverified = directory.path().join("unverified.db");
    let path = CString::new(unverified.to_str().unwrap()).unwrap();
    let handle = unsafe { floe_core_open(path.as_ptr(), &mut error) };
    assert!(handle.is_null());
    assert_eq!(take_json(error)["error"]["code"], "internal");
    assert!(!unverified.exists());
}
