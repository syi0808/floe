# Floe

Floe is an open-source personal assistant that helps a person's day run well by understanding their timeline, current state, and durable personal context.

The product is not an agent framework, automation builder, chat wrapper, or dashboard. Its long-term experience is one ambient assistant, with visual surfaces such as the **Day Canvas** for context, explicit approval, inspection and recovery. Voice, wake-up and cross-device delivery remain product roadmap work, not prerequisites for the current refactor.

## Current work

Floe is undergoing a three-stage refactor toward the approved modular monolith:

1. [Stage 1 — Physical Ownership](docs/refactoring/stage-1.md): complete.
2. [Stage 2 — Canonical Internal Runtime](docs/refactoring/stage-2.md): complete.
3. [Stage 3 — Product Boundary and Final Composition](docs/refactoring/stage-3.md): active.

Stage 1 established semantic owners. Stage 2 converged the actual General Conversation runtime on those owners. Stage 3 is carrying that architecture through AppHost, FFI, Flutter, native/server callers and remaining domain-specific roots.

There is no separate versioned refactoring edition or migration ledger. The Stage overviews are the refactoring source of truth; Stage 2 and Stage 3 link to step-specific execution plans. Stage 3 contains the active progress checklist and current checkpoint.

## Start here

| Need | Document |
|---|---|
| Find the authoritative document for a task | [Documentation map](docs/README.md) |
| Resume the active refactor | [Stage 3 — Product Boundary and Final Composition](docs/refactoring/stage-3.md) |
| Understand current architecture and ownership | [Architecture](docs/architecture/README.md) |
| Understand why an architectural decision exists | [ADR index](docs/decisions/README.md) |
| Read product meaning and long-term scope | [Product](docs/product/README.md) |
| Work on presentation | [Design system](DESIGN.md) and [screen specifications](docs/design/README.md) |
| Build or diagnose the client | [Client guide](apps/client/README.md) and [Agent debugging](docs/development/agent-debugging.md) |
| Configure the loopback model/provider gateway | [Server guide](server/README.md) |

Do not recursively read the whole documentation tree. Use [docs/README.md](docs/README.md) as the router and follow only the material relevant to the task.

## Repository boundaries

The current workspace is transitional while Stage 3 carries the canonical internal runtime through the product boundary. The architecture dependency policy in `tools/architecture/module-dependencies.json` defines allowed ownership edges; [Stage 3](docs/refactoring/stage-3.md) defines the remaining product-boundary cutover and deletion gates, with [Stage 2](docs/refactoring/stage-2.md) as the completed internal-runtime record.

Rust owns Session/Run, Expert Task and Connection Operation semantics through their respective modules. Flutter consumes typed commands and an application-lifetime read model. Native and Go adapters own actual OS/provider access. Host eligibility and authority checks remain separate from the Manager LLM's choice of Expert.

## Development

Use the repository's pinned toolchain inputs and [AGENTS.md](AGENTS.md). Rust requires 1.93 or newer and the client guide describes the Flutter/Apple build setup. Apple platforms are the current priority; dormant Android code is not a refactor delivery gate.

During structural work, check the affected module and boundaries rather than starting the app or running every suite after each edit:

```sh
cargo check -p floe-conversation
python3 tools/architecture/check_boundaries.py
git diff --check
```

The final structure review includes workspace type/compile checks, actual dependency and caller inspection, and old-path removal. Narrow controlled safety checks remain necessary when their semantics change. Broad product-boundary validation, the actual Apple app, Keychain/OAuth integration and end-to-end real-model evaluation are closed in Stage 3 rather than used as a gate after every internal-runtime edit. Commands here are instructions, not a claim that checks passed on this commit.

## Safety and compatibility

Keep inspectable, source-backed memory, explicit action approval, exact-recipient consent, key identity, provenance, durable pre-dispatch intent and uncertain-write recovery. Query, preview and screen disposal are not implicit conversation cancellation.

This local-development refactor does not maintain old APIs or data formats. Do not add parallel v2/v3 implementations or increase schema numbers merely for this rewrite. A fixed number does not make old binaries or data compatible: build the client and bundled library together, and explicitly select fresh Floe development data when stored meaning changes. Never automatically replace keys or delete data on an access error. Refactoring Stage numbers are planning boundaries only; they are independent of application/schema versions.

Historical refactoring plans and obsolete implementation checkpoints live in Git history. They are not current execution policy or acceptance evidence.
