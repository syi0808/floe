# Application Shell Specification

**Status:** target navigation and layout

## Information architecture

The global shell answers “where am I?”; the local toolbar answers “how am I viewing this?”

Global destinations:

- `Today` — Day Canvas and default home;
- `Tasks` — supporting task collection;
- `Notes` — supporting note collection.

Calendar views may live within Today or an explicit Calendar destination as product scope evolves. `Day / Week / Month` remains local to that context and must never appear in Notes or Task Detail.

## Wide layout

```text
┌ Brand ───── Today  Tasks  Notes ───────────── Settings ┐
├ Screen title / local controls / primary action ─────────┤
│ Primary content, minmax(0, 7fr) │ Context rail, 280–360 │
└──────────────────────────────────────────────────────────┘
```

- Header brand uses the 44px mascot and `Floe` wordmark.
- Global navigation uses text and selection state, not three separate filled buttons.
- The local toolbar owns date movement, view segmentation, search, filter, and object actions.
- The contextual rail appears only when its content helps the current object.

## Narrow layout

- Use a platform-native bottom destination bar, compact top navigation, or equivalent pattern.
- Keep title and one primary action visible; move secondary tools into an overflow menu.
- Stack the contextual rail after primary content in semantic order.
- Replace assistant panels and dialogs with sheets when space requires.
- Never shrink desktop columns side-by-side below readable width.

## State and continuity

Each destination preserves its scroll position, current filter, and selected object during ordinary switching. Deep links restore the destination and object. Back returns to the previous product context, not an arbitrary default date.

## Acceptance criteria

- A user can distinguish global destination from local view at a glance.
- `Day / Week / Month` exists only in a timeline/calendar context.
- The shell exposes exactly one persistent Floe entry.
- All global and local controls remain keyboard and screen-reader accessible.
