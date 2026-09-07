# S4 vault host and assistant integration

Date: 2026-09-07. Automated evidence uses synthetic conversations and test keys.

## App behavior

The default `PersonalDayScreen` uses `FfiDayGateway.secureAgent`, not the legacy
plaintext fixture gateway. Before storage is unlocked, opening the panel only
checks filesystem availability: it does not create a directory, provision a key
or request key access. An already-ready handle rechecks its key. The user
explicitly chooses **Set up secure storage** or **Unlock conversation storage**.
Ready storage permits the same fixed sample questions, now backed by the encrypted
vault and the existing bounded runtime. No free-text input, real model or source
collection is enabled by this change.

Storage is under `<core database path>.agent-vaults/<Person UUID>`. Existing
plaintext fixture records are not imported. A locked/unavailable store never
falls back to them. Other non-Agent Calendar/task operations keep their existing
storage and execution behavior.

New/resumed conversations, committed events, Stop, retry and explicit interrupted
recovery use the encrypted session store. Closing the panel, changing destination,
disposing the controller or receiving a non-resumed Flutter lifecycle event seals
the controller: messages/session references are cleared immediately, active work
is cancelled/drained, and the vault is locked. Late responses cannot republish
messages into a sealed view. Resume after locking requires explicit unlock.
This verifies Flutter lifecycle handling, not every physical macOS screen-lock or
fast-user-switch notification; the native lifecycle matrix remains open.

## Native ownership and protocol

`floe_core_agent_vault` uses a version-1 Person/request UUID envelope. Actions are
Status, Create, Unlock, Lock, or a preset-only session operation. Transport commands
are Submit, Poll with an event cursor, Stop and Release. Key bytes, filesystem
paths, arbitrary prompts, inference policy and provider credentials are not request
fields. Invalid payload errors are sanitized rather than echoing rejected text.

The lazy worker has one bounded command slot and one retained job. The job's
request ID, Person and action bind duplicate Submit to the original operation.
A conflicting operation cannot replace it, and Release cannot discard unfinished
work. Poll replays committed/progress events without consuming them. Event storage
is bounded to 64 records. Same-host recovery cannot race a running turn because
it cannot acquire the occupied job slot. The vault's lifetime file lock supplies
the cross-host ownership boundary.

All OS key calls, Turso work and sample runtime calls execute on a dedicated
native thread with its own Tokio runtime. The existing Dart FFI isolate only
submits work and copies snapshots. A blocked key-store call therefore does not
hold the Calendar worker or block Poll/Stop/native-handle destruction.

Stopping is cooperative: an OS key call itself is not forcibly interrupted.
Closing a native handle signals cancellation/shutdown without joining a stuck
worker. The worker retains its vault/lock until the OS call actually returns and
the worker exits. A new host cannot steal that lock or declare the old turn
abandoned merely because observation timed out.

The Dart gateway polls batch operations at 80 ms with a 35-second monotonic
observation limit. Timing out retains the pending request identity. A later read
reconciles that job instead of issuing a second create/turn; a lost Release reply
is reconciled through a NotFound lookup. No operation is silently resubmitted as
a new job after uncertain completion.

## Evidence

Recorded results: 96 Rust tests and 129 Flutter tests pass. Flutter analyzer,
native library build, formatting and Clippy with the existing exclusions pass.
The macOS Debug app builds, includes the vault C ABI export and passes deep/strict
signature verification. The app was not launched for live key access in this checkpoint.

- Worker tests: setup/reopen/resume, wrong-Person access, key loss, explicit unlock,
  cancellation, event replay, live recovery exclusion and typed future-cursor error.
- The sample transport rejects reading/resuming/recovering non-synthetic sessions;
  an encrypted store alone does not enable a personal-data route.
- A controllably blocked key provider proves Poll, Stop and handle close remain
  responsive; the file lock remains held until the blocked worker really finishes.
- Rust C ABI and real Dart/native tests report Missing without provisioning or
  accessing the OS key store. Session access while locked fails closed.
- Dart gateway fixtures exercise lost Create and Release responses and mismatched
  request identities without duplicate key provisioning.
- Controller tests exercise explicit setup/unlock, immediate sealing during a
  turn, delayed-unlock completion and message clearing after a key error.
- Widget tests exercise setup/lock at 320/390 widths with 200% text and inactive
  app lifecycle clearing. `apps/client/test/goldens/agent_vault_setup.png` is rendered
  and visually reviewed using the bundled fonts and shared controls.

```sh
CARGO_INCREMENTAL=0 cargo test --workspace
CARGO_INCREMENTAL=0 cargo build -p floe-ffi
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- \
  -D warnings -A clippy::too_many_arguments -A clippy::collapsible_if
cd apps/client
flutter analyze
flutter test
flutter build macos --debug
```

Clippy exclusions remain the two pre-existing Calendar warnings.

## Remaining S4 work

The subsequent [signed keyring smoke](s4-keyring-live-smoke.md) attempted real
provisioning: the read-only probe completes but creation fails, and exact cleanup
reports a missing entitlement. Its retained disposable markers need cleanup with
a correctly provisioned host. It does not establish key-store acceptance.

This is integration evidence, not P0-F or S4 acceptance. A development signing
identity was confirmed available; no real protected-store item was created or
read by the automated checks at this earlier checkpoint. Next, validate real key access/reopen from the
signed host, OS lock/denial/key-loss behavior, crash recovery and precise
disposable cleanup. Failed provisioning/repair/deletion and key-orphan handling
still need an explicit product path before personal dogfood.

Then connect supported local/remote generative adapters, the privacy/transfer and
authentication gates, Expert registry/assignment and bounded View contracts,
real connectors, Today briefing and the S3 proposal path. S4 remains **0/14** and
depends on S3 Accepted; no Memory, voice or cross-device work is counted here.
