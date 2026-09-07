# S4 Calendar scope consent UI

Date: 2026-09-07. Calendar installation/grant UI integration; S4 remains **0/14**.

## User path

In the unlocked assistant panel, open **Tools & Experts → Calendar access**.
The screen uses the existing Day Canvas Calendar connection selection; it does not
enumerate OS calendars, request Calendar permission, read native events or change
the Connections selection. A missing, loading, unsupported or foreign-Person source
context cannot offer new grants. Existing scopes remain inspectable/revocable when
their sources are no longer selected.

The user selects one to four exact Calendar IDs and checks a separate confirmation
before **Install disabled Expert** becomes available. The screen shows the provider,
Calendar names and exact IDs, including disambiguation when names collide or legacy
metadata is incomplete. `includeAll` on the Calendar connection never becomes an
open-ended Expert grant: only the explicit current IDs are sent to setup.

Installation uses the existing native/Dart controller contract and atomically saves
a disabled binding, Tool/Expert installations and Person assignments. The saved scope
has a separate **Allow this Calendar scope** switch. Tool/Expert installation and
Person assignment flags remain separate controls in Tools & Experts; this scope
switch does not silently enable them, approve actions or activate connected chat.

An exact scope already associated with a durable setup receipt cannot be duplicated
from this screen. Legacy bindings without a setup receipt are not inferred to be
installed Experts; an explicit new setup remains possible, with a fresh binding rather
than appropriation of the old one. No existing scope can be edited or widened in place.

## Consent and lifecycle rules

`AgentCalendarSources` projects only selected Calendar metadata from the current
same-Person Day snapshot. It is passed to all assistant layouts, including the narrow
bottom sheet, with a listener for Day state changes. Per-source import warnings do not
mislabel healthy sources when an unrelated source caused the connection-level error.
Warnings do not substitute for the Core's live access, freshness and lease checks.

A connection/provider/revision/selection/label change clears local selection and
confirmation. The install and positive-enable handlers recheck current context before
dispatch rather than relying only on a previously rendered button. Changing selection
also clears confirmation. New calendars never appear inside an already saved scope.
Positive enablement requires the saved exact scope to be available in the current
connection; turning off an enabled scope remains possible without its source.

An uncertain setup shows its original provider/IDs and explicit refresh/same-setup
retry controls. Retry cannot use altered consent or a currently unavailable scope.
If the connection changes, confirmation resets; a refresh can still reconcile an
already committed old scope without another install. Uncommitted intent can only be
discarded after the controller confirms its absence in a successful same-instance
inspection. Scope changes are not silently substituted into a pending setup.

The screen does not optimistically toggle state. Vault lock clears presented source
metadata, selections and pending consent immediately, and late results do not repopulate
the dialog. Closing the dialog alone does not undo a submitted operation; that boundary
is stated in the UI. Disabling a scope affects new use, not existing S3 proposals or
approvals. No model route or execution authority is added by this screen.

## Evidence

- One projection test covers exact legacy selected IDs, immutable metadata, provider
  boundaries, no `includeAll` widening, duplicate rejection and per-source warnings.
- Nine widget cases use the real panel/dialog/controller with injected native-shaped
  transport. They cover explicit default-off installation and source enablement at
  520px and 320px/200% text, Today-to-dialog wiring at 1280px and 390px, bounded selection,
  changed-connection confirmation reset, duplicate suppression, legacy bindings,
  unavailable/foreign sources, revocation, uncertain-response refresh and lock races.
- The new `agent_calendar_consent.png` golden uses real bundled fonts, is rendered and
  visually reviewed, and passes again without golden updates. Existing goldens remain
  unchanged. Tests access fictional source metadata, not OS calendars or personal data.
- All **169 Flutter tests**, Flutter analysis, **195 workspace Rust tests**, three
  keyring example tests and **25 native assertions** pass. The macOS Debug build and
  deep strict signature verification pass as well.

```sh
cd apps/client
flutter gen-l10n
flutter test
flutter analyze
flutter build macos --debug
codesign --verify --deep --strict build/macos/Build/Products/Debug/floe_client.app
cd ../..
CARGO_INCREMENTAL=0 cargo test --workspace
CARGO_INCREMENTAL=0 cargo test -p floe-core --example vault_keyring_smoke
bash tools/s3-validation/check-native.sh
```

## Remaining S4 work

The app still runs encrypted sample conversations. This UI installs only the built-in
Calendar Tool/Schedule pair; it is not a general package marketplace or a new source
connector. Native connected-turn dispatch, real local/remote model integration,
proposal recovery/presentation, live protected-key provisioning and source/privacy/auth
gates remain. Gmail, Contacts, location/ETA/weather, Screen Time/Health and S1/S3
acceptance are not validated by this fixture-driven screen. No complete S4 acceptance
criterion is promoted.
