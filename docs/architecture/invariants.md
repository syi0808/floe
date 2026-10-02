# Architecture invariants

This document defines repository-wide properties that Floe's completed architecture must preserve. It is not an execution plan and does not describe transient checkpoint status. Read the current owner-specific documents for concrete structure and use [architecture evolution](../development/architecture-evolution.md) when changing that structure.

## 1. One semantic owner per authority

Every durable piece of state, policy or authority has one semantic owner.

A caller may carry a reference, snapshot or derived value, but it must not become a competing source of truth. Identity, connections, credentials, source-processing permission, conversation state, execution state and side-effect intent must be obtained from or validated by their canonical owner.

Duplicating an authoritative value in request payloads, convenience structs or adapters creates disagreement states and forces downstream code to decide which copy wins.

When introducing or moving data, answer:

- Which owner defines its meaning?
- Which owner may mutate it?
- Which component validates it before use?
- Is another copy authoritative, cached, derived or merely serialized for a real boundary?

If two components can independently answer the same authority question, the design has not converged.

## 2. One canonical runtime path

A product behavior should have one canonical internal execution path.

A migration may temporarily create old and new paths while callers are being cut over, but the completed change must move all in-scope callers to the chosen path and delete the obsolete route. A compatibility branch is not a second implementation strategy.

Completed work must not retain:

- legacy/new runtime branching,
- two internal request shapes for the same meaning,
- duplicated coordinators or policy engines,
- alternate owner bypasses kept only because an old caller still exists,
- forwarding layers whose only purpose is preserving an obsolete internal API.

The default completion test is not "the new path works"; it is "the new path works and the old path no longer exists in scope."

## 3. Dependency direction follows ownership

Dependencies must point toward the component that owns the needed contract or capability, not toward a convenient concrete implementation.

The physical Rust dependency policy is encoded in tools/architecture/module-dependencies.json and checked by tools/architecture/check_boundaries.py. Cargo.toml manifests remain the current physical graph.

A dependency exception is an architecture change, not a local compilation fix. Do not solve it by:

- moving a type to an unrelated shared package,
- making an internal symbol public only so an upper layer can reach around its owner,
- importing a concrete provider into a business owner,
- introducing a facade whose only role is hiding a forbidden edge.

Pure cross-owner value contracts may live in crates/contracts/ when they genuinely have no business owner. Shared placement must not be used to erase ownership.

## 4. Adapters represent real boundaries

Adapters are expected where Floe meets a provider, OS API, storage engine, transport, native platform or other real external boundary. They translate between distinct systems while preserving the owning domain semantics.

An internal compatibility shim between two Floe designs is different. It is transitional debt and is allowed only when all of the following are explicit:

- the exact scope that still requires it,
- why direct cutover is not currently possible,
- the removal condition,
- the bounded checkpoint or lifetime in which it will disappear.

Do not create permanent legacy, compat, vNext, forwarding or dual-decoder structures merely to minimize a breaking internal change.

## 5. Public surface and state space are budgets

Every public constructor, public symbol, FFI command, wire field, persisted field, enum case and optional state expands the system's supported state space.

Add public surface only when another legitimate boundary needs it. Add optionality only when an absent value is a real domain state rather than a migration convenience or a way to avoid updating callers.

In particular, be suspicious when a change requires:

- an internal symbol becoming public,
- a second constructor for old callers,
- an optional context/identity/connection field that the canonical path always requires,
- a wire field carrying data already owned and derivable internally,
- a new shared type whose only consumer is a single implementation.

Removing obsolete surface is part of completing the change that made it obsolete.

## 6. Safety invariants survive simplification

Architecture convergence never authorizes weakening correctness or safety checks.

Authorization, verified Gateway identity, source-owned processing authority, key identity, provenance, compare-and-swap semantics, durable pre-dispatch intent, cancellation direction and uncertain external-write recovery remain hard requirements. Their detailed ownership and recovery semantics are documented in [authority and recovery](authority-recovery.md).

[ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md) deliberately replaces the exact model-recipient consent invariant. The final processing boundary is DeviceOnly/GatewayAllowed at the source grant, not a provider/profile approval. Pairing does not grant source access, model routing cannot widen source permission, and processing permission cannot authorize an Action. Keep live source/grant/identity/provenance fences when retiring the obsolete consent representation.

Manager, shipped Experts and Learner converge on one Gateway-primary/local-fallback selector. Fallback requires valid planning-time Primary absence; permission denial, credential or inventory errors, transport failure, deadline and cancellation cannot masquerade as absence. Product clients carry purpose and a meaningful Device/Gateway boundary, never concrete routing identity.

Health alone has a mandatory source-owned device-local semantic transform before any Health View enters reasoning. Raw samples and the pre-transform aggregate remain outside Agent/Context and Gateway inputs. Transform failure is closed; transform evidence is not authority; HighlySensitive is not downgraded; Gateway use still requires source processing permission. Do not generalize this into a speculative cross-connector framework.

These are completed-system requirements. Accepted decisions must not be presented as implemented runtime until their corresponding cutovers land.

If a simpler design cannot preserve a safety invariant, the design is incomplete rather than the invariant being optional.

## 7. Transitional complexity must converge

An architecture-changing implementation may temporarily increase complexity inside one bounded change set. A short compile break, temporary caller mismatch or staged contract cutover can be acceptable while replacement is in progress.

The final state must converge back to:

- one owner,
- one canonical path,
- no unnecessary compatibility branch,
- no unexplained public-surface growth,
- no migration-only state,
- current architecture documentation matching the repository.

Temporary complexity without an explicit deletion step is permanent complexity in practice.

## 8. Prefer durable enforcement at the right layer

Prefer the lowest-maintenance enforcement that directly represents the invariant. Make invalid states impossible through types, visibility and dependency direction where practical; otherwise exercise the owning API with regression tests. Repository tooling is appropriate when it automates a repeated, complex or high-risk procedure against stable structured inputs.

The current dependency-policy example is `tools/architecture/check_boundaries.py`, which derives the Cargo graph from manifests and compares it with `module-dependencies.json`. Ordinary source refactors do not require changing that checker; the policy changes only when the architecture changes.

Do not promote a migration's deleted-symbol list, exact source-text shape or one-off residual `rg` into permanent infrastructure. Those checks are useful completion evidence inside the bounded migration and should retire with it. A source-level semantic checker is a last resort when the invariant cannot be represented by types, dependency structure, owner tests or other stable structured data.

Documentation explains semantics and intent. It should not become a second manually maintained representation of facts that can be derived from the repository.

## Architecture smell signals

| Signal | Why it matters |
|---|---|
| A second internal adapter or route is needed | The system may be preserving two designs instead of converging |
| The same authority appears in two owners or payloads | Disagreement and provenance ambiguity become possible |
| A migration needs new optional domain state | Transition convenience is leaking into the permanent model |
| A local fix requires public/FFI widening | Ownership or dependency direction may be wrong |
| Provider/storage/transport data enters an owner contract | A boundary concern is leaking inward |
| One feature requires bypassing an existing abstraction | The abstraction or owner contract may no longer match reality |
| Plumbing growth dominates feature logic | The current architecture may be the root cause |
| A new abstraction has one implementation and only forwards calls | Indirection may be speculative rather than semantic |

When these signals appear, follow .agents/skills/architecture-change/SKILL.md rather than adding another workaround.

## Relationship to execution plans

A task-specific execution plan may temporarily describe states that do not yet satisfy every final-state invariant. It owns only the bounded migration sequence. Completion of an architecture-changing task requires convergence back to the invariants above unless the plan records an explicit bounded exception with a removal condition.
