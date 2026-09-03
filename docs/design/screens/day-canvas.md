# Day Canvas Screen Specification

**Status:** primary product screen

## Goal

Help the user orient to Now and Next, act on today's tasks, recover relevant notes, and receive rare useful help without turning the day into a dashboard.

## Wide composition

```text
Local toolbar: date · Today · Day/Week/Month
┌ Timeline / Now and Next ─────────┬ Today's tasks ┐
│ Event, task, note projections    │ Related note  │
│ Current-time marker              │ Floe entry or │
│                                  │ suggestion    │
└──────────────────────────────────┴───────────────┘
Universal Capture
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

Today's tasks support quick completion and `View all`. A related note is shown only when it has current value; the sparkle appears only if its content or action is Floe-generated. The final region is either passive Floe presence or one suggestion, never both.

## Narrow composition

Order content as toolbar, Now/Next, timeline, high-priority tasks, optional suggestion, related note, and capture. Secondary tasks and detail collapse behind explicit controls. No horizontal timeline scrolling is required for the primary day view.

## Interaction

- Date movement preserves focus and announces the new date.
- Selecting an item opens semantic detail without losing timeline position.
- Completing a task updates locally and offers undo.
- Capture preserves original input and uses the classification flow.
- Accepting a suggestion opens a proposal or confirmation; it does not silently edit the calendar.

## Empty and exceptional states

- Empty day: show calm orientation and capture, not filler recommendations.
- Conflict: explain which items conflict before offering a resolution.
- Overdue: use explicit language plus warning styling.
- Offline or stale: retain local content and identify unavailable actions.
- Loading: preserve final geometry to avoid layout shift.

## Prohibited

No speech bubbles or mascot heads over the timeline. No automatic assistant modal. No productivity score, streak, celebratory completion, or full-card categorical gradient.
