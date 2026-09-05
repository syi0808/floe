# EventKit live Calendar checkpoint

Date: 2026-09-05, approximately 22:28–22:44 KST.
Environment: macOS 26.5.2 (25F84), arm64, iCloud source via local EventKit.
Working-tree base: `41629f1`. The pre-existing debug app was **not rebuilt** in this
run; source-to-binary correspondence is not asserted. Its existing signature passed
`codesign --verify --deep --strict`.

Binary SHA-256:

- Existing Floe executable: `0d459dbc86bf8a230c60e746821edd8b6dd9ae69269b88fe6f98d2be0ade55a4`.
- PoC executable including edit-test: `04ae5de15c693186f8149be7b5ed3bff815a37674c4ff537ec4ee697a66a8471`.

## Authorization and data scope

The user authorized S1 live validation and the proposed dedicated calendar workflow:
`Floe Validation` on iCloud; one `Floe PoC — disposable` event at
2026-09-06 10:00–10:15 Asia/Seoul, without invitees or alarms, followed by test edit
and cleanup. No existing event or calendar was edited/deleted. Existing Floe calendar
selections were retained and the test calendar was added explicitly.

System Settings showed an existing Calendar full-access grant for the responsible
Codex process. No permission toggle was changed, no TCC reset performed, and no new
OS access grant was accepted by the agent. The CLI reports authorization 3/full access.
This does not establish a standalone Floe permission dialog or revocation cycle.

## Observed live sequence

1. Existing Floe displayed 10 connected calendars and a previous successful read.
   Refresh first showed the typed denied/revoked message while retaining the cached
   inventory/range/time. Manage access reached macOS Calendar privacy settings.
2. Reconnect/Continue successfully returned the real calendar picker; keeping all
   previous selections cleared the error and recorded a successful Sep 5 read.
   The exact cause of the initial denial is not established; do not call this a
   deliberately induced OS revocation test.
3. The isolated Swift helper created the dedicated iCloud test calendar, prepared a
   private immutable execution identity and accepted explicit event approval.
4. `create --lose-response` saved the event and exited 75 before persisting the
   returned external ID. A separate create invocation was blocked. Two fresh-process
   recoveries each returned exactly one matching URL marker/full payload, the same
   external ID, and `create_retried: false`. A later recovery also matched.
5. General app refresh still listed **10 calendars**, not 11. Reconnect listed the
   new test calendar **unchecked**. Explicitly adding it produced 11 connected
   calendars. This fails ADR 0008's automatic inclusion on explicit refresh.
6. On Sep 6, Day Canvas showed one test event at 10:00–10:15. Details showed iCloud /
   Floe Validation, Asia/Seoul, `event_kit`, Person and the external occurrence ID.
   Its item identifier matched the recovered receipt; the occurrence suffix was
   `2026-09-06T01:00:00.000Z`, local revision 0.
7. `edit-test` changed only this event's title. App refresh showed the edited title,
   unchanged external occurrence ID and local revision **1**, without a duplicate.
8. Exact-match cleanup removed this disposable event. Before refresh, the app still
   displayed its cached record. After refresh, Sep 6 showed **No saved events for
   this day**. The dedicated empty calendar remains connected for further tests.

Native AX temporarily lost Flutter semantics. Raising the window restored them;
subsequent semantic inspection worked. A quit/relaunch was attempted, but a complete
offline/revoked-cache lifecycle was not established and is not claimed.

## Acceptance disposition

| Criterion | This run | Still required |
| --- | --- | --- |
| S1-A1 | Partial: real inventory, initial typed denial and reconnect recovery | Controlled deny/grant/revoke cycle; no-selection all-calendar UX |
| S1-A2 | Partial: real timed event, timezone/Person/provenance/ID | All-day, midnight, DST, recurrence/exception identity |
| S1-A3 | **Not met:** simple edit/delete works, new calendar not auto-included | Automatic inclusion, partial-source failure preservation, recurring movement/range isolation |
| S1-A4 | Partial: retained cache on initial read denial | Verified cache across restart under revoked/offline state, signed release lifecycle |
| S3-A1–A5 | Pending: live isolated create/recovery PoC only | Approved prototype-to-native UI, trusted Rust/native binding and complete execution/error matrix |

S1 remains 0/4, S3 remains 0/5. Do not advance S1 to Verified or enable general
product writes based on this test. The Swift helper is not the Rust executor; only
the provider feasibility and real EventKit-to-existing-read-path behavior were live.
No server-side iCloud/cross-device/full-sync guarantee was measured.

## Prototype evidence

S3 UI is implemented **first in the HTML prototype**, no native UI changes.
Independent dev server: `http://127.0.0.1:5184`, loopback only; existing port 5173
belongs to another project and was left alone.

- Node 24.18.0, pnpm 10.34.5: production build and 42-entry component catalog pass.
- Pure reducer checks: 26 assertions pass (duplicate callbacks, approval/denial,
  expiry, immutable approved target, terminal states, recovery and read-only retry).
- Browser: ready creates exactly one simulated timeline event; timeout lookup
  recovers; conflict, denied, expired, missing-match and read-error paths pass.
- Approval dialog visually inspected in the real browser; it discloses the complete
  payload and simulation boundary. Native rendering is not inferred from this.
- Decline creates nothing; a fresh proposal allows destination changes; closing
  the dialog restores keyboard focus to its original Review suggestion button.

## Provider constraints from Apple

The experiment checks writable capability using
[EKCalendar](https://developer.apple.com/documentation/EventKit/EKCalendar).
Recovery does not rely exclusively on provider IDs: Apple documents that a full
sync can lose the [local item identifier](https://developer.apple.com/documentation/eventkit/ekcalendaritem/calendaritemidentifier)
and moving calendars can change the [event identifier](https://developer.apple.com/documentation/EventKit/EKEvent/eventIdentifier).
URL-marker recovery after a full sync remains an untested hypothesis, not a guarantee.
