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

## Go server owner boundaries

The Go server follows the same ownership direction. `server/internal/contracts/source` contains pure source identity, snapshot, descriptor and bounds values shared by source readers and authorization. `server/internal/views` owns View preview, admission, parsing, bounded read/validation and release orchestration. `server/internal/authority` owns signed admission/release enforcement, proof checks, one-use state, staging limits and source-fence enforcement; it implements the inward Views port without importing the Views application package or selecting a reader. `server/internal/integrations` resolves current source identity and owns connection, attempt, cleanup and receipt state. `server/internal/trust` owns principal, issuer, producer identity, pairing activation and revocation state. Trust, Integrations and Inference define their typed repository/configuration ports in their owner packages. `server/internal/adapters/storage` implements encrypted persistence; its `privatefiles` subpackage owns low-level private-file mechanics. `server/internal/adapters/credentials` owns platform credential stores and scoped owner capabilities. Pairing defines typed operations for its receipt index, attempt receipts and pairing tokens; the credential adapter derives and validates their exact slots before store IO.

Concrete server boundaries live under `server/internal/adapters`: provider connectors and lifecycle setup are in `integrations`, OAuth runtimes are in `oauth`, and Codex plus other inference executors are in `models`. These adapters depend on owner contracts; `server/internal/node` composes them. `server/internal/transport/http` owns request/response DTOs, JSON projection and HTTP status mapping.

Product Calendar Mirror remains a specialized Views workflow over the product query and page contract. Authority validates and signs its distinct product permit through a Views-owned enforcement port; the generic Assistant View permit cannot authorize it. HTTP projection remains transport-owned. Typed result expansion and model metadata/capability projection are separate from adapter placement.

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

The [architecture refactor plan](../plans/2026-10-02-architecture-refactor.md) owns the active cutover sequence and its compilation, build and reconstructed-test gates. A source implementation is not qualification: consult that plan's execution evidence for passed and pending validation. Earlier checkpoint plans live in Git history; they do not override the current P0–P6 sequence. The [DeviceModel contract](device-model-contract.md) and [native Calendar effect contract](native-calendar-effects.md) remain maintained boundary references.
