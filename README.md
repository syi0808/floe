# Floe

Floe is an open-source personal assistant that helps a person’s day run well by understanding their timeline, current state, and durable personal context.

The product is not an agent framework, automation builder, chat wrapper, or dashboard. Its primary experience is a calm **Day Canvas**, fast capture, and rare, useful interventions.

## Status

Greenfield restart. The previous implementation was intentionally removed; only the repository’s MIT `LICENSE` was retained.

The canonical planning specification is now **floe-planning v0.5** in [`docs/planning/`](docs/planning/README.md).

## Start here

- [Planning specification v0.5](docs/planning/README.md)
- [Product brief](docs/product-brief.md)
- [MVP definition](docs/mvp.md)
- [Implementation baseline](docs/decisions/0002-implementation-baseline.md)
- [Greenfield reset ADR](docs/decisions/0001-greenfield-reset.md)
- [Open questions](docs/open-questions.md)

## Working rules

- Product semantics come before implementation choices.
- Personal memory must be inspectable, editable, deletable, and source-backed.
- Intelligence may propose; explicit policy and user confirmation govern actions.
- Sensitive raw data should remain local whenever practical.
- Platform parity means equivalent assistant experiences, not identical screens or APIs.

## Planning source

`docs/planning/` is an in-repository copy of the user-supplied `floe-planning-v0.5` bundle, imported on 2026-09-02. It supersedes the earlier v0.3 planning baseline.
