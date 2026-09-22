---
name: architecture-change
description: Use for Floe changes that affect semantic ownership, dependency direction, public or FFI/wire contracts, persistence meaning, runtime lifecycle, authorization/authority, provider/platform boundary placement, cross-module abstractions, or require migrating multiple callers. Do not use for a purely local implementation change that stays inside one existing owner with no contract, dependency, persistence or lifecycle impact.
---

# Architecture change workflow

Use this workflow to keep high-velocity implementation from preserving obsolete internal designs. The objective is a coherent final architecture, not a minimal diff.

If an active Stage/execution plan already defines checkpoint order, deletion gates or report format, follow that plan first. This skill supplements the plan with repository-wide architecture discipline; do not create a parallel plan.

## 1. Establish the baseline

Before editing:

1. read the root AGENTS.md,
2. inspect actual HEAD/current working changes and preserve user work,
3. inspect the source, manifests and tests that currently implement the affected path,
4. read docs/architecture/README.md, docs/architecture/invariants.md, and only the owner-specific architecture document needed for the task,
5. read an ADR only when the reason for a durable boundary is needed,
6. if the task belongs to active refactoring, read the current Stage checkpoint and only its linked execution plan.

Do not use historical plans as current implementation truth.

## 2. Classify the change

Confirm whether the work is:

- **local**: one owner, no durable/public contract change, no dependency change, no migration;
- **cross-component**: several existing owners cooperate but their ownership/boundaries remain unchanged;
- **architectural**: ownership, authority, dependency direction, FFI/public contract, persistence semantics, lifecycle/recovery, boundary placement or multiple callers change.

If the task is actually local, do not invent an architectural migration.

## 3. Name the final state before the sequence

Write down, in the task context or execution plan:

- semantic owner of each changed state/behavior,
- canonical contract,
- canonical runtime path,
- allowed dependency direction,
- callers that remain after the change,
- old symbols/routes/fields/adapters that must be removed,
- safety/recovery invariants that must remain true.

Design the completed topology first. Do not start from "how can the old API keep working?"

## 4. Run the architecture smell check

Reassess the design before implementing if it appears to require any of the following:

- a second internal adapter or compatibility path,
- old/new permanent branching,
- duplicated authority or duplicate representations of the same required context,
- provider/storage/transport-specific data in an owner contract,
- migration-only optional/nullable state,
- public or FFI widening solely to keep an old caller alive,
- a forbidden/reversed dependency edge,
- a forwarding-only facade/port with no semantic boundary,
- bypassing the component that currently owns the behavior,
- plumbing growth that dominates the actual feature.

If one appears, prefer changing the root owner/contract/path over layering around it.

## 5. Choose plan depth

Do not create a repository plan for every change.

Use direct implementation for a truly local change.

Use a short task-local sequence for a bounded cross-component change.

Create or update one authoritative repository execution plan when the change:

- moves ownership/authority,
- changes package/crate boundaries,
- changes FFI/wire or persistence semantics,
- changes runtime lifecycle/cancellation/recovery,
- migrates multiple callers,
- removes a substantial old runtime/API path,
- is likely to span agent contexts or checkpoints.

The plan must define the target state, ordered cutover, deletion gate, residual searches and verification. Do not create another STATUS file, migration ledger or parallel global task board.

## 6. Execute toward one path

Prefer this sequence unless the active plan requires a safer order:

1. establish the canonical contract at the correct owner,
2. implement the canonical behavior,
3. migrate callers in scope,
4. port tests to exercise the canonical path,
5. remove obsolete internal types/routes/constructors/branches/adapters,
6. narrow public surface that no longer needs to be public.

A short bounded compile break is acceptable inside the same architectural change set. Do not add a wrapper merely to keep an obsolete caller compiling.

Real provider/OS/storage/transport adapters remain valid. Internal compatibility shims require explicit bounded justification and a removal condition.

## 7. Preserve safety invariants

Never weaken authorization, exact-recipient consent, key identity, provenance, CAS, durable pre-dispatch intent, cancellation direction or uncertain external-write recovery to make the new structure fit.

Use docs/architecture/authority-recovery.md when the change touches these semantics.

## 8. Residual audit

Before declaring the architecture complete, search for the obsolete design by concept, not only by one exact symbol.

Search as applicable for:

- old type/function/constructor names,
- legacy enum/command/ABI variants,
- old storage/wire fields,
- compatibility/fallback branches,
- forwarding adapters,
- old imports/dependencies,
- test fixtures still using the obsolete path,
- comments/docs that describe the removed route as current.

Every remaining match must be either unrelated, deliberately historical, or explicitly justified by the active plan.

## 9. Documentation convergence

In the same change set:

- update docs/architecture/* when current ownership/path/boundary changes,
- amend/supersede/add an ADR only when the durable decision or rationale changes,
- update the active execution plan when execution status/checkpoints change,
- do not copy progress state into architecture docs or ADRs.

One fact should keep one authoritative home.

## 10. Verification handoff

After implementation and residual audit, use the code-change-verification skill for the affected surfaces plus any stronger verification required by the active execution plan.

Passing tests alone is not completion. Confirm that the final system also has one owner/path and that obsolete transition surface is gone.

## Final report

For architectural work, report concisely:

1. changed owners/contracts and canonical final path,
2. migrated callers,
3. deleted obsolete/compatibility surface,
4. residual-audit result and any bounded exception,
5. verification performed,
6. architecture/ADR/plan documents updated,
7. remaining blocker, if any.
