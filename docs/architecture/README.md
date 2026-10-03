# Current architecture

This directory describes Floe's **current semantic ownership and repository boundaries**. It is the first source to read when a code task needs architectural context.

The Rust workspace uses the approved modular-monolith package layout. General Conversation, delegated Experts, Schedule and the Knowledge Learner execute through their canonical owners and shared Inference. Product callers enter through admitted AppHost services; FFI, Flutter, native and server layers translate intent or implement real external boundaries without owning model, source, grant or action policy.

## Layer map

```text
Product callers
Flutter / native / server
        |
        v
Bindings
protocol -> FFI
        |
        v
App
composition and service wiring
        |
        +-------------------------------+
        |                               |
        v                               v
Business modules                    Generic runtime
Conversation  Experts               Agent Runtime
Context       Access                Execution
Inference     Connections
Actions       Knowledge
Day
        |
        v
Adapters / platform
Vault  Providers  Native  Diagnostics
        |
        v
OS / provider / storage
```

Pure cross-owner value contracts live in `crates/contracts/`. Built-in Experts are extensions and may depend on owner APIs; business modules must not depend on the built-in package.

## Read next

| Need | Document |
|---|---|
| Repository-wide ownership, path and boundary invariants | [Architecture invariants](invariants.md) |
| Exact owner/package responsibilities | [Modules and ownership](modules.md) |
| General Conversation model/tool/delegation flow | [Runtime](runtime.md) |
| Authority, provenance, crash recovery and side-effect invariants | [Authority and recovery](authority-recovery.md) |
| Allowed Rust dependency edges | [Dependency policy](../../tools/architecture/module-dependencies.json) |
| Why a durable decision exists | [ADR index](../decisions/README.md) |

## Source-of-truth rules

- `Cargo.toml` manifests are the current physical dependency graph.
- `tools/architecture/module-dependencies.json` is the **allowed dependency policy**, not a manually copied current graph.
- `tools/architecture/check_boundaries.py` compares manifests with that policy and requires the current target packages and paths.
- These architecture documents describe stable current ownership and runtime boundaries.
- `invariants.md` defines cross-cutting final-state properties; [architecture evolution](../development/architecture-evolution.md) defines how changes converge back to those properties.
- Task-specific execution plans may own temporary sequencing. ADRs own durable rationale. Neither replaces current architecture documentation.

## Current state and accepted decisions

The modular-monolith boundaries give each product capability one semantic owner. [ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md) defines source-owned processing authority, verified Gateway-primary/local-fallback reasoning and the mandatory Health-local privacy transform. Current source uses these contracts across the common Engine, Conversation, Experts, Knowledge and Context; runtime and authority documents describe those owner paths.

The [architecture refactor plan](../plans/2026-10-02-architecture-refactor.md) owns the active cutover sequence and its compilation, build and reconstructed-test gates. A source implementation is not qualification: consult that plan's execution evidence for passed and pending validation. Earlier reasoning/source-processing and agent-execution-environment-grounding plans remain historical design evidence and do not restart their old checkpoints.
