# Agent execution environment and grounded Manager convergence

- Status: planned
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

## Goal

Make Expert Registry/package readiness a Vault-open/root-Agent invariant instead of a Conversation Start side effect or turn-time repair.

## Current code anchors

- crates/app/src/vault_host.rs:227-239 — OpenVault fields.
- crates/app/src/vault_host.rs:280-315 — OpenVault::activate.
- crates/app/src/vault_host.rs:317+ — publish_expert_directory.
- crates/app/src/vault_host.rs:2113+ — execute_conversation_turn_action currently performs ExistingOnly refresh.
- crates/app/src/vault_host.rs:2206+ — execute_conversation_resume_action currently performs ExistingOnly refresh.
- crates/app/src/vault_host.rs:2601-2640 — ConversationSessionOperation Start/Resume split; Start owns InstallIfAbsent today.
- crates/app/src/vault_host.rs:3058+ — ensure_expert_bundle wrapper.
- crates/app/src/vault_host/expert_setup.rs:12-48 — VaultExpertBundle / ExpertInstallStore.
- crates/app/src/vault_host/expert_binding_settings.rs:272-382 — bind_initial_defaults.
- crates/modules/experts/src/bundle_install.rs — InstallIfAbsent vs ExistingOnly semantics.

## Required implementation

1. Introduce one App-private root-environment preparation operation invoked inside OpenVault activation before the OpenVault becomes externally usable. A suitable private name is prepare_root_agent_environment; do not create a public/FFI command.
2. Extend OpenVault::activate inputs with the verified local device ID and one trusted operation ID from the admitted create/unlock job. Do not obtain either from a product payload that the caller can forge.
3. Inside OpenVault::activate:
   - construct the Vault/repositories/TaskCoordinator as needed;
   - read whether the Expert Registry is genuinely absent before reconciliation;
   - call ensure_expert_bundle with InstallIfAbsent;
   - only when the Registry was genuinely absent, run first-install default binding with the verified device;
   - publish the Expert Directory from the resulting Registry;
   - return OpenVault only after this succeeds.
4. Refactor bind_initial_defaults so first-install source selection remains idempotent and deterministic but is no longer semantically attached to Conversation Start.
5. Preserve the current rule that an existing Registry is never silently rebound on startup. Disabled assignments remain disabled. Existing explicit source selections remain unchanged.
6. Treat an existing valid Registry with zero enabled assignments as Ready, not uninitialized.
7. Delete the Start-only setup from ConversationSessionOperation::Start.
8. Delete the ExistingOnly refresh calls from execute_conversation_turn_action and execute_conversation_resume_action. A turn must not repair an uninitialized environment.
9. Keep ConversationSessionOperation::Start and Resume concerned only with Session semantics. Resume may continue to restore-or-create the root Session unless a separate product decision changes that behavior.
10. Do not add an EnvironmentReady wire state. OpenVault activation either produces a usable root environment or returns an existing typed failure.

## Required tests

Add/replace tests covering:

- create -> OpenVault activation creates Registry/package environment before any Session command;
- unlock of a valid existing profile republishes Directory without changing explicit Registry configuration;
- Resume on a fresh profile can create a Session, and Registry is already present before that Resume;
- Start and Resume produce equivalent Expert readiness;
- all Expert assignments disabled -> activation succeeds and Manager catalog is intentionally empty;
- existing source binding is not auto-rebound on unlock;
- first-install default binding runs only for genuinely absent Registry;
- repeated activation/reopen is idempotent with respect to install receipt, Registry revision, enabled/disabled state, and binding revision;
- storage/cancellation failures during environment preparation do not fall through into a runnable Conversation with an absent Registry.

Update tests around crates/app/src/vault_host.rs:3730+ and crates/app/src/vault_host/tests/registered_runner.rs rather than preserving assertions that setup is Start-only.

## Residual gate

Search and classify all matches for:

    ExpertInstallRefresh::ExistingOnly
    ConversationSessionOperation::Start
    ensure_expert_bundle
    bind_initial_defaults

Acceptance requires no turn-time or Start-only environment repair path. ExistingOnly may remain for a genuinely different non-Conversation use only if the plan report identifies that owner and reason.

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
- Status: not started
- Start HEAD:
- Commit(s):
- Evidence:

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
