# Health transform separation gap and proposed cutover

Status: **architecture and implementation approved by the user at 10:52 on 2026-10-03; exact contracts are being synchronized before source changes**. Revised on 2026-10-03 after the 09:50 shared Transform/DeviceModel clarification and the user’s 10:19 resolution of reasoning placement, with the earlier handoff retained as historical context. This document records the source comparison and approved direction. The user approved proceeding with the recommended structure; implementation begins only after the native and common DeviceModel contracts are synchronized. This document does not claim that those contracts already exist, compile or have passed behavioral checks. No compiler, build, formatter, analyzer, tests, native operation, provider operation, or data/credential operation was run for this review.

The coordinator reports that the original ChatGPT source `6abf3540-abf8-83ee-8882-6c013495822a` has been retrieved. The user's latest explicit clarification fixes the intended direction: **Transform is a shared inheritable interface for concrete HealthTransform and a future EmailTransform; FoundationModels is a peer backend to other local LLMs; domain/business logic consumes the same DeviceModel interface.** In Swift, conforming to a protocol with associated input/output types is the natural expression of that shared interface. The earlier version of this proposal was too restrictive when it excluded a common Transform interface. That interpretation is superseded here.

HealthTransform remains separate from **both** Health acquisition and model execution. A small shared Transform protocol is required by the clarification; a transform registry, discovery system, plugin framework, or generic orchestration engine is not implied. EmailTransform is an example of a future conformer, not an implementation in this scope. Apple and any future Android input mappings remain separate. The coordinator owns the full original-conversation comparison and coordinated implementation sequencing. Source facts below were read independently from the checkout. The newly verified historical requirements and stop context are attributed to the coordinator; this review did not independently retrieve the original transcript. The full retrieved artifact is still being assembled by the coordinator.

## Historical handoff and settled current policy

The coordinator verified an older Learner handoff specifying **ServerModelProvider + RemoteOnly + deep_work + knowledge.learner**, excluding FoundationModels. That conversation described FoundationModels as a hidden local minimizer and Manager local execution as a last resort when no server LLM existed. Those historical requirements explain why the recovered conversation initially appeared to conflict with current source. They are not the final current routing decision.

[Accepted ADR 0034](../decisions/0034-gateway-reasoning-and-source-processing-authority.md), dated 2026-10-02, explicitly replaced role-specific reasoning placement with one Gateway/server Primary and device-local Fallback selector for Manager, shipped Experts and Learner. It also retired model-recipient consent and Learner-specific background model grants, and retained `deep_work` for Learner. The coordinator reports that this accepted ADR predates the current refactor execution approval. The older handoff must not be restored as an isolated `RemoteOnly` override or background recipient grant.

**The user explicitly resolved the comparison at 10:19 on 2026-10-03: device models are prioritized for Transform; Learner, Manager and Experts all use remote LLM Primary with device Fallback. This routing decision is settled.** The user's 09:50 common Transform and peer DeviceModel clarification remains in force. A single device execution contract serves mandatory Health transformation and reasoning Fallback admitted by the common selector; interface reuse does not make the transform an Agent role.

Preserve ADR 0034's distinction between valid absence and failure: a valid observation showing no configured/admitted Gateway capability or unavailable purpose can permit device Fallback. Invalid credentials/identity/inventory, observation or transport errors, selected Primary failure/timeout/quota/cancellation, and insufficient source permission do not become valid absence or trigger a silent local retry. This proposal does not reopen or change those rules. Learner still uses `deep_work` and consumer `knowledge.learner`; the current `everyday_assistance` constant conflicts with both the older handoff and ADR 0034, independently of the now-settled routing decision.

The Health transform pipeline remains **deterministic reduction → hidden local semantic filter → typed Context**, without Agent context. Health's semantic transform is mandatory and device-local; failure cannot select a remote sanitizer or skip the semantic stage. Later reasoning on its sanitized result follows the common selector and current source-processing authority.

The coordinator also verified that CP07 Rev8's quality failure stopped further prompt tuning and CP08 progression and raised a question about the role definition. A common execution interface does not itself resolve that quality failure. ADR 0034 preserves the earlier Foundation result as historical evidence, explicitly distinguishes Primary and Fallback evaluation standards, and defers old CP08/automatic typed-grounding escalation. Preserve the original quality evidence and current evaluation plan separately from this ownership correction. The full retrieved artifact is forthcoming; this document attributes the historical account to the coordinator rather than claiming an independent transcript retrieval.

## Confirmed implementation gap

The current code separates the Health model invocation from ordinary Agent reasoning, but does not implement the required ownership split:

- [HealthPrivacyTransform.swift](../../apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift) imports FoundationModels directly. It contains the normalized input at line 10, output at line 40, Health-specific transformer protocol at line 122, job/receipt host at line 168, Foundation model availability at line 335, direct `LanguageModelSession` generation at line 367, digest at line 512, bundled ABI client at line 561, and C entry points at lines 611 and 625. Domain policy, SDK execution, receipt authority, and both ABI ends share this connector target.
- [HealthKitWellbeingProvider.swift](../../apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthKitWellbeingProvider.swift) stores `any HealthPrivacyTransforming` at line 20. `readDerivedWellbeing` at line 81 acquires HealthKit data, constructs transform input at line 99, calls the transformer at line 102, constructs the final source View at line 105, and updates the projection cache. Acquisition, mapping, transformation orchestration, and View publication are combined in the connector actor.
- [WellbeingProjection.swift](../../apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/WellbeingProjection.swift) owns the post-transform envelope builder at line 75. That builder correctly assigns source metadata after transform success; its responsibility must be preserved when moving it out of the SDK acquisition target.
- [FloeAppleHealth/Package.swift](../../apps/client/apple/FloeAppleHealth/Package.swift) currently declares one `FloeAppleHealth` target. Merely moving the transform file within that target would not establish the requested boundary.
- [AppleContextChannel.swift](../../apps/client/ios/Runner/AppleContextChannel.swift) constructs the connector with `BundledHealthPrivacyTransformer` at line 31. Its `readWellbeing` at line 169 compares the expected native subject before the call and checks continuity after it.

The current acquisition path is:

```text
Rust Context personal read
  -> NativePersonalDriver / registered PersonalBroker acquisition
  -> restricted Dart native-host pump
  -> iOS AppleContextChannel.readWellbeing
  -> HealthKitWellbeingProvider.readDerivedWellbeing
       -> bounded HealthKit acquisition and aggregation
       -> HealthPrivacyTransformInput
       -> BundledHealthPrivacyTransformer
            -> floe_health_privacy_transform(start/poll/...)
            -> HealthPrivacyTransformHost
            -> direct FoundationModels LanguageModelSession
       -> AppleWellbeingProjection + transform operation reference
  -> registered native completion
  -> independent Rust consume_receipt
  -> Context trusted observation and source/grant lineage
  -> Access admission before either Device or Gateway reasoning
```

Changing the concrete implementation name to `FoundationHealthPrivacyTransformer`, or injecting that Health-specific implementation into the connector, would retain the rejected coupling. HealthTransform must consume the shared DeviceModel execution contract. This reuse does not require sending pre-transform aggregates through the ordinary Agent reasoning request, tool loop, projection, or journal. The common backend abstraction and the separate source-transform invocation are compatible; model execution does not itself confer source or assistant authority.

## Existing local model facilities and their limits

[LocalModel.swift](../../apps/client/macos/LocalModel/LocalModel.swift) already has a `LocalModelHost` with an injected generator closure at lines 51 and 68. Its actual input at line 6 is instructions, prompt, and execution limits; its result is an Agent-style answer/call step. `foundationGenerate` at line 311 classifies the Learner prompt and otherwise reads Agent discovery/tool context. It is not yet the shared, domain-independent DeviceModel execution interface described by the user.

The `dynamicSchema` helper at line 643 translates object, array, string-enum, integer, number, and boolean schemas into FoundationModels schema types. It currently serves native tool argument schemas. This is useful implementation material for the FoundationModels backend of DeviceModel, but does **not** prove that the current host already supports arbitrary typed structured responses or that the abstraction is already shared with other local backends.

The separate runtime audit identified a concrete consequence of the current domain switch, confirmed by a narrow source read here. [LocalModel.swift](../../apps/client/macos/LocalModel/LocalModel.swift), lines 390–399, chooses the Learner structured path only when `run_frame.run_instructions.purpose` is `governed-memory-review`. [Knowledge learner.rs](../../crates/modules/knowledge/src/application/learner.rs), line 19, now defines `LEARNER_INFERENCE_PURPOSE` as `everyday_assistance`. [Context learner_projection.rs](../../crates/modules/context/src/application/learner_projection.rs), line 63, supplies that value, and [model_projection.rs](../../crates/modules/context/src/application/model_projection.rs), lines 86–87, copies it into run instructions. If the current Learner request reaches this native backend, it takes the general `GeneratedAnswer` path instead of the native Learner structured response. Rust still parses and rejects invalid domain output, so this is evidence of brittle domain-schema dispatch, not evidence that invalid memories are accepted. There are two concrete issues: `everyday_assistance` violates the required Learner purpose `deep_work`, and the SDK backend incorrectly infers a domain output schema from that purpose string. Removing the Learner-specific schema/role branch from FoundationModels is separate from the common selector’s routing policy. Learner’s structured output contract belongs to Knowledge and must be carried explicitly by whichever execution request the common selector admits, including a legitimately selected device Fallback. Restoring `governed-memory-review` or a `RemoteOnly` override would not be the correct fix.

The runtime owner also reports that Agent and Learner already share Engine → ModelPort → Inference, with the common handle assembled in [ready_generation.rs](../../crates/app/src/ready_generation.rs), line 62, and consumed by [learner_service.rs](../../crates/modules/knowledge/src/application/learner_service.rs), line 156. Preserve that convergence. The device Composite still selects FoundationModelProvider/PreparedFoundationTransport directly; the audit found no real alternate DeviceModel backend today. Its `RoleSpec` output contract is prose and `ModelCapabilities` only describes Chat, so the shared contract still needs an explicit structured-schema subset and typed response format. This Health document does not duplicate that full audit. Its design must join the same cross-language DeviceModel semantics for Swift Health and any Rust local consumer that routing policy actually authorizes, with an explicit adapter at the language/ABI boundary. All three reasoning roles preserve that shared selector under the now-confirmed remote Primary/device Fallback policy. Health’s mandatory local transformation is a separate use of the shared DeviceModel execution contract.

Duplicated backend policy has already diverged: `foundationModelAvailability` in [LocalModel.swift](../../apps/client/macos/LocalModel/LocalModel.swift), lines 291–309, requires OS major version exactly 26, whereas `healthTransformAvailability` in [HealthPrivacyTransform.swift](../../apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift), lines 335–347, permits OS 26 onward. This is evidence that the same SDK's backend admission policy is duplicated; this proposal does not choose which version policy is correct or edit either. DeviceModel backend capability/profile observation should be authoritative and shared, while domain-specific required capabilities remain with the caller.

Before implementation, the native design owner must verify the exact installed Apple SDK structured-response API: dynamic output schema generation, extraction of bounded generated JSON, cancellation behavior, availability requirements, and Swift 6 Sendable constraints. Do not assume a generic `Decodable` output also satisfies the SDK's `Generable` constraint. The executor adapter should translate the portable schema and strictly decode the result; Health must not acquire FoundationModels annotations to satisfy that constraint. Unsupported schema/API availability must fail closed.

## Target ownership and dependency direction

The responsibilities below implement the user's clarified direction. Package names and exact signatures remain proposals until the native design and the coordinator's separate Agent/Learner/ModelPort audit are reconciled. Do not create a second Health-only executor abstraction under a different name.

| Component | Proposed placement | Owns and depends on |
|---|---|---|
| Shared `Transform` interface | New SDK-free `apps/client/native/FloeTransforms` contract target | Associated typed input/output and one asynchronous transformation operation. No source IDs, receipts, model backend selection, registry, discovery, persistence, or platform SDKs. Concrete domains conform to it. |
| Shared `DeviceModel` interface and language bindings | One model-owner semantic contract; proposed SDK-free Swift binding in `apps/client/native/FloeModelExecution` plus coordinated Rust/native ABI bindings from the common model audit | Domain-independent typed request/output schema, capability observation/preparation, limits, cancellation and typed availability/failure. Generic inert tool-call proposals where required. One semantic contract for Swift Health, other authorized local consumers and peer local backends. Local reasoning is admitted only by the common selector; the backend itself grants no fallback authority. No Health/Email/Agent/Learner business policy, tool authority, source permission or prompt classification. |
| Concrete `HealthTransform` | New SDK-free `apps/client/native/FloeHealthTransform` target | Conforms to Transform for bounded Wellbeing input and a typed coarse Context payload. Owns platform-independent deterministic domain reduction, the hidden semantic-filter prompt/schema, limits and strict domain validation. Receives DeviceModel by injection. No HealthKit, FoundationModels, receipt minting, or routing imports. |
| `FoundationModelsDeviceModel` backend | A dedicated Apple adapter target, or approved new files beside the existing shared Apple `macos/LocalModel/LocalModel.swift` implementation | Implements DeviceModel using FoundationModels. Owns SDK availability, schema conversion, bounded SDK execution, cancellation and SDK error mapping. It is a peer of any other admitted local LLM backend, with no special domain authority or business logic. |
| Other local model backends | Their existing or separately authorized adapter targets | Implement the same DeviceModel contract and capability semantics. Their implementation is outside this Health slice; the interface must allow replacing FoundationModels without changing HealthTransform. No stub backend is needed to demonstrate the design. |
| HealthKit acquisition connector | Existing `FloeAppleHealth` target | HealthKit permission/acquisition, bounded window/sample handling, interval merging and numeric minimization. Returns an Apple acquisition value and SDK/lifecycle facts. It has no Transform or DeviceModel dependency and does not invoke models. |
| Apple Wellbeing input mapping and source-host assembly | A distinct `FloeAppleWellbeing` target, potentially in the existing Apple package | `AppleHealthWellbeingMapper` converts Apple acquisition values into HealthTransform input. A concrete source-host pipeline sequences acquisition, mapping, the bundled Health operation and post-success View construction. It depends on the connector and transform values, not a particular model backend. |
| Health operation host and C ABI | New files outside the connector target, for example `apps/client/apple/HealthTransformHost/` | Composes HealthTransform with an admitted DeviceModel handle; owns the separate Health job, cancellation/deadline state, receipts, tombstones and existing C symbols. Only this host can publish an independently consumable successful receipt. |
| Native composition | Existing app/native integration roots | Constructs the selected DeviceModel backend and injects the same contract into domain consumers. Binds the mandatory local transform to a local executor and composes the single Health receipt host. Selection remains outside business logic. |

The target layers and call graph are:

```text
HealthKit connector -> Apple acquisition values
Apple input mapper -> normalized HealthTransform.Input
HealthTransform -> shared Transform contract
HealthTransform -> shared DeviceModel contract
future EmailTransform -> shared Transform contract + shared DeviceModel contract
FoundationModelsDeviceModel -> shared DeviceModel contract
other local LLM backend -> shared DeviceModel contract
Manager / Experts / Learner -> common ModelPort/Inference -> server Primary
Manager / Experts / Learner -> common ModelPort/Inference
  -> valid-absence device Fallback -> device adapter -> shared DeviceModel binding
native composition -> chosen DeviceModel backend + HealthTransform + Health ABI host
Apple source-host assembly -> connector + mapper + bundled Health ABI client
Rust native receipt reader -> existing Health ABI only
```

For Health, the domain pipeline within these layers is:

```text
bounded HealthKit acquisition -> Apple input mapping
  -> deterministic Health domain reduction
  -> hidden local semantic filter using DeviceModel
  -> strictly validated typed Health Context payload
  -> source host envelope + independently consumable receipt
```

The semantic filter receives only minimized domain input and its own transform instructions/schema. It has no AgentContext, Conversation, Persona, Memory, discovery catalog or callable tools. The typed Context payload means the sanitized domain value, not the Agent reasoning envelope. The source host still supplies provenance and publication metadata after success. Platform-specific SDK acquisition/aggregation stays with the connector or mapper as appropriate; platform-independent domain reduction belongs to HealthTransform. Exact relocation of the existing HealthKit aggregation routines is an implementation design detail, not permission to remove acquisition bounds. Deterministic reduction is an earlier stage, never a successful fallback when the mandatory semantic filter fails.

The Email line expresses a future extension point only. The user prioritizes DeviceModel for Transform generally, but this does not establish a universal local-only policy for every future transform. Future domain execution policy requires its own decision; this proposal neither adds remote transform fallback nor implements Email. There is no Email target, prompt, connector, registry entry or backend implementation in this proposal. The Apple mapper is independent of any future Android mapper; the concrete HealthTransform input contract is shared across those mappings without importing either acquisition SDK.

The source-host assembly is concrete Wellbeing acquisition orchestration. It does not become another permission owner. Current Connections/Access/Context admission remains responsible for source authority. The native design must specify how the SDK-only connector reports permission/read limitations while the source host retains projection freshness/cache state; leaving the current View cache inside an allegedly acquisition-only connector would leave part of the coupling unresolved.

### Small shared interfaces

A minimal Swift expression of Transform is:

```swift
protocol Transform: Sendable {
    associatedtype Input: Sendable
    associatedtype Output: Sendable

    func transform(_ input: Input) async throws -> Output
}
```

HealthTransform conforms with concrete Health input/output types and receives DeviceModel through its initializer. A future EmailTransform can conform with different types. Protocol inheritance/conformance provides the user's shared abstraction; it does not require a base class, heterogeneous registry, runtime domain switch, or generic transform pipeline. The protocol also does not require every conceivable transform to carry a model field. The concrete model-backed domains depend on DeviceModel.

A candidate structured execution surface on the **shared DeviceModel** is:

```swift
protocol DeviceModel: Sendable {
    func availability() -> DeviceModelAvailability
    func generate<Input: Encodable & Sendable, Output: Decodable & Sendable>(
        _ request: DeviceModelRequest<Input, Output>
    ) async throws -> Output
}
```

These are design sketches, not compiled or frozen contracts. The DeviceModel sketch shows only structured generation; it does not settle the complete shared request/result or preparation API. The complete common DeviceModel surface, Swift existential or generic injection strategy, and corresponding Rust/ABI bindings must be reconciled with the other model audit before implementation. Swift Health and any authorized Rust local consumer must bind to one semantic execution contract, rather than each acquiring a similarly named but independent port. Learner can reach that contract only as a device Fallback legitimately selected by the common reasoning policy, just as Manager and Experts can. Health must use that common interface rather than introduce a parallel Health-specific execution port. Existing Agent `ModelPort` remains the higher authorized reasoning facade: Access plan/admission and permitted server/device selection remain there, and an authorized device transport adapts that facade to DeviceModel. Preserve the settled remote Primary/device Fallback policy for all reasoning roles and its valid-absence admission rules. HealthTransform receives a device-only capability with no remote selection. This sketch does not remove those controls or put source authorization into the backend.

`DeviceModelRequest` carries instructions, typed input, a portable bounded output-schema value and explicit token/output/deadline limits. The schema representation must be Sendable, not an unchecked `[String: Any]` crossing tasks. The shared execution result may also represent generic inert tool-call proposals required by Agent clients; those proposals carry no authority to execute tools. Runtime retains call validation and execution. DeviceModel must not import a Health- or Learner-specific enum/schema definition. Domain owners supply schemas as request data. Generic encoding/schema enforcement belongs to the execution contract/adapter; Health-specific field limits, prompt, categories and semantic validation belong to HealthTransform. Prompt text, package IDs or data inspection must never cause the FoundationModels backend to select Health/Email/Agent/Learner business logic. The other owner is auditing the current purpose bug, background lifecycle and removal of backend domain branches against the settled policy. This is a coordinated dependency, not a new placement override or work claimed complete by this Health proposal.

HealthTransform returns only validated coarse Health output. It does not accept source binding metadata in the model input and does not return a receipt. The Health operation host wraps a successful transform result with the independently verifiable operation/binding/digest/timestamp proof. The existing `HealthPrivacyTransforming` protocol combines transformation and proof transport; split those responsibilities rather than promote that exact Health-only signature into the shared Transform contract. The bundled ABI client is a host operation transport, not a model backend or domain transform implementation.

The concrete Health transform performs its deterministic reduction before the required hidden local semantic filter and keeps a strict output decoder that rejects extra fields and unknown enum values. Its input remains only optional finite, nonnegative sleep hours, steps and exercise minutes with the existing bounds and at least one available signal. It requests the necessary structured-output capability through DeviceModel, runs device-locally, and fails closed when the selected backend cannot satisfy it. Backend interchangeability does not authorize Gateway fallback, skipping the semantic filter after deterministic reduction, or permission weakening. No Android implementation, placeholders, build work or polymorphic platform switch is added.

## Feasibility, agreement, and migration risk

The Health ownership extraction is a bounded Swift target/composition/adapter change, with the Rust source authority path preserved. Completing the clarified system-wide DeviceModel boundary also requires coordinated Rust device-transport and cross-language contract work identified by the other model audit; the whole correction cannot be described as only a Swift file move. The current implementation already has a small typed Health input/output, a separate Health job, a native receipt reader, and strong downstream checks. Those pieces can be separated while retaining their existing wire meaning. This is a feasibility assessment from source, not a claim that a file move is sufficient or that the proposed SDK-free executor compiles today.

The shared Transform interface, common DeviceModel boundary and FoundationModels peer-backend role are now explicit user direction, not open decisions. The remaining conceptual decisions are:

1. The exact shared DeviceModel request/capability/result contract across domain clients and peer local backends: portable output-schema representation, supported schema subset, strict typed decoding, locality, limits, cancellation and failure semantics. A generic method declaration alone does not establish these properties. The exact FoundationModels dynamic structured-response API remains unverified in this review and could require revising the common request/adapter shape. That is a feasibility question about the backend, not a reason to move Health policy into it or retain a second executor interface.
2. The exact host/composition boundary: which native component owns acquisition orchestration, the Health operation singleton, proof transport and post-success envelope/cache state, and how it retains the selected DeviceModel handle through the operation. The existing independent Health ABI and receipt singleton are useful safety foundations to preserve. Their new module placement and cross-language construction still need to be frozen against the full model audit; a separate physical binary is not implied by the user's interface clarification.
3. The concrete migration of admitted local consumers to the shared DeviceModel contract, while removing Health/Agent/Learner domain decisions from FoundationModels and preserving the already shared Engine/ModelPort/Inference handle. The routing policy itself is settled: all reasoning roles use server Primary/device Fallback; Health transformation is device-local. Remaining work is implementation design and current-purpose/lifecycle auditing, not choosing a Learner-specific route or restoring background recipient grants. A Health-only extraction cannot be presented as completion of the common backend migration or quality work.
4. Whether implementing a second real local backend belongs to the current scope. The checkout has no such alternate backend today. Defining a substitutable interface and adapting FoundationModels establishes the architecture; it does not prove or deliver operational support for another backend. Name an actual backend and its required capabilities only if that implementation is authorized.

Package, target, file, and class names are routine implementation choices once those responsibilities and dependency directions are agreed. Keeping the current physical dylib is the recommended minimum. Nothing established by this source review demands a separate dylib at the user level; require that only if the original final direction or a concrete linkage/lifecycle constraint does. A separate binary would add loading, signing, deployment, and same-image receipt verification work without by itself fixing semantic coupling.

The main migration risks are:

- A second copy of the Health receipt singleton could be linked into Runner through SwiftPM while Rust continues loading the dylib copy. Typed output alone would then appear successful but independent receipt consumption would fail. Keep the authoritative host instantiated only in the native ABI image.
- Removing FoundationModels from the transform and making it a peer DeviceModel backend can expose limitations in dynamic schema generation or strict output decoding. Do not solve that by moving Health semantics into the executor, adding an Agent prompt mode, or accepting unconstrained output.
- Separating acquisition from publication can accidentally change permission-limited/no-data semantics, before/after subject checks, cache expiry, confidence/evidence construction, or cancellation propagation. Freeze those existing meanings alongside the connector/host interfaces.
- Build scripts currently compile the Health source into both macOS and iOS local-model dylib builds, while the connector package also contains it. Real target boundaries require coordinated SwiftPM and Xcode/build-input wiring, not only renamed files. The new module paths must be present in native build cache inputs and linked from the same source snapshot.
- Moving the worker/receipt host can weaken deadline, late-completion, consume-once, or operation-reuse behavior. Preserve these as explicit host responsibilities; model execution success is not a receipt and never supplies source authority.

The user currently prioritizes reconstructing the original conversation context and stop reason, followed by detailed design discussion. The coordinator has now verified that CP07 Rev8's quality failure stopped prompt tuning/CP08 and questioned the role definition; the full source artifact and chronology remain forthcoming. The architecture correction addresses ownership and replaceability. It is not proof of improved Learner capability, quality or authorization, and those require separate adjudication. The 10:19 routing decision is settled and the user approved implementation at 10:52. The packages below remain preliminary until their exact execution contracts and exclusive file ownership are frozen; no further user approval is implied for ordinary implementation within that accepted scope.

## Minimal ABI and physical packaging recommendation

Keep the existing physical `libfloe_local_model.dylib` bundle and the separate `floe_health_privacy_transform` / `floe_health_privacy_transform_free` symbols. A single physical image can compose independent semantic modules. This avoids unnecessary changes to native loading, signing, bundle paths, and Rust lookup while fixing the ownership problem.

Preserve the closed Health operations `availability`, `start`, `poll`, `cancel`, `release`, and `consume_receipt`, the bounded native command/reply encoding, and the independent Health job/receipt state. Do not add a Health mode to the Agent model ABI, `learnerPromptClassification`, or generic model-step grammar. The minimal Health extraction does not itself require a new generic model C ABI: the Health host can receive the shared Swift DeviceModel directly inside the same native image. The separate common-model audit may identify broader ABI changes; do not freeze a conflicting Health-specific model ABI in advance.

[macOS build_native.sh](../../apps/client/macos/build_native.sh) currently compiles both source files into that image at lines 11–15. [iOS build_native.sh](../../apps/client/ios/build_native.sh) does the equivalent at lines 40–48. Their [macOS Xcode inputs](../../apps/client/macos/Runner.xcodeproj/project.pbxproj) and [iOS Xcode inputs](../../apps/client/ios/Runner.xcodeproj/project.pbxproj) list the current Health source at lines 261 and 220 respectively. The packaging cutover must compile/link the new modules into the same image and update those tracked inputs.

Do not duplicate the authoritative Health host singleton into the Runner-linked connector package. The production bundled client and Rust receipt consumer must continue reaching the same live receipt registry. SDK-free value definitions can be shared; live host authority cannot be copied.

## Provenance and fail-closed behavior to preserve

The shared DeviceModel contract must preserve independent operation identities and coexistence of authorized local reasoning and Health work. The common selector admits reasoning Fallback; the separate Health host admits its mandatory local transform. Neither can derive authority solely from sharing the backend interface. Sharing a backend/service contract does not mean merging the existing two host jobs into one slot or giving Health a dependency on Agent job state. Explicit per-operation resource admission may remain in the backend/host; it must preserve typed busy/unavailable outcomes and cancellation isolation.

The Health host retains an independent job slot, the 10-second deadline, 64-token response budget, bounded output, bounded receipt/tombstone storage, and exact replay/conflict behavior. Cancellation/release must not recycle the slot while an abandoned worker can still publish. Success is recorded only after the concrete HealthTransform validates the shared DeviceModel output. Neither connector input nor model output can assert success, timestamps, source handles, evidence, or authority.

The output digest remains exactly SHA256 of UTF-8 `floe.health.transform.v1`, NUL, capacity wire value, NUL, recovery wire value. Preserve the existing snake-case recovery value `needs_recovery`. A receipt binds transform operation identity to the original acquisition request ID, host epoch, Person, device, native subject, exact output digest, and actual completion/expiry timestamps. The existing maximum freshness is 30 minutes; failure cannot extend it or publish a fresh cached result.

The Rust readback is a separate proof step:

- [native/health_privacy.rs](../../crates/platform/native/src/health_privacy.rs), lines 46 and 55, loads the existing image and consumes a single-use receipt. Its exact-response validation must remain intact.
- [providers/sources/personal_native.rs](../../crates/adapters/providers/src/sources/personal_native.rs), lines 84–140, compares the receipt against the actual acquired Wellbeing view, original broker request identity, device/host/subject, output digest, and timestamps before returning `AcquiredSource`.
- [context-contract/health_transform.rs](../../crates/contracts/context/src/health_transform.rs), lines 13–55, defines serialized evidence as correlation only and validates output/freshness correspondence.
- [Context personal_lineage.rs](../../crates/modules/context/src/application/personal_lineage.rs), line 75, includes the transform operation, host, digest, observation and process in the query fingerprint. [Context personal_sources.rs](../../crates/modules/context/src/application/personal_sources.rs), lines 224–230, compares the stored dependency against the live observation and validates transform evidence. These later checks bind source/grant/resource/process facts that are not all fields of the native receipt itself.
- [Access model_dispatch.rs](../../crates/modules/access/src/application/model_dispatch.rs), lines 100–119, requires live transform evidence and HighlySensitive classification for Health before either Device or Gateway reasoning. GatewayAllowed is an additional independent processing check.

Raw samples and pre-transform aggregates stay inside the native source/transform/executor path. They do not enter Flutter product payloads, Agent Runtime, Context evidence, Conversation, Memory, diagnostics, or Gateway transport. The separate registered native-host callback still carries only the sanitized view and transform operation reference. Transformation never grants permission or declassifies Health.

## Bounded implementation packages after contract synchronization

These are preliminary cloud work packages for the approved implementation and the coordinator's requested Luna Max Fast delegation. They are not yet launch instructions: native design must first freeze exact target names, signatures, schema bytes, lifecycle values and the verified SDK response call. The exact Health contract/task documents will supersede these preliminary package descriptions. One worker owns each package's files; shared manifests/build integration stay with the native integrator.

1. **Shared contracts and concrete Health transform.** New files only in the approved `FloeTransforms`, `FloeModelExecution` and `FloeHealthTransform` roots. Implement the frozen small Transform protocol, common DeviceModel values/port and HealthTransform conformance with deterministic domain reduction, concrete input, hidden semantic-filter prompt/schema, and typed Context output validation. Coordinate DeviceModel ownership with the common-model worker so only one contract is created. No FoundationModels, HealthKit, host authority, Email or Android implementation. Return exact exports and dependency manifest requirements.
2. **FoundationModels DeviceModel backend.** New adapter file(s) in the approved Apple model target, using the exact SDK API confirmed by native design. Implement the same DeviceModel interface as other local backends. Reuse or extract the existing schema translation only under explicit ownership of that helper. Coordinate authorized local caller migration and removal of leaked Agent/Learner schema branches with the runtime owner. Preserve the shared server Primary/device Fallback policy, including a legitimately admitted Learner fallback and its explicit `deep_work` purpose. Do not restore the historical RemoteOnly/background-grant policy. Do not preserve business-specific prompt classification inside the final backend. No Health/Email policy, receipts or domain routing. If SDK support is missing, report the exact unsupported capability rather than returning unconstrained text as typed success.
3. **Health host and ABI extraction.** Own the old `HealthPrivacyTransform.swift` split and new `apple/HealthTransformHost` files. Keep ABI tags, digest, limits, single-use receipts, cancellation and replay behavior exactly; compose the concrete HealthTransform and supplied shared DeviceModel. Keep the same physical image. Do not edit the connector or Rust policy checks. Native integrator reviews the complete host before removing the old file.
4. **Apple acquisition and mapping cutover.** Own `HealthKitWellbeingProvider.swift`, the approved new Apple mapper/source-host target, and the View envelope/lifecycle relocation from `WellbeingProjection.swift`. Remove the connector's transformer dependency; keep the separate mapper between SDK acquisition values and HealthTransform.Input. Preserve all HealthKit acquisition/minimization/permission behavior and before/after subject checks. The host assembles the sanitized envelope only after authenticated transform success. No Android work.
5. **Native integration and build wiring.** Native integrator owns package manifests, `AppleContextChannel.swift`, both build scripts, and both Xcode project input lists. Compose the selected peer DeviceModel backend and concrete Health pipeline once, preserving one receipt registry image. Incorporate the separately agreed common-model integration rather than adding a Health-only backend selector. Rust `health_privacy.rs`, provider consumption, Context lineage, and Access fences should need no semantic edits if the ABI remains exact; any actual mismatch must be resolved explicitly, not bypassed with a new caller assertion.

After authorized implementation, qualification must cover Transform conformance and backend substitution without domain changes, deterministic reduction followed by mandatory hidden semantic filtering without Agent context, absence of domain-specific prompt branching in the backend, preservation of the common remote Primary/device Fallback policy and absence-versus-failure distinction, malformed typed input/output, unavailable model, timeout/cancellation, same-ID altered input, abandoned-worker completion, forged/replayed/expired receipt, wrong acquisition/person/device/host/subject/digest, both reasoning boundaries, and same-snapshot Apple package/dylib integration. These are pending acceptance requirements, not tests or checks run by this proposal.
