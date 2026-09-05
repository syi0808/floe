# S3 native executor and collection checkpoint

Date: 2026-09-06. macOS, EventKit, Flutter, JSON/C ABI, Rust/Turso.

## Delivered implementation

- Today can prepare an immutable proposal from an explicit connected calendar,
  title, future UTC interval (maximum 24 hours) and timezone. Preparation performs
  no external write. The next screen reviews the full payload and decision.
- The action API adds `capabilities`, `execute` and `recover`. Requests contain an
  action ID, not a caller-selected provider implementation, policy, receipt or
  execution timestamp. Production execution is bound to the local device Person.
- Rust derives the allowed scope from its durable connection, claims execution
  using CAS, and retains all existing policy/revision/expiry/local-event checks.
  Disconnected and failed-target connections are explicitly blocked.
- Rust loads the bundled signed `libfloe_eventkit.dylib`; no model tool, method
  channel for arbitrary saves, environment override for library selection, or
  remote executor is introduced. This remains a trusted in-process boundary, not
  an OS isolation boundary against arbitrary native code loaded into the app.
- EventKit freshly resolves the exact target and connected scope, checks Full
  Access, writability, supported timezone, and current external/local overlaps.
  Create repeats the checks immediately before save. It creates a timed,
  non-recurring event with no attendees/alarms and a Person/execution URL marker.
- External all-day and recurring conflicts use fresh EventKit instances. Cached
  floating all-day conflicts are conservative across the full UTC−14…UTC+14 range;
  this can block a marginally free interval rather than miss an overlap.
- Native calls have a 12-second pre-save deadline and a 15-second Rust response
  timeout. A native call already inside EventKit cannot be forcibly cancelled;
  the ledger remains ambiguous and cannot dispatch again. A process-wide gate
  prevents a timed-out call from accumulating concurrent native saves.
- Lookup requires exactly one full-payload/marker match in a bounded ±1-day window.
  Edited, duplicated, moved-out-of-range or missing markers remain unresolved.
  Lookup remains available in write-disabled builds and never creates anything.
- Explicit new approval can execute immediately in a write-enabled build.
  Restored approvals require an explicit execution click and current checks;
  loading/relaunch never resumes creation. Unknown/executing states offer lookup
  only, and suppress replacement proposals until the uncertainty is resolved.
- Successful execution invokes the existing Calendar read/import path for the
  event's local civil day(s), matching calendar and external IDs. Collection
  failure keeps `Succeeded`, offers read-only retry, and cannot repeat creation.
  Collection feedback is in memory; after restart the durable receipt remains
  successful and a safe read retry remains available.

## Rollout gate

Normal builds compile native writing **off**. A validation build requires
`FLOE_ENABLE_CALENDAR_WRITES=1 flutter build macos --debug`; Dart cannot override
this native flag. Read/approval UI and OS permission disclosures distinguish the
two modes. Keep ordinary rollout gated until S1 verification and the live matrix
are complete. Full Access alone is never authorization to create an event.

## Automated evidence

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
sh tools/s3-validation/check-native.sh
cd apps/client
flutter analyze
flutter test
flutter build macos --debug
```

46 Rust tests, 70 Flutter tests and eight native assertions pass. Clippy and Flutter
analysis are clean. macOS write-enabled debug and default release builds pass
signature checks. The release library's actual capability response was checked:
`writes_enabled=false`, without reading Calendar or requesting permission.
The new Rust integration test runs a copied host with an isolated native fixture:
real ABI, dynamic loading, execution, lost response, reopen, retry denial, lookup
and conflict blocking, with exactly one native create call and no OS access.
Flutter covers proposal validation/review, no pre-approval write, enabled/disabled
build behavior, restoration, collection failure/read retry and lookup-only recovery.

## Real EventKit evidence

User authorized a disposable event in the existing iCloud `Floe Validation`
calendar, followed by verification and cleanup. At approximately 01:23 KST:

1. A private validation DB selected only that calendar. No existing app selection,
   task, note or external event was changed by this helper.
2. The real C ABI prepared and approved `Floe S3 — disposable`, Sep 6 10:00–10:15
   Asia/Seoul. A test-only shim forwarded to the actual native adapter, then
   replaced its successful create response with `timeout`.
3. Rust persisted `Unknown(timeout)`. A fresh process's execute request returned
   `Conflict`, without dispatching another create.
4. Another process recovered exactly one full marker/payload match and persisted
   `Succeeded`. An independent fresh EventKit store matched the same external ID.
5. The checked real event was passed through the existing Rust Calendar import
   operation and appeared once in the Day Canvas snapshot with matching provenance,
   timezone and external ID. This step is **not** a Flutter MethodChannel read.
6. Cleanup removed only that exact event. Fresh EventKit lookup returned zero
   matches; an empty-range import removed the mirror item without changing the
   successful execution ledger.

Private evidence is under `target/s3-validation/private/` (not committed).
The lookup marker was `84423660-ca9f-4d54-8b23-3a3b0776598d`; the disposable
external item was `0E1F9E17-4CB5-49F0-9637-0F020C2E11CC|`.
This is local EventKit evidence, not an iCloud cross-device durability guarantee.

## Integration issues found

The pre-existing app DB lacked the subsequently added `connection.scope` JSON
field. Re-serializing the decoded previous value caused every mirror CAS to fail.
Commit `8c068da` fixes this by comparing decoded state and then CASing the actual
stored bytes. A regression test proves both legacy refresh and stale-writer denial.

The signed app also encountered a TCC code-requirement mismatch after ad-hoc rebuild.
The user explicitly authorized Floe-only Calendar permission reauthorization.
System Settings off/on did not replace the old code requirement; the supported
`tccutil reset Calendar app.floe.floeClient` reset only that app/service, after
which Floe requested access again. No direct TCC DB modification or permission
change for another app was performed. The protected OS prompt requires user input.
The debug validation app remains at that prompt; it has not created another event.
The separate Release app is write-disabled. Do not replace/re-sign the running
debug app while completing its grant; rebuild it without the opt-in flag after
the remaining live validation, and never ship the response-loss app copy.
Use a stable trusted development/release signing identity for durable app grants;
do not weaken the designated requirement to avoid permission checks.

S3 is not marked Accepted solely from the helper/fixture evidence. The final app
permission grant, live UI create/collection, and remaining S1/provider failure
matrix must be recorded before changing the acceptance count.
