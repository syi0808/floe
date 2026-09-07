# S4 saved proposal inspection in the app

Date: 2026-09-07. Native and Flutter integration; S4 remains **0/14**.

## Read-only transport

The owned native vault worker now accepts `inspect_proposal` with a session UUID
and invocation UUID. Person ownership comes from the outer job, never an embedded
Expert output. It invokes the existing Core inspection API with job cancellation
and a 30-second deadline. The worker retains the already opened Core through an
`Arc`; inspection does not open or migrate a second Core database.

The optional versioned `proposal` result contains Person/session/invocation identity
and either an explicit `action: null` or the saved action UUID, execution UUID,
status and expiry. It omits Calendar IDs/names, native receipts, private Expert
state and raw source/conversation content. Old result shapes omit this field.
Failures do not masquerade as absence. Locked storage does not provision a vault,
access a source, or offer a plaintext fallback.

Duplicate submits retain the same owned result. Stop cannot release a job while
protected-key access is still blocked. Core evidence and action provenance checks
remain authoritative, including historical inspection after assignment revocation.
Neither this action nor its retry can publish, approve, execute or recover a
Calendar write. The existing publication and S3 execution APIs are unchanged.

## Conversation presentation

The bounded Expert parser accepts at most one proposal, with the exact View and
interval of a recorded focus-window insight. Unknown fields in the proposal,
different intervals and unapproved data classifications are rejected. Synthetic
remains the default parser classification. Personal evidence additionally requires
an explicitly Personal session and a ready encrypted-vault controller; this is
presentation support, not a new Personal session or model dispatch route.

An explicit **Check saved action** control queries the saved capability's
Person/session/invocation reference. The controller rejects transient, foreign or
unmatched capability messages and serializes inspection with conversation and
registry operations. Recorded status, explicit absence and failure have distinct
text. A failure clears the previous result rather than leaving a stale action link.
No lookup occurs on card creation, expansion or repaint.

Inspection results exist only in memory and are cleared on session reload/change,
lock, key failure and disposal. Lock hides the conversation immediately, drains
the owned operation before closing the vault, and ignores late inspection results.
No recovery journal or automatic model retry is introduced.

**Open Calendar action** opens the existing `ActionReviewDialog` by saved action
UUID and reloads the canonical S3 ledger. Desktop, narrow inline and bottom-sheet
assistant layouts use the same route. The card offers no approval/execution control
and labels the result as recorded status, not current execution authorization.
S3 continues to own review, expiry, connection/policy checks and explicit recovery
of an uncertain write. Opening the dialog does not decide or execute an action.

## Evidence

- The protocol test rejects injected owner, destination, approval, execution,
  retry, raw-output and key fields in the inspection request.
- Two native worker tests cover a proposal committed by a synthetic connected
  Calendar turn, absence before publication, explicit publication, revoke/unlock,
  duplicate submits, unchanged session/action identity, foreign/invalid references,
  key loss and cancellation while key access is blocked. Injected models, key
  providers and Calendar access replace all live services.
- Five Dart tests cover bounded proposal parsing, explicit Personal classification,
  all seven S3 statuses, malformed/missing results, foreign responses, durable
  message scoping, serialized operations, failure versus absence and lock/key loss.
- Eleven widget tests cover explicit interaction, all seven status displays,
  520-pixel and 320-pixel/200% text layouts, and routing to the real S3 dialog at
  desktop and narrow widths without a decision. The real-font panel golden is
  `apps/client/test/goldens/agent_proposal_card.png`.
- The existing real Dart/C ABI test also verifies that inspecting while no vault
  is open fails without creating storage or accessing protected keys.

All **206 workspace Rust tests**, three keyring example tests and **25 native
assertions** pass. Rust formatting and Clippy with the existing Calendar exclusions
pass. All **185 Flutter tests**, Flutter analysis, the rebuilt Rust FFI library,
macOS Debug build and deep strict signature verification pass. The new real-font
golden was visually reviewed and the full suite rerun without golden updates.

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

## Remaining S4 work

The follow-up [isolated Calendar sessions increment](s4-calendar-sessions.md)
adds scoped Start/Resume/Get/Recover operations to the native vault and Dart gateway.
Native connected Calendar turn dispatch and connected-session UI remain unconnected.
The normal app still offers encrypted sample conversations; those
sample prompts do not produce Calendar proposals. Card interaction is demonstrated
with test-injected recorded proposals, not a newly enabled live chat feature.
Live key/model/source/privacy/auth gates, other S4 connectors and S1/S3 acceptance
remain unverified. This increment does not enable personal-data dogfood or complete
an S4 acceptance criterion.
