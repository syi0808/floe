# Floe

Floe is an open-source personal assistant that helps a person’s day run well by understanding their timeline, current state, and durable personal context.

The product is not an agent framework, automation builder, chat wrapper, or dashboard. Its primary experience is a calm **Day Canvas**, fast capture, and rare, useful interventions.

## Status

The first Personal Day vertical slice is in progress. The macOS Flutter client now reaches the Rust core through a versioned JSON/C ABI, while Rust owns typed operations, deterministic Day Canvas snapshots, and embedded Turso persistence.

The canonical planning specification is now **floe-planning v0.8** in [`docs/planning/`](docs/planning/README.md).

## Start here

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

- Product semantics come before implementation choices.
- Personal memory must be inspectable, editable, deletable, and source-backed.
- Intelligence may propose; explicit policy and user confirmation govern actions.
- Sensitive raw data should remain local whenever practical.
- Platform parity means equivalent assistant experiences, not identical screens or APIs.
- Third-party Experts are untrusted by default: they receive explicit capability-scoped views and emit structured candidates, never arbitrary direct mutations.
- Product UI follows the tokens, interaction rules, and guardrails in [`DESIGN.md`](DESIGN.md).

## Planning source

`docs/planning/` is an in-repository copy of the user-supplied `floe-planning-v0.8` bundle, imported on 2026-09-02. It supersedes v0.5.
