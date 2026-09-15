# Architecture documentation

## Read only what the current task needs

| Question | Authoritative document |
|---|---|
| How should this coding agent work? | [agent-prompt.md](agent-prompt.md), with the repository's [AGENTS.md](../../AGENTS.md) |
| What structure and remaining changes are approved? | [implementation-plan.md](implementation-plan.md) |
| What has actually been implemented, removed or checked? | [migration-ledger.md](migration-ledger.md) |
| Which internal dependencies are allowed? | [module-dependencies.json](../../tools/architecture/module-dependencies.json) |
| What does the product intend to do? | [Product planning](../planning/README.md) and relevant accepted decisions |
| What did an earlier run or plan say? | [Historical refactor records](history/README.md), [product history](../history/README.md) and [validation records](../validation/) |

Start with the ledger's current checkpoint, then read the active plan section and actual symbols. Do not load the old implementation bundle, every checkpoint, or all past conversations as routine context.

## Current execution policy

One coding agent works sequentially in one workspace. Stage A implements real owner services, adapters, callers and the approved dependency graph, removes old paths, and performs structural checks plus limited safety checks. Stage B performs broad regressions, actual app/Keychain/provider validation and model evaluation. Product A2A delegation and Run/Task concurrency are not removed by the single-coding-agent workflow.

Backward compatibility, parallel v2/next implementations and additional schema bumps are not required. Preserve authorization, key identity, provenance, transaction atomicity, durable intent and current-schema recovery. The active plan specifies exact scope and constraints.

## Document maintenance

- Keep one active plan and one execution prompt here. Put actual progress only in `migration-ledger.md`; README and PROGRESS are navigation, not competing boards.
- Distinguish target design, source-level wiring, checks executed on a particular snapshot, and live product acceptance. Never update a historical test result to imply it ran on new code.
- Keep the current ledger concise. Move closed historical checkpoints into a dated record and link them; preserve evidence and source IDs rather than duplicating it across new summaries.
- Imported plans and former delivery sequences are historical. Read them for a specific rationale or old source anchor, not as current implementation instructions.
- Relative paths inside immutable archived snapshots reflect their original location. The history index provides the original commit view when those paths are needed.

The documentation refresh itself does not implement the target API, remove live Legacy code, change a schema constant, or certify product behavior.
