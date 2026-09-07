# EventKit disposable-event PoC

An isolated macOS arm64 command-line experiment, **not a production write adapter**.
No Flutter UI/method channel, model tool or Rust executor binding is added.
UI design and review happen in the Flutter client's design-system catalog and
design-feedback mode.

Build with the installed Apple SDK, no third-party dependencies:

```sh
sh tools/eventkit-poc/build.sh
target/eventkit-poc/floe-eventkit-poc status
target/eventkit-poc/floe-eventkit-poc calendars
```

The CLI never requests OS access. It requires existing full access from the macOS
responsible process; in the recorded run Codex already had it. This is **not** proof
of a standalone signed Floe app's permission prompt or sandbox behavior. Do not reset
TCC or silently broaden permissions to make a test pass.

## Explicitly approved test workflow

Get approval for the account, dedicated calendar, exact event and cleanup first.
`calendars` prints writable calendar/source IDs (keep these output records private).
Only a calendar named exactly `Floe Validation` is accepted. Never use production
calendar names as substitutes. `new-calendar` reuses a unique existing test calendar
in the explicit source rather than creating another on retry; run calendar setup
serially, not concurrently.

```sh
POC=target/eventkit-poc/floe-eventkit-poc
"$POC" new-calendar SOURCE_ID --approve-test-calendar
mkdir -p target/eventkit-poc/private
chmod 700 target/eventkit-poc/private
"$POC" prepare CALENDAR_ID target/eventkit-poc/private/disposable.json
"$POC" create target/eventkit-poc/private/disposable.json --approve=EXECUTION_ID --lose-response
"$POC" recover target/eventkit-poc/private/disposable.json
"$POC" recover target/eventkit-poc/private/disposable.json
```

Replace uppercase IDs with the preceding output. The prepared event is deliberately
fixed: **Floe PoC — disposable**, **2026-09-06 10:00–10:15 Asia/Seoul**, no guests,
alarms or recurrence. Approval expires in 15 minutes; create rejects a past interval.
Do not change the code's date to reuse an old approval—obtain a new approval first.

The per-ledger nonblocking file lock excludes concurrent invocation of the same
execution. Before calling `save`, the ledger atomically persists/syncs `executing`.
`--lose-response` exits with **75 after successful EventKit save**, before recording
its returned ID, simulating response loss/process interruption. A subsequent create
must fail; only `recover` may run. Recovery opens a new store/process, searches a
bounded ±1-day interval for the exact `floe-poc://execution/…` URL marker and verifies
calendar, title, interval, timezone and absence of guests/alarms/recurrence. Zero,
multiple, edited or moved-out-of-range matches remain unresolved, never a retry.
Provider IDs alone are not sufficient identity evidence.

To check S1, connect the test calendar in the existing app and navigate to Sep 6.
Confirm one occurrence, timezone and source. With approval for disposable edits:

```sh
"$POC" edit-test target/eventkit-poc/private/disposable.json --approve=EXECUTION_ID
```

This changes only the exact matched event title to `Floe PoC — disposable — edited`.
Refresh Floe; the occurrence ID should remain and its local revision increase.
Cleanup is equally constrained:

```sh
"$POC" cleanup target/eventkit-poc/private/disposable.json --approve=EXECUTION_ID
```

Refresh Floe and confirm absence. The dedicated calendar remains for future tests;
there is no calendar-delete command. Ledger files contain only disposable metadata,
live under ignored `target/`, and are mode 0600. Do not commit real calendar/account
IDs or private event data.

## Limits

- This is a single-event provider feasibility experiment, not S3 acceptance.
  Approval flags are operator safeguards, not production authorization or a sandbox.
- No guarantee about server-side iCloud persistence, another device, full resync,
  URL marker stripping or recurrence. Local EventKit success is the observed boundary.
- Native provider calls are synchronous; if stalled, interrupt the process and
  recover. There is no production timeout/cancellation wrapper yet.
- Title edits/cleanup are test utilities, outside S3 create-only product scope.
  A crash between edit and ledger persistence requires manual inspection; do not
  relax exact-match cleanup to remove an unidentified event.
- A no-match lookup does not prove no external event exists. Do not create a
  replacement automatically or overwrite the ledger.

See [live evidence](../../docs/validation/eventkit-live-poc.md).
