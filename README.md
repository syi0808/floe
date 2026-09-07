# Floe

Floe is an open-source personal assistant that helps a person’s day run well by understanding their timeline, current state, and durable personal context.

The product is not an agent framework, automation builder, chat wrapper, or dashboard. Its primary experience is a calm **Day Canvas**, fast capture, and rare, useful interventions.

## Status

The first Personal Day vertical slice is in progress. The macOS Flutter client now reaches the Rust core through a versioned JSON/C ABI, while Rust owns typed operations, deterministic Day Canvas snapshots, and embedded Turso persistence.

Delivery now prioritizes connected vertical slices over sequential roadmap phases.
S1 has an EventKit read implementation and fixture end-to-end coverage; live
permission/read validation is still pending. The Go model gateway and local connection
console remain reusable infrastructure without a focus-time product feature;
unfinished Personal Day work remains tracked separately in [progress](PROGRESS.md).
After S3, delivery validates conversational Agent/Expert chat (S4), governed
Memory and self-improvement (S5), voice mode (S6), and local wake-up (S7) before
cross-device/server (S8) and intervention (S9).

The canonical planning specification is now **floe-planning v0.8** in [`docs/planning/`](docs/planning/README.md).

## Start here

- [Vertical slice delivery plan](docs/planning/08-engineering/vertical-slice-delivery.md)
- [Go inference gateway setup](server/README.md)
- [Inference performance-class decision](docs/decisions/0011-inference-performance-classes.md)
- [Delivery board and validation evidence](PROGRESS.md)
- [Slice-driven delivery decision](docs/decisions/0006-slice-driven-delivery.md)
- [Memory-and-Expert-first sequencing decision](docs/decisions/0012-memory-and-expert-first-slices.md)
- [Conversational Agent, learning, and voice sequence](docs/decisions/0013-conversational-agent-learning-and-voice-sequence.md)
- [Planning specification v0.8](docs/planning/README.md)
- [Floe design system](DESIGN.md)
- [Interface and screen specifications](docs/design/README.md)
- [Interactive HTML UI prototype](prototypes/floe-ui/README.md)
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
See the [prototype setup](prototypes/floe-ui/README.md#run) for installation and scripts.

- Product semantics come before implementation choices.
- Personal memory must be inspectable, editable, deletable, and source-backed.
- Intelligence may propose; explicit policy and user confirmation govern actions.
- Sensitive raw data should remain local whenever practical.
- Platform parity means equivalent assistant experiences, not identical screens or APIs.
- Third-party Experts are untrusted by default: they receive explicit capability-scoped views and emit structured candidates, never arbitrary direct mutations.
- Product UI follows the tokens, interaction rules, and guardrails in [`DESIGN.md`](DESIGN.md).

## Planning source

`docs/planning/` is an in-repository copy of the user-supplied `floe-planning-v0.8` bundle, imported on 2026-09-02. It supersedes v0.5.
