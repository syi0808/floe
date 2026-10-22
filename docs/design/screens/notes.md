# Notes Screen Specification

**Status:** supporting collection view

## Goal

Make recent and relevant notes easy to scan without making Notes feel like a separate product or an AI-generated content gallery.

## Toolbar

Use `Notes · count` with the selected display date, followed by search and one `New note` action. This view is the selected day’s notes from the day snapshot, not an account-wide collection. Keep the date visible while searching. Do not show `Day / Week / Month`.

## Collection

Order a presentation copy newest-first by creation time, with a deterministic ID tiebreak. Do not change the shared snapshot order or widen the owner query.

Wide layouts may use a responsive grid; narrow layouts use a single list. Grid columns are content-width decisions, not a fixed three-column contract.

Each note preview includes:

1. a first-line heading;
2. a short plain-text excerpt;
3. creation time.

Do not invent categories or show a `Personal only` filter: the current note contract has no category field. Search the original saved content, not synthetic preview metadata.

Cards use `sq-lg`, a white or step-50 surface, and a 1px border. Any preview tint remains subtle and does not imply a category. Selected state uses focus and border in addition to color.

## Floe behavior

Do not embed a mascot or speech bubble in a note card. Floe actions become available after a note is selected or opened. AI-generated summaries and text must be labeled, reviewable, and visually distinct from the user's original content.

## Detail transition

Opening a note preserves the collection's query and scroll position. Back restores the same context. Wide layouts use a bounded reading dialog; narrow layouts use a full reading route. Keep the close action and creation date visible while the original text scrolls. Respect reduced-motion preferences.

The current note projection carries the complete original content in `NoteItem.title`; it is not a separate heading. Render that exact content as selectable body text, preserving internal whitespace and newlines. Never substitute preview/appearance excerpts for the original. Derive a short first-line heading and at most three preview lines for collection cards without modifying the stored content. Do not show unsupported editing controls or placeholder editing explanations.

New notes keep their actual creation timestamp, even while another display day is selected. After saving outside the current day view, confirm that the note belongs to today and offer an explicit `View notes` action. Never silently backdate a note or change the selected day before the user chooses that action.

## States

- Empty collection: offer `New note` and explain capture briefly.
- No search results: retain the query and offer `Clear search`.
- Loading: preserve card geometry.
- Sync/source issue: keep local readable content and identify stale metadata.

## Acceptance criteria

- Notes and calendar controls cannot be confused.
- Every card remains legible without its preview tint.
- Screen readers announce a bounded heading/excerpt preview and an open action, not the entire hidden note body. The reader exposes the complete original.
- Note cards contain no proactive assistant overlays.
- Keyboard navigation follows visual order at every responsive column count.
