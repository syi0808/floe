# Floe documentation

This file is the entry point for repository documentation. Do not recursively read the entire `docs/` tree before starting work. Start here, then follow only the documents needed for the current task.

## Authority map

| Question | Canonical source |
|---|---|
| What product is Floe building? | [Product documentation](product/README.md) |
| How is the code structured now? | [Architecture](architecture/README.md) and `tools/architecture/module-dependencies.json` |
| What invariants must every architecture change preserve? | [Architecture invariants](architecture/invariants.md) |
| How should Floe evolve while it is pre-stable? | [Architecture evolution](development/architecture-evolution.md) |
| Why was a durable design choice made? | [Architecture decisions](decisions/README.md) |
| How should an architecture-changing task be sequenced? | [Architecture evolution](development/architecture-evolution.md) and the repo-local architecture-change skill |
| How should the product look and behave? | [Design system](../DESIGN.md) and [design specifications](design/README.md) |
| How do I build/debug a surface? | The nearest component README, plus [development docs](development/) where applicable |
| How do I configure deployment/runtime infrastructure? | [Deployment docs](deployment/) and the relevant service README |

## Document classes

Repository documentation has five roles. Keep them separate.

1. **Product** defines durable product meaning, principles, boundaries and capability direction. It must not be an implementation progress board.
2. **Architecture** describes current ownership, dependencies and runtime boundaries. It must match the repository and architecture policy, not a historical target.
3. **ADR** records why a durable decision was made. ADR history is preserved; a later decision supersedes or amends it rather than rewriting history into a progress log.
4. **Execution plans** describe temporary task sequencing when a change is too large for one context. They are not durable architecture or a permanent progress board.
5. **Development/runbook/design** describes how to evolve, build, operate, debug or present the product. Durable engineering policy belongs here; commands and paths must match current code.

Historical implementation checkpoints, obsolete migration plans and old acceptance matrices belong in Git history, not in the active documentation hierarchy.

## Reading policy for agents

Use the smallest relevant context:

- For code changes, inspect the actual code and manifests first, then the current architecture document for the affected owner.
- For a change that affects ownership, dependency direction, public/FFI contracts, persistence meaning, runtime lifecycle, authority, adapters or multiple callers, use `.agents/skills/architecture-change/SKILL.md` to select the minimum additional context and workflow.
- If the current task has an explicit authoritative execution plan, read only that plan and the current architecture needed for the affected owners.
- Read an ADR only when the reason for a boundary or invariant is needed.
- Do not treat old slice numbers, historical validation counts, or an implementation path mentioned in an ADR as current code.
- If documentation conflicts with source, determine whether the document is stale. Current code plus accepted architecture policy define implementation reality; product/ADR documents define intent and rationale within their stated scope.

## Maintenance rules

- One fact should have one authoritative home. Other documents link to it instead of copying status tables.
- Do not create another global progress file, migration ledger or status matrix.
- Do not use `current`, `canonical`, `implemented` or similar wording for a historical snapshot unless it is verified against the current source tree.
- When an architecture change lands, update its current architecture description in the same change set.
- When a decision changes, add/amend/supersede an ADR; do not use an ADR as an implementation checklist.
- Git history is the archive for removed planning and validation documents.
- If an architectural invariant can be checked mechanically, prefer a repository tool/CI rule as the enforcement mechanism and keep prose as the semantic explanation rather than a second hand-maintained truth.

## Task-specific execution plans

For the Expert extensibility and preselected source-binding task, use the
[authoritative execution plan](development/plans/expert-extensibility/README.md)
and only its current checkpoint. It describes proposed work, not implemented
architecture; other tasks do not need to load it. Remove this pointer when the
completed plan is retired to Git history.
