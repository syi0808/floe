# ADR 0006: Adopt a calendar-first Day Canvas

- **Date:** 2026-09-03
- **Status:** accepted product/UX direction; implementation pending
- **Supersedes:** the presentation interpretation of ADR 0004 where `Now / Next` and a generic Event/Task/Note list were sufficient to represent the target Day Canvas
- **Does not supersede:** ADR 0004 domain separation, provenance, typed commands, deterministic projection, Turso persistence, or ADR 0005 Flutter ↔ Rust boundary

## Context

The first Personal Day vertical slice successfully validated important technical boundaries:

- Event, Task, Note, and Capture remain separate domain types;
- canonical mutations are owned by Rust;
- embedded Turso persistence works on macOS;
- Flutter consumes a coarse-grained DaySnapshot through a versioned JSON/C ABI;
- revision-aware mutations and capture provenance are present.

However, the implemented presentation interpreted the original `Now / Next` principle as a large hero panel followed by an equal-weight list of Event, Task, and Note rows.

That implementation behaves more like a daily summary/agenda dashboard than the intended product.

The intended Floe experience is closer to a simple, familiar calendar that quietly incorporates Todo, Notes, and assistant intelligence. Calendar should be the visual foundation because time is the common coordinate where schedules, workload, health-aware planning, movement, commitments, and assistant interventions meet.

## Decision

Adopt the following controlling UX rule:

> **Calendar is the canvas. Tasks, notes, and Floe are layers on top of it.**

### Calendar-first primary surface

The default Personal Day view uses a familiar vertical day-calendar time grid.

It includes:

- date navigation;
- all-day region;
- hour/sub-hour grid;
- current-time indicator;
- timed Event geometry;
- overlap handling;
- scroll/navigation toward the current time.

Day view is the initial primary home surface. Week view is a natural later extension. Full Month-view parity is not required for the Personal Day MVP.

### Now / Next

`Now / Next` remains an important information principle but is no longer a permanent hero section.

- **Now** is primarily expressed by the current-time line and subtle emphasis of the Event containing the current instant.
- **Next** is normally visible in the calendar. If the next Event is outside the viewport, Floe may show a compact sticky navigation affordance.

The calendar should answer these questions directly instead of repeating them in a large summary card.

### Event, Task, and Note are visually unequal by role

Their domain semantics remain independent and their visual presentation reflects those roles.

#### Event

Events form the main temporal geometry and occupy time-space according to duration.

All-day Events use a dedicated all-day region.

#### Task

Tasks are separated conceptually into:

- **scheduled Tasks** — Tasks with an explicit planned execution allocation; they may appear lightly in the time grid;
- **unscheduled Today Tasks** — Tasks intended for today without a specific execution time; they live in a compact secondary surface such as a Today rail.

Task deadline and scheduled execution time are distinct semantics. The current `deadline` field must not silently become the scheduling field.

#### Note

Notes are quiet context rather than equal-weight calendar rows.

Initial forms:

- Today Note in a secondary context surface;
- contextual annotation linked to an Event/Task/Person in a later slice.

### Floe intervention

The home screen does not gain a permanent AI/Insight/Expert dashboard.

Floe output should normally attach to the time or object it affects.

Examples:

- a proposed break shown as a ghost block in a free time range;
- a proposed Event move shown at the suggested destination;
- a contextual annotation attached to a Task/Event.

Proposals are visibly reversible and remain non-canonical until accepted through the normal Action Authority boundary.

### Desktop composition

The dominant region is a flexible calendar grid.

An optional compact Today rail may contain:

- unscheduled Today Tasks;
- short Today Notes.

The rail is subordinate to the calendar and must not become a dashboard or plugin/widget column.

### Mobile composition

Mobile preserves the calendar/time mental model but does not shrink the desktop layout literally.

Tasks and Notes may move into bottom sheets or other secondary surfaces while the day timeline remains primary.

### Universal Capture

The current explicit post-capture Event/Task/Note classification remains useful as an early trust-building PoC, but it is not the final interaction contract.

The mature flow must allow a user to capture first and return to their previous context. Structure may be:

- explicit immediately;
- suggested by Floe;
- deferred until later.

Pending Captures require a discoverable recovery/review path.

## Design consequences

`DESIGN.md` now treats the calendar grid as the primary visual structure.

- `hero-panel` is reserved for onboarding, empty/promotional, and rare focused surfaces; it is not a recurring Day Canvas header.
- Routine Day Canvas should not sit inside a large atmospheric gradient.
- Calendar-source colors may appear as restrained accent/tint rather than full saturated blocks.
- Empty calendar time is meaningful and should remain visually quiet.
- Expert extensibility does not authorize arbitrary dashboard UI in Day Canvas.

## Domain consequences

The existing Rust domain split remains correct and is intentionally preserved.

```text
Event
Task
Note
Capture
   ↓
Calendar-first Day Projection
```

The redesign should be implemented mainly through projection/presentation evolution, not by collapsing domain objects into a generic calendar record.

Before real calendar import, implementation should revisit:

1. IANA timezone identity instead of fixed UTC offsets for canonical Event semantics;
2. external source/provenance identity;
3. range-queryable/indexed Turso fields for calendar projection;
4. storage-level atomic revision compare-and-swap;
5. all-day and recurrence semantics;
6. duplicate source-route suppression.

## Validation

Dogfood the calendar-first view with real calendar data for at least two weeks.

Evaluate:

- current-time and schedule comprehension;
- dense-day readability;
- usefulness of the Today task surface;
- whether Notes remain available without visual noise;
- whether the user opens a separate calendar/task/note app less often;
- capture friction and Pending Capture recovery;
- unwanted Floe proposal frequency;
- calendar timezone/source projection errors.

## Revisit when

Revisit after real calendar dogfood, especially if:

- a calendar grid proves too visually dense for the target ADHD-friendly experience;
- scheduled Tasks repeatedly conflict with Event readability;
- the Today rail becomes a dumping ground or dashboard;
- mobile requires a materially different primary information architecture.

A revisit should preserve the core product question—how to make a person's day immediately understandable—rather than returning automatically to a summary dashboard.

## References

- `DESIGN.md`
- `docs/product-brief.md`
- `docs/mvp.md`
- `docs/planning/00-overview/product-vision.md`
- `docs/planning/00-overview/product-principles.md`
- `docs/planning/01-experience/day-canvas.md`
- ADR 0004 — first Personal Day vertical slice
- ADR 0005 — Flutter ↔ Rust JSON/C ABI bridge
