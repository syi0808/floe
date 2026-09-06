# Calendar Direct Manipulation

> Status: Implementation contract, revised 2026-09-07

## Current delivery

- Desktop time entry uses a date-picker button and separate two-digit hour/minute
  segments, direct typing, Up/Down keys and visible steppers. No clock-face picker
  or raw date-time string. Floe squircle surfaces, neutral fills, primary accents
  and tabular digits preserve the app's visual language.
- The date picker and event menu now share a custom, undimmed anchored popover:
  160ms origin-aware scale/fade, 120ms exit, subtle press feedback, viewport
  collision handling, focus restoration and reduced-motion support. The date
  grid includes adjacent days, Today, month stepping, arrow navigation and
  Page Up/Down. Menu arrows skip disabled actions; Enter selects, Escape cancels.
  Motion reference: [Emil Kowalski's practical animation tips](https://emilkowal.ski/ui/7-practical-animation-tips).
- Changing start preserves elapsed duration; changing end changes duration.
  Midnight rollover is allowed, end must follow start, maximum duration is 24 hours.
  Past events are valid for direct user entry. Nonexistent local clock times are
  rejected rather than silently normalized. The editor displays device-local time.
- Desktop drag moves within the visible day, preserves duration, snaps to 15
  elapsed minutes on the existing DST-aware day axis, and auto-scrolls at edges.
  The origin dims and a single snapped ghost shows the candidate time. Escape,
  outside drop, loading or changing day cancels. Cross-day moves use Edit's date
  picker; cross-day pointer dragging and duration resize are not yet supported.
- Right-click, More, menu key, Shift+F10 and touch long-press expose details,
  edit and delete. Delete has one event-specific confirmation. Touch uses Edit
  rather than drag so scrolling remains reliable.
- Provider import supplies `can_modify`; missing metadata fails closed until a
  refresh. Writable, non-recurring timed events without guests are
  editable. Existing alerts are now supported and preserved during edits; new
  creates still do not add alerts. Native execution checks capabilities again and compares the
  original provider revision, including last-modified metadata.
- User-confirmed create/edit/delete is durable direct authority, not a request
  for automated authority. It never appears in Review, including after restart
  or an uncertain result. Activity retains its origin, operation and execution
  status. Existing records without origin keep their previous Review semantics;
  their origin cannot be safely inferred retroactively.
- Execution still uses the existing one-shot ledger, native permission checks,
  selected-calendar scope, conflict checks and provider re-import. Explicit
  direct updates allow overlapping appointments, like ordinary calendar editing;
  automation and create conflict checks are unchanged. Delete ignores overlap.
  Existing event URLs are preserved.
- Failed or uncertain execution does not optimistically change the mirror.
  Activity offers lookup, never a repeated write. An update can reconcile its
  exact target and final state. Absence alone cannot prove a delete succeeded:
  response-loss deletion remains unresolved, rather than reporting false success.
  While unresolved, writes to the same event are unavailable. Unrelated events
  remain editable; uncertain creates block further creates, not all calendar drags.
- Mouse dragging is based on pointer kind rather than the theme's platform;
  long-press recognition is restricted to touch/pen so holding the mouse before
  moving cannot let the context menu steal the drag. Touch keeps long-press menus
  and normal scrolling. Startup refreshes connected
  EventKit calendars so stale cached capability flags do not disable editing.

## Next iterations

Anchored editor/sheet positioning, cross-day drag, duration resizing, Duplicate,
Move to calendar, source-app navigation, recurrence scopes, attendee handling,
Undo and durable native deletion receipts require separate capability work. Do not
enable placeholder menu actions. Validate live EventKit behavior only against a
disposable calendar with explicit user consent.

## Goal

Floe Calendar should feel familiar to Apple Calendar users without copying its
visual design. Creation, movement and deletion happen in the calendar itself;
they are not presented as a separate `Calendar Proposal` feature.

All mutations still use the shared execution ledger and Activity pipeline.
Review is for automation requests, not a second approval of the user's explicit
save, drag or confirmed deletion. Direct authority does not skip safety checks
or provider permissions. Automation policy remains independent.

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

Communicate the free day with one quiet banner in the unused leading side of the
Calendar tools row:

```text
A little breathing room
No events on this day. Double-click a time to create one.
```

- The message has `status` semantics but does not take focus automatically.
- It shares the tools row's existing height, stays clear of the timeline, and
  never intercepts grid gestures.
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

Submitting the editor creates an internal, explicitly authorized action intent:

```text
direct calendar.create intent
        ↓
durable approved ledger → fresh validation → execute once → Activity
```

The editor closes after successful execution. Provider re-import replaces the
provisional block. Errors remain explicit and point to Activity; uncertain results
never create a second provisional or external event.

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

- direct authority: validate fresh provider state, execute once, then re-import;
- blocked or uncertain: restore the authoritative position and point to Activity.

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

`Delete…` confirms the exact event and calendar before submitting a direct delete.
It does not create a Review request. The initial non-recurring implementation must
not imply recurring-series support. Automation-initiated delete is a separate,
future capability and must not inherit this direct authority.

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
7. Keep direct move/delete in Activity, separate from automation Review.
8. Add keyboard and touch equivalents, then validate DST and recovery matrices.
