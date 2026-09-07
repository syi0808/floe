# S4 read-only Expert proposal inspection

Date: 2026-09-07. Core proposal recovery boundary; S4 remains **0/14**.

## Recovery is not publication

`FloeCore::inspect_expert_calendar_action` accepts an encrypted vault and an
`ExpertCalendarInspection`: a Person/session/invocation reference, cancellation and
a deadline capped at 30 seconds. It reads an already committed Expert proposal and
looks up its existing S3 action. It never starts a model, reads a Calendar provider,
publishes a new action, approves/rejects an action, looks up an OS execution receipt
or invokes the executor. A valid proposal with no matching Person-scoped action
returns `None`; this is not permission to create a replacement or rerun the turn.

The existing Manager publication API still requires current enabled grants, Calendar
binding authorization and source/destination revision checks. Inspection instead
authenticates historical evidence so a user can recover an existing action after
assignment/scope revocation, source selection changes or restart. Revocation blocks
new use; it does not erase previously prepared actions or user decisions.

## Evidence and action linkage

The vault requires one successful paired capability output for the invocation and
exactly one explicit action proposal. The output is bounded to 16 KiB. Its durable
Expert receipt must identify the same session and assignment, with a positive commit
revision no newer than the current encrypted registry. The session must retain its
recorded data classification. Existing session-prefix protection prevents replacing
historical output through ordinary session CAS. A copied output in a new session,
a new invocation without a receipt or a foreign Person does not authenticate evidence.

`AgentRegistry::validate_historical_result` checks immutable package/assignment
identity, the pinned Tool's output classification, bounded result content and recorded
private-state revision without requiring current enablement or reproducing old grants.
This validator alone is not a receipt proof and must not authorize invocation or
publication. The vault uses it only after durable receipt checks in the separate
read-only inspection branch. Existing live `validate_recorded_result` and invocation
validation still enforce current grants; they do not use historical validation.

The Core lookup is scoped by Person and invocation UUID. A dedicated SQL read checks
the S3 payload's byte length before JSON decoding, rejecting records larger than
64 KiB rather than treating them as absent. Parsed row identity must match the lookup.
The action's Agent origin must match vault instance, session, invocation, assignment,
package, View, private-state revision and data class. It must be a non-direct Calendar
create with the fixed Focus time title and exact proposed UTC interval. Its expiry
cannot exceed the evidence expiry; Calendar-origin evidence must also match the
action's recorded connection revision. Mismatches in these evidence-bound fields
conflict instead of being attached to the conversation or replaced.

Destination Calendar and recorded approval/execution details come from the existing
canonical S3 action ledger, not an inferred destination from today's connection. The
inspection does not claim to reconstruct an unpersisted destination or authorize that
destination now. Returned action metadata excludes conversation text, source event
titles, raw Expert results and private-state bodies.

## State and failure behavior

Pending, Approved, Executing, Unknown, Succeeded, Rejected and Blocked records are
returned unchanged, including original execution identity, approval, expiry and
destination. Inspection does not expire a Pending record, resurrect a rejected one,
retry an Unknown write or refresh historical evidence. Consumers must use existing
S3 review/execution/recovery commands for any explicitly requested state transition.

Vault inspection holds a read-only immediate transaction across receipt validation
and action lookup, checks protected-key access before and after the lookup and again
on transaction completion, and rejects cancelled/deadline-expired completion. It does
not provide an atomic snapshot across the encrypted vault and separate Core database;
the returned S3 record can change later and is not a future execution authorization.
Key failure latches the vault unavailable until explicit reopen. Failure or dropped
inspection cannot publish or mutate either session/registry state or the action ledger.

## Automated evidence

- One Agent test validates historical provenance after disablement/grant replacement
  without restoring permissions, and rejects mismatched identity, package, class,
  private-state revision and malformed result content without advancing state.
- Six Core inspection tests cover absent/prepared actions, reopen, revocation/source
  changes, all recorded S3 states, stable execution identity, copied/uncommitted/foreign
  receipts, corrupted/oversized action records, cancellation, deadline and key loss.
  The state matrix uses deliberately stored synthetic records, not real OS executions.
- One connected Calendar turn test covers both Synthetic and Personal classifications
  with injected models/keys/access: revoking a real bound scope still permits inspection
  of its Pending S3 record, while the live publication path rejects the same reference.
- All **203 workspace Rust tests**, three keyring example tests and **25 native
  assertions** pass. Formatting and Clippy with the existing Calendar exclusions
  pass; the Rust FFI library is rebuilt. All **169 Flutter tests**, analysis, the
  macOS Debug build and deep strict signature verification pass; UI goldens are unchanged.

```sh
CARGO_INCREMENTAL=0 cargo test --workspace
CARGO_INCREMENTAL=0 cargo test -p floe-core --example vault_keyring_smoke
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments -A clippy::collapsible_if
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo build -p floe-ffi
bash tools/s3-validation/check-native.sh
cd apps/client
flutter test
flutter analyze
flutter build macos --debug
codesign --verify --deep --strict build/macos/Build/Products/Debug/floe_client.app
```

## Remaining integration

The follow-up [saved proposal presentation increment](s4-proposal-presentation.md)
connects this API to an owned native vault job, a conversation card and the existing
S3 review dialog. Native connected-turn dispatch and Personal session loading remain.
No automatic retry or plaintext recovery journal is added. The app still runs
encrypted samples, and live key/model/source/privacy/auth gates, other S4 connectors
and S1/S3 acceptance remain unverified. These tests do not promote a complete S4
acceptance criterion or enable personal-data dogfood.
