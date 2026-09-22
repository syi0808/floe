# Current architecture

This directory describes Floe's **current semantic ownership and repository boundaries**. It is the first source to read when a code task needs architectural context.

The physical Rust workspace has already moved to the approved modular-monolith package layout. Stage 2 is complete: the internal General Conversation runtime goes through the canonical owner services. Delegated Experts, Schedule and the Knowledge Learner also execute models through canonical Inference. Remaining product-boundary callers (AppHost, FFI, Flutter, native/server and the live AgentFixture runtime) converge in Stage 3. Do not confuse a transitional caller with ownership: temporary compatibility does not transfer policy to App, FFI, Flutter or an adapter.

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
| Completed internal-runtime cutover | [Stage 2](../refactoring/stage-2.md) (complete · frozen) |
| Active product-boundary cutover | [Stage 3](../refactoring/stage-3.md) |

## Source-of-truth rules

- `Cargo.toml` manifests are the current physical dependency graph.
- `tools/architecture/module-dependencies.json` is the **allowed dependency policy**, not a manually copied current graph.
- `tools/architecture/check_boundaries.py` compares manifests with that policy and requires the current target packages and paths.
- These architecture documents describe stable ownership and explicitly call out active transitions.
- `invariants.md` defines cross-cutting final-state properties; [architecture evolution](../development/architecture-evolution.md) defines how changes converge back to those properties.
- Stage documents own execution status. ADRs own rationale. Neither replaces current architecture documentation.

## Current transition

Stage 1 completed semantic ownership placement. Stage 2 converged the real internal runtime on that ownership, including canonical model, Context Tool and Expert delegation paths. Stage 3 now removes outer product-boundary routing/compatibility from AppHost, FFI, Flutter, native and server callers.

The active checkpoint and exact remaining work belong only in [Stage 3](../refactoring/stage-3.md); this file intentionally does not copy its progress checklist.
