# S4 encrypted Expert registry and private state

Date: 2026-09-07. Synthetic data only; S4 remains **0/14**.
This supersedes the secure sample path's per-turn registry limitation recorded in
[the Expert foundation checkpoint](s4-expert-foundation.md).

Follow-up: the [Manager action bridge](s4-manager-actions.md) consumes committed
proposal evidence and adds an append-only guard to ordinary vault session CAS.
Its Core/S3 integration does not yet enable live proposals from the app.

## Storage and initialization

Tool/Expert packages, installations, Person assignments, grants and private state
now share the existing encrypted Agent session database. The local registry
instance is the vault UUID; every assignment belongs to the vault's Person.
This is a single-Person, vault-local host namespace, not shared multi-Person or
cross-device installation storage. Registry JSON is bounded to 256 KiB and checked
against the common versioned snapshot contract on every restore.

Opening a recognized version-1 vault is read-only with respect to this component.
The first valid, explicitly submitted synthetic sample turn initializes the fixed
Tool/Schedule registry, receipt table and identity version 2 in one transaction.
Status, unlock, session creation and invalid/stale submissions do not seed it.
Initialization neither provisions a replacement key nor rewrites existing sessions.
Version 2 requires a valid registry and receipt schema; missing or malformed state
fails closed rather than silently recreating assignments or resetting counters.

Configuration saves use the expected global revision. They cannot rewrite private
state, remove existing assignments/packages/installations, replace a pinned package
version, or rebind an assignment's Person/installation. New assignments start with
empty private state. The default panel does not yet expose configuration controls.

## Atomic result boundary

The secure sample runner restores existing assignment and View IDs for each turn.
It checks the encrypted authoritative registry revision before Expert dispatch and
again when publishing results. A concurrent configuration change rejects the stale
turn; if its terminal update also conflicts, explicit interrupted-session recovery
is required. There is no automatic replay under newly changed permissions.

Each completed Expert result is validated against its Person, invocation, package,
assignment, grants and expected state transition. One SQL transaction then writes:

- The paired capability message and session revision.
- The matching registry/private-state revision.
- A durable invocation receipt that rejects older duplicates, not just the latest ID.

Existing message history and data classifications cannot be removed by this path.
An Expert draft abandoned before its result is committed does not advance durable
state. A later cancellation or interrupted-session recovery preserves already
committed Expert evidence without invoking it again. The receipt index is not a
full trace/replay archive and does not implement retention or compaction yet.

Key access is checked before and around transaction completion using the existing
key provider; failures latch the vault unavailable. Pre-commit failures roll back
all three records. If key access fails after commit, the caller receives unavailable
and must explicitly reopen/reconcile: the transaction may already be fully durable,
but there is no partial registry/session success or plaintext fallback.

The default native vault worker and signed smoke harness use this persisted runner.
The legacy plaintext synthetic test route intentionally keeps its ephemeral registry;
it is neither a migration path nor a fallback for the secure panel.

## Automated evidence

Eleven new encrypted-store tests use an injected in-memory key provider and the actual
pinned Turso engine. They cover:

- Stable IDs and accumulated private state across new chats, reopen and checkpoint;
  synthetic registry/result markers absent from database/WAL bytes.
- Explicit version-1 initialization, rollback after schema writes on key failure,
  missing-component rejection, configuration CAS and immutable namespaces/state.
- Injected storage failure and dropped future after registry/receipt writes, before
  session commit: old session/state remain intact, no receipt leaks, retry succeeds.
- Key loss before commit, post-commit key loss with complete-transaction reconciliation,
  persistent invocation replay rejection, abandoned drafts and recovery without re-execution.
- Configuration revocation during model work preventing Expert success publication.

An additional common-registry test reconstructs a validated result's exact state
transition and rejects malformed results without changing state. A native worker
test locks/drops the host, unlocks in a new host and starts a new chat while retaining
the same assignment and advancing its existing state. All **121 workspace Rust
tests** pass; the keyring example's three cleanup-confinement tests are separate.

All **133 Flutter tests** and the analyzer pass against the rebuilt native library.
Formatting and Clippy pass with the two existing Calendar exclusions. The macOS
Debug app builds and passes deep/strict signature verification. No UI changes or
new screenshot/golden acceptance are claimed for this storage-only increment.

These tests do not prove production OS-key provisioning, physical device lock,
process-kill/power-loss durability, real model behavior or live Connector permission
revocation. Dropping an async transaction is not a substitute for a crash matrix.

```sh
CARGO_INCREMENTAL=0 cargo test --workspace
CARGO_INCREMENTAL=0 cargo test -p floe-core --example vault_keyring_smoke
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments -A clippy::collapsible_if
```

## Remaining acceptance work

Registry configuration/trust/permission management in the host/app, explicit package
upgrade lifecycle, Manager conversion of focus advice into S3 review/action intents,
Communication/Health Experts and live bounded Views remain open. So do trace/replay,
real-model evaluation, supported remote inference and all S1/S3 prerequisites.

The [signed-key entitlement failure](s4-keyring-live-smoke.md) is unchanged; the
updated smoke harness now also compares the registry on reopen but has not passed
that production gate. [Native generation](s4-local-model.md) remains unverified
while Apple Intelligence is disabled. No personal chat or live source is enabled.
