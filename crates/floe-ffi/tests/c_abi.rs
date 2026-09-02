use std::ffi::{CStr, CString, c_char};

use floe_ffi::*;
use serde_json::{Value, json};
use uuid::Uuid;

struct Core(*mut FloeHandle);

impl Core {
    fn open(path: &str) -> Self {
        let path = CString::new(path).unwrap();
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
        take_json(unsafe { floe_core_execute(self.0, request.as_ptr()) })
    }

    fn load(&self, request: Value) -> Value {
        let request = CString::new(request.to_string()).unwrap();
        take_json(unsafe { floe_core_load_day(self.0, request.as_ptr()) })
    }
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

fn command(person_id: &str, command: Value) -> Value {
    json!({
        "schema_version": 1,
        "person_id": person_id,
        "day": day(),
        "command": command
    })
}

fn data(response: &Value) -> &Value {
    assert_eq!(response["status"], "ok", "{response}");
    &response["data"]
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
    let person_id = Uuid::new_v4().to_string();
    let core = Core::open(path.to_str().unwrap());

    let response = core.execute(command(
        &person_id,
        json!({
            "type": "submit_capture",
            "input": "Captured note",
            "occurred_at": "2026-09-02T09:00:00Z"
        }),
    ));
    let capture = &data(&response)["capture"];
    let capture_id = capture["id"].as_str().unwrap();
    let capture_revision = capture["revision"].as_u64().unwrap();

    let response = core.execute(command(
        &person_id,
        json!({
            "type": "classify_capture",
            "capture_id": capture_id,
            "expected_revision": capture_revision,
            "classification": {"kind": "note", "content": "Captured note"},
            "occurred_at": "2026-09-02T09:01:00Z"
        }),
    ));
    assert_eq!(data(&response)["changed_item"]["source"]["kind"], "capture");

    let response = core.execute(command(
        &person_id,
        json!({
            "type": "create_event",
            "title": "Review",
            "schedule": {
                "kind": "timed",
                "starts_at": "2026-09-02T10:00:00Z",
                "ends_at": "2026-09-02T11:00:00Z",
                "timezone": "UTC"
            },
            "occurred_at": "2026-09-02T08:00:00Z"
        }),
    ));
    let (event_id, event_revision) = changed(&response);
    let event_id = event_id.to_owned();
    let response = core.execute(command(
        &person_id,
        json!({
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
        }),
    ));
    assert_eq!(data(&response)["changed_item"]["title"], "Review updated");

    let response = core.execute(command(
        &person_id,
        json!({
            "type": "create_task",
            "title": "Ship",
            "deadline": "2026-09-02T10:00:00Z",
            "priority": "normal",
            "occurred_at": "2026-09-02T08:00:00Z"
        }),
    ));
    let (task_id, task_revision) = changed(&response);
    let task_id = task_id.to_owned();
    let response = core.execute(command(
        &person_id,
        json!({
            "type": "set_task_completion",
            "task_id": task_id,
            "expected_revision": task_revision,
            "completed": true,
            "occurred_at": "2026-09-02T10:35:00Z"
        }),
    ));
    let completed_revision = data(&response)["changed_item"]["revision"]
        .as_u64()
        .unwrap();
    assert!(!data(&response)["changed_item"]["completed_at"].is_null());
    let response = core.execute(command(
        &person_id,
        json!({
            "type": "set_task_completion",
            "task_id": task_id,
            "expected_revision": completed_revision,
            "completed": false,
            "occurred_at": "2026-09-02T10:36:00Z"
        }),
    ));
    let reopened_revision = data(&response)["changed_item"]["revision"]
        .as_u64()
        .unwrap();
    assert!(data(&response)["changed_item"]["completed_at"].is_null());
    let response = core.execute(command(
        &person_id,
        json!({
            "type": "update_task",
            "task_id": task_id,
            "expected_revision": reopened_revision,
            "title": "Ship updated",
            "deadline": null,
            "priority": "high",
            "occurred_at": "2026-09-02T10:37:00Z"
        }),
    ));
    let updated_task_revision = data(&response)["changed_item"]["revision"]
        .as_u64()
        .unwrap();
    let response = core.execute(command(
        &person_id,
        json!({
            "type": "delete_item",
            "target": {"kind": "task", "id": task_id},
            "expected_revision": updated_task_revision,
            "occurred_at": "2026-09-02T10:38:00Z"
        }),
    ));
    assert!(data(&response)["changed_item"].is_null());

    let response = core.execute(command(
        &person_id,
        json!({
            "type": "create_note",
            "content": "Manual note",
            "occurred_at": "2026-09-02T08:00:00Z"
        }),
    ));
    let (note_id, note_revision) = changed(&response);
    let note_id = note_id.to_owned();
    let response = core.execute(command(
        &person_id,
        json!({
            "type": "update_note",
            "note_id": note_id,
            "expected_revision": note_revision,
            "content": "Manual note updated",
            "occurred_at": "2026-09-02T08:05:00Z"
        }),
    ));
    assert_eq!(
        data(&response)["changed_item"]["content"],
        "Manual note updated"
    );

    drop(core);
    let core = Core::open(path.to_str().unwrap());
    let response = core.load(json!({
        "schema_version": 1,
        "person_id": person_id,
        "day": day()
    }));
    let items = data(&response)["items"].as_array().unwrap();
    assert!(items.iter().any(|item| item["title"] == "Review updated"));
    assert!(items.iter().any(|item| item["content"] == "Captured note"));
    assert!(!items.iter().any(|item| item["id"] == task_id));
}

#[test]
fn abi_returns_typed_errors_for_bad_boundary_input() {
    assert_eq!(floe_protocol_version(), 1);
    unsafe {
        floe_string_free(std::ptr::null_mut());
        floe_core_free(std::ptr::null_mut());
    }

    let malformed = CString::new("{").unwrap();
    let response =
        take_json(unsafe { floe_core_load_day(std::ptr::null_mut(), malformed.as_ptr()) });
    assert_eq!(response["status"], "error");
    assert_eq!(response["error"]["code"], "validation");
    assert_eq!(response["error"]["field"], "handle");

    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().join("person.db").to_str().unwrap());
    let response = take_json(unsafe { floe_core_execute(core.0, malformed.as_ptr()) });
    assert_eq!(response["error"]["field"], "request_json");

    let response = core.load(json!({
        "schema_version": 99,
        "person_id": Uuid::new_v4().to_string(),
        "day": day()
    }));
    assert_eq!(response["error"]["code"], "unsupported_version");

    let person_id = Uuid::new_v4().to_string();
    let response = core.execute(command(
        &person_id,
        json!({
            "type": "create_task",
            "title": "Revision check",
            "deadline": null,
            "priority": "normal",
            "occurred_at": "2026-09-02T10:30:00Z"
        }),
    ));
    let task_id = data(&response)["changed_item"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = core.execute(command(
        &person_id,
        json!({
            "type": "set_task_completion",
            "task_id": task_id,
            "expected_revision": 99,
            "completed": true,
            "occurred_at": "2026-09-02T10:31:00Z"
        }),
    ));
    assert_eq!(response["error"]["code"], "conflict");
    assert_eq!(response["error"]["metadata"]["expected"], "99");

    let person_id = Uuid::new_v4().to_string();
    let response = core.execute(json!({
        "schema_version": 1,
        "person_id": person_id,
        "day": {
            "date": "not-a-date",
            "timezone_offset_seconds": 0,
            "now": "2026-09-02T10:30:00Z"
        },
        "command": {
            "type": "create_note",
            "content": "must not persist",
            "occurred_at": "2026-09-02T10:30:00Z"
        }
    }));
    assert_eq!(response["status"], "error");
    let response = core.load(json!({
        "schema_version": 1,
        "person_id": person_id,
        "day": day()
    }));
    assert!(data(&response)["items"].as_array().unwrap().is_empty());

    let invalid_utf8 = [0xff_u8, 0];
    let mut error = std::ptr::null_mut();
    let handle = unsafe { floe_core_open(invalid_utf8.as_ptr().cast(), &mut error) };
    assert!(handle.is_null());
    assert_eq!(take_json(error)["error"]["field"], "path");
}
