# S1 Calendar — UI reference

Date: 2026-09-04. Status: interactive HTML design reference, **not native acceptance**.

The reference lives in `prototypes/floe-ui`. Its default Today page shows the S1
scenario. Connect and the settings button lead to connection management. The earlier
Personal Day reference remains available from the Prototype lab.

## Design intent

- The day stays primary. A healthy connection needs only a small source/status row,
  not a large account-management card in the timeline.
- Separate a successful empty read from a date not collected yet. Never imply “free”
  merely because Floe has no data.
- Cached events remain useful but are explicitly dated and marked when stale.
- Put the corrective action beside the problem, without repeated blocking alerts.
- Local tasks and notes do not depend on Calendar permission or connection.
- Permission wording must explain the EventKit full-access exception before the OS
  prompt. Read-only application behavior is not a read-only OS permission.

## Pages

| Surface | Content and actions |
| --- | --- |
| Today | Selected date/timezone, calendar provenance, refresh status, all-day and timed events, local task/note context, note capture |
| Connection management | Provider, selected calendar, Person/device scope, stored dates, last successful collection, manual refresh policy, change/reconnect/disconnect |
| Local-load failure | Honest failure heading, inspectable technical error, retry; no unexplained lone button |
| Prototype lab | Explicit fixture controls, deterministic state selection, simulated upstream edit/deletion, link to the earlier reference |

## Popups

| Popup | Primary action | Cancel / recovery |
| --- | --- | --- |
| Connection disclosure | Continue to permission handoff | Not now, Escape, close |
| OS permission handoff (simulation) | Simulate allow | Simulate denial produces a recoverable banner |
| Calendar picker | Select exactly one radio option and connect/use | Cancel preserves the old selection; search/no-result states; no-calendar state disables confirm |
| Calendar switch confirmation | Switch & read | Go back; explicitly distinguishes local-copy replacement from external deletion |
| Permission recovery | Simulate access restored, then reselect | Keep saved data; explains the real Settings path without changing settings |
| Imported event details | Return to day | Read-only provenance, original/display time, recurrence, all-day exclusive end, expandable source IDs/change token |
| Disconnect confirmation | Remove only the prototype’s saved Calendar copy | Keep connected; local tasks/notes remain; OS authorization is separate |

Native `<dialog>` supplies modality, focus containment, and Escape behavior. Closing
restores focus to the trigger. Dialogs scroll within the available viewport. No
popup silently performs an external action.

## Status hierarchy and state matrix

| Lab state | What remains visible | Message and next action |
| --- | --- | --- |
| Connected | Current sample events | Quiet up-to-date status; manual refresh |
| Disconnected | Local tasks/notes | Optional connection invitation, no error tone |
| Syncing | Existing cache, or first-read empty/loading copy | Progress indicator; duplicate refresh disabled |
| App reopened | Cached events | Collection time, not a claim of a fresh read; refresh |
| Read failed | Last good cache | Warning with timestamp; retry, no removal |
| Permission denied | Local day, no imported data | Review access; use the app without Calendar |
| Permission revoked | Last good cache | New reads paused; reconnect |
| Calendar unavailable | Last good cache | Reselect calendar; explain missing account/calendar |
| No calendars | Local day | Add in macOS Calendar, then reselect; no fake empty-event success |
| Empty date | Local day, blank imported timeline | Successful check and selected calendar/date identified |
| Uncollected date | Local day | Unknown is not empty; read this date; other dates remain cached |
| Local-load error | Last in-memory local context | Show cause and retry; pause external refresh until recovered |

Successful refresh and capture use non-blocking live-region messages. Important
errors persist until recovered; closing a modal does not dismiss the underlying
permission failure. Imported events have no edit/delete controls.

## Data and scope fidelity

- The sample uses fixed September 2026 dates and Asia/Seoul display time. A Los Angeles
  event demonstrates original versus display time. Multi-day all-day events show an
  exclusive end instead of pretending to be midnight meetings.
- Date collection is tracked independently: checking another day does not erase the
  cached sample day. Calendar switching does replace the previous connection’s cache.
- The lab can stage one upstream update and deletion. Cards retain their stable IDs;
  only a successful refresh applies the change, including its changed revision token.
- UI privacy text reflects the current unencrypted local-store baseline, not an
  unimplemented encryption promise. No model transmission or external writes in S1.
- OS handoffs, native permission behavior, persistent storage, background lifecycle,
  robust DST/recurrence normalization, and live acceptance remain separate work.
- Disconnect/local-copy removal and automatic recheck on return from Settings are
  proposed interaction targets, not newly delivered Flutter/Rust capabilities.

## Validation

Node 24: `npm ci --no-audit --no-fund` and `npm run build`.
Browser interaction checks on 2026-09-04 cover:

- Disclosure → denied → settings recovery → calendar selection → read success.
- Empty calendar search disables confirmation; calendar switching requires confirmation.
- Failed read retains cards; retry recovers. Uncollected → checked-empty → previous
  cached date leaves the previous date intact.
- Simulated upstream update/deletion applies on refresh without duplicate cards.
- Captured local note survives Calendar disconnect.
- Empty calendar list, local-load failure/retry, imported event details and Escape.
- Desktop connection layout reviewed at 1440×1000. All twelve phone state previews
  at 390×844 have no document horizontal overflow, with single-row five-item bottom
  navigation and a readable scrollable permission dialog. Browser console has no
  warnings or errors during these checks.

This prototype has no automated test harness. Browser checks and the static build
are design validation, not S1-A1–A4 integration evidence.
