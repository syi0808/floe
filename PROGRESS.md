# Floe Progress

> Last updated: 2026-09-07
>
> Purpose: 구현 진행현황만 추적한다. 제품 정의와 기술 설계는 `docs/planning/` 및 ADR을 따른다.

### Unified review and action authority — 2026-09-06

- Reframed Calendar Proposal as an internal action intent projected into shared
  Review requests and Activity surfaces.
- Main Review shows only actionable items; terminal Calendar results move to the
  separate Activity destination.
- Added durable Person-scoped Calendar create authority (`allow` / `ask` / `deny`)
  and an Action permissions settings surface. The default remains `ask`.
- All app builds include Calendar writing. Runtime policy and fresh provider
  checks still gate every execution.
- [Product and domain contract](docs/planning/01-experience/review-authority-and-activity.md).
- Began Apple Calendar-familiar direct manipulation: Calendar creation now starts
  from the toolbar `+` or an empty 15-minute-snapped double-click rather than a
  labeled proposal control in the contextual rail.
- Empty days retain the full interactive time grid. `A little breathing room` is
  now a compact, non-blocking banner in the existing Calendar tools row.
- Drag-to-move and capability-aware context menus remain the next interaction slice;
  they will use the same authority/review/activity boundary.

## Delivery Board

Delivery follows [ADR 0006](docs/decisions/0006-slice-driven-delivery.md) and the
[slice acceptance plan](docs/planning/08-engineering/vertical-slice-delivery.md).
Phase order is no longer an implementation gate. Acceptance counts below report
verified criteria, not estimated implementation percentages.

| Slice | Status | Integration evidence | Acceptance | Blocker / prerequisite | Next demo |
| --- | --- | --- | --- | --- | --- |
| S1 — Calendar read | Implementing | Live timed-event PoC; dual scope, partial recovery, disconnect and DST automated checks | 0/4 | Controlled permission/DST/recurrence/lifecycle gates | Finish controlled live matrix |
| S3 — Approved action | Integrated; validating | Signed-app approval/create/collection/restart; real response-loss recovery and exact cleanup | 2/5 | S1 Verified; live rejection/blocking/failure matrix; dogfood | Complete remaining acceptance matrix |
| S4 — Connected Agent/Experts | Planned; secure panel and native model preparation | Encrypted sample panel; bounded Foundation Models adapter; real availability probe, no live generation | 0/14 | S3 Accepted; P0-I/P0-C/P0-K/P0-L; provisioned key/lifecycle gate; Apple Intelligence disabled | Provisioned key smoke; live local model and supported remote adapter |
| S5 — Memory/self-improvement | Planned | None | 0/5 | S4 Accepted; P0-D corpus; P0-F local vault/key | Review, reuse and roll back one Memory and Playbook change |
| S6 — Transcription/voice | Planned | None | 0/5 | S5 Accepted; streaming/recording STT/TTS PoC | Continue Agent chat by voice and review one source-linked transcript |
| S7 — Local wake-up | Planned | None | 0/4 | S6 Accepted; resident wake lifecycle | Wake phrase opens a visible local voice session |
| S8 — Cross-device/server | Planned | None | 0/4 | S7 Accepted; sync/security PoCs | Same result on two devices |
| S9 — Intervention | Planned | None | 0/4 | S8 Accepted; intervention policy | Calendar change triggers controlled suggestion |

S1 implementation now connects a native EventKit adapter to Rust-owned mirror
storage and Day Canvas. No live acceptance criterion is marked verified yet.

### S4 signed-key probe and native model adapter — 2026-09-07

- Added a disposable signed Core/keyring smoke harness with exact-slot cleanup
  and retained markers on uncertainty. Read-only NoEntry succeeds, but production
  vault creation fails and cleanup reports a missing entitlement. Signing alone
  did not pass the gate; no personal database or existing key was accessed.
- Added a macOS Foundation Models adapter behind the common Rust ModelRunner.
  Bounded structured output, policy checks, one native job, cancellation/deadline
  ownership and conservative full-context token reservations are implemented.
- The actual bundled availability probe reports AppleIntelligenceNotEnabled.
  Live generation is not verified; the default app remains on encrypted samples.
- Validation: 101 workspace Rust tests, three keyring example tests and native
  Swift fixture checks pass; macOS Debug build, formatting and Clippy with existing
  exclusions pass. No acceptance criterion is promoted.
- [Signed-key result and cleanup](docs/validation/s4-keyring-live-smoke.md);
  [native model evidence and remaining gates](docs/validation/s4-local-model.md).

### S4 vault host and panel integration — 2026-09-07

- Connected the default Today assistant to the encrypted vault through a new
  versioned submit/poll/stop/release C ABI and typed Dart gateway. The old sample
  route remains preset-only test infrastructure, not a fallback or migration.
- Moved key/DB operations and sample turns to a dedicated native worker. Blocked
  OS key calls do not block polling, Stop or native-handle shutdown; the original
  worker retains ownership until it actually finishes and releases the vault.
- Added explicit storage setup, unlock and lock controls. Close/navigation/app
  inactivity clears presented messages immediately, cancels/drains work and
  requests lock; late completion cannot repopulate a sealed controller.
- Added response-loss reconciliation without duplicate provisioning, worker
  ownership/cancellation tests, native read-only status tests and secure-panel
  checks at 320/390 widths and 200% text.
- Validation: 96 Rust and 129 Flutter tests pass; Flutter analyzer, native build,
  macOS Debug build/signature verification, formatting and Clippy with the
  existing Calendar exclusions pass.
- No personal text or real model is enabled. A signing identity is available,
  but live key creation/reopen, physical lock/denial and repair/cleanup still
  require validation. S4 remains 0/14; S1/S3 acceptance is unchanged.
- [Evidence and remaining work](docs/validation/s4-agent-vault-host.md).

### S4 encrypted session-store component — 2026-09-07

- Added a separate Person-scoped Turso AES-256-GCM session vault, encrypted
  identity binding, bounded session CAS and a lifetime exclusive file lock.
- Adopted the keyring-rs ecosystem (`keyring-core` + explicit Apple protected
  store) instead of directly implementing Security.framework calls. Keys stay
  native; the backend requests device-local, when-unlocked access.
- Key loss/change, failed provisioning, wrong keys, tampering and missing/empty
  databases fail closed without silent key replacement or plaintext fallback.
- Synthetic tests cover DB/WAL content, reopen, identity/revision isolation,
  competing processes and interrupted runtime recovery after key loss.
- Validation: 91 Rust and 117 Flutter tests pass; Flutter analyzer, native
  library/macOS Debug app builds, signature verification, formatting and Clippy
  with the two existing Calendar exclusions pass.
- This component is not connected to the sample panel/C ABI. No live Keychain
  access or personal data was used; signed-host key access and lifecycle gates
  remain open. S4 stays 0/14 and S1/S3 acceptance is unchanged.
- [Evidence, library choice and remaining gates](docs/validation/s4-agent-vault.md).

### S4 sample assistant panel — 2026-09-07

- Added one user-invoked Floe entry on Today: a contextual desktop panel and a
  narrow-screen sheet. Sample questions, source disclosure, progress, Stop,
  retry, read-only reload, explicit interrupted-session recovery and new/resumed
  conversations now use the shared Rust session/event contract.
- Added a bounded native run slot with begin/poll/stop/release, replayable event
  cursors and cancellation on native-handle shutdown. Calendar requests remain
  usable while the cooperative fixture model is waiting.
- The panel never accepts free text, reads connected sources or sends data to a
  model provider. Responses are synthetic; model delay is intentional fixture
  latency, not token streaming. Personal chat stays locked pending the vault.
- Controller/native tests cover cancellation, duplicate starts, response replay,
  shutdown, persistence and transport recovery; widget checks cover 320/390
  widths, 200% text, keyboard focus and the desktop/sheet entry paths.
- Validation: 77 Rust and 117 Flutter tests pass (three new native and 13 new
  Dart/widget tests); Flutter analyzer and macOS Debug app build pass. Strict
  whole-workspace Clippy still reports the two existing Calendar warnings.
- [Validation and remaining gates](docs/validation/s4-agent-panel.md).
  S4 stays 0/14; S1/S3 live acceptance is unchanged.

### S4 Agent contract foundation — 2026-09-07

- Began preparatory S4 implementation at the user's request without promoting the
  slice past its S3/session-vault prerequisites. S1/S3 live status is unchanged;
  S4 remains 0/14, not Integrated or Accepted.
- Added provider/UI-independent `floe-agent` ports, versioned commands/events,
  bounded multi-turn execution, read-only capability discovery, cancellation,
  deadline/token/cost/output/context/session limits and repeated-call halting.
- Added explicit inference placement/consent/projection checks, sensitive history
  classification retention, stale-source checks and fail-closed vault availability.
  Raw device data and credentials are excluded from Agent context entirely.
- Added CAS-persisted synthetic sessions and a preset-only fixture route through
  JSON/C ABI and typed Dart gateway. Completed capability input/results stay
  paired; final assistant text and successful outcome commit atomically.
- No personal chat storage, actual model, native chat UI, live connectors or S3
  mutation integration is enabled. Native fixture responses batch events; live
  streaming/stop transport, encrypted vault and Expert registries remain next work.
- [Validation, reproduction and exact boundaries](docs/validation/s4-agent-foundation.md).
- Validation: 74 Rust tests and 104 Flutter tests pass, including 24 new Rust
  Agent/core/ABI tests and three Dart tests. Flutter analyzer, Agent Clippy and
  formatting checks pass; whole-workspace Clippy retains two existing Calendar warnings.

### Decision-first UI checkpoint — 2026-09-06

- Follow-up removes timezone/UTC/offset fields from user input, review and event
  detail surfaces. Planning accepts device-local date/time with consistent field
  spacing; UTC and fixed-offset scheduling metadata remain internal boundaries.
- Parallel Select/Dropdown prototype and action-review simplification are integrated.
  Shared controls support keyboard/typeahead, focus recovery and restrained motion;
  the review prioritizes the user's decision and collapses technical metadata.
- Recorded the product-wide abstraction principle; applied it in prototype and
  Flutter review without changing execution authority or the write-disabled gate.
- 48 component contracts, 26 action assertions, 13 selection navigation assertions,
  11 source guards, production prototype build and 72 Flutter tests pass; analyze clean.
- Browser checks cover desktop/390/320 layouts, Select/Dropdown keyboard flow,
  diagnostic disclosure, rejection and lookup/read-only recovery simulations.
- [Evidence and remaining accessibility checks](docs/validation/decision-first-ui.md).
  S3 remains 2/5 verified; this presentation pass is not new live acceptance evidence.

### S3 native execution checkpoint — 2026-09-06

- After user-granted Calendar permission, the signed Flutter app completed explicit
  approval → native create → MethodChannel read/import → Day Canvas. Relaunch and
  read retries retained one event; exact cleanup removed it without changing the
  successful ledger or creating a replacement. S3-A3/A4 verified (2/5), not Accepted.
- Existing 11 calendar selections stayed unchanged. At this checkpoint, both
  Debug and Release were restored to write-disabled builds; ad-hoc rebuilds may
  require a fresh OS grant. The current rollout policy enables the executor in all
  builds.
- Connected proposal preparation, explicit approval, native execution, lookup-only
  recovery and separate Calendar read retry.
- Live disposable create through the actual Rust/native adapter survived injected
  response loss and process restart; duplicate create was blocked. Exact cleanup
  and absence verification passed. Existing calendar selections were preserved.
- Fixed legacy mirror CAS compatibility (`8c068da`); diagnosed ad-hoc signing/TCC
  mismatch and began user-authorized Floe-only Calendar reauthorization.
- 46 Rust tests, 72 Flutter tests and nine native assertions pass.
- [Implementation, evidence and remaining gates](docs/validation/s3-native-executor.md).

### S3 native review UI checkpoint — 2026-09-06

- Today loads saved proposals, opens immutable review and records approve/reject
  through Rust. No proposal producer or live write is enabled.
- Persistent visible outcomes, guarded approval, in-flight/response-loss handling
  and read-only reload; narrow/desktop and keyboard widget coverage.
- [Behavior and validation boundaries](docs/validation/s3-action-review-ui.md).
  S3 remains 0/5; native execution and S1/live gates remain pending.

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
the gateway after removal of the focus-time experiment. This does not advance S8
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
2. Complete signed-app S3 approval/create/collection and provider-failure validation.
   Prototype review, Flutter UI and native Rust binding are implemented; runtime
   authority and fresh safety checks remain mandatory.
3. Start S4 with text chat, durable sessions, the bounded Agent loop and shared
   Tool/Expert/Connector registries; validate Codex authentication, a device-local
   Foundation Model/sLLM and sensitive routing; add Gmail, Contacts, location/ETA/weather
   plus physical-device Screen Time and Apple Health gates before connecting Expert output to S3.
4. Follow with S5 source-backed Memory and governed Playbook learning,
   then S6 press-to-talk voice and S7 local wake-up. Do not begin S8 server/sync
   as a shortcut.
5. Evaluate an officially supported OAuth adapter and Apple native availability separately; do not assume CLIProxyAPI adoption.

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
