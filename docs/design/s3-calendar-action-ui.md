# S3 approved Calendar action — prototype first

2026-09-05. User direction: implement/review UI in the HTML prototype before
porting to Flutter. This is an interactive design reference, not live execution.

Today includes a quiet focus suggestion beside the timeline. Review opens the
existing accessible dialog shell with explicit destination (writable fixture
calendars only), title, date, start/end, timezone, Person and no guests/alerts.
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

## Next native gate

Review desktop/narrow layout and keyboard flows here first. Only after that review
and EventKit create/recovery PoC should these components be translated to Flutter,
binding decisions/state to the Rust ledger rather than reproducing this reducer as
production authority. Native read-only disclosure must be updated when live writes
are actually introduced; the S1 connection fixtures remain read-only references.
