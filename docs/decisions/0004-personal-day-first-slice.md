# ADR 0004: Define the first Personal Day vertical slice

- **Date:** 2026-09-02
- **Status:** accepted

## Context

Floe needs a first implementation slice that validates the Personal Day product boundary and the Flutter ↔ Rust ↔ Turso architecture without pulling connector, model, voice, sync, or ambient-platform risks into the same milestone.

The planning documents leave the initial input boundary, Now/Next policy, domain command and snapshot contract, and local storage topology open. The repository also does not currently have a Flutter SDK available. This absence is an environment constraint, not a change to the accepted Flutter client baseline.

## Decision

Implement a local-first, macOS-oriented Personal Day slice with this flow:

```text
manual typed capture
  → explicit user classification
  → Rust typed command
  → separate domain mutation
  → one logical Turso database per Person
  → batched Day Canvas snapshot
```

### Input and classification

- The first input boundary is manual typed capture only.
- A capture preserves its original text, capture time, source, and processing result.
- The user explicitly classifies a capture as an Event, Task, or Note and may correct that choice.
- No heuristic or model silently promotes a capture into canonical domain data.

### Domain and storage

- `Capture`, `Event`, `Task`, and `Note` remain separate domain types and persistence models.
- A classified object retains provenance back to its source `Capture`.
- Canonical mutations pass through Rust-owned typed commands. Flutter or any future UI must not write to the database directly.
- The storage PoC uses one logical embedded Turso database per Person, with Floe-owned schema migrations.
- Storage is accessed through a Rust abstraction so the domain and projection logic can be tested independently of Turso.

### Client contract

- The core returns a coarse-grained, immutable, batched Day Canvas snapshot rather than serving per-widget or per-row queries.
- The contract covers capture submission and classification; Event, Task, and Note creation and editing; task completion; deletion; and Day Canvas retrieval.
- Flutter remains the accepted visual client. Flutter implementation is deferred only because the SDK is absent from the current environment; this decision does not authorize a replacement UI stack.

### Initial Now/Next policy

The first dogfood policy is deliberately deterministic and replaceable:

- **Now:** an Event whose interval contains the supplied current instant. If intervals overlap, choose by earliest start time and then stable ID.
- **Next:** the earliest Event starting after the supplied current instant, with stable ID as the tie-breaker.
- **Overdue:** an incomplete Task whose deadline is before the supplied current instant.
- Remaining projected items use an explicit effective-time, domain-kind, creation-time, and stable-ID ordering.
- Projection receives the clock, date, and timezone explicitly so tests do not depend on wall-clock state.

This policy is an initial dogfood hypothesis, not a permanent personalization rule.

## In scope

- Rust domain types, validation, typed commands, and Day Canvas projection.
- Manual capture and explicit Event, Task, or Note classification.
- Create, edit, task complete, delete, and deterministic retrieval behavior.
- Capture provenance and revision-aware domain records.
- Embedded Turso macOS feasibility, initial migration, and one logical database per Person.
- Core unit and integration tests, including restart persistence.
- A stable batched snapshot boundary ready for a Flutter client.

## Out of scope

- Read-only or writable calendar import and all other connectors.
- Voice capture, transcription, wake word, and always-listening behavior.
- Automatic classification, LLM interpretation, Manager behavior, and Personal Memory extraction.
- Action Proposal execution, external mutations, and remembered confirmation preferences.
- Device Agent IPC, resident background processes, and native platform bridges.
- Cross-device sync, hosted accounts, self-hosting, encryption topology, and conflict resolution.
- iOS, Android, and Windows builds.
- Replacing Flutter because it is unavailable in the current development environment.

## Acceptance criteria

The slice is accepted when:

1. A typed capture can be stored and explicitly classified as an Event, Task, or Note while retaining its original text, timestamp, source, processing result, and provenance link.
2. Event, Task, Note, and Capture use distinct domain and persistence representations; the Day Canvas unifies only their projection.
3. Create, edit, task completion, delete, and retrieval execute through Rust typed commands and survive closing and reopening the local database.
4. Event validation rejects invalid intervals, required text fields reject empty values, and command failures return typed errors without partial writes.
5. Given a fixed clock and timezone, Now, Next, overdue state, empty state, and complete item ordering are reproducible across runs, including tie cases.
6. One command can produce one updated batched snapshot without row-by-row UI/database calls.
7. The embedded Turso PoC opens, migrates, reads, and writes a Person-scoped database on macOS. A Turso feasibility failure is recorded explicitly rather than silently changing the persistence decision to plain SQLite.
8. Rust unit and integration tests cover validation, provenance, command behavior, migration, persistence restart, and deterministic projection.
9. The snapshot and command boundary is documented in code well enough for the deferred Flutter client to consume without duplicating domain logic in Dart.

## Consequences

- Domain, projection, and storage work can proceed now while Flutter UI work waits for the SDK.
- Manual classification provides trustworthy canonical data and a usable capture path without making model quality a prerequisite.
- The slice validates the highest-value Personal Day path while also exercising the P0 Turso boundary.
- Calendar usefulness and the full Calendar + Todo + Notes product hypothesis remain only partially validated until import or native calendar entry is added.
- Action authority is not exercised by this slice because all included mutations are explicit user-authored local commands.

## Revisit when

Revisit this decision after the first macOS dogfood cycle, when the Flutter SDK becomes available, or if the Turso PoC fails. Any change to the input boundary, Now/Next policy, UI stack, or persistence foundation should be supported by dogfood or PoC evidence.

## References

- `docs/mvp.md`
- `docs/open-questions.md`
- `docs/planning/00-overview/roadmap.md`
- `docs/planning/01-experience/day-canvas.md`
- `docs/planning/01-experience/capture-and-transcription.md`
- `docs/planning/02-domain/personal-timeline.md`
- `docs/planning/08-engineering/poc-plan.md`
- `docs/planning/09-implementation/client-architecture.md`
- `docs/planning/09-implementation/turso-storage.md`
