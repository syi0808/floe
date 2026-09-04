# S1 Calendar — UI reference

Date: 2026-09-04. Status: interactive HTML design reference, **not native acceptance**.
Reference: `prototypes/floe-ui`. Scope: [ADR 0008](../decisions/0008-unified-calendar-read.md).

## Design intent

- Today starts with the date and one refresh action, then the unified day timeline.
  No marketing heading, timezone label in the toolbar, source selector, healthy-status
  badge, imported-data footer, or prototype lab. Use sentence case, including “All day.”
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
  simulated OS prompt. The app remains read-only despite the broader OS authorization.

## Pages and popups

| Surface | Content and actions |
| --- | --- |
| Today | Date, refresh all calendars for that date, all-day/timed events, local task/note context and capture |
| Connect / Settings | All included calendars/accounts, device/Person, stored dates, collection status, refresh, permission recovery, integration-wide disconnect |
| Local-load failure | Inspectable error and retry; no unexplained lone button |
| Connection disclosure | Explicit all-calendar scope and read-only behavior; continue or cancel |
| OS handoff simulation | Allow connects all calendars directly; denial leaves a recoverable local day |
| Permission recovery | Simulated access restoration refreshes all calendars; keep cache or close |
| Event details | Source account/calendar, original/display time, recurrence, all-day exclusive end and source identifiers; no external editing |
| Disconnect confirmation | Remove all imported copies, preserve Floe tasks/notes and external events; OS authorization remains separate |

There is no calendar picker or switch confirmation. New calendars join subsequent
date-navigation reads or explicit refreshes. Native dialogs handle modality, focus restoration, Escape and
viewport scrolling. OS handoffs never change actual settings in the prototype.

## State coverage without on-page demo controls

For development review only, load `/?state=<value>` directly. Normal navigation has
no lab, state dropdown, badge or link to the old reference.

| Value | Presentation |
| --- | --- |
| `connected` (default) | Unified sample events; no healthy-status banner/badge on Today |
| `disconnected` | Optional connection invitation; local items remain usable |
| `syncing` | Cache retained under calendar-local loading overlay, duplicate refresh disabled |
| `cached` | Saved data with collection time and refresh action |
| `offline` | Last good cache, failure timestamp and retry |
| `denied` | Local day and permission recovery |
| `revoked` | Cached events with paused-read warning and recovery |
| `missing` | Calendar/account unavailable warning, retained cache, link to inventory |
| `noCalendars` | No-source message; manage calendars in macOS then retry |
| `empty` | Successful all-calendar empty-date message |
| `uncollected` | Unknown date with read action, previous cached date retained |
| `loadError` | Local-load error and retry |

Query states are static fixtures, not actual provider failure injection. Successful
refreshes and local capture use non-blocking live-region messages. State resets on
reload, not navigation. Local notes survive simulated integration disconnection.

## Data and implementation boundary

Fixed September 2026 fixtures use Asia/Seoul display time and source-specific IDs.
The Los Angeles event preserves original time; all-day events use exclusive date ends.
Collected dates are tracked separately. Non-sample dates become empty only after a read.

The native implementation still selects one calendar. This prototype and the updated
plan do not deliver native multi-calendar migration, per-source partial-failure
reconciliation, discovery, persistent storage, robust DST handling, or live acceptance.
Those remain required by ADR 0008. The old external-edit/deletion lab was removed;
the prototype no longer exposes that simulation. Native integration evidence is unchanged.

## Validation

Use Node 24 and `npm run build`. Browser review covers the simplified Today, mixed-source
event provenance, all-calendar inventory, permission handoff and disconnection flows,
and responsive mobile layout. This prototype has no automated test harness; browser
checks and static build are design validation, not S1-A1–A4 integration evidence.
