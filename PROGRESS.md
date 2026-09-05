# Floe Progress

> Last updated: 2026-09-06
>
> Purpose: 구현 진행현황만 추적한다. 제품 정의와 기술 설계는 `docs/planning/` 및 ADR을 따른다.

## Delivery Board

Delivery follows [ADR 0006](docs/decisions/0006-slice-driven-delivery.md) and the
[slice acceptance plan](docs/planning/08-engineering/vertical-slice-delivery.md).
Phase order is no longer an implementation gate. Acceptance counts below report
verified criteria, not estimated implementation percentages.

| Slice | Status | Integration evidence | Acceptance | Blocker / prerequisite | Next demo |
| --- | --- | --- | --- | --- | --- |
| S1 — Calendar read | Implementing | Live timed-event PoC; dual scope, partial recovery, disconnect and DST automated checks | 0/4 | Controlled permission/DST/recurrence/lifecycle gates | Finish controlled live matrix |
| S3 — Approved action | Implementing (bridge only) | Rust fixture; isolated live EventKit create/recovery; prototype UI; Dart decision/ledger bridge | 0/5 | S1 Verified; prototype review; trusted native executor binding | Bind reviewed approval UI after live gates |
| S4 — Cross-device/server | Planned | None | 0/4 | S3 Accepted; sync/security PoCs | Same result on two devices |
| S5 — Intervention | Planned | None | 0/4 | S4 Accepted; resident lifecycle | Calendar change triggers controlled suggestion |

S1 implementation now connects a native EventKit adapter to Rust-owned mirror
storage and Day Canvas. No live acceptance criterion is marked verified yet.

### S3 decision bridge checkpoint — 2026-09-06

- Added Person-scoped action list/get, proposal and explicit approve/reject over
  JSON/C ABI, with typed Dart access and durable reload. Rust owns action clocks.
- No execution/receipt/policy input is exposed; native UI and live writes remain gated.
- [Contract and automated validation](docs/validation/s3-action-bridge.md).
  S3 remains 0/5 and S1 is not Verified.

### S1 scope and recovery checkpoint — 2026-09-05

- Selected retains explicit IDs; All discovers new calendars on refresh/date reads.
  Legacy connections remain Selected. Scope UI was implemented in the prototype first.
- Per-source recovery, missing-source cache, disconnect revision tombstones,
  23/25-hour civil-day reads/timelines and ordinary EventKit move identity are implemented.
- [Validation and remaining live gates](docs/validation/s1-scope-recovery.md).
- Historical notes below predate this two-mode decision; auto-inclusion is required
  only in All mode. S1 remains 0/4 verified, not complete from fixture evidence.

### Live EventKit and prototype-first checkpoint — 2026-09-05

- User-approved isolated helper created one disposable event in a dedicated iCloud
  Calendar, deliberately lost its response, blocked a second create and recovered
  exactly one matching execution marker from fresh processes.
- Existing app re-imported it with matching ID, Person and Asia/Seoul times. External
  test edit preserved occurrence identity and advanced revision 0→1; test deletion
  disappeared on refresh. The empty test calendar remains; existing events untouched.
- **S1 defect:** ordinary refresh omitted the new calendar; reconnect required an
  explicit selection. S1 is not Verified; complete acceptance remains 0/4.
- Native permission deny/revoke cycles, DST/recurrence, partial failure and offline
  restart remain unverified. This is local EventKit evidence, not cloud durability.
- UI is implemented/reviewed in the HTML prototype first: approval/rejection,
  preflight/create/re-import, blocked states, lookup-only recovery and read-only retry.
  No Flutter UI or production write API added. S3 remains 0/5.
- Prototype build, 42 component contracts and 26 reducer assertions pass; seven
  browser scenarios validated. Swift helper compiles with warnings denied/signs.
- [Full live record and remaining gates](docs/validation/eventkit-live-poc.md).

The reusable [performance-class routing](docs/decisions/0011-inference-performance-classes.md)
and local Go model gateway remain implemented. No current product feature invokes
the gateway after removal of the focus-time experiment. This does not advance S4
server/sync acceptance.

### Local connection console checkpoint

[ADR 0010](docs/decisions/0010-local-connection-console.md) and its
[validation record](docs/validation/local-connections.md) cover reusable infrastructure.
Rust and Flutter suites, paired non-default-port fixture, Go race/vet, installed
Codex handshake and disposable Keychain round trip pass.
Local server/dashboard are running and the operator confirmed native app pairing.
Keychain persistence across a relaunch remains a manual checkpoint because native
UI automation timed out. No real model or OAuth consent is claimed.

## Acceptance Evidence

### S3 executor checkpoint — 2026-09-05

- Immutable calendar-create proposals with explicit approve/reject and expiry.
- Rust-owned durable execution IDs and atomic CAS transitions; duplicate execution
  claims cannot dispatch twice. Ambiguous/interrupted execution uses lookup only.
- Trusted policy/Person/target checks, connection-change detection, provider
  preflight contract for capability/permission/timezone/conflicts, and receipt matching.
- Fixture covers successful re-import, reopen, concurrent execution, cancellation
  after write, typed failures and ambiguous recovery without blind retries.
- `cargo test --workspace`: 35 passed, including 11 new action tests;
  Clippy with warnings denied and formatting checks pass.
- [Validation and adapter contract](docs/validation/s3-calendar-action.md).
  No Flutter/FFI action surface, EventKit write adapter, live provider PoC or UI
  acceptance is claimed. S3 remains 0/5; S1 remains Deferred, not Verified.

S1 automated and native-build evidence is described in
[the validation runbook](docs/validation/s1-calendar.md). Live results remain pending;
fixture-only evidence cannot satisfy the live integration acceptance gate.

Evidence build: `0d52e2620938fc7008572937e72dc8d6572c6e0b`, 2026-09-04,
macOS 26.2 (25C56), arm64. Connector integration is fixture; EventKit is SDK/build-only.

| Criterion | Result | Evidence | Remaining live gate |
| --- | --- | --- | --- |
| S1-A1 | Pending | `calendar_gateway_test.dart`: typed denial retains cache; settings/refresh actions pass. Native EventKit app compiles/signature verifies. | Real prompt, selection, revocation/reconnect |
| S1-A2 | Pending | Rust calendar tests and Flutter FFI/widget tests: provenance, Person, IDs, change token, all-day and UTC+09 midnight | Real normalization, recurring exceptions and timezone cases |
| S1-A3 | Pending | Rust calendar tests: idempotent import, update, range-only deletion, stale/invalid batch rejection | Real external edit/delete and recurring identity |
| S1-A4 | Pending | Rust and native FFI tests: cached events/error survive reopen; retry clears failure | Signed app lifecycle and provider-failure demo |

### S1 implementation checkpoint

- EventKit calendar listing, explicit permission request, date-range read, and settings recovery.
- User-approved OS full-access exception; the app exposes no external write operation.
- Person-scoped selection, stable occurrence provenance, change token, and atomic CAS mirror persistence.
- Duplicate/stale/invalid-batch rejection, range-scoped reconciliation, preserved cache on failure/restart.
- Source labels, multiple all-day events, expanded timeline for midnight/off-hours, and manual refresh.
- Fixture tests cover Rust storage/projection and the actual Dart/JSON/C ABI boundary.
- Live EventKit permission/read behavior, recurring identity, DST, S3 create PoC, and three-day dogfood remain unverified.

### S1 UI reference

The HTML prototype now uses a quiet unified Today view of all connected calendars,
connection inventory, and five popup flows without an on-page prototype lab.
User feedback expands the target from one selected calendar to all calendars available
through macOS Calendar; [ADR 0008](docs/decisions/0008-unified-calendar-read.md) records
the scope and pending native migration. Existing native single-selection evidence does
not establish acceptance of this new multi-calendar contract.
[UI specification](docs/design/s1-calendar-ui.md) records behavior, responsive checks,
and proposed-vs-native boundaries. This changes the design reference only; native
implementation and live acceptance counts are unchanged.

## Existing Personal Day Baseline

ADR 0004 remains partially delivered, not accepted. Existing delivered work is
preserved below. Non-blocking local UI breadth is deferred while S1 and S3 are
prioritized; MVP acceptance and its two-week dogfood requirement remain separate.

## Current Checkpoint

- [x] Rust workspace and Personal Timeline domain baseline
- [x] Event, Task, Note, and Capture separation
- [x] Capture provenance and revision-aware mutations
- [x] Deterministic Day Snapshot with Now, Next, and overdue projection
- [x] Embedded Turso persistence and schema migration on macOS
- [x] Versioned JSON protocol and C ABI
- [x] Dedicated Dart FFI isolate and native handle lifecycle
- [x] Flutter Day Canvas and typed Universal Capture
- [x] Explicit Event, Task, and Note classification
- [x] Task completion/reopen and item deletion
- [x] macOS dylib build, embedding, signing, and persistence verification
- [x] Squircle-first responsive shell, Day Canvas, Notes, and Task Detail baseline
- [ ] Event, Task, and Note editing UI
- [ ] Explicit conflict recovery UI
- [ ] Dense-day folding and complete timeline treatment
- [ ] Calendar integration
- [ ] Two-week Day Canvas dogfood

## MVP Areas — Separate Product Scope

| Area | Status | Completed | Remaining |
| --- | --- | --- | --- |
| Day Canvas | Partial; non-blocking breadth deferred | Now/Next, unified local projection, empty and overdue states | Conflict UX, editing, folding, interventions |
| Universal Capture | Partial | Typed capture, original input, explicit classification, provenance | Voice/STT, correction flow, semantic candidates |
| Local Personal-Day Store | Partial | Rust-owned Turso, CRUD core, deterministic snapshots, reopen persistence | Encryption, export/forget, sync, multi-person management |
| Minimal Assistant | Not started | Reusable Go inference infrastructure only | Select and validate a product use case |

## Roadmap Status

These are delivered capability statuses, not sequential work gates. Planned
cross-phase coverage is defined in the slice plan and does not change these statuses.

| Phase | Status | Notes |
| --- | --- | --- |
| Phase 0 — Architecture PoCs | Partial | macOS embedded Turso and Flutter↔Rust boundary validated; other PoCs remain |
| Phase 1 — Personal Day | Partial | Technical vertical slice works; product breadth and dogfood remain |
| Phase 2 — Connected Floe | Partial | S1 EventKit read path implemented; live validation and other connectors remain |
| Phase 3 — Personal Memory | Not started | Memory, people and identity resolution remain |
| Phase 3.5 — Expert Ecosystem | Not started | Package, permissions, sandbox, SDK, and marketplace remain |
| Phase 4 — Cross-device | Not started | Sync, Device Agent, and native packaging remain |
| Phase 5 — Ambient Floe | Not started | Wake word, transcription, handoff, and interventions remain |
| Phase 6 — Hosted/Self-host | Partial; local inference only | Local Go gateway; hosted server, accounts, deployment and administration remain |

## S1 Automated Validation

2026-09-04, macOS arm64. Dependency mode: fixture for connector behavior;
EventKit native adapter compilation only, not a live read.

- Rust: 22 tests passing (`cargo test --workspace`, including existing External-source compatibility).
- Flutter: 39 tests passing, including actual native FFI tests and load-error recovery (none skipped).
- Clippy: clean with warnings denied; Flutter analyzer: clean.
- macOS: debug app builds with Calendar entitlement and EventKit usage strings.
- No external calendar data collected or changed; live acceptance remains 0/4.

### Follow-up test run

2026-09-04 14:05 KST, source `8fafc96` (implementation `0d52e26`), same macOS environment:

- `cargo test --workspace`: 20 passed; `cargo build -p floe-ffi`: passed.
- `flutter test`: 38 passed, no skips, including native FFI and Calendar fixture tests.
- Clippy with warnings denied, Flutter analyzer, and debug app signature verification pass.
- Existing debug app launches; process sampling shows the main thread waiting normally
  in the AppKit event loop rather than blocked in Calendar code.
- Live UI automation is blocked: Computer Use repeatedly returns `-10005 timeoutReached`
  while acquiring the Floe window, including by resolved bundle ID `app.floe.floeClient`.
  This is not evidence of an EventKit permission/read failure or a successful UI demo.
- No Calendar permission prompt was accepted, no calendar selected, and no live data
  collected or modified in this run. S1-A1–A4 stay pending; resume with a visible
  connection/permission result supplied by the user or working UI automation.

## Local-data Compatibility Fix — 2026-09-04

- User reported only the retry button was visible. An isolated diagnostic copy of
  the existing store reproduced `storage: unknown variant External` through the
  bundled C ABI. This is independent of the UI automation timeout noted above.
- Added lossless `External` source decoding/encoding and a separate legacy-source
  wire variant; existing provenance is not rewritten into a newly selected Calendar.
- Existing external events remain read-only; no stored records are deleted or reset.
- Day-load failures now show the actual error and retry action instead of an unexplained button.
- The same diagnostic copy loads successfully with the fixed native library.
  Regression tests cover old JSON persistence/reopen, provenance round-trip,
  write rejection, and visible error/retry recovery. No private data is committed.
- Live EventKit acceptance remains pending; this validates local-data compatibility,
  not permission grants or provider reads.
- Rebuilt and signature-verified the debug app, then relaunched it against the
  existing store. UI inspection now succeeds and shows Day Canvas plus **외부 Calendar
  연결 / 연결 / 권한 설정**, rather than only retry. The temporary diagnostic copy was removed.

## Historical Personal Day Validation

Last recorded baseline: 2026-09-03. These results are preserved from the previous
progress record, not rerun or newly verified by the 2026-09-04 planning change.

- Rust: 15 tests passing
- Flutter: 12 tests passing
- Rust workspace: Clippy clean with warnings denied
- Flutter: analyzer clean
- macOS: signed release app contains the expected six C ABI symbols
- Native persistence: Event, Task, and Note lifecycle survives app/core restart

## Next Priorities

1. Complete S1's live Calendar criteria.
2. Review S3 approval/recovery in the HTML prototype first. The isolated EventKit
   create/recovery PoC passes; native Rust binding and production permission/error
   handling remain gates before enabling product writes or porting UI to Flutter.
3. Evaluate an officially supported OAuth adapter and Apple native availability separately; do not assume CLIProxyAPI adoption.

Deferred, not completed: Event/Task/Note editing UI, general conflict recovery UI,
dense-day folding, and the separate two-week Personal Day dogfood. If one blocks
the active slice, pull in only the necessary portion; slice-specific error and
conflict handling remains mandatory.

## Update Rules

- Update this file in the same commit as a milestone status change.
- Record only delivered, validated, in-progress, or blocked work.
- Do not duplicate product requirements or architecture decisions here.
- Do not mark roadmap work complete based only on scaffolding.
- Update validation counts whenever tests are added or removed.
- Track at most one slice in Implementing, Integrated, or Verified; Dogfooding may overlap the next slice.
- Advance states only with evidence; record blockers separately and regress status when acceptance fails.
- Distinguish fixture, sandbox, and live evidence for each dependency; record known limitations.
- Keep criterion definitions in the slice plan and results here; do not restore subjective phase percentages as the primary delivery metric.
