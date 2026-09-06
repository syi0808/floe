# Calendar Direct Manipulation

> Status: Product direction and interaction contract

## Goal

Floe Calendar should feel familiar to Apple Calendar users without copying its
visual design. Creation, movement and deletion happen in the calendar itself;
they are not presented as a separate `Calendar Proposal` feature.

All mutations still use the shared Action Authority, Review and Activity
pipeline. Direct manipulation changes how an intent is expressed, not which
safety checks or permissions apply.

## Primary create entry

Remove the labeled `Plan a Calendar event` control from the contextual rail.
Place one compact `+` icon button in the Calendar toolbar beside the refresh and
view controls.

- Tooltip and accessibility label: `Create event`.
- The button opens the event editor with the currently viewed date selected.
- On Today, default to the next sensible future interval. On another date, use a
  daytime default rather than the device's current clock time.
- The editor is an anchored popover on wide layouts and a sheet on narrow
  layouts. It is not a Review inbox item.

## Double-click to create

Double-clicking empty space on the timed calendar opens the same editor anchored
to that position.

- Convert the pointer position through the Calendar day axis, including 23- and
  25-hour DST days.
- Snap the initial start to a visible 15-minute grid and choose the configured
  default duration.
- Show a provisional event block while the editor is open.
- Double-clicking an existing event opens its detail/editor instead of creating
  an overlapping event.
- `Escape`, clicking Cancel, or dismissing the popover removes the provisional
  block and performs no action.

Pointer double-click is an accelerator, not the only path. Keyboard users use
the toolbar `Create event` button; touch layouts use the button or a deliberate
long-press on empty time.

## Empty day

An empty day is a useful calendar state, not a disabled state. After a successful
complete read confirms that the selected calendars contain no events, keep the
full all-day row, time grid, scrolling, zoom and pointer/keyboard interactions
available.

Do not blur, dim, cover or wrap the empty calendar in `IgnorePointer`. Do not use
a full-card empty overlay or replace the grid with a mascot illustration. Those
treat the best surface for creating the first event as unavailable.

Communicate the free day with one quiet inline status between the Calendar tools
and the timed grid:

```text
A little breathing room
No events on this day. Double-click a time to create one.
```

- The message has `status` semantics but does not take focus automatically.
- It occupies a compact fixed region and never intercepts grid gestures.
- The `+` button remains the explicit accessible action; the message does not need
  another create button.
- On touch layouts, the helper says `Use + or press and hold a time to create an
  event` rather than teaching a desktop gesture.
- The current-time line remains visible on an empty Today.
- Creating a provisional block removes the empty message immediately. Canceling
  restores it; provider re-import remains authoritative.

`No events` is valid only after a successful read of the complete selected scope.
Not connected, permission denied, stale, partial and failed reads use their own
status/recovery UI and must not be described as breathing room.

## Event editor

The quick editor contains title, destination calendar, start/end and an explicit
`More options` path. The initial Calendar capability continues to create one
timed, non-recurring event without guests or alerts. Unsupported fields must not
appear editable.

Submitting the editor creates an internal action intent:

```text
calendar.create intent
        ↓
Action Authority
  allow → validate and execute
  ask   → Review request
  deny  → explain and link to Action permissions
```

The editor closes after a valid submission. If review is required, the provisional
block is removed and the Review request becomes visible. If automatic execution
succeeds, the imported provider event replaces it. An uncertain result never
creates a second provisional or external event.

## Drag to move

Dragging a writable timed event moves its start while preserving its duration.
The first delivery does not resize duration.

- Start dragging only after a movement threshold so normal click/detail remains
  reliable.
- Show the original event as a muted origin and a single moving ghost.
- Snap to 15 minutes by default and auto-scroll near viewport edges.
- `Escape` cancels and restores the original position.
- Dropping outside a valid day/time or onto a read-only destination cancels.
- Keyboard users receive `Move event`, date/time fields and arrow-step controls
  from the event menu/detail. Pointer drag is never required.

Drop submits `calendar.update.time`; it does not immediately rewrite the local
mirror:

- `allow`: validate against fresh provider state, execute once, then re-import;
- `ask`: restore the event to its authoritative position and create a Review
  request showing old and new time;
- `deny`: restore it and explain why moving is unavailable.

Conflict policy remains explicit. A visual overlap is not proof that the provider
will accept or reject the move. Validation uses current local and provider events
immediately before execution.

## Context menu

Right-clicking an event opens a native-feeling context menu anchored to the event.
The initial menu model is capability driven:

```text
Open details
Edit…
Move to calendar…
Duplicate…
Delete…
Show in source calendar
```

Only implemented and provider-supported actions are enabled. Read-only events
keep details and source navigation but disable mutations with an explanation.
The same menu is available from an event `More` button, keyboard menu key or
`Shift+F10`; touch uses long-press.

`Delete…` submits a separate `calendar.delete` intent and defaults to `ask`, even
when Calendar create is automatic. The Review describes the exact event,
calendar, recurrence scope and attendee/notification side effects. The initial
non-recurring implementation must not imply recurring-series support.

## Feedback and history

- Review contains only decisions still waiting for the user.
- Activity records direct, automatic and reviewed mutations using the same event
  language: created, moved, deleted, blocked or unresolved.
- Successful create/move/delete is reflected in the calendar only after provider
  execution and re-import.
- A short in-place progress treatment may occupy the affected block, but terminal
  history never remains embedded in the day timeline.
- Response loss triggers exact lookup/reconciliation; it never repeats a mutation.

## Capability and authority model

Calendar permissions are independent:

```text
calendar.create
calendar.update.time
calendar.update.details
calendar.delete
```

Each has `allow automatically`, `ask every time`, or `do not allow`, plus future
calendar/account and Expert scopes. Provider writability, OS permission and Floe
Action Authority are all required. A broad preset may change supported rules but
must enumerate create, update and delete separately before confirmation.

## Delivery sequence

1. Move create entry to the toolbar `+` button.
2. Replace the inert empty overlay with an interactive grid and inline status.
3. Add empty-slot double-click and anchored quick editor.
4. Add capability-aware event context menu.
5. Implement durable Calendar update/delete intents and native executors.
6. Add drag ghost, snapping, cancellation and auto-scroll.
7. Connect move/delete to Review, Activity and per-action authority.
8. Add keyboard and touch equivalents, then validate DST and recovery matrices.
