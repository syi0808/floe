#![cfg(target_os = "macos")]

use floe_ffi::*;
use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString},
    process::Command,
};

const PERSON: &str = "00000000-0000-4000-8000-000000000001";

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
    data(core.call(json!({"schema_version": 1, "person_id": PERSON, "day": day,
        "command": {"type": "select_calendar", "provider": "event_kit", "calendar_id": "target", "calendar_name": "Fixture · Target"}}), false));
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
