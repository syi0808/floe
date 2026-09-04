# Floe Progress

> Last updated: 2026-09-04
>
> Purpose: 구현 진행현황만 추적한다. 제품 정의와 기술 설계는 `docs/planning/` 및 ADR을 따른다.

## Delivery Board

Delivery follows [ADR 0006](docs/decisions/0006-slice-driven-delivery.md) and the
[slice acceptance plan](docs/planning/08-engineering/vertical-slice-delivery.md).
Phase order is no longer an implementation gate. Acceptance counts below report
verified criteria, not estimated implementation percentages.

| Slice | Status | Integration evidence | Acceptance | Blocker / prerequisite | Next demo |
| --- | --- | --- | --- | --- | --- |
| S1 — Calendar read | Implementing | Fixture end-to-end; EventKit app builds | 0/4 | Live permission/read/create PoC and dogfood | Connect a dedicated macOS Calendar |
| S2 — Contextual suggestion | Planned | None | 0/4 | S1 Verified; model provider selection | Source-backed focus-time suggestion |
| S3 — Approved action | Planned | None | 0/5 | S2 Verified; create capability | Approve, create externally, re-import |
| S4 — Cross-device/server | Planned | None | 0/4 | S3 Accepted; sync/security PoCs | Same result on two devices |
| S5 — Intervention | Planned | None | 0/4 | S4 Accepted; resident lifecycle | Calendar change triggers controlled suggestion |

S1 implementation now connects a native EventKit adapter to Rust-owned mirror
storage and Day Canvas. No live acceptance criterion is marked verified yet.

## Acceptance Evidence

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

## Existing Personal Day Baseline

ADR 0004 remains partially delivered, not accepted. Existing delivered work is
preserved below. Non-blocking local UI breadth is deferred while S1–S3 are
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
| Minimal Assistant | Not started | — | Manager, contextual suggestions, Action Proposal/Policy execution |

## Roadmap Status

These are delivered capability statuses, not sequential work gates. Planned
cross-phase coverage is defined in the slice plan and does not change these statuses.

| Phase | Status | Notes |
| --- | --- | --- |
| Phase 0 — Architecture PoCs | Partial | macOS embedded Turso and Flutter↔Rust boundary validated; other PoCs remain |
| Phase 1 — Personal Day | Partial | Technical vertical slice works; product breadth and dogfood remain |
| Phase 2 — Connected Floe | Partial | S1 EventKit read path implemented; live validation and other connectors remain |
| Phase 3 — Personal Memory | Not started | Memory, people, relationships, and identity resolution remain |
| Phase 3.5 — Expert Ecosystem | Not started | Package, permissions, sandbox, SDK, and marketplace remain |
| Phase 4 — Cross-device | Not started | Sync, Device Agent, and native packaging remain |
| Phase 5 — Ambient Floe | Not started | Wake word, transcription, handoff, and interventions remain |
| Phase 6 — Hosted/Self-host | Not started | Server, accounts, deployment, and administration remain |

## S1 Automated Validation

2026-09-04, macOS arm64. Dependency mode: fixture for connector behavior;
EventKit native adapter compilation only, not a live read.

- Rust: 20 tests passing (`cargo test --workspace`).
- Flutter: 38 tests passing, including actual native FFI tests (none skipped).
- Clippy: clean with warnings denied; Flutter analyzer: clean.
- macOS: debug app builds with Calendar entitlement and EventKit usage strings.
- No external calendar data collected or changed; live acceptance remains 0/4.

## Previous Validation Baseline

Last recorded baseline: 2026-09-03. These results are preserved from the previous
progress record, not rerun or newly verified by the 2026-09-04 planning change.

- Rust: 15 tests passing
- Flutter: 12 tests passing
- Rust workspace: Clippy clean with warnings denied
- Flutter: analyzer clean
- macOS: signed release app contains the expected six C ABI symbols
- Native persistence: Event, Task, and Note lifecycle survives app/core restart

## Next Priorities

1. Run the dedicated-calendar live permission/read checklist in `docs/validation/s1-calendar.md`.
2. Validate recurring-event identity, timezone boundaries, edits/deletions, and signed-build restart/recovery.
3. With separate explicit approval, complete the bounded S3 create-capability PoC.
4. Record S1-A1–A4 evidence and three-day dogfood before advancing acceptance.

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
