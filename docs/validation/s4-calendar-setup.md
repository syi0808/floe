# S4 atomic Calendar Expert setup

Date: 2026-09-07. Core setup integration; S4 remains **0/14**.

## Installation boundary

`EncryptedAgentVault::install_calendar_expert` accepts a vault instance UUID,
expected registry revision, caller-generated setup UUID, provider and exact Calendar
IDs. The vault supplies the Person; callers cannot install into a different Person.
An uninitialized registry uses expected revision zero. Reading or unlocking a vault
still does not initialize the registry, install packages or create a conversation.

One setup atomically adds a fresh immutable Calendar binding, two pinned built-in
packages (unless identical versions already exist), separate Tool/Expert installations,
same-Person assignments and a durable setup receipt. The operation advances the
registry exactly once. The binding, installations and assignments all start disabled;
execution requires their separate explicit enablement. Setup does not enumerate
calendars, request OS authorization, import data, infer consent or execute an action.

Fixture and EventKit packages have distinct `floe.calendar.<provider>.timeline` and
`floe.calendar.<provider>.schedule` identities at version `1.0.0`. Their Tool output
classification is respectively Synthetic and Personal. They cannot collide with or
reclassify the existing synthetic `floe.timeline.read` / `floe.schedule` sample pair.
An existing package with the same reference but different contents fails closed.
Each new scope gets its own installations and assignments, not shared enablement.

The registry enforces its existing package/installation/assignment/View bounds plus
at most 64 setup receipts, all within the encrypted 256 KiB registry payload limit.
Calendar scopes remain one to four unique nonempty IDs of at most 512 bytes, sorted
canonically. A changed scope requires a new setup identity and current revision.

## Atomicity and uncertain responses

The registry stages and validates the complete setup before replacing its snapshot.
The vault commits all components in one immediate transaction with a revision CAS.
For a new registry, registry/Expert-receipt tables and the vault identity-version
upgrade are in that same transaction. Cancellation and protected-key checks run
before commit; failure rolls back rather than leaving a partial setup.

A caller must retain the original setup UUID, expected revision and parameters until
the result is reconciled. An exact retry, including a reordered identical Calendar
set, returns the original receipt and the current minimized registry overview without
writing. This works after reopening, unrelated configuration changes, Expert private
state advancement and scope revocation. It never re-enables, duplicates or resets a
setup. Reusing an ID with a changed scope/provider/revision/Person conflicts. A new
ID with a stale revision also conflicts. Concurrent new writes remain CAS-governed;
the caller reconciles uncertain completion using the original request, not a fresh ID.

Key loss after commit may return `VaultUnavailable` even though installation is
durable. The vault remains unavailable until explicit reopen. The same request then
recovers the receipt without another installation. Cancellation after commit likewise
does not undo the committed setup or justify a replacement identity.

Registry CAS rejects removal or replacement of existing setup receipts, appropriation
of older bindings/installations/assignments, and pre-enabled new setup components.
Restore validates receipt references, pinned package contents and the original grant
shape. Enablement and legitimately committed Expert state can change; setup identity
and its source/assignment wiring cannot be silently retargeted.

## Sample coexistence and compatibility

Calendar setup does not require a prior sample Send. If a user later explicitly sends
a sample turn, missing sample packages/assignments are added atomically while retaining
all Calendar scopes, receipts and existing state. Existing sample configuration is
never recreated or re-enabled. Partial/conflicting sample package identity fails
closed rather than being repaired by silently granting new access.

The version-1 registry snapshot adds optional `calendar_setups`, omitted when empty.
Legacy snapshots deserialize with an empty receipt list; no setup is inferred from
old grants. Older strict readers may reject the new nonempty field. No downgrade,
plaintext migration, protected-key backend change or app wire-protocol change is made.
The existing registry overview continues to omit Calendar IDs and private-state bodies.

## Verification

- Four Agent tests cover one-revision/default-off setup, explicit enablement,
  provider-pinned classification, package reuse/collision, bounded atomic failure,
  exact/changed retries, revocation/state preservation and corrupt/legacy snapshots.
- Seven encrypted-vault tests cover both sample/setup orders, disabled sample
  preservation, reopen, ciphertext canaries, pre-commit key loss/cancellation rollback,
  post-commit key-loss reconciliation, stale requests and receipt CAS protections.
- One Calendar turn test exercises both Synthetic and Personal classifications using
  fictional mirrors and injected keys/access/models. Disabled setup rejects before
  model/native dispatch; explicit enablement permits a committed Expert result and
  a Pending S3 review action. Retry preserves the advanced private state. No native
  Calendar write or live personal-data access occurs.
- All **191 workspace Rust tests**, three keyring example tests and **25 native
  assertions** pass. Formatting and Clippy with the existing Calendar exclusions
  pass; the Rust FFI library is rebuilt. All **146 Flutter tests**, analysis, the
  macOS Debug build and deep strict signature verification pass. No UI goldens change.

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

## Remaining work

This is a trusted Core setup operation, not a model capability or a user-visible
consent flow. [Native/Dart management transport](s4-calendar-management.md) now wraps
these operations. Calendar selection and confirmation UI, source-binding enablement
controls and connected chat dispatch remain.
The app still runs encrypted samples. Live protected-key provisioning, local/remote
models, other source connectors, privacy/authentication gates and S1/S3 acceptance
remain open. These tests do not promote any complete S4 acceptance criterion.
