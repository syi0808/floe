# S3 approved Calendar action — prototype first

2026-09-05. User direction: implement/review UI in the HTML prototype before
porting to Flutter. This is an interactive design reference, not live execution.

2026-09-06: [Native execution](../validation/s3-native-executor.md) binds the
Flutter review to the durable ledger. A later product decision replaces this
Calendar-specific surface with shared Review and Activity, enables the trusted
executor in Release, and makes runtime Action Authority the authorization gate.
Debug builds remain write-disabled unless explicitly enabled.

## Decision-first review

Review is a decision surface, not a ledger inspector. Its first screen answers:
**what will change, where, when, and what will not change**. Show the event title,
human-readable destination, date/time, and the one-event scope. Keep guests,
alerts, recurrence and effects on existing events explicit. Reducing noise must
not reduce the user's understanding of what they authorize.

The prototype and Flutter display both endpoints in the device's local time. Full
dates avoid ambiguity for overnight events. The user neither enters nor reviews a
timezone, offset or UTC string. This is presentation only: approval retains the
original immutable UTC instants and internal scheduling metadata, never a reparsed
display string.

The planning dialog accepts `yyyy-MM-dd HH:mm` in device-local time. Destination,
title, start and end fields use a consistent 12px vertical gap; explanatory copy
and the submit action use the larger section spacing. Local input is strictly
validated before conversion to UTC at the FFI boundary.

Provider codes, Person/calendar/proposal/execution IDs, local approval timestamps
and external IDs belong in collapsed **Technical details**. Raw UTC timestamps and
timezone metadata stay internal rather than appearing even in this disclosure.
They remain selectable for support, but are not prerequisites for a decision.
Block reasons are translated into ordinary language outside that disclosure.
Unknown outcomes, permission/conflict failures and created-but-not-collected
states remain visible, alongside the correct lookup-only or read-only recovery.
Closing review remains neither consent nor rejection. Simplification does not
change expiry, fresh validation, write gating or duplicate suppression.

Today includes a quiet focus suggestion beside the timeline. Review opens the
existing accessible dialog shell with explicit destination (writable fixture
calendars only), title, local date/start/end and no guests/alerts.
Destination changes produce a new proposal revision before approval. Decline does
not create an event; closing the dialog is not approval or rejection.

Approval shows revalidation, create and re-import as separate steps. The created
event appears in Today only after simulated re-import. Closing/reopening or
navigating during execution does not restart its request. All terminal and
ambiguous states remain accessible through the card; never rely only on a toast.

## Review scenarios

Use `/?action=ready`, `conflict`, `denied`, `expired`, `timeout`, `missing`, or
`read-error` and click **Review suggestion → Approve & create**.

- `ready`: checks → create → re-import → one timeline event.
- `conflict`, `denied`, `expired`: no create; request/review a fresh proposal and
  explicitly approve again. The fresh fixture simulates the prerequisite resolved.
- `timeout`: ambiguous result; **Check Calendar for this event** only looks up the
  original execution, then re-imports its matched receipt.
- `missing`: lookup cannot confirm a match; keep unresolved and require inspection
  of the original calendar. No create/retry/replacement button is offered.
- `read-error`: create succeeded, collection failed. **Retry Calendar read** never
  sends another create.
- Decline: terminal no-write state with an explicit fresh-proposal path.
- Non-current fixture day hides the proposal; stale/disconnected days cannot approve.

The reducer enforces allowed transitions and ignores duplicate clicks/out-of-order
callbacks. Approval expires after 15 real minutes in the browser; the displayed
event date is the existing fixed Sep 4 fixture, not a real scheduling recommendation.
No model, OS settings operation, EventKit call or real persistent ledger is involved.
Reload resets the simulation. Native durable restart behavior is tested separately
in Rust; it is not established by this prototype.

## Validation boundary

The simplified review is implemented in the prototype and Flutter. Production
decisions/state remain bound to the Rust ledger, not this simulated reducer.
Release includes Calendar create while Debug remains write-disabled by default.
This presentation change does not
advance the remaining live acceptance or dogfood gates recorded in
[S3 native validation](../validation/s3-native-executor.md).
