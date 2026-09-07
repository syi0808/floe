# S1 Calendar — UI reference

Date: 2026-09-04. Status: UI design reference, **not native acceptance**.
Scope: [ADR 0008](../decisions/0008-unified-calendar-read.md).

2026-09-05 update: Connect exposes explicit All/Selected scope. Selected preserves
checked IDs; All includes new sources on refresh. Cancel keeps saved scope, and an
empty selected subset cannot be saved. Partial-source cache status and disconnect are
reflected natively. Repeated local wall-clock hours remain distinct in layout without
exposing timezone labels.

## Design intent

- Today starts with the date and one refresh action, then the unified day timeline.
  No marketing heading, timezone label in the toolbar, source selector, healthy-status
  badge or imported-data footer. Use sentence case, including “All day.”
- Date navigation automatically reads the destination date with a loading overlay
  inside the calendar box, not a page-level banner. Cached event actions are disabled
  during loading and the surrounding layout stays in place.
  Retain that date's existing cache during loading, preserve other dates, and show an
  empty result only after success. Use the requested offset, not the previous date.
  Navigation does not request permission or bypass existing access/provider errors.
  Automatic reads finish without a success toast; manual refresh still confirms completion.
- Include all calendars available through macOS Calendar, not one selected calendar.
  Work, Personal and Product team fixtures share the timeline, retaining source colors
  and identity. Calendar inventory belongs in Connect, not above the day.
- Remove the local badge and decorative note-heading dot. Local tasks/notes remain
  independent of the connection. Event details retain provenance and original/display time.
- Show contextual banners only for pending, stale or problematic data. An unread date
  is not empty; an empty result must mean a successful read across all included sources.
- Disclose the EventKit full-access exception and all-calendar read scope before the
  OS prompt. The app remains read-only despite the broader OS authorization.

## Pages and popups

| Surface | Content and actions |
| --- | --- |
| Today | Date, refresh all calendars for that date, all-day/timed events, local task/note context |
| Connect / Settings | Responsive icon-card grid of connected and available services; selecting macOS Calendar opens its detail |
| macOS Calendar service detail | All included calendars/accounts, device/Person, stored dates, collection status, refresh, permission recovery, integration-wide disconnect; sidebar Connect returns to the list |
| Local-load failure | Inspectable error and retry; no unexplained lone button |
| Connection disclosure | Explicit all-calendar scope and read-only behavior; continue or cancel |
| OS handoff | Allow connects all calendars directly; denial leaves a recoverable local day |
| Permission recovery | Restored access refreshes all calendars; keep cache or close |
| Event details | Source account/calendar, original/display time, recurrence, all-day exclusive end and source identifiers; no external editing |
| Disconnect confirmation | Remove all imported copies, preserve Floe tasks/notes and external events; OS authorization remains separate |

There is no calendar picker or switch confirmation. New calendars join subsequent
date-navigation reads or explicit refreshes. Native dialogs handle modality, focus restoration, Escape and
viewport scrolling.

## State coverage

Popup entry uses a 240ms ease-out scale (0.96 → 1), 8px upward settling and opacity,
with a 200ms backdrop fade. No overshoot, content staggering or layout animation.
Reduced-motion disables both animations. The shared dialog handles all popup types;
changing content inside an already-open dialog does not replay entry. Closing uses a
120ms ease-in fade with scale 1 → 0.97 and 6px downward movement; backdrop fades with
it. X, Escape, backdrop and dismissing content actions share the same exit. Focus and
scroll lock remain until exit completes; reduced-motion closes immediately. This follows
the restrained scale and easing guidance in
[Emil Kowalski's animation tips](https://emilkowal.ski/ui/7-practical-animation-tips).

The preview should cover connected, disconnected, syncing, cached, offline, denied,
revoked, missing-source, no-calendar, empty, uncollected and local-load-error states.
Successful refreshes use non-blocking live-region messages. Local notes survive
integration disconnection.

## Data and implementation boundary

Fixed September 2026 fixtures use Asia/Seoul display time and source-specific IDs.
The Los Angeles event preserves original time; all-day events use exclusive date ends.
Collected dates are tracked separately. Non-sample dates become empty only after a read.
Timed fixtures carry numeric start/end minutes. Event position, duration, hour lines
and the current-time marker share a midnight origin in a 24-hour internal scroll region.
The base scale is one pixel per minute, with 3× and 12× zoom for short events.
Cards never grow to fit text. [Timeline density rules](calendar-event-layout.md) define
five-minute duration bands, available content, keyboard/touch access and zoom behavior.
Only hour and half-hour guides are drawn, never five-minute grid lines.

The native implementation still selects one calendar. This design does not deliver
native multi-calendar migration, per-source partial-failure
reconciliation, discovery, persistent storage, robust DST handling, or live acceptance.
Those remain required by ADR 0008. Native integration evidence is unchanged.

## Validation

Use the Flutter preview and design-feedback mode to review the simplified Today,
mixed-source event provenance, all-calendar inventory, permission handoff,
disconnection flows and responsive layout. This design review is not S1-A1–A4
integration evidence.
