# Floe HTML UI Prototype

Interactive browser prototype for the squircle-first Floe interface. It implements the shared application shell, Today timeline, timeline-anchored Floe suggestion, Task Detail, and Notes collection.

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
