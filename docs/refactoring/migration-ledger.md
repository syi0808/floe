# Floe migration ledger

This is the only mutable refactoring progress record. The [active document edition](README.md) selects the plan, detailed execution instructions and prompt. Published editions keep their own history; they are not status logs.

## Current checkpoint

| Field | State |
|---|---|
| Source reviewed for this documentation edition | `cfde8e24387454d519c9e3308606a7cc6bb7f6c9`; product trees unchanged from `89452eb5523ef6b1c76b7fe857095748d76de22d` |
| Active plan | **R003**, [PLAN](versions/r003-structure-first/PLAN.md) / [execution](versions/r003-structure-first/EXECUTION_PLAN.md) |
| Current stage | **A — structural refactoring; not complete** |
| Execution | One coding agent, one sequential change set; product Manager/Expert A2A retained |
| Current change | Separate refactoring history from architecture, preserve R001/R002, publish the all-stage R003 execution instructions |
| Runtime changes in this refresh | None; no product sources, schemas, dependencies, runtime APIs or active tests changed |
| Product validation in this refresh | `not_run`; documentation checks do not establish application/model/provider success |
| Latest recorded app observation | Normal macOS debug app ended in VaultUnavailable before route selection; cause not confirmed |
| Next code task | R003 01.1–01.4: inspect actual contracts/callers, fix canonical intent and shared archive type ownership; then continue with 02 |

The SHA is a fixed review anchor. A later implementation session must inspect its actual HEAD and dirty diff. Document publication is not completion of the contract work.

## Current implementation and remaining structure

At the anchor the workspace has 23 crates, including 17 final target-path crates and six legacy-path crates. The approved target remains 22 packages; counts are not completion percentages.

| Existing P scope | Implemented or wired at the anchor | Remaining structural work |
|---|---|---|
| P00/P01 | Earlier inventory, contract crates and approved workflow exist | Finish canonical input/owner/port contracts; remove legacy value/wire duplication |
| P02/P03 | Execution scopes/budgets and generic Engine; actual general conversation uses the new service/Engine | Saved pending batch/cursor, scoped failure and one attempt-accounting owner |
| P04/P09 | Access, Context, lineage, bounded views/history and earlier safety fixes | Complete dispatch/release and lazy source composition; keep archive values out of Context runtime dependencies |
| P05/P08 | Day/Knowledge policy, services and repository ports | Remaining concrete stores/callers, transaction-bound evidence and narrow public APIs |
| P06/P20 | Pairing service and parts of Go authority/provider separation | Durable Connection/OAuth Operations, reconciliation, remaining Console/Widget state |
| P07 | Typed routing, recipient consent and host-side route selection | Remove synchronous route/catalog I/O from FFI admission; separate route observation from command identity |
| P10 | Approval/preflight/uncertainty protections in legacy Core/Vault | Move actual behavior into Actions and its repositories/adapters |
| P11/P12 | Durable Task coordinator and Conversation admission/query/cancel/retry/recovery | Canonical failure/journal/Session contracts; remove legacy repository/composition bridges |
| P13/P14 | Real encrypted storage under Core/FFI; native/provider target starts with identity | Move actual Vault/model/source/control/OS implementations, preserve key diagnostics and current-schema recovery |
| P15 | Calendar-first root removed; Schedule uses common root/Task path | Remove builtin dispatch, parent-model filter and context staging; register independent endpoints |
| P16/P17 | AppHost lifetime/caller, app command/query/events, Dart client/transport wired | Replace LegacyComposition; one API without version suffix; complete Session/management and safe shutdown |
| P18/P19 | App-lifetime conversation read model and visible turn/cancel/retry path | Other slices, bootstrap outside Day, remove legacy Session/management gateway and unrelated busy OR |
| P21 | Apple build/native paths and prior diagnostics | Align final bindings/paths and safe key/Vault stages; actual Keychain diagnosis remains B |
| P22/P23 | Historical checks and dependency checker exist | Final actual structure checks, then current-snapshot behavioral acceptance |
| P24/P25 | Legacy remains; neither full structure nor product acceptance established | Delete replaced paths and perform separate A/B reviews |

These are source and prior checkpoint summaries, not new passes. R003 source windows and step prescriptions are fixed to the reviewed commit. Active code locations change only when an implementation change is actually performed.

## Policy for subsequent implementation

1. Follow R003 steps 01–08 sequentially for A. Implement real owner logic, repositories/adapters and callers, then remove old counterparts. Do not spawn subagents or add compatibility wrappers.
2. Keep the currently adopted schema numbers; reviewed app wire is 2 and Conversation marker 7. Do not force later HEAD values backward. Authority revision, executor generation and epochs still advance.
3. Use relevant type/compile/DAG/caller checks during A. Only changed high-risk meaning and design-critical feasibility justify early narrow controlled behavior checks.
4. Add safe native error classification during A. Reproduce/fix the known normal-app Keychain issue during B instead of blocking unrelated structure work on live UI/OAuth/LLM success.
5. Current `_v2` exports and legacy Session/management paths have not been removed by this documentation commit. Their replacement is work specified in R003, not accomplished evidence.

## Evidence and history

[Full ledger through 89452eb](history/migration-ledger-through-89452eb.md) preserves the original inventory/P/T/code/test records. [R002 snapshot](versions/r002-sequential/MIGRATION_SNAPSHOT.md) preserves the previous concise current ledger without changes. Both are historical, not competing mutable records.

The last recorded code checkpoint is `2f8818c`, documented by `89452eb`. It reported focused adapter/FFI/C ABI/Flutter checks and a normal-app Vault failure. Broad Clippy was not a pass; successful Keychain-backed local/remote StartTurn/retry was not established. No product test was rerun for R003 publication.

## Recording and resumption

```text
Document edition / step and substep / existing P / one responsibility
Actual HEAD plus relevant dirty changes
Structure: implemented / real caller wired / old path removed / remaining boundary
Checks: actual command, result, environment and source snapshot
Behavior: not_run | passed | failed | environment_blocked, with scope
Unfinished: contract, compile issue, removal or required check
Next: one executable task and its first file/symbol
```

Resume by reading this checkpoint, inspecting HEAD/status/diff, and completing any interrupted change. Keep detailed completed evidence in history with its source/environment. Do not copy progress into published plans, README, PROGRESS or a new STATUS file. `wired` is code composition; `passed` requires execution. A completion is **structure complete / behavior validation pending**, not product stability.
