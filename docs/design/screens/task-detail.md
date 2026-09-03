# Task Detail Screen Specification

**Status:** semantic detail template

## Goal

Let a user understand, edit, decompose, and complete a task while preserving the time and note context that makes it actionable.

## Wide composition

The primary column contains task type, title, description, due context, time context, source calendar, and subtasks. The contextual rail contains one relevant Floe suggestion and related notes or provenance. Both rail regions use the standard card system.

## Local toolbar

Show a contextual back action such as `Back to today`, then object actions in an overflow menu. Do not carry `Day / Week / Month` into Task Detail. Back restores the originating list or day position.

## Primary column

- Title uses `display-md` only when space permits; otherwise `headline-lg`.
- Editable fields have explicit labels and preserve unsaved input on recoverable errors.
- Due and time context are separate values.
- Calendar or source relationship is a link with text, not color alone.
- Subtasks use open rows, `sq-xs` checkboxes, duration metadata, and a final `Add subtask` action.
- Task completion is reversible; deletion requires a named confirmation.

## Contextual rail

A suggestion may reference the task and a future event, for example reviewing a brief before a meeting. It uses the standard `Floe suggests` anatomy and does not float independently. Related notes appear in the same `sq-lg` card system.

## Narrow composition

Place summary and required fields first, then subtasks, related notes, and an optional suggestion. A high-urgency suggestion may appear after the summary only if intervention policy justifies the interruption. Editing uses full-width fields or sheets.

## States

- Completed: retain detail, completion time, and undo where appropriate.
- Overdue: explicit wording plus warning token.
- Missing source: explain that the related source is unavailable without blocking local edits.
- Concurrent change: preserve the local draft and present a comprehensible resolution.
- Action failure: keep the proposal and offer retry; do not imply success early.

## Acceptance criteria

- The task remains the visual protagonist.
- Suggestion and notes align to one rail system.
- Task semantics remain usable with no assistant available.
- The screen supports keyboard completion, editing, reordering, and back navigation.
