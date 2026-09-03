# Notes Screen Specification

**Status:** supporting collection view

## Goal

Make recent and relevant notes easy to scan without making Notes feel like a separate product or an AI-generated content gallery.

## Toolbar

Use `All notes · count` as context, followed by search, filter, sort where justified, and one `New note` action. Do not show `Day / Week / Month`.

## Collection

Wide layouts may use a responsive grid; narrow layouts use a single list. Grid columns are content-width decisions, not a fixed three-column contract.

Each note preview includes:

1. category or source label with redundant text/icon;
2. title;
3. a short plain-text excerpt;
4. created or updated time;
5. optional relationship to today's context.

Cards use `sq-lg`, a white or step-50 surface, and a 1px border. Category tints remain subtle. Selected state uses focus and border in addition to color.

## Floe behavior

Do not embed a mascot or speech bubble in a note card. Floe actions become available after a note is selected or opened. AI-generated summaries and text must be labeled, reviewable, and visually distinct from the user's original content.

## Detail transition

Opening a note preserves the collection's query, filters, and scroll position. Back restores the same context. On wide layouts, a detail pane is allowed if it maintains readable measure; on narrow layouts, use a full route.

## States

- Empty collection: offer `New note` and explain capture briefly.
- No search results: retain the query and offer filter reset.
- Loading: preserve card geometry.
- Sync/source issue: keep local readable content and identify stale metadata.

## Acceptance criteria

- Notes and calendar controls cannot be confused.
- Every card remains legible without its category tint.
- Note cards contain no proactive assistant overlays.
- Keyboard navigation follows visual order at every responsive column count.
