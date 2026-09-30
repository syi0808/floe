# Manager–Expert domain acquisition boundary convergence

- **Status:** active
- **Execution shape:** one atomic architecture task; no checkpoints
- **Planning baseline:** main at 430fd3530549c5fee94cbb13e3d698d940852bfd
- **Baseline date:** 2026-09-30
- **Primary owners:** Conversation/App composition, Context, Experts, Access
- **Required repo workflows:** AGENTS.md, .agents/skills/architecture-change/SKILL.md, .agents/skills/code-change-verification/SKILL.md

Line numbers below are baseline locators for 430fd353, not stable identifiers. Before editing, fetch current main, require a clean worktree, compare it with this baseline, and re-resolve every named symbol in current source. If main moved only by unrelated changes, adapt the locators without changing this contract. If the ownership model itself moved, stop and reconcile the plan before implementing.

## 1. Purpose

The current root Manager still owns seven source-backed domain Tools:

- people.identity.read
- schedule.feasibility.read
- attention.coarse.read
- wellbeing.derived.read
- mail.communication.read
- work.context.read
- life.logistics.read

That is inconsistent with the intended Manager–Expert boundary. The Manager is the single user-facing orchestrator: it understands the request, chooses an active Expert, delegates a natural-language goal through the A2A Task boundary, synthesizes admitted results, and owns the final user response. New source-backed domain acquisition and domain judgment belong inside admitted Expert execution, not in a parallel Manager direct-read path.

This task removes that parallel path and converges first-party Observe policy accordingly.

## 2. Final architecture contract

After this task, the canonical general-turn topology is:

    Person
      -> Manager
          -> already-admitted Conversation / Persona / Memory / product context
          -> Active Expert Index
          -> A2A Delegate
              -> admitted Expert Task
                  -> Expert-declared source requirement
                  -> Context + Access
                  -> exact selected source
                  -> Expert judgment / Artifact
          -> Manager synthesis
      -> Person

The following invariants are normative for this task:

1. The root Manager advertises no source-backed domain Tool descriptors.
2. The root Manager cannot directly acquire Contacts, Feasibility, Attention, Wellbeing, Mail, Work Context, or Life Logistics evidence.
3. Manager-to-Expert domain collaboration remains A2A delegation, not Tool execution.
4. Source-backed domain reads used by built-in Experts remain selected by the admitted Expert manifest and Task selection and are authorized under the exact Expert consumer.
5. Standing first-party Observe consumers are derived from trusted shipped Expert capability declarations only. The product no longer adds the assistant consumer for a Manager direct reader.
6. Registry installation, assignment, binding, or arbitrary extension state still cannot widen first-party Observe consumers.
7. Generic Agent Runtime Tool contracts stay role-neutral and remain available for Expert capabilities and future non-domain product-shell capabilities. This task does not delete ToolDescriptor, ToolPort, ModelStep::CallTool, Tool journaling, or provider Tool wire support.
8. A future Manager presentation/navigation capability is not implemented here. It may be added later as an App/client product-shell capability without reintroducing domain source access.
9. Feasibility remains a separate contextual Access/native substrate in this task. schedule.feasibility.read is removed from the Manager, but this task does not invent a new Expert source-selection model for Feasibility and does not delete the Apple/native Feasibility implementation.
10. No compatibility shim, legacy Manager catalog, fallback direct reader, or dual policy is retained.

## 3. Baseline inventory and disposition

### 3.1 Context-owned direct Manager Tool surface

**crates/modules/context/src/application/tools.rs**

Baseline 430fd353:

- lines 1–9: module is explicitly the Manager direct Tool catalogue.
- lines 27–36: seven Manager Tool IDs and MANAGER_TOOL_DEFINITION_REVISION.
- lines 38–65: Manager Tool output class, schemas, and descriptor builder.
- lines 68–118: ManagerToolSpec and MANAGER_TOOLS.
- lines 120–135: manager_direct_remote_view and manager_direct_native_connector.
- lines 137–143: manager_tool_descriptors.
- lines 145–623: ContextToolService and all direct Manager source acquisition.
- lines 625 onward: fixtures and tests for that direct path.

**Disposition:** delete this module completely. Do not move its Manager-specific descriptor/invoke layer elsewhere.

The lower-level Context/Access acquisition functions that Experts or other real owners use remain in their existing modules. Deleting application/tools.rs must not delete source authorization, source candidate discovery, selected Expert reads, dependency reauthorization, or generic remote/native View primitives.

**crates/modules/context/src/lib.rs**

Baseline:

- line 28: application::tools module declaration.
- lines 110–115: public re-exports of ContextToolService, the seven Tool IDs, MANAGER_TOOL_DEFINITION_REVISION, manager_direct_native_connector, manager_direct_remote_view, and manager_tool_descriptors.
- lines 39–40: ASSISTANT_CONSUMER remains referenced by the separate Feasibility contextual authority today.

**Disposition:** remove the tools module declaration and every direct-Manager Tool re-export. Do not remove or rename ASSISTANT_CONSUMER merely to make this diff look cleaner; first run the residual audit after direct Tool deletion. Any remaining use must be a real non-standing contextual authority such as Feasibility, not a hidden Manager domain-read route.

### 3.2 Root Manager composition

**crates/app/src/vault_host/conversation_turn.rs**

Baseline:

- lines 314–328: root AllowedCatalog combines Expert cards with floe_context::manager_tool_descriptors().
- lines 387–397: root turn constructs ContextToolService.
- lines 398–409: direct Tool-only review snapshot composition.
- lines 410–419: PublishingToolPort wraps the direct Context Tool service.
- lines 455–462: ConversationPorts receives that wrapper as its ToolPort.

**Disposition:**

- Keep Expert cards from the Experts-owned Directory.
- Set the root Manager Tool catalogue to an empty vector.
- Remove ContextToolService construction and the review-snapshot locals used only by that direct Tool wrapper.
- Keep personal/remote/calendar DependencyResolvers required for Conversation history/model coverage reauthorization.
- Keep TaskCoordinator as the Manager DelegationPort.
- Provide a tiny App-private fail-closed ToolPort only because ConversationPorts/EnginePorts are role-neutral and still require a ToolPort reference. Put it in conversation_turn/engine_ports.rs next to ManagerPayloadValidator unless current source has a better existing private seam.
- Name the type for what it means, for example NoManagerTools. invoke must return CapabilityDenied and perform no I/O, publication, or state mutation.
- Do not change the generic ConversationPorts or Agent Runtime contracts just to make the root Manager special.

**crates/app/src/vault_host/conversation_turn/engine_ports.rs**

Baseline lines 1–20 currently contain only ManagerPayloadValidator.

**Disposition:** add the private fail-closed Manager ToolPort here if no equivalent production port already exists at execution time. The implementation should be intentionally trivial and have a regression proving an unexpected invocation is denied. It is a role-composition fence, not a new abstraction layer.

### 3.3 Direct Tool publication wrapper versus shared blocker publication

**crates/app/src/vault_host/conversation_turn/interaction_publication.rs**

Baseline:

- lines 23–48: ToolOutcomePort exists only to adapt ContextToolService.
- lines 50–549: generic blocker labels, reviewed target construction, source/model/Expert publication helpers, safe interaction artifacts, and blocked result construction.
- lines 552–633: PublishingToolPort is the product direct-Tool wrapper.

**Disposition:**

Delete:

- ToolOutcomePort and its ContextToolService implementation.
- PublishingToolPort and its ToolPort implementation.
- tests whose subject is the root Manager direct Tool wrapper or InteractionOrigin::Tool produced by that wrapper.

Preserve:

- generic_source_label;
- interaction_requirement and reviewed target construction;
- publish_model_blocker;
- publish_expert_binding_blockers;
- publish_requirements where used by admitted Expert/source blocker publication;
- blocked_text, interaction_ref_artifacts, blocked_tool_result where still used by the Expert path;
- model-recipient blocker publication and Expert Task-origin interaction semantics.

Do not delete shared blocker publication simply because the root Manager Tool caller disappears.

### 3.4 First-party standing Observe policy

**crates/app/src/first_party_observe.rs**

Baseline:

- lines 18–38: trusted_shipped_consumers derives consumers from shipped Expert manifests. This is the desired authority input and remains.
- lines 40–62: policy adds assistant when manager_direct_remote_view(view_id) is true, then adds trusted Experts.
- lines 118–141: personal_policy adds assistant when manager_direct_native_connector(connector_id) is true, then adds trusted Experts.
- lines 185 onward: tests still encode Manager + trusted Expert consumer semantics.

**Disposition:**

- policy(view_id, categories) must derive consumers only from trusted_shipped_consumers(view_id).
- personal_policy(connector_id) must no longer add assistant. Preserve the existing contacts.android product exception unless a separate current requirement changes it; Android work is out of scope.
- Keep deterministic sorting/deduplication, categories, Read operation, Assistant purpose, LocalOnly processing, and policy_digest inputs unchanged apart from the consumer-set contraction.
- Do not make Registry state an input.
- Do not add an empty-consumer fallback or wildcard.

Policy digest changes caused by removing assistant are intentional. Old local development reviews may become stale and require fresh review. Do not add a migration, compatibility decoder, or schema/protocol bump for disposable local development state.

### 3.5 Built-in Expert coverage of the removed standing views

**crates/experts/builtin/src/catalog.rs**

Baseline lines 61–74 and 177–190 already declare:

- people.identity -> Relationships;
- mail.communication -> Commitments and Communication;
- attention.coarse -> Focus & Attention;
- wellbeing.derived -> Wellbeing;
- work.context -> Focus & Attention and Work Context;
- life.logistics -> Life Logistics.

No migration of those six domains is needed. Their existing manifest-declared selected-source paths are the final path.

Add or strengthen a small manifest/policy regression only where useful to prove the removed Manager consumer is not replaced by an implicit wildcard.

### 3.6 Feasibility exception and bounded non-goal

At baseline:

- Context direct Tool schedule.feasibility.read is a Manager-only caller.
- BuiltinExpertKind::Schedule currently declares Calendar, not schedule.feasibility, as its source requirement.
- crates/modules/context/src/application/expert_sources.rs has no LocalExpertSource::Feasibility.
- AppLocalExpertSource in conversation_turn/expert_dispatch.rs has no Feasibility branch.
- docs/architecture/modules.md and authority-recovery.md explicitly describe Feasibility as a separate contextual Access contract rather than a standing SourceConnection candidate.

Therefore this task must **not** silently add schedule.feasibility to the Schedule Expert manifest. Doing that correctly would require defining its selection/admission semantics and is a separate architecture decision.

For this task:

- remove schedule.feasibility.read with the rest of the Manager direct Tool catalogue;
- keep the typed Feasibility View, contextual Access record/review, native acquisition provider, and their tests;
- ensure no root Manager catalog or ToolPort can invoke it;
- document that the substrate is not a standing first-party Observe consumer and is not a Manager direct read;
- leave future Schedule-Expert Feasibility integration for a separate task.

### 3.7 Current tests that encode the obsolete path

**crates/app/src/vault_host/conversation_turn.rs test module**

Baseline examples:

- around 1994–2025: asserts seven Manager tools and model-route-independent descriptors.
- around 2159–2170: direct Manager mail input validation.
- around 2173–2252: ContextToolService personal/remote blocker behavior.
- around 2306 onward: direct attention Tool journal/publication coverage.
- around 2568 onward: direct Manager ready-result coverage.
- around 2715–2718 and 3617–3620: projection fixtures call manager_tool_descriptors().
- around 3527 onward: direct attention read coverage.

**Disposition:** delete tests whose only contract is the removed direct Manager Tool layer. Replace fixture catalog construction with tools: vec![] when the test is really about Manager projection/history/model behavior. Remove now-unused fixtures/imports rather than retaining a test-only shadow of the deleted design.

**crates/app/src/vault_host/tests/conversation_flows.rs**

Baseline around 2977:

- direct_attention_tool_blocked_completes_turn_with_one_durable_ref drives the Manager through attention.coarse.read and expects a Tool-origin review card.

**Disposition:** delete or replace it with a negative general-turn regression that proves a domain Tool is not admitted by the Manager catalogue and cannot create a direct Tool-origin source interaction. Do not preserve the old flow merely to keep a blocker test; Expert blocker publication already has its own Task-origin path.

**crates/app/src/vault_host/tests/interaction_resolution.rs**

Baseline around 3108:

- manager_mail_read_requires_assistant_in_reviewed_product_policy asserts assistant in the reviewed Mail policy;
- directly reads Mail as assistant;
- constructs ContextToolService and invokes MAIL_COMMUNICATION_READ;
- separately proves an extension is not authorized.

**Disposition:** rewrite this as a first-party policy/authorization regression:

- reviewed Mail policy contains only shipped Experts that declare mail.communication;
- assistant is absent;
- a trusted shipped Mail Expert consumer can be admitted;
- an arbitrary extension remains blocked;
- no ContextToolService or Manager Tool invocation remains.

Keep the useful extension-denial assertion.

## 4. Ordered single-task execution

This is one work item. The sections below are an execution order, not checkpoints. Do not leave intermediate compatibility paths committed.

### Step 1 — establish the Manager composition boundary

1. Re-fetch origin/main and record start HEAD.
2. Require a clean worktree; preserve any operator work rather than overwriting it.
3. In conversation_turn/engine_ports.rs, add or reuse one private fail-closed ToolPort for a root Manager with an empty Tool catalogue.
4. In run_general_turn:
   - keep Directory Expert cards;
   - set AllowedCatalog.tools to vec![];
   - remove ContextToolService and PublishingToolPort construction;
   - pass the fail-closed Manager ToolPort into ConversationPorts;
   - keep A2A delegation, model projection, dependency resolvers, inference fencing, and active Expert index unchanged.
5. Add a focused regression that the unexpected Manager Tool port invocation fails closed and that normal delegation remains usable.

Do not modify generic Tool validation/runtime semantics.

### Step 2 — delete the obsolete Context direct-Tool owner surface

1. Delete crates/modules/context/src/application/tools.rs.
2. Remove application::tools and all direct Manager Tool re-exports from floe-context.
3. Compile floe-context and floe-app immediately.
4. Resolve all compile errors by moving callers to the final architecture, not by reintroducing a wrapper.
5. Delete test fixtures/imports that existed only for ContextToolService.

No replacement Manager source service is permitted.

### Step 3 — delete the App direct-Tool publication adapter

1. Remove ToolOutcomePort and PublishingToolPort from interaction_publication.rs.
2. Preserve Task/model/source blocker publication helpers used by Expert execution.
3. Delete only direct-Tool adapter tests.
4. Verify Expert binding/source/model blockers still publish under Task origin and retain safe refs.

### Step 4 — contract first-party Observe consumers to shipped Experts

1. Remove manager_direct_remote_view and manager_direct_native_connector use from first_party_observe.
2. Remote and native personal standing policies must obtain consumers from trusted_shipped_consumers only.
3. Rewrite policy unit tests:
   - no assistant in Contacts/Attention/Wellbeing standing consumers;
   - no assistant in Mail/Work/Logistics remote policies;
   - Calendar behavior remains shipped-Expert-only;
   - expected shipped consumers still match manifest declarations;
   - arbitrary extension remains excluded;
   - registry/binding state still cannot affect policy or digest.
4. Retain policy_digest canonicalization tests.
5. Do not alter the separate Feasibility contextual authority to mimic standing Observe.

### Step 5 — port integration tests to the final boundary

1. Remove root direct Tool tests listed in section 3.7.
2. Replace useful coverage with:
   - Manager has no domain Tool descriptors;
   - Manager direct Tool invocation cannot create a source-read interaction;
   - A2A delegation remains the domain execution path;
   - trusted shipped Expert source reads still use the package consumer and preserve dependency coverage;
   - source blockers from Expert execution still produce Task-origin durable interactions;
   - assistant is not a standing Observe consumer;
   - extensions remain denied by default.
3. Prefer existing Expert delegation/source tests over duplicating the same acquisition behavior in App tests.
4. Keep generic Agent Runtime Tool tests; they protect role-neutral runtime behavior and are not obsolete.

### Step 6 — add machine enforcement for the standing-policy regression

**tools/architecture/check_connection_observe_conformance.py**

Baseline lines 25–42 define architecture regexes; lines 66–105 scan production source.

Extend this existing checker narrowly:

- reject manager_direct_remote_view and manager_direct_native_connector if reintroduced in production;
- reject assistant/ASSISTANT_CONSUMER injection inside crates/app/src/first_party_observe.rs;
- do not ban assistant globally because Feasibility is a distinct contextual authority;
- do not ban ToolPort/ToolDescriptor globally because the runtime remains role-neutral and future product-shell capabilities are valid.

**tools/architecture/test_check_connection_observe_conformance.py**

Baseline lines 25–37 contain forbidden fixtures and lines 39–48 allowed owner values.

Add regression fixtures for the new rule and keep a positive fixture that shipped manifest-derived policy is allowed.

### Step 7 — converge Manager prompt, product meaning, architecture, and ADR

**crates/modules/conversation/prompts/manager_role.txt**

Baseline lines 3–6 currently tell the Manager to select/seek evidence directly and delegate only when independent judgment would materially improve the answer.

Rewrite the role so that:

- the Manager may answer from already-admitted Conversation/Persona/Memory/product context when sufficient;
- when new source-backed domain evidence or domain judgment is required, it delegates to an active Expert;
- the Manager sends a natural-language goal/context/constraints and owns synthesis;
- source/tool blocker language refers to admitted Expert/host outcomes rather than implying a Manager source Tool;
- external mutations remain typed proposals behind host policy/review.

Do not put installed Expert names or source routing rules into the static role.

**docs/product/intelligence.md**

Baseline lines 5–7 say the Manager chooses evidence/expertise and may ask for evidence; line 30 already says Manager-to-Expert collaboration is delegation.

Clarify the durable product boundary: direct answers may use already-admitted context, while new domain acquisition is delegated. Experts own bounded source-backed domain judgment.

**docs/architecture/modules.md**

Baseline line 34 says first-party policy includes explicit Manager direct-read policy.

Change it to shipped trusted Expert capability declarations for standing Observe. Preserve the statement that Feasibility is separate contextual Access.

**docs/architecture/runtime.md**

Baseline lines 52–61 describe the root Manager direct Tool -> ContextToolService path. Remove that as a current canonical path.

Describe instead:

- root Manager general turn has active Expert cards and no domain source Tools;
- its domain acquisition path is Delegate -> TaskCoordinator -> Expert endpoint -> declared source read;
- generic Tool runtime remains role-neutral;
- remove statements around lines 99–103 that Manager has direct selected/current source reads or assistant standing policy.

**docs/architecture/authority-recovery.md**

Baseline line 26 describes Manager Tool catalogue ownership of remote Views and assistant standing consumers.

Replace that with shipped Expert-only first-party standing consumer derivation. Keep line 25 Feasibility contextual authority semantics intact.

**docs/product/integrations-and-privacy.md**

Baseline line 17 names Manager direct readers as product-approved first-party readers.

Remove that category. Standing Observe first-party consumers are trusted shipped Experts declaring the View capability. Keep the Feasibility distinction.

**docs/decisions/0018-manager-expert-a2a-delegation.md**

This change strengthens the durable decision, so amend the accepted ADR rather than pretending the original text never existed.

Add a dated amendment that states:

- Manager source-backed domain acquisition is not a Tool path;
- A2A delegation is the Manager-to-domain boundary;
- Experts acquire their declared domain Views under their own consumer identity;
- Manager may still receive future orchestration/presentation/product-shell capabilities, but those capabilities cannot be used as a backdoor for domain source reads or domain actions.

Do not rewrite historical migration prose merely to erase history.

Update docs/architecture/invariants.md only if the implementation team finds this rule needs a repository-wide invariant beyond the current ownership/dependency rules. Prefer the executable conformance check plus the current runtime/ownership documents over duplicating prose.

### Step 8 — residual deletion audit

Before broad verification, all of these searches must be run:

    rg -n "manager_tool_descriptors|manager_direct_remote_view|manager_direct_native_connector|ContextToolService|MANAGER_TOOL_DEFINITION_REVISION|PEOPLE_IDENTITY_READ|SCHEDULE_FEASIBILITY_READ|ATTENTION_COARSE_READ|WELLBEING_DERIVED_READ|MAIL_COMMUNICATION_READ|WORK_CONTEXT_READ|LIFE_LOGISTICS_READ" crates tools docs

Expected: no production/current-architecture match for the removed design. The active execution plan itself may name the symbols until cleanup.

    rg -n "PublishingToolPort|ToolOutcomePort" crates/app

Expected: zero.

    rg -n "Manager direct|direct-read tool|direct reader|ContextToolService" docs/architecture docs/product crates

Expected: no current prose or production code that describes Manager domain direct reads. Historical ADR text is allowed only when clearly marked historical/amended.

    rg -n "ASSISTANT_CONSUMER|GrantConsumer::builtin\(\"assistant\"\)|identifier\(\) == \"assistant\"" crates/app/src/first_party_observe.rs crates/app/src/vault_host crates/modules/context

Expected:

- zero Manager standing-Observe policy use;
- any remaining assistant identity must be traced to a real separate contextual contract such as Feasibility or another justified non-standing owner;
- no remaining match may authorize a root Manager domain Tool.

Also search direct string literals for all seven removed Tool IDs across current production source. Test/fixture literals must either prove rejection or be removed.

Every residual match must be classified in the final report as:
- required role-neutral Tool runtime;
- Expert-owned source capability;
- separate Feasibility contextual substrate;
- historical/amended ADR text;
- or a defect to delete before completion.

## 5. Expected file changes

Expected deletion:

- crates/modules/context/src/application/tools.rs

Expected production/test edits:

- crates/modules/context/src/lib.rs
- crates/app/src/vault_host/conversation_turn.rs
- crates/app/src/vault_host/conversation_turn/engine_ports.rs
- crates/app/src/vault_host/conversation_turn/interaction_publication.rs
- crates/app/src/first_party_observe.rs
- crates/app/src/vault_host/tests/conversation_flows.rs
- crates/app/src/vault_host/tests/interaction_resolution.rs
- tools/architecture/check_connection_observe_conformance.py
- tools/architecture/test_check_connection_observe_conformance.py
- crates/modules/conversation/prompts/manager_role.txt

Expected documentation edits:

- docs/product/intelligence.md
- docs/product/integrations-and-privacy.md
- docs/architecture/modules.md
- docs/architecture/runtime.md
- docs/architecture/authority-recovery.md
- docs/decisions/0018-manager-expert-a2a-delegation.md
- docs/README.md only for active-plan lifecycle

Potentially affected files discovered by residual search must be handled in the same change if they encode the removed path. Do not treat this list as permission to ignore new current callers.

No Flutter, FFI/protocol, Go server, provider credential, or native Apple change is expected. If implementation discovers that one is actually required, classify why before editing and apply the corresponding verification skill gate.

## 6. Verification

Run targeted feedback first:

    cargo test -p floe-context
    cargo test -p floe-app first_party_observe
    cargo test -p floe-app conversation
    python3 tools/architecture/test_check_connection_observe_conformance.py

Then run the architecture/Rust broad gate:

    cargo check --workspace --lib
    CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast
    python3 tools/architecture/check_boundaries.py
    python3 tools/architecture/check_connection_observe_conformance.py
    git diff --check

Run the residual searches in section 4 Step 8 after the final code/doc edits, not only before them.

If no Flutter/FFI/native source changes occur, do not add unrelated Flutter or Apple validation merely for ceremony. If such a boundary changes, follow .agents/skills/code-change-verification/SKILL.md and add the required build/tests.

## 7. Acceptance criteria

The task is complete only when all of the following hold:

1. Root Manager AllowedCatalog has no source-backed domain Tools.
2. No production ContextToolService or Manager direct Tool descriptor surface remains.
3. No root App PublishingToolPort direct-source wrapper remains.
4. Generic Tool runtime contracts remain intact.
5. A2A delegation and active Expert discovery remain intact.
6. Existing Expert-selected source paths for People, Mail, Attention, Wellbeing, Work Context, and Life Logistics still pass.
7. Standing Observe first-party consumers are exactly trusted shipped Expert consumers for the relevant View, with no assistant addition.
8. Third-party/extension/Registry state cannot widen that policy.
9. Policy digest reflects the contracted consumer set with no compatibility migration.
10. Feasibility contextual Access/native substrate remains intact but is not callable by the root Manager.
11. Expert source/model blockers still produce durable Task-origin interactions and retain safety/authority fences.
12. Current architecture/product docs and ADR 0018 describe the same boundary as the code.
13. Conformance tooling rejects reintroduction of Manager direct standing-Observe policy.
14. Residual audit is clean or every remaining match is explicitly justified by one of the categories in Step 8.
15. Required targeted and broad verification passes, or any environmental/unrelated failure is reported with exact evidence rather than worked around.

## 8. Commit discipline

Implement as one architecture convergence task. Multiple local commits are acceptable for safe iteration, but do not publish an intermediate state in which:

- Manager Tool descriptors are removed while assistant remains an intended standing reader without explanation;
- assistant policy is removed while the Manager still advertises direct domain Tools;
- old and new source paths coexist through a compatibility adapter;
- tests are weakened to tolerate both designs.

No push is authorized by this plan itself. Follow the operator's explicit instruction for remote publication.

## 9. Agent report format

At completion report:

1. start HEAD / fetched origin-main / final HEAD;
2. Manager final AllowedCatalog and ToolPort shape;
3. deleted direct Manager symbols/files;
4. final first-party consumers by People, Attention, Wellbeing, Mail, Work Context, Life Logistics, and Calendar View;
5. confirmation that Feasibility stayed a separate contextual substrate and was not reintroduced as a Manager Tool;
6. A2A/Expert source regression evidence;
7. source/model blocker interaction regression evidence;
8. conformance and residual-search results;
9. exact targeted and broad verification commands/results;
10. docs/ADR updated;
11. worktree status and any remaining blocker.

After implementation evidence is recorded and the operator accepts the architecture, retire this temporary plan and its docs/README active-plan pointer in the normal documentation cleanup; Git history is the archive.
