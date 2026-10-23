# Task Detail Screen Specification

**Status:** semantic detail template

## Goal

Let a user understand, edit, decompose, and complete a task while preserving the time and note context that makes it actionable.

## Wide composition

Use one readable primary column for title, relevant description, due state and completion. Related context appears only when available. A secondary panel is reserved for actual selected context or a policy-qualified proposal, not generic advice. Do not surround every field group with a card.

## Local toolbar

Show a contextual back action such as `Back to today`. Keep completion primary and expose destructive removal as a clearly labeled, low-emphasis action in the detail view; use neutral color at rest and reserve destructive color for hover, focus, press, and confirmation. Do not use a one-action overflow menu. Do not carry `Day / Week / Month` into Task Detail. Back restores the originating list or day position and keyboard focus.

## Primary column

- Title uses `display-md` only when space permits; otherwise `headline-lg`.
- Editable fields have explicit labels and preserve unsaved input on recoverable errors.
- Due and time context are separate values.
- Calendar or source relationship is a link with text, not color alone.
- Subtasks use open rows. Editable checkboxes and Add subtask require a connected mutation owner; preview-only context is read-only.
- Task completion is reversible; deletion requires a named confirmation.

## Contextual rail

A real suggestion may reference the task and a future event. It uses the standard Floe suggestion anatomy and appears only when intervention policy qualifies it. Related notes remain quiet readable content; they do not require a colored card.

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
- Only relevant, source-backed context competes with the task.
- Task semantics remain usable with no assistant available.
- Completion and back navigation work by keyboard. Editing and reordering appear only when supported by an owning service.

## Content and action integrity

Use a single readable content column with title, relevant description, due state and the real completion action. Keep a plainly labeled Delete task action available in detail, confirm it by naming the task, and disable repeated actions while a command is pending. Omit empty metadata sections, invented update timestamps and generic assistant advice. Preview-only subtasks and notes may supply visual context, but must not expose mutation controls without an owning service. Review and snooze controls require real proposals and durable behavior; hiding a card is neither review nor snooze. The completion action reflects pending state and ignores repeat input until the owner responds. Keep command failures visible with the detail and its retryable actions. A late delete completion must not replace newer navigation or move focus away from it. Back restores the originating day or task collection, including scroll position and keyboard focus.
