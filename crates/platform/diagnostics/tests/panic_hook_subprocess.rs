use std::{
    env,
    panic::{self, AssertUnwindSafe},
    process::Command,
};

use floe_diagnostics::{TraceContext, install_panic_hook, panic_record, with_context};

#[test]
fn panic_hook_subprocess_does_not_emit_payload() {
    if env::var_os("FLOE_DIAGNOSTICS_PANIC_SMOKE").is_some() {
        let _guard = install_panic_hook().expect("hook is available");
        let context = TraceContext::new(uuid::Uuid::new_v4());
        let payload = panic::catch_unwind(AssertUnwindSafe(|| {
            with_context(context, || panic!("private prompt payload"));
        }))
        .expect_err("panic must be caught");
        let _ = panic_record(payload);
        return;
    }

    let output = Command::new(env::current_exe().expect("test executable path"))
        .env("FLOE_DIAGNOSTICS_PANIC_SMOKE", "1")
        .arg("panic_hook_subprocess_does_not_emit_payload")
        .arg("--exact")
        .arg("--nocapture")
        .output()
        .expect("spawn panic smoke test");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stdout.contains("private prompt payload"));
    assert!(!stderr.contains("private prompt payload"));
}
