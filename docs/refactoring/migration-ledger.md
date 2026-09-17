# Floe migration ledger

This is the only mutable refactoring progress record. The [active document edition](README.md) selects the plan, detailed execution instructions and prompt. Published editions keep their own history; they are not status logs.

## Current checkpoint

| Field | State |
|---|---|
| Source reviewed for this documentation edition | `40d9397fe31c9dda42073e82dc04ef735a230806`; text-contract implementation checkpoint `64ab369` |
| Active plan | **R003**, [PLAN](versions/r003-structure-first/PLAN.md) / [execution](versions/r003-structure-first/EXECUTION_PLAN.md) |
| Current stage | **A — structural refactoring; not complete** |
| Execution | One coding agent, one sequential change set; product Manager/Expert A2A retained |
| Current change | Stage-A ownership placement, third pass: give the Expert its own execution contract, send the personal and remote source bodies to Context and Access, and take the scripted fixture run off App's wire |
| Runtime changes in this refresh | None intended. The Expert model contract, the registry admission and settlement, the transcript projection, the four personal reads and the remote view review are the same rules in a different crate. Two things are now stated structurally rather than checked at runtime, with the same outcome: an Expert's step type has no delegation variant (the binding refuses one, where the Expert used to), and an Expert invocation no longer carries the turn ledger (the model port does). App wire schema remains 2 |
| Contract validation in this refresh | `cargo check --workspace` clean; 231 unit tests pass across twenty crates, including `floe-provider-adapters` (40), `floe-context` (39), `floe-conversation` (26), `floe-execution` (24), `floe-knowledge` (21), `floe-access` (15); `floe-experts-builtin` passes 69 with its integration targets and `floe-protocol` 25 including `--test app_wire_v2`; `python3 tools/architecture/check_boundaries.py .` reports 18 errors, down from 26; every package's test targets build at least as well as before this refresh, package for package (floe-app 157 -> 152 errors, floe-ffi unchanged at 36, floe-experts-builtin still 0); `git diff --check` passes |
| Product validation in this refresh | `not_run` |
| Latest recorded app observation | Normal macOS debug app ended in VaultUnavailable before route selection; cause not confirmed; unchanged by this refresh |
| Next code task | R003 07: split `AgentVaultActionDto` so the conversation turn request, the remote route and the native acquisition requests stop being wire types, which is the whole of App's remaining reach into floe-protocol — first file `crates/bindings/protocol/src/dto/agent.rs`. Restoring the broken integration targets is not this stage's work and must not precede it |

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

### R003 stage-A ownership placement evidence

- **Completed scope:** The structure-first reshape moved files without moving ownership with them. This entry records the correction.
  - *Bodies returned to their owners.* The vault-host conversation turn, its engine ports, the agent fixture and their prompts were filed under floe-conversation and go back to floe-app; the Schedule Expert's calendar-history boundary and model-history bound go to floe-experts-builtin; the Expert setup operations, filed as `impl AgentRegistry` inside the builtin crate where they could not compile, go back to floe-experts; `ExpertSettlement` joins the Task owner.
  - *Attempt accounting.* The journaling `UsageLedger` moves to floe-inference beside `ModelAttemptRecord` and `AttemptJournal`. It had been declared in Conversation, so `A2ASendMessageRequest` could not carry it and had been silently rebound to the plain budget ledger, dropping delegated Experts' attempts. Conversation keeps its own capability journal and the Session usage projection.
  - *Capability executions.* `CapabilityExecution`, its state and `ProviderReplay` move to floe-agent-contract beside a `CapabilityJournal` port; the record-dispatch-record helper moves to floe-agent-runtime. A root Run and a delegated Expert now leave the same shape through the same path.
  - *Grants.* `ConversationExperts::source_granted` walked the setup receipt inside App and answered `true` when no setup existed. `SourceGrants`/`SourceGrant` answer from the registry instead, distinguishing a source no setup bound, a source whose connection is down, and a source this Expert holds no grant to.
  - *Eligibility.* `agent_cards` filtered on `matches!(model, Server(_)) || BuiltinExpertKind::runs_on_device_model(id)`. Cards now declare `supported_placements`, recorded by the registry from the Expert's own declaration, and `floe_experts::eligible_cards` filters on where the turn runs.
  - *Schedule.* `ScheduleEndpoint::execute` chose the active setup, built the day range, interpreted `/focus`, decided remote acquisition and assembled the policy. Those are `floe_experts_builtin::schedule::{select_active_setup, plan_run, run_policy, day_bounds}` now, with their own regressions; App reads the records and builds the readers the plan asks for. Each Expert also names its own result artifact.
  - *Boundaries.* floe-ffi's manifest is down to floe-app and floe-protocol, what the approved architecture allows it; `floe_app::modules` re-exports the module values a binding must name, and `FloeHandle` plus the request-boundary helpers move to floe-app. The provider adapters drop floe-experts and floe-experts-builtin; the confirmed-interaction view joins the other source views in floe-context.
- **Wiring/removal:** App registers the seven builtin Experts and the Schedule endpoint, supplies the setup declarations and packaging, and injects `SourceGrants`; floe-vault's registry methods take the specs and packaging rather than knowing the builtin crate; `ObservationFence` returns the subject fingerprint every caller already used; `ExpertActionStore::store_agent_action_envelope` reports the admission the implementation always returned.
- **Checks:** `cargo check --workspace` is clean for the first time since the reshape. 226 tests pass across every target that builds. The boundary checker goes 40 -> 34.
- **Behavior:** `not_run`; no live macOS app, Keychain/Vault diagnosis, OAuth, provider or model behavior was exercised.
- **Safety finding:** Restoring the builtin crate's tests surfaced a regression the reshape had introduced: replacing setup validation with the `SetupValidator` port left `AgentRegistry::restore` calling `NoSetupValidator`, so a restored snapshot validated neither its calendar nor its builtin setup receipts. The registry validates its own records again; the port remains for setup an owner outside the crate adds.
- **Behavior change to note:** with no recorded setup, an Expert's source grant is now `NotConfigured` rather than an implicit allow. A mandatory source that is `NotConfigured` or `Denied` refuses the Expert; `Unavailable` reports `CapabilityUnavailable` so the Person can act on it. Optional enrichment is skipped for all three.
- **Unfinished:** floe-app's and floe-vault's test targets, and seven integration targets in floe-connections, floe-day, floe-context, floe-actions, floe-experts, floe-conversation and floe-ffi, still carry the imports the reshape scattered. `AgentConversationTurnRequestDto` and `AgentRemoteRouteDto` are the in-process vault-worker transport rather than app wire, but they are reached through `AgentVaultActionDto`, the legacy vault operation envelope R003 07 replaces, so they cannot move without splitting that envelope. `ExpertHost` in `schedule/host.rs` is the Expert-neutral invocation loop and still sits in the builtin crate. floe-experts-builtin still depends on floe-context, floe-conversation, floe-inference, floe-knowledge, floe-experts and floe-kernel; closing those needs the view and model-path types to reach their contract owners, which is R003 06.1. Twelve of the 34 boundary errors are contracts crates the policy's allowed lists omit (`floe-kernel` for app, context, experts, builtin, providers; `floe-agent-contract` for access, connections, actions, day, vault); whether that is code to change or policy to correct is an open question for the plan owner.

### R003 stage-A ownership placement, second pass

Review of the first pass found three things done, three not, and one done in the
wrong direction. This entry records the correction and what it did and did not
close.

- **App and the ABI, corrected.** The first pass narrowed floe-ffi's manifest by
  moving the wire into the app — `ErrorDto`, the version check, the serde
  re-encode — and opened `floe_app::modules` over every business module and the
  native calendar adapter. The manifest got shorter; the reach did not. Parsing a
  wire field and naming a failure on the wire are floe-protocol's `wire` now;
  restating an owner's failure (`HostError`, `CoreError`, `AppOpenError`) is
  floe-ffi's, with `FloeHandle`. The calendar action body follows its judgment:
  `allow_create` is `CalendarActionPolicy::for_execution` in floe-actions,
  choosing the provider and the device Person binding is floe-app's
  `CalendarActionCommand`, and floe-protocol's codec gained the classification,
  record and batch conversions the binding was open-coding. `floe_app::modules`
  is gone; what replaces it is the values App's own signatures carry — a
  receipt, a run state, a failure, the panic barrier's diagnostics.
- **The immutable projection.** floe-context-contract takes the source views and
  their validators, the evidence they yield, the confirmed memories that reach a
  context, the optional-source rule, and `AuthorizedRead`, the lease-held read an
  Expert holds open and releases by dropping without naming the registry behind
  it. floe-agent-contract takes `AgentContext`, `InferencePolicyDecision`, the
  role-neutral prompt assembly and the turn-boundary history bound. floe-context
  and floe-knowledge re-export what moved, so callers read the same names; the
  Learner keeps its own role text. floe-experts-builtin drops floe-context,
  floe-knowledge, floe-kernel and floe-inference — the last it had never used.
- **Personal and remote source acquisition.** `active_read_grant`,
  `grant_unchanged`, `subject_unchanged`, `valid_subject_fingerprint` and
  `active_resource_grant` are floe-access's; the four personal reads had each
  spelled the first out, differing only on whether the source authority must
  match and whether a second live grant is a review. The query fingerprints that
  make a stored dependency re-checkable, and the remote view catalog — which
  views exist, which connector serves one, how its resource is named, what a
  grant covers, what a query and an answer must satisfy — are floe-context's.
  App composes the acquisition and injects what it built, and is ~240 lines
  lighter.
- **Schedule.** `plan_run` already decided the acquisition; it names the
  reasoning placement that follows from it (`ScheduleReasoning`) instead of the
  App host deriving it. `external_transfer_consent` is floe-inference's, which
  owns where a run executes and, separately, who may receive its input.
- **Checks:** `cargo check --workspace` clean; 231 unit tests across twenty
  crates; floe-experts-builtin 72 and floe-protocol 25 with their integration
  targets. Access gained five regressions over the moved grant rules, Context one
  over the attention lineage, floe-experts-builtin one pinning that a remotely
  acquired calendar is never reasoned over off-device. `git diff --check` passes.
- **Behavior:** `not_run`; no live macOS app, Keychain/Vault diagnosis, OAuth,
  provider or model behavior was exercised.
- **Boundary checker, 34 -> 26, split by kind.** Seventeen are real module or
  adapter coupling and are code to change: floe-app -> floe-protocol (the vault
  worker's own envelope, R003 07); floe-provider-adapters -> floe-protocol,
  floe-vault, floe-conversation, floe-knowledge, floe-day; floe-experts-builtin
  -> floe-conversation, floe-experts; floe-experts -> floe-day,
  floe-agent-runtime; floe-access -> floe-day, floe-context; floe-actions ->
  floe-experts; floe-conversation -> floe-day; floe-protocol -> floe-day;
  floe-connections -> floe-day; floe-vault -> floe-context. Nine are the
  contracts doorway question, unchanged and still the plan owner's: floe-app,
  floe-experts, floe-context and floe-provider-adapters name floe-kernel or a
  contract their approved list omits, and floe-vault names both contracts where
  its list routes it through the modules. Four crates that were in that count —
  Access, Day, Connections and Actions — are out of it because their
  floe-agent-contract dependency was spurious: they named only `AgentFailure`
  (floe-kernel's) and `DataClass` (floe-context-contract's), and both of those
  contracts were already theirs.
- **Unfinished.** floe-experts-builtin's boundary still names floe-conversation
  and floe-experts, and both are the same thing: `ModelRunner`/`ModelRequest` in
  `turn/session.rs` and `EngineRequest`/`ModelPort` in floe-agent-contract are
  two live agent execution contracts, and `schedule/host.rs` — the
  Expert-neutral invocation loop, still filed under builtin — needs the whole of
  the first. Consolidating them is R003 06.1 and is the next task. App still
  composes the native acquisition requests as `LocalContext*Dto`, and still
  reaches floe-protocol for `AgentVaultActionDto`; both are the legacy vault
  worker transport R003 07 replaces. floe-app's and floe-vault's test targets,
  and the integration targets in floe-connections, floe-day, floe-context,
  floe-actions, floe-experts and floe-ffi, still do not build; this refresh
  broke none of them further and fixed floe-ffi's from 39 errors to 36.

### R003 stage-A ownership placement, third pass

Review of the second pass found the direction corrected and three things
unfinished: the Expert execution contract, the source acquisition and approval
bodies still in App, and a public wire path the previous entry had not named.
This entry records all three.

- **The Expert execution contract.** What an Expert does is one bounded model
  call, so that is what the contract says: `ExpertModel` takes a prompt, a
  policy, a context and an assignment with its bounds, and returns one answer.
  An Expert that reasons over several steps, calling its own capabilities,
  states that as `ExpertReasoner` over its own transcript — not a Session, and
  with no delegation variant, because an Expert has none. Everything the Expert
  used to name and had no business deciding is behind those ports now: which
  provider answers, how a failed attempt is recovered, and which ledger the
  tokens are charged to. App implements them and binds each delegated message's
  own ledger to the model for the length of that message, so a delegated
  Expert's attempts are still accounted to the turn that asked.
- **The registry and the transcript.** `ExpertInvocation`, `ExpertResult` and
  the package identity they carry are contract values and move to
  floe-agent-contract. Admitting an invocation and settling it are the
  registry's: `ExpertAssignments` is the port, `RegistryAssignments` in
  floe-experts answers it, and a declarative package's own focus rule is read
  there rather than in the Expert. The calendar history rule splits the same
  way — which results carry a Person's calendar is the Schedule Expert's
  (`CalendarHistoryBoundary`), what a later turn may still be shown of that
  history is Conversation's. floe-experts-builtin's manifest is now exactly its
  approved allowed list: agent-contract, agent-runtime, context-contract,
  execution, actions, day.
- **Personal source reads.** The four read bodies move to floe-context. What
  was four near-identical flows is one — find the grant that admits the read,
  ask the device, check that the subject and the grant are the ones it started
  under, validate the view, record the provenance — with what actually differs
  stated as data on the read. Attention keeps its own path, because its
  observation is committed as it is admitted. The personal source catalog goes
  with them. App keeps the two adapters: the vault's grant records and the
  native driver, whose acquisition wire shape stays with the driver.
- **Remote view approval.** Comparing the producer against the one the Person
  pinned, checking the signed source describes it, checking authority, revision,
  provider identity and recipient against the review, building the scope with
  its approved recipient, and deciding what a review does to an existing grant
  are floe-access's now, with the producer identity, the signed source reference
  and the feasibility query. floe-vault re-exports all three.
- **The fixture run.** `floe_app::agent_run::run` took a DTO, checked the
  protocol version and built a wire result, and the ABI called straight into it.
  App states the run in its own terms now; reading the request and re-encoding
  the result are the binding's.
- **Checks:** `cargo check --workspace` clean; 231 unit tests across twenty
  crates; floe-experts-builtin 69 and floe-protocol 25 with their integration
  targets. New regressions: an Expert refuses an answer that overspends its
  budget or overflows its output bound (floe-experts-builtin), the five
  transcript-projection cases that moved with the projection
  (floe-conversation), and five over the moved grant rules (floe-access).
- **Behavior:** `not_run`; no live macOS app, Keychain/Vault diagnosis, OAuth,
  provider or model behavior was exercised.
- **Boundary checker, 26 -> 18.** Eleven are real module or adapter coupling and
  are code to change: floe-app -> floe-protocol and floe-provider-adapters ->
  floe-protocol (both the legacy vault-worker and route envelopes, R003 07);
  floe-provider-adapters -> floe-vault, floe-conversation, floe-knowledge,
  floe-day; floe-vault -> floe-context (the coverage projection it drives, which
  is a Context use case with the vault as its repository); floe-experts ->
  floe-day and floe-agent-runtime; floe-actions -> floe-experts; floe-access,
  floe-connections and floe-conversation -> floe-day; floe-protocol ->
  floe-day. Seven are the contracts doorway, and the recommendation is now that
  the policy is what is wrong: floe-app and floe-vault both legitimately speak
  floe-kernel, floe-agent-contract and floe-context-contract — the app composes
  owners that trade in those values, and the vault stores agent sessions, tasks,
  settlements and dependencies — and routing either through a module re-export
  would hide the dependency rather than remove it. Where a crate really was
  reaching past its approved contract, it no longer does: Access, Day,
  Connections and Actions dropped floe-agent-contract, Context, Experts and the
  provider adapters now reach floe-kernel through the contract their list
  allows, and floe-vault reads the grant vocabulary through floe-access. Four
  dependencies that were declared and never used are gone.
- **Unfinished.** App still reaches floe-protocol for the in-process vault
  worker envelope (`AgentVaultActionDto`) and the native acquisition requests
  (`LocalContext*Dto`); both are R003 07 and the native driver owner's, and they
  are the last of App's wire. `GovernedDependencyResolver` — whether a stored
  dependency still holds — is still implemented in App against a vault port
  rather than owned by Context. floe-app's and floe-vault's test targets, and
  the integration targets in floe-connections, floe-day, floe-context,
  floe-actions, floe-experts and floe-ffi, still do not build; this refresh
  broke none of them further and floe-app's is five errors better.

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
