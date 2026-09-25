# 02: package-owned prompts/results and generic settlement

Prerequisite: checkpoint 01 in [the plan](README.md), completed on the current baseline at \`ed2498dc04457d6ee7a4e00e9427f7537c470960\`. Checkpoint 01 intentionally repaired the old Schedule-shaped result contract before this direct cutover. Checkpoint 02 removes that shared domain contract instead of preserving it as compatibility surface.

This checkpoint changes prompt identity, Expert result ownership, Task/A2A projection, stateful settlement, Actions proposal evidence and Flutter consumption as one bounded architectural cutover. It must not begin checkpoint 03 registration generalization or checkpoint 04 source binding.

The proposal-card golden assertion currently commented in \`apps/client/test/features/actions/agent_proposal_card_test.dart\` is an explicit operator-held exception. **Leave the commented golden assertion and \`agent_proposal_card.png\` unchanged in checkpoint 02.** Do not re-enable, update, delete or count that golden as verification. The surrounding non-golden proposal-card behavior remains in scope.

## Purpose and exit state

Checkpoint 02 is complete only when all of the following are true:

1. shared prompt contracts know only runtime roles, not eight built-in Expert identities;
2. the delegated-Expert inference consumer is generic and no longer encodes built-in distribution;
3. \`floe-agent-contract\` no longer owns Schedule/Focus input, insight, proposal or result semantics;
4. the canonical endpoint/Task result is \`ExpertReport -> TaskSnapshot\`: bounded result text, typed \`Artifact\` values and complete \`DependencyCoverage\`;
5. package payloads use package-owned media types/schemas; generic Experts/Registry/App do not deserialize their domain meaning;
6. consequential Calendar proposals use an Actions-owned typed proposal artifact and exact contributing dependency, not a generic Expert result or “first dependency” heuristic;
7. App/Vault stateful settlement validates exact assignment/invocation/coverage atomically without parsing Focus semantics;
8. Flutter consumes the generic Task result/safe artifact projection and Actions inspection DTOs; \`AgentExpertResult\` and the Schedule-shaped client parser are deleted;
9. the checkpoint-01 Rust -> Dart fixture has moved to the generic delegation/report boundary and the old \`fixtures/expert-result/schedule-v1.json\` fixture is deleted;
10. old shared types, result MIME assumptions and semantic validators have no production residuals;
11. the golden exception above is unchanged.

No dual parser, \`ExpertResultV2\`, compatibility envelope, optional migration field or “old/new” branch is allowed. Floe is pre-stable; perform a direct internal/wire cutover and update all in-scope callers together.

## Current source anchors

Line numbers are orientation only; search symbols on the execution HEAD.

| Concern | Current anchors | Current coupling to remove |
|---|---|---|
| Prompt role | \`crates/contracts/agent/src/prompts.rs::PromptRole\`, \`expert_prompt\`; \`crates/experts/builtin/src/prompts.rs\` | Shared enum lists all eight built-in Experts. |
| Expert inference identity | \`crates/contracts/agent/src/expert_model.rs::EXPERT_INFERENCE_CONSUMER\`; App Expert endpoint; Inference/provider tests | Generic delegated model execution is named \`experts.builtin\`. |
| Shared result | \`crates/contracts/agent/src/expert.rs\` | \`ExpertInput\`, \`ExpertInsight\`, \`ExpertFocusProposal\`, \`ExpertResult\` encode Schedule/Focus semantics. |
| Generic endpoint | \`crates/contracts/agent/src/endpoint.rs::ExpertReport\` | Already the correct generic endpoint boundary; retain it. |
| Generic artifact | \`crates/contracts/agent/src/message.rs::{Artifact, ArtifactPart}\` | Already carries media type and internal \`DependencyCoverage\`; retain it. |
| A2A projection | \`crates/modules/experts/src/a2a.rs\`, \`dispatch.rs\` | \`EXPERT_RESULT_MEDIA_TYPE\` and one primary result artifact are required; A2A is then parsed back into \`ExpertReport\`. |
| Built-in output | \`crates/experts/builtin/src/host.rs::BuiltinExpertOutput\`, \`StatefulExpertDraft\`, \`StatefulFocusProposal\` | Common built-in host shape carries Schedule result fields/call counts. |
| Schedule producer | \`crates/experts/builtin/src/schedule/{expert.rs,dispatch.rs}\` | Produces shared \`ExpertInsight\` and focus proposal. |
| Registry semantic validation | \`crates/modules/experts/src/registry.rs::{validate_result_content,validate_recorded_result,validate_historical_result}\` | Generic Registry interprets Focus/result content. |
| Stateful settlement | \`crates/app/src/vault_host/conversation_turn/expert_dispatch/stateful_settlement.rs\`; \`crates/modules/experts/src/settlement.rs\`; Vault Task/Registry settlement | App builds a shared \`ExpertResult\`, parses \`source_handle\` and persists it as Task result. |
| Actions | \`crates/modules/actions/src/application/expert.rs\`, \`ports/mod.rs\`; \`crates/adapters/vault/src/vault/expert_actions.rs\` | Actions/Vault deserialize \`ExpertResult\`, inspect \`action_proposals[0]\`, parse source-handle prefixes and depend on Registry result validators. |
| Flutter | \`agent_session.dart\`, \`agent_controller.dart\`, \`agent_expert_result.dart\`, proposal UI/tests | Completed delegation requires \`application/vnd.floe.expert-result+json;version=1\`; generic UI parses commitments/focus/action proposal fields. |

## Frozen boundary decisions

### Generic endpoint and durable Task

Retain \`floe_agent_contract::ExpertReport\` as the single canonical endpoint report:

~~~text
task_id
principal
agent_id
definition_revision
result: bounded user-facing result text
artifacts: Vec<Artifact>
coverage: complete DependencyCoverage
settlement: optional internal EndpointSettlement
~~~

\`TaskSnapshot\` is the durable lifecycle projection. Experts/Task validate invocation identity, result/artifact bounds, terminal state, coverage and exact settlement identity. It must not deserialize a package payload to decide whether a Task is valid.

A completed Task has one bounded result string. Package/domain structure belongs in artifacts, not in the result string.

### Internal Artifact versus product-safe A2A projection

\`floe_agent_contract::Artifact\` remains the authoritative internal artifact. Its \`coverage\` may contain full \`ContextDependency\` authority/provenance facts and therefore is **not automatically a Flutter/public wire payload**.

Keep a safe A2A/session artifact projection if needed, but it is projection only and must not be used to reconstruct internal authority. Never expose full \`ContextDependency\` merely to avoid a conversion type.

The current built-in endpoint round-trip:

~~~text
BuiltinExpertOutput
  -> A2ATask / EXPERT_RESULT_MEDIA_TYPE
  -> expert_report()
  -> ExpertReport
~~~

must be removed. Factor the common built-in execution once so the canonical endpoint path produces \`ExpertReport\` directly with intact internal artifact coverage. The A2A/session path may project that completed value separately for display/history.

### Package-owned domain artifacts

Each Expert package owns its result payload type and media type. There is no central enum/switch of all package result schemas.

Examples of ownership, not a central registry:

~~~text
schedule/*        owns ScheduleAssessment + its media type
commitments/*     owns CommitmentsExpertResult + its media type
communication/*   owns CommunicationExpertResult + its media type
work_context/*    owns WorkContextExpertResult + its media type
...
~~~

Unknown data media types are bounded inert data to generic code. Generic code validates bytes, identity and coverage; it does not execute or infer behavior from an arbitrary media type.

Package-produced domain artifacts may exist briefly with \`DependencyCoverage::Unknown\` inside the trusted extension output before host settlement. The canonical \`ExpertReport\` boundary must bind them to the captured report coverage (or a narrower explicitly proven contributor set) and reject any remaining Unknown artifact coverage.

### Actions-owned proposal

Use an Actions-owned typed proposal draft/evidence, not a shared Agent \`ExpertFocusProposal\`.

The current Schedule package may create a typed Calendar proposal **draft** owned by \`floe-actions\` containing only proposal semantics such as start/end. It is not authority and cannot execute anything.

Trusted settlement binds that draft to:

- exact Person;
- registry/assignment/package/invocation identity;
- settled assignment state revision;
- data class;
- source expiry;
- exact contributing \`ContextDependency.observation_id\`;
- an Actions-owned proposal media type such as \`application/vnd.floe.actions.calendar-proposal+json;version=1\`;
- artifact coverage containing the exact contributor.

Actions validates the typed artifact again before publishing an actionable Calendar record. A generic package artifact never authorizes Act.

For the current Schedule focus proposal, the implementation may rely on **exactly one matching Calendar contributor as a checked invariant**, because focus proposal generation already requires a single selected Calendar view. Do not choose \`dependencies[0]\`. Filter by exact Person, package consumer, Read/Assistant scope and Calendar source semantics, then require exactly one match; zero or multiple matches fail closed.

### Prompt and inference identities

Reduce \`PromptRole\` to:

~~~text
Manager
Expert
Learner
~~~

\`expert_prompt\` should produce \`PromptRole::Expert\` itself rather than accepting a package-specific role variant. Package prompt identity remains the Role component's source/revision/content.

Replace the built-in-distribution inference identity \`experts.builtin\` with one generic delegated-Expert identity. Use a single named constant (recommended: \`DELEGATED_EXPERT_INFERENCE_CONSUMER\`) and one value (recommended: \`experts.delegated\`) across App, Inference profiles, Access consent and provider tests.

This is **model-dispatch identity only**. Do not change source-read consumer identity: an Expert still reads Context/Access under the exact package/assignment consumer required by that source policy. Root inference remains \`conversation.root\`; Learner remains its own consumer. Existing external-recipient consent for the old \`experts.builtin\` identity must not silently authorize the new identity.

## Execution sequence and commit gates

The preferred sequence is:

| Substep | Scope | Commit rule |
|---|---|---|
| 02-A | Generic endpoint/Task/A2A result path | One canonical result path; no domain parse in Experts dispatch. |
| 02-B | Prompt role + delegated inference identity | Shared prompt/inference role cleanup, all callers/tests migrated. |
| 02-C | Package-owned artifacts + shared result-type deletion | All built-in result producers compile on package artifacts; no shared Focus semantics. |
| 02-D | Stateful settlement + exact contributor | Registry/Vault settlement generic; source-handle parsing removed. |
| 02-E | Actions-owned proposal bridge | Proposal publication/inspection/recovery uses Actions artifact + exact coverage. |
| 02-F | Product/Flutter + cross-language fixture | Generic Task result/artifacts, typed Actions inspection; old client parser/fixture deleted. |
| 02-G | Residual audit, broad gates, architecture/status convergence | Mark 02 Complete only here. |

02-C through 02-E may be one atomic implementation commit if splitting them would require a stale \`ExpertResult\` bridge or an unbound proposal. Do not add a compatibility type merely to obtain smaller commits.

## 02-A: make ExpertReport / TaskSnapshot the only canonical result path

### 02-A1: stop reconstructing ExpertReport from A2A

Refactor \`ConversationExperts\` / \`BuiltinExpertEndpoint\` so built-in execution has one private composition helper that performs:

1. exact registered Expert admission;
2. bounded child model/source execution;
3. trusted blocker capture/publication;
4. captured dependency coverage calculation;
5. package artifact coverage binding;
6. raw source-requirement artifact rejection;
7. optional stateful settlement;
8. construction of the canonical \`ExpertReport\`.

The helper may be App-private; do not introduce another public result envelope.

\`BuiltinExpertEndpoint::execute\` consumes that helper directly. Delete \`floe_experts::expert_report\` if no other legitimate caller remains.

### 02-A2: generic A2A/session projection

Change \`A2ATask\` so a completed task carries a direct bounded \`result: Option<String>\` in addition to its safe artifact projection. State invariants must mirror \`TaskSnapshot\`:

- Submitted/Working: no result, no terminal failure;
- Completed: nonempty result, no failure;
- Failed/Rejected/Cancelled: no successful result.

\`task_receipt_to_a2a\` copies \`TaskSnapshot.result\` directly and safely projects \`TaskSnapshot.artifacts\`; it must not deserialize \`ExpertResult\` or inspect a package media type.

\`completed_expert_task\`, if retained for in-process A2A tests/runners, receives generic result text and already validated artifacts. It no longer manufactures a special primary \`EXPERT_RESULT_MEDIA_TYPE\` artifact.

Do not expose internal \`Artifact.coverage\`/ContextDependency on the product wire unless an existing explicit safe DTO already requires it. A2A/session artifacts are display/typed-payload projection, never source authority.

Delete \`EXPERT_RESULT_MEDIA_TYPE\` from \`floe-experts\` once all callers migrate.

### 02-A3: generic report validation

Strengthen \`ExpertReport::validate\` / Task validation as needed:

- report coverage cannot be Unknown;
- completed result is bounded/nonempty;
- every canonical internal artifact validates;
- canonical internal artifact coverage cannot remain Unknown;
- every dependency named by artifact coverage must be contained in the report's complete coverage;
- an Independent artifact is allowed under a Dependent report;
- a Dependent artifact cannot introduce a dependency absent from report coverage;
- artifact IDs are unique where the owning Task/report requires uniqueness.

Do not interpret artifact media types here except for existing trusted coordination artifacts such as the user-interaction reference validation already owned by the Agent contract.

### 02-A tests

Port dispatch/Task tests to prove:

- generic result survives endpoint -> TaskSnapshot -> replay;
- arbitrary bounded unknown package artifact survives inertly;
- artifact with forged dependency outside report coverage is rejected;
- Unknown artifact coverage is rejected at canonical report boundary;
- A2A product projection does not become an authority source;
- duplicate artifact IDs fail closed;
- recovery/replay preserves Task result/artifacts exactly.

Recommended focused gate:

~~~sh
cargo test -p floe-agent-contract
cargo test -p floe-experts
cargo test -p floe-vault task
cargo test -p floe-app expert
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Use actual containing test targets when a filter would execute zero tests.

## 02-B: remove closed-world prompt and model-execution identity

### 02-B1: PromptRole

In \`crates/contracts/agent/src/prompts.rs\`:

- remove every individual built-in Expert variant;
- keep Manager / Expert / Learner;
- make \`expert_prompt\` assemble \`PromptRole::Expert\` without a role argument;
- retain package Role component source/revision/content and all kernel/protocol bounds;
- retain the “Persona only on Manager” invariant.

Update \`crates/experts/builtin/src/prompts.rs\` and prompt tests. Tests must prove two different package prompts both use runtime role Expert while retaining distinct Role component source/revision/content.

### 02-B2: delegated Expert inference consumer

In \`crates/contracts/agent/src/expert_model.rs\` and every App/Inference/provider caller:

- remove \`experts.builtin\`;
- use one generic delegated-Expert inference consumer;
- rename the constant if necessary so the identifier does not imply built-in distribution;
- keep root-only model profiles root-only;
- preserve exact recipient-consent semantics;
- do not map package IDs to model profiles;
- do not change Context/Access source consumers.

Update recipient-consent tests so an approval for the old consumer cannot authorize the new consumer implicitly. As this is a direct pre-stable cutover, do not add an alias.

Focused gate:

~~~sh
cargo test -p floe-agent-contract prompts
cargo test -p floe-experts-builtin
cargo test -p floe-inference
cargo test -p floe-provider-adapters
cargo test -p floe-app
python3 tools/architecture/check_boundaries.py
~~~

## 02-C: package-owned domain artifacts and deletion of shared Schedule result types

### 02-C1: delete caller-zero shared input

\`ExpertInput\` has no production caller on the checkpoint-01 baseline. Delete it from \`floe-agent-contract\`, re-exports and tests instead of relocating dead API.

### 02-C2: move Schedule semantics to Schedule

Remove these shared Agent semantics:

~~~text
ExpertInsight
ExpertFocusProposal
ExpertResult
~~~

Schedule owns its assessment/insight payload. Define Schedule-local serializable types for commitments/focus/no-focus semantics and a Schedule-local result media type. Package tests validate intervals, item bounds and Schedule semantics there.

Other built-in Experts already own result structs in their own modules. Give each package result an explicit package-owned media-type constant and emit it as a typed \`ArtifactPart::Data\`; do not add a central media-type enum/switch.

### 02-C3: simplify BuiltinExpertOutput

Converge \`BuiltinExpertOutput\` to generic endpoint material:

~~~text
result: bounded text
artifacts: Vec<Artifact>
settlement: Option<EndpointSettlement>
~~~

Remove \`artifact_name + summary + data\` as a special primary-result tuple.

A small built-in helper may serialize a package-owned result into an Artifact when the package supplies the artifact name and media type. That helper must not know any package ID or schema.

Package artifacts created before trusted host binding may use Unknown coverage only as an internal unsettled marker. The App execution helper must bind Unknown package artifacts to captured report coverage before creating \`ExpertReport\`, and canonical report validation rejects any Unknown that escapes.

Blocked-domain completion remains a bounded result with trusted interaction artifacts. Do not invent a domain conclusion or fake source evidence for a blocked source/model.

### 02-C4: remove generic Registry result semantics

Delete Registry/Vault code whose purpose is to validate \`ExpertResult\` domain content:

- \`validate_result_content\`;
- Focus-window/summary/model-call/view-call universal validation;
- \`validate_recorded_result\` / \`validate_historical_result\` forms that deserialize or interpret the old result;
- \`expert_identity_matches(..., ExpertResult)\` style helpers.

Retain/strengthen generic Registry invariants:

- exact instance/assignment/installation/package identity;
- assignment private-state revision/completed invocation transition;
- exact invocation identity;
- staged registry CAS;
- Task identity/settlement atomicity.

Package semantics are validated before artifact publication; Actions semantics are validated at Actions.

## 02-D: make stateful settlement domain-neutral and bind exact evidence explicitly

Current \`stateful_settlement.rs\` assembles a shared \`ExpertResult\` and derives evidence from \`source_handle\` string prefixes. Remove both behaviors.

### 02-D1: stateful draft

The built-in stateful draft must carry only generic settlement material plus owner-typed proposal drafts:

- bounded result text;
- package-owned artifact(s);
- optional Actions-owned proposal draft(s).

Remove shared \`insights\`, shared Focus proposal, \`source_handle\`, \`model_calls\` and \`view_calls\` from the common stateful settlement shape. Execution-budget/count assertions stay at the producing Expert/model/source tests; they are not a universal Task schema.

The Schedule package may use an Actions-owned Calendar proposal draft type for start/end semantics. Add the direct \`floe-experts-builtin -> floe-actions\` dependency only if needed; the architecture dependency policy already permits that edge. Run the checker. Do not put Actions types back into \`floe-agent-contract\`.

### 02-D2: exact contributor

Delete \`evidence_for_source\` and all authority decisions based on parsing:

~~~text
calendar.timeline:...
calendar.lease:...
calendar.observe:...
~~~

from generic settlement/Actions evidence selection.

For each Actions proposal draft, filter the captured dependencies by the proposal's actual source contract and require the exact expected contributor set. For the current Schedule focus proposal, require exactly one matching Calendar contributor; zero or more than one is a hard failure. Never select \`dependencies.first()\`.

Use the contributor's canonical \`observation_id\` as evidence identity and its expiry/current source facts as the proposal fence.

### 02-D3: atomic Task + Registry settlement

\`ExpertSettlement\` remains internal opaque settlement metadata. It may retain a generic task-result field if that is still needed for equality, but it must not embed/parse package domain JSON.

Vault settlement must verify:

- owner matches Task agent;
- exact assignment/invocation;
- staged assignment private-state transition;
- Task result equals settled result text;
- Task report coverage equals the canonical coverage computed from settlement dependencies;
- artifact coverage is a subset of Task/report coverage;
- Task/Registry commit stays atomic.

Do not hold a Vault transaction over model/source I/O. Existing CAS/conflict/retry behavior remains.

Focused tests must retain stale registry, foreign assignment, duplicate invocation, rollback and crash/reopen cases.

## 02-E: Actions owns the proposal artifact and consequential validation

Add an Actions-owned typed Calendar proposal evidence schema/media type. The trusted settlement bridge constructs it from:

- Actions-owned Calendar proposal draft;
- exact settled Expert identity;
- exact assignment/package/invocation/state revision;
- exact contributor observation identity;
- data class/source expiry;
- exact contributor coverage.

The proposal Artifact is the durable, atomically Task-persisted **proposal intent**. It is not dispatch authority. Creating/approving/executing a Calendar action still goes through the existing Actions owner, approval mode, current source fence, preflight, durable pre-dispatch intent and uncertain-write recovery.

Refactor \`ExpertActionStore\` and Vault expert-action code so they no longer accept or deserialize \`ExpertResult\`.

The new flow is:

~~~text
ExpertProposalReference
  -> find exact recorded delegation/Task
  -> find exactly one Actions-owned Calendar proposal artifact
  -> parse/validate Actions schema
  -> verify receipt + assignment/package/invocation identity
  -> verify artifact contributor is present in trusted recorded Task/turn coverage
  -> revalidate current grant/policy/source through existing fence
  -> publish/inspect CalendarAction
~~~

Do not use a package domain artifact as executable input. Do not infer the proposal from the first artifact or first dependency.

Preserve \`AgentActionOrigin\`, idempotent action IDs, approval, cancellation, response-loss and uncertain external-write recovery.

Mandatory negative tests:

- forged Actions media type/payload;
- proposal identity for another invocation/assignment/package;
- proposal coverage points at the wrong contributor;
- contributor absent from report coverage;
- multiple proposal artifacts when exactly one is expected;
- expired/revoked/changed grant;
- stale source subject;
- generic package artifact that looks executable but is not an Actions media type;
- duplicate/replayed publication remains idempotent.

## 02-F: generic product/client result and cross-language fixture

### 02-F1: product-visible delegation shape

Update the safe A2A/session/product projection and \`AgentCapabilityMessage.fromDelegation\` so a completed delegation consumes:

- Task result text directly;
- bounded safe artifact metadata/data parts;
- Task state/failure.

It must not search for \`EXPERT_RESULT_MEDIA_TYPE\` or require exactly one Schedule-shaped data artifact.

Keep internal ContextDependency/authority coverage out of the public client payload unless an existing explicit safe projection already exposes it.

### 02-F2: delete AgentExpertResult

Delete:

~~~text
apps/client/lib/features/experts/domain/agent_expert_result.dart
apps/client/test/features/experts/agent_expert_result_test.dart
old support fixture/parser helpers that exist only for that schema
~~~

Remove \`AgentController.expertResult\`.

The generic conversation panel displays the Task result text and may show bounded artifact labels/types without interpreting package JSON. It does not render Commitment/FocusWindow as universal Expert semantics.

### 02-F3: proposal UI uses Actions inspection

The proposal affordance may recognize the **Actions-owned proposal media type** as “there is a reviewable action proposal”, but Flutter must not parse its Schedule/domain payload as authority.

\`canInspectProposal\` is based on the saved exact delegation + Actions proposal artifact + proposal gateway, not \`AgentExpertResult.proposal\`.

If the proposal card needs the proposed interval, add start/end to the Actions-owned inspection DTO from the recorded CalendarAction/proposal owner and parse it in \`AgentProposalInspection\`. The card gets display semantics from Actions, not the Schedule artifact.

The existing commented proposal-card golden assertion stays commented and the image stays untouched.

### 02-F4: replace checkpoint-01 cross-language fixture

Replace:

~~~text
fixtures/expert-result/schedule-v1.json
fixtures/expert-result/README.md
~~~

with one generic Rust-produced fixture at the Task/delegation product boundary, recommended:

~~~text
fixtures/expert-report/delegation-v1.json
fixtures/expert-report/README.md
~~~

Generate it from the real Rust production path after package settlement and generic Task/A2A projection, with deterministic identity/time normalization only after the real producer ran. Prefer the exact Rust DTO/map shape consumed by \`AgentCapabilityMessage.fromDelegation\`.

A normal Rust test compares production output to the tracked fixture. A Dart test reads the same bytes and validates generic Task result/artifacts. Keep regeneration instructions beside it.

Delete the old fixture in the same cutover. No dual fixture/parser.

## Test disposition

| Existing coverage | Treatment |
|---|---|
| PromptRole per-builtin variant tests | Rewrite: all built-ins are runtime Expert; package Role component identity stays distinct. |
| \`ExpertInput\` tests/callers | Delete if caller-zero; do not preserve test-only API. |
| Registry FocusWindow/content validation | Delete from Registry; move Schedule semantics to Schedule package and Actions semantics to Actions. |
| Checkpoint-01 model/view-call parser assertions | Remove from generic/client schema; retain budget/count checks at producing owners. |
| \`fixtures/expert-result/schedule-v1.json\` | Replace with generic delegation fixture and delete old fixture. |
| Schedule judgment/focus-window tests | Retain under Schedule package using Schedule-owned result type. |
| Expert Task settlement atomicity/CAS/stale assignment | Retain and rewrite against generic result/artifacts. |
| Action approval/evidence/current-grant/uncertain-write recovery | Retain and rewrite against Actions-owned proposal artifact. |
| Interaction-ref artifacts | Retain trusted host publication and anti-forgery assertions. |
| Proposal-card non-golden behavior | Retain/update for Actions inspection. |
| Proposal-card golden assertion/image | **Leave exactly as currently disabled/unchanged by operator instruction.** |

## 02-G: deletion audit, verification and documentation convergence

### Residual audit

Search production and tests for at least:

~~~text
PromptRole::ScheduleExpert
PromptRole::CommitmentsExpert
PromptRole::CommunicationExpert
PromptRole::RelationshipsExpert
PromptRole::FocusAttentionExpert
PromptRole::WellbeingExpert
PromptRole::WorkContextExpert
PromptRole::LifeLogisticsExpert

experts.builtin
EXPERT_INFERENCE_CONSUMER

ExpertInput
ExpertInsight
ExpertFocusProposal
ExpertResult
StatefulFocusProposal
StatefulExpertDraft
validate_result_content
validate_recorded_result
validate_historical_result

EXPERT_RESULT_MEDIA_TYPE
application/vnd.floe.expert-result
agent_expert_result
expertResult(
fixtures/expert-result

source_handle.starts_with("calendar.
strip_prefix("calendar.
dependencies[0]
dependencies.first
~~~

Allowed residuals:

- historical/checkpoint documentation;
- Schedule-local semantic type names that are no longer shared;
- explicitly owner-local source-handle display data that is not used for authority/evidence selection;
- the operator-held commented proposal-card golden code.

Every production residual of the removed shared contract must be deleted or narrowly justified.

### Targeted gates

Run focused tests as the substeps land. At final convergence, at minimum run:

~~~sh
cargo test -p floe-agent-contract
cargo test -p floe-experts
cargo test -p floe-experts-builtin
cargo test -p floe-actions
cargo test -p floe-vault
cargo test -p floe-app
cargo test -p floe-protocol
cargo test -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

A zero-test filter is not evidence; run the containing test target/package.

### Broad gate

Because 02 changes shared Rust contracts, persistence/settlement semantics and Flutter-visible delegation/action DTOs:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
cargo build -p floe-ffi

(
  cd apps/client
  flutter analyze
  flutter test
  flutter build macos
)

git diff --check
~~~

The proposal-card golden comparison is already disabled by explicit operator instruction. Do not run an update-goldens mode and do not modify the PNG to satisfy this checkpoint.

Go server behavior is not intended to change. If server source/protocol behavior is unexpectedly changed, run \`go test -race ./...\` and \`go vet ./...\` from \`server/\` and explain why the boundary expanded. Do not change server endpoints merely to make 02 compile.

iOS is not a required checkpoint-02 gate. Report it as not executed if not run; never claim pass.

### Documentation convergence

Update current architecture only after code changes establish the new truth:

- runtime docs: generic delegated Expert role/result path;
- authority/recovery docs: Actions proposal artifact is not authority and exact contributor/current source fences remain;
- any current product docs that still say generic UI interprets FocusWindow as the common Expert result.

Do not document checkpoint 03 generic registration or checkpoint 04 source binding as implemented.

Only after all deletion/verification gates pass, update the 02 row in \`README.md\` to Complete with actual commit SHAs. Leave 03 Not started.

## Required completion report

Report:

1. start HEAD/origin/worktree and 02 substep commit SHAs;
2. final shared PromptRole and delegated inference consumer;
3. old \`experts.builtin\` consent/profile disposition;
4. canonical \`ExpertReport -> TaskSnapshot\` result path and safe A2A/product projection;
5. package result media-type ownership and removed central result switches;
6. deleted shared \`ExpertInput/Insight/FocusProposal/ExpertResult\` APIs and their replacement owners;
7. stateful settlement identity/coverage algorithm and proof that source-handle parsing/first-dependency selection is gone;
8. Actions-owned proposal artifact schema/media type, exact contributor binding and retained approval/recovery fences;
9. Flutter generic result/action-inspection path and deleted \`AgentExpertResult\`;
10. new Rust-generated fixture path/regeneration command and deletion of the checkpoint-01 fixture;
11. exact targeted/broad verification commands and observed results;
12. residual-search results and any narrowly justified 03+ residual;
13. confirmation that the proposal-card golden assertion/image were left unchanged;
14. architecture/status docs updated;
15. final local HEAD and next checkpoint 03, without starting it.

A report that leaves the old shared result decoder/validator in production is not checkpoint-02 completion.
