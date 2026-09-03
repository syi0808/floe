# MVP: Personal Day

**Status:** proposed scope, aligned with current planning  
**Last updated:** 2026-09-03

## Outcome to validate

Can a **calendar-first Calendar + Todo + Notes experience**, presented as Floe’s Day Canvas, help a real user run their day better than the separate tools they already use?

The MVP must validate the product interaction model, not merely prove that Event/Task/Note CRUD works.

## Product scope

### 1. Calendar-first Day Canvas

- The primary surface is a familiar single-day calendar time grid.
- Timed Events occupy temporal space rather than appearing only as generic list rows.
- A clear current-time line makes `Now` immediately understandable.
- The current Event may receive subtle emphasis.
- The next Event is normally discovered in the grid; a compact navigation affordance may surface it when it is outside the viewport.
- A dedicated all-day region represents all-day Events.
- No large permanent `Now / Next` hero panel is required.

### 2. Tasks integrated by role

- A Task planned for a specific execution time may appear lightly in the time grid.
- A Task intended for today but not assigned a time remains in a compact Today task surface.
- Deadline and planned execution time are distinct semantics.
- Completed and secondary Tasks recede before calendar readability is sacrificed.

The exact scheduled-Task domain schema may be added after the visual/product PoC, but the UI must not treat every Task as an Event or use deadline as an implicit scheduled time.

### 3. Notes as quiet context

- Today Notes remain quickly accessible without taking equal visual weight with timed Events.
- Notes may later attach to Event/Task/Person context.
- The MVP may begin with a compact Today Notes surface while preserving the domain boundary for contextual notes.

### 4. Universal Capture

- One low-friction typed capture action.
- Basic voice capture where the initial platform supports it; wake word and always-listening behavior remain separate PoCs.
- Preserve original input, timestamp, source, and processing result.
- The current explicit Event/Task/Note classification flow remains valid as an early trust and canonical-data PoC.
- The product must also define a recovery path for Pending Capture so `later` does not mean lost.
- Long-term UX should allow immediate classification, Floe-assisted classification, or deferred organization without forcing a blocking metadata workflow after every thought.

### 5. Local personal-day store

- Model Events, Tasks, Notes, and Capture provenance separately from the Day Canvas projection.
- Support create, edit, complete, delete, and deterministic ordering.
- Use the Rust Core → embedded Turso boundary for local persistence; do not couple the Flutter UI directly to the database.
- Before large Calendar mirrors are imported, storage should expose/index the fields required for day-range queries rather than requiring full-Person JSON deserialization for every Day Canvas load.
- Revision-aware writes must evolve toward storage-level atomic compare-and-swap semantics before multiple mutation sources are introduced.

### 6. Read-only Calendar integration

Calendar integration is the first connector required to validate the intended product rather than just local CRUD.

The first integration should:

- import at least one real user calendar into the Day Canvas;
- preserve timed vs all-day semantics;
- preserve source/provenance identity;
- use an IANA timezone model suitable for DST-aware calendars;
- avoid duplicate canonical ingestion when the same provider calendar is visible through multiple routes.

The first implementation may be macOS device-native and read-only.

### 7. Minimal assistant behavior

- Contextual answers and suggestions derived from local Personal Day data.
- A basic Manager may aggregate internal advice, but suggestions remain non-mutating until the user confirms.
- Floe suggestions should attach to the time/object they affect rather than live in a permanent AI dashboard.
- Every external action follows Action Proposal → Policy → validation → deterministic execution.

Minimal assistant behavior is not required before the calendar-first Day Canvas can be dogfooded, but it is part of the broader Personal Day MVP.

## Initial implementation baseline

- **First client:** macOS, while preserving an architecture that treats iOS, Android, and Windows as first-class.
- **Main UI:** Flutter/Dart.
- **Shared local core:** Rust.
- **macOS integration:** Swift or Rust, chosen per native API.
- **Persistence:** embedded Turso behind the Rust Core.

This is a recommended baseline, not a substitute for Phase 0 feasibility checks.

## Explicitly out of scope

- Ambient wake word and always-listening audio.
- Health ingestion or health advice.
- Gmail and full communication processing.
- Durable relationship-memory extraction.
- Expert Marketplace/runtime implementation.
- Autonomous actions and background workflows.
- Cross-device sync, hosted accounts, and self-hosting deployment.
- Full Month-view calendar replacement.

## Acceptance signals

Dogfood the product for at least two weeks and measure:

- whether the user can identify current time, current/next schedule, and major free windows without another calendar app;
- whether Today Tasks are visible without overwhelming the calendar;
- whether short Notes are accessible without turning the home screen into a feed;
- capture-to-retrieval success;
- Pending Capture recovery success;
- manually reported moments of reduced context switching;
- incorrect calendar projections, timezone errors, missed commitments, and unwanted suggestions;
- dense-day readability;
- whether the user voluntarily prefers Floe’s daily view to opening separate calendar/task/note views.

The MVP advances only if users return to it voluntarily and the Day Canvas improves daily orientation without increasing interruption or data-entry burden.

## UX failure conditions

The MVP should be considered directionally wrong if:

- the screen reads primarily as a dashboard rather than a calendar;
- `Now / Next` duplicates obvious calendar information in large permanent surfaces;
- Event, Task, and Note become equal-weight generic rows;
- adding Tasks/Notes makes busy calendar days materially harder to scan;
- Floe requires too much immediate classification or metadata entry for quick capture;
- assistant output demands its own permanent home-screen region.

## Evidence

See `docs/planning/00-overview/product-vision.md`, `01-experience/day-canvas.md`, `01-experience/capture-and-transcription.md`, `03-intelligence/skills-and-actions.md`, `08-engineering/poc-plan.md`, `09-implementation/technology-selection.md`, and root `DESIGN.md`.
