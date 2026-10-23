# Application Shell Specification

**Status:** target navigation and layout

## Information architecture

The global shell answers “where am I?”; the local toolbar answers “how am I viewing this?”

Global destinations:

- `Today` — Day Canvas and default home;
- `Tasks` — supporting task collection;
- `Notes` — supporting note collection.

Calendar belongs to Today. `Day / Week / Month`, when supported, is local to that context and must never appear in Notes or Task Detail.

Floe is one persistent invocation, not a fourth destination. Its conversation surface exposes Voice as an explicit mode handoff; there is no separate global Voice route. Do not auto-start the microphone on invocation. Closing an active Voice surface requires confirmed local Stop (or confirmed cancellation of preparation); uncertain starts retain recovery controls.

Activity, Connected services and Settings live in a secondary More menu. Contextual source/permission links still lead directly to the appropriate review. Pending decisions must not be treated as completed history or hidden merely to simplify navigation. An unavailable conversation explains the limitation and offers Settings, not a voice action that cannot load a session.

## Wide layout

```text
┌ Brand ───── Today  Tasks  Notes ───────────── Settings ┐
├ Screen title / local controls / primary action ─────────┤
│ Primary content, minmax(0, 7fr) │ Context rail, 280–360 │
└──────────────────────────────────────────────────────────┘
```

- A quiet brand identifies the workspace. The mascot is reserved for the single Floe invocation.
- Global navigation shows Today, Tasks and Notes with visible labels, subtle selection and explicit keyboard focus. A compact side rail is supported.
- The local toolbar owns date movement, view segmentation, search, filter, and object actions.
- The contextual rail appears only when its content helps the current object.

## Narrow layout

- Use three visibly labeled destinations and a distinct Floe invocation. Keep More in the compact top bar, not in the primary bottom destinations.
- Keep title and one primary action visible; move secondary tools into an overflow menu.
- Stack relevant tasks and source coverage after primary content in semantic order. Omit decorative quotes and empty note cards.
- Replace assistant panels and dialogs with sheets when space requires.
- Never shrink desktop columns side-by-side below readable width.

## Page heading rhythm

Collection pages lead with the shared `pageTitle` role (28px/700). Counts and
selected dates remain supporting `bodySmall` context, and the first collection
content begins about 24px below the heading/context block. Keep the selected Day
date as a contextual toolbar headline beside its calendar controls rather than
recasting it as a collection-page title.

The Tasks destination reads as a checklist: task titles lead, due information is
quiet supporting type, and rows omit the repeated item-kind label. A missing
optional deadline stays available to assistive technology without taking a
visible line in every row. Keep overdue state explicit and warning-colored. On
wide layouts, bound the collection to a readable column so each row's actions
stay visually grouped with its task.

## State and continuity

Each destination preserves its scroll position, current filter, and selected object during ordinary switching. Task detail provides Back to the originating collection or day. Notes preserve filters across destination switches. Deep links restore the destination and object. Back returns to the previous product context, not an arbitrary default date.

## Acceptance criteria

- A user can distinguish global destination from local view at a glance.
- `Day / Week / Month` exists only in a timeline/calendar context.
- The shell exposes exactly one persistent Floe entry.
- All global and local controls remain keyboard and screen-reader accessible.

The desktop navigation rail keeps consistent button padding and caption sizing for the primary destinations and Floe. When window height or large text makes the rail taller than its viewport, it scrolls instead of clipping destinations or the utility menu. The utility menu remains bottom-aligned when space permits.
