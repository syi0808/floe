# Calendar-first Day Canvas — Implementation Update Specification

- **Status:** implementation-ready specification
- **Depends on:** ADR 0006, `DESIGN.md`, `docs/planning/01-experience/day-canvas.md`
- **Scope:** evolve the current Personal Day vertical slice into the calendar-first product direction without replacing the validated Flutter ↔ Rust ↔ Turso architecture

## 1. Goal

Update the current `Now / Next hero + generic list` presentation into a real calendar-first Day Canvas.

The controlling product rule is:

> **Calendar is the canvas. Tasks, notes, and Floe are layers on top of it.**

The implementation must preserve the already validated architecture:

```text
Flutter presentation
      ↓ typed/coarse commands + snapshots
Rust canonical domain/core
      ↓
Embedded Turso
```

This update is primarily a **projection, storage-query, protocol, and presentation redesign**. It is not a rewrite of the domain core.

---

# 2. Target Product Shape

Desktop Day Canvas:

```text
┌──────────────────────────────────────────────────────────────┐
│ ‹   9월 3일 목요일              오늘                        │
├─────────────────────────────────────────┬────────────────────┤
│ all day    프로젝트 마감                 │ 오늘 할 일          │
│                                         │                    │
│ 08 ──────────────────────────────────   │ □ PR 리뷰           │
│                                         │ □ 엄마에게 전화      │
│ 09       ┌────────────────────────┐     │                    │
│          │ 출근                    │     │ 메모                │
│ 10       └────────────────────────┘     │                    │
│                                         │ Connector 구조      │
│ 11 ──────────────────────────────────   │ 다시 보기           │
│                                         │                    │
│          □ 회의 자료 확인                │ + 메모              │
│ 12       ┌────────────────────────┐     │                    │
│          │ 점심                    │     │                    │
│ 13 ──────●──────────────────────────     │                    │
│          current time                   │                    │
│ 14       ┌────────────────────────┐     │                    │
│          │ Design Review           │     │                    │
│ 15       └────────────────────────┘     │                    │
│                                         │                    │
├─────────────────────────────────────────┴────────────────────┤
│ + 무엇이든 적기...                                            │
└──────────────────────────────────────────────────────────────┘
```

The time grid is dominant. The Today rail is secondary and disappears into a sheet/drawer on narrow layouts.

---

# 3. Product Semantics to Encode

## 3.1 Event

An Event occupies calendar time.

It may be:

- timed
- all-day
- local/manual
- imported from a calendar connector

Event visual weight is the strongest among Event / Task / Note.

## 3.2 Task

A Task has **two independent temporal concepts**:

```text
deadline
≠
scheduled execution time
```

A deadline answers:

> By when must this be done?

A schedule answers:

> When do I intend to work on it?

Therefore a Task may be:

```text
unscheduled + no deadline
unscheduled + deadline
scheduled + no deadline
scheduled + deadline
```

Scheduled Tasks appear inside the time grid as lighter/slimmer calendar objects.
Unscheduled Tasks appear in the Today rail.

## 3.3 Note

Notes are context, not calendar blocks by default.

For this implementation update:

- Notes created/assigned to the selected day appear in the Today rail.
- Event/Task-linked annotations are deferred until the association model is designed.
- Notes do not occupy time-grid height merely because they were created at a time.

## 3.4 Now / Next

`Now` and `Next` remain product semantics but are no longer permanent hero cards.

- `Now` = current-time line + emphasis on currently active timed items.
- `Next` = the next upcoming Event, exposed only as a compact navigation aid when useful.
- If multiple Events overlap the current instant, all are active; do not force a false single-current-event model.

---

# 4. Rust Domain Changes

## 4.1 Time Zone Identity

The existing fixed-offset-style timezone strings are insufficient for calendar data.

Introduce an explicit IANA timezone identifier type.

Conceptual API:

```rust
pub struct TimeZoneId(String);
```

Examples:

```text
Asia/Seoul
America/New_York
Europe/London
```

`TimedSchedule` continues to store UTC instants for ordering and comparison, while preserving the source/display timezone identity.

```rust
pub struct TimedSchedule {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub timezone: TimeZoneId,
}
```

Do not derive future recurring/local calendar behavior from a fixed `UTC+09:00` offset.

### Acceptance

- invalid/empty timezone IDs are rejected
- DST-zone roundtrip tests exist
- protocol transmits IANA IDs

---

## 4.2 Task Schedule

Extend Task with an optional execution schedule separate from deadline.

Recommended initial shape:

```rust
pub struct TaskSchedule {
    pub starts_at: DateTime<Utc>,
    pub duration_minutes: Option<u32>,
    pub timezone: TimeZoneId,
}

pub struct Task {
    // existing fields
    pub deadline: Option<DateTime<Utc>>,
    pub scheduled: Option<TaskSchedule>,
}
```

`duration_minutes = None` means a point/compact scheduled task. The UI gives it a minimum visual height but canonical data does not invent an end time.

### Rules

- deadline and scheduled time are independently editable
- completing a task does not erase scheduling metadata
- rescheduling increments revision
- scheduled task placement uses `scheduled.starts_at`, never `deadline`

---

## 4.3 External Source Provenance

Replace the current source model that only distinguishes `Manual` and `Capture` with a model capable of connector identity.

Recommended shape:

```rust
pub enum SourceRef {
    Manual,
    Capture(CaptureId),
    External(ExternalSourceRef),
}

pub struct ExternalSourceRef {
    pub connection_id: ConnectionId,
    pub resource_type: String,
    pub external_id: String,
    pub external_revision: Option<String>,
    pub provider: String,
}
```

The exact string/enums may evolve, but the following identity must be preserved:

```text
which connection
which external object
which provider/route
which external revision
```

This is required before real calendar mirroring.

### Important

Do not model connector origin as only:

```text
EventKit
Google
Microsoft
```

Two Google accounts and two calendars must remain distinguishable.

---

## 4.4 Imported Event Mutability

The first Calendar Connector is read-only.

Domain records must therefore expose whether an Event is currently writable through Floe.

Do not encode this as a permanent property of the Event type if it is connector-capability dependent. Prefer projection/capability resolution based on `SourceRef` / connection state.

---

# 5. Turso Storage Migration

The current storage intentionally stores domain payloads as JSON blobs. This was sufficient for the first vertical slice but is not sufficient for a calendar mirror.

## 5.1 Rule

Keep a lossless serialized payload if useful, but **materialize hot query/concurrency fields into indexed columns**.

## 5.2 Event table

Minimum queryable fields:

```text
id
person_id
starts_at
ends_at
all_day_start
all_day_end_exclusive
deleted_at
revision
source_kind
source_connection_id
source_external_id
source_external_revision
payload
```

Recommended indexes:

```text
(person_id, starts_at)
(person_id, ends_at)
(person_id, all_day_start)
(person_id, deleted_at)
(source_connection_id, source_external_id)
```

## 5.3 Task table

Materialize at least:

```text
id
person_id
deadline
scheduled_at
completed_at
deleted_at
revision
payload
```

Indexes:

```text
(person_id, scheduled_at)
(person_id, deadline)
(person_id, completed_at)
```

## 5.4 Note table

For the current Today-note model:

```text
id
person_id
created_at
deleted_at
revision
payload
```

Add explicit day assignment only when the product supports moving/pinning a Note to another day.

## 5.5 Range Queries

Replace `list all for Person → filter in Rust` with storage queries.

Required store APIs:

```rust
list_events_in_range(person_id, range)
list_tasks_for_day(person_id, day, timezone)
list_notes_for_day(person_id, day, timezone)
```

A Day Canvas load must not deserialize years of calendar history.

---

# 6. Atomic Revision Enforcement

The current read-check-write revision handling is acceptable for the single-isolate PoC but must be strengthened before connectors/sync introduce concurrent writers.

Storage mutation must perform compare-and-swap semantics.

Conceptually:

```sql
UPDATE ...
SET payload = ?, revision = revision + 1, ...
WHERE id = ? AND revision = ?;
```

If affected rows = 0:

- distinguish NotFound from Conflict if needed
- return typed `Conflict`
- never partially mutate related records

Apply this to:

- Event update/delete
- Task update/complete/reopen/delete
- Note update/delete
- capture classification state transition

Multi-record operations remain transactional.

---

# 7. Day Canvas Projection V2

The existing `DaySnapshot.items: Vec<TimelineItem>` encourages equal-weight list rendering.
Replace it with a presentation-oriented but domain-safe snapshot.

Recommended conceptual schema:

```rust
pub struct DayCanvasSnapshot {
    pub person_id: PersonId,
    pub date: NaiveDate,
    pub generated_at: DateTime<Utc>,
    pub timezone: TimeZoneId,

    pub all_day_events: Vec<EventProjection>,
    pub timed_items: Vec<TimedCanvasItem>,
    pub unscheduled_tasks: Vec<TaskProjection>,
    pub notes: Vec<NoteProjection>,

    pub active_event_ids: Vec<EventId>,
    pub next_event_id: Option<EventId>,
    pub overdue_task_count: usize,
    pub pending_capture_count: usize,
}

pub enum TimedCanvasItem {
    Event(EventProjection),
    ScheduledTask(TaskProjection),
}
```

## Projection ownership

Rust owns:

- date/time semantics
- filtering
- deterministic ordering
- active/next semantics
- scheduled vs unscheduled classification
- overdue semantics

Flutter owns:

- pixels
- lane widths
- overlapping-block visual layout
- scroll position
- responsive rail/sheet composition

Do **not** put pixel geometry into the Rust snapshot.

## Ordering

Within `timed_items`:

1. start time
2. Event before Task only as a deterministic tie-breaker, not visual importance
3. stable ID

`unscheduled_tasks` should have a deterministic product order, initially:

1. overdue first
2. deadline ascending
3. priority
4. creation time / stable ID

Dogfood may revise this policy.

---

# 8. Protocol / C ABI V2

Bump the JSON protocol schema version when `DayCanvasSnapshot` and Task schedule semantics change.

The app and native library ship together, so hard fail on unsupported protocol versions is acceptable.

Required DTO changes:

- IANA timezone IDs
- Task `scheduled`
- external `SourceRef`
- `DayCanvasSnapshot` sections
- active Event IDs
- pending capture count
- edit commands for Event / Task / Note

Keep JSON/C ABI for this control-plane boundary.

Do **not** optimize this into a binary protocol unless profiling shows a real problem.

### Guardrail

The JSON/C ABI is not the future transport for:

- microphone frames
- raw Health samples
- embedding arrays
- wake-word features
- high-rate model token/audio streams

Those use dedicated native/hot-path mechanisms later.

---

# 9. Flutter Presentation Rewrite

Remove the current application-level dependency on:

```text
_NowNext hero
_generic _Row list as the Day Canvas body
```

Replace the presentation composition with reusable pieces.

Recommended component boundaries:

```text
PersonalDayScreen
├─ DayToolbar
├─ DayCanvasLayout
│  ├─ CalendarDayGrid
│  │  ├─ AllDayLane
│  │  ├─ TimeAxis
│  │  ├─ TimedEventBlock
│  │  ├─ ScheduledTaskBlock
│  │  └─ CurrentTimeIndicator
│  └─ TodayRail
│     ├─ UnscheduledTaskList
│     └─ TodayNoteList
└─ UniversalCaptureBar
```

These names are recommendations, not mandatory public APIs.

---

# 10. Calendar Grid Behavior

## 10.1 Time Axis

Render a scrollable 24-hour day grid.

Recommended initial visual constant:

```text
hourExtent ≈ 60–72 logical px
```

Keep it configurable in one token/constant.

## 10.2 Initial Scroll

When viewing today:

```text
scroll to roughly now - 2 hours
```

When viewing another date:

```text
first meaningful timed item - 1 hour
```

If the day is empty, use a reasonable daytime default rather than midnight.

## 10.3 Current Time

For today only:

- render a thin current-time line
- use one restrained Floe accent
- update at a low-frequency timer; per-second updates are unnecessary
- active Event blocks receive subtle emphasis

## 10.4 Event Blocks

- vertical position = start time
- height = duration with a minimum readable height
- source calendar may use a 2–3px accent and very pale tint
- do not use full-saturation category blocks
- all-day Events live in a dedicated lane above the timed grid

## 10.5 Overlap

Flutter computes overlapping lanes from the visible projections.

Requirements:

- deterministic lane assignment
- no block fully hides another
- three or more overlaps may progressively compress metadata before making text unreadable

Do not move overlap layout into Rust unless a cross-client requirement later justifies it.

## 10.6 Scheduled Task Blocks

A scheduled Task is visually lighter than an Event.

Requirements:

- checkbox remains directly actionable
- no opaque full-size card if a slimmer row/block communicates it
- if duration is absent, use a compact minimum visual height
- deadline is secondary metadata and must not determine grid position

---

# 11. Today Rail

Desktop wide layout:

```text
Calendar Grid  = dominant / flexible
Today Rail     = about 240–280px
```

The rail contains only low-noise context:

```text
오늘 할 일
□ ...
□ ...

메모
...
+ 메모
```

It must not become:

- analytics dashboard
- AI feed
- Health dashboard
- Expert widget area

## Responsive behavior

Below the wide-layout threshold:

- remove the permanent rail
- expose Tasks/Notes through a sheet, drawer, or compact secondary surface
- preserve the time grid as the primary mental model

Do not horizontally squeeze the calendar merely to preserve the rail.

---

# 12. Universal Capture Update

The current blocking classification dialog is an implementation PoC, not the final interaction contract.

## Required change

Submission must be able to finish after the raw Capture is safely persisted.

```text
input
 ↓
Capture persisted
 ↓
UI returns to user immediately
 ↓
optional review / classification later
```

## Pending Capture UX

Add a recoverable path for `CaptureProcessing::Pending`.

Minimum requirements:

- pending capture count is visible but quiet
- user can open a review surface
- multiple pending captures are preserved
- `나중에` never creates an orphaned UI state
- classification may still be explicit until trustworthy model suggestions exist

Do not silently promote AI interpretation into canonical Event/Task/Note yet.

---

# 13. Editing UX

The Rust Core already has update methods; expose them through the Dart gateway/protocol and UI.

Required first-pass editing:

## Event

- title
- date/start/end
- all-day
- timezone preservation

## Task

- title
- deadline
- scheduled time
- optional duration
- priority
- complete/reopen

## Note

- content

Use sheets/popovers/dialogs appropriate to viewport.

Do not make inline editing of every field a prerequisite for this phase.

---

# 14. Controller Concurrency / Stale UI Protection

Add request identity/generation to `PersonalDayController`.

Problem to prevent:

```text
selected day changes
old command/load response arrives
old snapshot temporarily replaces current-day UI
```

Recommended mechanism:

```text
query generation / request token
```

A returned snapshot is applied only if it still matches the active query generation/date/person.

The FFI worker may remain serialized; this is a presentation consistency guard, not a native concurrency replacement.

---

# 15. First Calendar Connector — Read-only macOS

After the domain/storage/projection work above, implement the first real Calendar integration.

## Runtime placement

```text
EventKit
   ↓
Swift native adapter
   ↓
Rust Device/Core connector boundary
   ↓
Normalized Event mirror
   ↓
Turso
```

Flutter owns only:

- permission explanation
- calendar selection/visibility UI
- connection status

## Initial capabilities

```text
calendar.list_sources
calendar.list_calendars
calendar.bootstrap_events
calendar.refresh_changes
```

External mutations are explicitly out of scope for the first connector.

## Required imported metadata

At minimum:

- stable source/connection identity
- external event ID
- external revision/change identity if available
- calendar ID/name
- source calendar color/accent hint
- title
- timed/all-day schedule
- timezone
- recurrence identity sufficient to avoid obvious duplication/data loss
- deleted/cancelled state when observable

## Source route rule

Do not ingest the same Google/Microsoft calendar through both direct-provider and OS routes once direct connectors exist.

The source model implemented in §4.3 must make route selection possible later.

---

# 16. All-day and Recurrence

The UI rewrite must correctly render existing all-day domain Events before connector dogfood.

For recurrence:

- do not invent a complete recurrence engine during this UI update
- imported instances must retain enough external identity to update/delete the correct occurrence
- recurring series semantics must be designed before writable external Calendar support

The first read-only connector may mirror expanded Event instances for the visible/sync horizon if that is the safest EventKit integration, provided provenance is sufficient to deduplicate/update them.

---

# 17. Testing Requirements

## Rust domain

Add coverage for:

- IANA timezone validation/roundtrip
- Task scheduled time independent from deadline
- external provenance identity
- active overlapping events
- scheduled vs unscheduled task projection

## Storage

Add coverage for:

- migration from current schema without losing existing data
- indexed day/range queries
- CAS conflict behavior
- external source uniqueness
- soft deletion filters

## Protocol / FFI

Add coverage for:

- schema v2 mismatch
- Task schedule roundtrip
- all-day Event roundtrip
- external provenance roundtrip where exposed
- edit commands
- conflict envelope

## Flutter

Widget/controller tests for:

- calendar grid renders timed Events by time
- all-day lane
- current-time line only on today
- scheduled Task in grid
- unscheduled Task in rail
- Note in rail
- narrow layout hides permanent rail
- pending captures can be recovered/reviewed
- stale request result is discarded

## Integration

EventKit connector tests should use a boundary/fake adapter where possible; do not require the entire widget suite to depend on real system calendar state.

---

# 18. Performance Requirements

The calendar-first rewrite must not turn Day Canvas into a heavy dashboard.

## Core

Benchmark at least:

```text
1 year / 10k Event mirror
selected-day snapshot
```

A selected-day load must use indexed range queries and scale with relevant day data, not total history.

## Flutter

Targets:

- smooth calendar scroll at 60Hz minimum
- preserve 120Hz-capable interaction where hardware supports it
- layout complexity proportional to visible/relevant items
- no per-frame FFI calls
- no per-row database calls

## Current-time updates

Update only as often as visually useful (for example once per minute), not every frame/second.

---

# 19. Explicitly Deferred

Do **not** include these in the calendar-first implementation update unless they are required to unblock the above:

- Manager LLM
- Health Expert
- Gmail
- Personal Memory compiler
- Expert marketplace/runtime
- cross-device sync
- calendar write-back
- drag-and-drop automatic scheduling
- arbitrary Expert UI
- wake word / ambient voice

The point of this update is to validate the **primary daily product surface** with real calendar data.

---

# 20. Recommended PR Sequence

Keep implementation reviewable. Do not land the entire redesign as one giant PR.

## PR A — Calendar-ready domain and storage

- IANA `TimeZoneId`
- Task schedule separate from deadline
- external `SourceRef`
- Turso migration/indexed columns
- range queries
- atomic CAS

No major Flutter redesign yet.

## PR B — DayCanvasSnapshot / protocol v2

- projection v2
- protocol DTO v2
- C ABI compatibility checks
- Dart models/gateway update
- existing UI temporarily adapts enough to compile

## PR C — Calendar-first Flutter Day Canvas

- time grid
- all-day lane
- Event blocks
- scheduled Task blocks
- Today rail
- current-time line
- remove permanent Now/Next hero

## PR D — Capture and editing UX

- pending Capture recovery
- non-blocking capture completion
- Event/Task/Note edit surfaces
- stale-query protection

## PR E — Read-only EventKit connector

- permission/onboarding
- source/calendar selection
- bootstrap + refresh
- Turso mirror
- calendar accent

## PR F — Dogfood fixes

Run the product as the primary daily calendar for at least two weeks and fix:

- information density
- overlap behavior
- task/rail noise
- current-time navigation
- capture friction
- missing calendar semantics

Do not proceed to broader connector/Expert work merely because PR E compiles.

---

# 21. Definition of Done

This calendar-first update is complete when all of the following are true:

1. Opening Floe primarily feels like opening a calm day calendar, not a summary dashboard.
2. Real imported calendar Events occupy a scrollable time grid.
3. All-day Events render correctly.
4. Current time and active Events are apparent without a permanent hero panel.
5. Scheduled Tasks and deadlines are separate in the data model and UI.
6. Unscheduled Today Tasks remain visible without cluttering the time grid.
7. Notes are available without competing visually with Events.
8. Universal Capture can preserve input without forcing immediate classification.
9. Day loads use indexed range queries rather than deserializing all Person history.
10. Revision conflicts are storage-atomic.
11. Existing local data migrates without loss.
12. Flutter remains presentation-only with Rust owning canonical domain/storage semantics.
13. Calendar import has complete external provenance and is read-only for the first integration.
14. The resulting product is dogfooded for two weeks before the Day Canvas contract is treated as stable.
