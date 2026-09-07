# S4 bounded Calendar Timeline View

Date: 2026-09-07. Connector/Expert preparation; S4 remains **0/14**.

## View boundary

`CalendarTimelineViews` implements the same `ExpertViews` port used by the Schedule
and declarative Experts. It projects the real Core Calendar mirror schema rather
than embedding a second hard-coded sample schedule. The trusted host supplies an
immutable per-lease grant: Person, opaque View handle, provider, exact Calendar IDs,
connection revision, one local day, a UTC planning window and expiry.

The grant allows at most four selected calendars, a planning window of at most
24 hours and an expiry no more than five minutes away. It is not an Agent-supplied
request, durable installation record or new permission UI. Host orchestration must
bind it to explicitly authorized Person/Expert assignments; this component does not
automatically grant or broaden registry access, discover calendars or refresh imports.

Before returning evidence, the adapter:

1. Checks caller Person/View identity, budget, deadline and cancellation.
2. Obtains a live read-access stamp for exactly the granted calendars through the
   `CalendarReadAccess` port. It never assumes a saved connection means current OS access.
3. Reads the Person-bound mirror with a 4 MiB SQL payload guard before JSON decoding.
4. Requires the expected connection revision, selected scope and per-source successful
   import covering the requested day/window. Data older than five minutes, a future
   import clock, disconnected/failed/missing sources and uncovered ranges fail closed.
5. Projects at most 32 events and 16 KiB of output. Overfull schedules fail rather
   than truncating away busy intervals and fabricating a free slot.
6. Rechecks native access generation, mirror identity/content/revision and expiry
   before returning the immutable View. A later `revalidate` checks the same lease
   before a caller publishes dependent results.

A healthy explicitly granted subset can remain available when another selected
calendar fails. If a failed calendar is part of this View, the whole View fails;
it is not silently omitted from focus analysis. These source failures remain typed
Agent failures so another independent Expert can still answer.

Only Core event UUIDs, bounded untrusted titles and clipped time intervals reach
the Expert. Provider IDs/revisions, calendar/account names, modification capability,
native objects and timezone metadata are not included. Titles are shortened at a
UTF-8 boundary with an ellipsis; their contents never become instructions.

All-day events conservatively block the requested window; crossing-midnight/long
timed events are clipped, not discarded. The host's start/end day offsets preserve
23/25-hour DST day bounds. A 25-hour whole-day request exceeds the existing Expert
contract and is rejected, while a valid shorter planning window remains supported.
This Calendar-only View does not read local manual events, tasks or notes; S3 still
checks local events and fresh provider conflicts before any eventual write.

Fixture mirrors produce Synthetic data; EventKit mirrors produce Personal data.
No raw Health/Screen Time/location projection or remote transfer is introduced.
The View expiry is the earlier of the grant expiry and each source's cache expiry.
This is a bounded cached observation, not proof that EventKit has had no changes
since the last import. Callers must refresh via the existing read connector when needed.

## Native read-access adapter

The bundled EventKit library now supports a read-only `view_access` operation.
It validates version, local Person, provider, unique bounded calendar scope and
deadline, requires full read access before and after inventory lookup, and checks
that each requested calendar still exists. It never requests permission, returns
events or writes Calendar data. Write-only permission is insufficient.

A process-local generation changes on delivered `EKEventStoreChanged` notifications;
generation changes during a read invalidate the result. This is an invalidation
hint, not a durable sync checkpoint or a guarantee that every external change was
observed before a cached read. The actual SDK declaration in `EventKit/EKEventStore.h`
documents that notification and the current-authorization query used here.

Rust runs access checks on one owned background worker and passes a deadline to
the native boundary. Stop or a dropped waiter cannot release its ownership early
and start another native check. The existing native call lock still isolates a
blocked OS call after a transport timeout. Cancellation drops delivery, not the OS
operation itself. Per-read child cancellation tokens prevent an abandoned View from
leaving a cooperative provider worker running or cancelling its caller after success.

## Automated evidence

- Thirteen new Core tests cover exact scopes/identity, metadata minimization, clipped
  intervals, healthy subsets, expired/missing coverage, source revisions/generations,
  denial, Unicode/byte/item bounds, DST and all-day behavior, malformed foreign rows,
  cancellation/deadline/drop cleanup and the bounded database read.
- One of these runs the actual Schedule Expert over the Core-backed View, revalidates
  it, atomically commits its result/private state to the encrypted vault and passes
  its reference into the existing Manager-to-S3 bridge. The result is a pending
  Review action. All records, access stamps and keys are fixtures; no provider write
  or personal conversation is performed.
- Two native Rust tests cover pre-dispatch scope/cancellation denial and worker
  ownership after Stop/drop, followed by a successful independent access check.
- The production Swift source compiles with warnings as errors. Its pure injected
  access tests check schema/scope, generation and before/after permission withdrawal.
  Together with existing action checks, **25 native assertions** pass without invoking
  OS permissions, Calendar inventory or event access.
- All **151 workspace Rust tests** and the separate keyring example's three tests pass.
  Formatting and Clippy pass with the two existing Calendar exclusions.
- All **135 Flutter tests**, Flutter analysis, the rebuilt Rust/native libraries and
  the macOS Debug build pass. Deep strict signature verification passes for
  `floe_client.app`. No UI or golden changes were needed for this adapter.

```sh
CARGO_INCREMENTAL=0 cargo test --workspace
CARGO_INCREMENTAL=0 cargo build -p floe-ffi
bash tools/s3-validation/check-native.sh
cd apps/client
flutter test
flutter analyze
flutter build macos --debug
codesign --verify --deep --strict build/macos/Build/Products/Debug/floe_client.app
```

## Remaining integration and live gates

The default sample panel is unchanged and still does not read connected sources.
The new lease/access port is not yet orchestrated by a production Agent turn or
automatically revalidated by its session store. The host must integrate live package
installation/assignment and View binding, refresh/scope selection, per-turn lease
validation, model calls and Manager proposal publication before enabling that path.
This checkpoint does not claim a live native access-stamp round trip or full C1/A4
acceptance merely because the production adapter builds and injected checks pass.

The [production keyring gate](s4-keyring-live-smoke.md), [local model generation gate](s4-local-model.md),
remote inference/authentication, other Connectors, trace/replay and S1/S3 live
acceptance remain open. Personal chat stays gated; no plaintext fallback or automatic
permission request was added.
