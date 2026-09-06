# Day Canvas Screen Specification

**Status:** primary product screen

## Goal

Help the user orient to Now and Next, act on today's tasks, recover relevant notes, and receive rare useful help without turning the day into a dashboard.

## Wide composition

```text
Local toolbar: date · Today · Day/Week/Month
┌ Timeline / Now and Next ─────────┬ Today's tasks ┐
│ Event, task, note projections    │ Related note  │
│ Current-time marker              │ Related note  │
│ Optional anchored Floe button    │               │
└──────────────────────────────────┴───────────────┘
```

The timeline is primary. The rail order is tasks, related note/context, then Floe. Rail cards use one shared system.

## Primary content

- Make Now the strongest local element and Next clearly visible.
- Keep Event, Task, Note, Commitment, and Intervention semantics distinct.
- Show only metadata required to act; reveal recurrence, source, attendees, and provenance in detail.
- Use open rows or compact event blocks instead of a card per item.
- Mark current time with text and a line; do not depend on violet alone.
- Fold distant or dense periods before reducing typography or hit targets.

## Contextual rail

Today's tasks support quick completion and `View all`. A related note is shown only when it has current value; the sparkle appears only if its content or action is Floe-generated. The rail may end with passive Floe presence only when no timeline Floe button or suggestion is visible.

## Timeline suggestion

A time-specific suggestion appears first as one 52×52 white `sq-md` button with a 30px Floe mascot. It sits on the right edge of the relevant time block and overlaps it slightly without hiding title or time. It contains no text or notification badge. Activation opens a non-modal `sq-xl` popover anchored above or beside the button; the underlying timeline remains readable and undimmed.

## Narrow composition

Order content as toolbar, Now/Next, timeline, high-priority tasks, optional suggestion, and related note. Secondary tasks and detail collapse behind explicit controls. No horizontal timeline scrolling is required for the primary day view.

## Interaction

- Date movement preserves focus and announces the new date.
- Selecting an item opens semantic detail without losing timeline position.
- The Calendar toolbar exposes one icon-only `Create event` button. A labeled
  Calendar-planning control does not live in the contextual rail.
- Double-clicking empty timed space opens an anchored quick event editor at that
  time. Double-clicking an event opens that event instead.
- Writable timed events support drag-to-move with a ghost, 15-minute snapping,
  edge auto-scroll and Escape cancellation. The original mirror does not move
  until provider execution and re-import succeed.
- Right-click opens a capability-aware event menu. The same actions remain
  reachable through keyboard, an event More button and touch long-press.
- Completing a task updates locally and offers undo.
- The calendar has no bottom capture input; create notes from the Notes page.
- Activating the Floe button opens its suggestion. Consequential responses enter
  shared Review only when Action Authority requires a decision.

Detailed create, drag, context-menu and authority behavior follows
[Calendar Direct Manipulation](../../planning/01-experience/calendar-direct-manipulation.md).

## Empty and exceptional states

- Empty day: show calm orientation, not filler recommendations.
- Conflict: explain which items conflict before offering a resolution.
- Overdue: use explicit language plus warning styling.
- Offline or stale: retain local content and identify unavailable actions.
- Loading: preserve final geometry to avoid layout shift.

## Prohibited

No speech bubbles or free-floating mascot heads over the timeline; the single standardized Floe squircle button is the only exception. No automatic assistant modal. No productivity score, streak, celebratory completion, or full-card categorical gradient.
