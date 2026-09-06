# S3 native executor and collection checkpoint

Date: 2026-09-06. macOS, EventKit, Flutter, JSON/C ABI, Rust/Turso.

## Delivered implementation

- Today can prepare an immutable proposal from an explicit connected calendar,
  title and future device-local interval (maximum 24 hours). The FFI serializes its
  instants as UTC and derives scheduling metadata internally. Preparation performs
  no external write. The next screen reviews the decision-relevant payload.
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
  Access, writability, valid internal time metadata, and current external/local overlaps.
  Create repeats the checks immediately before save. It creates a timed,
  non-recurring event with no attendees/alarms and a Person/execution URL marker.
- External all-day and recurring conflicts use fresh EventKit instances. Cached
  floating all-day conflicts are conservative across the full UTC−14…UTC+14 range;
  this can block a marginally free interval rather than miss an overlap.
- Native calls have a 12-second pre-save deadline and a 15-second Rust response
  timeout. A native call already inside EventKit cannot be forcibly cancelled;
  the ledger remains ambiguous and cannot dispatch again. A process-wide gate
  prevents a timed-out call from accumulating concurrent native saves.
- Lookup requires exactly one marker, target, title and UTC-interval match in a
  bounded ±1-day window. EventKit uses the device's current timezone for its event.
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

Product direction changed after this checkpoint: Release now compiles the trusted
Calendar create adapter in, while Debug remains off unless built with
`FLOE_ENABLE_CALENDAR_WRITES=1`. `FLOE_ENABLE_CALENDAR_WRITES=0` produces an
explicitly write-disabled Release artifact for recovery or testing. Runtime
Action Authority defaults Calendar creation to `ask` and supports `allow`, `ask`
and `deny`. Full Access alone is never authorization to create an event, and the
existing connection, validation, conflict and duplicate-suppression gates remain.

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

46 Rust tests, 72 Flutter tests and nine native assertions pass. Clippy and Flutter
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
change for another app was performed. The user subsequently granted Full Access
in the protected OS prompt. The app completed a fresh read of all 11 previously
selected calendars without changing the selection or mode.
Use a stable trusted development/release signing identity for durable app grants;
do not weaken the designated requirement to avoid permission checks.

## Signed-app UI evidence after permission grant

At 01:47–01:56 KST on September 6, the original write-enabled debug app, without
the response-loss shim, completed the following actual Flutter/EventKit flow:

1. Reconnection retained Selected mode and the same 11 calendars. All sources
   reported a fresh successful collection. Relaunch retained that connection.
2. The UI prepared a new `Floe S3 — disposable` proposal in the dedicated iCloud
   calendar, Sep 6 10:00–10:15 Asia/Seoul. Review displayed destination, Person,
   UTC interval, timezone, expiry, proposal ID and execution ID before approval.
3. `Approve & create` persisted success with external ID
   `B5D9C8A2-F4A0-4310-AFBF-687F89B6FF1A|`. The existing Flutter Calendar read
   MethodChannel/import path reported collection success; Day Canvas displayed
   exactly one 10:00–10:15 event. Event detail showed the correct calendar source,
   Asia/Seoul timezone and collection timestamp.
4. Read retry completed without another create. After quitting and reopening the
   app, the same event and successful action remained. Review exposed read retry
   but no create/approve button. A second read retry collected the same event.
5. Independent fresh EventKit inspection after those operations found exactly one
   full marker/payload match, with the same external ID as the durable action.
6. The inspector deleted only that exact disposable event. Fresh lookup returned
   zero matches. The app's read retry then reported inability to collect the
   created event, preserved successful execution and offered only read retry.
   Today returned to `No saved events for this day`.
7. After app exit, the real C ABI read of its actual DB confirmed that the complete
   action record was unchanged from before cleanup. Another fresh EventKit lookup
   returned zero matches, proving no replacement create during the failed read.

Proposal: `0e19374b-7fcf-449c-9e2b-8dc449e44cf5`.
Execution: `cd79c789-8af8-44b5-97d1-e808b0fe0f9b`.
Private evidence: `target/s3-validation/private/ui-*.json`.
This UI run created one additional disposable event after the earlier helper's
event had already been removed; both were individually verified absent.
The app's durable successful test action remains as audit history. No existing
external event, local task/note, selected-calendar list or connection mode changed.

After validation, the app was closed and Debug rebuilt without
`FLOE_ENABLE_CALENDAR_WRITES`; both ordinary Debug and Release artifacts have
writing disabled. The rebuilt app is left closed: ad-hoc identity changes can
require another user-controlled Calendar grant on a future validation build.
Never ship the private response-loss app copy.

## Acceptance boundary

| Criterion | Result | Evidence / remaining work |
| --- | --- | --- |
| S3-A1 | Pending | Actual payload review and approval pass; live rejection/no-write observation remains. |
| S3-A2 | Pending | Automated policy/Person/staleness/conflict coverage passes; controlled live blocking matrix remains. |
| S3-A3 | Verified | Durable real-provider response-loss/restart/lookup-only recovery, denied repeat execute, automated concurrent dispatch, and signed-app restart/read retries with exactly one event. |
| S3-A4 | Verified | Actual app approval → native create → Flutter Calendar read/import → Day Canvas with matching proposal/execution/external IDs. |
| S3-A5 | Pending | Real response loss and post-create read failure covered; write-denial/revocation and full provider-failure matrix remain. |

S3 has 2/5 verified criteria, not Accepted. S1 verification and dogfood remain
prerequisites; successful UI execution alone does not satisfy those gates.
