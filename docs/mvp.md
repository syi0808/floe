# MVP: Personal Day

**Status:** proposed scope, aligned with planning v0.5
**Last updated:** 2026-09-02

## Outcome to validate

Can a unified Calendar + Todo + Notes experience, presented as Floe’s Day Canvas, help a real user run their day better than the separate tools they already use?

## Product scope

### 1. Day Canvas

- A single, calm view with a clear **Now** and **Next** horizon.
- Timeline projections for events, tasks, and captured notes.
- Explicit empty, conflict, and overdue states.
- No productivity score, streak, badge, or dense analytical dashboard.

### 2. Universal Capture

- One low-friction typed capture action.
- Basic voice capture where the initial platform supports it; wake word and always-listening behavior remain separate PoCs.
- Preserve original input, timestamp, source, and processing result.
- Let the user classify or correct a capture as a task, note, or event candidate.

### 3. Local personal-day store

- Model events, tasks, notes, and capture provenance separately from the Day Canvas projection.
- Support create, edit, complete, delete, and deterministic ordering.
- Use the Rust Core → embedded Turso boundary for local persistence; do not couple the Flutter UI directly to the database.

### 4. Minimal assistant behavior

- Contextual answers and suggestions derived from local Personal Day data.
- A basic Manager may aggregate internal advice, but suggestions remain non-mutating until the user confirms.
- Every external action follows Action Proposal → Policy → validation → deterministic execution.

## Initial implementation baseline

- **First client:** macOS, while preserving an architecture that treats iOS, Android, and Windows as first-class.
- **Main UI:** Flutter/Dart.
- **Shared local core:** Rust.
- **macOS integration:** Swift or Rust, chosen per native API.
- **Persistence:** embedded Turso behind the Rust Core.

This is a recommended baseline, not a substitute for the Phase 0 feasibility checks.

## Explicitly out of scope

- Ambient wake word and always-listening audio.
- Health ingestion or health advice.
- Gmail, contacts, provider connectors, and Activepieces.
- Durable relationship-memory extraction.
- Autonomous actions, background workflows, and multi-agent surfaces.
- Cross-device sync, hosted accounts, and self-hosting deployment.

## Acceptance signals

Dogfood the product for two weeks and measure:

- capture-to-retrieval success;
- daily use of Now/Next;
- manually reported moments of reduced context switching;
- incorrect projections, missed commitments, and unwanted suggestions;
- whether the user prefers Floe’s daily view to separate calendar/task/note views.

The MVP advances only if users return to it voluntarily and the Day Canvas improves daily orientation without increasing interruption or data-entry burden.

## Evidence

See `docs/planning/00-overview/roadmap.md`, `01-experience/day-canvas.md`, `01-experience/capture-and-transcription.md`, `03-intelligence/skills-and-actions.md`, `08-engineering/poc-plan.md`, and `09-implementation/technology-selection.md`.