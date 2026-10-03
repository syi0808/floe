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

The previous diagnostics and transport suites were removed in T0 after their
assertions were recorded in the [bindings](../plans/t0-bindings-behavior-ledger.md)
and [client runtime](../plans/t0-client-behavior-ledger-runtime.md) behavior ledgers.
Reconstruct regression coverage against the final owner and transport contracts
in S3. The [active execution plan](../plans/2026-10-02-architecture-refactor.md)
controls compilation, build and test gates; no removed test command is a current
verification result.
