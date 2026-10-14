# Post-audit qualification — 2026-10-14

Source baseline: `0566a7dfe39891ead8a140780f30950863c4ce9b`.
This is a bounded qualification record, not a claim that every product feature or
Apple platform is complete. Implementation and this validation were performed
directly by the primary worker.

## Combined gates

- `cargo test -p floe-app --no-default-features --features qa-fixtures --tests --no-fail-fast`: 65 passed, zero failed; one standalone crash-driver entry intentionally ignored and exercised by the passing subprocess tests. The 41-test Conversation suite includes actual 129-turn history/reopen and finished in 651.98 seconds.
- Server `go test -race -count=1 ./...` and `go vet ./...`: passed using Go 1.27.2.
- `flutter build linux --debug --no-pub`: passed both with ordinary development storage and with `FLOE_LINUX_QA_FEATURE=qa-fixtures`, Flutter 3.47.6 / Rust 1.93.1.
- Earlier source-matched gates remain recorded in the refactor plan: 393 workspace tests including doctests, 233 Flutter tests, production FFI build, 15 compiled Swift Calendar identity checks and 17 compiled Swift sleep-window checks.

## Actual Linux desktop observations

A fresh, isolated development profile was used; existing profiles were neither
reset nor adopted. The desktop was controlled through native UI automation.
Firefox ESR 140.16 was the browser used; no real provider credentials, external
model calls or user-calendar/health data were used.

- Floe rendered its Day screen, including the explicit missing Calendar coverage state.
- The missing-coverage link opened Connections; unavailable Apple integrations were shown as unavailable on Linux.
- Settings opened the Expert policy, Assistant features, remote-server and Data & privacy screens. Manage memory showed zero saved and pending items plus the immediate-forget semantics.
- A fresh loopback development Go server started. Firefox rendered its dashboard and normal test-token sign-in reached `Node unlocked`; the token was not committed or reported. An initial UI-keyboard case-encoding error was corrected before the successful sign-in. This was not a server authentication defect.
- Reopening the same isolated app profile with the fixture-enabled build succeeded. Synthetic QA Calendar became available; choosing only the synthetic team calendar persisted as one connected resource.
- The synthetic source was allowed for this device only. Returning to Day completed collection and displayed “Calendars synced for this date” with the correct empty day. Opening New event refused creation because no writable Calendar destination was confirmed; Create event stayed disabled. No external write was dispatched or claimed.
- The New task action currently shows the existing “This feature is not available yet” placeholder. This is not a successful Task CRUD qualification.

## Remaining qualification boundaries

- Full macOS/iOS SDK builds, native EventKit/HealthKit behavior and Apple screen QA remain unverified here. Foundation helper execution and syntax parsing do not substitute for those gates.
- Historical intermittent Storage failure R050 is still unclassified; a passing combined run does not prove its underlying cause resolved.
- The user's original Castles reset/run-local failure cannot be attributed without its exact log and environment; synthetic/script checks do not establish that original cause.
- No live provider/model, persistent server pairing or production deployment was qualified by this run.
