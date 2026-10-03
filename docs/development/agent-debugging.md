# Agent debugging

AppWire calls carry `request_id` across Flutter and the JSON/C ABI. Retain the
returned owner command, Run, Task or operation identity when following durable work
across later observations. The Vault queue handles lifecycle operations; domain
owners drive their own work. Diagnostic output must not
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

Flutter records include `component`, `operation`, optional `failure`,
`request_id`, `session_id`, `invocation_id`, `elapsed_ms`, and an `error_id`.
Owner failures also carry domain/category, reason code, incident identity and safe
actions when supplied by the owner. Rust FFI panic diagnostics emit
`rust_core_panicked` at `ffi_boundary` with an `error_id`. The removed vault-worker
`agent_job_*` stage sequence is not emitted by the current owner path. Use the
owner's command lookup, Run receipt and event cursor to establish durable status;
a missing log event does not prove that work was never admitted or completed.

## Failure boundaries

- Envelope errors preserve Rust `code`, `field`, and `metadata` in Dart.
- Owner failures preserve their typed failure projection and correlation identity.
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
