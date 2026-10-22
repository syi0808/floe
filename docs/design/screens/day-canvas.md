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

On wide screens the timeline is primary. A quiet context column places the owner-selected Now and Next before relevant tasks. Use readable open rows; do not add a card for every group or duplicate the global Floe invocation.

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

Use a single scrolling Schedule list by default: source coverage, Now/Next when applicable, all-day and remaining events, collapsible Earlier events, then tasks. A visible Timeline switch preserves the full interactive grid; do not squeeze it above a fixed 30% task panel. In compact Timeline, show incomplete/stale status in the fixed Day toolbar beside Refresh. The warning icon exposes the full localized status as its accessible name and opens the existing Day details destination. When the agenda owns the empty-status notice, omit the toolbar warning, keep its single normal Day details opener, and do not repeat the status. Keep the warning initially visible and actionable at supported large-text sizes without a hidden-header swipe or focus traversal. Keep both modes available and preserve the selected view during ordinary destination changes.

Keep the selected civil date visually complete when its localized weekday form fits. When it does not fit, display the localized month/day form and retain the full weekday/date as the Text semantics label. Format directly from the selected civil date.

Now/Next follow the owner snapshot IDs and observation time, never a new UI prioritization policy. Other overlapping and tied events remain visible in the list. Suppress live focus labels after a failed latest read, even if its error is dismissed. All-day membership comes from the owner-selected civil-date projection, not UTC instant filtering. Empty or stale coverage must not imply an unoccupied day.

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
[Calendar direct manipulation](../../product/experience.md#calendar-direct-manipulation).

## Empty and exceptional states

- Empty day in Timeline mode: keep the full calendar grid interactive. Show `A little breathing
  room` as a compact inline status with gesture-appropriate creation guidance;
  never blur, dim, overlay or disable the grid.
- Initial Day read failure: keep `Could not load your day` and `Try again` within
  the middle viewport. Keep the complete diagnostic selectable in a bounded
  scroll region; at exceptionally short heights, make the whole failure card
  scrollable rather than shrinking text or clipping the recovery action.
- Conflict: explain which items conflict before offering a resolution.
- Overdue: use explicit language plus warning styling.
- Offline or stale: retain local content and identify unavailable actions.
- Loading: preserve final geometry to avoid layout shift.

## Prohibited

No speech bubbles or free-floating mascot heads over the timeline; the single standardized Floe squircle button is the only exception. No automatic assistant modal. No productivity score, streak, celebratory completion, or full-card categorical gradient.

## Content density

Omit the timeline's empty All day strip. The Schedule list uses title, time and an optional calendar label, with provenance and actions in the existing event detail/menu. Current events have a restrained violet emphasis; other rows remain neutral. Large text and overlapping events must expand list rows rather than clipping titles into miniature calendar blocks.

Show a visible event overflow button only when a target-gated edit or delete action is available; omit unavailable menu actions. Read-only detail remains reachable by the primary click, context gestures and keyboard. Accessible event cards expose one enabled, fully labeled activation action.

The timeline time marker stays behind event cards and never intercepts their input. Hour-label spacing scales with text size, while the reference line remains aligned to the actual minute. Grid guides follow the selected civil-day duration, including 23- and 25-hour days.

## Event detail handoff

Event detail leads with the event's own dates and times, not the selected canvas date. Timed endpoints preserve the schedule display zone (device-local when compatible, otherwise its declared offset); overnight events show both civil dates and repeated DST hours distinguish endpoint zones. All-day values remain civil dates and display the inclusive date span (stored exclusive end remains unchanged). Source timezone is secondary Calendar details, not an implied display timezone.

Edit and Delete appear only for a verified action target with a supplied owner callback. Selecting them closes detail and enters the existing editor or deletion confirmation; merely opening or closing detail never changes an event. Existing alert preservation and recurrence/invitation limitations stay visible before save.
