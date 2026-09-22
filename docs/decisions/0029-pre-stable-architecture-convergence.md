# ADR 0029: Prefer architecture convergence during pre-stable development

- **Date:** 2026-09-22
- **Status:** accepted

## Context

Floe is under active pre-stable development. Product requirements and implementation choices still change quickly, and agent-assisted development is used to deliver those changes at high throughput.

The repository previously accumulated locally reasonable compatibility layers, forwarding adapters, duplicate execution routes and widened contracts. These changes often minimized the immediate diff and kept old callers working, but repeated application made ownership ambiguous, expanded the number of supported states and increased the cost of debugging and later changes.

Floe does not currently have a stable public internal API that requires preserving obsolete Rust/Dart/Go call shapes. Local development data can also be recreated when a reviewed format/meaning change intentionally makes it incompatible. At the same time, external provider protocols, independently versioned wire/security contracts, durable safety semantics and uncertain external side effects remain real boundaries and cannot be treated as disposable internal compatibility.

## Decision

While Floe remains pre-stable:

1. Optimize architecture-affecting changes for the simplest correct final system, not the smallest diff.
2. Treat one semantic owner and one canonical internal runtime path as default invariants.
3. When replacing an internal contract or path, migrate the in-scope callers and remove the obsolete internal path in the same bounded change rather than preserving it indefinitely through wrappers or dual routing.
4. Do not provide internal backward compatibility unless a current requirement identifies a real consumer or durable boundary that requires it.
5. Treat provider/OS/storage/transport adapters as legitimate boundary adapters; treat adapters between competing internal Floe designs as temporary exceptions that need explicit scope and a removal condition.
6. Avoid speculative abstractions, migration-only optional state and unnecessary public/FFI widening. New surface must represent a real owner, boundary, policy or demonstrated variation.
7. Permit temporary complexity or short compile breaks during a bounded cutover when they lead to a converged final state and do not weaken safety invariants.
8. Require architecture-affecting work to include caller cutover, obsolete-path deletion, residual audit, appropriate full-surface verification and current architecture-document convergence.
9. Prefer machine-enforced architecture rules when they can be derived and checked from the repository.

The current invariants are documented in [architecture invariants](../architecture/invariants.md). The development workflow and compatibility scope are documented in [architecture evolution](../development/architecture-evolution.md). Execution details belong in active plans/skills rather than this ADR.

## Consequences

- Some correct changes will intentionally have larger diffs because callers and obsolete surfaces are updated together.
- Internal APIs and development-only data formats may break during active product evolution when no explicit compatibility requirement applies.
- Tests passing are not sufficient to declare an architectural migration complete; deletion and residual checks are part of completion.
- New adapters, optional state and public surface receive stronger scrutiny because they expand the permanent state space.
- Agent workflows can be more deterministic because they are instructed to design the final topology first instead of preserving the current topology by default.
- Current architecture documents must be updated with architecture changes, while this ADR remains rationale rather than a progress ledger.

## Non-goals

This decision does not authorize:

- weakening authorization, consent, provenance, CAS, recovery or other safety invariants,
- deleting unrelated or uncertain user/provider data,
- renaming or breaking independently versioned external protocols merely to match an internal cleanup,
- avoiding all abstractions or adapters,
- performing broad rewrites when a local change is genuinely local.

## Revisit when

Revisit and supersede this decision when Floe introduces one or more of the following as product requirements:

- a stable public API/SDK with compatibility guarantees,
- externally deployed clients that cannot be upgraded atomically,
- durable user-data migration guarantees across released versions,
- a formal long-term support/versioning policy.

At that point, compatibility must become an explicit product architecture with versioning and migration rules rather than an accumulation of ad hoc internal shims.
