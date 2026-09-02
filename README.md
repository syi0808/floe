# Floe

Floe is an open-source personal assistant that helps a person’s day run well by understanding their timeline, current state, and durable personal context.

The product is not an agent framework, automation builder, chat wrapper, or dashboard. Its primary experience is a calm **Day Canvas**, fast capture, and rare, useful interventions.

## Status

Greenfield restart. The previous implementation was intentionally removed; only the repository’s MIT `LICENSE` was retained.

## Start here

- [Product brief](docs/product-brief.md)
- [MVP definition](docs/mvp.md)
- [Decision log](docs/decisions/0001-greenfield-reset.md)
- [Open questions](docs/open-questions.md)

## Working rules

- Product semantics come before implementation choices.
- Personal memory must be inspectable, editable, deletable, and source-backed.
- Intelligence may propose; explicit policy and user confirmation govern actions.
- Sensitive raw data should remain local whenever practical.
- Platform parity means equivalent assistant experiences, not identical screens or APIs.

## Source material

The initial planning baseline is `floe-planning-v0.3`, supplied separately on 2026-09-02. A linked ChatGPT share is also an intended source, but it has not been imported into this repository yet.
