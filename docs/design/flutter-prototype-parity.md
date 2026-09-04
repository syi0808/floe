# Flutter prototype parity — September 4, 2026

The latest `prototypes/floe-ui` calendar and connectors replace the earlier Flutter
timeline. Production continues to use `PersonalDayController` and the existing
Rust/FFI gateway; previews use in-memory data only. This is UI implementation,
not certification of live EventKit acceptance.

## Reusable units

| Flutter unit | Responsibility / inputs | Prototype counterpart |
| --- | --- | --- |
| `CalendarAgenda` | Snapshot, loading and connection navigation; owns persisted zoom/scroll, full-day viewport and centered empty state | CalendarAgenda / TimelineZoom / CalendarAllDay |
| `layoutCalendarEvents` | Pure clipped display-minute placement and deterministic overlap columns | calendar-layout.js |
| `CalendarEventCard` | Exact event height, density, hover, tooltip and detail navigation | CalendarEvent |
| `CalendarEventDetails` | Actual source metadata, read-only note, disclosure; no imported-event mutations | CalendarEventDetails |
| `CalendarContextRail` | Controlled task checkbox and task navigation, saved notes | CalendarContextRail |
| `ConnectorScreen` | List/detail navigation, service identity and purpose, boundary/offline cards | ConnectorList / ConnectorServiceCard |
| `CalendarPanel` | Actual gateway calendar selection, refresh and OS settings; pending/error feedback | CalendarConnections |
| `FloeTextLink` | Underline-only hover/focus, no extra horizontal padding | text-link / all-day actions |
| `FloeDotSpinner` | Eight rotating dots, semantic loading label, static reduced-motion variant | DotSpinner |
| `showFloeDialog` / `FloeDetailDialog` | Backdrop blur, modal focus/navigation, 240 ms enter and 120 ms exit; zero motion when requested | Modal |
| `FloeInfoNote` / `FloeReadOnlyPill` | Compact top-aligned note and rounded permission indicator | note / pill |

## Calendar rules

- 1 logical pixel/minute at 1×; zoom 1–12× preserves the top visible minute.
- Visible day is 00:00–24:00. Clip overnight intervals; omit invalid and outside intervals.
- Only hour and half-hour guides. Five-minute appointments are five pixels at 1×,
  not inflated into overlapping cards. Zoom or keyboard/tap opens readable detail.
- Under 24 px: straight inset accent bar; 24–41 px: title; 42–57 px: title/time;
  58 px and above: title/time/calendar when available. Thresholds grow with text scale.
- Simultaneous intervals share columns; adjacent boundaries do not overlap. Column
  reuse is deterministic. A transitive overlap group shares a consistent width.
- Empty state covers the complete calendar surface, not just the scroll content.
- Source detail padding is 16 px on both ends; spacing occurs only between records.

## Scope boundaries

Tasks and Notes retain their real local create/classify/complete/delete paths and
existing collection layouts. The current HTML Tasks page is a static example of a
task detail; Flutter opens that layout from an actual task. Task-detail Back to
today is removed; connector-detail Back to connections remains. The HTML Progress
screen is a development dashboard, not a product destination.

Native `CalendarGateway` still selects one calendar and has no disconnect method.
All-calendar collection, deleting a mirror on disconnect, recurrence fields and
IANA original-zone conversion are not fabricated in the presentation layer.
Settings opens the real OS settings action only from the native connection detail.
The preview does not request calendar permissions or touch the user's database.

## Verification

`flutter analyze`, `flutter test`, and `flutter build macos --debug`.
Automated tests cover controller load ordering/retry/disposal and data/native gateway
behavior. Flutter design, layout, geometry, interaction and visual-capture tests are
not maintained. Review those manually using `lib/main_preview.dart`, including zoom,
overlap, short-event detail, dialog dismissal, navigation and narrow-window layouts.
