# MVP: Personal Day

**Status:** proposed scope  
**Last updated:** 2026-09-02

## Outcome to validate

Can a unified Calendar + Todo + Notes experience, presented as Floe’s Day Canvas, help a real user run their day better than the separate tools they already use?

## In scope

### 1. Day Canvas

- A single, calm view with a clear **Now** and **Next** horizon.
- Timeline projections for events, tasks, and captured notes.
- Explicit empty, conflict, and overdue states.
- No productivity score, streak, badge, or dense analytical dashboard.

### 2. Universal Capture

- One low-friction capture action for typed input first.
- Preserve the original input, timestamp, source, and processing result.
- Let the user classify or correct a capture as a task, note, or event candidate.
- Voice capture remains a separately validated follow-up, not an MVP dependency.

### 3. Local personal day store

- Model events, tasks, notes, and capture provenance separately from UI projection.
- Support create, edit, complete, delete, and deterministic ordering.
- Start local-first and define deletion behavior before sync or remote providers.

### 4. Minimal assistant behavior

- Contextual answers and suggestions derived from the local Personal Day data.
- Suggestions remain non-mutating until the user confirms.
- Every proposed action shows what will change.

## Explicitly out of scope

- Wake word and always-listening audio.
- Health ingestion or health advice.
- Gmail, contacts, provider connectors, and Activepieces.
- Durable relationship-memory extraction.
- Autonomous actions, background workflows, and multi-agent surfaces.
- Cross-device sync, hosted accounts, and self-hosting deployment.
- iOS, Android, and Windows clients; macOS is the first client target after the domain slice is usable.

## Acceptance signals

Dogfood the product for two weeks and measure:

- capture-to-retrieval success;
- daily use of Now/Next;
- manually reported moments of reduced context switching;
- incorrect projections, missed commitments, and unwanted suggestions;
- whether the user prefers Floe’s daily view to the separate calendar/task/note views.

The MVP advances only if users return to it voluntarily and the Day Canvas improves daily orientation without increasing interruption or data-entry burden.

## Next implementation slice

Before choosing a UI stack, create a small, tested domain slice with Event, Task, Note, Capture, TimelineProjection, and ActionProposal types; then render that slice in a minimal Day Canvas.

## Evidence

Derived from `00-overview/roadmap.md`, `01-experience/day-canvas.md`, `01-experience/capture-and-transcription.md`, `03-intelligence/skills-and-actions.md`, and `08-engineering/poc-plan.md` in the supplied planning bundle.
