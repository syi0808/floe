# S1 Calendar validation

Date: 2026-09-04. Native provider: macOS EventKit. Actual account data has **not**
been collected by this implementation run. No external events have been created.

## Automated evidence

Run from the repository root:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p floe-ffi
cd apps/client
flutter analyze
flutter test
flutter build macos --debug
```

`crates/floe-core/tests/calendar.rs` covers stable local identity, unchanged imports,
updates, date-limited deletion, Person isolation, stale selection/batch rejection,
atomic invalid-batch rejection, permission-error persistence, retry, reopen,
all-day exclusive boundaries, and UTC+09 midnight projection.

`apps/client/test/calendar_gateway_test.dart` uses the same CalendarAdapter contract
as EventKit, but the dependency is explicitly **fixture**. It exercises Dart → JSON
C ABI → Rust → Turso → snapshot decoding, error recovery and reopening, plus the
permission-failure widget's refresh/settings actions. Native ABI tests require the
debug dylib and are not evidence if skipped.

## Live PoC and acceptance checklist — not yet run

Use a dedicated test calendar in macOS Calendar, not a personal production calendar.
Run `flutter run -d macos`, then use **Calendar 연결 → 계속 → 캘린더 선택**.

1. S1-A1: deny the OS request; verify a recoverable error. Open **권한 설정**, allow
   Calendars access, reconnect and select the test calendar. Revoke permission while
   running; refresh and verify `permission_denied`, retained data and recovery.
2. S1-A2: prepare ordinary, multiple all-day, cross-midnight, and recurring events.
   Refresh their dates; verify title, calendar provenance, correct local wall time,
   all-day exclusive end, and the local/occurrence IDs in a test-only inspection.
   Test a timezone different from the device and a DST transition explicitly.
3. S1-A3: refresh twice, edit/delete in macOS Calendar, then refresh. Verify no
   duplicates, updated fields and deletion. Cache another date and verify a refresh
   of the first date does not remove the second date's data. Test recurring exceptions
   and moving an occurrence into/out of the fetched interval.
4. S1-A4: quit/relaunch after collection, then retry with denied permission and with
   the calendar removed. The cached timeline, last range/time and typed error survive;
   successful refresh clears the error. Validate the signed release build too.
5. S3 prerequisite, separate from S1: with explicit approval for a disposable test
   event, validate EventKit create capability and re-import in a bounded PoC. No
   create API or create PoC execution is part of this read-only app implementation.

Record each criterion's result, dependency mode, build SHA, OS/account type and
limitations in PROGRESS.md. Fixture success and a successful build must not advance
the live acceptance count.

## Dogfood plan

After all four live criteria pass, use S1 for three working days. Each day record:
missing/duplicated events, wrong day/time, stale status clarity, permission recovery,
and refresh usefulness. Do not record private event text. Review all blocking defects
before marking Accepted. This is separate from Personal Day's two-week dogfood.

## Current limitations

- Refresh is manual for the selected date; no background/resident sync or push.
- One calendar, one device-local Person; switching clears only the previous mirror.
- DayQuery uses a fixed offset. DST-transition-day behavior is not proven.
- Native identifier behavior after account reset and recurring edits is unverified.
- An EventKit fetch that silently returns an incomplete array cannot be distinguished
  from a complete result by this API. Live provider validation remains required.
- Mirror data uses baseline local unencrypted Turso storage; revocation preserves
  the cache rather than deleting it. No raw calendar data is logged.
