# S4 durable Calendar View bindings

Date: 2026-09-07. Core permission integration; S4 remains **0/14**.

## Persistent scope identity

The encrypted Expert registry now stores `CalendarViewBinding`: a fresh opaque View
UUID, one Person, one Calendar provider, a canonical exact list of Calendar IDs and
enablement. A binding permits at most four unique nonempty IDs, each at most 512 bytes;
the registry permits at most 256 bindings within its existing 256 KiB payload bound.
Provider determines classification: Fixture is Synthetic and EventKit is Personal.
No caller-supplied classification can relabel an EventKit binding as Synthetic.

`AgentRegistry::register_calendar_view` creates a new handle with enablement off.
Enabling and disabling are explicit revision-checked registry operations. Ordinary
vault registry CAS refuses an already-enabled new binding, foreign-Person bindings,
or a binding that appropriates a previously assigned legacy/unbound View handle.
It also rejects removing a persisted binding or changing its handle, Person, provider
or Calendar IDs. Only enablement can change in place. A different scope requires a
fresh binding and explicitly granted Tool/Expert assignments, not a retargeted ID.

These are trusted Core setup operations, not model capabilities or a new native/UI
consent flow. Registration does not discover calendars, request OS permissions, select
sources, create an assignment or grant access automatically. The application must
still obtain explicit scope consent before setup and assignment. A trusted initial
registry bootstrap can include its explicitly configured bindings; the existing
sample bootstrap continues to include none.

Bindings are part of the encrypted registry snapshot and existing CAS transaction,
not a sidecar file or provider cache. Private-state/result commits preserve them.
The registry overview sent to the app still omits the binding records, source IDs and
View handles. Model descriptors and Expert results also do not acquire raw source IDs.

## Runtime enforcement

The Core Calendar turn host now requires an enabled persisted binding before starting
the runtime. The supplied lease must match its Person, handle, provider and exact
Calendar set. Reordering the set is harmless; narrowing, widening or substituting it
under the same handle is not allowed. Missing or revoked bindings fail before model
or native access. Enabled assignments, required Tools, data classes and inference
policy remain independently required.

Connection revision, planning day/window and cache expiry remain per-turn state.
The existing Calendar View still validates selected scopes, current OS access,
import coverage and freshness; a durable binding does not make a cached observation
current or bypass those checks. Changes to connection selection do not automatically
extend this binding. The runtime's registry revision guards and transactional result
validation also cover binding changes while a turn runs.

The common Expert resolver denies a revoked bound View, so it also denies new
publication of a committed proposal through the Manager bridge. Calendar-origin
receipts require an active binding, even outside the turn host. Their source handle
must identify the same View and connection revision as the publication request;
an old receipt cannot be made current by substituting a newer destination revision.
Existing S3 actions and user approvals are not deleted or retroactively revoked.

## Compatibility and limits

The version-1 registry snapshot adds an optional `calendar_views` field. Existing
snapshots without it load as having no Calendar bindings; they are not silently
upgraded into authorized Calendar scopes. Synthetic sample/declarative ports can
still use their existing unbound fixture Views. Connected Core Calendar turns cannot.
Existing Calendar-origin receipts without a binding cannot authorize new publication.

Empty binding lists retain the previous serialized shape. Older strict readers may
reject a snapshot containing the new field; no downgrade/write-back path is provided.
Removing bindings from a later snapshot through registry CAS fails closed. This is
not a table/identity-version migration or a change to the protected key backend.

## Automated evidence

- Two registry tests cover canonicalization, size/cardinality, default-off state,
  stale revisions, foreign Person, provider classification, malformed/duplicate
  bindings and legacy unbound snapshot behavior.
- Two encrypted-vault tests cover persistence/reopen, omission from app overviews,
  ciphertext canary absence, immutable identity/scope, no removal, no pre-enabled
  addition and no appropriation of an old opaque grant.
- Three turn/Manager tests cover rejection before any model/provider dispatch,
  revocation after a committed Expert result and an old receipt paired with a newer
  connection revision. Existing end-to-end Calendar tests now explicitly initialize
  a binding rather than substituting Calendar data behind an unbound sample handle.
- All **179 workspace Rust tests**, three keyring example tests and **25 native
  assertions** pass. Formatting and Clippy with the existing Calendar exclusions pass.
  Tests use fictional records, injected keys/access stamps and scripted models.
- All **146 Flutter tests**, Flutter analysis, the rebuilt Rust library, macOS Debug
  build and deep strict signature verification pass. The app protocol/registry overview
  and existing UI goldens remain unchanged by this Core scope binding feature.

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

The app still needs explicit Calendar scope selection/confirmation, binding and package
installation/assignment management, and native dispatch into the connected turn host.
Its existing Tools & Experts screen changes only installation/assignment enablement;
it does not create, display or edit source bindings yet. No personal-chat route was
enabled, no live Calendar/model permission was requested, and no key was provisioned.
Live key/model gates, other Connectors, remote privacy/authentication and S1/S3
acceptance remain open; these fixtures do not satisfy the full S4 acceptance matrix.
