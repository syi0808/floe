# Manager–Expert domain acquisition boundary convergence

- **Status:** active; ready for implementation, not an implementation-completion report
- **Execution shape:** one atomic architecture task; no checkpoints
- **Planning baseline:** main at 430fd3530549c5fee94cbb13e3d698d940852bfd
- **Baseline date:** 2026-09-30
- **Primary owners:** Conversation/App composition, Context, Experts, Access
- **Required repo workflows:** AGENTS.md, .agents/skills/architecture-change/SKILL.md, .agents/skills/code-change-verification/SKILL.md

This is the sole execution plan for this task. The concurrent same-baseline planning documents have been consolidated here; do not execute a removed plan from Git history or create another checkpoint series. The planning commits change documentation only. The production-code baseline remains 430fd353.

Line numbers below are baseline locators for 430fd353, not stable identifiers. Before editing, fetch current main, record local HEAD and fetched origin/main, inspect the worktree, compare with this baseline, and re-resolve every named symbol in current source. Preserve operator work; do not reset or clean it to obtain a clean worktree. If main moved only by unrelated changes, adapt the locators without changing this contract. If the ownership model itself moved, reconcile the plan before implementing.

Read AGENTS.md, docs/README.md, the architecture-change skill, this plan, docs/architecture/README.md, invariants.md, and the relevant runtime/authority-recovery sections. Read ADR 0018 and ADR 0031 for the decisions being amended. Do not restore the retired connection-observe execution plan or recursively read historical plans.

## 1. Purpose

The current root Manager still owns seven source-backed domain Tools:

- people.identity.read
- schedule.feasibility.read
- attention.coarse.read
- wellbeing.derived.read
- mail.communication.read
- work.context.read
- life.logistics.read

That is inconsistent with the intended Manager–Expert boundary. The Manager is the single user-facing orchestrator: it understands the request, chooses an active Expert, delegates a natural-language goal through the A2A Task boundary, synthesizes admitted results, and owns the final user response. Fresh domain acquisition and delegated domain judgment belong inside admitted Expert execution, not in a parallel Manager direct-read path.

This task removes that parallel path and converges first-party standing Observe policy accordingly. It does not turn ordinary answers or synthesis into mandatory delegation.

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

1. The root Manager advertises no source-backed domain Tool descriptors. Its current production Tool catalogue is empty because presentation tools are not implemented yet.
2. The root Manager cannot directly acquire Contacts, Feasibility, Attention, Wellbeing, Mail, Work Context, or Life Logistics evidence.
3. Manager-to-Expert domain collaboration remains A2A delegation, not Tool execution.
4. Source-backed domain reads used by built-in Experts remain selected by the admitted Expert manifest and Task selection and are authorized under the exact Expert consumer.
5. Standing first-party Observe consumers are derived from trusted shipped Expert capability declarations only. The product no longer adds the assistant consumer for a Manager direct reader.
6. Registry installation, assignment, binding, or arbitrary extension state still cannot widen first-party Observe consumers.
7. Generic Agent Runtime Tool contracts stay role-neutral and remain available for Expert capabilities and future non-domain product-shell capabilities. This task does not delete ToolDescriptor, ToolPort, ModelStep::CallTool, Tool journaling, or provider Tool wire support.
8. A future Manager presentation/navigation capability is not implemented here. It may be added later as an App/client product-shell capability without reintroducing domain source access.
9. Feasibility remains a separate contextual Access/native substrate in this task. schedule.feasibility.read is removed from the Manager, but this task does not invent a new Expert source-selection model for Feasibility, redesign its permission UI/API, or delete the Apple/native implementation.
10. No compatibility shim, legacy Manager catalog, fallback direct reader, or dual standing policy is retained.
11. Source-derived Expert results and history retain current dependency reauthorization, exact-recipient model consent, and final-release fences. Removing direct acquisition does not make this evidence independent of its source.
12. Day, Connections setup/review, user-driven product reads, and native adapters retain their owners. Do not route ordinary product reads through an LLM merely because Manager domain reads are removed.

Keep existing governed Persona/Memory/conversation projection. Do not add a new User Model, client-view context, microphone or surface-generation field in this task. Do not replace the deleted tools with hidden live-source prefetch into the Manager context assembler.

Keep GrantPurpose::Assistant, message roles, and the Manager's model-processing consumer such as conversation.root. Those are not the removed assistant standing-source consumer. Do not globally replace the word assistant.

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

**Disposition:** delete this module completely. Do not move its Manager-specific descriptor/invoke layer elsewhere, hide it behind a feature flag, rename it to a generic source tool, or wrap seven source functions as pseudo-Experts.

The lower-level Context/Access acquisition functions that Experts or other real owners use remain in their existing modules. Deleting application/tools.rs must not delete source authorization, source candidate discovery, selected Expert reads, dependency reauthorization, or generic remote/native View primitives.

**crates/modules/context/src/lib.rs**

Baseline:

- line 28: application::tools module declaration.
- lines 110–115: public re-exports of ContextToolService, the seven Tool IDs, MANAGER_TOOL_DEFINITION_REVISION, manager_direct_native_connector, manager_direct_remote_view, and manager_tool_descriptors.
- lines 39–40: ASSISTANT_CONSUMER remains referenced by the separate Feasibility contextual authority today.

**Disposition:** remove the tools module declaration and every direct-Manager Tool re-export. Do not remove or rename ASSISTANT_CONSUMER merely to make this diff look cleaner; first run the residual audit after direct Tool deletion. Any remaining use must be a real non-standing contextual authority such as Feasibility, not a hidden Manager domain-read route. Keep the shared selected-source APIs and narrow only genuinely unused direct-layer imports/exports.

### 3.2 Root Manager composition

**crates/app/src/vault_host/conversation_turn.rs**

Baseline:

- lines 275–310: source-client and personal/remote/calendar dependency resolver composition.
- lines 314–328: root AllowedCatalog combines Expert cards with floe_context::manager_tool_descriptors().
- lines 387–397: root turn constructs ContextToolService.
- lines 398–409: direct Tool-only review snapshot composition.
- lines 410–419: PublishingToolPort wraps the direct Context Tool service.
- lines 455–462: ConversationPorts receives that wrapper as its ToolPort.
- lines 528–558: CompositeDependencyResolver routes current evidence reauthorization.

**Disposition:**

- Keep Expert cards from the Experts-owned Directory and their actual definition revisions.
- Set the root Manager Tool catalogue to an empty vector.
- Remove ContextToolService construction and the review-snapshot locals used only by that direct Tool wrapper.
- Keep personal/remote/calendar DependencyResolvers required for Conversation history/model coverage reauthorization. A remaining resolver is not a remaining direct source Tool.
- Keep TaskCoordinator as the Manager DelegationPort, the explicit DelegationExecutionContext binding, and the model Inference/Access path.
- Keep root model-recipient consent publication and linked-resume semantics. A Manager without domain Tools may still need consent to process Expert-derived evidence using an external recipient.
- Provide a tiny App-private fail-closed ToolPort only because ConversationPorts/EnginePorts are role-neutral and still require a ToolPort reference. Put it in conversation_turn/engine_ports.rs next to ManagerPayloadValidator unless current source has a better existing private seam.
- Name the type for what it means, for example NoManagerTools. invoke must return CapabilityDenied and perform no I/O, publication, or state mutation.
- Do not change the generic ConversationPorts or Agent Runtime contracts just to make the root Manager special. Do not introduce migration-only optional ToolPort state.

**crates/app/src/vault_host/conversation_turn/engine_ports.rs**

Baseline lines 1–20 currently contain only ManagerPayloadValidator.

**Disposition:** add the private fail-closed Manager ToolPort here if no equivalent production port already exists at execution time. The implementation should be intentionally trivial and have a regression proving an unexpected invocation is denied. It is a role-composition fence, not a new abstraction layer.

A small App-private root-catalog constructor is acceptable when both production and tests use it. Test the actual composition policy; an independently constructed empty vector in a test does not prove the production Manager catalogue is empty. Do not add a new public registry or framework for this seam.

### 3.3 Direct Tool publication wrapper versus shared blocker publication

**crates/app/src/vault_host/conversation_turn/interaction_publication.rs**

Baseline:

- lines 23–48: ToolOutcomePort exists only to adapt ContextToolService.
- lines 50–526: generic blocker labels, reviewed target construction, source/model/Expert publication helpers, and safe interaction artifacts.
- lines 530–549: blocked_tool_result builds the direct-wrapper ToolResult; its baseline production caller is PublishingToolPort.
- lines 552–633: PublishingToolPort is the product direct-Tool wrapper.

**Disposition:**

Delete:

- ToolOutcomePort and its ContextToolService implementation.
- PublishingToolPort and its ToolPort implementation.
- blocked_tool_result after removing its sole direct-wrapper caller.
- tests whose only subject is the root Manager direct Tool wrapper or its obsolete direct-read availability contract.

Preserve:

- generic_source_label;
- interaction_requirement and reviewed target construction;
- publish_model_blocker;
- publish_expert_binding_blockers;
- publish_requirements used by admitted Expert/source blocker publication;
- blocked_text and interaction_ref_artifacts where used by Expert outcomes;
- model-recipient blocker publication and Expert Task-origin interaction semantics.

Do not delete shared blocker publication simply because the root Manager Tool caller disappears. Retain generic InteractionOrigin::Tool, UserInteractionRef validation, ToolResult media validation, and journal variants that have a role-neutral contract.

Port still-valid safety assertions from direct-wrapper tests to a real admitted Expert Task or the shared owner unit seam: deterministic publication, no private payload, immutable reviewed target, capture failure, foreign/stale review, decision CAS, response loss, and linked resume. Replacing a Tool origin with an unadmitted random Task ID is not a valid port.

### 3.4 First-party standing Observe policy

**crates/app/src/first_party_observe.rs**

Baseline:

- lines 18–38: trusted_shipped_consumers derives consumers from shipped Expert manifests. This is the desired authority input and remains.
- lines 40–62: policy adds assistant when manager_direct_remote_view(view_id) is true, then adds trusted Experts.
- lines 63–116: policy_digest and member_policy_digest bind the actual permission policy.
- lines 118–141: personal_policy adds assistant when manager_direct_native_connector(connector_id) is true, then adds trusted Experts.
- lines 145–177: Calendar and remote supported connector/View policies.
- lines 185 onward: tests still encode Manager + trusted Expert consumer semantics.

**Disposition:**

- policy(view_id, categories) must derive consumers only from trusted_shipped_consumers(view_id).
- personal_policy(connector_id) must no longer add assistant. Preserve the existing contacts.android product exclusion of shipped consumers; Android delivery is out of scope.
- The Android exclusion must not become an empty-success grant or implicit new Relationships permission after Manager removal. Return the existing unsupported/unavailable failure shape for a product policy with no supported readers. Shared dormant Android source value contracts may remain.
- Keep deterministic sorting/deduplication, categories, Read operation, Assistant purpose, LocalOnly processing, and policy_digest inputs unchanged apart from the consumer-set contraction.
- Do not make Registry state an input.
- Do not add an empty-consumer fallback or wildcard.

The supported baseline reader sets are below. Names are suffixes after floe.builtin.; production must derive them from manifests, not duplicate this table in a package-name switch.

| Logical View | Trusted shipped readers |
|---|---|
| calendar.timeline | commitments, focus-attention, schedule, wellbeing |
| people.identity | relationships |
| attention.coarse | focus-attention |
| wellbeing.derived | wellbeing |
| mail.communication | commitments, communication |
| work.context | focus-attention, work-context |
| life.logistics | life-logistics |

Policy digest changes caused by removing assistant are intentional. The existing digest includes sorted consumers; do not bump its format tag or add a consumer-policy epoch. Calendar's unchanged policy should retain its digest under the same algorithm.

Test the authority consequences, not only the string set:

- an old Manager-inclusive reviewed policy digest is rejected/superseded before grant mutation, including reviewed absence;
- explicit fresh review updates permission through normal Access CAS and advances GrantAuthority when scope changes;
- exact permission/query no-op and source-only resource/subject edits keep their existing authority semantics;
- source identity and physical resources are not changed as a side effect of reader contraction;
- startup/inspection does not silently rewrite grants;
- prior history is not relabeled from assistant to an Expert consumer;
- third-party installation/binding cannot widen consumers or alter the intended policy digest.

Old local development reviews may become stale and require fresh review. Do not add a migration, compatibility decoder, or schema/protocol bump for this standing-policy contraction. Do not automatically reset existing data or key slots.

### 3.5 Built-in Expert coverage of the removed standing views

**crates/experts/builtin/src/catalog.rs**

Baseline lines 61–74 and 177–190 already declare:

- people.identity -> Relationships;
- mail.communication -> Commitments and Communication;
- attention.coarse -> Focus & Attention;
- wellbeing.derived -> Wellbeing;
- work.context -> Focus & Attention and Work Context;
- life.logistics -> Life Logistics.

No new implementation of those six domains is needed. Their existing manifest-declared selected-source paths are the final path. Preserve exact Task selection, dependency coverage, and current source fences.

Add or strengthen a small manifest/policy regression only where useful to prove the removed Manager consumer is not replaced by an implicit wildcard.

### 3.6 Feasibility exception and bounded non-goal

At baseline:

- Context direct Tool schedule.feasibility.read is a Manager-only caller.
- BuiltinExpertKind::Schedule currently declares Calendar, not schedule.feasibility, as its source requirement.
- crates/modules/context/src/application/expert_sources.rs has no LocalExpertSource::Feasibility.
- AppLocalExpertSource in conversation_turn/expert_dispatch.rs has no Feasibility branch.
- docs/architecture/modules.md and authority-recovery.md explicitly describe Feasibility as a separate contextual Access contract rather than a standing SourceConnection candidate.
- crates/modules/access/src/application/personal_grants.rs:171–195 has assistant-specific contextual review/scope checks; the WorkerAction::FeasibilityAccess composition in vault_host.rs supplies that existing consumer. These are not standing first-party policy derivation.

Therefore this task must **not** silently add schedule.feasibility to the Schedule Expert manifest. Doing that correctly would require defining its selection/admission semantics and is a separate architecture decision.

For this task:

- remove schedule.feasibility.read with the rest of the Manager direct Tool catalogue;
- keep the typed Feasibility View, contextual Access record/review, native acquisition provider, and their tests;
- ensure no root Manager catalog, ToolPort, hidden context prefetch, or fallback can invoke it;
- document that the retained substrate is not a standing first-party Observe consumer and does not provide a callable Manager read;
- preserve the existing Feasibility management API/UI and exact contextual authority checks; do not introduce the separate review/enable-to-inspect/pause API cutover in this task;
- leave future Schedule-Expert Feasibility integration for a separate task.

This is an explicit capability gap, not feature parity: six domains retain existing Expert readers; new conversational Feasibility acquisition has no replacement in this task. Report this clearly. A retained contextual grant does not create a callable root capability. Do not claim Schedule can acquire location/ETA/weather because it can read Calendar.

The distinction does not authorize weakening contextual consumer checks or deleting stored evidence. Remove only direct-layer dead scaffolding; a broader Feasibility product retirement/redesign is outside this bounded task.

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

**Disposition:** delete tests whose only contract is the removed direct Manager Tool layer. Replace fixture catalog construction with tools: vec![] when the test is really about Manager projection/history/model behavior. Remove now-unused fixtures/imports rather than retaining a test-only shadow of the deleted design. Port independent safety assertions to the remaining owner/path.

**crates/app/src/vault_host/tests/conversation_flows.rs**

Baseline around 2977:

- direct_attention_tool_blocked_completes_turn_with_one_durable_ref drives the Manager through attention.coarse.read and expects a Tool-origin review card.

**Disposition:** replace it with a negative general-turn regression that proves a domain Tool is not admitted by the Manager catalogue and cannot acquire a source or create a direct Tool-origin source-access interaction. Do not preserve the old flow merely to keep a blocker test; Expert blocker publication has its own admitted Task-origin path.

Important runtime detail: crates/runtime/agent/src/engine.rs:700–850 journals a stable ToolIntent and a soft invalid-output ToolResult for an unregistered tool, without invoking ToolPort. The correct negative assertion is zero source/native/provider acquisition, grant mutation and direct-source review publication, **not zero ToolIntent or ToolResult records**. Preserve durable correction/replay behavior. Exercise all seven removed IDs and prove the model can then delegate or answer honestly.

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

**Other projection, provenance and replay fixtures**

Inspect crates/modules/context/src/application/model_projection.rs, crates/modules/conversation/src/application/model_projection.rs, and crates/adapters/vault/src/vault/context_dependencies.rs for literals of the removed Tool IDs. Synthetic generic Tool serialization/coverage tests are not obsolete production Manager behavior: keep their semantic assertions using explicit synthetic names or an appropriate Expert case. Keep revoked-history filtering, finalization, bounded projection and recipient-consent regressions.

Inspect generic runtime/Conversation recovery tests for catalog pins. Previously validated direct-tool pins must not dispatch under the new empty root catalog. Settled results are not re-executed, mismatched durable identities still fail closed, and stored historical evidence is not silently reclassified as Expert output.

## 4. Ordered single-task execution

This is one work item. The sections below are an execution order, not checkpoints. Do not leave intermediate compatibility paths committed as completed work or stop for phase approval.

### Step 1 — establish the Manager composition boundary

1. Re-fetch origin/main and record start HEAD, fetched SHA, and pre-existing worktree changes.
2. Preserve operator work; do not overwrite it or reset the repository.
3. In conversation_turn/engine_ports.rs, add or reuse one private fail-closed ToolPort for a root Manager with an empty Tool catalogue.
4. In run_general_turn:
   - keep Directory Expert cards;
   - set AllowedCatalog.tools to vec![];
   - remove ContextToolService and PublishingToolPort construction;
   - pass the fail-closed Manager ToolPort into ConversationPorts;
   - keep A2A delegation, model projection, dependency resolvers, inference fencing, root model consent, and active Expert index unchanged.
5. Add focused regressions over the actual root catalog and denying port. Preserve a context-only/greeting answer with no unnecessary delegation, successful admitted delegation, and honest limitation when the relevant Expert is disabled/unbound/unavailable.

Do not modify generic Tool validation/runtime semantics.

### Step 2 — delete the obsolete Context direct-Tool owner surface

1. Delete crates/modules/context/src/application/tools.rs.
2. Remove application::tools and all direct Manager Tool re-exports from floe-context.
3. Compile floe-context and floe-app immediately.
4. Resolve all compile errors by moving callers to the final architecture, not by reintroducing a wrapper.
5. Delete test fixtures/imports that existed only for ContextToolService.

No replacement Manager source service is permitted. Existing selected-source APIs used by Experts and product owners remain.

### Step 3 — delete the App direct-Tool publication adapter

1. Remove ToolOutcomePort, PublishingToolPort, and the direct-only blocked_tool_result from interaction_publication.rs.
2. Preserve Task/model/source blocker publication helpers used by Expert execution.
3. Delete only direct-Tool adapter tests; port their still-relevant safety assertions.
4. Verify Expert binding/source/model blockers still publish under admitted Task origin and retain safe refs, current reviewed targets and linked-resume behavior.

### Step 4 — contract first-party Observe consumers to shipped Experts

1. Remove manager_direct_remote_view and manager_direct_native_connector use from first_party_observe.
2. Supported remote and native personal standing policies obtain consumers from trusted_shipped_consumers only.
3. Rewrite policy unit tests:
   - no assistant in Contacts/Attention/Wellbeing standing consumers;
   - no assistant in Mail/Work/Logistics remote policies;
   - Calendar behavior remains shipped-Expert-only;
   - expected shipped consumers exactly match manifest declarations;
   - arbitrary extension remains excluded;
   - Registry/binding state cannot affect policy or digest;
   - the unsupported contacts.android policy does not become a new grant or empty success.
4. Retain policy_digest canonicalization tests and add stale prior-review / explicit fresh-review CAS / source-only-change cases from section 3.4.
5. Do not alter the separate Feasibility contextual authority to mimic standing Observe.

### Step 5 — port integration tests to the final boundary

1. Remove root direct Tool tests listed in section 3.7.
2. Replace useful coverage with:
   - actual Manager catalogue has no domain Tool descriptors;
   - unexpected Manager Tool invocation cannot acquire payload, mutate grants, or publish a direct-source review;
   - correction journal records are allowed and never mistaken for dispatch;
   - A2A delegation remains the domain execution path;
   - trusted shipped Expert native and remote source reads use the package consumer and preserve dependency coverage;
   - source blockers from admitted Expert execution produce Task-origin durable interactions;
   - root and delegated model-recipient consent still operate;
   - source/grant revocation and drift fence later model/history/final-output use;
   - assistant is not a standing Observe consumer;
   - extensions remain denied by default.
3. Prefer existing Expert delegation/source tests over duplicating the same acquisition behavior in App tests.
4. Keep generic Agent Runtime Tool, replay, response-loss, cancellation, and durable pre-dispatch intent tests; they protect role-neutral behavior and are not obsolete.

No payload/provenance/authority assertion may be weakened to make the deletion compile. A random Task ID or fake grant is not a replacement for an admitted Expert fixture.

### Step 6 — add machine enforcement

**tools/architecture/check_connection_observe_conformance.py**

Baseline lines 25–42 define architecture regexes; the production scan is in check_tree below.

Extend this existing checker narrowly:

- reject manager_direct_remote_view and manager_direct_native_connector if reintroduced in production;
- reject assistant/ASSISTANT_CONSUMER injection inside crates/app/src/first_party_observe.rs;
- do not ban assistant globally because Feasibility, message roles, model consumers and purposes are distinct contracts;
- do not ban ToolPort/ToolDescriptor globally because the runtime remains role-neutral and future product-shell capabilities are valid;
- retain every existing Registry/binding/leaf-resource/source-authority rule.

**tools/architecture/test_check_connection_observe_conformance.py**

Add negative fixtures for Manager policy restoration and positive fixtures for shipped-manifest policy, GrantPurpose::Assistant, and unaffected contextual ownership. Do not weaken existing fixtures.

**tools/architecture/check_expert_extensibility.py**

Baseline lines 24–50 contain source patterns, and check_tree around 98–156 scans production source. Extend the existing deletion checks for the removed Manager catalogue/composition symbols where not already enforced by the Observe checker. Pair with test_check_expert_extensibility.py fixtures. Keep each rule in one checker rather than duplicating it; cover root direct-tool restoration as well as policy restoration.

A regex check supplements actual production-catalog/dispatch/authority tests; it does not replace them. Do not add a new general framework or permission registry.

### Step 7 — converge Manager prompt, product meaning, architecture, and ADR

**crates/modules/conversation/prompts/manager_role.txt**

Baseline lines 3–6 currently tell the Manager to select/seek evidence directly and delegate only when independent judgment would materially improve the answer.

Rewrite the role so that:

- the Manager may answer from sufficient already-admitted Conversation/Persona/Memory/product context;
- when fresh source-backed domain evidence or delegated domain judgment is required, it delegates to an active Expert;
- the Manager sends a natural-language goal/context/constraints and owns synthesis;
- source/tool blocker language refers to admitted Expert/host outcomes rather than implying a Manager source Tool;
- an unavailable/disabled/unbound Expert is a limitation, not permission for root fallback;
- external mutations remain typed proposals behind host policy/review.

Do not put installed Expert names or source routing rules into the static role. Do not mandate delegation for every turn, change the shared capability protocol to prohibit all Tools, or add live source acquisition to context assembly. Keep source-backed existing-context reauthorization.

**docs/product/intelligence.md**

Baseline lines 5–7 say the Manager chooses evidence/expertise and may ask for evidence; line 30 already says Manager-to-Expert collaboration is delegation.

Clarify the durable product boundary: direct answers may use already-admitted context, while fresh domain acquisition is delegated. Experts own bounded source-backed domain judgment. Do not present Feasibility Expert support as implemented.

**docs/architecture/modules.md**

Baseline line 34 says first-party policy includes explicit Manager direct-read policy.

Change it to trusted shipped Expert capability declarations for standing Observe. Preserve the statement that Feasibility is separate contextual Access.

**docs/architecture/runtime.md**

Baseline lines 52–61 describe the root Manager direct Tool -> ContextToolService path. Remove that as a current canonical path.

Describe instead:

- root Manager general turn has active Expert cards and no current domain source Tools;
- its domain acquisition path is Delegate -> TaskCoordinator -> Expert endpoint -> declared source read;
- generic Tool runtime remains role-neutral;
- remove statements around lines 99–103 that Manager has direct selected/current source reads or assistant standing policy;
- keep evidence/history dependency reauthorization and root/delegated model-recipient consent distinct from acquisition.

**docs/architecture/authority-recovery.md**

Baseline line 26 describes Manager Tool catalogue ownership of remote Views and assistant standing consumers.

Replace that with shipped Expert-only first-party standing consumer derivation. Keep line 25 Feasibility contextual authority semantics intact. Preserve exact source/grant epochs, CAS, provenance, final-release fencing and uncertain external-write recovery.

**docs/product/integrations-and-privacy.md**

Baseline line 17 names Manager direct readers as product-approved first-party readers.

Remove that category. Standing Observe first-party consumers are trusted shipped Experts declaring the View capability. Keep the Feasibility distinction and state its current conversational capability limitation where relevant.

**docs/decisions/0018-manager-expert-a2a-delegation.md**

This change strengthens the durable decision, so amend the accepted ADR rather than pretending the original text never existed.

Add a dated amendment that states:

- the original ADR separated Experts from Tools but did not prohibit direct Manager source Tools;
- the new Manager source-backed domain acquisition boundary is A2A delegation;
- Experts acquire their declared domain Views under their own consumer identity;
- Manager may receive future orchestration/presentation/product-shell capabilities, but not a backdoor for domain source reads or domain actions;
- existing admitted context, synthesis and model-recipient authority remain separate concerns.

Do not rewrite historical migration prose merely to erase history.

**docs/decisions/0031-connection-owned-source-scope-and-logical-observe.md**

Decision 10 at baseline lines 46–55 explicitly retains Manager direct-read policy in first-party consumers. Add a dated amendment/cross-reference to ADR 0018 removing that exception. Do not leave this newer accepted ADR contradicting updated runtime/product docs. All other Connection-owned source, logical Observe, source/grant epoch and contextual authority decisions remain unchanged.

Inspect ADR indexes and docs/architecture/README.md for any affected descriptions. Add a narrow ADR 0017 cross-reference only if needed to clarify that context assembly is not a direct-acquisition loophole. Update invariants.md only if a repository-wide rule is genuinely needed; prefer executable conformance and owner docs over duplicated policy prose.

Update canonical docs in the same implementation change set as the code, not as an early claim that this planning commit implemented the boundary. Do not revive historical connection-observe execution plans.

### Step 8 — residual deletion audit

Before broad verification, run:

    rg -n "manager_tool_descriptors|manager_direct_remote_view|manager_direct_native_connector|ContextToolService|MANAGER_TOOL_DEFINITION_REVISION|PEOPLE_IDENTITY_READ|SCHEDULE_FEASIBILITY_READ|ATTENTION_COARSE_READ|WELLBEING_DERIVED_READ|MAIL_COMMUNICATION_READ|WORK_CONTEXT_READ|LIFE_LOGISTICS_READ" crates tools docs

Expected: no production/current-architecture restoration of the removed design. The active execution plan and explicitly negative conformance fixtures may name the symbols.

    rg -n "PublishingToolPort|ToolOutcomePort|blocked_tool_result" crates/app

Expected: zero direct-wrapper implementation or caller.

    rg -n "Manager direct|direct-read tool|direct reader|ContextToolService" docs/architecture docs/product crates

Expected: no current prose or production code that describes an active Manager domain direct-read path. Historical ADR text is allowed only when clearly marked historical/amended.

    rg -n "ASSISTANT_CONSUMER|GrantConsumer::builtin\(\"assistant\"\)|identifier\(\) == \"assistant\"" crates/app/src/first_party_observe.rs crates/app/src/vault_host crates/modules/context

Expected:

- zero Manager standing-Observe policy use;
- any remaining assistant identity is traced to a real separate contextual contract such as Feasibility or another justified non-standing owner;
- no remaining match may authorize root Manager domain Tool acquisition.

Also search direct string literals for all seven removed Tool IDs across crates, apps, tools and docs. Generic serialization/coverage fixtures may use explicitly synthetic names; negative tests/checker fixtures may name the removed IDs to prove rejection. Do not delete generic runtime safety tests for grep cleanliness.

Every residual match must be classified in the final report as:

- required role-neutral Tool runtime or synthetic test;
- Expert-owned source capability;
- separate Feasibility contextual substrate;
- purpose/model consumer/message role rather than standing source consumer;
- negative conformance fixture or this temporary plan;
- historical/amended ADR text;
- or a defect to delete before completion.

An rg exit code of 1 can mean no matches, not a failed build. No new crate, public/FFI widening, optional migration state, provider-specific root source service, compatibility wrapper or dual policy is justified by this task.

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
- tools/architecture/check_expert_extensibility.py
- tools/architecture/test_check_expert_extensibility.py
- crates/modules/conversation/prompts/manager_role.txt

Possible fixture/recovery edits after the caller audit:

- crates/modules/context/src/application/model_projection.rs
- crates/modules/conversation/src/application/model_projection.rs and recovery/tests
- crates/adapters/vault/src/vault/context_dependencies.rs
- crates/runtime/agent/src/engine.rs tests, without changing role-neutral execution semantics

Expected documentation edits:

- docs/product/intelligence.md
- docs/product/integrations-and-privacy.md
- docs/architecture/modules.md
- docs/architecture/runtime.md
- docs/architecture/authority-recovery.md
- docs/decisions/0018-manager-expert-a2a-delegation.md
- docs/decisions/0031-connection-owned-source-scope-and-logical-observe.md
- docs/README.md only for active-plan lifecycle

Potentially affected files discovered by residual search must be handled in the same change if they encode the removed path. Do not treat this list as permission to ignore current callers.

No Flutter, FFI/protocol, Go server, provider credential, or native Apple implementation change is expected. This does not mean the resulting product behavior needs no validation: source policy and Manager execution change what the client observes. Do not introduce the separate Feasibility API/UI retirement or new Expert feature. If a real additional boundary change is required, state why and apply its verification gate.

## 6. Verification

Run targeted feedback first with normal incremental compilation:

    cargo test -p floe-context
    cargo test -p floe-app first_party_observe
    cargo test -p floe-app conversation
    cargo test -p floe-agent-runtime
    cargo test -p floe-conversation
    cargo test -p floe-experts
    cargo test -p floe-experts-builtin
    python3 tools/architecture/test_check_connection_observe_conformance.py
    python3 tools/architecture/test_check_expert_extensibility.py

Use narrower filters during iteration when useful; zero matched tests are not evidence. The broad gate must include all affected crate tests, including Access and Vault. Keep profiles/features/target directory consistent. Do not globally disable incremental development, create separate target directories for routine runs, or automatically cargo clean.

Then run the architecture/Rust broad gate from repository root:

    cargo check --workspace --lib
    CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
    python3 tools/architecture/test_check_boundaries.py
    python3 tools/architecture/check_boundaries.py
    python3 tools/architecture/test_check_expert_extensibility.py
    python3 tools/architecture/check_expert_extensibility.py
    python3 tools/architecture/test_check_connection_observe_conformance.py
    python3 tools/architecture/check_connection_observe_conformance.py
    cargo build -p floe-ffi
    git diff --check

Run the residual searches in section 4 Step 8 after final code/doc edits, not only before them.

Because this changes Flutter-visible Manager/source-policy behavior, verify the same-source product boundary even if no Dart or DTO source file changes:

    cd apps/client
    flutter analyze
    flutter test
    flutter build macos
    flutter test integration/product_conversation_test.dart

The product-conversation integration uses the real gateways and a same-source signed private Flutter test host. Its macOS/Foundation availability and credential-conflict prerequisites are documented in apps/client/README.md:39–61. Do not bypass bundle-relative loading, patch a shared Flutter binary, clear the saved-server slot, or reset user keys to make it run. If an environment prerequisite is unavailable, report UNAVAILABLE and its concrete cause; do not claim a pass. The greeting/model integration is not proof of all domain delegation scenarios; the deterministic owner/runtime regressions above supply those proofs.

Native implementation changes are not expected, so unrelated Swift/device suites are not required. If native code genuinely changes, run its relevant validation according to the verification skill. iOS/device testing is not required for this bounded macOS-led cutover; report it as not run. Android work/builds are out of scope. Go changes are not expected; if a shared server contract genuinely changes, run go test -race ./... and go vet ./... from server/ and explain why.

Use isolated stores or explicitly selected fresh development profiles. No external account change, real source data acquisition, Calendar write, mail send, OAuth setup, key rotation, or production data reset is authorized by this plan. Preserve uncertain external-operation records and existing user work. A stale old local review is not permission to delete the Vault.

Report exact commands and observed PASS/FAIL/UNAVAILABLE results. Distinguish a reproduced baseline golden/platform issue from a new failure; do not blanket-update goldens, weaken safety assertions, or hide unverified surfaces. Format changed code without unrelated repository-wide formatting churn.

## 7. Acceptance criteria

The task is complete only when all of the following hold:

1. Actual root Manager AllowedCatalog has no current source-backed domain Tools and preserves admitted Expert cards/revisions.
2. No production ContextToolService or Manager direct Tool descriptor surface remains.
3. No root App PublishingToolPort direct-source wrapper or direct-only blocked ToolResult builder remains.
4. Generic Tool runtime contracts, stable correction journals and replay behavior remain intact.
5. A2A delegation and active Expert discovery remain intact; context-only answers do not mechanically delegate.
6. Existing Expert-selected source paths for People, Mail, Attention, Wellbeing, Work Context, and Life Logistics still pass.
7. Supported standing Observe first-party consumers are exactly trusted shipped Expert consumers for the relevant View, with no assistant addition or unsupported empty-success fallback.
8. Third-party/extension/Registry state cannot widen that policy; Android support is not expanded.
9. Policy digest reflects the contracted consumer set; old reviews fail before mutation, explicit fresh review uses normal grant CAS, and source-only edits remain independent.
10. Feasibility contextual Access/native substrate remains intact but is not callable by the root Manager. No new Expert-backed Feasibility support or permission API/UI cutover is claimed.
11. Expert source/model blockers produce durable admitted Task-origin interactions; root model consent, history/provenance, cancellation and uncertain-write recovery retain their fences.
12. Current architecture/product docs and dated amendments to ADR 0018 and ADR 0031 describe the same boundary as code.
13. Conformance tooling rejects reintroduction of Manager direct source composition and standing-Observe policy without banning generic Tools, contextual Feasibility or purposes.
14. Residual audit is clean or every remaining match is explicitly justified by one of the categories in Step 8.
15. Required targeted and broad verification passes, or any environmental/unrelated failure is reported with exact evidence rather than worked around. Compilation alone is not architectural completion.

## 8. Commit discipline

Implement as one architecture convergence task. Multiple local commits are acceptable for safe iteration, but do not publish an intermediate state in which:

- Manager Tool descriptors are removed while assistant remains an intended standing reader;
- assistant standing policy is removed while Manager still advertises direct domain Tools;
- old and new source paths coexist through a compatibility adapter;
- tests are weakened to tolerate both designs.

There is one completion gate, not a checkpoint series. No push, PR, deployment or external-account mutation is authorized by this plan itself. Follow the operator's explicit instruction for remote publication. The planning-document commit does not authorize automatic publication of later implementation changes.

## 9. Agent report format

At completion report:

1. start HEAD / fetched origin-main / final HEAD and local commit SHAs;
2. actual Manager AllowedCatalog and ToolPort shape, including zero domain dispatch for all seven rejected IDs;
3. deleted direct Manager symbols/files and direct-only publication/tests;
4. final supported first-party consumers by People, Attention, Wellbeing, Mail, Work Context, Life Logistics and Calendar View, plus policy-digest/CAS evidence;
5. confirmation that Feasibility stayed a separate contextual substrate, its management contract stayed unchanged, and conversational Expert-backed Feasibility acquisition is not provided;
6. A2A/Expert source regression evidence and no-Expert/no-fallback behavior;
7. source/model blocker, provenance, replay and response-loss regression evidence;
8. conformance and residual-search results with intentional exceptions;
9. exact targeted/broad Rust and product-boundary verification commands/results, separately listing baseline failures and unavailable prerequisites;
10. docs/ADR updated;
11. worktree status and any remaining blocker.

Record the execution report in section 10 of this document, not a second progress ledger. After the operator accepts the architecture, retire this temporary plan and its docs/README active-plan pointer in the normal documentation cleanup; Git history is the archive.

## 10. Execution evidence

Not started. Planning and document consolidation do not constitute implementation or Rust/Flutter test execution. Replace this paragraph with observed implementation and verification evidence when the single task is executed.
