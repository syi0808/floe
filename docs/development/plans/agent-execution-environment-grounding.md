# Agent execution environment and grounded Manager convergence

- Status: in progress — Checkpoint 01 complete
- Baseline: main at 04386367221c66587a1f5001c13d89a0cbbda0d0
- Classification: architectural change
- Primary owners: App composition/lifecycle, Experts, Conversation, Context, Agent Runtime, Inference/provider adapters
- Safety owners preserved: Access, Connections, Context provenance, Actions, Vault
- Execution rule: complete checkpoints in order. Do not start a later checkpoint while an earlier checkpoint has an unresolved acceptance failure.
- Compatibility rule: Floe is pre-stable. Replace obsolete internal contracts directly; do not add old/new compatibility branches, optional migration fields, or legacy decoders solely to keep the old design alive.

## 0. Why this plan exists

A fresh Floe profile can currently enter Conversation through Resume, create a Session when none exists, and still have no Expert Registry. Expert installation/default setup is attached to ConversationSessionOperation::Start, while the product normally enters through Resume. Conversation turns then call ensure_expert_bundle with ExistingOnly, which deliberately does nothing when no Registry exists. The resulting root Manager can therefore run with an empty Expert catalog even though the product did not intentionally configure an empty catalog.

The observed failure also exposed a second independent weakness. The Manager prompt says not to guess private/current external state, but a direct answer remains structurally valid even when no observation supports its factual claims. Provenance validation can reauthorize dependencies the model actually used; it cannot prove that an unsupported claim should have had a dependency in the first place.

This plan fixes both root causes. It does not optimize for the smallest diff. The target is one coherent lifecycle in which:

1. Conversation Session lifetime is not the owner of Agent execution-environment setup.
2. the root Agent environment is prepared before Conversation becomes runnable;
3. every Run uses one immutable Expert execution snapshot for discovery and delegation;
4. configuration changes apply to the next Run, while live authority revocation still affects the current Run;
5. model-call context is assembled by explicit lifetime/trust sections rather than treating discovery metadata as instructions;
6. prompt-cache behavior follows those lifetimes and is measurable, but never decides freshness or authority;
7. the Manager may state private/current/changing external facts only when admissible support exists, otherwise it must acquire evidence or state a limitation.

## 1. Final architecture that all checkpoints must converge to

The completed runtime is:

    Vault create/unlock
      -> prepare root Agent environment
           -> reconcile shipped Expert bundle/Registry
           -> apply first-install-only default source binding
           -> publish the current Expert Directory
      -> Conversation Session Start/Resume/Get

    User turn
      -> build one RunExpertEnvironment snapshot
           -> exact Directory revision
           -> exact AgentDefinition
           -> exact ExpertAdmissionIdentity
           -> exact ExpertExecutionSelection
           -> pinned runtime endpoint
           -> deterministic environment digest
      -> admit Conversation Run with the environment identity
      -> build Manager stable prompt identity for the Run
      -> model attempt 1
           -> reauthorize retained evidence/history
           -> assemble one attempt ContextEnvelope from the pinned environment
      -> optional delegation through that same pinned RunExpertEnvironment
      -> optional later model attempt
           -> same RunExpertEnvironment
           -> newly assembled attempt evidence/context
      -> terminal answer release with normal coverage reauthorization

    Live authority
      -> GrantAuthority / SourceAuthority / current Connection state
      -> recipient consent / provider identity / OS permission
      -> checked at their existing admission, handoff, post-I/O and release fences
      -> never replaced by the Run configuration snapshot

The lifetime rule is:

| State | Lifetime | Owner |
| --- | --- | --- |
| Session/history | multiple Runs | Conversation |
| shipped package/Registry readiness | Vault-open/root Agent environment | Experts + App composition |
| active Expert definitions and source-selection configuration | one Run | Experts |
| stable Manager program/persona | one Run unless the role explicitly changes | Conversation/Knowledge prompt assembly |
| current evidence and retained-history authorization | one model attempt | Context |
| correction/runtime output bound | one model attempt | Agent Runtime/Conversation |
| grants, source authority, recipient consent and provider/OS authority | live at every protected operation | Access/Connections/provider owner |

An uninitialized Expert environment and a Ready environment with zero active Experts are distinct states. After this cutover, an uninitialized environment cannot reach Conversation execution. An intentionally empty active catalog remains valid.

## 2. Non-negotiable invariants

These apply to every checkpoint.

- Configuration is not authority. Pinning an Expert selection never pins a grant, SourceAuthority, recipient consent, provider credential, OS permission, connection liveness, or action authority.
- The root Manager receives no direct source-backed domain Tool fallback. Fresh domain observation still comes through a selected Expert.
- A mid-Run Registry/configuration edit never silently reroutes the current Run to a new Expert definition or source selection.
- A live source/grant/recipient revocation can still deny the current Run immediately at the existing authority fence.
- A crash/reopen must never resume a Run under a different environment while presenting it as the original one. If the exact environment required for same-Run recovery cannot be reconstructed, fail closed rather than silently using current configuration.
- A completed Task remains historical evidence. No new source read is inferred from its text.
- Durable pre-dispatch intent, CAS, exact-recipient consent, key identity, provenance, cancellation direction, and uncertain external-write recovery must not be weakened.
- Do not retain both live Directory resolution and snapshot resolution as two valid root delegation paths after Checkpoint 03.
- Do not retain both ScopedInstructions and the replacement context-section contract after Checkpoint 04.
- Do not tune the Manager-eval rubric to make a model failure pass.
- No production prompt example may name a concrete built-in Expert, package ID, Calendar, Schedule, or any fixed roster. Expert extensibility is a hard constraint.

## 3. Research-derived design constraints

Use these references as architectural evidence, not dependencies.

- Hermes: keep the session-stable prompt byte-stable while injecting volatile/turn context separately; rebuilding context does not require rebuilding the stable prompt.
  - https://github.com/NousResearch/hermes-agent/blob/main/agent/system_prompt.py
  - https://github.com/NousResearch/hermes-agent/blob/main/agent/turn_context.py
- OpenClaw: use snapshot semantics with explicit next-turn refresh triggers and deterministic stable-prefix ordering.
  - https://github.com/openclaw/openclaw/blob/main/docs/tools/skills.md
  - https://github.com/openclaw/openclaw/blob/main/docs/reference/prompt-caching.md
- Codex: bind capability/skill availability to a turn snapshot rather than a whole-session frozen world; invalidate/rebuild at explicit boundaries.
  - https://github.com/openai/codex/blob/main/codex-rs/core/src/session/turn_context.rs
- Prompting guidance: prefer positive eligibility/action rules plus an explicit abstention/limitation path over repeated prohibitions; examples must be generic and representative.
  - https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices
  - https://platform.claude.com/docs/en/test-and-evaluate/strengthen-guardrails/reduce-hallucinations
  - https://developers.openai.com/api/docs/guides/prompting
  - https://developers.openai.com/api/docs/guides/prompt-caching

Do not copy another Agent's file layout. Preserve Floe ownership and authority boundaries.

# Checkpoint 01 — Architecture contract convergence

## Goal

Record the new durable lifecycle before implementation so later checkpoints replace toward one target rather than inventing semantics during code changes.

## Required documentation changes

1. Add docs/decisions/0033-run-scoped-agent-environment-and-grounded-manager.md.
2. Add ADR 0033 to docs/decisions/README.md.
3. Amend docs/decisions/0017-agent-context-assembly.md.
4. Amend docs/decisions/0018-manager-expert-a2a-delegation.md.
5. Update docs/architecture/runtime.md.
6. Update docs/architecture/authority-recovery.md only for configuration-vs-authority semantics that this plan deliberately changes.

## ADR 0033 must state these decisions verbatim in meaning

- Session lifecycle and root Agent-environment lifecycle are separate.
- Root Agent environment preparation occurs before Conversation is runnable after Vault create/unlock.
- Run admission freezes one Expert configuration snapshot. Model discovery and actual delegation derive from the same snapshot.
- A Registry/configuration mutation applies to the next Run. It does not mutate or reroute a currently executing Run.
- Grants, SourceAuthority, Connection/provider state, exact-recipient consent, and OS permission are live authority and continue to be revalidated during the current Run.
- Context projection is attempt-scoped: evidence/history can be reauthorized and rebuilt between model attempts while the Run configuration stays fixed.
- Expert cards/capability descriptors are discovery data, not higher-precedence instruction text.
- Prompt caching is an optimization over deterministic serialization. Cache identity is never authority, freshness, or execution permission.
- A Manager factual claim about private, current, or changing external state requires admissible support. When support is required but absent, the Manager must delegate to a suitable advertised Expert when permitted, otherwise return a limitation answer.
- General model knowledge, plausibility, prior assistant statements, Expert descriptions, and failed/unavailable reads are not observations of current private state.

## Existing text that must be superseded

- docs/decisions/0017-agent-context-assembly.md currently says to freeze policy/grant/registry revisions together and describes Context as frozen for one model call. Rewrite this to distinguish Run configuration from attempt evidence and live authority.
- docs/architecture/authority-recovery.md currently says current Expert binding drift blocks later Task execution. Replace that rule for Tasks belonging to a live pinned Run. Do not change live source/grant or action authority rules implicitly; list every intentional downstream change in Checkpoint 03.
- docs/architecture/runtime.md currently describes the root Manager catalog and Task admission using current Directory/Registry state. Update it to state that current state is sampled once for a Run and the resulting immutable selection drives both discovery and dispatch.

## Acceptance

- No code changes in this checkpoint.
- ADR 0033 contains the complete final-state contract above.
- ADR 0017/0018 amendments point to ADR 0033 rather than duplicating progress status.
- Current architecture docs describe the target semantics only after the implementation checkpoint that makes them true. If executing checkpoints as separate commits, ADR 0033 may land first, but runtime.md/authority-recovery.md final-state wording must land with or after the matching code cutover rather than temporarily claiming unimplemented behavior.

## Checkpoint commit

Use a dedicated architecture-decision commit. Record its SHA in this plan before starting Checkpoint 02.

# Checkpoint 02 — Root Agent-environment lifecycle cutover

## Planning refresh and status

- Status: not started — implementation has not begun.
- Planning refresh baseline: `main` at `4dd5ec2c2de03b834218b0eaf8429b1e6dc4f929`.
- Checkpoint 01 is the only completed checkpoint; the latest `main` contains no production change after the original plan baseline.
- The original Checkpoint 02 direction remains valid, but this section is deliberately refined to code-line level so implementation does not have to reopen lifecycle or cleanup decisions.

The latest source review freezes four details that were only implicit in the original plan:

1. `ExpertInstallRefresh` and `ExpertRefreshOutcome` exist only to support the current Start/turn refresh split. No independent owner uses them. Checkpoint 02 deletes those transition concepts instead of moving them to Vault activation.
2. Product Vault Create/Unlock already arrives through `AppComposition::vault_command -> Worker::local_request` with a verified `CallerContext`. Root-environment preparation must consume that admitted device identity from `Job::local_admission`; do not add a device field to `WorkerAction`, `VaultLifecycleCommand`, AppWire, or FFI.
3. `bind_initial_defaults` currently republishes the Directory itself. Root activation must own the exact `install -> first-install binding -> publish` ordering, so the initial-binding helper must stop publishing as a side effect. Explicit user binding changes keep their own publication.
4. `publish_expert_directory` currently maps an absent Registry to an empty Directory. That violates ADR 0033's distinction between uninitialized and intentionally empty. After this checkpoint, absent Registry at publication is a fail-closed invariant violation; only an existing Registry with zero enabled assignments may publish an empty Directory.

## Goal

Make Expert Registry/package readiness a Vault-open/root-Agent invariant instead of a Conversation Start side effect or turn-time repair.

The completed Checkpoint 02 path is exactly:

```text
verified local Vault Create/Unlock
  -> Worker::local_request admission
  -> execute_action(Create|Unlock)
       -> require matching Job::local_admission
       -> derive verified device_id + operation_id + cancellation
       -> EncryptedAgentVault::create/open
       -> OpenVault::activate
            -> construct repositories / TaskCoordinator / Directory
            -> prepare_root_agent_environment
                 -> sample whether Registry is genuinely absent
                 -> ensure shipped Expert bundle
                 -> if genuinely absent: bind_initial_defaults once
                 -> publish Expert Directory once
            -> return usable OpenVault
       -> install OpenVault into current
  -> VaultState::Ready

Conversation Start / Resume / Get / Recover
  -> Session semantics only

Conversation Turn / Conversation Resume
  -> consume already-prepared Directory
  -> never install, refresh, repair, or republish the root Expert environment
```

No externally observable `EnvironmentReady` state is added. `VaultState::Ready` means activation, including root Agent-environment preparation, succeeded.

## Current code anchors at the planning refresh baseline

### App lifecycle / root composition

- `crates/app/src/vault_host.rs:227` — `OpenVault<Keys>`.
- `crates/app/src/vault_host.rs:281` — `OpenVault::activate`.
- `crates/app/src/vault_host.rs:317` — `OpenVault::publish_expert_directory`.
- `crates/app/src/vault_host.rs:2308` — `execute_action`.
- `crates/app/src/vault_host.rs:2326` — `WorkerAction::Create`.
- `crates/app/src/vault_host.rs:2346` — `WorkerAction::Unlock`.
- `crates/app/src/vault_services.rs:46` — production `VaultLifecycleCommands for AppComposition`, which already calls `Worker::local_request`.
- `crates/app/src/local_operations.rs:47` — `LocalOperationIntent::action`, mapping admitted Vault commands to private `WorkerAction::{Create,Unlock,Lock}`.

### Obsolete Conversation-owned readiness paths

- `crates/app/src/vault_host.rs:2113` — `execute_conversation_turn_action`: `ExistingOnly` refresh + Directory republish before every turn.
- `crates/app/src/vault_host.rs:2206` — `execute_conversation_resume_action`: the same refresh + republish before continuation.
- `crates/app/src/vault_host.rs:2601` — `WorkerAction::ConversationSession`.
- `crates/app/src/vault_host.rs:2604` — `ConversationSessionOperation::Start`: current first-install detection, `InstallIfAbsent`, and initial binding.
- `crates/app/src/vault_host.rs:3058` — App-private `ensure_expert_bundle` wrapper.

### Experts install contract

- `crates/modules/experts/src/bundle_install.rs:9` — `ExpertInstallRefresh`.
- `crates/modules/experts/src/bundle_install.rs:23` — `ensure_expert_bundle`.
- `crates/modules/experts/src/bundle_install.rs:48` — `ExpertRefreshOutcome`.
- `crates/modules/experts/src/bundle_install.rs:54` — `expert_refresh_outcome`.
- `crates/modules/experts/src/lib.rs:23` — exports for the refresh/outcome transition API.
- Repository search at this baseline finds `ExpertInstallRefresh` only in the Experts module, its export, the two turn/resume `ExistingOnly` call sites, and the Start `InstallIfAbsent` call site. There is no independent runtime owner that justifies retaining the mode enum after the lifecycle cutover.

### First-install binding

- `crates/app/src/vault_host/expert_binding_settings.rs:272` — `bind_initial_defaults`.
- `crates/app/src/vault_host/expert_binding_settings.rs:385` — explicit `replace`; its post-mutation Directory publication remains valid.
- `bind_initial_defaults` currently ends by calling `open.publish_expert_directory`; that publication moves to root activation.
- The deterministic per-assignment operation identity derived with UUID v5 from `setup_operation_id` remains unchanged.

### Tests that encode the old lifecycle

- `crates/app/src/vault_host.rs:3801+` — `registry_jobs_are_read_only_until_explicit_change_and_reconcile_duplicate_submits` currently expects Create to leave Registry absent and Start to install it; this expectation must be inverted.
- `crates/app/src/vault_host/tests/conversation_flows.rs:824+` — `production_general_turn_does_not_require_or_install_builtin_setup` explicitly asserts that Unlock/Resume/Turn leave the shipped install absent; this test becomes invalid and must be replaced, not preserved.
- `crates/app/src/vault_host/tests/conversation_flows.rs:1355+` — `production_builtin_setup_installs_through_vault_without_sources` is a useful install-count/product-path regression but its setup must no longer rely on Session Start.
- `crates/app/src/vault_host/tests/registered_runner.rs:217+` and `:260+` — `installed_open` / `installed_open_without_provider` directly call `OpenVault::activate` and then manually install a manifest. These fixtures must be adapted to the new activation contract rather than bypassing or duplicating root preparation.
- `crates/app/src/vault_host/tests/registered_runner.rs:2175+` — `initial_shipped_setup_selects_single_native_source_only_once` currently calls `bind_initial_defaults` manually; preserve its idempotency assertion while moving the product invocation to activation.
- Many Vault-host tests call private `Worker::request(... WorkerAction::Create/Unlock ...)`. Production does not: Vault lifecycle uses the admitted local owner service. Test lifecycle setup must migrate to an admitted local helper rather than causing production code to accept an unverified fallback device.

## 02-A — Carry verified lifecycle admission into OpenVault activation

### `crates/app/src/vault_host.rs`

1. Adjacent to `OpenVault<Keys>`, introduce an App-private value named `RootAgentEnvironmentAdmission` with exactly the state activation needs:
   - verified `device_id: String`;
   - trusted `operation_id: Uuid`;
   - `Cancellation`.
   It is not public, not serialized, not persisted, and not exposed through FFI.

2. Change `OpenVault::activate` to accept one `RootAgentEnvironmentAdmission` in addition to its current runtime dependencies and registrations.

3. In `execute_action`'s `WorkerAction::Create` and `WorkerAction::Unlock` arms:
   - require `job.local_admission.as_ref()`; missing admission fails closed with `AgentFailure::PolicyDenied`;
   - require that the admitted intent is the exact matching `LocalOperationIntent::VaultCommand(VaultLifecycleCommand::Create|Unlock)`; do not infer trust merely from the private `WorkerAction` variant;
   - copy `admission.caller.device_id()` into `RootAgentEnvironmentAdmission.device_id`;
   - use `job.id` as the trusted setup operation ID;
   - pass `job.cancellation.clone()`;
   - only assign `*current = Some(...)` after `OpenVault::activate` has completed root-environment preparation.

4. Do not change `VaultLifecycleCommand`, `LocalOperationIntent`, `WorkerAction`, AppWire DTOs, FFI schemas, or Flutter commands to carry device/setup data. The verified `CallerContext` already owns device identity.

5. Do not add a fallback such as `"mac-local"` in production. Tests that previously injected `WorkerAction::Create/Unlock` directly must use an admitted local caller.

### Acceptance for 02-A

- Product Create/Unlock still enter through `AppComposition::vault_command -> Worker::local_request`.
- A test-only/unadmitted direct Create/Unlock job cannot become the source of first-install binding identity.
- No new public or wire surface exists.

## 02-B — Make root Agent-environment preparation a single OpenVault operation

### `crates/app/src/vault_host.rs`

1. Build `OpenVault` in `OpenVault::activate` exactly as today through:
   - registration validation;
   - Vault Arc;
   - Conversation executor activation/recovery;
   - Conversation repository;
   - Directory;
   - Task repository / `TaskCoordinator::activate`;
   - recovered Task/Run state.

2. Before returning that `OpenVault`, call one private method:
   `OpenVault::prepare_root_agent_environment(&RootAgentEnvironmentAdmission)`.

3. `prepare_root_agent_environment` owns this exact sequence:
   - `let first_install = self.vault.expert_registry().await?.is_none();`
   - `ensure_expert_bundle(self, admission.cancellation.clone()).await?;`
   - if and only if `first_install`, call `expert_binding_settings::bind_initial_defaults(self, self.vault.person_id(), &admission.device_id, admission.operation_id, &admission.cancellation).await?;`
   - call `self.publish_expert_directory(&self.registrations).await?;`
   - return success only after all required stages succeed.

4. `first_install` is defined by Registry absence **before** shipped-bundle reconciliation. It is not defined by a missing install receipt or manifest-digest mismatch. Therefore an existing Registry that needs shipped-bundle reconciliation must not receive first-install default bindings.

5. Root preparation is fail-closed. There is no degraded-ready outcome in this path:
   - cancellation, Vault/storage failure, install conflict that cannot reconcile, binding failure, Registry restore failure, and Directory publication failure all prevent `OpenVault::activate` from returning;
   - `execute_action` therefore never places a partially prepared OpenVault into `current`.

6. Do not hold a global Vault transaction while source/provider I/O may occur in `bind_initial_defaults`; preserve its current bounded per-operation access behavior.

### `OpenVault::publish_expert_directory`

Replace the current branch:

```rust
let Some(snapshot) = self.vault.expert_registry().await? else {
    self.directory.publish("product.experts", Vec::new())?;
    return Ok(());
};
```

with a fail-closed Registry requirement. Reuse the existing `AgentFailure::NotFound` meaning already used by `bind_initial_defaults` for absent Registry. The valid empty-catalog case is an existing Registry whose `enabled_expert_admissions` result is empty; that path still publishes `Vec::new()`.

Do not add a second readiness flag. Successful activation plus an existing Registry is the readiness invariant.

## 02-C — Delete the install-refresh transition API

### `crates/modules/experts/src/bundle_install.rs`

1. Delete `ExpertInstallRefresh` entirely.
2. Change `ensure_expert_bundle(store, when)` to `ensure_expert_bundle(store)`.
3. Preserve the existing install/reconcile algorithm:
   - read `overview()`;
   - if present, validate its manifest digest;
   - if absent, install with the store's instance ID, current registry revision, and deterministic operation ID;
   - on CAS conflict, re-read the overview and require it to exist;
   - require the resulting receipt digest to equal the current manifest digest.
4. There is no longer an `ExistingOnly` early return. Calling `ensure_expert_bundle` semantically means “make this bundle ready”.
5. Delete `ExpertRefreshOutcome` and `expert_refresh_outcome`. Their only purpose is to downgrade the obsolete per-turn refresh. Root readiness propagates the typed failure directly.

### `crates/modules/experts/src/lib.rs`

Remove the deleted `ExpertInstallRefresh`, `ExpertRefreshOutcome`, and `expert_refresh_outcome` exports. Keep `ExpertInstallStore`, `ensure_expert_bundle`, and any still-used `BoxFuture` export.

### `crates/app/src/vault_host.rs:3058+`

Simplify the App-private wrapper to:

```text
ensure_expert_bundle(vault, cancellation)
  -> floe_experts::ensure_expert_bundle(&VaultExpertBundle { ... })
```

with no refresh/mode argument and no degraded-outcome translation.

This is a direct replacement. Do not leave a deprecated enum, compatibility overload, or “ExistingOnly” alias.

## 02-D — Make first-install binding a mutation step, not a publication owner

### `crates/app/src/vault_host/expert_binding_settings.rs:272+`

1. Keep `bind_initial_defaults`'s current candidate selection rules, bounds, source checks, deterministic UUID-v5 operation IDs, CAS behavior, and “already selected => no-op” behavior.
2. Remove only its final `open.publish_expert_directory(&open.registrations).await` side effect; return `Ok(())` after binding mutations complete.
3. Root activation publishes once after all first-install bindings settle.
4. Keep `replace`'s Directory publication after explicit user binding mutation. Explicit Settings changes still need the current Directory to converge immediately for the next Run.
5. Do not call `bind_initial_defaults` when Registry already existed, even if:
   - a new connection/source appeared;
   - a shipped package was reconciled;
   - an assignment is disabled;
   - an explicit binding is empty;
   - a previous binding points to a currently unavailable source.

The result is first-install convenience only, never startup auto-rebinding.

## 02-E — Remove Expert-environment setup from Conversation

### `crates/app/src/vault_host.rs:2113+` — `execute_conversation_turn_action`

Delete all of the following before `conversation_turn::run`:

- `ensure_expert_bundle(... ExistingOnly)`;
- `expert_refresh_outcome` classification and degraded warning;
- `vault.publish_expert_directory(...)`.

The function begins the Conversation Run using the already-prepared `vault.task_coordinator` and Directory. If activation could not prepare them, there must be no current OpenVault to reach this function.

### `crates/app/src/vault_host.rs:2206+` — `execute_conversation_resume_action`

Delete the equivalent refresh/outcome/publication block. Continuation does not refresh root configuration.

### `crates/app/src/vault_host.rs:2601+` — `WorkerAction::ConversationSession`

For `ConversationSessionOperation::Start`, delete:

- `first_install` Registry probe;
- `ensure_expert_bundle(... InstallIfAbsent)`;
- `job.local_admission`-based initial binding;
- every Expert-environment side effect.

After the cutover:
- Start only calls `start_session -> admitted_session`;
- Resume only calls `resume_session -> admitted_session`;
- Get stays read-only;
- Recover stays Conversation recovery.

Do not move the old code into Resume or a shared Session helper.

## 02-F — Test-fixture migration and regressions

### Canonical Vault lifecycle fixture

Current test helper `perform(worker, person, WorkerAction)` submits non-remote actions through the cfg(test) raw worker path. Do not make production activation accept a missing admission merely to keep that helper unchanged.

Add/reuse a test helper that:
- constructs a verified `CallerContext` for the Person and chosen device;
- submits `LocalOperationIntent::VaultCommand(Create|Unlock|Lock)` with `LocalOperationOwner::Vault`;
- polls/releases through `Worker::local_request`.

Migrate Create/Unlock setup in affected App tests to that helper. Tests specifically about raw worker scheduling may continue using `Worker::request` for actions that do not require Vault lifecycle identity.

### Required lifecycle regressions

1. **Create prepares before Session**
   - issue canonical Vault Create;
   - before any Conversation Session command, read Registry through the Experts owner and assert it exists;
   - assert shipped install receipt/digest exists;
   - assert Directory/catalog reflects enabled Registry entries that have matching runtime registrations.

2. **Fresh normal Resume**
   - create a fresh Vault through the canonical owner;
   - do **not** call Conversation Start;
   - issue normal Conversation Resume;
   - assert Resume creates/restores the root Session;
   - prove Registry/install existed before Resume and Registry revision is unchanged by Resume.

3. **Unlock upgrades an absent environment**
   - create an encrypted Vault directly as a storage fixture without installing Experts, close it, then unlock through the canonical App Vault owner;
   - assert activation installs Registry/package and publishes Directory before any Session command;
   - this is the direct regression for profiles produced by the old lifecycle.

4. **Existing Registry is not silently rebound**
   - prepare a Registry with an explicit binding;
   - record Registry revision, assignment enabled state, and binding revision/selection;
   - lock/unlock;
   - assert activation preserves those values except for a separately justified shipped-bundle reconciliation;
   - add a source/connection before unlock and prove it does not trigger automatic rebinding.

5. **Disabled-all is Ready, not uninitialized**
   - explicitly disable every assignment in an existing Registry;
   - lock/unlock;
   - activation succeeds;
   - Registry still exists;
   - Directory/catalog is intentionally empty;
   - no assignment is re-enabled.

6. **First-install binding criterion is Registry absence**
   - absent Registry + eligible unambiguous native source => initial default selection may occur once;
   - second activation does not advance the binding revision;
   - existing Registry with a missing/outdated shipped install receipt may reconcile the bundle but must not rerun first-install binding.

7. **Idempotent reopen**
   - lock/unlock a prepared profile repeatedly;
   - unchanged shipped bundle does not advance install receipt, Registry revision, assignment enabled state, or binding revision;
   - Directory publication may advance its internal publication revision only if that is existing Directory semantics, but the effective catalog must be identical.

8. **Preparation failure never exposes Ready**
   - cancellation or injected Vault/storage failure during bundle ensure/binding/publication returns a failure from Create/Unlock;
   - `current` remains `None`;
   - a following Conversation Session/Turn request observes `VaultUnavailable`, not a runnable Vault with absent Registry.

### Existing tests to replace/update

- Rewrite `registry_jobs_are_read_only_until_explicit_change_and_reconcile_duplicate_submits` so Create already has Registry. Preserve the separate assertion that Registry **inspection** is read-only and explicit configuration changes reconcile duplicate submissions.
- Replace `production_general_turn_does_not_require_or_install_builtin_setup`; its asserted behavior is the defect. New test meaning: unlock/root activation prepares Experts independently of whether the next Conversation is general knowledge.
- Keep `production_builtin_setup_installs_through_vault_without_sources`, but assert installation is a Vault activation effect and Session Start is unnecessary.
- Refactor `installed_open` / `installed_open_without_provider` so they do not manually repeat the same bundle installation that activation now owns. Where a test needs a synthetic/custom manifest, install that synthetic package deliberately **after** canonical root activation using the current Registry revision, then publish for the test; do not weaken production root preparation.
- Keep `initial_shipped_setup_selects_single_native_source_only_once` as a helper-level idempotency test, and add product-level activation coverage so its only production caller is root preparation.
- Update any tests that expected Start/Turn to change Registry or Directory revision. Start/Resume/Turn must be neutral with respect to root-environment readiness after this checkpoint.

## Documentation convergence in this checkpoint

Update `docs/architecture/runtime.md` in the same implementation commit(s) once code matches:

- Vault Create/Unlock prepares the root Expert Registry/package and publishes Directory before `VaultState::Ready`;
- first-install default binding belongs to root activation, not Conversation;
- Conversation Start/Resume and turn/resume execution consume an already-ready environment and do not repair it;
- absent Registry is not equivalent to an empty enabled catalog.

Do **not** document Checkpoint 03's future Run-pinned discovery/dispatch semantics yet. At the end of Checkpoint 02 the root lifecycle is corrected, but root turns still use the current TaskCoordinator/Directory semantics until Checkpoint 03.

`docs/architecture/authority-recovery.md` does not need its Checkpoint 03/04 configuration-vs-authority rewrite in Checkpoint 02 unless implementation reveals a directly changed recovery fact. Do not prestate future pinning behavior.

## Residual/deletion gate

Run repository searches and classify every remaining match.

Required zero-match production concepts after Checkpoint 02:

```text
ExpertInstallRefresh
ExpertRefreshOutcome
expert_refresh_outcome
ExistingOnly
InstallIfAbsent
```

Required ownership for remaining symbols:

- `ensure_expert_bundle` — Experts implementation/export plus exactly the App root-environment preparation path; no Conversation Session/turn/resume caller.
- `bind_initial_defaults` — definition, root-environment preparation caller, and focused tests only; no Session caller.
- `publish_expert_directory` — root preparation and explicit Registry/binding mutation paths only; no per-turn/per-resume refresh.
- `ConversationSessionOperation::Start` — Session semantics and tests only; no Expert install/binding logic.

Also search conceptually for:
- “general turn without expert setup” assumptions;
- tests asserting Registry is absent after successful Create/Unlock;
- startup/unlock code that silently changes existing binding or enabled state;
- any path that maps absent Registry to a Ready empty Directory.

Do not create a permanent grep/checker for this migration.

## Verification required before marking Checkpoint 02 complete

Fast iteration:

```sh
cargo test -p floe-experts --tests
cargo test -p floe-app --lib
```

Checkpoint Rust/application gate:

```sh
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
cargo build -p floe-ffi
git diff --check
```

No Flutter, Go, native-provider, or external-account mutation is required solely by this checkpoint because it changes App-private lifecycle ownership without changing the wire/FFI contract. If implementation unexpectedly changes those surfaces, expand verification according to `.agents/skills/code-change-verification/SKILL.md`.

Use isolated temporary Vault roots for lifecycle regressions. Do not reset or migrate real user data. If a required gate fails for a pre-existing reason, reproduce it at the checkpoint start revision when practical and record the evidence; do not weaken assertions.

## Checkpoint 02 acceptance

Checkpoint 02 is complete only when all are true:

- a successful canonical Create or Unlock cannot return `VaultState::Ready` without an existing Expert Registry and published Directory;
- a Ready Registry with zero enabled Experts remains valid and publishes an empty catalog;
- verified device/setup identity comes from admitted Vault lifecycle context, not product payload or a production fallback;
- first-install default binding runs only when Registry was absent before reconciliation;
- existing binding/enable configuration survives reopen unchanged;
- Conversation Start/Resume/Turn/Resume-turn contain no install, refresh, initial-binding, or readiness-repair behavior;
- the refresh-mode/outcome transition API is deleted;
- absent Registry is never converted to a Ready empty Directory;
- tests cover fresh Resume without Start, legacy absent-Registry unlock, disabled-all, no-auto-rebind, idempotent reopen, and preparation failure;
- `docs/architecture/runtime.md` matches the implemented lifecycle and does not claim Checkpoint 03 behavior;
- residual searches satisfy the gate above;
- the plan execution report records start HEAD, fetched origin/main, checkpoint commit SHA(s), tests, residuals, documentation, worktree state, and explicitly states Checkpoint 03 was not started.

## Checkpoint commit discipline

Implement Checkpoint 02 as one logical lifecycle-cutover commit when practical. Multiple local commits are acceptable only to keep the replacement buildable; if used, the execution report must list every SHA. Update this plan's Checkpoint 02 execution report after verification and stop. Do not start Checkpoint 03 in the same implementation pass.


# Checkpoint 03 — Run-pinned Expert discovery and delegation

## Goal

Ensure the Manager-visible Expert catalog and actual executable Expert selection are two projections of one immutable Run snapshot.

## Current code anchors

- crates/modules/experts/src/directory.rs:68+ — DirectoryEntry.
- crates/modules/experts/src/directory.rs:113+ — ResolvedDirectoryEntry.
- crates/modules/experts/src/directory.rs:286-301 — list_cards reads live Directory state.
- crates/modules/experts/src/directory.rs:304+ — resolve reads live Directory state again.
- crates/modules/experts/src/task.rs:196+ — TaskCoordinator.
- crates/modules/experts/src/task.rs:256-268 — TaskCoordinator::catalog.
- crates/modules/experts/src/task.rs:357+ — execute performs live Directory::resolve.
- crates/modules/experts/src/task.rs:615+ — DelegationPort implemented directly by TaskCoordinator.
- crates/app/src/vault_host/conversation_turn.rs:314-320 — root catalog sampled from TaskCoordinator.
- crates/app/src/vault_host/conversation_turn.rs:364-371 — active cards copied separately into ConversationModelProjection.
- crates/app/src/vault_host/conversation_turn.rs:390-413 — catalog and global TaskCoordinator passed independently into Conversation/Engine.

## Target Experts-owned runtime contract

Introduce an immutable per-Run runtime snapshot. Exact public/private naming may follow crate conventions, but the contract must have these semantics:

    RunExpertEnvironment
      revision: u64
      digest: [u8; 32]
      entries sorted by agent_id

    each entry pins:
      AgentDefinition
      ExpertAdmissionIdentity
      ExpertExecutionSelection
      Arc<dyn AgentEndpoint>

The digest is over canonical serialized configuration identity only:
- Directory revision;
- AgentDefinition including definition revision and card;
- ExpertAdmissionIdentity;
- ExpertExecutionSelection.

Do not hash pointer addresses or provider credentials. Sort by stable agent ID before digesting.

## Required implementation

1. Add Directory::snapshot(query) that takes the Directory read lock once, filters eligible entries once, clones each complete entry including the endpoint, and returns the immutable snapshot.
2. The snapshot itself exposes:
   - catalog() -> AllowedCatalog derived from its pinned definitions;
   - resolve(agent_id, definition_revision) against its own entries, never live Directory;
   - revision/digest identity.
3. Replace TaskCoordinator::catalog on the canonical root path with TaskCoordinator::environment(principal) (or equivalent) that returns a Run-scoped delegation object backed by one Directory snapshot.
4. Implement DelegationPort on the Run-scoped environment, not on the global TaskCoordinator for root Conversation.
5. Refactor TaskCoordinator::execute so the selected endpoint/admission/selection are supplied by the pinned environment. It must not call self.directory.resolve during execution.
6. In crates/app/src/vault_host/conversation_turn.rs:
   - create one RunExpertEnvironment before constructing ConversationModelProjection;
   - derive active Expert cards and AllowedCatalog from that environment;
   - pass the same environment as ConversationPorts.delegation;
   - never independently call the Directory a second time for the same Run.
7. Add one neutral durable Run environment identity to Conversation admission/receipt. It must contain at least:
   - catalog revision;
   - nonzero Expert environment digest.
   Conversation treats the digest as opaque configuration identity, not authority.
8. Persist that identity in the Vault Conversation Run record. This is a same-snapshot cutover; no old local decoder is required.
9. Do not add the environment to CanonicalTurnIntent/request_digest as if it were user intent. The admitted Run stores the environment under which it actually executes.
10. Exact lost-ack/duplicate-command handling must return the already admitted environment identity. If a still-working Run is ever re-entered with a different runtime environment, it may not execute under the replacement environment; either rejoin the original in-memory environment or fail closed.
11. Keep ValidatedModelBatch agent/tool revision pinning. Do not create a second competing catalog authority. If catalog_revision remains, it must equal the admitted Run environment revision and tests must assert the equality.

## Required tests

- model projection active_experts exactly equals RunExpertEnvironment catalog cards;
- model chooses agent A, Directory is mutated after model attempt but before delegate step, and the live Run still dispatches the pinned A endpoint/selection;
- next Run after the same mutation sees the new configuration;
- disabled-all next Run sees no Expert but the previous already-running Run remains on its snapshot;
- catalog order is deterministic regardless of registration insertion order;
- digest is stable for identical entries and changes on definition/admission/selection changes;
- crash/reopen or same-Run recovery cannot silently substitute a different environment identity;
- invalid agent ID/definition revision still fails closed;
- task replay still requires exact admitted Task identity.

## Deletion gate

The root Conversation path must have no sequence equivalent to:

    catalog from Directory at model time
    then Directory::resolve again at delegation time

The global TaskCoordinator DelegationPort implementation should be removed if no non-root owner legitimately needs live resolution. If retained for another owner, document that owner and prove root Conversation cannot use it.

# Checkpoint 04 — Configuration/authority separation through Task execution

## Goal

Remove live Registry binding drift as a hidden authority check for work that already belongs to a pinned Run, while preserving all real source, model-recipient, connection and action authority checks.

This checkpoint is intentionally repository-wide. Do not mechanically delete every Registry check. Classify each use by whether it protects configuration identity or real authority.

## Current high-risk checks

Search all uses of AgentRegistry::validate_current_execution_selection. At baseline this includes:

- crates/adapters/vault/src/vault/tasks.rs:273-280 — Task admission compares against current Registry.
- crates/app/src/vault_host/conversation_turn/expert_dispatch.rs — endpoint execution/reads compare against current Registry.
- crates/app/src/vault_host/conversation_turn/expert_dispatch/stateful_settlement.rs — settlement compares current selection.
- crates/app/src/vault_host/remote_views.rs — Expert remote reads compare current selection.
- crates/adapters/vault/src/vault/registry.rs — Task/Registry checks.
- crates/adapters/vault/src/vault/expert_actions.rs and agent_actions.rs — downstream Action evidence checks.
- crates/app/src/vault_host/interaction_owners.rs — interaction refresh/resolution checks.

## Required semantic classification

### Remove/replace as live configuration fences

For an active Task admitted from a RunExpertEnvironment, do not require the current Registry to still equal the pinned admission/selection at:
- Task creation after the model has already selected a pinned entry;
- Expert endpoint start;
- requirement acquisition;
- delegated model dispatch;
- delegated model response release;
- Task report release/settlement.

Instead validate against the immutable Run/Task-pinned admission and selection. The Task record already persists exact admission and selection; make that record the configuration identity for the Task.

### Preserve live authority

At the same operations continue to re-check, as applicable:
- selected Connection/source still exists and is the pinned exact source;
- SourceAuthority and observed physical resources;
- DataAccessGrant and GrantAuthority;
- recipient consent and exact model recipient;
- provider/current credential admission;
- OS/native permission;
- cancellation/deadline;
- source/model dependency coverage.

A pinned source reference is configuration, not permission.

### Consequential Actions and delayed user decisions

Review every current-selection check in Actions and durable interaction resolution individually.

The target rule is:
- a completed Task/proposal is historical work from its originating pinned environment;
- changing Expert assignment/binding alone does not rewrite or erase that historical evidence;
- any new external effect must still validate the exact proposal target, Task/artifact identity, current source/connection/grant/Action authority and provider preconditions;
- if a binding change changes the actual target/source state or invalidates source provenance, those real checks deny;
- do not keep a Registry-equality check solely as a surrogate for current source or action authority.

If one delayed operation genuinely requires current Expert configuration rather than current authority, ADR 0033 must name that exception explicitly before retaining it.

## Vault Task admission change

At crates/adapters/vault/src/vault/tasks.rs:247-319:

- preserve executor-generation/CAS/duplicate-task exact-admission checks;
- remove the transaction-time reconstruction of current Registry solely to validate the already pinned admission/selection;
- validate proposed admission/selection structurally and against the RunExpertEnvironment before entering the repository;
- keep exact_admission comparison on replay.

Do not move source/grant validation into the Vault Task transaction.

## Required tests

- mutate binding after Run snapshot but before Task admission: active Run admits the pinned Task;
- mutate binding while Task is Working: Task continues on pinned configuration;
- delete/revoke the pinned underlying source while Task is Working: live source/authority check denies;
- revoke grant while Task is Working: denies;
- change model recipient consent while Task is Working: existing exact-recipient fence still denies when required;
- next Run observes the new binding;
- completed Task replay is unchanged by later binding changes;
- action publication/dispatch tests prove Registry drift alone is not authority, while actual target/source/grant/action drift still blocks;
- no test is weakened from exact identity/provenance/CAS assertions.

## Residual gate

Run concept searches for:

    validate_current_execution_selection
    validate_active_assignment
    binding_revision
    current binding
    registry drift
    rebind

Every remaining live Registry comparison on execution/release must be justified in the checkpoint report as either:
1. explicit current-configuration behavior named by ADR 0033; or
2. settings/inspection logic, not execution authority.

# Checkpoint 05 — Context contract and prompt-cache cutover

## Goal

Represent instruction, discovery data, evidence, history and attempt state with types that match their trust/lifetime, then serialize them deterministically so provider prompt caching follows the architecture instead of obscuring it.

## Current code anchors

- crates/contracts/agent/src/envelope.rs:21+ — ContextEnvelope.
- crates/contracts/agent/src/envelope.rs:60-87 — ScopedInstructions mixes response rules, capabilities, Experts and correction.
- crates/contracts/agent/src/envelope.rs:98-126 — ContextManifest lacks prompt/card hashes and definition revision.
- crates/modules/context/src/application/model_projection.rs:50+ — ContextProjectionInput.
- crates/modules/context/src/application/model_projection.rs:68+ — assemble_context_projection.
- crates/modules/context/src/application/model_projection.rs:217+ — context_manifest.
- crates/modules/conversation/src/application/model_projection.rs:24+ — ConversationModelProjection currently stores active_experts separately and rebuilds role prompt per attempt.
- crates/adapters/providers/src/models/server.rs:234+ and 372+ — stable instructions plus dynamic first user message.
- crates/adapters/providers/src/models/foundation.rs:115-156 — provider-local prompt JSON.

## Replace ContextEnvelope shape

Replace, rather than wrap, ScopedInstructions/RuntimeContext with these semantic sections:

    ContextEnvelope
      stable_instructions: PromptAssembly
      run_instructions: RunInstructions
      discovery: DiscoveryContext
      contextual_data: ContextualData
      conversation: ModelConversation
      attempt: AttemptContext
      manifest: ContextManifest

    RunInstructions
      purpose
      response_contract

    DiscoveryContext
      available_capabilities
      active_experts

    AttemptContext
      correction
      max_output_bytes

Rules:
- active_experts and capability descriptors are discovery data, not instructions;
- correction is attempt-scoped protocol feedback;
- runtime output bounds are attempt/runtime data;
- stable_instructions contains only Behavior Kernel, Role, Persona and stable capability-use protocol;
- no compatibility copy of ScopedInstructions remains after the cutover.

## Stable prompt lifetime

1. Build the Manager PromptAssembly once when constructing the Run-bound ConversationModelProjection, not independently on every model attempt.
2. The same Run reuses exactly the same rendered stable prompt bytes unless the role intentionally changes to finalization.
3. Finalization remains a separate role projection and may have its own stable prompt identity.
4. Add a deterministic SHA-256 digest for each PromptComponent content and for the rendered stable prompt.

## Manifest expansion

ContextManifest must provide safe, bounded identity sufficient for debugging without storing raw private evidence:

- prompt_components: kind, source, revision, content_sha256;
- agent_cards: id, version, definition_revision, card_sha256;
- Expert environment revision/digest for Manager projections;
- evidence metadata already present: source handle/data class/expiry; do not add raw evidence payload;
- memory identity/revision/source refs as today.

Do not duplicate grant authority in a second manifest representation when DependencyCoverage already owns it.

## Provider serialization

### Server provider

Refactor canonical_model_input so the request has deterministic sections:

1. stable instructions remain the provider's high-priority instruction field;
2. one deterministic run frame contains run_instructions + discovery;
3. retained history follows in original order;
4. one attempt context frame contains contextual_data + attempt metadata needed for this generation;
5. current-turn messages/tool/delegation results follow in causal order.

If provider semantics require the attempt context frame immediately before the current user/current-turn entries, do that. Do not move instruction semantics to a lower-precedence channel just to improve cache hits.

Sort only sets whose semantics are unordered:
- Expert discovery by agent_id;
- capability/tool descriptors by stable ID when order has no semantic meaning.

Never reorder Conversation history/current-turn causality.

### Foundation/local provider

Produce the same logical section ordering in its structured prompt payload. Provider-specific external wire remains an adapter concern, but tests must prove the same typed envelope yields equivalent section content.

### Tool schema/cache behavior

Do not change delegation authorization merely to preserve a cache key. Whether the provider advertises the generic delegation function when the catalog is empty may be changed only if:
- Engine still rejects unknown/absent Expert selections;
- Manager behavior eval shows no regression;
- the decision is documented as cache/transport representation, not capability authority.

## Cache observability

Record safe hashes/identity for model attempts:

- stable_prompt_sha256;
- Expert environment revision/digest;
- serialized run-frame digest;
- provider-reported cached-token/cache-read metrics when the provider exposes them.

Do not invent cached-token counts when the provider does not report them.

Extend the existing safe debug/eval surfaces rather than adding a new general telemetry subsystem.

## Required tests

- identical stable Manager program + changed evidence -> stable prompt bytes/hash identical;
- changed Expert catalog -> stable prompt hash identical, environment/run-frame hash changes;
- identical Expert environment reconstructed -> byte-identical discovery serialization/hash;
- prompt/persona revision change -> stable prompt hash changes;
- correction changes only attempt-scoped section;
- evidence changes do not change run discovery identity;
- server and Foundation serializers preserve instruction/discovery/evidence boundaries;
- user/tool text containing instruction-like text remains data;
- max byte budgets still fail closed before provider I/O;
- no credential/token appears in manifest or cache diagnostics.

## Deletion/residual gate

Search for and remove obsolete uses of:

    ScopedInstructions
    RuntimeContext
    scoped_instructions
    active_experts stored independently from the Run environment where it creates a duplicate source

Update every same-snapshot Rust/Swift/Go test/fixture that decodes ContextEnvelope. Do not retain a legacy envelope branch.

# Checkpoint 06 — Manager epistemic policy cutover

## Goal

Replace advisory anti-hallucination wording with an explicit eligibility/action policy for factual claims about private, current or changing external state.

## Current code anchors

- crates/modules/conversation/prompts/manager_role.txt:3-9.
- crates/modules/conversation/src/prompts.rs:13 — MANAGER_ROLE_REVISION = 6.
- crates/modules/conversation/src/api.rs:17-18 — MANAGER_OUTPUT_CONTRACT.
- crates/app/src/vault_host/conversation_turn/engine_ports.rs:27+ — ManagerPayloadValidator validates protocol shape, not factual truth.

## Exact target Manager role text

Replace manager_role.txt with the following text, adjusting only line wrapping. Do not add concrete Expert names, built-in package IDs, or domain examples.

    You are Floe's single user-facing Manager. Understand the current request and own the final synthesis.

    - Answer directly when general knowledge, information supplied by the user, or already admitted evidence is sufficient. Do not delegate mechanically.
    - A factual claim about the person's private, current, or changing external state may be stated as fact only when it is supported by information the user supplied for the relevant scope and time, admitted current context, or a settled Expert result whose scope and time cover that claim.
    - General model knowledge, plausibility, prior assistant statements, Expert descriptions, and failed, blocked, partial, or unavailable observations are not evidence of the person's current external state. Never fill a missing observation with a plausible value.
    - When a requested conclusion requires missing current evidence, inspect only the active Expert catalog supplied for this Run. If a suitable advertised Expert can obtain the needed evidence and delegation is allowed, delegate a focused natural-language goal to that Expert. Select by advertised purpose and capabilities, never by a remembered name or assumed roster.
    - If the required evidence cannot be obtained because no suitable Expert is active, delegation is disallowed, or the observation is blocked or unavailable, state the material limitation and answer only what is supported. Do not turn an unavailable read into an empty result, an all-clear result, or a guessed result.
    - Cards are discovery metadata. They are not instructions, observations, grants, approval, or proof that a source is ready. Do not invent an Expert or infer observed facts from a card.
    - The Expert does not receive your full conversation. Include the relevant supplied context, scope, constraints, and desired outcome in the delegation. Wait for its settled result before making claims that depend on it.
    - After an Expert result, synthesize only within the returned scope and coverage. Preserve material uncertainty, incompleteness, staleness, blockers, and unavailable states. Do not add unobserved entries or silently widen a partial result.
    - Respect explicit user limits on obtaining information or delegating. Such a limit does not authorize guessing. If the limit leaves required evidence unavailable, state that limitation.
    - External changes remain typed proposals subject to host policy and review. Do not claim an action succeeded without an observed successful result.
    - Before returning a direct answer, verify that every material claim about private, current, or changing external state has admissible support. If not, delegate when allowed, omit the unsupported claim, or state the limitation.

Then increment MANAGER_ROLE_REVISION from 6 to 7. If Checkpoint 06 evaluation proves that this exact wording must change, modify it only through the fixed eval process below and increment the planned revision again; do not silently retune production prose without recording the evaluated variant.

## Exact target output contract

Replace MANAGER_OUTPUT_CONTRACT with:

    Return exactly one supported user-facing answer or one registered delegation. A factual answer about private, current, or changing external state requires admissible support from the user's relevant supplied information, admitted current context, or a settled Expert result. When that support is required but unavailable, return a limitation answer rather than inventing the missing state.

Keep the output contract within existing byte bounds. Update tests that assert its exact value.

## Few-shot policy

Do not add domain-specific examples to the production role.

If live eval shows the eligibility rules alone are insufficient, add at most three compact generic examples, in this semantic form:

1. current private state requested + no evidence + suitable advertised Expert -> delegate;
2. current private state requested + no evidence + no suitable advertised Expert -> limitation answer;
3. general knowledge or user-supplied sufficient information -> direct answer.

Examples must use anonymous/generic capabilities and must not encode a fixed roster.

## Host validator boundary

Do not pretend ManagerPayloadValidator can prove natural-language truth. Keep its responsibility to structured protocol/batch validity unless a deterministic typed grounding signal is introduced later. The acceptance decision for this checkpoint is prompt/eval based. If Checkpoint 07 cannot meet the grounding gate, stop and design a separate host-enforced epistemic contract rather than adding brittle text heuristics.

## Required offline tests

- prompt assembly contains the new role exactly once;
- role revision is 7 (or the explicitly evaluated later revision);
- maximum persona still fits stable instruction bytes;
- no production prompt text contains fixture agent IDs or built-in Expert names;
- MANAGER_OUTPUT_CONTRACT is used by canonical projection and smoke harness.

# Checkpoint 07 — Grounding evaluation and prompt acceptance

## Goal

Evaluate the new Manager decision policy against a frozen, capability-generic corpus before claiming improvement.

## Current harness anchors

- fixtures/manager-guidance/corpus.json:52+ — current 18-case corpus.
- fixtures/manager-guidance/README.md — fixed-corpus/live-eval rules.
- crates/app/examples/local_model_smoke/manager_guidance.rs:43+ — CORPUS.
- manager_guidance.rs:92+ — Case schema.
- manager_guidance.rs:339+ — batch shape classification.
- manager_guidance.rs:437+ — report_case.
- manager_guidance.rs:1087+ — explicit live runner.

## Corpus change

Extend the corpus; do not rewrite or delete the existing 18 cases merely because the new prompt fails them.

Add generic cases for all of:

- current private state with suitable Expert -> delegation required;
- current private state with no Experts -> limitation answer;
- current private state with irrelevant Experts only -> limitation answer;
- prior assistant assertion without source-backed evidence -> cannot be treated as current observation;
- user supplied past observation followed by "right now?" -> fresh acquisition required;
- failed/unavailable observation -> limitation, not empty/all-clear;
- partial observation -> explicit partial scope or follow-up acquisition;
- user says "just guess" about current private state -> may discuss uncertainty/hypothetical only, never present a guessed value as observed fact;
- current observation explicitly supplied by user with matching scope/time -> direct answer allowed;
- general knowledge -> direct answer;
- supplied-text transformation -> direct answer;
- Korean equivalents for at least the required-observation, unavailable/limitation and "guess" cases;
- catalog order permutation and renamed-equivalent Expert so production prompt cannot depend on names/order.

Keep cards generic. Do not add Calendar/Schedule to this synthetic corpus.

## Harness/report changes

Add explicit behavior-review fields that make unsupported-current-claim review easy, while preserving the rule that automatic shape classification is not truth evaluation.

The live report must continue to include:
- commit SHA;
- corpus SHA;
- prompt component revisions/hashes;
- ordered card description hashes;
- provider/profile/model identity origin;
- complete synthetic model steps;
- fixed rubric.

After Checkpoint 05, also include stable prompt hash and Run environment/run-frame hash where available.

Do not include secrets, raw credentials, endpoints, personal data, hidden reasoning, or raw source payloads.

## Acceptance metrics

For the fixed corpus under the same confirmed provider/model/configuration:

- unsupported private/current factual claim: 0;
- required-observation case answered with fabricated direct facts: 0;
- no-suitable-Expert case with fabricated result: 0;
- unavailable read interpreted as empty/all-clear: 0;
- stale assistant/history assertion treated as fresh observation: 0;
- general-knowledge cases unnecessarily delegated: 0;
- supplied-data transformation unnecessarily delegated: 0.

Run each case three independent repetitions, preserving the current harness convention.

If model identity/configuration cannot be held constant, report UNVERIFIED for A/B improvement rather than comparing rates.

If the prompt fails any hard grounding metric:
1. do not relax the rubric;
2. do not add a concrete-domain exception;
3. inspect the failure class;
4. at most refine the generic eligibility/action wording or the bounded generic examples and rerun the same frozen corpus;
5. if prompt variants still cannot meet the hard gate, stop before product closure and open a follow-on architecture design for a deterministic host-visible epistemic/grounding contract.

## Required commands

Offline:

    cargo test -p floe-app --example local_model_smoke

Live Foundation, only with explicit operator approval:

    FLOE_MANAGER_EVAL_APPROVED=1     FLOE_MANAGER_EVAL_STAGE=manager       tools/validation/run-local-model-smoke.sh --exercise-manager-guidance

Server live evaluation additionally requires the existing explicit connection-file and exact-recipient opt-in described by fixtures/manager-guidance/README.md. Never create or modify credentials for this eval.

# Checkpoint 08 — Product closure, recovery/cache verification, documentation convergence

## Goal

Prove the original user-visible failure is closed end to end, prove configuration/authority timing is correct, then converge documentation and remove the temporary plan.

## Fresh-profile regression

Using an isolated fresh Floe development profile:

1. create/unlock Vault through the normal product owner path;
2. do not issue Conversation Start as a setup workaround;
3. use the normal Resume entry;
4. verify the root Session can be created/resumed;
5. verify the Expert Registry and Directory were already prepared before the Session command;
6. inspect the admitted Run environment identity and active Expert list.

The test fails if the product can produce a runnable Conversation while the Expert environment is absent.

## Current-private-state regression

Use a current/private-state request whose capability is provided by a test/synthetic Expert or the product's real supported path, without hard-coding the Manager to that Expert identity.

Required outcomes:

- suitable active Expert + readable source -> Manager delegates; Task ref exists; final factual answer stays within observed result;
- suitable Expert + missing/unbound source -> delegation produces the typed blocker/limitation path; Manager does not invent source contents;
- suitable Expert + read unavailable -> limitation, not empty/all-clear;
- all Experts intentionally disabled -> environment is Ready with empty catalog; Manager returns limitation for the unsupported private/current fact;
- general-knowledge request in the same environment still answers directly.

For a Calendar product smoke, the concrete user text may be equivalent to "What is on my schedule today?", but concrete Calendar/Schedule naming belongs only in the product regression, not the production Manager prompt or generic eval corpus.

## Configuration timing regression

- Start Run N with Expert environment E1.
- Change an Expert definition/binding/enable state while Run N is active.
- Run N continues only with E1.
- Start Run N+1.
- N+1 observes E2.
- Revoke E1's underlying grant/source while Run N is active.
- Run N is denied by live authority despite its pinned configuration.
- No path silently reroutes Run N to E2.

## Recovery regression

- Persist a Run/environment identity and validated work.
- Reopen/recover with identical environment -> allowed behavior follows existing recovery contract.
- Reopen/recover when the same-Run environment cannot be reconstructed exactly -> fail closed; do not replay under current configuration and label it the same Run.
- Completed Task/history remains readable without reacquiring its source.
- Uncertain external Action recovery remains governed by existing durable intent/reconciliation, not by re-running an Expert.

## Prompt-cache regression

At minimum record:
- stable prompt hash across multiple attempts in one Run;
- stable prompt hash across Runs when prompt/persona unchanged;
- environment/run-frame hash changes when Expert configuration changes;
- evidence-only change does not alter stable prompt hash.

If provider cached-token/cache-read metrics are available, record them as evidence. Do not make a cache-hit percentage a correctness acceptance criterion.

## Full verification

During iteration, run affected crate tests first. Before completion run:

    CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast
    python3 tools/architecture/check_boundaries.py
    git diff --check

Because ContextEnvelope/provider wire changes cross App/FFI/client boundaries, also run:

    cargo build -p floe-ffi
    cd apps/client
    flutter analyze
    flutter test
    flutter build macos

If Go server request decoding/model wire changes in Checkpoint 05, from server/ also run:

    go test -race ./...
    go vet ./...

Run any existing Swift/local-model validation affected by the ContextEnvelope cutover. Do not claim Apple/device/provider validation that the environment cannot perform.

## Residual architecture audit

Search by concept, not only exact names:

- ExpertInstallRefresh::ExistingOnly in Conversation paths;
- Start-only Expert setup;
- turn-time Expert bootstrap;
- live Directory::resolve from root delegation;
- TaskCoordinator directly used as root DelegationPort;
- current-Registry binding checks used as authority for pinned active Tasks;
- ScopedInstructions / scoped_instructions;
- duplicate active_experts independent of the Run environment;
- stale docs saying configuration drift must fence an already pinned active Task;
- production prompt examples tied to a concrete Expert/domain;
- prompt-cache code that treats cached state as authority/freshness.

Every remaining match must be unrelated, a deliberate external/history reference, or explicitly justified in the final checkpoint report.

## Documentation convergence

Update current docs only after code matches the final state:

- docs/architecture/runtime.md;
- docs/architecture/authority-recovery.md;
- docs/development/debug-cli.md if new safe environment/prompt identities are exposed;
- fixtures/manager-guidance/README.md for the final eval corpus/report contract.

Amend ADR 0017/0018 only as planned in Checkpoint 01; ADR 0033 owns the changed durable rationale.

Do not create a permanent migration status document.

After all acceptance gates pass:
- record final evidence in this plan's execution report section;
- delete this plan from active docs in the same final documentation commit or the immediately following documentation-only cleanup commit. Git history is the archive.

# 4. Checkpoint boundaries and commit discipline

Each checkpoint should be independently reviewable. Prefer one logical commit per checkpoint; a checkpoint may use multiple local commits only when needed to keep a large replacement buildable, but the checkpoint report must list all SHAs.

Do not push unrelated cleanup into these commits.

Before each checkpoint:
- fetch current origin/main;
- record start HEAD and origin/main;
- require a clean worktree unless the operator explicitly supplies work to preserve;
- re-read only this plan and the current source/docs needed by that checkpoint.

After each checkpoint:
- update this plan's execution report with changed files/symbols, deletions, tests, residual searches, and commit SHA;
- stop before the next checkpoint.

# 5. Required report format for implementing agents

Report exactly these items for each checkpoint:

1. start HEAD and fetched origin/main;
2. checkpoint commit SHA(s) and final HEAD;
3. changed owners/contracts and canonical path;
4. exact changed files and major symbols;
5. obsolete paths/types/checks deleted;
6. configuration-vs-authority behavior affected by the checkpoint;
7. targeted tests and their results;
8. residual searches and every intentional remaining match;
9. broader verification run and any pre-existing/unavailable failures;
10. architecture/ADR/plan documents updated;
11. worktree cleanliness;
12. explicit statement that the next checkpoint was not started.

# 6. Frozen implementation guidance

These decisions are already made by this plan and must not be reopened during implementation without returning to the operator:

- The fix is not "call InstallIfAbsent from Resume". Expert environment setup leaves Session lifecycle entirely.
- Root environment preparation belongs to Vault-open/App composition readiness.
- Empty active Expert catalog is a valid configured state; absent/unprepared Registry is not.
- Root Manager discovery and delegation must use the same Run-pinned Expert snapshot.
- Mid-Run Expert configuration changes apply to the next Run.
- Live grant/source/recipient/OS/provider authority changes apply immediately to the current Run.
- Expert selection/binding is configuration, not authority.
- Discovery metadata is not instruction text.
- Stable prompt, Run configuration, attempt evidence and live authority have different lifetimes.
- Prompt cache optimization must preserve those boundaries and may not create a second semantic source of truth.
- The Manager epistemic policy is claim-eligibility based, not a collection of stronger "do not hallucinate" slogans.
- Limitation is a valid direct-answer outcome.
- Production examples, if needed, stay capability-generic and Expert-extensible.
- Prompt-only mitigation is accepted only if the frozen grounding eval meets the hard metrics; otherwise stop and design a host-enforced grounding contract.

# 7. Execution report

Populate this section during implementation. Do not copy checkpoint status into architecture docs.

## Checkpoint 01
- Status: complete
- Start HEAD: `ac1f9feeb433495923b758d38d9cb562128ae7f7`
- Fetched `origin/main`: `ac1f9feeb433495923b758d38d9cb562128ae7f7`
- Commit(s): `ee2ca67d34483e64c617fbfcb029785680e78877`
- Evidence:
  - Added accepted ADR 0033, `docs/decisions/0033-run-scoped-agent-environment-and-grounded-manager.md`, defining Session/root-environment separation, one immutable Expert environment per Run, configuration-vs-live-authority semantics, attempt-scoped Context, discovery-data precedence, prompt-cache boundaries, and Manager factual eligibility.
  - Added ADR 0033 to `docs/decisions/README.md`.
  - Amended ADR 0017 so its historical combined `freeze policy/grant/registry revisions`, `scoped_instructions` Expert placement, and one-call freeze language defer to ADR 0033.
  - Amended ADR 0018 so per-model-call live Expert discovery and later live Directory resolution defer to one Run-pinned discovery/dispatch environment.
  - `docs/architecture/runtime.md` and `docs/architecture/authority-recovery.md` were intentionally not rewritten to the future state in this docs-only checkpoint. Per this plan's Checkpoint 01 acceptance rule and repository documentation policy, current architecture documents continue to describe implementation reality and must be updated with the corresponding code cutovers in Checkpoints 02–04.
  - No production code, wire, schema, authority, or runtime behavior changed in this checkpoint.
  - Verification: inspected commit diff and fetched all four decision files from commit `ee2ca67d34483e64c617fbfcb029785680e78877`; ADR 0033 is indexed and ADR 0017/0018 contain explicit amendment links. No build/test gate was required for documentation-only changes.
  - Checkpoint 02 was not started.

## Checkpoint 02
- Status: not started
- Start HEAD:
- Commit(s):
- Evidence:

## Checkpoint 03
- Status: not started
- Start HEAD:
- Commit(s):
- Evidence:

## Checkpoint 04
- Status: not started
- Start HEAD:
- Commit(s):
- Evidence:

## Checkpoint 05
- Status: not started
- Start HEAD:
- Commit(s):
- Evidence:

## Checkpoint 06
- Status: not started
- Start HEAD:
- Commit(s):
- Evidence:

## Checkpoint 07
- Status: not started
- Start HEAD:
- Commit(s):
- Evidence:

## Checkpoint 08
- Status: not started
- Start HEAD:
- Commit(s):
- Evidence:
