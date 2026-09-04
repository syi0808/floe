# Floe HTML UI Prototype

Interactive browser prototype for the squircle-first Floe interface. Today now opens
the S1 unified Calendar reference: one day across all connected calendars, read-only event
details, local tasks and notes, and recoverable connection states. Connect (or the
settings icon) opens Calendar management. Task Detail, Notes and Progress remain available.

## Explore S1

- **Today:** open an event for provenance, original/display time zones, recurrence,
  and all-day exclusive boundaries. Changing the date automatically loads its events
  with a loading indicator; manual refresh remains available. Cached dates are preserved.
- **Connect / Settings:** inspect all included calendars/accounts and stored dates;
  refresh all calendars, manage permission recovery, or simulate integration-wide
  disconnection. There is no single-calendar selection or switching flow.
- **State review:** no prototype controls appear on the page. Development URLs such
  as `/?state=disconnected`, `/?state=offline` and `/?state=revoked` load deterministic
  fixtures. All 12 state values are listed in the screen/state specification below.
- **Local data:** captured notes and tasks survive navigation and Calendar disconnection.
  S1 has no model suggestions or external write actions. The external-change lab and
  earlier-reference link are removed from the UI.

All data, permission dialogs, settings handoffs, and refreshes are **simulated**.
Nothing touches EventKit, accounts, or the real Floe database. Prototype state is
in-memory and resets on browser reload, while navigation within the app preserves it.
The proposed all-calendar scope and disconnect UI are not claims of implemented
native functionality; the existing native app still uses single-calendar selection.

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
