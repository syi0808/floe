# Floe HTML UI Prototype

Interactive browser prototype for the squircle-first Floe interface. Today now opens
the S1 connected Calendar reference: a quiet source/status row, read-only event
details, local tasks and notes, and recoverable connection states. Connect (or the
settings icon) opens Calendar management. Task Detail, Notes, Progress, and the
earlier Personal Day/suggestion reference remain available.

## Explore S1

- **Today:** open an event for provenance, original/display time zones, recurrence,
  and all-day exclusive boundaries. Refresh the selected date; use day arrows to
  distinguish an unread date from a successfully checked empty date.
- **Connect / Settings:** inspect the selected calendar and stored dates; change it
  with a confirmation, manage permission recovery, or simulate disconnection.
- **S1 · Preview states:** opens the Prototype lab with 12 deterministic states:
  connected, disconnected, syncing, reopened cache, failed read, denied/revoked
  permission, unavailable calendar, no calendars, empty date, unread date, local-load error.
- **External changes:** in the lab, simulate a provider edit/deletion, then refresh.
  The cached view changes only after successful collection. Repeated reads do not
  append duplicate cards. Local capture/tasks survive Calendar switching/disconnection.
- **Earlier design:** the lab links to the original Personal Day reference, including
  the Floe suggestion. S1 itself has no model suggestions or external write actions.

All data, permission dialogs, settings handoffs, and refreshes are **simulated**.
Nothing touches EventKit, accounts, or the real Floe database. Prototype state is
in-memory and resets on browser reload, while navigation within the app preserves it.
The proposed disconnect UI is not a claim of implemented native functionality.

See [the screen/state specification](../../docs/design/s1-calendar-ui.md) for the
interaction matrix and implementation boundaries.

## Run

Node.js 20.19 or newer is required; the checked-in `.nvmrc` selects Node.js 24.

```sh
cd prototypes/floe-ui
nvm use
npm install
npm run dev
```

Build the static output with:

```sh
npm run build
```

## Squircle implementation

The prototype uses `@squircle-js/react` rather than large CSS `border-radius` values. The library measures responsive elements with `ResizeObserver`, generates Figma-style SVG paths, and applies them through `clip-path`. Shared wrappers in `src/primitives.jsx` keep smoothing and semantic corner sizes consistent.

The HTML prototype is the primary visual implementation reference. The PNG renders in `docs/design/renders/` remain composition history only.

## Responsive behavior

At tablet widths the main content becomes a single column. Below `780px`, the desktop side rail becomes a safe-area-aware bottom navigation, the timeline and context cards use the full viewport width, and Task and Notes controls reflow for touch targets. A second `430px` breakpoint tightens calendar and content spacing for phone-sized screens.
