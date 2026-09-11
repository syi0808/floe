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

    fn actions(&self, person_id: &str, operation: Value) -> Value {
        let request = CString::new(
            json!({
                "schema_version": 1, "person_id": person_id, "operation": operation
            })
            .to_string(),
        )
        .unwrap();
        take_json(unsafe { floe_core_calendar_actions(self.0, request.as_ptr()) })
    }

    fn agent(&self, person_id: &str, operation: Value) -> Value {
        let request = CString::new(
            json!({
                "schema_version": 1, "person_id": person_id, "operation": operation
            })
            .to_string(),
        )
        .unwrap();
        take_json(unsafe { floe_core_agent_fixture(self.0, request.as_ptr()) })
    }

    fn agent_run(&self, person_id: &str, session: &Value, operation: Value) -> Value {
        let request = CString::new(json!({
            "schema_version": 1, "person_id": person_id,
            "session_id": session["id"], "expected_revision": session["revision"], "operation": operation,
        }).to_string()).unwrap();
        take_json(unsafe { floe_core_agent_fixture_run(self.0, request.as_ptr()) })
    }

    fn local_context(&self, person_id: &str, operation: Value) -> Value {
        let request = CString::new(
            json!({
                "schema_version": 1, "person_id": person_id, "operation": operation
            })
            .to_string(),
        )
        .unwrap();
        take_json(unsafe { floe_core_local_context(self.0, request.as_ptr()) })
    }
}

#[test]
fn local_context_abi_is_ephemeral_person_and_device_bound() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().join("floe.db").to_str().unwrap());
    let person_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let published = core.local_context(
        &person_id,
        json!({
            "kind": "publish",
            "device_id": "mac-local",
            "view": {
                "schema_version": 1,
                "view_id": "attention.coarse",
                "source_handle": "attention:macos_local",
                "observed_at_unix_ms": now - 1,
                "expires_at_unix_ms": now + 60_000,
                "state": "focused",
                "confidence_millis": 800,
                "evidence_handles": ["activity:coarse"]
            }
        }),
    );
    assert_eq!(published["status"], "ok");
    let read = core.local_context(
        &person_id,
        json!({"kind": "read", "view_id": "attention.coarse"}),
    );
    assert_eq!(read["data"]["device_id"], "mac-local");
    assert_eq!(read["data"]["view"]["state"], "focused");
    let revoked = core.local_context(
        &person_id,
        json!({
            "kind": "revoke",
            "device_id": "mac-local",
            "view_id": "attention.coarse"
        }),
    );
    assert_eq!(revoked["data"]["removed_count"], 1);
    let missing = core.local_context(
        &person_id,
        json!({"kind": "read", "view_id": "attention.coarse"}),
    );
    assert_eq!(
        missing["error"]["metadata"]["agent_failure"],
        "capability_unavailable"
    );
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

#[cfg(unix)]
#[test]
fn vault_bridge_reports_status_without_keys_and_rejects_untrusted_commands() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault-boundary.db");
    let core = Core::open(path.to_str().unwrap());
    let person = Uuid::new_v4().to_string();
    let id = Uuid::new_v4().to_string();
    let request = |operation: Value| {
        let request = CString::new(
            json!({"schema_version":1,"person_id":person,"request_id":id,"operation":operation})
                .to_string(),
        )
        .unwrap();
        take_json(unsafe { floe_core_agent_vault(core.0, request.as_ptr()) })
    };
    let mut response = request(json!({"kind":"submit","action":{"kind":"status"}}));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while data(&response)["done"] != true {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
        response = request(json!({"kind":"poll","after_sequence":0}));
    }
    assert_eq!(data(&response)["state"], "missing");
    assert!(
        !directory
            .path()
            .join("vault-boundary.db.agent-vaults")
            .exists()
    );
    assert_eq!(request(json!({"kind":"poll","after_sequence":0})), response);
    assert_eq!(
        request(json!({"kind":"poll","after_sequence":1}))["status"],
        "error"
    );
    request(json!({"kind":"release"}));
    let malformed = request(
        json!({"kind":"submit","action":{"kind":"session","operation":{"kind":"turn","session_id":Uuid::new_v4().to_string(),"expected_revision":0,"prompt":"today","text":"secret-marker-do-not-echo"}}}),
    );
    assert_eq!(malformed["status"], "error");
    assert!(!malformed.to_string().contains("secret-marker"));
    let wrong_version = CString::new(json!({"schema_version":99,"person_id":person,"request_id":id,"operation":{"kind":"submit","action":{"kind":"status"}}}).to_string()).unwrap();
    assert_eq!(
        take_json(unsafe { floe_core_agent_vault(core.0, wrong_version.as_ptr()) })["error"]["code"],
        "unsupported_version"
    );
}

#[test]
fn async_agent_progress_is_replayable_bounded_and_cancellable_without_blocking_calendar() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().join("stream.db").to_str().unwrap());
    let person = Uuid::new_v4().to_string();
    let started = core.agent(&person, json!({"kind":"start"}));
    let session = &data(&started)["session"];
    let begin = core.agent_run(&person, session, json!({"kind":"begin","prompt":"today"}));
    assert_eq!(data(&begin)["done"], false);
    let first = poll_until(&core, &person, session, |update| {
        update["next_sequence"].as_u64().unwrap() >= 3
    });
    assert_eq!(first["done"], false);
    assert!(
        first["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["event"]["kind"] == "model_started")
    );
    let replay = core.agent_run(&person, session, json!({"kind":"poll","after_sequence":0}));
    let length = first["events"].as_array().unwrap().len();
    assert_eq!(
        &data(&replay)["events"].as_array().unwrap()[..length],
        first["events"].as_array().unwrap()
    );
    assert_eq!(
        core.agent_run(
            &person,
            session,
            json!({"kind":"poll","after_sequence":9999})
        )["error"]["code"],
        "validation"
    );
    assert_eq!(
        core.agent_run(&Uuid::new_v4().to_string(), session, json!({"kind":"stop"}))["error"]["code"],
        "not_found"
    );
    assert_eq!(
        core.agent_run(&person, session, json!({"kind":"release"}))["error"]["code"],
        "conflict"
    );
    let current = core.agent(&person, json!({"kind":"get","session_id":session["id"]}));
    assert_eq!(
        core.agent(
            &person,
            json!({"kind":"recover","session_id":session["id"],
        "expected_revision":data(&current)["session"]["revision"]})
        )["error"]["code"],
        "conflict"
    );
    let before = std::time::Instant::now();
    assert_eq!(
        data(&core.load(json!({"schema_version":1,"person_id":person,"day":day()})))["items"],
        json!([])
    );
    assert!(before.elapsed() < std::time::Duration::from_millis(400));
    data(&core.agent_run(&person, session, json!({"kind":"stop"})));
    let stopped = poll_until(&core, &person, session, |update| update["done"] == true);
    assert_eq!(stopped["session"]["last_outcome"]["reason"], "cancelled");
    assert!(
        !stopped["session"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["kind"] == "assistant")
    );
    data(&core.agent_run(&person, session, json!({"kind":"release"})));
    assert_eq!(
        core.agent_run(&person, session, json!({"kind":"begin","prompt":"today"}))["error"]["code"],
        "conflict"
    );
}

#[test]
fn async_agent_completion_and_typed_failure_resume_without_duplicate_dispatch() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("complete.db");
    let person = Uuid::new_v4().to_string();
    let core = Core::open(path.to_str().unwrap());
    let initial = core.agent(&person, json!({"kind":"resume"}));
    let session = &data(&initial)["session"];
    let begin = json!({"kind":"begin","prompt":"today"});
    data(&core.agent_run(&person, session, begin.clone()));
    data(&core.agent_run(&person, session, begin));
    let completed = poll_until(&core, &person, session, |update| update["done"] == true);
    assert_eq!(
        completed["session"]["messages"].as_array().unwrap().len(),
        3
    );
    assert_eq!(completed["session"]["last_outcome"]["status"], "completed");
    let replay = core.agent_run(&person, session, json!({"kind":"poll","after_sequence":0}));
    assert_eq!(data(&replay), &completed);
    data(&core.agent_run(&person, session, json!({"kind":"release"})));
    drop(core);
    let core = Core::open(path.to_str().unwrap());
    let restored = core.agent(&person, json!({"kind":"resume"}));
    assert_eq!(data(&restored)["session"], completed["session"]);
    let session = &data(&restored)["session"];
    data(&core.agent_run(
        &person,
        session,
        json!({"kind":"begin","prompt":"unavailable"}),
    ));
    let failed = poll_until(&core, &person, session, |update| update["done"] == true);
    assert_eq!(
        failed["session"]["last_outcome"]["reason"],
        "model_unavailable"
    );
    assert_eq!(failed["session"]["messages"].as_array().unwrap().len(), 4);
    data(&core.agent_run(&person, session, json!({"kind":"release"})));
    let newer = core.agent(&person, json!({"kind":"start"}));
    assert_eq!(
        data(&core.agent(&person, json!({"kind":"resume"})))["session"],
        data(&newer)["session"]
    );
}

#[test]
fn closing_native_handle_stops_active_fixture_before_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("close.db");
    let person = Uuid::new_v4().to_string();
    let core = Core::open(path.to_str().unwrap());
    let initial = core.agent(&person, json!({"kind":"start"}));
    let session = &data(&initial)["session"];
    data(&core.agent_run(&person, session, json!({"kind":"begin","prompt":"today"})));
    poll_until(&core, &person, session, |update| {
        update["next_sequence"].as_u64().unwrap() >= 3
    });
    drop(core);
    let core = Core::open(path.to_str().unwrap());
    let restored = core.agent(&person, json!({"kind":"resume"}));
    assert_eq!(data(&restored)["session"]["id"], session["id"]);
    assert_eq!(data(&restored)["session"]["active_turn"], Value::Null);
    assert_eq!(
        data(&restored)["session"]["last_outcome"]["reason"],
        "cancelled"
    );
}

fn poll_until(core: &Core, person: &str, session: &Value, ready: impl Fn(&Value) -> bool) -> Value {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let response = core.agent_run(person, session, json!({"kind":"poll","after_sequence":0}));
        let update = data(&response);
        if ready(update) {
            return update.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "agent poll timed out: {update}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn agent_fixture_bridge_resumes_typed_events_but_does_not_accept_personal_text_or_policy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agent.db");
    let person = Uuid::new_v4().to_string();
    let core = Core::open(path.to_str().unwrap());
    let initial = core.agent(&person, json!({"kind": "start"}));
    let session_id = data(&initial)["session"]["id"].clone();
    let operation = json!({"kind": "turn", "session_id": session_id, "expected_revision": 0, "prompt": "today"});
    for field in ["text", "policy", "context", "model", "credentials"] {
        let mut injected = operation.clone();
        injected[field] = json!("PRIVATE_SENTINEL");
        assert_eq!(core.agent(&person, injected)["error"]["code"], "validation");
    }
    let mut bad_prompt = operation.clone();
    bad_prompt["prompt"] = json!("PRIVATE_SENTINEL");
    assert_eq!(
        core.agent(&person, bad_prompt)["error"]["code"],
        "validation"
    );
    let response = core.agent(&person, operation.clone());
    assert_eq!(
        data(&response)["session"]["last_outcome"]["status"],
        "completed"
    );
    assert_eq!(data(&response)["events"][0]["schema_version"], 1);
    assert_eq!(core.agent(&person, operation)["error"]["code"], "conflict");
    assert_eq!(
        core.agent(
            &Uuid::new_v4().to_string(),
            json!({"kind": "get", "session_id": session_id})
        )["error"]["code"],
        "not_found"
    );
    drop(core);
    let core = Core::open(path.to_str().unwrap());
    let restored = core.agent(&person, json!({"kind": "get", "session_id": session_id}));
    assert_eq!(data(&restored)["session"], data(&response)["session"]);
    let next = core.agent(
        &person,
        json!({"kind": "turn", "session_id": session_id,
        "expected_revision": data(&restored)["session"]["revision"], "prompt": "follow_up"}),
    );
    assert_eq!(
        data(&next)["session"]["messages"].as_array().unwrap().len(),
        5
    );
    assert_eq!(
        data(&core.load(json!({"schema_version": 1, "person_id": person, "day": day()})))["items"],
        json!([])
    );
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
fn calendar_decisions_are_person_scoped_durable_and_never_create() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("actions.db");
    let person = Uuid::new_v4().to_string();
    let other = Uuid::new_v4().to_string();
    let core = Core::open(path.to_str().unwrap());
    data(&core.execute(command(
        &person,
        json!({
        "type": "set_calendar_scope",
            "connection_id": "00000000-0000-4000-8000-000000000010",
            "connection_revision": 1,
            "device_id": "fixture-device",
            "provider": "fixture", "scope": "selected",
            "calendars": [{"calendar_id": "target", "calendar_name": "Target"}]
        }),
    )));
    let now = chrono::Utc::now();
    let proposal = json!({
        "kind": "propose", "calendar_id": "target", "title": " Focus ",
        "starts_at": (now + chrono::Duration::hours(1)).to_rfc3339(),
        "ends_at": (now + chrono::Duration::hours(2)).to_rfc3339(),
        "timezone": "Asia/Seoul"
    });
    let response = core.actions(&person, proposal.clone());
    let action = &data(&response)["actions"][0];
    assert_eq!(action["state"]["status"], "pending");
    assert_eq!(action["title"], "Focus");
    assert_eq!(action["calendar_name"], "Target");
    let action_id = action["id"].clone();
    let decision = json!({"kind": "decide", "action_id": action_id, "decision": "approve"});
    assert_eq!(
        core.actions(&other, decision.clone())["error"]["code"],
        "not_found"
    );
    assert_eq!(
        data(&core.actions(&other, json!({"kind": "list"})))["actions"],
        json!([])
    );
    let approved = core.actions(&person, decision.clone());
    assert_eq!(data(&approved)["actions"][0]["state"]["status"], "approved");
    assert!(!data(&approved)["actions"][0]["approved_at"].is_null());
    assert_eq!(core.actions(&person, decision)["error"]["code"], "conflict");
    let rejected = core.actions(&person, proposal);
    let rejected_id = data(&rejected)["actions"][0]["id"].clone();
    let rejected = core.actions(
        &person,
        json!({"kind": "decide", "action_id": rejected_id, "decision": "reject"}),
    );
    assert_eq!(data(&rejected)["actions"][0]["state"]["status"], "rejected");
    drop(core);
    let core = Core::open(path.to_str().unwrap());
    let restored = core.actions(&person, json!({"kind": "get", "action_id": action_id}));
    assert_eq!(data(&restored), data(&approved));
    assert_eq!(
        data(&core.actions(&person, json!({"kind": "list"})))["actions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        data(&core.load(json!({"schema_version": 1, "person_id": person, "day": day()})))["items"],
        json!([])
    );
}

#[test]
fn action_authority_defaults_to_ask_and_persists() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authority.db");
    let person = Uuid::new_v4().to_string();
    let core = Core::open(path.to_str().unwrap());

    assert_eq!(
        data(&core.actions(&person, json!({"kind": "get_authority"})))["authority"]["calendar_create"],
        "ask"
    );
    assert_eq!(
        data(&core.actions(
            &person,
            json!({"kind": "set_authority", "calendar_create": "allow"}),
        ))["authority"]["calendar_create"],
        "allow"
    );
    drop(core);

    let reopened = Core::open(path.to_str().unwrap());
    assert_eq!(
        data(&reopened.actions(&person, json!({"kind": "get_authority"})))["authority"]["calendar_create"],
        "allow"
    );
}

#[test]
fn calendar_action_boundary_rejects_execution_and_caller_authority() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().join("invalid.db").to_str().unwrap());
    let person = Uuid::new_v4().to_string();
    for operation in [
        json!({"kind": "execute", "action_id": Uuid::new_v4()}),
        json!({"kind": "list", "now": "2020-01-01T00:00:00Z"}),
        json!({"kind": "list", "allow_create": true}),
        json!({"kind": "decide", "action_id": Uuid::new_v4(), "decision": "maybe"}),
        json!({"kind": "get", "action_id": "invalid"}),
    ] {
        assert_eq!(
            core.actions(&person, operation)["error"]["code"],
            "validation"
        );
    }
    assert_eq!(
        core.actions("invalid", json!({"kind": "list"}))["error"]["code"],
        "validation"
    );
    let request = CString::new(
        json!({"schema_version": 999, "person_id": person, "operation": {"kind": "list"}})
            .to_string(),
    )
    .unwrap();
    let response = take_json(unsafe { floe_core_calendar_actions(core.0, request.as_ptr()) });
    assert_eq!(response["error"]["code"], "unsupported_version");
    let response =
        take_json(unsafe { floe_core_calendar_actions(std::ptr::null_mut(), request.as_ptr()) });
    assert_eq!(response["error"]["code"], "validation");
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
