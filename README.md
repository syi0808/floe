# Floe

Floe is an open-source personal assistant that helps a person's day run well by understanding their timeline, current state, and durable personal context.

The product is not an agent framework, automation builder, chat wrapper, or dashboard. Its long-term experience is one ambient assistant, with visual surfaces such as the **Day Canvas** for context, explicit approval, inspection and recovery. Voice, wake-up and cross-device delivery remain product roadmap work, not prerequisites for the current refactor.

## Current work

Floe is undergoing a **single-agent, structure-first refactor** toward the approved modular monolith. One coding agent completes contracts, state ownership, real adapters and callers, and removes old paths before broad product validation. This does not remove the product's Manager-to-Expert A2A delegation.

The reviewed code snapshot is `89452eb5523ef6b1c76b7fe857095748d76de22d` (2026-09-15). General conversation already uses the Conversation service and generic Engine, with durable Run/Task records and typed command/query/event transport. The Calendar-first root branch has been removed. Session and management bridges, FFI-owned composition and some Expert-specific dispatch still require replacement; the single target API is a goal, not a completed cutover.

The latest recorded normal macOS app attempt ended in `VaultUnavailable` before model routing. Component and controlled integration records do **not** establish successful normal-app chat. Current state, evidence boundaries and the next task live only in the [migration ledger](docs/architecture/migration-ledger.md).

## Start here

| Need | Document |
|---|---|
| Implement the current refactor | [Active implementation plan](docs/architecture/implementation-plan.md) |
| Give the coding agent its execution instructions | [Single-agent prompt](docs/architecture/agent-prompt.md) |
| Resume work and distinguish structure from behavior evidence | [Migration ledger](docs/architecture/migration-ledger.md) |
| Understand document ownership and historical records | [Architecture documentation guide](docs/architecture/README.md) |
| Read product requirements and long-term scope | [Product planning](docs/planning/README.md) |
| Work on presentation | [Design system](DESIGN.md) and [screen specifications](docs/design/README.md) |
| Build or diagnose the client | [Client guide](apps/client/README.md) and [Agent debugging](docs/development/agent-debugging.md) |
| Configure the loopback model/provider gateway | [Server guide](server/README.md) |

## Repository boundaries

The current workspace is transitional. Existing `crates/contracts/`, `runtime/`, `modules/`, `platform/`, `adapters/` and `app/` coexist with legacy `floe-*` paths. The [approved dependency policy](tools/architecture/module-dependencies.json) describes the target boundaries; the [plan](docs/architecture/implementation-plan.md) identifies remaining moves and deletions.

Rust owns Session/Run, Expert Task and Connection Operation semantics through their respective modules. Flutter consumes typed commands and an application-lifetime read model. Native and Go adapters own actual OS/provider access. Host eligibility and authority checks remain separate from the Manager LLM's choice of Expert.

## Development

Use the repository's pinned toolchain inputs and [AGENTS.md](AGENTS.md). Rust requires 1.93 or newer and the client guide describes the Flutter/Apple build setup. Apple platforms are the current priority; dormant Android code is not a refactor delivery gate.

During structural work, check the affected module and boundaries rather than starting the app or running every suite after each edit:

```sh
cargo check -p floe-conversation
python3 tools/architecture/check_boundaries.py --mode migration
git diff --check
```

The final structure review includes workspace type/compile checks, actual dependency and caller inspection, and old-path removal. Narrow controlled safety checks remain necessary when their semantics change. Broad regressions, the actual Apple app, Keychain, OAuth and real-model evaluation belong to Stage B in the plan. Commands here are instructions, not a claim that checks passed on this commit.

## Safety and compatibility

Keep inspectable, source-backed memory, explicit action approval, exact-recipient consent, key identity, provenance, durable pre-dispatch intent and uncertain-write recovery. Query, preview and screen disposal are not implicit conversation cancellation.

This local-development refactor does not maintain old APIs or data formats. Do not add parallel v2/v3 implementations or increase schema numbers merely for this rewrite. A fixed number does not make old binaries or data compatible: build the client and bundled library together, and explicitly select fresh Floe development data when stored meaning changes. Never automatically replace keys or delete data on an access error.

Historical slice plans, validation results and imported implementation bundles remain available through the [history index](docs/architecture/history/README.md). Their old next-demo instructions and test counts are not current execution policy or acceptance.
