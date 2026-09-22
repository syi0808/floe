# S3 controlled validation

`ResponseLoss.swift` is a **test-only** native response-loss shim. It forwards to
the actual built EventKit adapter, then replaces a successful create response with
`timeout`. Creates are restricted to `Floe S3 — disposable` in
`iCloud · Floe Validation`. It does not approve anything or retry a create.
Rust and Flutter must recover the original execution marker by lookup.

Do not ship this shim. Normal builds include the guarded Calendar executor; obtain operator
authorization for the dedicated calendar, disposable event and cleanup first.
Never change TCC grants, existing calendars, or unrelated events to make a test pass.

Permission reauthorization is a separate user-approved operation, not part of this
helper. If an ad-hoc build no longer matches its old grant, stop and obtain explicit
authorization before resetting Floe's Calendar grant. Never edit the TCC database.

Build the app with `flutter build macos --debug`. In a disposable copy of that app,
rename the signed native library to
`libfloe_eventkit_real.dylib`, compile this shim as `libfloe_eventkit.dylib`, and sign
both libraries and the copied app. Launch only the copied app for fault injection.
Do not modify the original build. Persist the exact proposal/execution/time/external IDs in a private evidence record and
remove only the exact disposable event after collection is verified.

Canonical automated uncertainty coverage (Tier B, no real Calendar access) is
`cargo test -p floe-app native_executor_uses_rust_ledger_and_lookup_only_after_response_loss`.
The test uses the admitted Actions owner and a test-only native adapter; it does
not replace the separately authorized real EventKit procedure above.

`check-native.sh` runs pure native payload/conflict/default-gate assertions without
reading Calendar. `core-check.c` hosts the real Rust C ABI in a private validation
bundle/DB through the final `command_v2/query_v2/events_v2` entry points. Compile it
against the current `crates/bindings/ffi/include/floe_ffi.h` and same-source dylib.
Run `core-check <private-product-profile>/people/<person>/floe.db`; each input is
two newline-terminated lines: `command_v2`, `query_v2`, or `events_v2`, then its
complete AppWire JSON envelope. Each response is one JSON line. Keep stdin open
while submitting and observing owner jobs; EOF closes the host. Profile identity
must already be configured in `<private-product-profile>/local_device_id`.

`inspect.swift` verifies or cleans up only one full marker/payload match
in the exact dedicated calendar; `--verify-absent` checks cleanup without writing.
