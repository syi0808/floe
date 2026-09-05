# S3 action review bridge

Date: 2026-09-06. Integration: Dart → JSON/C ABI → Rust → disposable Turso DB.
This checkpoint resumes implementation, not live rollout or acceptance.

The later [native execution checkpoint](s3-native-executor.md) adds ID-only
execution/recovery and capability queries. The no-execution contract below is
historical; caller-supplied policy, time and receipts remain prohibited.

Follow-up: [native review UI](s3-action-review-ui.md) binds these decisions to
the Today rail. The unfinished-UI statements below describe this bridge checkpoint.

## Contract

`floe_core_calendar_actions` accepts schema version 1, `person_id`, and an
`operation` tagged by `kind`: `list`, `get`, `propose`, or `decide`.
`decide` accepts only `approve` or `reject` and an immutable action ID.
Responses use the existing envelope with `data.actions`: a Person-scoped list
(newest creation first, ID tie-break) or one action for other operations.
The action wire fields follow the existing Rust ledger serialization.

Propose supplies calendar ID, title, RFC3339 start/end and timezone. Rust derives
the provider, calendar name and revision from its connection, generates IDs, and
uses `Utc::now()` for creation/expiry and approval. Caller-supplied clocks, policy,
execution state and unknown request fields are rejected. There is no provider
create, recovery, receipt injection or execution operation in this API.

`FfiDayGateway` exposes typed proposal, decision, single-action and list methods.
`CalendarAction` preserves review payload, timestamps, revision, execution identity,
state/reason and external ID. Dart's injected Day Canvas clock does not control
approval. Reopening the DB lists the durable ledger without remembered UI IDs;
loading does not reset states, approve, dispatch or recover anything.

This is a trusted in-process application API, **not** an authentication boundary.
Person-scoped lookup does not authenticate an arbitrary caller. Never expose the
decision API as a model tool, remote endpoint or untrusted Expert capability.
Approval records consent only; it does not claim current permission, writable
capability or conflict clearance. Those remain mandatory execution checks.

## Validation

```sh
cargo test --workspace
cargo build -p floe-ffi
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cd apps/client
flutter test test/calendar_action_gateway_test.dart
flutter test
flutter analyze
```

Results: 44 Rust tests and 58 Flutter tests pass (including the new real-FFI
integration test). Rust build, Clippy with warnings denied, formatting check and
Flutter analysis pass.

C ABI tests cover pending/approved/rejected persistence, repeated approval conflict,
foreign Person isolation, restore/list, malformed IDs, unsupported schema, null
handle, and rejection of execution/clock/policy input. Flutter integration covers
the real dylib/isolate path, typed fields, trusted clock despite a year-2000 Dart
clock, duplicate approval, reopen and an unchanged empty timeline/provider fixture.

No live calendar read/write or OS permission interaction is needed for these tests.
S3 remains 0/5. Flutter approval UI, native executor/preflight/recovery, read retry
and the controlled S1/live S3 matrix remain unfinished. Continue using the
[prototype-first review gate](../design/s3-calendar-action-ui.md) before UI porting.
