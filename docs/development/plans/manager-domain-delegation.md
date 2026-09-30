# Manager domain acquisition through Expert delegation

- Status: ready for implementation; this document does not report an implemented change.
- Date: 2026-09-30.
- Repository: `syi0808/floe`.
- Planning baseline: `430fd3530549c5fee94cbb13e3d698d940852bfd` (`main`).
- Baseline source: https://github.com/syi0808/floe/tree/430fd3530549c5fee94cbb13e3d698d940852bfd
- Execution shape: one bounded architecture-convergence task, with one completion gate. The numbered sections below are instructions, not checkpoints. Do not create checkpoint documents, stop for intermediate approval, or start a parallel plan.
- Scope: remove the Manager's live domain-source tools and their product authorization/publication paths; preserve admitted Expert execution, governed context/history, and user-driven product reads.
- Non-goal: implementing client presentation tools, voice, a new Expert, or a new Feasibility Expert capability.

## 1. Read order and baseline discipline

Read `AGENTS.md`, `docs/README.md`, `.agents/skills/architecture-change/SKILL.md`, this plan, `docs/architecture/README.md`, `docs/architecture/invariants.md`, and the relevant sections of `runtime.md` and `authority-recovery.md`. Use `.agents/skills/code-change-verification/SKILL.md` for verification. Read ADR 0018 and ADR 0031 for the two decisions this task amends; do not recursively read historical plans.

The planning baseline has already removed `docs/development/plans/connection-observe-authority/`. Do not restore it. This file is the only execution plan for this task. Do not update root `AGENTS.md` with transient task status or claim the target architecture is already implemented.

Before implementation:

```sh
git status --short
git rev-parse HEAD
git fetch origin
git rev-parse origin/main
git diff --name-status 430fd3530549c5fee94cbb13e3d698d940852bfd..origin/main
```

Record local HEAD, fetched `origin/main`, and pre-existing changes. Preserve user work. Do not hard-reset, clean a worktree, or overwrite concurrent changes. If main advanced, inspect the changed owners and refresh the anchors in this plan before editing; do not implement against an obsolete checkout or blindly apply line offsets. The plan-document commit is not a production-code change.

Line ranges below are inclusive navigation anchors at the planning baseline. Symbols and ownership are the edit contract. Use `nl -ba`/`rg -n` on the execution checkout for precise current positions. Search results may lag branch indexing; load actual files at the fetched commit when resolving a discrepancy.

## 2. Target invariant and explicit exclusions

The final agent path is:

```text
User request + admitted conversation/personal context
  -> user-facing Manager
  -> active Expert discovery + natural-language Delegate
  -> TaskCoordinator / admitted Expert Task / retained endpoint
  -> Expert-owned declared source acquisition and domain judgment
  -> bounded Task result + artifacts + exact internal coverage
  -> Manager synthesis and user-facing answer
```

Manager owns conversation, orchestration, delegation, synthesis, and potentially separately implemented product presentation capabilities. It does not directly acquire domain data or execute domain actions. In this task the production Manager tool catalog is empty, because no product-presentation tool exists yet. Do not create a placeholder presentation tool or a general-purpose tool registry to make the empty catalog look extensible.

The following seven Manager tools are removed, not renamed, hidden behind a feature flag, exposed through another generic source tool, or wrapped as seven pseudo-Experts:

```text
people.identity.read
schedule.feasibility.read
attention.coarse.read
wellbeing.derived.read
mail.communication.read
work.context.read
life.logistics.read
```

This is a Manager boundary, not a ban on every product read. Day, Connections setup/review, user-driven UI reads, native acquisition adapters, and authorized Expert reads retain their actual owners. Do not force a Day refresh or a settings inspection through an LLM or A2A.

Preserve existing governed Persona/Memory/conversation projection and Expert discovery. The current `AgentContext` value contract and history dependency reauthorization remain valid. Do not add new User Model, client-view, microphone, or surface-generation fields in this task. Do not turn context assembly into a hidden replacement for the deleted live Manager reads.

Preserve generic `ToolDescriptor`, `AllowedCatalog.tools`, `ModelStep::CallTool`, `ToolPort`, Tool journals/replay, and provider function-call normalization. The Agent Runtime remains role-neutral. Expert delegation remains a distinct `Delegate` operation, not `expert.*` tools or a new `delegate` tool wrapper.

An ordinary answer using sufficient, still-authorized existing context does not require a gratuitous delegation. Fresh domain evidence must come from an eligible Expert. With no appropriate enabled/bound Expert, return an honest limitation; never fall back to a root source reader.

## 3. Baseline findings that change the deletion scope

### 3.1 Existing code intentionally publishes direct reads

`crates/modules/context/src/application/tools.rs` defines seven descriptors, schemas, and a `ContextToolService`. `run_general_turn` composes that service with `PublishingToolPort` and publishes its descriptors beside the Expert cards. Removing only the descriptors leaves an obsolete production implementation and first-party policy derivation behind.

### 3.2 Source consumers are not purposes or model recipients

`crates/app/src/first_party_observe.rs` currently adds the `assistant` source consumer both in remote `policy` and native `personal_policy`. Its tests explicitly expect that exception. Remove this derivation, not every occurrence of the word assistant.

Keep `GrantPurpose::Assistant`, message roles, user-facing terminology, and the Manager's Inference consumer/recipient-consent path such as `conversation.root`. Expert results still need authorized model processing when the Manager synthesizes them. Removing direct source tools does not remove model-recipient consent.

### 3.3 Feasibility has no existing Expert replacement

At this baseline, `crates/experts/builtin/src/catalog.rs` declares Calendar for Schedule, Contacts for Relationships, Attention for FocusAttention, and the other existing source requirements. There is no `BuiltinContextSource::Feasibility` or shipped `schedule.feasibility` source requirement. `read_feasibility[_outcome]` is called by the Manager tool; Access review helpers and the worker currently pin that grant to `assistant`.

Therefore six removed read capabilities already have appropriate shipped Expert consumers; Feasibility does not. Do not claim feature parity for it. The bounded disposition in this task is to retire its Manager-only agent read and new Manager grant activation, retain native/query/storage foundations and read/disable management of existing grants, and report that new conversational Feasibility acquisition is unavailable. Adding an Expert-bound contextual acquisition contract is a separate feature, not a compatibility exception in this task.

### 3.4 Empty-reader policies need deliberate treatment

`personal_policy("contacts.android")` currently omits shipped Expert consumers and depends on the Manager exception. Removing the exception must not start granting Android access to Relationships merely because the View ID matches, or report a successful usable policy with no readers. Reject that unsupported product policy through the existing unavailable/error shape. Preserve dormant Android source value contracts; do not implement or validate Android parity.

### 3.5 A rejected tool may still have a journal entry

`crates/runtime/agent/src/engine.rs:700-850` records stable Tool intent/result identities for invalid/unregistered tools and generates a soft invalid-output observation without invoking `ToolPort`. The negative invariant is **zero domain dispatch/payload acquisition/grant mutation**, not zero ToolIntent or zero ToolResult rows. Preserve this recovery behavior.

## 4. Code-line map

| Baseline file / line anchor | Symbol or region | Required disposition |
|---|---|---|
| `crates/app/src/vault_host/conversation_turn.rs:275-472` | `run_general_turn`, root `AllowedCatalog`, `tool_service`, `review_snapshots`, `publishing_tools`, `ConversationPorts` | Empty root tools; remove Manager direct-read/publication composition; keep model, delegation, and dependency authority. |
| Same file `:275-310` and `:528-558` | personal/remote/calendar resolvers and `CompositeDependencyResolver` | Keep. These reauthorize Expert-derived current/history evidence, not just direct Tool results. |
| `crates/app/src/vault_host/conversation_turn/engine_ports.rs:1-20` | `ManagerPayloadValidator` | Keep; colocate a minimal App-private denying Manager ToolPort and, if useful for production-path tests, the actual root catalog constructor. |
| `crates/modules/context/src/application/tools.rs:1-185` and the remainder of the file | seven constants/descriptors, `MANAGER_TOOLS`, `manager_direct_*`, `ContextToolService`, input decoders and direct dispatch | Delete the Manager-only file and its direct-only tests. Shared selected-read owners live elsewhere and remain. |
| `crates/modules/context/src/lib.rs:1-40`, `:84-94`, `:110-120` | `pub mod tools`, `ASSISTANT_CONSUMER`, direct-read reexports | Remove obsolete publication. Audit Feasibility callers before removing its exports; retain shared Context APIs. |
| `crates/app/src/vault_host/conversation_turn/interaction_publication.rs:1-48` | `ToolOutcomePort` and `ContextToolService` implementation | Delete. |
| Same file `:262-526` | `publish_model_blocker`, `publish_expert_binding_blockers`, `publish_requirements`, safe-ref/text helpers | Keep the Expert-used functions and their authority semantics. |
| Same file `:530-640` | `blocked_tool_result`, `PublishingToolPort` | Delete direct-only result wrapper and port; narrow imports/visibility after call-site audit. |
| `crates/app/src/first_party_observe.rs:18-61`, `:118-177` | `trusted_shipped_consumers`, `policy`, `personal_policy`, `remote_policies` | Derive supported first-party readers only from trusted shipped declarations. Preserve connector/View mapping and processing rules. |
| Same file `:63-116`, tests from `:179` | `policy_digest`, `member_policy_digest`, policy/digest tests | Preserve digest format/semantic binding; update consumer expectations and add old-review mismatch tests. |
| `crates/app/src/personal_source_spec.rs:16-53` | Apple and dormant Android connector cases | Do not broaden platform support. Policy rejection is not deletion of the source value contract. |
| `crates/experts/builtin/src/catalog.rs:61-83`, `:161-210` | `required_sources`, source capability mapping | Verify real replacement consumers; do not add Feasibility or change existing Expert scope opportunistically. |
| `crates/modules/access/src/application/personal_grants.rs:22-45`, `:171-195`, `:226-270` and `apply_feasibility` below | consumer constant, Feasibility command/configuration, assistant-only grant creation | Retire Manager-specific new grant activation; retain read/disable and stored authority semantics as detailed in section 8. |
| `apps/client/lib/features/settings/domain/feasibility_access.dart:1-12`, `:44-156` | `FeasibilityAccessGateway` and overview | Narrow review/enable to inspect/pause; preserve truthful stored-grant inspection. |
| `crates/runtime/agent/src/engine.rs:700-900` | `execute_tool` and `execute_delegation` | Preserve generic behavior; add/retain no-dispatch and replay regression coverage. |
| `tools/architecture/check_expert_extensibility.py:24-50`, `:98-156` | deletion patterns / production traversal | Extend existing source checks for removed Manager path; do not ban generic ToolPort or Expert reads. |
| `tools/architecture/check_connection_observe_conformance.py:25-43`, `:69-111` | policy rules / production scan | Reject Manager direct-reader policy restoration; retain all existing source/leaf/Registry rules. |
| `docs/decisions/0031-connection-owned-source-scope-and-logical-observe.md:46-55` | decision 10 and contextual authority distinction | Amend direct-reader exception; distinguish retained Feasibility authority foundation from current agent availability. |

For large test, projection and outer-boundary files, use these exact symbol locators to obtain their current code lines rather than guessed offsets:

```sh
rg -n 'manager_tool_descriptors|ContextToolService|PublishingToolPort|ToolOutcomePort|blocked_tool_result' crates
rg -n 'read_feasibility|ASSISTANT_CONSUMER|ATTENTION_ASSISTANT_CONSUMER|reviewed_feasibility_consumers|feasibility_scope' crates
rg -n 'WorkerAction::FeasibilityAccess|FeasibilityAccessConfiguration|FeasibilityAccessChange' crates/app crates/modules/access
rg -n 'FeasibilityAccessChangeDto|feasibility_access_change|AccessFeasibilityConfigure' crates/bindings
rg -n 'reviewFeasibility|setFeasibilityEnabled|FeasibilityAccessGateway|FeasibilityAccessCard' apps/client/lib apps/client/test
rg -n 'people\.identity\.read|schedule\.feasibility\.read|attention\.coarse\.read|wellbeing\.derived\.read|mail\.communication\.read|work\.context\.read|life\.logistics\.read' crates apps tools
```

Known additional files reached by these locators include:

- `crates/app/src/vault_host/tests/interaction_resolution.rs`;
- `crates/modules/context/src/application/model_projection.rs` and `personal_sources.rs`;
- `crates/modules/conversation/src/application/model_projection.rs`, `recovery.rs`, and `tests.rs`;
- `crates/adapters/vault/src/vault/context_dependencies.rs`;
- `crates/app/src/vault_host.rs` and `local_access_services.rs`;
- `crates/bindings/protocol/src/dto/agent.rs`, `commands.rs`, and `crates/bindings/ffi/src/conversion/owners.rs`;
- `apps/client/lib/app/runtime/local_owner_gateways.dart`;
- `apps/client/lib/features/settings/domain/feasibility_access.dart`;
- `apps/client/lib/features/connections/presentation/feasibility_access_card.dart`;
- `apps/client/test/features/settings/feasibility_access_gateway_test.dart`.

## 5. Change the root composition, not the generic runtime

In `run_general_turn`:

1. Keep the principal-admitted Directory catalog and its actual Agent definition revisions. Keep its revision semantics and `DelegationExecutionContext` binding.
2. Set production Manager tools to `Vec::new()`. Do not filter a populated Manager source catalog by model route, persona, consent, or platform; delete that catalog.
3. Remove construction of `ContextToolService` and `PublishingToolPort` and the root-only `calendar_subject`, `personal_subject`, and `HostReviewSnapshots` values whose sole caller was that port.
4. Supply a zero-state App-private ToolPort that always returns `AgentFailure::CapabilityDenied` without touching a source, Vault, native broker, or provider. This implements the existing required generic port and enforces a real role boundary; it is not a compatibility adapter. Do not change `ConversationPorts.tools` into migration-only optional state.
5. A small App-private root-catalog constructor is acceptable when used by production composition and its tests to establish this policy in one place. Do not create a second test-only catalog that merely asserts an independently built empty vector.
6. Keep `ServerSourceClient` / `RemoteDependencyResolver`, personal and Calendar dependency resolvers, `ContextualRecipientAuthority`, model projection, Inference, and `TaskCoordinator`. Do not delete the entire remote composition because one direct reader was removed.
7. Keep root consent publication, linked-resume admission and model-attempt evidence. A Manager with no source tools can still be blocked from sending Expert-derived content to a model recipient.

In `crates/modules/context/src/lib.rs`, remove the `tools` module declaration and direct-tool reexports; delete `application/tools.rs`. Do not move its seven descriptors to App or to a generic source-call escape hatch.

Keep `read_selected_people_outcome`, `admit_selected_attention_outcome`, `read_selected_wellbeing_outcome`, `read_declared_source`, selected remote reads, Calendar acquisition, connection source validation, and all shared provenance/liveness checks required by Experts/product reads. Any visibility narrowing follows the resulting caller graph, not a bulk edit.

## 6. Preserve Expert blockers while deleting root-tool publication

`interaction_publication.rs` is not wholly obsolete. Remove the direct-tool trait, its concrete Context implementation, `PublishingToolPort`, and its direct-only blocked ToolResult wrapper. Keep host-authored requirement publication used by the Expert endpoint and model host, including immutable reviewed targets, snapshot capture, navigation-only fallback, exact consumer, Task origin, and safe opaque refs.

Update module comments so they describe the remaining Expert/owner publication path, not an active Manager source Tool path. Do not delete `InteractionOrigin::Tool`, generic `UserInteractionRef`, ToolResult media validation, or generic Conversation journal variants just to remove this one product caller.

Tests must distinguish two categories:

- Delete tests whose only invariant is that a removed Manager tool dispatches, offers source access, or inherits source availability on different model routes.
- Port tests for deterministic publication, safe refs/no payload, capture failure, stale/foreign review, blocked source/model dispatch, decision CAS, response loss, and linked resume onto a real admitted Expert Task or the appropriate shared owner unit seam. Merely changing a Tool origin to a random Task UUID is not a valid replacement.

Retain `publish_model_blocker`, `publish_expert_binding_blockers`, `publish_requirements`, and Expert-used text/artifact helpers. Preserve the current root model-consent path outside the removed direct source wrapper.

## 7. Converge first-party Observe policy

Keep `trusted_shipped_consumers(capability)` as the source of trusted reader identity. Both supported remote `policy` and Apple `personal_policy` use exactly that sorted, deduplicated set. Remove calls to `manager_direct_remote_view`, `manager_direct_native_connector`, and the branches that append `ASSISTANT_CONSUMER`.

At the planning baseline, the expected supported sets are:

| Logical View | Trusted shipped consumers, package suffixes after `floe.builtin.` |
|---|---|
| `calendar.timeline` | `commitments`, `focus-attention`, `schedule`, `wellbeing` |
| `people.identity` | `relationships` |
| `attention.coarse` | `focus-attention` |
| `wellbeing.derived` | `wellbeing` |
| `mail.communication` | `commitments`, `communication` |
| `work.context` | `focus-attention`, `work-context` |
| `life.logistics` | `life-logistics` |

This table is a test expectation at the baseline, not a new production policy registry. Derive production sets from manifests, not from duplicated package-name switches. Do not use installed assignments, active bindings, selected leaf resources, or a third-party installation to compute default source permissions.

Preserve connector/View mapping, categories, Read operation, `GrantPurpose::Assistant`, and LocalOnly restrictions. Do not accidentally convert a source grant into approval for an external model recipient. An unsupported/empty-reader policy fails closed; no wildcard, Manager fallback, successful empty grant, or unrequested Android support is introduced.

`policy_digest` already includes sorted consumers. Removing a consumer naturally changes affected digests; do not bump its format tag, add a consumer-policy epoch, or hard-code replacement hash strings. Calendar's unchanged permission set should retain its digest under the same algorithm.

Verify policy and state effects:

- A fresh supported grant contains exactly the appropriate shipped Expert consumers and no Manager `assistant` consumer.
- Old review expectations carrying the previous consumer-policy digest are rejected/superseded before mutation, including reviewed absence.
- Explicit fresh review can replace an existing permission scope through normal Access CAS and advance `GrantAuthority` when its scope changes. Source identity and current resources do not change as a side effect.
- Exact scope/policy no-op remains a no-op. Source-only resource/subject edits retain their established source/grant separation.
- Startup/read inspection never silently rewrites, regrants, or upgrades old grants. This task does not add a migration worker.
- Existing stored history may mention `assistant`; it is not rewritten into an Expert identity. Later evidence/model reuse still follows current authority and coverage.
- Third-party installation/binding cannot widen first-party consumers or change policy digest.

## 8. Retire Manager-only Feasibility activation without inventing a replacement

This is a deletion consequence inside the same task, not an Expert feature project.

### 8.1 Agent acquisition

Audit `read_feasibility` and `read_feasibility_outcome` in `crates/modules/context/src/application/personal_sources.rs` and their reexports. The planning-baseline production caller is the deleted Manager tool. Remove those now-unreachable Manager agent-acquisition entry points and their direct-only test scaffolding. Preserve native Feasibility calculation, typed query/lineage values, source provenance foundations, and safe reauthorization/inspection of stored evidence where still used. Do not broaden a retained consumer-equality check when removing a shared Manager constant: retain exact stored-consumer validation locally, or fail closed for an unsupported dependency. A renamed constant is not permission to retain an acquisition path.

Do not add a `Schedule -> Manager -> feasibility` callback, declare a fake selected source, treat Feasibility as a standing connection, or silently grant its data to Schedule. Schedule currently declares Calendar; that declaration does not authorize location/ETA/weather acquisition.

### 8.2 Product grant activation

Trace this concrete chain:

```text
FeasibilityAccessCard / FeasibilityAccessGateway
  -> NativeFeasibilityAccessGateway
  -> access.feasibility.configure / FeasibilityAccessChangeDto
  -> conversion::feasibility_access_change
  -> LocalAccessCommand / WorkerAction::FeasibilityAccess
  -> apply_feasibility_access
  -> assistant-only reviewed_feasibility_consumers / feasibility_scope
```

Remove the new-review/enable operation that exists solely to grant the deleted Manager capability. The final supported management behavior is inspection and pausing an existing grant; do not remove the user's ability to stop access or erase stored review/authority records.

Narrow the Feasibility owner request, same-snapshot wire DTO, conversion and Flutter gateway accordingly. Prefer `Inspect` and an explicit pause operation to a boolean that still appears to offer re-enable. Remove the unused requested-consumers field and the worker's forced `vec![ASSISTANT_CONSUMER]`. Remove assistant-only new-grant scope builders when they have no remaining supported caller. Do not retain a permanent `Review` variant that silently succeeds without a reader, manufacture an empty scope, or add a fallback consumer.

Affected anchors: `personal_grants.rs:22-45`, `:171-195` and `apply_feasibility`; `vault_host.rs`'s `WorkerAction::FeasibilityAccess`; `local_access_services.rs`; `dto/agent.rs`'s `FeasibilityAccessChangeDto`; `conversion/owners.rs`'s `feasibility_access_change`; `local_owner_gateways.dart`'s `NativeFeasibilityAccessGateway`; and the settings Feasibility domain/gateway tests. Follow the section 4 locators to every in-scope caller.

The change narrows a local development wire contract: update all same-snapshot callers and negative decoding tests together. Do not add v3/next DTOs, old decoders or a compatibility branch. Preserve storage records and uncertain external-operation history. No automatic database/key reset is authorized.

In `FeasibilityAccessCard`, remove new-query/review/OS-permission solicitation for a now-absent agent feature. Present the unsupported conversational capability honestly and keep only relevant existing-grant inspection/pause. Remove callback plumbing that becomes unused, but do not redesign Connections navigation or delete the native Feasibility/Weather/location packages. Keep generic native/query tests that are independent of Manager access.

Acceptance: a fresh profile cannot create a Manager Feasibility grant from UI or AppWire, malformed/removed activation requests do not perform native or source I/O, and an existing grant can still be inspected and paused through current authority checks. The final report explicitly states the capability gap; it must not say all seven reads were transferred to Experts.

## 9. Role guidance and context projection

Update `crates/modules/conversation/prompts/manager_role.txt` and relevant prompt tests:

- Manager selects Expert expertise when fresh domain evidence/judgment is needed, delegates a natural-language goal, and owns synthesis.
- Existing sufficient governed context can support a direct conversational answer.
- Unavailable/disabled/unbound Experts are limitations, not permission to acquire their sources at the root.
- Host-produced blockers are explained without inventing evidence, consent, or approval.
- External changes remain reviewable domain proposals from their proper owner; neither an answer nor presentation grants Act authority.

Keep the prompt small. Do not enumerate all installed Experts, hard-code routing keywords, mandate an Expert call for every turn, or introduce workflow scripts into the stable prompt.

Keep the shared capability protocol role-neutral. Do not change it to "never call any tool" globally. Generic Expert/tools/runtime fixtures may still exercise Tools.

Review `crates/modules/context/src/application/model_projection.rs` and `crates/modules/conversation/src/application/model_projection.rs`. Replace fixtures that accidentally advertise the removed tools as the current Manager catalog. Generic serialization/coverage tests can use explicitly synthetic capability names or an appropriate Expert case. Preserve their assertions about filtering revoked history, bounded projection, source coverage, data classes, exact recipient, and finalization.

The root may still carry resolver objects and authorize source-derived Task artifacts/history. Presence of a resolver is not evidence that Manager direct acquisition remains. Conversely, do not inject newly read payload into `agent_context.evidence` during root assembly as a replacement for the deleted tools.

## 10. Regression requirements and test migration

Add or update focused tests in the actual owner suites, not a second unconnected test architecture.

| Contract | Required proof |
|---|---|
| Production root catalog | Actual catalog constructor/composition has zero tools, preserves admitted Expert cards/revisions, and does not vary source-tool exposure by local/remote profile. |
| Defense in depth | App-private Manager ToolPort denies every call without I/O, even if invoked directly by a test. |
| Invalid root source call | Script each removed ID against an empty root catalog. The Engine may journal its soft invalid-output observation; source/native/provider counters and grant mutations remain zero. The model can subsequently delegate or answer honestly. |
| Normal conversation | A greeting/context-only answer works with no Expert dispatch; do not turn orchestration into mandatory delegation. |
| Expert success | Representative native personal and remote source reads run under the actual admitted Expert consumer and settle bounded Task results that Manager can synthesize. Retain existing full Expert suite. |
| Expert unavailable | Missing/disabled/unbound appropriate Expert produces honest limitation/binding flow, with no root direct-source fallback. |
| Source blockers | An admitted Expert source denial publishes a safe Task-origin interaction; resolving/reviewing it preserves CAS, immutable target and linked-resume behavior. |
| Model blockers | Root and delegated exact-recipient consent still work. No source grant or removed Manager tool is used to approve a model recipient. |
| Provenance | Revocation, source drift, physical-resource drift and current grant checks still fence Expert results/history before model dispatch and final release. |
| Replay | Previously validated direct-tool pins cannot dispatch after catalog removal. Settled results are not re-executed, mismatched identities fail closed, and old source history is never relabeled as Expert evidence. |
| First-party policies | Exact manifest-derived reader sets; no Manager/wildcard/third-party default grants; digest matches the policy used for activation. |
| Policy change | Prior Manager-inclusive review digest is stale before mutation; explicit fresh review advances only the appropriate grant authority; ordinary source edits remain independent. |
| Feasibility retirement | No new Manager query grant/re-enable; existing inspection/pause remains; native substrate tests remain; no fake Expert transfer. |
| Unsupported Android policy | No fallback or empty-success policy and no newly authorized shipped reader; shared Rust contract check only, not Android delivery work. |

Known direct-test concentration is in `crates/app/src/vault_host/conversation_turn.rs`, `conversation_turn/interaction_publication.rs`, and `vault_host/tests/interaction_resolution.rs`. Do not delete these whole files. Delete direct-read-only assertions; move still-valid safety assertions to their actual remaining producer.

Also inspect `crates/adapters/vault/src/vault/context_dependencies.rs`: a synthetic failed-tool fixture there tests coverage semantics, not necessarily production Manager availability. Preserve the test's semantic purpose, using a clearly synthetic name if needed. Do not remove generic capability/replay coverage simply to make a residual grep return zero.

Do not replace assertions like "no private payload", "exact consumer", "source drift rejected", "no duplicate effect", or "review requires current digest" with weaker existence/success checks. A failed test that depended on the old architecture needs an honest new owner/path, not an ignored test.

## 11. Mechanical conformance and deletion gate

Extend the existing Expert checker and its `test_check_expert_extensibility.py` fixtures to reject restoration of production Manager direct-tool registration/composition. Cover at least `ContextToolService`, `manager_tool_descriptors`, `manager_direct_*`, `PublishingToolPort`, and a concrete root source-tool descriptor regression. Keep tests/comments and synthetic role-neutral test tools appropriately scoped; do not ban those strings everywhere in the repo.

Extend the existing Observe checker and `test_check_connection_observe_conformance.py` to reject an explicit Manager consumer append/exception in first-party policy. Keep its Registry/binding/leaf-resource rules. Positive and negative fixture tests must prove both the new rule and its lack of false positives for `GrantPurpose::Assistant`, generic roles, and supported Expert policy.

Do not treat a regex checker as a substitute for runtime catalog/dispatch/authority tests. Do not create a new framework, global state machine, or global permission registry for this task.

Run residual searches after implementation:

```sh
rg -n 'ContextToolService|manager_tool_descriptors|manager_direct_remote_view|manager_direct_native_connector|PublishingToolPort|ToolOutcomePort|blocked_tool_result|MANAGER_TOOL_DEFINITION_REVISION' crates apps tools docs
rg -n 'people\.identity\.read|schedule\.feasibility\.read|attention\.coarse\.read|wellbeing\.derived\.read|mail\.communication\.read|work\.context\.read|life\.logistics\.read' crates apps tools docs
rg -n 'ASSISTANT_CONSUMER|ATTENTION_ASSISTANT_CONSUMER|reviewed_feasibility_consumers|feasibility_scope' crates apps
rg -n 'Manager direct|Manager.*direct.read|direct.*Manager|manager.*source.*tool' docs crates apps tools
rg -n 'assistant' crates/app/src/first_party_observe.rs crates/modules/access/src/application/personal_grants.rs
rg -n 'Review|SetEnabled|reviewFeasibility|setFeasibilityEnabled' crates/modules/access/src/application/personal_grants.rs crates/bindings/protocol/src/dto/agent.rs apps/client/lib/features/settings/domain/feasibility_access.dart apps/client/lib/features/connections/presentation/feasibility_access_card.dart
```

`rg` returning 1 means no matches, not a failed build gate. Classify every match. Allowed examples are this plan, explicit dated ADR history, negative conformance fixtures, generic synthetic tests, message roles/purposes, and retained historical inspection/reauthorization with a documented caller. Current production direct acquisition, first-party Manager reader derivation, source payload prefetch into Manager context, and new Manager-only Feasibility activation are not allowed residuals.

No new crate, internal compatibility facade, optional migration field, wire version fork, source-to-Manager fallback, or duplicate source policy remains at completion. Trim now-unused imports and public exports. Leave unrelated formatting and dormant platform code alone.

## 12. Canonical documentation changes during implementation

Update these documents only when the corresponding production cutover lands in the same implementation change set:

| Document | Change |
|---|---|
| `docs/architecture/runtime.md` | Replace Manager ContextToolService path with delegation-only domain acquisition; describe empty current Manager tools and retained provenance/model-consent paths. Keep generic Tool runtime semantics distinct. |
| `docs/architecture/modules.md` | Remove explicit Manager direct-reader contribution; App composes trusted shipped Expert source consumers. |
| `docs/architecture/authority-recovery.md` | Remove source-grant Manager exception; preserve purposes, recipient consent, authority epochs and recovery. State Feasibility's current agent limitation without discarding its native/stored authority foundations. |
| `docs/product/integrations-and-privacy.md` | Connected first-party domain readers are admitted Experts, not Manager direct readers; do not promise conversational Feasibility availability. |
| `docs/product/intelligence.md` | Clarify orchestration versus domain acquisition only where existing prose implies direct Manager reads. |
| `docs/decisions/0018-manager-expert-a2a-delegation.md` | Add a dated amendment: its original separation of Experts from Tools did not prohibit Manager reads; the new boundary does. Preserve original rationale/history and A2A-aligned terminology. |
| `docs/decisions/0031-connection-owned-source-scope-and-logical-observe.md` | Amend decision 10's Manager exception and cross-link the ADR 0018 amendment; other Connection/Observe semantics remain. |
| `docs/decisions/0017-agent-context-assembly.md` | Add a narrow cross-reference if necessary: governed context is not a direct-acquisition loophole. Do not rewrite the whole context design or claim unimplemented fields exist. |
| `docs/architecture/README.md` | Update checker descriptions if their enforced scope expands; no progress table. |
| Relevant client/Feasibility documentation | Distinguish native capability foundations from unsupported conversational acquisition/new activation. |

Inspect ADR indexes/references for accurate descriptions, but do not renumber ADRs or create a second competing decision just to track progress. Do not bulk rewrite historical evidence. Do not restore deleted execution plans.

Append execution evidence to section 15 of this one plan. Once implementation is accepted and durable facts are in canonical documents, Git history is the archive; no permanent migration board or second handoff document is required.

## 13. Verification commands and environments

First run affected crates with normal incremental compilation:

```sh
cargo test -p floe-context
cargo test -p floe-app
cargo test -p floe-access
cargo test -p floe-agent-runtime
cargo test -p floe-conversation
cargo test -p floe-experts
cargo test -p floe-experts-builtin
cargo test -p floe-vault
```

Use targeted test-name filters during iteration, but a filter matching zero tests is not evidence. Run the full affected crate before completion. Keep profiles/features/target directory consistent; do not globally disable incremental development or create a second target directory for routine validation.

Required final Rust/architecture gate, from repository root:

```sh
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
```

Required client/macOS gate because source-policy presentation and Feasibility management cross AppWire:

```sh
cd apps/client
flutter analyze
flutter test
flutter build macos
flutter test integration/product_conversation_test.dart
```

The product-conversation integration has explicit macOS/Foundation availability and signing prerequisites described in `apps/client/README.md:39-61`. Use the repository's same-source signed test-host mechanism. Do not bypass bundle-relative native loading, patch shared Flutter binaries, erase a conflicting saved-server credential, or claim the test passed when a prerequisite was unavailable. This integration alone does not prove all domain delegation cases; the deterministic Rust/Flutter owner tests above supply that proof.

Run relevant native tests if native implementation changes are actually required. Native package redesign is not part of this task. iOS/device validation is not required for this bounded macOS-led cutover; report it as not run, not passed. Android is out of scope. Go production changes are not expected; if a shared fixture or server contract genuinely changes, additionally run `go test -race ./...` and `go vet ./...` in `server/` and explain why it was necessary.

Use isolated test stores or explicitly selected fresh Floe development profiles. No real Calendar writes, mail sends, contacts reads, OAuth setup, key rotation, or external account mutation is authorized by this plan. Do not reset user data to make a test pass. Stored old grants/uncertain actions are not disposable fixture data unless specifically identified as validation-owned.

Record exact commands, observed counts, and PASS/FAIL/UNAVAILABLE. A known baseline golden/platform problem is not automatically waived: reproduce/classify it and distinguish it from new failures. Do not blanket-update goldens, weaken authority tests, or label a partially verified cutover fully validated. Format changed code without repository-wide formatting churn.

## 14. One completion gate and final agent report

Execute the connected cutover end-to-end: root policy/composition, direct-path deletion, first-party/Feasibility authorization cleanup, test migration, conformance, docs, and final verification. Temporary compile breaks inside the same working change are acceptable; an intermediate state is not a completion boundary.

Local implementation commits may be organized for review, but this is one task with one final acceptance decision. Do not create a PR or push implementation commits unless separately authorized. The user authorized the planning-document commit in this conversation, not an automatic later publication of every implementation change.

Completion requires all of the following: no Manager live domain tools or fallback, admitted Expert execution preserved, no new Manager source grants, no fictitious Feasibility/Android support, current authority/recovery/provenance preserved, obsolete publication paths deleted, canonical docs amended, residuals classified, and required gates honestly reported.

Final report, concise but evidence-based:

1. Start/fetched/final revisions and implementation commit SHAs; worktree state.
2. Changed files and owners; final Manager -> Expert -> Context path.
3. Actual production root catalog and no-dispatch evidence for all seven old IDs.
4. Preserved generic Tool runtime, admitted Expert paths and root/delegated model consent.
5. Final supported first-party consumer sets and policy-digest/review-CAS results.
6. Feasibility disposition and explicit lack of new Expert-backed Feasibility acquisition.
7. Deleted direct-read, publication, authorization and obsolete test surface.
8. Preserved or ported safety/recovery tests, including stale evidence and response loss.
9. Residual-search findings and each intentional exception.
10. Exact validation commands/results; separate baseline failures and unavailable environments.
11. Architecture/product/ADR updates and execution evidence location.
12. Remaining blocker, if any; never declare completion solely because compilation passed.

## 15. Execution evidence

Not started. The planning-document commit does not constitute implementation or test execution. Replace this paragraph with the implementation report and actual verification evidence when the single task is executed.
