# Agent debugging

Agent failures are correlated across Flutter, the JSON/C ABI, the vault worker,
Experts, and model attempts with the vault `request_id`. Diagnostic output must not
contain prompt text, model responses, calendar content, credentials, or Person IDs.

## Capture a failure

From the repository root, run:

```sh
./scripts/run-agent-debug.sh
```

The command builds `floe-ffi`, starts the macOS Flutter client, and writes combined
Flutter and Rust output to `.floe-debug/agent-<timestamp>.log`. Set `FLOE_LOG` to
change the Rust filter:

```sh
FLOE_LOG=debug ./scripts/run-agent-debug.sh
```

In a debug build, press `Command-Shift-D` to export the in-memory Flutter diagnostic
ring buffer. The resulting JSON path is copied to the clipboard. The same action is
available from the bug icon in the `Command-Shift-F` review overlay.

## Follow one request

Find the error's `request_id` in the exported bundle, then filter the combined log:

```sh
rg 'REQUEST_ID' .floe-debug/agent-*.log
```

Expected Rust stages are `agent_job_started`, optional `model_attempt_*` and
`expert_invocation_*` events, followed by `agent_job_completed` or
`agent_job_failed`. Flutter records include `component`, `operation`, `failure`,
`request_id`, `session_id`, `elapsed_ms`, and a locally generated `error_id`.

## Failure boundaries

- Envelope errors preserve Rust `code`, `field`, and `metadata` in Dart.
- Agent result failures preserve their exact `AgentFailure`, request ID, and action stage.
- Rust panics are caught at the FFI boundary and receive an `error_id`; panic payloads
  are not returned or logged.
- Uncaught Flutter framework, platform, and asynchronous errors enter the same local
  diagnostic buffer.

## Regression checks

```sh
cargo test -p floe-ffi diagnostics
cd apps/client
flutter test test/infrastructure/diagnostics test/infrastructure/native/native_transport_error_test.dart
flutter test test/features/agent/agent_vault_gateway_test.dart
```
