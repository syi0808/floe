# S3 controlled validation

`ResponseLoss.swift` is a **test-only** native response-loss shim. It forwards to
the actual built EventKit adapter, then replaces a successful create response with
`timeout`. Creates are restricted to `Floe S3 — disposable` in
`iCloud · Floe Validation`. It does not approve anything or retry a create.
Rust and Flutter must recover the original execution marker by lookup.

Do not ship this shim or enable writes as part of a normal build. Obtain operator
authorization for the dedicated calendar, disposable event and cleanup first.
Never change TCC grants, existing calendars, or unrelated events to make a test pass.

Permission reauthorization is a separate user-approved operation, not part of this
helper. If an ad-hoc build no longer matches its old grant, stop and obtain explicit
authorization before resetting Floe's Calendar grant. Never edit the TCC database.

Build the app explicitly with `FLOE_ENABLE_CALENDAR_WRITES=1 flutter build macos --debug`.
In a disposable copy of that app, rename the signed native library to
`libfloe_eventkit_real.dylib`, compile this shim as `libfloe_eventkit.dylib`, and sign
both libraries and the copied app. Launch only the copied app for fault injection.
The original build must be restored to write-disabled after validation. Persist
the exact proposal/execution/time/external IDs in a private evidence record and
remove only the exact disposable event after collection is verified.

Fixture end-to-end coverage (no OS access) is `cargo test -p floe-ffi --test native_action`.

`check-native.sh` runs pure native payload/conflict/default-gate assertions without
reading Calendar. `core-check.c` hosts the real Rust C ABI in a private validation
bundle/DB. `inspect.swift` verifies or cleans up only one full marker/payload match
in the exact dedicated calendar; `--verify-absent` checks cleanup without writing.
