# HTML prototype component boundaries

**Status:** as-built implementation map, not a new product contract.

**Scope:** `prototypes/floe-ui`, React browser prototype. Native Flutter/Rust remains unchanged.

## Sources of truth

- [Component catalog](../../prototypes/floe-ui/src/components/catalog.json) is the
  machine-readable index: stable ID, exported name, source, props, responsibility,
  state owner, review states, style dependencies and specification link.
- [Shared component targets](components.md) remain normative design intent. Do not
  silently turn fixture copy, simulated callbacks or implementation limitations into
  native acceptance requirements.
- [Calendar screen states](s1-calendar-ui.md) and [timeline rules](calendar-event-layout.md)
  supply the interaction and time-geometry context for the calendar components.

## Dependency and ownership rules

```text
App → GlobalSidebar + screen controllers
CalendarScreen → ConnectorList → ConnectorServiceCard
               → CalendarConnections / CalendarDateToolbar / CalendarAgenda
               → CalendarContextRail + CalendarCapture + CalendarStatusBanner
               → CalendarDialogs → Modal + one dialog-content component
CalendarAgenda → CalendarAllDay + TimelineZoom + CalendarEvent + CalendarEmptyState
TaskDetail → SubtaskList → CheckControl
           → TaskSuggestionCard
NotesCollection → NotePreview
ProgressScreen → SectionHeading + ProgressBar + Checklist + ValidationItem
UI and feature components → primitives.jsx
```

1. Controllers own application state, timers and mutations. Leaf components receive
   values and semantic callbacks; they do not import a screen, navigate directly or
   read browser query parameters.
2. `CalendarScreen` stays mounted while switching Today/Connect/other navigation.
   It owns phase, collected dates, current date, zoom/scroll refs, note draft, local
   notes, local task, dialog selection and notification lifetime. Do not move these
   into conditionally mounted cards: doing so loses state on navigation.
3. Task and Notes screens retain their existing local state lifetime: navigation away
   unmounts them. This refactor does not introduce persistence or change that behavior.
4. `calendar-fixtures.js`, `progress-fixtures.js` and `data.js` contain sample data;
   `calendar-layout.js` is a pure overlap calculation, not a UI component. Fixtures
   are injected into reusable event/row components by the controller.
5. Each exported feature component has its own named file. `primitives.jsx` remains
   the cohesive geometry module. Private subcomponents of the historical reference
   screen are intentionally not promoted into the active reusable API.
6. No universal card with dozens of variants is introduced. Feature composition
   belongs in its domain; only geometry, the dialog shell and completion control
   belong in the shared UI layer.

## Contract vocabulary

The catalog records prop names; the source signature supplies defaults/rest props.
These are the nontrivial data contracts used by the calendar:

| Value | Contract |
| --- | --- |
| `calendar` | `{ id, name, account, color }`; color is blue, mint or violet in current fixtures |
| timed `event` | Stable `id`, `calendarId`, `title`, display `time`, numeric `startMinutes`/`endMinutes`, `detail`, `timezone`, `original`; optional `recurring` |
| laid-out `event` | Timed event plus zero-based `column` and positive `columns` from `layoutTimedEvents`; intervals already normalized into the displayed day |
| all-day `event` | Stable `id`, `calendarId`, `title`, `time`, `timezone`, `original`, `allDay`, presentation `color`/`caption`; optional `detail` and `endDateExclusive` |
| `pixelsPerMinute` | Positive scale, currently controlled by integer 1–12 slider; 1 is 60px/hour |
| `currentTime` | `null` or `{ minutes, label }`; screen supplies the fixture clock, agenda only positions it |
| `phase` | One of the keys in `scenarios`; only supported phases may be sent to a phase-specific component |
| `modal` | Non-null event object or `disclosure`, `permission`, `settings`, `disconnect`; controller mounts dialogs only when selected |
| `timelineScroll` | React ref to actual scroll element, owned by controller for restoration |
| `onScrollMinute` | Receives numeric top visible minute; does not itself scroll the DOM |
| note preview | `{ id, category, title, excerpt, timestamp, tone }` |
| subtask | `{ id, title, duration, done }`; toggles emit ID, not an updated array |

## Shared UI and shell

| Unit | Responsibility / interaction | Style ownership |
| --- | --- | --- |
| `SquircleSurface` | Outer border + inner surface, semantic `as`, shared radius/smoothing | `.sq-border`, `.sq-surface`, caller classes |
| `SquircleButton` | Real button; forwards native attributes/events/ref, defaults to type button | `.sq-button`, button variants |
| `SquircleBlock` | Shape wrapper; `asChild` preserves semantic child element | Caller classes |
| `CheckControl` | Controlled `checked`, accessible `label`, emits `onClick`; aria-pressed toggle | `.check-control`, `.check-visual` |
| `DotSpinner` | Eight 4px circles in a 32px ring, continuous 900ms rotation; optional screen-reader-only label with status role; static under reduced motion | `.floe-dot-spinner`, `.floe-dot-spinner-ring` |
| `Modal` | Mount opens native dialog, locks body scroll; Escape/backdrop/X calls `onClose`; unmount restores focus/scroll; unique title ID | `.s1-dialog`, `.s1-modal*` |
| `GlobalSidebar` | Controlled screen and `onNavigate(destination)`; icon-only names, tooltips, aria-current | `.sidebar-shell`, `.global-sidebar`, `.global-nav`, `.nav-link`, `.brand` |

`Modal` uses a generated title ID instead of a fixed DOM ID. Closing keeps the native
dialog, focus containment and body scroll lock mounted for a 180ms scale/fade exit,
then calls `onClose`. Reduced motion closes immediately. A 240ms fallback handles
missing animation-end events; repeated clicks are ignored during exit. `children`
accepts a node or a render function `({ close }) => content`. Content actions must use
`close()` for dismissal or `close(action)` to run a confirming mutation after exit;
that action must unmount the modal. CalendarDialogs wires every dismissing action
through this API; Continue to permission changes content without closing the shell.
`CalendarCapture` similarly generates its label/input association so instances do not collide.

## Calendar components

Connect navigation and Settings open `ConnectorList`, not the macOS detail page.
Selecting a service opens its detail (`calendar-connection` for macOS Calendar),
while Connect stays active in the sidebar. Clicking Connect again returns to the
list without resetting calendar state. No page-level back links are reintroduced.

`ConnectorList({ services, onSelect })` partitions records into Connected services
and Available services. `ConnectorServiceCard({ service, onSelect })` emits the service
ID on click/Enter/Space. A service record has `id`, `name`, Lucide `icon`, `readOnly`,
`connected`, `description`, `status` and `tone` (connected/neutral/warning). The grid
adapts to available width; the whole card is a named button with focus styling.
Both depend on `styles.css` and `components/connectors/connectors.css`.

Only macOS Calendar is currently supported. iCloud and Google are accounts inside
that connection, not separate direct integrations. Disconnect moves macOS Calendar
to Available services; access warnings stay on its connected card. Selecting an
available card opens the existing detail/setup flow rather than granting access.

| Unit | Contract and review boundary | Main selectors |
| --- | --- | --- |
| `CalendarSurface` | Feature-specific surface adapter, children + className; no state | `.s1-surface`, `.s1-surface-content` |
| `CalendarDateToolbar` | Label and independent navigation/refresh disabled flags; emits previous/next/today/refresh | `.s1-day-toolbar`, `.s1-date-controls`, `.s1-refresh` |
| `CalendarAgenda` | Composes the whole calendar box. Receives events, calendars, phase, clock and scroll/zoom callbacks; computes max column count, not event intervals | `.s1-agenda`, `.s1-timeline-*`, `.s1-time-grid`, `.s1-hour`, `.s1-half-hour-line`, `.s1-now`, `.s1-calendar-loading` |
| `CalendarAllDay` | Receives all-day rows and disabled flag; `onSelect(event)`. Empty list shows dash; first and subsequent rows retain layout | `.s1-all-day`, `.s1-more-all-day`, `.s1-all-day-title` |
| `CalendarEvent` | Receives laid-out event, resolved calendar, zoom and disabled flag; `onSelect(event)`. Preserves exact start/end, column spacing, height-based density and full accessible name | `.s1-event*`, `[data-density]`, `[data-overlapping]`, `.tone-dot` |
| `TimelineZoom` | Controlled value; emits numeric `onChange(value)`; native keyboard slider and accessible magnification text | `.s1-timeline-tools`, `.s1-zoom-*` |
| `CalendarEmptyState` | Phase/date copy with one `onAction`; controller chooses connect, read or navigate. Agenda owns whether it is shown | `.s1-empty-day`, `.s1-empty-content` |
| `CalendarContextRail` | Controlled task and notes; emits boolean task change and navigation destination. Local copy is still prototype content | `.s1-side-stack`, `.s1-local-task`, `.s1-personal-note`, `.s1-captured-note`, `.s1-provenance-hint` |
| `CalendarCapture` | Controlled draft; emits raw `onChange(value)` and nonempty trimmed `onSubmit(value)`; parent saves and clears | `.s1-capture-card`, `.s1-capture` |
| `CalendarConnections` | Controlled inventory, phase/status and collected dates; emits dialog ID and refresh; no permissions or mutations | `.s1-connections-layout`, `.s1-connection-record`, `.s1-connected-calendars`, `.s1-facts`, `.s1-actions` |
| `CalendarStatusBanner` | Supported phase selects status/alert copy and supplied recovery callback. Connected/empty are not valid banner inputs | `.s1-banner`, `.s1-banner-action` |
| `CalendarDialogs` | Routes a non-null selection into shared Modal + content. Does not change phase or clear data itself | Shared modal layout |
| `CalendarDisclosure` | Read scope / read-only explanation; close or permission callback | `.s1-modal-icon`, `.s1-permission-list`, `.s1-note`, `.s1-modal-actions` |
| `CalendarPermission` | Simulated OS permission; deny or refresh callback; no OS API | `.s1-demo-label`, `.s1-modal-actions` |
| `CalendarAccessRecovery` | Settings instructions; close or refresh callback | `.s1-settings-steps`, `.s1-note`, `.s1-modal-actions` |
| `CalendarDisconnect` | Consequences + close/confirm callbacks. Controller clears saved calendar fixture state | `.s1-permission-list`, `.s1-modal-actions`, `.s1-danger-button` |
| `CalendarEventDetails` | Event/calendar/date/provenance; browser-owned details disclosure, close callback. No edit/delete callbacks | `.s1-detail-source`, `.s1-pill`, `.s1-event-purpose`, `.s1-detail-time`, `.s1-facts`, `.s1-source-details` |

Loading and empty states occupy the whole box without shifting the calendar. Empty
background remains inert, aria-hidden and blurred. Existing CSS selectors and DOM
wrappers are retained; extraction must not add layout wrappers to the timeline.
Calendar loading uses DotSpinner without visible text; the status label remains
available to assistive technology.

CalendarConnections uses matching 23px violet icons for the boundary and offline
cards. Its permission callout has 6px vertical padding, 12px top margin and top-aligned
icon/text; this compact spacing is scoped to `.s1-connection-note`, not modal callouts.

## Tasks, notes and progress

| Unit | State / interaction | Main selectors |
| --- | --- | --- |
| `TaskDetail` | Owns fixture subtasks and suggestion visibility; options remain right-aligned | `.task-detail-screen`, `.task-panel`, `.task-metadata`, `.detail-layout` |
| `SubtaskList` | Controlled list; `onToggle(id)` | `.subtask-list`, `.subtask-row` |
| `TaskSuggestionCard` | Parent mounts/hides; emits dismiss. Review now and Snooze remain visual-only, as before | `.detail-suggestion`, `.suggestion-heading`, `.inline-actions` |
| `NotesCollection` | Owns notes/search/filter; existing New note creates a single Untitled draft | `.notes-screen`, `.notes-toolbar`, `.notes-grid`, `.notes-empty` |
| `NotePreview` | Data-only display; no click/open callback exists yet | `.note-preview-border`, `.note-preview`, `.note-category` |
| `ProgressScreen` | Composition over fixed fixture estimates, not live telemetry | `.progress-screen`, dashboard layout selectors |
| `SectionHeading` | Icon, eyebrow, title, meta display | `.section-heading`, `.section-icon` |
| `ProgressBar` | Value/tone; aria-valuenow unchanged, visual minimum fill remains 2% | `.progress-bar`, `.progress-fill` |
| `Checklist` | Title/items/complete determine delivered vs open icon | `.checkpoint-list` |
| `ValidationItem` | Value or icon with label; caller must supply one | `.validation-item` |

`components/reference/TodayScreen.jsx` preserves the earlier reference and its private
Timeline/SuggestionPortal/Popover/TasksCard/NoteCard helpers. It is not reachable from
normal navigation and must not be used to infer active S1 calendar behavior.

## Styling boundary

No visual redesign or CSS rewrite is part of this extraction. `styles.css` remains
the global token/base/responsive layer loaded by `main.jsx`; `calendar.css` is the
feature layer loaded by `CalendarScreen`. Catalog entries list both when necessary,
including Modal. Isolated previews must load these styles in that order. The shared
Modal's `s1-*` names are retained for compatibility, not a permission to import the
screen into the UI layer. `assets.js` centralizes the canonical mascot URL.

Keeping selectors stable avoids regressions in existing compound selectors,
breakpoints and Squircle clipping. A later CSS-module split is separate work.

## Reverse-spec workflow

1. Select a stable catalog ID, not an incidental DOM path or screenshot marker.
2. Read its source, props/defaults, state owner and style dependencies. Follow child
   components instead of duplicating their specifications into the parent.
3. Capture each listed state, plus applicable hover, keyboard focus, disabled,
   reduced-motion and narrow-screen cases. Record input data and viewport/zoom.
4. Describe anatomy, sizing, typography, callbacks and accessibility. Link design
   targets separately from observed prototype behavior; mark placeholders explicitly.
5. On a component change, update its contract/catalog entry and the affected feature
   specification, then run `npm run check:components` and `npm run build`.

The catalog validator checks export coverage, unique IDs, prop signatures and source/
style/spec paths. It does not certify behavior, accessibility or visual parity.
State lists are review prompts, not generated acceptance evidence.

## Regression review

- Today: event geometry/overlaps, all-day details, short-event zoom, date read/loading,
  empty overlay and manual refresh; scroll/zoom and local data survive navigation.
- Dialogs: open, close, Escape/backdrop, focus return, source disclosure, permission
  denial/recovery and disconnect; no external writes.
- Local capture/task: trim/empty guard, save feedback and task checkbox.
- Tasks: subtask toggle and suggestion dismissal. Notes: search/filter/clear/new draft.
- Progress and responsive shell: no missing imports, preserved layout and named controls.

No native acceptance status changes as a result of this refactor.

Page-level back links are removed from Connect and Task Detail. Use primary navigation
to change pages; dialog dismissal actions such as Back to my day remain available.
