# Floe migration ledger

This is the only mutable refactoring progress record. The active [plan](implementation-plan.md) and [execution prompt](agent-prompt.md) define work; this ledger records what the code and evidence actually establish.

## Current checkpoint

| Field | State |
|---|---|
| Source reviewed for this documentation refresh | `89452eb5523ef6b1c76b7fe857095748d76de22d` |
| Current stage | **A — structural refactoring; not complete** |
| Execution | One coding agent, one sequential change set; product Manager/Expert A2A is retained |
| Current change | Install the approved sequential plan/prompt, reconcile entry-point docs and archive obsolete planning context |
| Runtime changes in this refresh | None; no schemas, dependencies, runtime APIs, authorization checks or tests changed |
| Product validation in this refresh | `not_run`; no new application, model or provider success is claimed |
| Latest recorded app observation | Normal macOS debug app ended in `VaultUnavailable` before route selection; cause not confirmed |
| Next code task | Plan §3.1: inspect current public contracts and direct consumers, then fix canonical command/owner boundaries; continue with §3.2 rather than starting live-app diagnosis |

The source SHA is a review anchor, not a claim that subsequent HEADs were checked. Documentation installation is not completion of the refactor's contract work.

## Current implementation and remaining structure

The workspace at the review anchor contains 23 crates: 17 target-path crates and six legacy-path crates. Crate counts are not completion percentages. The final 22-module boundary policy is unchanged.

| Existing P scope | Implemented or wired at the review anchor | Remaining structural work |
|---|---|---|
| P00/P01 | Earlier inventory/tooling and contract crates exist; this refresh installs the agreed workflow | Complete canonical public contracts and remove legacy value/wire duplication; preserve P IDs |
| P02/P03 | Execution scopes/budgets and the generic Engine exist; general conversation calls the new Conversation service/Engine | Durable pending batch/cursor, scoped failure meaning and one attempt-accounting owner |
| P04/P09 | Access, Context, provenance, bounded views/history and prior safety fixes exist | Complete dispatch/release boundaries, lazy source composition and final storage placement without relaxing authority |
| P05/P08 | Day and Knowledge services, policies and repository ports exist | Move remaining concrete stores/callers and narrow public APIs; do not rewrite working policy algorithms |
| P06/P20 | Pairing service and portions of the Go authority/provider split exist | Durable Connection/OAuth Operations, reconciliation and remaining Console/Widget-owned state |
| P07 | Typed inference routing, recipient checks and host-side route selection are wired | Remove synchronous route/catalog I/O from FFI admission; keep mutable route data out of command identity |
| P10 | Approved actions and uncertainty protections remain in legacy Core/Vault code | Move valid behavior to the Actions owner and its actual repository/adapters |
| P11/P12 | Durable Task coordinator, Conversation Run admission/query/cancel, retry lineage and recovery code are wired | Finish canonical failure/journal/Session contracts and remove legacy repository/composition bridges |
| P13/P14 | Real encrypted storage remains under Core/FFI; target native/provider crates currently cover local identity | Move actual Vault/model/source/control/OS implementations to final adapters; add safe key/Vault error stages |
| P15 | Calendar-first root was removed; Schedule uses the common root and Task path | Remove central builtin dispatch, parent-model card filtering and special endpoint staging; finish independent registered endpoints |
| P16/P17 | AppHost lifetime, verified caller, typed app command/query/events and Dart transport/client are wired | Replace `LegacyComposition`; one unversioned-name API; typed Session/management; bounded host/worker shutdown |
| P18/P19 | App-lifetime conversation read model and product turn/cancel/retry path are wired | Complete other slices, move bootstrap out of Day, remove legacy Session/management gateway and unrelated busy aggregation |
| P21 | Existing Apple build/native paths and diagnostics support prior checkpoints | Align final paths/bindings and compile the target structure; perform normal-app Keychain investigation in Stage B |
| P22/P23 | Historical focused checks and the migration dependency checker exist | Final structure checks, then bounded regressions/product scenarios on the actual integrated snapshot |
| P24/P25 | Legacy code remains; neither final structural nor product acceptance is established | Delete replaced paths and perform the plan's separate A and B reviews |

These are source-level and recorded-checkpoint summaries, not new test passes. Useful starting locations include `crates/modules/conversation`, `crates/modules/experts`, `crates/runtime/agent`, `crates/floe-ffi/src/vault_host`, `crates/floe-ffi/src/inference_routes.rs`, and `apps/client/lib/runtime_client`. The active plan contains fixed source anchors and exact remaining changes.

## Execution policy applied from this checkpoint

1. Follow plan §3.1–3.8 sequentially for Stage A. Implement actual owner logic, adapters and callers; remove the old counterpart as each change closes. Do not create subagents, parallel coding branches or compatibility wrappers.
2. Keep app wire 2 and Conversation storage marker 7 at the reviewed baseline; do not add schema bumps or force later values backward. Fixed numbers do not imply old-data compatibility. Authority revisions, generations and epochs still advance.
3. During A, use relevant compile/type, dependency and caller/removal checks. Only design-critical feasibility and changed high-risk invariants justify narrow controlled behavior checks early.
4. Preserve the recorded Vault failure as a Stage B item. Implement safe native error classification in A, but do not block unrelated structural work on live Keychain, OAuth or model success.
5. The current code still exports `_v2` symbols and uses legacy Session/management paths. Their removal is planned, not accomplished by updating documentation.

## Evidence and history

The [complete ledger through 89452eb](history/migration-ledger-through-89452eb.md) preserves the original baseline inventory, P/T references, code checkpoints and execution records without changes. Its obsolete top status table and historical next tasks are not current instructions. Use the [history index](history/README.md) for original paths and the imported plan bundle.

The last code checkpoint recorded there is `2f8818c`, documented by `89452eb`. It records focused adapter/FFI/C-ABI/Flutter checks and a normal-app Vault failure. Broad Clippy was not a pass; a successful Keychain-backed local or remote StartTurn/retry was not established. Those outcomes remain attached to that checkpoint and were not rerun for this documentation refresh.

## Checkpoint format and resumption

Update this current record instead of copying status to README, PROGRESS, a STATUS JSON or another plan. Keep closed detailed evidence in a dated historical record and link it.

```text
Stage / plan section / existing P / one change responsibility
Actual HEAD and any relevant uncommitted changes
Structure: implemented / concrete caller wired / old path removed / remaining boundary
Checks: exact command, result, environment and source snapshot
Behavior: not_run | passed | failed | environment_blocked, with scope
Unfinished: contract, compile issue, removal or required check
Next: one executable task, with first file and symbol
```

On resumption, read this checkpoint, inspect HEAD/status/diff, and finish any interrupted change before starting the next one. `wired` describes code composition; `passed` requires execution. Stage A completion is reported as **structure complete / behavior validation pending**, never as product stability.
