# S3 native action review checkpoint

Date: 2026-09-06. Scope: Flutter review of the existing Rust action ledger.

The Today context rail now loads Person-scoped proposals for gateways implementing
`CalendarActionGateway`. Opening Review reloads the ledger, then shows the immutable
destination/provider/ID, title, start/end, timezone, Person, expiry, proposal and
execution IDs, approval time and any recorded external ID/reason.

This ports the [HTML review reference](../design/s3-calendar-action-ui.md), not its
simulated reducer or execution progress. The existing prototype/live PoC evidence
does not establish S1 Verified or authorize product write rollout.

## Deliberate interim behavior

- **Save approval only** persists a decision through the existing FFI bridge.
  The UI explicitly states that writing is disabled and no event is scheduled or
  queued. No executor is called on approval, mount, navigation, reload or relaunch.
- No model or fixture proposal is fabricated in the production app. A database
  without proposals shows an empty state; there is no proposal editor/producer yet.
- The ledger remains inspectable across day navigation, including terminal and
  ambiguous states. Approval requires viewing Today, a ready connection, matching
  provider/calendar/revision, successful target read and an unexpired proposal.
  Rejection remains possible when disconnected. Rust remains the decision authority;
  these UI guards are not substitutes for trusted native execution preflight.
- Time bounds are explicitly labeled **UTC**, with the proposal IANA timezone
  separately shown. No incorrect local-zone conversion is inferred. Target-zone
  formatting and a proposal editor remain future UI work.
- A controller owned by the screen survives internal navigation and dialog closure.
  Duplicate decisions are suppressed. Failed/ambiguous saves disable decisions
  until a read confirms the durable state; reload never submits a decision.
- Pending approval expiry is checked on click and reflected every second while
  the dialog is open. Close/Escape is neither approval nor rejection.
- Approved, rejected, blocked, executing, unknown and succeeded remain visible.
  Executing/unknown require provider inspection; there is no create/replacement
  or pretend-recovery button. Success distinguishes a recorded external creation
  from successful collection into Today.

## Automated evidence

```sh
cd apps/client
flutter gen-l10n
flutter analyze
flutter test
flutter build macos --debug
```

Flutter analysis is clean; all 66 tests pass, including eight new controller/widget
tests. Formatting and the macOS debug application build pass. The built app was
not launched against the user's live calendar as part of this validation.

`calendar_action_ui_test.dart` covers 390/1200-pixel layouts, payload disclosure,
Close/Escape, approval and decline, duplicate dispatch suppression, closure during
a pending decision, response loss followed by read-only reload, owner disposal,
foreign Person rejection, expiry and changed/disconnected/denied connections, and
all terminal/recovery states. Existing real-FFI tests cover persistence/reopen.

Widget rendering/keyboard checks are automated, not a live signed-app UX acceptance.
No real calendar was read or written for this checkpoint. S3 remains 0/5.
Next: proposal production/editing and trusted native preflight/create/lookup binding,
then read/recovery UI and controlled live validation after S1 verification.
