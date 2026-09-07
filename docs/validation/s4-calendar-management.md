# S4 Calendar Expert management transport

Date: 2026-09-07. Native/Dart management integration; S4 remains **0/14**.

## Explicit management contract

The encrypted vault now exposes `calendar_expert_overview`: the minimized registry
overview plus Person-scoped Calendar bindings and immutable setup receipts. Unlike
the generic registry overview, this dedicated management response intentionally
includes exact Calendar IDs so a consent screen can inspect existing grants. It does
not contain event records, credentials or private-state bodies and is not a model
capability. Generic registry responses still omit source IDs and View handles.

Inspection of an uninitialized registry returns its vault instance UUID and revision
zero with empty collections. It does not create tables, bootstrap packages, create
a session or request source access. This gives setup callers a revision/instance
boundary without requiring an earlier sample Send. Protected-key access is checked
before returning either empty or populated results.

The version-1 vault protocol adds `calendar_experts` with optional `setup`. A null
setup inspects; a supplied `CalendarExpertSetup` calls the existing atomic default-off
installer and returns current management state. The response's optional
`calendar_experts` field is omitted for existing operations, preserving their wire
shape. A `calendar_view` registry configuration target changes only an existing
Person-scoped binding's enablement under the current instance/revision CAS. It cannot
retarget scope, replace grants, change provider classification or enable installations
and assignments implicitly. Existing S3 approvals are not retracted by revocation.

The same native submit/poll/stop/release worker owns all operations. Locked or foreign
Person access fails, duplicate owned-job submissions return the owned result, and
an OS key call that is still blocked retains the worker slot after Stop. A new setup
job after release/restart uses the durable setup receipt rather than job identity to
reconcile installation. Key-unavailable/interrupted failures close the vault; other
configuration errors do not silently reopen or replace it.

## Dart gateway and controller

`AgentCalendarExpertGateway` supports read and installation with a caller-retained
`AgentCalendarSetup`. Typed parsing validates bounded/canonical scopes, owner/instance,
receipt uniqueness and links, pinned package identity/version, and installation and
assignment grant counts. Scope ordering matches Rust's UTF-8 byte ordering, including
non-BMP identifiers; invalid UTF-16 that would be replaced during encoding is rejected.
IDs, revisions and consent parameters are immutable once the request is constructed.

The native gateway drains/releases an uncertain owned job before starting another.
Installation retries retain the exact setup UUID, original expected revision and scope;
the native transport may use a new job UUID, but never a replacement setup identity.
A refresh can reconcile an accepted setup without submitting it again. A missing or
mismatched receipt, wrong owner/instance, malformed management state or non-ready vault
does not become successful configuration. Exact retries may report a newer current
registry revision after explicit enablement/revocation or private-state advancement.

`AgentController` serializes setup, binding configuration, registry operations and chat
with the same busy/lifecycle ownership. Installation and enablement are non-optimistic.
After a lost setup response the controller retains the original intent for explicit
retry or refresh, not a second setup with altered scope. An uncommitted intent can be
discarded only after successful same-instance inspection finds no receipt. Binding
enablement waits for confirmed current-state readback; a lost toggle reply is reconciled
by reading, not automatically replaying the mutation. Generic registry changes
invalidate cached Calendar management state.

Lock/close clears presented source scopes and pending consent immediately, ignores late
results and waits for the owned operation before locking the vault. Closing does not
undo a setup that already committed. Key failure also clears conversation/source state;
there is no plaintext pending-request journal or implicit replay after restart. A later
unlocked management read lists durable receipts for review. No personal-chat route,
OS consent prompt or source enumeration is introduced by this transport increment.

## Evidence

- One protocol test covers explicit setup/enablement shapes, rejection of injected
  keys/state/owner/grant fields and unchanged serialization of legacy vault responses.
- One Core test covers read-only revision-zero inspection, scoped revision-checked
  binding enablement, unchanged assignments/receipts and key-unavailable inspection.
- Two native-worker tests cover locked/foreign access, duplicate submits, durable
  replay after worker restart and revocation, changed-intent conflict, key failure,
  blocked-key Stop/release ownership and cancelled setup without initialization.
- Seven Dart gateway/model tests cover immutable bounded Unicode scope intent,
  malformed/foreign/link-mismatched responses, empty inspection, exact retry, explicit
  scope toggles and lost submit/poll/release reconciliation.
- Six controller tests cover serialized non-optimistic setup, same-intent retry/read
  reconciliation, explicit abandonment after refresh, confirmed scope toggles,
  lock during read/install/configure, discarded late results and fail-closed key loss.
- All **195 workspace Rust tests**, three keyring example tests and **25 native
  assertions** pass. Formatting and Clippy with the existing Calendar exclusions
  pass; the Rust FFI library is rebuilt. All **159 Flutter tests**, analysis, the
  macOS Debug build and deep strict signature verification pass. UI goldens are unchanged.

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

These controller methods now have a [Calendar access consent screen](s4-calendar-consent.md)
under Tools & Experts, using selected same-Person Calendar metadata, explicit bounded
confirmation and saved-scope enablement without automatic expansion. Native connected
chat dispatch, live protected-key/model/source gates, other connectors, remote privacy
and authentication, and S1/S3 acceptance remain open. No S4 acceptance condition is
promoted by these fictional/injected tests.
