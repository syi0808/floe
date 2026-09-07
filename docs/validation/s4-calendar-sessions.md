# S4 isolated Calendar conversations

Date: 2026-09-07. Encrypted session and native transport integration;
S4 remains **0/14**.

## Immutable conversation context

`AgentSession` now has an optional typed `scope`. The Calendar variant records an
immutable setup receipt UUID and provider. The receipt already binds the Person,
View and pinned Expert/Tool assignments to an exact Calendar scope. The provider
determines the session's single classification: Fixture is Synthetic; EventKit is
Personal. Callers cannot supply a replacement provider or classification when
creating a conversation.

Legacy sessions omit `scope`, retain their existing serialization and are not
automatically adopted into a Calendar conversation. The optional field is stored
inside the encrypted session payload; there is no plaintext pointer, separate
metadata file or database schema migration. Both ordinary session CAS and atomic
Expert/session commits reject scope changes. A scoped session also rejects a
different or widened classification.

The Calendar turn host checks the saved setup's Expert assignment, View and data
classification before dispatch. A caller cannot switch a scoped conversation to a
different granted Expert simply by supplying another otherwise valid invocation.
Existing unscoped Core fixtures retain their prior contract; the new native
Calendar session API requires explicit scoped sessions.

## Start, resume, get and recover

The encrypted vault creates a new empty Calendar session only for a recorded setup
receipt. Start does not enable its packages, assignments or View. Creation checks
cancellation and protected-key access before transaction commit, rolls back staged
insertion on failure, and checks access again before returning. A response lost
after commit is reconciled by Resume; blindly repeating Start is not an
exactly-once creation promise across process restarts.

Resume selects the latest Calendar session for the requested setup, not the latest
session for the Person. If none exists, it creates an empty scoped session. Get
requires a Calendar-scoped session and verifies the original receipt/provider.
Older conversations remain addressable by UUID. Existing sample resume excludes
scoped sessions, while sample get/turn/recovery refuse to adopt them, including
Synthetic Calendar conversations. Legacy unscoped Personal sessions remain outside
the sample route and are not relabeled or migrated into a setup.

Recovery requires the current session revision. It preserves every committed
message and the immutable scope, clears only an abandoned active-turn pointer and
records Interrupted. An already settled session is returned unchanged. It invokes
no model, Expert or Calendar provider and never publishes or retries a proposal.
Revoked/disabled setup components do not erase readable history or prevent this
local recovery; subsequent turns still require current grants and freshness.

The owned native vault job exposes a separate `calendar_session` operation with
Start/Resume/Get/Recover. It accepts opaque setup/session references and an expected
revision only, not prompt text, classification, provider, credentials or grants.
Duplicate submissions retain the same owned result and the existing job boundary
prevents recovery while another job owns the vault. Dart parses the typed scope,
validates result ownership/setup identity and recovery revision/provider, and
keeps this API separate from the current sample controller.

## Evidence

- Five encrypted-vault tests cover separate setup/sample resume, process-handle
  reopen, immutable scope/classification, refused legacy adoption, sample dispatch
  rejection, recovery without replay, cancellation, missing setup, latched key loss
  and ciphertext checks for the setup ID and a synthetic private-message marker.
- Core tests additionally cover scope changes through atomic Expert commits,
  rollback after the post-insert key check fails, and a mismatched scoped Expert
  being rejected before any model or Calendar access.
- A native worker test covers typed Personal empty-session creation using injected
  keys and a synthetic EventKit-shaped setup, duplicate submits, sample separation,
  unlock/resume, Person confinement and unchanged disabled registry state. The
  proposal-worker integration test now commits its synthetic Calendar turn through
  a scoped session before inspecting the saved S3 action.
- A protocol test rejects injected prompt/model/provider/classification/grants and
  verifies that legacy session serialization does not gain a null scope field.
- Four Dart tests cover scope/classification decoding, reference-only jobs, foreign
  responses, recovery revision/provider and rejection by the sample controller.
  The existing real Dart/C ABI test verifies that Calendar Resume with no open
  vault fails without creating storage or requesting keys.

All **216 workspace Rust tests**, three keyring example tests and **25 native
assertions** pass. Rust formatting, Clippy with the existing Calendar exclusions,
all **189 Flutter tests**, Flutter analysis, the rebuilt Rust FFI library, macOS
Debug build and deep strict signature verification pass. Existing UI goldens are
unchanged; no live key, model or source gate was exercised by these tests.

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

This provides native storage operations, not native Calendar model execution or
a connected-chat panel. No arbitrary user text or live Calendar source is read by
these operations. Personal classification on an empty session is not evidence that
the signed key/lifecycle or local-model gates passed. The Foundation Models adapter
remains SyntheticOnly, and the normal app still presents encrypted sample chat.

Next: dispatch the Core Calendar turn through the owned native worker using the
saved scope, construct bounded leases from current connection state, await Manager
proposal preparation after the conversation finishes, and connect the appropriate
session/controller UI without weakening model/key/privacy gates. Other S4 sources,
live model/auth/privacy validation and S1/S3 acceptance remain outstanding.
