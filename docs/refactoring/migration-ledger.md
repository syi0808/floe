# Floe migration ledger

This is the only mutable refactoring progress record. The [active document edition](README.md) selects the plan, detailed execution instructions and prompt. Published editions keep their own history; they are not status logs.

## Current checkpoint

| Field | State |
|---|---|
| Source reviewed for this documentation edition | `40d9397fe31c9dda42073e82dc04ef735a230806`; text-contract implementation checkpoint `64ab369` |
| Active plan | **R003**, [PLAN](versions/r003-structure-first/PLAN.md) / [execution](versions/r003-structure-first/EXECUTION_PLAN.md) |
| Current stage | **A — structural refactoring; not complete** |
| Execution | One coding agent, one sequential change set; product Manager/Expert A2A retained |
| Current change | R003 step 01.2: align turn text normalization and boundary validation; preserve the existing explicit profile forwarding path |
| Runtime changes in this refresh | Rust canonical turn text normalization now drives the Dart precheck, app-wire admission, App service forwarding, Conversation intent/digest, stored user message and Engine prompt; explicit profile selection remains preserved; app wire schema remains 2 |
| Contract validation in this refresh | `cargo test -p floe-app`, `cargo test -p floe-protocol --test app_wire_v2`, `cargo test -p floe-conversation`, `cargo test -p floe-ffi --lib app_wire::tests`, `flutter test test/runtime_client/floe_client_test.dart`, and `git diff --check` pass; no live Apple app/model/provider behavior was exercised |
| Product validation in this refresh | `not_run` |
| Latest recorded app observation | Normal macOS debug app ended in VaultUnavailable before route selection; cause not confirmed |
| Next code task | R003 02.1: inspect `crates/modules/connections/src/api.rs` and introduce the concrete `ConnectionIntent`/`ConnectionObservation` owner boundary |

The SHA is a fixed review anchor. A later implementation session must inspect its actual HEAD and dirty diff. Document publication is not completion of the contract work.

## R003 step 01 owner map

| Existing feature | Owner and implementation location | Current caller / new public boundary | Remaining removal |
|---|---|---|---|
| create/unlock/lock, permission review and revocation | Access; existing `floe-app` host and native/Vault ports | `AppHost` caller context and Access service ports | Move remaining direct Vault access behind Access owners |
| Session, conversation, Continue, Retry, Cancel, archive | Conversation; `crates/modules/conversation` and `floe_conversation` ports | `FloeClient` → app wire → `ConversationCommands`; canonical `StartTurn`, `CanonicalTurnIntent`, `ArchiveReadRequest` | Remove legacy Session/management gateways in later steps |
| Registry, Expert configuration and A2A Task | Experts; `crates/modules/experts` | existing typed Expert/Task ports and A2A adapters | Complete owner repository composition |
| Pairing/OAuth, connection lookup, refresh and disconnect | Connections; `crates/modules/connections` and provider adapters | existing connection service/provider callers | Add durable Operations and reconciliation boundaries |
| model profile, attempt, receipt and usage | Inference; current model/runtime ports and execution records | Conversation model port and runtime receipts | Extract durable Inference owner and attempt accounting |
| Memory/Playbook review and learner | Knowledge; `crates/modules/knowledge` | existing review/learner services and repositories | Finish concrete stores and transaction-bound evidence |
| Calendar/Task/Note reads, capture and mirror | Day; `crates/modules/day` | existing Day queries and action/context callers | Remove remaining staging and builtin dispatch paths |
| proposal, approval, execution and uncertain-result reread | Actions; current Core/Vault action repositories | existing action command/query callers | Move behavior from legacy Core/Vault composition |
| OS authorization UI and acquisition callback | Native driver; native adapters | `floe-ffi` native action/context bridges | Complete owner-specific request lifetimes |

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

### R003 step 01 evidence

- **Structure:** `StartTurn`/`CanonicalTurnIntent` owns normalized text, retry/Continue validation, profile preference, and principal-bound fixed-field digest; canonical archive values and `ArchiveReader` live in `floe-agent-contract`; `CommandQuery` scopes command lookup to the principal.
- **Wiring/removal:** The app wire maps explicit profile to the app facade, `LegacyComposition` carries it through `AgentConversationTurnRequestDto`, and the vault turn maps it into `Conversation::TurnRequest` instead of forcing `Auto`; Flutter conversation requests and `FloeClient` serialize the same selection. Context-owned archive value definitions and `request_context_digest` callers are removed. ABI `_v2` names remain until the prescribed Stage 07 consumer replacement.
- **Checks:** `cargo check -p floe-ffi -p floe-conversation -p floe-protocol` passed; `cargo test -p floe-protocol -p floe-conversation` passed (6 app-wire, 16 Conversation, 14 protocol tests); `cargo test -p floe-ffi --lib app_wire::tests` passed (3 tests); `flutter test test/runtime_client/floe_client_test.dart test/features/conversation/conversation_runtime_gateway_test.dart` passed (15 tests); `python3 tools/architecture/check_boundaries.py . --mode migration` passed with only expected migration warnings; `git diff --check` passed. `rustfmt --check` remains not clean because of unrelated pre-existing formatting in the touched legacy FFI test/source files; no formatter-only changes were applied.
- **Behavior:** `not_run`; no live macOS app, Keychain/Vault diagnosis, OAuth, provider, or model behavior was exercised.
- **Unfinished:** Stage 01 does not remove final ABI aliases or legacy composition; those require the later caller replacement. No data regeneration is indicated by the archive type-path move.

### R003 step 01.2 text contract evidence

- **Completed scope:** Rust `char::is_whitespace`-based trimming is the canonical definition; Dart uses the matching explicit code-point set. Normalized text must be non-empty, no more than 8192 UTF-8 bytes, and may retain only LF among control characters. The 64 KiB raw wire payload defense remains at the Dart/Protocol/App boundaries.
- **Wiring:** Dart stores and transmits normalized text; Protocol and App validate the normalized candidate without rejecting trim-eligible raw whitespace first; the app-wire caller forwards the App-normalized value; Conversation authoritative admission uses one canonical text for the intent, principal-bound digest, user-message storage, and Engine prompt. Existing retry/Continue and explicit profile semantics are unchanged.
- **Checks:** The focused Rust, FFI app-wire, Flutter client, and diff checks recorded above pass. The boundary cases cover `\thello\t`, preserved `hello\\nworld`, rejection of internal tab/CR/other controls, empty input, and the 8192-byte exact/overflow cases.
- **Behavior:** `not_run`; no live macOS app, Keychain/Vault diagnosis, OAuth, provider, or model behavior was exercised.
- **Remaining Step 01:** This entry completes only the 01.2 text contract. It does not mark R003 Step 01 or product stability complete; the final ABI alias/legacy-composition removal and the remaining caller replacement work stay unfinished as recorded above and in later R003 steps.

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
