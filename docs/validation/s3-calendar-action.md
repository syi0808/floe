# S3 Calendar action — executor checkpoint

Date: 2026-09-05. Integration mode: Rust fixture only.

2026-09-06 follow-up: [Dart decision/ledger bridge](s3-action-bridge.md) adds
proposal/list/get/decision APIs, without exposing execution or live writes.
The no-FFI statements below describe the original executor checkpoint.

Follow-up: [isolated live EventKit PoC](eventkit-live-poc.md) validates one disposable
create/response-loss recovery and re-import through the existing app. The Rust
executor itself remains fixture-only. UI work starts in the
[action-review contract](../design/s3-calendar-action-ui.md); current review happens
in Flutter.

## Delivered boundary

`floe-core` owns an immutable calendar-create proposal, explicit approve/reject
decision, and durable execution ledger in `calendar_actions` (migration 5).
The proposal includes Person, provider, explicit calendar ID/name, title, UTC
interval, timezone, connection revision, creation/expiration, approval timestamp,
proposal ID, execution ID, typed state, and successful external ID.
Proposals expire after 15 minutes or at their start, whichever comes first.

Transitions:

```text
Pending -> Approved | Rejected | Blocked(expired)
Approved -> Executing -> Succeeded | Blocked | Unknown
Executing | Unknown -> lookup only -> Succeeded | Unknown
```

Decisions and execution claims use payload compare-and-swap in Turso. A failed
claim cannot dispatch a create. A failed final ledger write leaves `Executing`,
not an approval that can be retried. `Blocked` requires a new proposal and explicit
approval. `Unknown` and interrupted `Executing` never automatically create again,
even if lookup returns no match. An empty or ambiguous lookup is not proof of
absence; recovery requires user inspection if an exact match cannot be established.

Execution checks a trusted policy snapshot (Person, provider, target allowlist and
create permission), connection revision/error, and expiry. The provider must
perform fresh permission, writable capability, timezone and conflict checks.
Preflight receives current Person-scoped local events as well as the proposal;
local events and the connection revision are checked again after preflight.
Expiry is also checked immediately before dispatch using the supplied clock.
Production callers must use a real clock, not a model- or UI-provided timestamp.

Receipts and lookup results must match execution ID, Person, provider, calendar,
title and interval/timezone and contain a nonempty external ID. Recovery accepts
exactly one matching receipt. Any create error is conservatively ambiguous,
including permission errors after dispatch. The ledger is independent of mirror
import: a subsequent read/import failure must not cause the create to repeat.

## Adapter obligations before live enablement

- Implement the provider inside a trusted native boundary, never a model tool.
  The Rust trait is an integration contract, not an OS sandbox or an authorization
  boundary against malicious Rust code in the same process.
- Obtain current policy from the trusted application, bind the native account to
  the Person, and resolve the exact writable calendar immediately before saving.
- Preflight must fetch current external events across the connected scope and
  check those plus non-deleted local events. Resolve IANA timezone, DST, all-day
  dates and recurrence correctly. These behaviors are **not** validated by the
  timed-event fixture. Provider-side changes can race a preflight; the native
  create implementation must recheck as close to save as its API allows.
- Preserve/recover the execution marker in provider data if supported. Never
  infer success from a title-only or interval-only match. If reliable lookup is
  unavailable, keep `Unknown` and ask the user to inspect the provider.
- Bound native calls with timeouts and route cancellation/restart to lookup.
- Re-import successful results through the existing Calendar read path, retaining
  the successful execution status if re-import fails. Show retry-read separately
  from create and retain proposal-to-external-ID traceability.
- Add app review/approval/rejection, state reload after relaunch, and recovery UI.
  Never accept approval, policy grants or execution requests from model output.

## Automated evidence

Commands:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

35 Rust tests pass, including 11 new `calendar_action` tests. Clippy is clean.
Coverage includes:

- No dispatch before approval, after rejection, or for another Person.
- Durable approval/result, stable execution ID and idempotent receipt re-import.
- Concurrent execution claims dispatch only once.
- Policy denial, changed calendar, permission/capability denial, invalid timezone,
  schedule conflict, provider unavailability and expiry after preflight.
- Current local events are supplied for conflict checking.
- Timeout/permission/provider errors after create recover by lookup after reopen.
- Cancellation after an external write retains `Executing` across reopen and
  recovers without dispatching again.
- Missing, duplicate or mismatched recovery receipts remain `Unknown`.
- Invalid target, empty title and malformed interval are rejected.

No Flutter/FFI API or EventKit write operation is added. No live Calendar was
read or modified, no OS permission was requested, and no model was invoked.
S3-A1–A5 remain pending (0/5): fixture evidence is not live acceptance.
S1 Verified and provider create/recovery PoC remain prerequisites to live rollout.
