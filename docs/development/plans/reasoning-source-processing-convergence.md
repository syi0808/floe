# Reasoning, Gateway, source-processing, and Health privacy convergence

- Status: in progress — Checkpoints 00–01 complete; Checkpoint 02 not started
- Investigation baseline: main at af2eda2bf276241d903bd4ed29c2565081804339
- Governing decision: [ADR 0034](../../decisions/0034-gateway-reasoning-and-source-processing-authority.md)
- Classification: architectural change
- Authoritative scope: this plan supersedes the previously implied "start CP08 / add typed grounding contract next" sequence after agent-execution-environment-grounding Checkpoint 07. Do not start that old CP08 while this plan is active.
- Primary owners: Product/AppWire, Conversation, Context, Access, Inference, provider adapters, Experts, Knowledge/Learner, Connections, Apple native Health/local-model integration, Go Gateway
- Safety owners preserved: Connections source truth, Access source grants and processing policy, Context provenance/freshness, Actions authority, Vault key/CAS/recovery semantics
- Execution rule: checkpoints are ordered. Do not start a later checkpoint while an earlier checkpoint has an unresolved semantic, deletion, or verification failure.
- Compatibility rule: Floe is pre-stable. Replace obsolete internal contracts directly. Do not keep old/new model-consent, routing, or source-processing paths alive through compatibility adapters, nullable migration fields, parallel schemas, or permanent legacy decoders.
- Platform rule: Apple is the delivery target. Android Health semantics may be made fail-closed at shared boundaries, but this plan does not add or validate an Android local-model implementation.
- Privacy scope rule: mandatory local semantic privacy transformation is implemented for Health/Wellbeing only in this plan. Do not generalize it to Calendar, Mail, Contacts, Work Context, Logistics, Tasks, or Memory without a separate product decision and a demonstrated second use case.
- Checkpoint detail rule: this file remains the ordering/invariant authority. Code-line execution detail may live under [reasoning-source-processing-convergence/](reasoning-source-processing-convergence/); each parent checkpoint links its active detail document instead of duplicating implementation instructions.

## 0. Why this plan exists

Two investigations converged on one root problem.

First, the current product model leaks concrete inference implementation into the client and user-approval path even though ADR 0011 already states that users should reason in product purposes rather than providers, model IDs, endpoints, or reasoning controls. Today the Conversation command carries ProfileSelection/profileId, server purpose discovery returns recipient/placement/external-consent detail, Access derives exact-recipient ProcessingRequirement records, Conversation persists a ProcessingRecipient interaction, and Flutter renders internal profile/consumer/recipient values. The resulting Model request card is not only poor presentation; it exposes a routing decision that the product should own.

Second, privacy authority and reasoning placement have become coupled. Current first-party Observe policies are LocalOnly, exact model-recipient consent is a separate authority vertical, and some shipped Experts use local reasoning largely because their sources are sensitive. The intended architecture is different: source owners produce bounded Views under explicit processing policy; Agent reasoning then uses those Views with a server/Gateway primary model and a device-local fallback. Reasoning placement does not perform privacy minimization.

Health exposes the strongest version of this distinction. Current Apple Health already performs a bounded 36-hour acquisition and deterministic reduction from HealthKit samples to wellbeing.derived, but it does not perform the newly required local-LLM privacy transform. The existing Apple Foundation Models runtime is used as an Agent reasoning provider, not as a source-owned Health sanitizer. Current Context classification is also inconsistent: the Wellbeing Expert manifest is HighlySensitive, while personal_context_evidence and the common Expert policy classify the actual model evidence as Personal.

The completed system must therefore converge on four independent concepts:

1. Product inference intent: purpose plus the safe Device/Gateway boundary, never provider/model/profile/recipient.
2. Source processing authority: whether a source-owned View may be processed only on device or also by the paired Floe Gateway.
3. Health privacy transformation: a mandatory device-local, source-owned semantic minimization step before any Health-derived View enters Agent reasoning.
4. Reasoning selection: Manager, Expert, and Learner all use the same Gateway-primary / device-local-fallback planning policy.

The previous Checkpoint 07 Manager evaluation concluded with a valid Foundation hard-gate failure and proposed typed grounding as the next architecture investigation. That decision is intentionally deferred here. Re-evaluate grounding only after the primary Gateway reasoning topology and the fallback acceptance modes below are in place.

## 1. Final architecture all checkpoints must converge to

### 1.1 Product-visible inference boundary

The Floe client may know:

- product purpose: quick_response, everyday_assistance, deep_work;
- whether work is device-local or crosses the paired Floe Gateway boundary when that distinction is materially useful to the product.

The Floe client must not know or select:

- provider;
- model identifier;
- profile identifier;
- Codex OAuth;
- OpenAI or any other concrete downstream model recipient;
- reasoning effort;
- server-internal route target;
- endpoint.

The server/operator surface may continue to expose provider, model, target, reasoning effort, and downstream transport diagnostics because those are deployment concerns.

The target product flow is:

~~~text
Product feature / Conversation
  -> purpose
  -> canonical Inference
  -> Device or Floe Gateway boundary

Floe Gateway
  -> purpose class
  -> configured provider
  -> configured model
  -> configured reasoning effort
~~~

No concrete Gateway downstream recipient becomes a user-approval identity.

### 1.2 Shared reasoning plane

Manager, shipped Experts, and Learner all use one canonical selection policy:

~~~text
Manager / Expert / Learner
        |
        v
Canonical Inference
        |
        +-- Primary  -> paired Floe Gateway / server reasoning
        |
        +-- Fallback -> device-local Foundation model
~~~

Their task semantics remain distinct:

| Caller | Purpose | Consumer |
| --- | --- | --- |
| Manager | everyday_assistance | conversation.root |
| delegated Expert | everyday_assistance | canonical delegated-Expert consumer |
| Learner | deep_work | knowledge.learner |

Fallback means planning-time primary absence, not failure masking.

The common rule is:

~~~text
usable primary profile exists
  -> use Primary only

no saved/admitted Gateway profile
or the Gateway inventory truthfully says the requested purpose is unavailable
  -> use local Fallback

credential expiry
invalid inventory
server observation/transport failure
policy denial
source-processing denial
deadline/cancellation
  -> surface the real failure or SourceAccess blocker
  -> do not silently retry on the local model
~~~

Explicit model/profile selection is not a product concept. A test/evaluation harness may force an internal provider/profile only through an Inference-private test seam.

### 1.3 Source-processing authority

The final standing source processing contract is boundary-oriented, not recipient-oriented:

~~~text
ProcessingPolicy
  DeviceOnly

  GatewayAllowed {
      categories
  }
~~~

GatewayAllowed means the admitted bounded View may be processed on device and by the paired Floe Gateway. It says nothing about which provider/model the Gateway will use.

A model route change inside the Gateway must never invalidate a connector grant or ask the person to approve a new model.

A missing or insufficient source-processing permission is a SourceAccess problem. It may produce a source/connection review interaction. It is never a Model request interaction.

### 1.4 Health privacy plane

Health is the only source family that receives a mandatory local semantic privacy transform in this plan.

~~~text
HealthKit
  -> bounded HealthKit reads
  -> deterministic local aggregation / metadata minimization
  -> mandatory device-local Health privacy transform
       -> unavailable/failure: fail closed
       -> valid typed result: continue
  -> WellbeingView
  -> Context / provenance
  -> Agent reasoning
       -> Gateway Primary
       -> local reasoning Fallback
~~~

The same physical Apple Foundation Model may serve both the privacy-transform operation and an Agent reasoning fallback, but these are separate APIs and separate invocations.

Health privacy transform:

- is source-owned;
- accepts only a bounded Health-specific input;
- produces a typed Health-specific output;
- receives no Conversation history, Persona, Memory, Manager/Expert catalog, Tool catalog, delegation capability, or ModelStep grammar;
- cannot call Tools or Experts;
- cannot authorize Gateway processing;
- cannot self-assert source/grant authority;
- cannot fall back to a remote model.

Agent reasoning:

- uses the ordinary ContextEnvelope and Agent Runtime;
- sees only the post-transform WellbeingView;
- never receives raw HealthKit records or the pre-transform aggregate.

If the mandatory local transformer is unavailable, Health-derived reasoning is unavailable. Deterministic semantic classification is not used as a hidden fallback after this cutover.

### 1.5 Durable user-interaction plane

Durable interactions remain for user decisions that actually belong to the person:

~~~text
SourceAccess
ExpertBinding
Actions review/execution remains Actions-owned
~~~

The model-recipient interaction vertical disappears.

When a SourceAccess or ExpertBinding interaction is resolved, the initiating work resumes automatically through a durable, idempotent resume slot. Resolved does not project a routine manual Continue action.

## 2. Non-negotiable invariants

These apply to every checkpoint.

1. Model invocation is not a user-consent event. Do not replace exact-recipient consent with another model/provider approval UI.
2. Source authority remains exact. Connection/resource selection, GrantAuthority, SourceAuthority, consumer, purpose, processing policy, provenance, and current provider/OS source state must still be checked by their existing owners.
3. Gateway identity is still authenticated/pinned through the saved connection and pairing boundary. Removing model-recipient consent must not turn an arbitrary endpoint into an admissible Gateway.
4. GatewayAllowed never authorizes an Action. Observe/processing and consequential Actions remain separate.
5. Source-processing expansion is owned by the source/connection review path. A model dispatch cannot silently mutate a connector grant.
6. Source-processing denial is never a reason to use the local reasoning fallback. Fallback is capability selection, not an authorization bypass.
7. Health raw samples and the pre-transform Health aggregate never cross the device boundary and never enter Conversation, Agent Runtime, Memory, Flutter product payloads, or the Gateway.
8. A Health WellbeingView cannot enter reasoning without valid source-owned local privacy-transform evidence after the Health cutover.
9. Health transform success does not downgrade HighlySensitive to Personal. Sensitivity and processing eligibility are separate.
10. Credential and DeviceOnlyRaw remain forbidden from generic Agent reasoning.
11. The model/LLM cannot grant itself transform provenance, source authority, processing authority, or Action authority.
12. Manager, Expert, and Learner use one Inference planning policy. Do not retain role-specific hidden routing rules after the cutover.
13. A selected Primary transport failure is not retried on Fallback. Do not turn server outages into silent local-mode changes.
14. Exact command/recovery semantics, CAS, key identity, cancellation direction, durable pre-dispatch intent, and uncertain external-write recovery remain fail-closed.
15. A SourceAccess interaction resolution that should resume work must survive crash/reopen and admit at most one linked child Run.
16. Do not add Android parity work, Android Foundation-model substitutes, or Android build gates in this plan.
17. Do not introduce a generic privacy-transform framework merely to anticipate future sensitive connectors. The Health contract may be generalized only after a second real use case proves a common boundary.
18. Do not start typed grounding-contract implementation as part of this plan. Revisit it only at Checkpoint 11.

## 3. Current source anchors and mismatches

These anchors are from the investigation baseline and must be refreshed against main before each checkpoint.

### Product/model routing leaks

- crates/modules/conversation/src/domain/intent.rs
  - ProfileSelection::Auto / Explicit(String)
  - StartTurn.profile
  - CanonicalTurnIntent.profile
  - turn digest binds the profile
- apps/client/lib/features/conversation/application/agent_conversation_gateway.dart
  - AgentConversationTurnRequest.profileId
- apps/client/lib/features/conversation/application/conversation_runtime_gateway.dart
  - forwards profileId
- apps/client/lib/app/runtime/floe_client.dart
  - serializes explicit profile selection
- crates/bindings/protocol/src/dto/commands.rs
  - AppProfileSelectionDto / ConversationStartTurn profile
- crates/bindings/ffi/src/app_wire.rs
  - converts product profile selection to the domain command

### Recipient-consent authority vertical

- crates/contracts/context/src/processing.rs
  - ProcessingRequirement
  - ProcessingSourceScope
  - RecipientLineage
- crates/modules/access/src/application/model_dispatch.rs
  - derives an exact recipient/profile requirement from a selected model route
- crates/modules/access/src/application/recipient_consent.rs
  - contextual exact-recipient authority and deterministic consent identity
- crates/modules/access/src/ports/recipient_consent.rs
  - consent persistence/admission ports
- crates/adapters/vault/src/vault/recipient_consents.rs
  - recipient_consent_schema / recipient_consents persistence
- crates/adapters/vault/src/repositories/recipient_consents.rs
  - Vault consent repository
- crates/adapters/providers/src/control/recipient_authority.rs
  - saved connection plus recipient-consent authority composition

### Model-blocked Conversation interaction

- crates/contracts/agent/src/model.rs
  - ModelCallOutcome::NeedsUserAction(ProcessingRequirement)
- crates/modules/conversation/src/domain/interaction.rs
  - RecipientConsentTarget / ProcessingRecipient reviewed target
- crates/modules/conversation/src/application/interactions.rs
  - publish_model_requirement
- crates/modules/conversation/src/api.rs
  - MODEL_CONSENT_LIMITATION
- crates/modules/conversation/src/application/coordinator.rs
  - blocked model attempt completes the Run with the fixed limitation
- crates/modules/conversation/src/application/finalization.rs
  - same blocked-finalization pattern
- crates/app/src/vault_host/conversation_turn/interaction_publication.rs
  - publish_model_blocker
- crates/app/src/vault_host/conversation_turn/expert_host.rs
  - stashes model_blocked ProcessingRequirement
- crates/bindings/protocol/src/dto/interactions.rs
  - recipient-consent target crosses AppWire
- crates/bindings/ffi/src/app_wire.rs
  - Resolved interaction currently projects ContinueRequest
- apps/client/lib/features/conversation/domain/agent_interaction.dart
  - AgentRecipientConsentTarget
- apps/client/lib/features/conversation/presentation/agent_interaction_card.dart
  - renders raw recipient/profile/purpose/consumer/data/scope fields

### Processing restriction mismatch

- crates/contracts/context/src/lib.rs
  - ProcessingRestriction::LocalOnly / ApprovedRecipient
- crates/app/src/first_party_observe.rs
  - first-party policy() currently creates LocalOnly for every supported view
- crates/modules/access/src/application/remote_view.rs
  - remote_view_scope() is LocalOnly and documents model-recipient consent as a separate decision
- crates/modules/access/src/application/model_dispatch.rs
  - external target + LocalOnly is denied

This means server/Gateway-first Expert reasoning cannot be cut over safely before the source-processing contract is replaced.

### Inference routing mismatch

- crates/modules/inference/src/application/service.rs
  - rank_profile() is device-first
  - Auto may try later ranked candidates after transport failure
  - missing recipient consent blocks fallback
- crates/modules/inference/src/ports/model_provider.rs
  - ModelProvider::observe_profiles returns Vec, so observation errors are lossy
- crates/adapters/providers/src/models/root.rs
  - RootModelProvider composes Foundation + optional server for both root and delegated Expert use
- crates/adapters/providers/src/models/server.rs
  - observe_profiles() converts inventory/network/credential failures into an empty candidate list
- crates/modules/inference/src/application/router.rs
  - stale parallel router surface remains outside the canonical InferenceService path

### Expert mismatch

- crates/experts/builtin/src/*/expert.rs
  - shipped Expert requirements currently mix Any, DeviceOnly, and RemoteOnly
- crates/experts/builtin/src/catalog.rs
  - supports_device_model() creates a shipped package-specific placement distinction
- crates/experts/builtin/src/registration.rs
  - AgentCard.supported_placements follows supports_device_model()
- crates/app/src/vault_host/conversation_turn/expert_dispatch.rs
  - delegated Experts currently construct RootModelProvider and ContextualRecipientAuthority

### Learner mismatch

- crates/modules/knowledge/src/application/learner.rs
  - LEARNER_INFERENCE_PURPOSE = governed-memory-review
  - Learner assumes device placement
- crates/app/src/vault_host/learner_worker.rs
  - FoundationModelProvider only
  - InferenceExecutionConstraint::DeviceOnly
  - LearnerModelHost::placement() returns DeviceLocal
  - LearnerRecipientAuthority always fails external recipient checks

### Health privacy mismatch

- apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthKitWellbeingProvider.swift
  - bounded 36-hour HealthKit acquisition and deterministic aggregation
- apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/WellbeingProjection.swift
  - AppleWellbeingReducer performs deterministic semantic classification
- apps/client/apple/FloeAppleHealth/README.md
  - documents deterministic on-device reduction only; no local LLM transform exists
- apps/client/macos/LocalModel/LocalModel.swift
  - shared macOS/iOS FoundationModels runtime exists, but only as an Agent/Learner local-model host
- apps/client/ios/build_native.sh
  - compiles the same LocalModel.swift for iOS
- crates/contracts/context/src/views/personal.rs
  - personal_context_evidence() labels People, Attention, and Wellbeing uniformly as DataClass::Personal
- crates/experts/builtin/src/catalog.rs
  - Wellbeing manifest declares DataClass::HighlySensitive
- crates/app/src/vault_host/conversation_turn/expert_host.rs
  - expert_policy() is hard-coded to [Personal]
- apps/client/android health descriptors/fixtures
  - wellbeing.derived currently declares personal; Android local-model work remains out of delivery scope

## 4. Checkpoint 00 — Freeze the durable architecture decision

### Goal

Change the governing architecture before code implementation so agents are not instructed to preserve the exact-recipient design that this plan intentionally retires.

### Required durable-document changes

Add a new ADR, recommended path:

    docs/decisions/0034-gateway-reasoning-and-source-processing-authority.md

ADR 0034 must state, in meaning:

1. Product clients choose/observe product purpose and a safe Device/Gateway execution boundary, never provider/model/profile/exact downstream recipient.
2. The paired Floe Gateway is the external reasoning trust boundary from the client perspective.
3. Ordinary model invocation does not require per-request user approval.
4. Access owns source-processing authority; a source grant decides DeviceOnly versus GatewayAllowed.
5. Processing expansion is reviewed through the owning source/connection, not a model card.
6. Manager, shipped Expert, and Learner use Gateway-primary / device-local-fallback planning with identical fallback semantics.
7. Fallback is used only when no usable Primary profile exists at planning time; it does not mask credential, policy, transport, timeout, or source-authority failures.
8. Health-derived reasoning input requires a successful device-local privacy transform before it enters Context/Agent reasoning.
9. The Health transform is a source operation, not an Agent ModelPort call; local transform unavailability fails closed.
10. Transform success is provenance/processing evidence, not authorization. Gateway processing still requires the source grant's GatewayAllowed policy.
11. Typed grounding is re-evaluated only after the primary Gateway Manager is tested under the post-cutover architecture.

Amend/supersede active prose in:

- AGENTS.md safety section;
- .agents/skills/architecture-change/SKILL.md safety section;
- README.md safety/compatibility wording;
- docs/architecture/invariants.md;
- docs/architecture/authority-recovery.md;
- docs/architecture/runtime.md;
- docs/architecture/modules.md;
- docs/product/principles.md;
- docs/product/integrations-and-privacy.md;
- docs/product/intelligence.md;
- docs/product/experience.md;
- ADR 0011, 0015, 0024, 0028, 0030, and 0033 where their rationale explicitly assumes exact-recipient model consent or product-selected model profiles.

Do not rewrite historical evidence into current architecture. Amend or supersede the durable decision in the owning ADR and update current architecture only as implementation lands.

### Old invariant to retire

Before Checkpoint 00, active repository guidance said "preserve exact-recipient consent". After this checkpoint, the active invariant must instead be equivalent to:

    preserve verified Gateway identity, source-owned processing authority,
    provenance, key identity, CAS, durable pre-dispatch intent,
    cancellation direction, and uncertain-write recovery.

This is a deliberate product/security-model change, not a weakening performed merely to make tests pass.

### CP07/CP08 relation

Record that agent-execution-environment-grounding Checkpoint 07 remains historical execution evidence, but its proposed immediate typed-grounding follow-up is deferred. Do not execute its old CP08 until Checkpoint 11 of this plan decides whether primary Gateway reasoning still needs it.

### Acceptance

- No production behavior change in Checkpoint 00.
- Active instructions no longer require preserving exact model-recipient consent.
- Current architecture documents do not claim the new runtime already exists until the corresponding implementation checkpoint lands.
- ADR 0034 is the durable rationale authority for the new trust boundary.
- The plan remains the single migration sequence.

## 5. Checkpoint 01 — Correct Health sensitivity and model-input classification

### Status and refreshed baseline

Checkpoint 01 is complete; execution evidence is in section 22. This section was refreshed against `main` at:

    f08c6254f8820289378f5d2aa92b69c98097b3a8

No CP01 production implementation has started at this baseline. The line numbers below are execution anchors for this revision; if `main` moves before implementation, refresh the affected ranges before editing rather than applying line numbers mechanically.

Checkpoint 01 owns only sensitivity truth and model-input classification. It does **not** implement the Health privacy transformer, source-processing-policy cutover, model-recipient-consent deletion, Gateway routing, Primary/Fallback selection, or Android runtime work.

### Goal

Make Health/Wellbeing sensitivity source-owned and make every model projection carry at least the sensitivity of the content it actually contains.

After this checkpoint:

~~~text
PeopleView
  -> ContextEvidence Personal

AttentionView
  -> ContextEvidence Personal

WellbeingView
  -> ContextEvidence HighlySensitive

declared/admitted caller classes
  + actual live evidence classes
  + Personal when projected Persona/Memory is present
  -> canonical sorted/deduplicated AuthorizedModelProjection.input_data_classes
~~~

A caller or Expert policy may declare an equal or stricter class set. It may never relabel or omit an actual evidence class.

### Refreshed source findings that constrain the implementation

At the refreshed baseline:

1. `crates/contracts/context/src/views/personal.rs:99-131` defines `PersonalContextProjection` without a sensitivity method and applies one common macro implementation to People, Attention and Wellbeing.
2. `crates/contracts/context/src/views/personal.rs:196-205` hard-codes every personal-context evidence item to `DataClass::Personal`.
3. `crates/contracts/agent/src/context.rs:72-95` already treats `InferencePolicyDecision.data_classes` as an allow-list: any live evidence class absent from the policy fails closed. Persona or Memory additionally requires `Personal`, and `Credential` / `DeviceOnlyRaw` are rejected.
4. `crates/app/src/vault_host/conversation_turn/expert_host.rs:39-48` currently creates one common Expert policy containing only `Personal`. If Wellbeing evidence is corrected to `HighlySensitive` without changing this allowance, Context authorization will reject the Wellbeing path before model projection.
5. `crates/experts/builtin/src/catalog.rs` already declares the Wellbeing package as `DataClass::HighlySensitive` through `BuiltinExpertKind::context_data_class()`, and `crates/experts/builtin/src/registration.rs:81-110` copies that value into `ExpertManifest.data_class`. This package declaration may define the Expert's permitted sensitivity envelope, but it must not become the source of an evidence item's classification.
6. `crates/modules/context/src/application/model_projection.rs:47-60` receives caller-declared `input_data_classes`, while `assemble_context_projection():65-117` copies that list directly into the final projection at line 114 even though the assembled envelope already contains live evidence/Memory/Persona. That is a second downgrade path independent of `personal_context_evidence()`.
7. `crates/modules/access/src/application/model_dispatch.rs:117-133` currently rejects `Credential` / `DeviceOnlyRaw` and also rejects external dispatch containing `HighlySensitive`. This remains intentionally fail-closed in CP01. Do not weaken it to make the newly correct classification reach a remote model.
8. Apple and Android Health connector descriptors still advertise `wellbeing.derived` as `personal`. The Rust connected-context contract already parses descriptor `data_class` into `DataClass`; no schema/version expansion is required.

These findings mean CP01 is not a one-line `Personal -> HighlySensitive` replacement. The source evidence, policy allowance and projection fold must converge together.

### 01-A — Make personal View sensitivity source-owned

Primary owner:

    crates/contracts/context/src/views/personal.rs

#### 01-A1 — Put `data_class()` on the View projection contract

Edit around `PersonalContextProjection` at lines 99-105.

Add a method equivalent to:

~~~text
fn data_class(&self) -> DataClass;
~~~

The implementation must be statically determined by the View type:

~~~text
PeopleView     -> Personal
AttentionView  -> Personal
WellbeingView  -> HighlySensitive
~~~

The existing `projection!` macro at lines 107-131 may be changed to accept the class as an argument, for example:

~~~text
projection!(PeopleView, DataClass::Personal)
projection!(AttentionView, DataClass::Personal)
projection!(WellbeingView, DataClass::HighlySensitive)
~~~

or replaced by equally direct explicit implementations. Do not classify by `view_id` string at runtime and do not ask the consuming Expert which class the View should have.

Do not change the serialized People/Attention/Wellbeing payload shape in CP01.

#### 01-A2 — Remove the hard-coded Personal evidence class

Edit `personal_context_evidence()` around lines 196-205.

Replace:

~~~text
data_class: DataClass::Personal
~~~

with the class supplied by the View contract.

The same typed `WellbeingView` must therefore be HighlySensitive regardless of whether it is read by Wellbeing, another future consumer, a fixture, or a local reasoning path.

#### 01-A3 — Owner-level and fixture regressions

Add or extend focused tests so the contract itself proves:

- People evidence is `Personal`.
- Attention evidence is `Personal`.
- Wellbeing evidence is `HighlySensitive`.
- JSON payload/provenance validation behavior is otherwise unchanged.

Use the existing cross-platform fixture tests as secondary evidence:

- `crates/experts/builtin/tests/personal_context.rs:29-51` — assert the Android People fixture is Personal and Android Wellbeing fixture is HighlySensitive.
- `crates/experts/builtin/tests/personal_context.rs:88-125` — stop treating the three evidence values as sensitivity-equivalent; assert each class explicitly before the existing raw-data non-leakage assertions.
- `crates/experts/builtin/tests/apple_wellbeing_projection.rs:5-30` — after `personal_context_evidence(&view)`, assert `HighlySensitive` before the existing forbidden-field assertions.

Do not move sensitivity ownership into `floe-experts-builtin` merely because these cross-language tests currently live there.

### 01-B — Make authorization and model projection preserve actual evidence sensitivity

CP01 must close both current downgrade sites: the common Expert allow-list and the projection's direct copy of caller classes.

#### 01-B1 — Scope the common Expert policy by admitted manifest sensitivity

Primary files:

    crates/app/src/vault_host/conversation_turn/expert_host.rs
    crates/app/src/vault_host/conversation_turn/expert_dispatch.rs

At `expert_host.rs:39-48`, replace the zero-argument:

    expert_policy()

with a helper taking the admitted package sensitivity, conceptually:

    expert_policy(package_data_class: DataClass)

Build a deterministic allowed class list with these semantics:

1. preserve `Personal` because delegated Expert context may include Persona, Memory, Calendar/day or other Personal evidence;
2. add the admitted manifest's `data_class` when it is not already present;
3. sort and deduplicate the list;
4. do not silently remove `Credential` or `DeviceOnlyRaw` if a malformed/untrusted manifest somehow declares them — existing authorization must still fail closed rather than the helper sanitizing the policy.

For the shipped packages this yields:

~~~text
ordinary built-in Expert -> [Personal]
Wellbeing Expert          -> [Personal, HighlySensitive]
~~~

At `expert_dispatch.rs:310-343`, construct this policy from the exact Run-pinned registration already held by the endpoint:

    self.registration.manifest.data_class

Do not look up `BuiltinExpertKind` or the live shipped catalog again. The admitted registration is the configuration truth for this Task.

Update every test/internal caller of `expert_policy()` to pass the intended fixture class explicitly. Personal-only tests should continue to pass `Personal`; add a focused assertion that Wellbeing's admitted manifest produces a policy containing both Personal and HighlySensitive.

This policy is an **allowance**, not the evidence classifier. An Expert manifest cannot convert Personal evidence into HighlySensitive or vice versa, and a Personal-only policy must still reject HighlySensitive evidence through the existing Context authorizer.

Keep the existing CP01-era values of:

    external_transfer_consent
    bounded_sensitive_projection
    allowed_placements

unless a compile-only mechanical signature update is required. In particular, **do not** set `bounded_sensitive_projection = true` for Wellbeing and do not grant remote transfer in CP01. The mandatory Health privacy transform does not exist until CP02/03.

#### 01-B2 — Fold effective classes from the actual Context projection

Primary file:

    crates/modules/context/src/application/model_projection.rs

Current anchors:

- `ContextProjectionInput.input_data_classes`: lines 47-60;
- `assemble_context_projection()`: lines 65-117;
- direct copy into `AuthorizedModelProjection`: line 114;
- `validate_input()`: lines 120-140;
- unit-test fixtures begin around line 250;
- test `input()` sets caller classes around lines 417-439.

Keep the existing field for this checkpoint to avoid an unrelated caller-wide rename, but change its meaning/documentation from “the complete model-input class list” to the caller's admitted/declared minimum or stricter class set.

After:

    let live = live_context(input.role, input.agent_context);

derive an effective class vector from the **post-role-filtered `live` context**, not from the unfiltered original `AgentContext`.

Add a small private helper equivalent in behavior to:

~~~text
effective_input_data_classes(declared, live):
  classes = copy(declared)
  classes += each live.evidence[*].data_class

  if live.persona exists or live.memories is non-empty:
      classes += Personal

  sort classes by DataClass canonical Ord
  deduplicate
  require 1..=MAX_INPUT_DATA_CLASSES
  return classes
~~~

Use that result for:

    AuthorizedModelProjection.input_data_classes

instead of cloning `input.input_data_classes`.

Important semantics:

- The fold is a **union**, not an intersection. A stricter caller declaration remains visible.
- Actual evidence can only add sensitivity; it cannot be hidden by a caller that declared `Personal`.
- Persona/Memory adds `Personal` only when that content is actually in the projected `live` context. Do not add it from an original context that the selected role filtered out.
- Do not infer classes from source handles, Expert IDs, Tool names, `view_id` strings or package names.
- Do not filter `Credential` / `DeviceOnlyRaw` out of the declared list. Existing policy/dispatch denial owns rejection.
- Do not place this fold in Inference or provider adapters. Context owns the assembled model input and therefore owns its effective class set.

#### 01-B3 — Projection regressions

Extend the existing `model_projection.rs` unit tests around lines 368-520.

Required cases:

1. Personal-only live evidence plus Personal declared input remains exactly `[Personal]`.
2. Declared `[Personal]` plus a live HighlySensitive evidence item produces `[Personal, HighlySensitive]`.
3. Wellbeing-like HighlySensitive evidence plus Personal Calendar/evidence produces the same canonical two-class set.
4. Duplicate/unsorted declared values plus actual evidence normalize to one deterministic sorted/deduplicated vector.
5. Persona or Memory in the projected role adds `Personal` even when the declared set is stricter and did not name Personal.
6. A role that removes live Persona/Memory/evidence does not acquire classes from content that is absent from its output envelope.
7. The resulting `AuthorizedModelProjection` still validates and preserves the existing coverage calculation.

Add one regression that directly proves a caller-declared `[Personal]` cannot make a projection containing HighlySensitive evidence report only Personal.

Do not change the fixed Manager grounding corpus/rubric as part of these tests.

#### 01-B4 — Preserve current authorization failures

Keep `crates/contracts/agent/src/context.rs:72-95` behavior:

- evidence class must be included in `InferencePolicyDecision.data_classes`;
- Persona/Memory requires Personal;
- `Credential` and `DeviceOnlyRaw` fail closed.

Keep and rerun `crates/modules/context/tests/policy.rs`, especially:

- the HighlySensitive placement/consent test;
- `raw_sources_and_credentials_stay_outside_agent_context_even_with_consent`.

Keep `crates/modules/access/src/application/model_dispatch.rs:117-133` unchanged in meaning. External HighlySensitive dispatch is still denied at this checkpoint. If the corrected Wellbeing path exposes that denial in an integration test, record it as the intended pre-CP02/03/04 fail-closed state; **do not** bypass it with local fallback, fake consent, class downgrading or a new exception.

### 01-C — Align Health connector descriptors with the shared semantic class

This sub-checkpoint changes descriptor meaning only. It does not add a Health transform or Android delivery work.

#### 01-C1 — Apple Health descriptor

Primary file:

    apps/client/ios/Runner/AppleContextChannel.swift

Current anchors:

- `connectionSnapshots(deviceID:)`: lines 192-223;
- Contacts call: lines 198-205;
- Health call: lines 206-214;
- Attention call: lines 215-221;
- shared `connectionSnapshot(...)` helper: lines 226-278;
- helper hard-codes `"data_class": "personal"` around line 271.

Make the shared helper accept a bounded `dataClass` argument rather than hard-coding Personal.

Pass:

~~~text
contacts.apple / people.identity    -> personal
health.apple / wellbeing.derived    -> highly_sensitive
attention.apple / attention.coarse  -> personal
~~~

Use the existing wire spelling `highly_sensitive`. Do not change Health retention, freshness, source handles, permission semantics or payload bytes in CP01.

#### 01-C2 — Android Health semantic descriptor only

Primary files:

    apps/client/android/app/src/main/kotlin/app/floe/floe_client/AndroidContextChannel.kt
    apps/client/android/fixtures/health_connect_snapshot.json

At `AndroidContextChannel.kt:641-653`, change only the Health `wellbeing.derived` descriptor's `data_class` from `personal` to `highly_sensitive`.

Update the matching fixture descriptor in `health_connect_snapshot.json`.

Do not alter Calendar/Contacts classes, add an Android local model, change Health Connect acquisition, or run Android build gates.

#### 01-C3 — Shared descriptor regressions

Primary Rust test:

    crates/modules/connections/tests/connected_context.rs

The existing `android_context_descriptors_conform_to_the_shared_contract` test around lines 362-405 parses Calendar, Contacts and Health fixtures but currently checks only connector/provider/execution/authority.

Extend it so the expected descriptor class is explicit:

~~~text
calendar.android -> Personal
contacts.android -> Personal
health.android   -> HighlySensitive
~~~

This proves the JSON fixture is interpreted by the shared Rust `ViewDescriptor.data_class: DataClass` contract, not merely that the string is present.

Update product-test fixture literals that model Health descriptors:

    apps/client/test/features/settings/settings_screen_test.dart
      _healthConnection: around lines 575-635
      _appleHealthConnection: around lines 704-743

    apps/client/test/features/connections/connector_screen_test.dart
      _appleConnection: around lines 1138-1205

Health fixture descriptors must use `highly_sensitive`; Personal connectors remain Personal.

For `_appleConnection`, derive the descriptor class from the provider so only `apple_health` becomes HighlySensitive. Do not opportunistically fix unrelated `apple_screen_time` fixture semantics in this checkpoint.

`apps/client/lib/features/connections/domain/agent_connections.dart` currently validates descriptor `data_class` structurally but does not retain it as a Dart domain field. Do **not** widen the product model merely for CP01 unless implementation proves a current product consumer genuinely needs the class. The Rust/shared descriptor and native snapshot remain the semantic source.

### 01-D — Ordered implementation sequence

Execute CP01 in this order to keep failures diagnostic:

1. **01-A contract truth**
   - add View-owned `data_class()`;
   - change `personal_context_evidence()`;
   - add People/Attention/Wellbeing classification tests.
2. **01-B1 authorization allowance**
   - make `expert_policy` manifest-class-aware;
   - pass the exact admitted `registration.manifest.data_class`;
   - update focused App tests/callers.
3. **01-B2 projection fold**
   - compute effective classes from declared classes + post-filter live context;
   - port/add model-projection regressions.
4. **01-C descriptors**
   - Apple Health `highly_sensitive`;
   - Android Health semantic descriptor + fixture only;
   - Rust/Flutter fixture assertions.
5. Run focused tests and fix only failures caused by this semantic cutover.
6. Run the CP01 residual audit.
7. Run the final affected-surface gates.
8. Record the checkpoint execution evidence in this plan and mark CP01 complete only after every required gate is either PASS or explicitly unavailable for an environment reason allowed by the repository verification policy.

Do not start CP02 while closing CP01.

### 01-E — Explicit non-goals / forbidden shortcuts

CP01 must not:

- add `HealthPrivacyTransformInput/Output` or any FoundationModels Health operation;
- mark pre-transform Wellbeing as Gateway-safe;
- add transform provenance;
- modify `ProcessingRestriction` or source grant meaning;
- delete recipient-consent code;
- change `allow_external` / `expected_recipient`;
- change model routing order or fallback semantics;
- set Wellbeing `bounded_sensitive_projection = true` merely to make remote dispatch pass;
- change Access's current external HighlySensitive denial;
- use local inference as a permission/classification bypass;
- downgrade transformed or untransformed Health back to Personal;
- create a generic sensitivity/transform framework;
- add Android parity/build work;
- bump wire/schema versions solely for this internal semantic correction.

A failure caused by the newly correct HighlySensitive class is evidence of a later checkpoint boundary unless CP01 itself is incorrectly dropping/denying the class before the existing fail-closed remote fence.

### 01-F — Focused verification

Run narrow checks first.

Rust:

~~~sh
cargo test -p floe-context-contract --tests
cargo test -p floe-context --tests
cargo test -p floe-experts-builtin --tests
cargo test -p floe-connections --tests
cargo test -p floe-app --tests
cargo test -p floe-access --tests
~~~

If a package has no standalone integration target under `--tests`, use its current Cargo-defined test target rather than creating a new test executable solely for this checkpoint.

Flutter fixture/UI parsing:

~~~sh
cd apps/client
flutter test test/features/settings/settings_screen_test.dart
flutter test test/features/connections/connector_screen_test.dart
~~~

Because `AppleContextChannel.swift` changes, validate an Apple product build that compiles that Runner source using the repository's current supported environment. At minimum retain:

~~~sh
cd apps/client
flutter analyze
flutter test
flutter build macos
~~~

If a supported iOS simulator/device build is available in the local Xcode 26 environment, run the current repository iOS product build for that target. If no eligible runtime/signing/device prerequisite exists, record the exact reason as **UNVERIFIED**; do not change signing accounts, install runtimes, reset permissions or substitute an Android build.

Final Rust/shared-contract gate after all Rust inputs are final:

~~~sh
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

A successful targeted test does not replace the final workspace gate.

### 01-G — Residual audit before completion

Search the changed/current sources for the obsolete CP01 meanings, at minimum:

~~~text
personal_context_evidence
data_class: DataClass::Personal
"data_class": "personal"
"data_class" to "personal"
'data_class': 'personal'
expert_policy()
input_data_classes: input.input_data_classes.clone()
input_data_classes: call.policy.data_classes.clone()
input_data_classes: step.policy.data_classes.clone()
wellbeing.derived
health.apple
health.android
apple_health
health_connect
HighlySensitive
DeviceOnlyRaw
Credential
~~~

Classify every remaining match. Expected legitimate Personal matches include Calendar, Contacts, Attention, Mail and other Personal Views. The audit is complete only when every Health/Wellbeing descriptor/evidence path is HighlySensitive and no projection path can understate an included HighlySensitive evidence item.

Do not turn this one-time residual list into a permanent grep script.

### Acceptance

Checkpoint 01 is complete only when all of the following are true:

1. `PeopleView -> ContextEvidence::Personal`.
2. `AttentionView -> ContextEvidence::Personal`.
3. `WellbeingView -> ContextEvidence::HighlySensitive`.
4. The production Wellbeing Expert policy permits both its HighlySensitive evidence and any Personal context it legitimately carries, without using the manifest to relabel evidence.
5. `AuthorizedModelProjection.input_data_classes` is the canonical union of caller-declared classes and the actual post-role-filtered model input; Persona/Memory contributes Personal.
6. Declared `Personal` cannot downgrade live HighlySensitive evidence.
7. Wellbeing + Personal Calendar/context projects at least `[Personal, HighlySensitive]` in canonical order.
8. Credential and DeviceOnlyRaw remain rejected from generic Agent reasoning.
9. Apple and Android Health descriptors advertise `wellbeing.derived` as `highly_sensitive` while non-Health Personal descriptors remain unchanged.
10. The existing external HighlySensitive dispatch denial remains fail-closed; CP01 does not claim Health is ready for Gateway reasoning.
11. No Health privacy-transform, source-processing-policy, recipient-consent, routing or fallback work from CP02+ has started.
12. Focused and final required verification is recorded, with any unavailable Apple build gate called out precisely.

No Health-derived View may reach Inference or a provider transport while being represented only as Personal after this checkpoint.



## 6. Checkpoint 02 — Add the Health-only device-local privacy-transform contract

### Status

Checkpoint 02 is the active next checkpoint after CP01. Production implementation has not started at the refreshed baseline:

    fb6e36a160149cac6e5b1a4c02991a4d78b15544

Authoritative code-line execution detail:

- [02 — Health privacy-transform contract and FoundationModels host](reasoning-source-processing-convergence/02-health-privacy-transform-contract.md)

If `main` moves before implementation, refresh that document's line anchors and assumptions first.

### Goal

Introduce one narrow Health-owned typed semantic transform plus a separate device-local FoundationModels operation. Do not create a generic privacy-transform framework and do not cut the real HealthKit product path over yet.

~~~text
FloeAppleHealth
  -> typed HealthPrivacyTransformInput / Output / protocol
  -> deterministic post-transform Wellbeing projection seam

libfloe_local_model.dylib
  -> existing Agent/Learner LocalModelHost
  -> separate HealthPrivacyTransformHost
       -> independent job/lifecycle
       -> same physical FoundationModels capability
       -> no Agent prompt/tools/history/catalog
~~~

The Health transform must not share the existing Agent `LocalModelHost` job slot. Privacy transform and local Agent reasoning are distinct operations and must be able to coexist without replay/conflict aliasing.

### Frozen boundary

- Health owns transform input/output/protocol and fixed transform instruction.
- Transform input contains only bounded aggregate sleep/steps/exercise values.
- FoundationModels Health generation uses `tools: []` and a closed typed generated output.
- The local-model dylib exposes a separate Health command/ABI with its own start/poll/cancel/release state.
- The new Health transform path never calls deterministic capacity/recovery classification as fallback.
- CP02 does not change `HealthKitWellbeingProvider.readDerivedWellbeing()`. The old deterministic path remains only as the pre-CP03 product path.
- CP03 owns transformer injection, mandatory product use, legacy classifier removal from the product path, transform failure lifecycle and provenance.
- Do not change processing authority, recipient consent, Gateway routing, Primary/Fallback routing, AppWire, Flutter product API or Android local-model behavior.

### Acceptance

- One source-owned typed Health transform contract exists.
- One independent FoundationModels Health operation exists and is deterministically testable without a live model.
- Health operation cannot receive Agent instructions, Conversation, Tools, Expert discovery, Persona, Memory, credentials or authority identifiers.
- Agent and Health local-model jobs have independent ownership/lifecycle.
- Every production/validation compile of `LocalModel.swift` also compiles the source-owned Health contract source.
- The real HealthKit provider remains pre-cutover and CP03 has not started.


## 7. Checkpoint 03 — Cut Apple Health acquisition over to mandatory local transform

### Goal

Make the real Apple Health source incapable of producing reasoning-ready Wellbeing without the local transform.

### Primary files

- apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthKitWellbeingProvider.swift
- apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/WellbeingProjection.swift
- apps/client/apple/FloeAppleHealth/README.md
- apps/client/ios/Runner/AppleContextChannel.swift
- apps/client/macos/LocalModel/LocalModel.swift
- Apple/native build and validation inputs only if required by the chosen composition

### Product constructor

HealthKitWellbeingProvider.currentHostProvider must no longer be able to create a reasoning-ready Health source without a real privacy transformer in product composition.

Tests may inject a fake transformer explicitly.

Do not hide an optional transformer that falls back to the old reducer when nil.

### Successful path

~~~text
HealthKit authorization
  -> bounded read
  -> AppleHealthAggregate
  -> HealthPrivacyTransforming
  -> validated typed result
  -> WellbeingView
  -> local publication
~~~

### Failure path

If the transformer is unavailable or fails:

- do not publish a new WellbeingView;
- clear/expire any in-flight value according to the existing source lifecycle rules;
- report source unavailability;
- never send the aggregate to Gateway reasoning;
- never substitute deterministic capacity/recovery classification.

A previously cached WellbeingView may only remain usable under its ordinary freshness/provenance rules. Transform failure must not extend its expiry or represent a fresh observation.

### Health transform provenance

Introduce the smallest owner-appropriate typed evidence required so downstream admission can distinguish a valid post-transform Health projection from an old/pre-transform one.

Conceptually:

~~~text
PrivacyTransformEvidence::DeviceLocal {
  policy: health_wellbeing
  policy_revision: 1
  contract_revision: 1
}
~~~

Exact type placement is an implementation decision for this checkpoint, but the semantic owner is the source/Context provenance path, not the LLM and not Flutter.

The evidence must:

- be generated only after successful typed transform;
- be retained with the source dependency/projection needed by dispatch reauthorization;
- not be user-editable;
- not be model-generated;
- not be shown as a primary product field;
- participate in stale/validation checks as needed.

### Android

Do not implement Android privacy transformation.

Shared downstream logic must reject Health-derived reasoning without the required transform evidence. Dormant health.android code/fixtures may remain otherwise unchanged after their DataClass correction.

### Acceptance

- Apple Health cannot produce a new reasoning-ready WellbeingView without local transform success.
- Health raw data and aggregate do not cross the source boundary.
- No deterministic semantic fallback remains in the product path.
- Existing HealthKit authorization semantics remain distinct from local-model availability.

## 8. Checkpoint 04 — Replace exact model-recipient authority with source-owned processing policy

### Goal

Move external-processing authorization to the source/connection boundary and retire the model-recipient-consent vertical.

### 04-A — Replace ProcessingRestriction

Primary contract:

    crates/contracts/context/src/lib.rs

Replace:

~~~text
LocalOnly
ApprovedRecipient { recipient, categories }
~~~

with boundary-oriented semantics equivalent to:

~~~text
DeviceOnly

GatewayAllowed {
  categories
}
~~~

Name choices may differ if current module terminology requires it, but the meaning above is fixed.

GatewayAllowed must not carry provider/model/profile/exact recipient.

Update:

- GrantScope validation;
- ContextDependency processing fields;
- policy digest generation;
- native/remote grant serialization;
- review snapshots;
- Vault persisted grant meaning;
- server-side signed source/grant verification where it currently compares ApprovedRecipient;
- tests and fixtures.

Because Floe is pre-stable, use a direct schema/meaning cutover. Do not retain old ProcessingRestriction decoding merely to keep disposable local data.

### 04-B — First-party policy

Update crates/app/src/first_party_observe.rs so current first-party source policies explicitly choose the new processing boundary.

The implementation checkpoint must document the shipped defaults it chooses.

This plan does not require local privacy transforms for non-Health connectors. Their existing bounded Views may be GatewayAllowed according to product policy.

Health remains special:

    GatewayAllowed alone is insufficient;
    valid Health device-local transform evidence is also required.

Do not automatically widen an existing DeviceOnly grant after the meaning changes. The current connection review/policy digest must force review where the saved processing policy no longer matches.

### 04-C — Model dispatch authority

Refactor:

    crates/modules/access/src/ports/model_dispatch.rs
    crates/modules/access/src/application/model_dispatch.rs

Access should authorize a Device or Gateway processing boundary, not a concrete model route.

Inference may retain an internal profile ID for prepared-transport integrity, but that profile ID does not participate in user consent or source grant identity.

Gateway dispatch must:

1. validate the model projection;
2. reauthorize all source dependencies;
3. verify every source dependency permits Gateway processing for the categories actually used;
4. enforce Health's successful local-transform precondition when HighlySensitive Health evidence is present;
5. deny Credential/DeviceOnlyRaw;
6. fail closed for unknown/ambiguous provenance.

### 04-D — SourceAccess blocker for processing-policy mismatch

When a known source grant exists but does not permit the required Gateway boundary, return an owner-produced SourceAccess blocker, not a model-recipient blocker.

Add/rename a requirement reason equivalent to:

    ReviewProcessingPolicy

The requirement must identify:

- source identity;
- connector;
- connection;
- logical resource/View;
- consumer;
- purpose;
- expected current grant/source revision as required for safe review;
- required Gateway processing boundary.

It must not identify:

- model;
- profile;
- provider;
- Codex;
- downstream recipient.

The owning connection UI remains responsible for actual permission expansion.

### 04-E — Delete recipient-consent vertical

Delete, subject to a final usage audit:

- ProcessingRequirement;
- ProcessingSourceScope if it exists only for model-recipient review;
- RecipientLineage if it exists only to scope recipient consent;
- RecipientConsent;
- RecipientConsentState;
- RecipientConsentStore;
- ContextualRecipientAuthority;
- recipient_consent_id;
- recipient_consent_schema / recipient_consents persistence;
- recipient-consent Vault repository;
- model recipient approval commands/helpers;
- RecipientConsentTarget / ProcessingRecipient reviewed target;
- ApproveProcessingRecipient source requirement kind;
- model-consent test fixtures.

If a type has a second independent semantic use, narrow/rename it to that real owner instead of keeping model-consent terminology.

### Acceptance

A server/provider/model change behind the same authenticated Gateway does not change source authorization and cannot generate a user model-approval card.

## 9. Checkpoint 05 — Converge durable interactions and automatic resume

### Goal

Keep durable user review for source/configuration decisions, remove model review, and make post-resolution continuation durable and automatic.

### 05-A — Delete model-blocked interaction behavior

Remove:

- ModelCallOutcome::NeedsUserAction(ProcessingRequirement);
- MODEL_CONSENT_LIMITATION;
- publish_model_requirement / publish_model_blocker;
- ExpertModelHost.model_blocked ProcessingRequirement stash;
- "Model approval needs your review" built-in Expert paths;
- ProcessingRecipient interaction DTO/domain projection;
- model-consent AppWire fixtures and Flutter card code.

If model dispatch is blocked on source processing, the typed outcome is a SourceAccess blocker.

A transport/model failure remains an AgentFailure. Do not turn it into a permission card.

### 05-B — Preserve SourceAccess and ExpertBinding interactions

The original Run may still terminate without an Assistant answer when user action is genuinely required.

Do not persist a fake Assistant message such as the old MODEL_CONSENT_LIMITATION merely to explain the blocker.

Project the durable interaction/card separately from generated model output.

If Run-state/reporting needs a distinct blocked terminal reason to avoid reporting "Completed + generated reply", add the smallest Conversation-owned terminal outcome that preserves the no-live-waiting invariant from ADR 0030.

Do not hold a live Run open while waiting for a person.

### 05-C — Durable auto-resume

Current resolution triggers best-effort post-resolution auto-resume and Resolved projects Continue. Close the crash window.

Prefer reusing/extending the existing deterministic per-origin resume command and unique resume slot rather than creating a second orchestration system.

Required semantics:

~~~text
interaction group becomes terminal
  + at least one Resolved
  -> durable resume-required state/slot
  -> worker/reconciler claims idempotently
  -> fresh linked child Run
~~~

The durable state must survive:

- crash after authority mutation but before child submission;
- process restart/reopen;
- lost acknowledgement;
- duplicate Resolve;
- Resolve/Refresh race.

Exactly one child Run may win.

### 05-D — Remove product Continue

Delete routine product projection of:

    Resolved -> ContinueRequest

Delete the corresponding Flutter manual-continue path if no independent product use remains.

Backend/internal resume commands and deterministic resume IDs remain valid implementation machinery.

The user approves/repairs the source; continuation is automatic.

### Acceptance

A resolved source permission can never be stranded solely because the process died before best-effort resume. A normal user never needs to press Continue after Allow.

## 10. Checkpoint 06 — Remove model routing from Product/AppWire/Flutter

### Goal

Make the client structurally unable to choose or inspect a concrete model profile.

### 06-A — Conversation intent

Delete/narrow:

- ProfileSelection;
- StartTurn.profile;
- CanonicalTurnIntent.profile;
- profile contribution to the Conversation command digest;
- AdmittedExecution profile preference if it exists only to mirror user intent.

Conversation owns the user's semantic request, not a model choice.

The root product purpose is selected by composition/policy, normally everyday_assistance.

### 06-B — Protocol/FFI

Delete:

- AppProfileSelectionDto;
- ConversationStartTurn profile field;
- FFI mapping of explicit/auto profile selection;
- profile_id input accepted from ordinary product Conversation commands.

Do not replace it with a provider enum.

### 06-C — Flutter

Delete:

- AgentConversationTurnRequest.profileId;
- Prepared/start-turn profile plumbing;
- any product setting that exposes server-model/foundation-device as a normal user selection;
- Recipient/Profile/Consumer rows from the removed model-consent card.

Debug/evaluation model forcing must move behind an internal test/smoke seam rather than AppWire.

### 06-D — Error vocabulary

Replace product-facing strings that mention:

- selected model adapter;
- server-model;
- model route;
- profile;
- exact provider/recipient.

Use product boundary language such as Floe Gateway / reasoning service when needed.

Internal logs and server operator UI may retain detailed diagnostics.

### Acceptance

Repository search over apps/client product code and AppWire must find no ordinary user-turn field that accepts or displays a model/profile/provider/recipient.

## 11. Checkpoint 07 — Clean the App ↔ Gateway inference protocol

### Goal

Make the paired Gateway the external reasoning boundary and keep downstream provider routing server-owned.

### 07-A — Purpose discovery

Current GET /v1/inference-purposes exposes:

- available;
- requires_external_consent;
- placement;
- recipient.

Reduce the product contract to purpose availability. If an explicit Gateway boundary marker is useful, it may say Gateway, but it must not reveal downstream placement/provider/recipient.

Update:

- server/internal/inference/gateway.go;
- Rust server model inventory decoder;
- Flutter LocalServerClient purpose DTO if it still participates in product settings;
- tests/fixtures.

### 07-B — Generation request

Remove client-supplied model-recipient controls from the paired Gateway request:

- allow_external;
- expected_recipient.

The authenticated/pinned Gateway decides its internal provider route from the product purpose.

Keep:

- purpose;
- bounded data classes;
- Agent instructions/input/output/replay semantics;
- authenticated paired client identity;
- request limits/audit.

### 07-C — Audit projection

Server/operator audit may retain provider/model/downstream details internally.

Client privacy activity should project only product-meaningful facts, e.g.:

- purpose;
- data classes;
- Gateway processing fact;
- outcome;
- time/trace identity where useful.

Do not expose the concrete downstream recipient merely because the server logs it.

### 07-D — Security regression

Verify that removing expected_recipient does not permit arbitrary client endpoints. Saved-connection admission, loopback/bounded pairing setup, pinned server identity, bearer handling, and current credential checks remain intact.

### Acceptance

The client can change neither provider nor exact recipient. A server route change requires no client authority mutation.

## 12. Checkpoint 08 — Converge Inference on one Primary/Fallback selector

### Goal

Create one routing path that Manager, Expert, and Learner all share.

### 08-A — Preserve profile-observation failures

Change ModelProvider::observe_profiles from:

    Future<Output = Vec<PreparedModelProfile<_>>>

to:

    Future<Output = Result<Vec<PreparedModelProfile<_>>, AgentFailure>>

Propagate through InferenceAvailability and all providers/tests.

Server observation must distinguish at least:

- no saved Gateway connection;
- requested purpose truly unavailable;
- credential expired/unauthorized;
- invalid inventory;
- server unavailable/transport error.

Do not convert the latter failures into "no candidates".

### 08-B — Add selection tier

Add runtime-only candidate metadata equivalent to:

~~~text
ModelSelectionTier
  Primary
  Fallback
~~~

The server/Gateway leg is Primary.

The device Foundation leg is Fallback.

Same-tier selection may keep deterministic placement/profile validation as needed, but there must be no hidden "local-first" rank for the default reasoning policy.

### 08-C — Fallback semantics

Auto selection:

~~~text
if at least one valid Primary exists
  -> consider Primary only

else if Primary absence is a valid capability absence
  -> consider Fallback

else
  -> return the Primary observation/admission failure
~~~

Once a Primary candidate is selected/dispatched:

- no local fallback on transport failure;
- no local fallback on credential failure;
- no local fallback on policy/source denial;
- no local fallback on quota/deadline/cancellation unless a future explicit product policy says otherwise.

### 08-D — Provider composition

Replace RootModelProvider with a role-neutral composition, recommended semantic name:

    ReasoningModelProvider

It should compose:

- optional ServerModelProvider as Primary;
- FoundationModelProvider as Fallback.

It is parameterized by purpose/consumer scope; it is not root-only.

Secrets remain in prepared transports.

### 08-E — Delete stale router

Delete if still unused by the canonical service:

- crates/modules/inference/src/application/router.rs;
- InferenceRouter;
- RouteRequest;
- PlannedRoute;
- RecipientConstraint;
- RoutePlanError;
- exports/tests that only preserve that parallel route planner.

There must be one canonical candidate planner.

### Acceptance

One set of candidate rules governs execution and availability for all reasoning callers.

## 13. Checkpoint 09 — Converge Manager, shipped Experts, and Learner

### Goal

Remove caller-specific placement policy and make all three reasoning roles use the shared Primary/Fallback behavior.

### 09-A — Manager

Manager composition:

    purpose  = everyday_assistance
    consumer = conversation.root
    Primary  = Gateway
    Fallback = Foundation

Delete product profile preference. Internal eval forcing does not travel through Conversation.

Required cases:

- Gateway available -> Gateway;
- no saved Gateway -> Foundation;
- purpose unavailable -> Foundation;
- credential expired -> error, no fallback;
- inventory invalid/unreachable -> error, no fallback;
- source processing review required -> SourceAccess interaction, no fallback.

### 09-B — shipped Experts

Remove shipped package differences where one Expert says Any, another DeviceOnly, another RemoteOnly solely because of the old reasoning/privacy topology.

All shipped built-in Experts use default reasoning:

    Gateway Primary
    Foundation Fallback

Generic expert abstractions may retain explicit DeviceOnly or GatewayOnly constraints for third-party/special deployments, but shipped built-ins do not use them unless a separate non-privacy semantic requirement remains and is documented.

Remove/narrow:

- BuiltinExpertKind::supports_device_model();
- shipped AgentCard placement differences derived from it;
- current FocusAttention DeviceOnly requirement;
- Communication/Commitments/WorkContext/LifeLogistics RemoteOnly requirements;
- Schedule/Relationships/Wellbeing Any requirements as a routing policy.

Availability/eligibility must use the same shared selector as execution.

### 09-C — Expert source semantics

A Wellbeing Expert may use Gateway reasoning only after:

- source authority admits the Wellbeing View;
- Health local transform evidence is present;
- processing policy admits Gateway.

If Gateway reasoning is unavailable at planning time, local reasoning fallback consumes the same already-transformed WellbeingView. It never receives the pre-transform aggregate.

### 09-D — Learner

Replace:

    governed-memory-review
    FoundationModelProvider only
    DeviceOnly constraint
    LearnerModel::placement() == DeviceLocal
    LearnerRecipientAuthority

with:

    purpose  = deep_work
    consumer = knowledge.learner
    ReasoningModelProvider
    Gateway Primary
    Foundation Fallback

Knowledge keeps ownership of the Learner prompt, memory proposal schema, budgets, and job lifecycle.

Do not add a special background model-recipient grant. Model recipient consent no longer exists.

If Learner data later requires source-specific processing authority beyond its current stored-memory semantics, solve that at the data owner, not with a provider grant.

### Acceptance

Manager, shipped Expert, and Learner differ only in task/purpose/consumer/prompt and role-specific contracts, not in hidden provider topology.

## 14. Checkpoint 10 — Health × reasoning integration and recovery verification

### Goal

Prove the two planes remain independent under every relevant availability combination.

Required integration scenarios:

### A. Gateway available, Health transformer available

~~~text
Health raw
  -> local transform
  -> HighlySensitive WellbeingView + transform evidence
  -> Gateway Expert reasoning
  -> success
~~~

Assert no raw/aggregate Health payload reaches Gateway transport.

### B. Gateway available, Health transformer unavailable

~~~text
Health raw/aggregate
  -> transform unavailable
  -> source unavailable
  -> zero Gateway model calls
~~~

Do not ask for model approval and do not send the aggregate to Gateway.

### C. Gateway unavailable at planning time, Health transformer available

~~~text
Health raw
  -> local privacy-transform invocation
  -> WellbeingView
  -> independent local Expert-reasoning invocation
~~~

Assert the two local-model operations have distinct typed inputs and no cross-contamination of Agent context into the privacy transform.

### D. Gateway unavailable, local model unavailable

Health cannot transform and the Wellbeing Expert cannot derive a reasoning-ready Health view. Surface an honest limitation/unavailable source.

### E. Gateway available, Health source is DeviceOnly

Local transform may succeed, but Gateway dispatch must return a Health/source processing review requirement. Do not silently use local reasoning merely because local execution would satisfy DeviceOnly.

After user expansion to GatewayAllowed:

- source authority is revalidated;
- the original interaction resolves;
- automatic durable resume admits exactly one child Run;
- the child reacquires/revalidates current source state.

### F. Transform provenance stale/forged/missing

Gateway and local Agent reasoning must fail closed if the Health projection claims HighlySensitive Health data without valid source-owned transform evidence.

### G. Cached prior Wellbeing after new transform failure

A previous View is usable only until its existing expiry and under ordinary source/provenance continuity. A failed new transform cannot refresh timestamps or authority.

### Acceptance

The Health privacy plane remains mandatory regardless of which reasoning model ultimately runs.

## 15. Checkpoint 11 — Re-run Manager evaluation and decide grounding

### Goal

Evaluate the intended primary and fallback configurations separately before adding a new grounding architecture.

### Corpus

Keep the existing 22-case Manager guidance corpus and rubric byte-identical unless a separately approved product requirement changes the corpus.

Do not tune the rubric to fit either model.

### Primary Manager mode

Evaluate the Gateway/server reasoning configuration under the existing strict gate.

The question is:

    Is the configured primary reasoning model a full Floe Manager?

Existing hard epistemic/grounding criteria remain hard.

### Fallback Manager mode

Evaluate Foundation as a degraded fallback.

Hard safety floor must still reject:

- unsupported current/private factual claims;
- stale history presented as current observation;
- guesses presented as observations;
- unavailable treated as empty/all-clear;
- blockers presented as observed state;
- unobserved external success;
- explicit user no-delegate violation.

Quality metrics may remain non-zero:

- unnecessary delegation;
- redundant reacquisition;
- over-conservative limitation;
- suboptimal Expert consultation.

The question is:

    Is this a safe minimum fallback when Primary reasoning is unavailable?

not:

    Is this model equal to the Primary Manager?

### Grounding decision

- Primary strict PASS -> keep typed host-visible grounding contract deferred unless another concrete need remains.
- Primary strict FAIL -> design typed grounding as a new architecture task based on the post-cutover runtime.
- Fallback safety-floor FAIL only -> do not block the Primary architecture; mark Foundation fallback not Ready and design the smallest fallback-specific mitigation.

Do not reuse the pre-cutover exact-recipient/profile architecture in a new grounding design.

## 16. Checkpoint 12 — Product closure and old-plan rebase

### Goal

Finish the migration, rebase any still-valid closure work from agent-execution-environment-grounding, and remove migration-only residue.

### Bring forward only still-valid closure work

Examples:

- fresh Vault lifecycle;
- Run environment timing/identity;
- recovery;
- configuration drift;
- cache identity;
- source/grant continuity;
- product Conversation smoke;
- Gateway routing;
- Manager/Expert/Learner fallback selection;
- Health transform/runtime availability.

Do not bring forward obsolete closure requirements for:

- exact model-recipient consent;
- ProcessingRecipient cards;
- product profile selection;
- Model request Continue behavior.

### Current architecture/docs convergence

Update current architecture only to the final implemented ownership/path.

Archive/delete the execution plan according to repository documentation policy after operator acceptance; historical checkpoint evidence belongs in Git history, not a permanent active plan.

## 17. Residual deletion gate

Before declaring the architecture complete, search conceptually and by symbol.

The final product/current architecture should have no live matches for the obsolete model-consent/product-routing design except historical ADR text that is explicitly marked superseded.

Expected deletions/narrowing include, subject to final usage audit:

~~~text
RecipientConsent
RecipientConsentState
RecipientConsentStore
ContextualRecipientAuthority
recipient_consent_id
recipient_consent_schema
recipient_consents

ProcessingRequirement
ProcessingSourceScope        [if consent-only]
RecipientLineage             [if consent-only]

RecipientConsentTarget
ProcessingRecipient
ApproveProcessingRecipient

MODEL_CONSENT_LIMITATION
publish_model_requirement
publish_model_blocker

AppInteractionActionDto::ContinueRequest
AgentInteractionAction.continueRequest
manual conversation.interaction.resume product path

ProfileSelection             [product Conversation meaning]
AppProfileSelectionDto
AgentConversationTurnRequest.profileId

requires_external_consent
expected_recipient
allow_external               [paired Gateway model-routing contract]

RootModelProvider
LearnerRecipientAuthority
governed-memory-review
shipped Learner DeviceOnly route
BuiltinExpertKind::supports_device_model
shipped package-specific model placement rules

InferenceRouter
RouteRequest
PlannedRoute
RecipientConstraint
RoutePlanError
~~~

Internal provider profile IDs may remain inside Inference/provider prepared-transport integrity and server operator configuration. The residual rule is that they do not cross into product intent or user authority.

Also search current docs/UI strings for:

~~~text
exact-recipient consent
model approval
Model request
selected model adapter
server-model
recipient approval
~~~

and classify every remaining match.

Do not create a permanent grep script solely to remember this one migration.

## 18. Verification strategy

Use targeted validation during each checkpoint. Do not run the full workspace after every small edit.

### Rust iteration

Run affected crate tests first, e.g.:

~~~text
cargo test -p floe-context-contract --tests
cargo test -p floe-access --tests
cargo test -p floe-inference --tests
cargo test -p floe-conversation --tests
cargo test -p floe-experts-builtin --tests
cargo test -p floe-knowledge --tests
cargo test -p floe-app --tests
cargo test -p floe-protocol --tests
cargo test -p floe-ffi --tests
~~~

Select only the crates actually changed in the current sub-checkpoint.

### Apple Health/local model

Run the existing Swift package/native validation paths and extend them for the transform contract.

At minimum, final Health checkpoints must cover:

- FloeAppleHealth unit tests;
- local-model host tests;
- iOS build for the supported Apple target when implementation touches iOS product wiring;
- macOS local-model build because the same LocalModel.swift is shared;
- no Android build requirement.

### Go Gateway

When purpose inventory or agent request contract changes:

~~~text
cd server
go test -race ./...
go vet ./...
~~~

### Flutter/AppWire

When product DTO/UI/runtime changes:

~~~text
cd apps/client
flutter analyze
flutter test
flutter build macos
~~~

Run the current required iOS simulator/device build gate only when the repository's current Apple verification skill requires it for the changed native surface and the environment supports it. Record an explicit skip reason otherwise; do not substitute Android validation.

### Final Rust gate

Once, after all Rust inputs are final:

~~~text
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Follow .agents/skills/code-change-verification/SKILL.md for the final affected-surface matrix and evidence reuse rules.

### Required semantic regressions

The completed suite must prove at least:

1. ordinary Conversation uses Gateway without model-approval interaction;
2. no product request contains model/profile/provider/recipient;
3. server provider/model route changes do not invalidate source grants;
4. DeviceOnly source + Gateway Primary produces SourceAccess review, not local reasoning fallback;
5. source review resolution auto-resumes exactly once, including crash/reopen;
6. no normal Resolved interaction projects Continue;
7. Manager Gateway Primary / Foundation Fallback obeys planning-time-only fallback;
8. every shipped Expert obeys the same Primary/Fallback rule;
9. Learner deep_work obeys the same Primary/Fallback rule;
10. server observation credential/inventory errors are not misclassified as "no Primary";
11. Health Wellbeing evidence is HighlySensitive;
12. Health local transform is mandatory;
13. Health transform input contains no raw sample metadata or Agent context;
14. Health transform unavailability yields zero Gateway model calls;
15. Health aggregate never crosses the device boundary;
16. Gateway reasoning and local reasoning fallback both consume only transformed Health;
17. Health source processing permission remains separately enforced after transform;
18. Credential/DeviceOnlyRaw never enter generic Agent reasoning.

## 19. Checkpoint execution/report discipline

Before each checkpoint:

1. fetch latest main;
2. record starting HEAD and fetched origin/main-equivalent SHA;
3. confirm the previous checkpoint is complete;
4. read this plan section plus only the current source/architecture needed for that checkpoint;
5. do not reopen later checkpoint design unless the current code disproves a frozen assumption.

For each checkpoint implementation, report:

1. starting HEAD and final HEAD;
2. commit SHA(s) for the checkpoint;
3. changed owners/contracts and canonical path;
4. deleted obsolete surface;
5. targeted verification and exact results;
6. residual-search result and any deliberate bounded exception;
7. current architecture/ADR updates made in the same change set;
8. worktree cleanliness;
9. explicit statement that the next checkpoint has not started.

If the implementation discovers that a checkpoint's target would require a permanent compatibility layer, duplicated authority, provider details in a domain contract, or a second routing path, stop and revise this plan before proceeding.

## 20. Final target summary

The completed architecture is intentionally simpler than the current one:

~~~text
User / Product
  -> purpose
  -> Conversation / Expert / Learner
  -> Canonical Inference
       -> Gateway Primary
       -> device-local Fallback

Source owner
  -> source/system permission
  -> exact resource/grant authority
  -> processing policy: DeviceOnly | GatewayAllowed
  -> bounded typed View

Health owner only
  -> bounded deterministic acquisition
  -> mandatory device-local semantic privacy transform
  -> HighlySensitive WellbeingView + transform evidence

Context
  -> provenance/freshness/classification

Access
  -> source-processing boundary enforcement

Gateway
  -> provider/model/reasoning configuration

User review
  -> SourceAccess / ExpertBinding / Actions
  -> no model-recipient approval
  -> automatic durable resume
~~~

The key architectural correction is:

> Privacy minimization belongs to the source owner; model placement does not substitute for it. The product trusts the paired Floe Gateway as the external reasoning boundary, while the server owns concrete model routing. Health adds one mandatory device-local privacy transform before either Gateway or local Agent reasoning can consume its derived context.

## 21. Checkpoint 00 execution record — 2026-10-02

### Baseline and scope

- Starting main and fetched main-equivalent: 12881cafd620f58fb522167fdf8904d2feb2ab75, verified through the GitHub branch API.
- CP00 is documentation-only. No production Rust/Swift/Go/Dart, tests, manifests, schemas, credentials, accounts or stored user data were changed.
- The checkpoint commit is the commit introducing this record; its parent is the baseline above. Git history supplies the final SHA without a self-referential completion field.

### Completed decision cutover

- Added accepted ADR 0034 with the eleven required decisions, source/processing/Action separation, the Health-only boundary, common Primary/Fallback semantics and Primary-versus-Fallback evaluation consequences.
- Replaced the governing exact-recipient preservation instruction in AGENTS.md, the architecture-change skill, README and architecture invariants with verified Gateway identity and source-owned processing authority while preserving the other safety fences.
- Updated all four required product documents and the architecture runtime/modules/authority documents. Accepted targets are explicitly distinguished from current pre-cutover implementation; no Gateway-first, mandatory-transform or crash-durable-resume behavior is claimed as already shipped.
- Added scoped amendments to ADRs 0011, 0015, 0024, 0028, 0030 and 0033 without rewriting their original evidence. Updated the ADR and architecture indexes to route to the new authority decision and this single migration plan.
- Preserved the old agent-execution-environment-grounding file and its CP07 failure evidence. Its recipient-preservation and immediate CP08 instructions are superseded by this plan and ADR 0034, not another active sequence. Only still-valid closure work may be rebased at CP12.

### Verification and bounded residuals

- Reviewed the changed document contents and GitHub comparison against the baseline. The decision/invariant/product change set contains only Markdown paths; production behavior, fixed Manager corpus/rubric and historical CP07 evidence are unchanged.
- Reviewed the CP00-required document set and ADR cross-references. The six earlier ADR amendments identify which decisions are superseded and which source, identity, Run, provenance and Action safety properties remain.
- Residual terminology was classified within the changed guidance: new-decision explanations, explicitly superseded ADR rationale, explicitly marked pre-cutover runtime descriptions, and this plan's investigation/deletion instructions. These are not instructions to preserve the old model-consent authority. Existing production symbols remain deliberately untouched for CP01–CP09; this is not the final residual-deletion gate in section 17.
- No permanent grep checker, second execution plan or new runtime abstraction was introduced. Existing source/Action authority checks are not disabled or relaxed.
- Runtime/build suites were not run: CP00 changes no code, build inputs or runtime configuration. A local checkout was unavailable because container GitHub DNS resolution failed; local git diff --check and the Cargo dependency checker were not run. Verification used the connected GitHub file/tree/commit APIs and document/diff review, not an asserted local test pass.
- Worktree cleanliness is not applicable to this API-based edit; no local repository worktree was created or modified. Publish only as a non-forced fast-forward from the verified baseline, then verify main points to the resulting commit.

### Next boundary

Checkpoint 00 is complete. Checkpoint 01 has not started. The next implementation is Health sensitivity/model-input classification, not routing migration, recipient-consent code removal, or old CP08 grounding work.

## 22. Checkpoint 01 execution record — 2026-10-02

### Baseline, commits and scope

- Fetched `origin main` before implementation. Starting HEAD and fetched `origin/main` were both `90c7a6967e018a674f2f5d3e339d66508b0ced4b`; the worktree was clean. CP00 was already complete.
- Implemented in the prescribed order: 01-A View contract/evidence truth, 01-B1 Run-pinned Expert allowance, 01-B2 actual post-role-filtered input-class fold, 01-C native/fixture descriptors, then focused verification, residual audit and final gates.
- The checkpoint commit is the commit introducing this record; Git history supplies its final SHA without a self-referential field. Its parent is `cd2f49d1`, a separate user-requested removal of Flutter pixel golden comparisons, commented-out comparisons and unused golden images. Widget behavior/layout assertions remain. No push was performed.
- No stored payload/schema version, acquisition, retention, freshness, permission, source-grant, recipient-consent, route, Primary/Fallback, or Manager corpus/rubric meaning changed. Android changes are only its Health descriptor and matching fixture; no Android build was run.

### Changed owners and removed downgrade surface

- Context's typed `PersonalContextProjection::data_class()` statically classifies People/Attention as Personal and Wellbeing as HighlySensitive. `personal_context_evidence()` now uses that owner method, without changing serialized View bytes or provenance validation.
- App's `expert_policy(package_data_class)` creates the sorted/deduplicated allowance `[Personal, admitted manifest class]`. Production uses `self.registration.manifest.data_class` from the exact Run-pinned endpoint registration, never a new live-catalog lookup. Malformed Credential/DeviceOnlyRaw allowances are retained for authorization to reject, not sanitized. Existing placements, transfer consent and `bounded_sensitive_projection = false` remain unchanged.
- Context's single canonical assembler folds caller minimum/stricter classes with actual post-role-filtered evidence and Personal for projected Persona/Memory. The result is sorted/deduplicated and bounded; finalization does not inherit classes from discarded live context. Coverage and projection validation remain unchanged.
- Removed the common hard-coded Personal evidence label, zero-argument Expert policy and assembler's direct clone of declared classes. No compatibility path, new provider abstraction or product DTO field was added.
- Apple Contacts/Attention remain Personal; Apple Health now passes `highly_sensitive` through the shared descriptor helper. Android Health descriptor/fixture and Flutter Health fixture descriptors agree. Shared Rust descriptor parsing explicitly asserts Calendar/Contacts Personal and Health HighlySensitive.

### Exact verification evidence

All commands below completed with exit 0 on the final covered inputs unless the initial failure is explicitly identified:

| Command | Observed result |
| --- | --- |
| `cargo test -p floe-context-contract --tests` | PASS, 27 unit tests |
| `cargo test -p floe-context --tests` | PASS, 96 unit + 54 integration tests, including projection union/role/normalization and unchanged policy safety regressions |
| `cargo test -p floe-experts-builtin --tests` | PASS, 14 unit + 42 integration tests. Initial two Wellbeing fixture failures were PolicyDenied from their obsolete Personal-only allowance; changed only Wellbeing test calls to an explicit Personal/HighlySensitive allowance, retaining invented-evidence/diagnosis assertions |
| `cargo test -p floe-connections --tests` | PASS, 10 unit + 16 connected-context + 9 source-connection tests |
| `cargo test -p floe-app --tests` | PASS, 303 unit tests, 1 ignored, 6 integration tests; isolated subprocess unit check also passed. Includes admitted manifest allowance and Personal-only/forbidden-class rejection |
| `cargo test -p floe-access --tests` | PASS, 41 tests, including external HighlySensitive denial and forbidden-class denial |
| `cd apps/client && flutter test test/features/settings/settings_screen_test.dart` | PASS, 11 tests |
| `cd apps/client && flutter test test/features/connections/connector_screen_test.dart` | PASS, 15 tests |
| `cd apps/client && flutter analyze` | PASS, no issues; rerun after user-requested golden removal also PASS |
| `cd apps/client && flutter test` | Final PASS, 367 tests. Initial run: 366 passed, 1 failed, Expert Registry golden at width 520, 29px/0.02% difference. Reproduced the identical failure in an isolated `git archive` of starting HEAD with `flutter pub get` and the unchanged Registry test. User explicitly requested golden test deletion; separate commit removes pixel comparisons rather than rewriting images or weakening semantic assertions |
| `cd apps/client/apple/FloeAppleHealth && swift test` | PASS, 8 tests |
| `bash tools/validation/check-local-model.sh` | PASS, Swift host regression executable plus provider-adapter and Inference tests; local-model implementation unchanged |
| `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast` | PASS, including doctests and default example compilation. Run once after final Rust inputs; no Rust inputs changed afterward |
| `cargo build -p floe-ffi` | PASS |
| `python3 tools/architecture/check_boundaries.py` | PASS, 22 nodes, 103 edges, no errors/warnings |
| `cd apps/client && flutter build macos` | PASS, Release `floe_client.app`; native dylib rebuilt from the same Rust snapshot |
| `cd apps/client && flutter build ios --simulator` | PASS under Xcode 26.0.1 / iOS 26 SDK, `Runner.app`; compiles the changed AppleContextChannel Runner source. Available iOS 26 simulator runtimes confirmed with `xcrun simctl list devices available` |
| `git diff --check` | PASS |

Existing dead-code warnings in App (`admit_agent_action_dispatch*`, `Supplied`, `generic_source_label`, `blocked_text`) and Vault test `RefreshOutcome.reason` remain; no warnings were suppressed. Native builds are compilation evidence, not real-device Health permission/runtime acceptance; CP01 changes descriptor classification only.

### Residual audit and completion audit

- Ran the entire 01-G pattern set over current `crates` and `apps/client` sources/fixtures, excluding generated build/Pods/ephemeral artifacts; reviewed all 293 matches by semantic owner. Also audited all `AuthorizedModelProjection` construction sites: the only production constructor is Context's assembler; the other literals are owner/runtime/provider test fixtures or the contract definition.
- `expert_policy()` and `input_data_classes: input.input_data_classes.clone()` have zero live source matches. The two `call.policy`/`step.policy` clones remain deliberately as caller declarations into `ContextProjectionInput`, not final projection classes; both flow through the canonical union.
- Every typed Wellbeing evidence path uses the corrected owner contract. Production Apple/Android Health descriptors and Health descriptor fixtures are HighlySensitive. Personal residuals are Calendar, Contacts, Attention, Mail, Work/Day/confirmed-interaction evidence, or synthetic Personal test contexts. Routing tests with Health-named provider labels use generic `mail.communication` fixture descriptors, not Health payload/classification; the explicitly deferred `apple_screen_time` fixture View naming remains untouched, with its Personal class derived from provider as required by 01-C3.
- Health identifiers remaining in source selection, connection review, publication, native validation, freshness tests, supported View lists and prompts identify the existing View, not another classifier. HighlySensitive/Credential/DeviceOnlyRaw matches are canonical enum/serialization mappings, manifest declarations, owner regressions and existing fail-closed authorization/dispatch checks. Unrelated credential-storage/error names are not data-class downgrade paths.
- Regression evidence covers People/Attention/Wellbeing classes; actual HighlySensitive + Personal Calendar union; normalization and stricter declarations; projected Persona/Memory; finalization filtering; unchanged coverage; Personal-only and forbidden-class rejection. Access's external HighlySensitive fence and Context authorizer are unchanged and pass their existing safety tests.
- Updated `docs/architecture/modules.md` to describe current type-owned sensitivity, Run-pinned manifest allowance, canonical class union and the still-closed external Health fence. ADR 0034 rationale did not change; no ADR amendment was needed. Progress/evidence remains in this plan only.
- Worktree is clean after the checkpoint commit; generated golden failure images were removed, not committed. No push, account/signing change, provider mutation or local-data reset was performed.

### Next boundary

Checkpoint 01 is complete. **Checkpoint 02 has not started.** Health still has deterministic reduction only; no local privacy transform/provenance, processing-policy cutover, recipient-consent deletion, Gateway routing or Primary/Fallback change was implemented. External HighlySensitive model dispatch remains denied.
