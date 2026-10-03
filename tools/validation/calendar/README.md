# Calendar diagnostics and exact recovery

The legacy deterministic native suites and response-loss test shim were removed
in T0 after behavior extraction. Their assertions and external-effect limits are
recorded in [the root tooling ledger](../../../docs/plans/t0-root-tools-behavior-ledger.md).
New tests are reconstructed only after full architecture and build closure.
No current response-loss injection procedure is provided by this directory.

`core-check.c` remains an opt-in C ABI diagnostic host. Compile it against the
current `crates/bindings/ffi/include/floe_ffi.h` and the same-source dylib at the
authorized build gate. Run `core-check <private-product-profile>/people/<person>/floe.db`;
each input is two newline-terminated lines: `command_v2`, `query_v2`, or `events_v2`,
then its complete AppWire JSON envelope. Each response is one JSON line. Keep
stdin open while submitting and observing owner jobs; EOF closes the host.
Profile identity must already be configured in
`<private-product-profile>/local_device_id`.

`inspect.swift` remains the exact external-effect recovery tool. It verifies or
cleans up only one full marker/payload match in the dedicated calendar;
`--verify-absent` checks cleanup without writing. Historical markers
`Floe S3 — disposable` and `iCloud · Floe Validation` remain exact operation
identity for recovering previously authorized effects. Never infer permission
to create, delete or retry an event from those strings.

Use whole-second proposal start/end times. EventKit discards subsecond event
times; the production adapter rejects nonrepresentable write precision before
permission/preflight/create. Lookup requires the exact marker and payload;
never widen matching tolerance or blindly retry an uncertain write.

Any real Calendar operation and cleanup requires explicit authorization for
its exact target. Preserve private evidence and uncertain-operation records.
Permission reauthorization is a separate user-approved operation. Never change
TCC grants, edit the TCC database, reset credentials or alter unrelated events
as a validation shortcut.
