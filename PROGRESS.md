# Floe Progress

> Last updated: 2026-09-03  
> Purpose: 구현 진행현황만 추적한다. 제품 정의와 기술 설계는 `docs/planning/`, `DESIGN.md`, 및 ADR을 따른다.

## Status Summary

| Scope | Progress | Status |
| --- | ---: | --- |
| ADR 0004 — First Personal Day technical slice | 75–80% | In progress |
| ADR 0006 — Calendar-first Day Canvas | 0–10% | Design accepted, implementation pending |
| MVP Engineering | 50–55% | In progress |
| Phase 0 — Architecture PoCs | 10–15% | In progress |
| Phase 1 — Personal Day | 30–40% | In progress |
| Phase 2+ | <5% | Not started |
| Full roadmap | <10% | In progress |

Percentages are directional estimates. A scope is complete only when its acceptance criteria and required dogfood validation are complete.

The technical vertical slice is intentionally more complete than the target product UX. The current hero/list Day Canvas validates Flutter ↔ Rust ↔ Turso but is scheduled for a calendar-first presentation redesign under ADR 0006.

## Current Checkpoint

### Delivered technical foundation

- [x] Rust workspace and Personal Timeline domain baseline
- [x] Event, Task, Note, and Capture separation
- [x] Capture provenance and revision-aware mutations
- [x] Deterministic Day Snapshot with Now, Next, and overdue projection
- [x] Embedded Turso persistence and schema migration on macOS
- [x] Versioned JSON protocol and C ABI
- [x] Dedicated Dart FFI isolate and native handle lifecycle
- [x] Flutter client connected to Rust Core
- [x] Explicit Event, Task, and Note classification PoC
- [x] Task completion/reopen and item deletion
- [x] macOS dylib build, embedding, signing, and persistence verification
- [x] Floe design token and quiet interaction baseline

### Product/UX work now required

- [x] Calendar-first Day Canvas direction accepted in ADR 0006
- [ ] Replace large Now/Next hero with calendar current-time semantics
- [ ] Implement Day time grid and timed Event geometry
- [ ] Implement all-day Event region
- [ ] Define/implement compact Today Tasks surface
- [ ] Define/implement Today Notes/context treatment
- [ ] Add Pending Capture recovery/review path
- [ ] Event, Task, and Note editing UI
- [ ] Explicit conflict recovery UI
- [ ] Dense-day / overlap treatment
- [ ] First real read-only Calendar integration
- [ ] Two-week calendar-first Day Canvas dogfood

## MVP Areas

| Area | Progress | Completed | Remaining |
| --- | ---: | --- | --- |
| Calendar-first Day Canvas | ~25% | Connected Flutter shell, current projection primitives, design/ADR baseline | Time grid, Event geometry, current-time line, all-day, Today rail, dense-day behavior |
| Universal Capture | ~55% | Typed capture, original input, explicit classification, provenance | Pending recovery, non-blocking/deferred flow, voice/STT, semantic candidates |
| Local Personal-Day Store | 80–85% | Rust-owned Turso, CRUD core, deterministic snapshots, reopen persistence | Range-query fields/indexes, atomic CAS, encryption, export/forget, sync, multi-person management |
| Calendar Integration | 0% | Product/source boundary documented | native read-only connector, timezone/provenance, all-day/recurrence, route deduplication |
| Minimal Assistant | 0% | — | Manager, contextual suggestions, Action Proposal/Policy execution, calendar-attached proposal UX |

## Roadmap Status

| Phase | Status | Notes |
| --- | --- | --- |
| Phase 0 — Architecture PoCs | Partial | macOS embedded Turso and Flutter↔Rust boundary validated; other PoCs remain |
| Phase 1 — Personal Day | Partial | Technical vertical slice works; calendar-first product redesign and real Calendar dogfood now gate validation |
| Phase 2 — Connected Floe | Not started | Connector breadth and external communication/health sources remain |
| Phase 3 — Personal Memory | Not started | Memory, people, relationships, and identity resolution remain |
| Phase 3.5 — Expert Ecosystem | Not started | Package, permissions, sandbox, SDK, and marketplace remain |
| Phase 4 — Cross-device | Not started | Sync, Device Agent, and native packaging remain |
| Phase 5 — Ambient Floe | Not started | Wake word, transcription, handoff, and interventions remain |
| Phase 6 — Hosted/Self-host | Not started | Server, accounts, deployment, and administration remain |

## Validation Baseline

- Rust: 15 tests passing
- Flutter: 10 tests passing
- Rust workspace: Clippy clean with warnings denied
- Flutter: analyzer clean
- macOS: signed release app contains the expected six C ABI symbols
- Native persistence: Event, Task, and Note lifecycle survives app/core restart

These checks validate implementation boundaries, not the calendar-first product hypothesis.

## Next Priorities

1. Prototype the calendar-first Flutter Day Canvas using the existing Rust snapshot/domain boundary.
2. Add Pending Capture recovery so deferred classification cannot disappear from the UX.
3. Prepare Core/storage for real Calendar data: IANA timezone, external source provenance, range-query indexes, and storage-level CAS.
4. Implement the first read-only macOS Calendar integration.
5. Add Event/Task/Note edit and conflict recovery flows needed for daily dogfood.
6. Run structured two-week Day Canvas dogfood with real calendar data.

## Update Rules

- Update this file in the same commit as a milestone status change.
- Record only delivered, validated, in-progress, or blocked work.
- Do not duplicate detailed product requirements or architecture decisions here.
- Do not mark roadmap work complete based only on scaffolding.
- Technical PoC completion does not imply product UX acceptance.
- Update validation counts whenever tests are added or removed.
