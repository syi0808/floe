# Floe

Floe is an open-source personal assistant that helps a person’s day run well by understanding their timeline, current state, and durable personal context.

The product is not an agent framework, automation builder, chat wrapper, or dashboard. Its primary experience is a calm **calendar-first Day Canvas**, fast capture, and rare, useful interventions.

> **Calendar is the canvas. Tasks, notes, and Floe are layers on top of it.**

## Status

The first Personal Day vertical slice is in progress. The macOS Flutter client reaches the Rust core through a versioned JSON/C ABI, while Rust owns typed operations, deterministic projections, and embedded Turso persistence.

That vertical slice successfully validated the technical boundary, but the initial `Now / Next` hero + generic list presentation is **not the target Day Canvas**. ADR 0006 adopts a calendar-first redesign with a day time grid, current-time indicator, real Event geometry, secondary Today Tasks/Notes, and Floe proposals attached to relevant times or objects.

The canonical planning specification is **floe-planning v0.8 plus accepted in-repository ADRs** in [`docs/planning/`](docs/planning/README.md) and [`docs/decisions/`](docs/decisions/).

## Start here

- [Calendar-first Day Canvas decision](docs/decisions/0006-calendar-first-day-canvas.md)
- [Floe design system](DESIGN.md)
- [Product brief](docs/product-brief.md)
- [MVP definition](docs/mvp.md)
- [Planning specification](docs/planning/README.md)
- [v0.8 integration and expert baseline](docs/decisions/0003-native-connectors-and-experts.md)
- [Implementation baseline](docs/decisions/0002-implementation-baseline.md)
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
- **Time is the primary visual structure of the Day Canvas.**
- Event, Task, and Note keep separate semantics and do not need equal visual weight.
- Personal memory must be inspectable, editable, deletable, and source-backed.
- Intelligence may propose; explicit policy and user confirmation govern actions.
- Floe suggestions attach to the time/object they affect instead of demanding a permanent AI dashboard.
- Sensitive raw data should remain local whenever practical.
- Platform parity means equivalent assistant experiences, not identical screens or APIs.
- Third-party Experts are untrusted by default: they receive explicit capability-scoped views and emit structured candidates, never arbitrary direct mutations or home-screen widgets.
- Product UI follows the tokens, interaction rules, and guardrails in [`DESIGN.md`](DESIGN.md).

## Planning source

`docs/planning/` contains the imported v0.8 planning bundle. Accepted ADRs under `docs/decisions/` may refine or supersede parts of that imported baseline; when they do, the ADR and current product/design documents are controlling.
