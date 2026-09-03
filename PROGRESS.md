# Floe Progress

> Last updated: 2026-09-03  
> Purpose: 구현 진행현황만 추적한다. 제품 정의와 기술 설계는 `docs/planning/` 및 ADR을 따른다.

## Status Summary

| Scope | Progress | Status |
| --- | ---: | --- |
| ADR 0004 — First Personal Day Slice | 75–80% | In progress |
| MVP Engineering | 55–60% | In progress |
| Phase 0 — Architecture PoCs | 10–15% | In progress |
| Phase 1 — Personal Day | 35–45% | In progress |
| Phase 2+ | <5% | Not started |
| Full roadmap | <10% | In progress |

Percentages are directional estimates. A scope is complete only when its
acceptance criteria and required dogfood validation are complete.

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
- [x] Squircle-first Floe design system, responsive shell, and interaction baseline
- [ ] Event, Task, and Note editing UI
- [ ] Explicit conflict recovery UI
- [ ] Dense-day folding and complete timeline treatment
- [ ] Calendar integration
- [ ] Two-week Day Canvas dogfood

## MVP Areas

| Area | Progress | Completed | Remaining |
| --- | ---: | --- | --- |
| Day Canvas | ~70% | Now/Next, unified local projection, empty and overdue states | Conflict UX, editing, folding, interventions |
| Universal Capture | ~60% | Typed capture, original input, explicit classification, provenance | Voice/STT, correction flow, semantic candidates |
| Local Personal-Day Store | 80–85% | Rust-owned Turso, CRUD core, deterministic snapshots, reopen persistence | Encryption, export/forget, sync, multi-person management |
| Minimal Assistant | 0% | — | Manager, contextual suggestions, Action Proposal/Policy execution |

## Roadmap Status

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

- Rust: 15 tests passing
- Flutter: 11 tests passing
- Rust workspace: Clippy clean with warnings denied
- Flutter: analyzer clean
- macOS: signed release app contains the expected six C ABI symbols
- Native persistence: Event, Task, and Note lifecycle survives app/core restart

## Next Priorities

1. Add Event, Task, and Note edit flows.
2. Add typed conflict recovery and refresh behavior.
3. Implement the first read-only Calendar integration.
4. Add dense-day timeline and folding behavior.
5. Start structured two-week Day Canvas dogfood.

## Update Rules

- Update this file in the same commit as a milestone status change.
- Record only delivered, validated, in-progress, or blocked work.
- Do not duplicate product requirements or architecture decisions here.
- Do not mark roadmap work complete based only on scaffolding.
- Update validation counts whenever tests are added or removed.
