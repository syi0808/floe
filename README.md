# Floe

Floe is an open-source personal assistant that helps a person’s day run well by understanding their timeline, current state, and durable personal context.

The product is not an agent framework, automation builder, chat wrapper, or dashboard.
Its long-term primary experience is one ambient assistant reached mainly through
voice and rare, useful interventions. Visual surfaces such as the calm **Day Canvas**
appear for context, explicit approval, inspection and recovery rather than becoming
the assistant's primary home.

## Status

The first Personal Day vertical slice is in progress. The macOS Flutter client now reaches the Rust core through a versioned JSON/C ABI, while Rust owns typed operations, deterministic Day Canvas snapshots, and embedded Turso persistence.

Delivery now prioritizes connected vertical slices over sequential roadmap phases.
S1 has an EventKit read implementation and fixture end-to-end coverage; live
permission/read validation is still pending. The Go model gateway and local connection
console remain reusable infrastructure without a focus-time product feature;
unfinished Personal Day work remains tracked separately in [progress](PROGRESS.md).
After S3, delivery validates a headless-capable Manager/Expert semantic loop through
an initial conversational inspection panel with bounded
Calendar, Gmail, Contacts, travel/weather, Screen Time, and Apple Health sources plus
privacy-aware local/remote model routing (S4), governed
Memory and self-improvement (S5), voice mode (S6), and local wake-up (S7) before
cross-device/server (S8) and intervention (S9).

S4 preparatory work now includes a bounded Rust Agent runtime, durable synthetic
and Calendar-scoped sessions, and a user-invoked conversation panel with progress,
stop and resume through the C ABI/Dart gateway. An enabled Calendar Expert scope now
selects the isolated Calendar session and bounded native turn path; otherwise the
panel remains visibly sample-only. General free-text chat and live model/source
acceptance remain gated. A separate encrypted session-store component now uses
the keyring-rs ecosystem for OS key access. The default panel now requires explicit
secure-storage setup/unlock through a nonblocking native worker; it never falls
back to the legacy plaintext sample store.
See the [panel](docs/validation/s4-agent-panel.md) and
[vault component](docs/validation/s4-agent-vault.md) and
[app integration validation records](docs/validation/s4-agent-vault-host.md).
The vault currently keeps only its random database key in the user's macOS login
Keychain, so local development does not require an Apple Developer Program
provisioning profile. A future signed release may migrate keys to Apple Protected
Data after an explicit compatibility and recovery design. See the
[keyring smoke](docs/validation/s4-keyring-live-smoke.md). A bounded
[native Foundation Models adapter](docs/validation/s4-local-model.md) now implements
the common model contract, but the real probe reports Apple Intelligence disabled;
live generation and personal-chat integration remain unverified.
A [bounded Expert foundation](docs/validation/s4-expert-foundation.md) now supplies
versioned Tool/Expert registration, Person grants and a shared Schedule/declarative
contract. The sample panel renders structured Expert evidence; the secure host now
[persists registry/private state atomically with conversation results](docs/validation/s4-expert-persistence.md).
A [Manager-to-S3 bridge](docs/validation/s4-manager-actions.md) now converts committed
focus advice into the existing policy/review/action ledger with stable retry identity.
[Registry enablement management](docs/validation/s4-registry-management.md) is now
available in the unlocked panel. User-facing installation/source-grant setup,
live sources and app-level proposal orchestration still remain.
A [bounded Calendar Timeline adapter](docs/validation/s4-calendar-timeline.md) now
projects exact authorized mirror scopes through the common Expert port, with a
native read-access boundary. [Core turn orchestration](docs/validation/s4-calendar-turn.md)
now connects ordinary model calls, leased Views, atomic Expert results and governed
proposal preparation. The app's connected personal-chat route is not enabled yet.
[Durable Calendar bindings](docs/validation/s4-calendar-bindings.md) now pin each
connected View to an exact encrypted source scope. An
[atomic Calendar setup operation](docs/validation/s4-calendar-setup.md) now installs
disabled built-ins, assignments and a bound scope with durable retry identity;
[native transport and controller management](docs/validation/s4-calendar-management.md)
now expose explicit setup/reconciliation and scope enablement.
[Calendar access UI](docs/validation/s4-calendar-consent.md) selects and confirms an
exact existing connection scope without automatically expanding grants. Connected
chat dispatch and live key/model/source verification remain open.
Core [read-only proposal inspection](docs/validation/s4-proposal-inspection.md) now
recovers existing S3 action links after restart or scope revocation without re-execution.
[Native inspection and conversation cards](docs/validation/s4-proposal-presentation.md)
now show saved action status and open existing S3 review.
[Isolated Calendar sessions](docs/validation/s4-calendar-sessions.md) now support
encrypted start/resume/get/recover through native and Dart storage APIs.
[Native Calendar turn dispatch](docs/validation/s4-calendar-native-turn.md) now binds
those sessions to the current durable scope, streams Core events and waits for Manager
proposal preparation. Its typed Dart transport validates completed job identity. The
[Calendar conversation app path](docs/validation/s4-calendar-app-turn.md) now connects
enabled scopes to the controller and panel and admits Personal input to Foundation Models
only from the encrypted local session boundary; live generation is still unverified. The
[assistant access experience update](docs/validation/s4-agent-experience-feedback.md)
for automatic conversation-store access, flat Settings-owned permissions and the custom
Floe switch component. See the
[S4 handoff and remaining work](docs/validation/s4-handoff.md) for the committed
baseline, validation limits and restart checklist. S4 acceptance remains 0/14.

The canonical planning specification is now **floe-planning v0.8** in [`docs/planning/`](docs/planning/README.md).

## Start here

- [Vertical slice delivery plan](docs/planning/08-engineering/vertical-slice-delivery.md)
- [Go inference gateway setup](server/README.md)
- [Inference performance-class decision](docs/decisions/0011-inference-performance-classes.md)
- [Delivery board and validation evidence](PROGRESS.md)
- [Slice-driven delivery decision](docs/decisions/0006-slice-driven-delivery.md)
- [Memory-and-Expert-first sequencing decision](docs/decisions/0012-memory-and-expert-first-slices.md)
- [Conversational Agent, learning, and voice sequence](docs/decisions/0013-conversational-agent-learning-and-voice-sequence.md)
- [S4 connected Agent sources](docs/decisions/0014-s4-connected-agent-sources.md)
- [S4 privacy-aware inference](docs/decisions/0015-s4-privacy-aware-inference.md)
- [Agent context assembly and progressive Playbooks](docs/decisions/0017-agent-context-assembly.md)
- [Ambient assistant, domain Experts and capability connectors](docs/decisions/0020-ambient-assistant-expert-connector-model.md)
- [Planning specification v0.8](docs/planning/README.md)
- [Floe design system](DESIGN.md)
- [Interface and screen specifications](docs/design/README.md)
- [Product brief](docs/product-brief.md)
- [MVP definition](docs/mvp.md)
- [v0.8 integration and expert baseline](docs/decisions/0003-native-connectors-and-experts.md)
- [v0.5 implementation baseline](docs/decisions/0002-implementation-baseline.md)
- [First Personal Day vertical slice](docs/decisions/0004-personal-day-first-slice.md)
- [Flutter ↔ Rust JSON/C ABI bridge](docs/decisions/0005-json-c-abi-flutter-bridge.md)
- [Open questions](docs/open-questions.md)

## Development

Rust 1.93 or newer is required.

```sh
cargo test --workspace
```

Flutter 3.47 or newer is required for the cross-platform client.

```sh
cd apps/client
flutter test
flutter run -d macos
```

## Working rules

Local model connections and app pairing: [server dashboard setup](server/README.md#local-dashboard-and-app-pairing).
The node is loopback-only; it is not the hosted/sync server.

JavaScript/TypeScript projects in this repository use **pnpm**, with the version
pinned in each `package.json`. Commit `pnpm-lock.yaml`, not npm or Yarn lockfiles.

- Product semantics come before implementation choices.
- Personal memory must be inspectable, editable, deletable, and source-backed.
- Intelligence may propose; explicit policy and user confirmation govern actions.
- Sensitive raw data should remain local whenever practical.
- Platform parity means equivalent assistant experiences, not identical screens or APIs.
- Third-party Experts are untrusted by default: they receive explicit capability-scoped views and emit structured candidates, never arbitrary direct mutations.
- Product UI follows the tokens, interaction rules, and guardrails in [`DESIGN.md`](DESIGN.md).

## Planning source

`docs/planning/` is an in-repository copy of the user-supplied `floe-planning-v0.8` bundle, imported on 2026-09-02. It supersedes v0.5.
