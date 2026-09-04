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
| S1 — Calendar read | Planned | None | 0/4 | Select provider; validate read/create capabilities | External events in Day Canvas |
| S2 — Contextual suggestion | Planned | None | 0/4 | S1 Verified; model provider selection | Source-backed focus-time suggestion |
| S3 — Approved action | Planned | None | 0/5 | S2 Verified; create capability | Approve, create externally, re-import |
| S4 — Cross-device/server | Planned | None | 0/4 | S3 Accepted; sync/security PoCs | Same result on two devices |
| S5 — Intervention | Planned | None | 0/4 | S4 Accepted; resident lifecycle | Calendar change triggers controlled suggestion |

No connected slice is implemented or verified by this documentation change.
Next implementation focus is S1; no connected slice is currently Implementing.

## Acceptance Evidence

No S1–S5 acceptance evidence has been recorded yet. On validation, add a row per
criterion with result, integration mode per dependency, test/demo evidence,
verified commit, date/environment, and limitations using the delivery plan format.
Fixture-only evidence cannot satisfy the live integration acceptance gate.

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
| Phase 2 — Connected Floe | Not started | Connector framework and external sources remain |
| Phase 3 — Personal Memory | Not started | Memory, people, relationships, and identity resolution remain |
| Phase 3.5 — Expert Ecosystem | Not started | Package, permissions, sandbox, SDK, and marketplace remain |
| Phase 4 — Cross-device | Not started | Sync, Device Agent, and native packaging remain |
| Phase 5 — Ambient Floe | Not started | Wake word, transcription, handoff, and interventions remain |
| Phase 6 — Hosted/Self-host | Not started | Server, accounts, deployment, and administration remain |

## Validation Baseline

Last recorded baseline: 2026-09-03. These results are preserved from the previous
progress record, not rerun or newly verified by the 2026-09-04 planning change.

- Rust: 15 tests passing
- Flutter: 12 tests passing
- Rust workspace: Clippy clean with warnings denied
- Flutter: analyzer clean
- macOS: signed release app contains the expected six C ABI symbols
- Native persistence: Event, Task, and Note lifecycle survives app/core restart

## Next Priorities

1. Start S1 by selecting a Calendar provider through a bounded read/create PoC.
2. Connect a fixture Calendar source through Rust storage/snapshot to Flutter.
3. Replace the fixture with the real read-only provider and verify S1-A1–A4.
4. Record the live demo and S1 dogfood evidence, then proceed to S2 and S3.

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
