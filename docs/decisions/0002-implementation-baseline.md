# ADR 0002: Adopt v0.5 implementation baseline

- **Date:** 2026-09-02
- **Status:** accepted as a recommended baseline; validate through Phase 0 PoCs

## Context

The v0.5 planning bundle adds a concrete cross-platform implementation direction while retaining product-first boundaries. The project needs a baseline that allows the Personal Day MVP to begin without treating every technology choice as final.

## Decision

Adopt the following baseline:

| Concern | Baseline |
| --- | --- |
| Main cross-platform visual UI | Flutter / Dart |
| Shared performance and local domain core | Rust |
| Apple OS integration | Swift, with Rust where lower-level APIs fit better |
| Android OS integration | Kotlin + Rust NDK Core |
| Windows low-level integration | Rust + `windows-rs` |
| Server control plane | Go |
| Connector host | isolated TypeScript / Node.js |
| Device and hosted persistence foundation | Turso, behind the Rust Core on devices |
| Repository strategy | monorepo first |

Desktop separates a resident Rust/native Device Agent from the Flutter UI. Mobile embeds the Rust Core in the application and follows OS lifecycle restrictions.

## Consequences

- Flutter owns presentation and transient UI state; canonical domain changes flow through typed commands to Rust.
- Rust → Flutter communication is batched snapshots/events, not per-widget FFI calls. Audio, model, and large-data hot paths do not route through Dart or Platform Channels.
- Turso replication does not replace Floe-owned identity, authorization, encryption, device revocation, or deletion semantics.
- Go and the Rust Core share protocol/schema/test vectors rather than requiring the entire server to be Rust.
- The P0-G Turso PoC gates storage/sync topology; technology decisions remain revisable with evidence.

## Source precedence

The v0.5 `09-implementation/connector-runtime.md` diagram labels the Floe Server as Rust. That conflicts with D-024, `technology-selection.md`, `server-stack.md`, and `architecture-map.md`, all of which designate **Go** for the server control plane. This ADR uses D-024 and those detailed implementation documents as the controlling decision; the connector runtime diagram should be corrected in the next planning bundle revision.

## References

- `docs/planning/08-engineering/decisions.md` (D-017–D-025)
- `docs/planning/09-implementation/technology-selection.md`
- `docs/planning/09-implementation/client-architecture.md`
- `docs/planning/09-implementation/rust-core.md`
- `docs/planning/09-implementation/server-stack.md`
- `docs/planning/09-implementation/turso-storage.md`
- `docs/planning/09-implementation/repository-layout.md`
