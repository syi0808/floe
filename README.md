# Floe

Floe is an open-source personal assistant that helps a person's day run well by understanding their timeline, current state, and durable personal context.

The product is not an agent framework, automation builder, chat wrapper, or dashboard. Its long-term experience is one ambient assistant, with visual surfaces such as the **Day Canvas** for context, explicit approval, inspection and recovery. Voice, wake-up and cross-device delivery remain product roadmap work, not prerequisites for current macOS stabilization.

## Current work

Current engineering focus is **macOS stability and product quality** on top of the converged modular-monolith architecture. iOS/iPad validation resumes with active platform development; Android remains outside the current implementation scope unless explicitly requested.

Current source and manifests define implementation reality. Use the architecture documents for semantic ownership and the nearest component runbook for build/validation commands. Create a task-specific execution plan only when a change is large enough to require caller migration or multiple checkpoints.

## Start here

| Need | Document |
|---|---|
| Find the authoritative document for a task | [Documentation map](docs/README.md) |
| Understand current architecture and ownership | [Architecture](docs/architecture/README.md) |
| Understand why an architectural decision exists | [ADR index](docs/decisions/README.md) |
| Read product meaning and long-term scope | [Product](docs/product/README.md) |
| Work on presentation | [Design system](DESIGN.md) and [screen specifications](docs/design/README.md) |
| Build or diagnose the client | [Client guide](apps/client/README.md) and [Agent debugging](docs/development/agent-debugging.md) |
| Configure the loopback model/provider gateway | [Server guide](server/README.md) |

Do not recursively read the whole documentation tree. Use [docs/README.md](docs/README.md) as the router and follow only the material relevant to the task.

## Repository boundaries

The workspace follows the modular-monolith ownership described in [Architecture](docs/architecture/README.md). The dependency policy in `tools/architecture/module-dependencies.json` defines allowed ownership edges and `tools/architecture/check_boundaries.py` checks the actual Cargo manifests.

Rust owns Session/Run, Expert Task and Connection Operation semantics through their respective modules. Flutter consumes typed commands and an application-lifetime read model. Native and Go adapters own actual OS/provider access. Host eligibility and authority checks remain separate from the Manager LLM's choice of Expert.

## Development

Use the repository's pinned toolchain inputs and [AGENTS.md](AGENTS.md). Rust requires 1.93 or newer and the client guide describes the Flutter/Apple build setup. Apple platforms are the current priority; dormant Android code is not a current delivery gate.

The Cargo development profile omits debug information for external dependencies, while Floe crates retain full debug information for normal builds. Tests use line-level debug information for Floe crates to limit repeated test-build artifacts. When stepping through dependency internals or inspecting test-local variables, temporarily raise the relevant `Cargo.toml` profile's `debug` setting and rebuild. Turso's FTS feature is disabled because Floe does not use FTS SQL.

During structural work, check the affected module and boundaries rather than starting the app or running every suite after each edit:

```sh
cargo check -p floe-conversation
python3 tools/architecture/check_boundaries.py
git diff --check
```

The final structure review includes workspace type/compile checks, actual dependency and caller inspection, and old-path removal. Narrow controlled safety checks remain necessary when their semantics change. Run broad product-boundary validation when a change touches those surfaces; the client/server runbooks and validation tools define the current commands. Commands here are instructions, not a claim that checks passed on this commit.

## Safety and compatibility

Keep inspectable, source-backed memory, explicit action approval, exact-recipient consent, key identity, provenance, durable pre-dispatch intent and uncertain-write recovery. Query, preview and screen disposal are not implicit conversation cancellation.

Floe is pre-stable, so internal backward compatibility is not a default goal unless an external protocol or durable user-data requirement makes it one. Do not add parallel v2/v3 implementations merely to preserve obsolete local callers. A fixed schema/version number does not make old binaries or data compatible: build the client and bundled library together, and explicitly select fresh Floe development data when stored meaning changes. Never automatically replace keys or delete data on an access error.

Historical implementation plans and obsolete checkpoints live in Git history. They are not current execution policy or acceptance evidence.
