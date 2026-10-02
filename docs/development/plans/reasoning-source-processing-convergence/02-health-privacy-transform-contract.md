
# Checkpoint 02 — Health privacy-transform contract and FoundationModels host

- Parent: [reasoning/source-processing convergence](../reasoning-source-processing-convergence.md)
- Governing ADR: [ADR 0034](../../../decisions/0034-gateway-reasoning-and-source-processing-authority.md)
- Status: planned — CP01 complete; CP02 not started
- Refreshed baseline: `fb6e36a160149cac6e5b1a4c02991a4d78b15544`
- Scope: FloeAppleHealth contract, shared Apple FoundationModels host operation, native build/validation inputs
- Excluded: CP03 product cutover/provenance, CP04 processing policy, recipient-consent deletion, routing convergence, Android local-model implementation

## 0. Current defect and checkpoint boundary

CP01 made `WellbeingView` HighlySensitive end to end. External HighlySensitive dispatch is still fail-closed. Before routing can change, Health needs a device-local semantic minimization operation that is independent from Agent reasoning.

Current Health path:

~~~text
HealthKit
  -> 36-hour bounded acquisition
  -> AppleHealthAggregate
  -> AppleWellbeingReducer.reduce()
  -> AppleWellbeingView
~~~

Current local-model path:

~~~text
floe_local_model
  -> one LocalModelHost job
  -> Agent/Learner prompt parsing
  -> optional native tools/delegation
  -> model step
~~~

Do not put Health into that Agent grammar or job slot. The existing `LocalModelHost` has one in-flight job, so multiplexing Health through it would couple privacy transformation to Agent prompt/tool semantics and would make simultaneous Health-transform and local-reasoning work conflict.

CP02 creates the contract and separate host. CP03 later wires the real provider and removes the deterministic product classifier.

## 1. Refreshed source anchors

Baseline anchors; refresh if `main` moves.

### FloeAppleHealth

`apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/WellbeingProjection.swift`

- lines 74–78: `AppleHealthAggregate` already contains exactly sleep hours, steps and exercise minutes.
- lines 80+: `AppleWellbeingReducer` performs deterministic semantic classification.
- lines 83+: `reduce(...)` directly produces `AppleWellbeingView`.

`apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthKitWellbeingProvider.swift`

- lines 71–107: `readDerivedWellbeing()` reads a bounded 36-hour window and calls `AppleWellbeingReducer.reduce(...)` around line 89.
- Do not change this call path in CP02.

`apps/client/apple/FloeAppleHealth/Tests/FloeAppleHealthTests/AppleWellbeingProjectionTests.swift`

- line ~13 onward: current deterministic reducer tests.
- line ~101: no-signal behavior.
- Keep these as pre-CP03 behavior; add separate transform tests.

### Shared local model

`apps/client/macos/LocalModel/LocalModel.swift`

- lines 6–20: Agent `LocalModelInput`.
- lines 22–27: `LocalModelCommand`.
- lines 36–43: `LocalModelReply`.
- lines 50+: `LocalModelHost` and its single private job slot.
- lines 291–309: `foundationModelAvailability()`.
- lines 311+: `foundationGenerate(...)`, Learner classification and Agent tool path.
- lines 679–708: existing `floe_local_model` ABI and strict Agent decoder.

Health may reuse `foundationModelAvailability()` and `SystemLanguageModel.default` only. It must not call `foundationGenerate`, `learnerPromptClassification`, `nativeActionTools`, `currentUserRequest` or `dynamicSchema`.

### Direct local-model compile sites

Every place below currently compiles `LocalModel.swift` directly and must receive the new shared Health contract source:

- `apps/client/ios/build_native.sh`
- `apps/client/macos/build_native.sh`
- `tools/validation/check-local-model.sh`
- `tools/validation/run-local-model-smoke.sh`
- `apps/client/integration/product_conversation_test.dart`
- iOS and macOS Runner Xcode “Build Swift Libraries” inputPaths.

## 2. Final CP02 shape

~~~text
FloeAppleHealth
  HealthPrivacyTransform.swift
    HealthPrivacyTransformInput
    HealthPrivacyTransformOutput
    HealthPrivacyTransforming
    HealthPrivacyTransformFailure
    fixed Health transform policy/instruction

  WellbeingProjection.swift
    AppleHealthAggregate
      -> HealthPrivacyTransformInput

    AppleWellbeingProjection
      aggregate + typed transform output
      -> AppleWellbeingView
      -> timestamps/confidence/evidence handles

libfloe_local_model.dylib
  existing LocalModelHost
  separate HealthPrivacyTransformHost
    -> FoundationHealthPrivacyTransformer
    -> LanguageModelSession(tools: [])
    -> typed generated Health output

  floe_local_model / free
  floe_health_privacy_transform / free
~~~

FloeAppleHealth must not depend on Conversation, Agent Runtime, Inference, Experts or Knowledge.

## 3. 02-A — Source-owned Health transform contract

Add:

    apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift

`Package.swift` already includes source-directory files automatically. Do not create another target or package dependency.

### 02-A1 — Exact input

Define a public Codable/Equatable/Sendable input with only:

~~~text
sleep_hours: Double?
steps: Double?
exercise_minutes: Double?
~~~

Use explicit snake-case coding keys.

Canonical validation must reject:

- all three values absent;
- NaN or infinity;
- negative values;
- sleep greater than 36 hours;
- exercise greater than 2160 minutes;
- steps greater than 1,000,000.

Do not clamp. Invalid aggregate input fails closed.

No transform input may contain Person/device/source handles, timestamps, HealthKit sample/source metadata, evidence handles, grant/source authority, Conversation, Persona, Memory, Expert/Tool discovery, credentials or model/provider/profile identifiers.

### 02-A2 — Closed output

Define transform-specific closed enums:

~~~text
capacity:
  reduced | typical | strong | unknown

recovery:
  needs_recovery | typical | recovered | unknown
~~~

`HealthPrivacyTransformOutput` contains only those two values. No free-form explanation, confidence, timestamps, provenance, handles or authority.

The LLM does not construct `AppleWellbeingView`.

### 02-A3 — Protocol and failure vocabulary

Define:

~~~swift
public protocol HealthPrivacyTransforming: Sendable {
    func transform(_ input: HealthPrivacyTransformInput) async throws
      -> HealthPrivacyTransformOutput
}
~~~

Define a closed failure vocabulary that at minimum distinguishes:

~~~text
invalid_input
unsupported_os
unsupported_profile
device_not_eligible
apple_intelligence_not_enabled
model_not_ready
model_unavailable
invalid_output
deadline_exceeded
cancelled
~~~

Do not expose FoundationModels error types outside the adapter.

### 02-A4 — Fixed policy

The Health contract owns fixed transform settings, not callers:

~~~text
deadline: 10 seconds
maximum response tokens: 64
small fixed Health-specific request/reply byte limits
~~~

Do not accept reasoning effort, provider, profile, temperature, tools or caller instructions.

Freeze one source-owned instruction with this meaning:

> You are Floe's device-local Health privacy transformer. Use only the supplied bounded aggregate. Produce only the closed capacity and recovery categories. Do not infer diagnoses, medical conditions, identity, causes, or unrelated facts. Use unknown when the aggregate does not support a category.

## 4. 02-B — Deterministic seams around the transform

Primary:

    apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/WellbeingProjection.swift

### 02-B1 — Aggregate to transform input

Keep `AppleHealthAggregate` as the deterministic result of HealthKit preprocessing.

Add one conversion from exactly:

~~~text
sleepHours
steps
exerciseMinutes
~~~

to validated `HealthPrivacyTransformInput`.

No source handle, timestamp or evidence namespace crosses into the transform.

### 02-B2 — Typed output to Wellbeing View

Add a deterministic helper such as:

~~~text
AppleWellbeingProjection.project(
  aggregate,
  transformed,
  sourceHandle,
  observedAtUnixMs,
  evidenceHandle
) -> AppleWellbeingView
~~~

This host-side helper owns schema/version, `wellbeing.derived` ID, source handle, observed/expiry timestamps, 30-minute freshness, confidence and opaque evidence handles.

For the new transform path:

- both categories unknown -> confidence 0 and no evidence handles;
- otherwise each aggregate signal actually present may contribute its existing opaque window evidence handle;
- confidence remains deterministic, using the current coarse 400 + 100 per contributing signal capped at 700;
- aggregate numeric values never enter the final View.

### 02-B3 — Do not start CP03

Keep the current production call:

~~~text
HealthKitWellbeingProvider.readDerivedWellbeing()
  -> AppleWellbeingReducer.reduce(...)
~~~

until CP03.

The old reducer must not be called by the new transform contract, FoundationModels Health operation or error handling. It is a temporary pre-CP03 product path, not a fallback abstraction.

## 5. 02-C — Separate FoundationModels Health operation

Primary:

    apps/client/macos/LocalModel/LocalModel.swift

Compile alongside:

    apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift

### 02-C1 — Disjoint command grammar

Do not add Health fields or operations to `LocalModelInput`, `LocalModelCommand`, `LocalModelReply`, `floe_local_model` or `decodeLocalCommand`.

Add separate narrow Health command/reply types supporting only:

~~~text
availability
start
poll
cancel
release
~~~

`start` carries exactly `HealthPrivacyTransformInput`. Poll/cancel/release carry no input.

Strict decoding rejects unknown top-level/input keys. Agent fields such as `instructions`, `prompt`, `tools`, `maxResponseTokens` and `maxOutputBytes` are invalid Health input.

### 02-C2 — Independent host/job

Add a dedicated `HealthPrivacyTransformHost` with its own lock/job.

Required semantics match the proven lifecycle:

- exact same request ID + input rejoins;
- different request/input during an owned Health job -> conflict;
- deadline cancellation;
- sticky explicit cancel;
- release after completion clears the slot;
- release while running abandons/cancels;
- not-found/malformed fail closed.

Do not broadly genericize `LocalModelHost` merely to remove duplication.

Add a regression where an Agent `LocalModelHost` job and a Health job are both pending simultaneously and neither conflicts with the other.

### 02-C3 — FoundationModels adapter

Add a private `FoundationHealthPrivacyTransformer` conforming to the source protocol.

It may reuse `foundationModelAvailability()` and the physical default model. It must use:

~~~swift
LanguageModelSession(
  model: .default,
  tools: [],
  instructions: HealthPrivacyTransformPolicy.instructions
)
~~~

Add Health-only `@Generable` enums/struct for the closed output and explicitly convert to source-owned output enums.

Use greedy generation and the fixed Health token budget.

The prompt is deterministic sorted JSON of only the transform aggregate fields. No Agent wrapper/context is prepended.

Failure mapping:

- unsupported/profile/device/Apple Intelligence/readiness -> closed unavailable reason;
- guardrail/refusal -> fail closed;
- decoding/unsupported guide -> invalid output;
- rate limit/model failure -> model unavailable;
- cancellation/deadline -> cancelled/deadline exceeded.

Never retry remotely.

### 02-C4 — Separate C ABI

Export:

~~~text
floe_health_privacy_transform
floe_health_privacy_transform_free
~~~

Do not change the existing `floe_local_model` ABI or Rust ByteCall binding.

Use a small Health-specific encoded request limit. Error replies never echo invalid input bytes.

CP03 will add the product-side transport wrapper/injection. CP02 adds no Flutter method.

## 6. 02-D — Native build and validation inputs

Because `LocalModel.swift` now compiles against the Health contract source, update every direct compile.

### iOS product build

`apps/client/ios/build_native.sh`

- add a path to `HealthPrivacyTransform.swift`;
- compile that file together with `LocalModel.swift` for every architecture;
- keep current weak FoundationModels linkage and lipo/embed behavior.

### macOS product build

`apps/client/macos/build_native.sh`

- compile the same Health contract source with `LocalModel.swift`;
- do not link HealthKit or Flutter into the local-model dylib.

### Xcode dependency inputs

Update iOS and macOS Runner project Build Swift Libraries `inputPaths` to include the Health contract source.

Do not add the file to Runner Sources; Runner receives FloeAppleHealth from the Swift package separately.

### Direct validation/smoke compiles

Update:

~~~text
tools/validation/check-local-model.sh
tools/validation/run-local-model-smoke.sh
apps/client/integration/product_conversation_test.dart
~~~

to compile the same source-owned Health contract file alongside `LocalModel.swift`.

Also extend `tools/validation/test_native_build.py` with a second-Swift-source invalidation case. The current helper hashes every file argument, but the existing regression mutates only one source; CP02 must prove changing the newly added Health contract source alone causes a rebuild.

Do not duplicate contract code into fixtures.

## 7. 02-E — Tests

### FloeAppleHealth tests

Add a focused file such as:

    Tests/FloeAppleHealthTests/HealthPrivacyTransformTests.swift

Required tests:

1. valid input encodes only the three allowed keys;
2. all-nil input rejected;
3. negative/NaN/infinity/out-of-bound input rejected;
4. output encodes only capacity/recovery;
5. no identity/source/provenance/Agent/credential field exists;
6. aggregate conversion copies only the three values;
7. fake typed output projects deterministic View metadata/evidence;
8. both output categories unknown -> zero confidence/no evidence;
9. projected View contains no aggregate numbers.

Keep existing deterministic reducer tests as pre-CP03 evidence.

### Local model host tests

Extend `tools/validation/LocalModelHostTests.swift` with fake Health generation so no live Foundation model is required.

Required cases:

- availability;
- start/poll/done;
- exact rejoin;
- conflict;
- cancel;
- deadline;
- release before/after completion;
- strict decoder extra-field rejection;
- Agent-shaped input rejection;
- invalid input rejected before generation;
- failure output does not echo input;
- result has only closed categories;
- Agent and Health hosts can own independent pending jobs;
- malformed C ABI bytes fail without echo;
- new C ABI/free symbols execute.

## 8. Implementation order

1. Add source-owned Health contract and contract tests.
2. Add aggregate-to-input and typed-output-to-View deterministic seams.
3. Add FoundationModels generated Health output and transformer.
4. Add independent Health host and command/reply decoder.
5. Export separate Health C ABI.
6. Extend local-model deterministic host tests.
7. Update every direct local-model compile/build input and add the second-source native-cache invalidation regression.
8. Update `apps/client/apple/FloeAppleHealth/README.md` to say the contract/host operation exists but product cutover is still CP03.
9. Narrowly update current architecture ownership docs only if needed; do not claim mandatory runtime transform.
10. Run focused checks, residual audit, then Apple product build gates.
11. Append CP02 execution evidence here and update parent status only after acceptance.

## 9. Forbidden shortcuts

Do not:

- inject transformer into `currentHostProvider`;
- change `readDerivedWellbeing()` to transform;
- remove deterministic product reducer yet;
- use deterministic classification after a new transform fails;
- add transform provenance;
- make Wellbeing Gateway-eligible;
- modify processing restriction/grants/recipient consent;
- change Inference routing/fallback;
- expose transform through Flutter/AppWire;
- add a Tool/Expert for transformation;
- pass Agent instructions/history/Persona/Memory/discovery to Health;
- share Agent host job ownership;
- create generic cross-connector transform framework;
- add Android local-model parity;
- bump unrelated schema/protocol versions.

## 10. Verification

Follow `.agents/skills/code-change-verification/SKILL.md`.

Focused:

~~~sh
cd apps/client/apple/FloeAppleHealth
swift test

cd ../../../../
bash tools/validation/check-local-model.sh
python3 tools/validation/test_native_build.py
~~~

Product/native gates:

~~~sh
cd apps/client
flutter analyze
flutter test
flutter build macos
flutter build ios --simulator
~~~

Use the supported Xcode 26/iOS 26 simulator environment when available. Record an exact environment blocker rather than substituting Android.

Run the safe existing availability smoke when supported:

~~~sh
tools/validation/run-local-model-smoke.sh --availability
~~~

CP02 should not change Rust production code. Do not run a full Rust workspace gate unless implementation unexpectedly changes Rust/native binding inputs. Always run `git diff --check`; run the architecture checker if architecture/dependency surfaces change.

The native build regression must prove that changing `HealthPrivacyTransform.swift` invalidates the local-model native artifact fingerprint.

## 11. Residual audit

Search/classify at minimum:

~~~text
HealthPrivacyTransform
FoundationHealthPrivacyTransformer
floe_health_privacy_transform
LocalModelHost
foundationGenerate
learnerPromptClassification
nativeActionTools
currentUserRequest
AppleWellbeingReducer.reduce
readDerivedWellbeing
LocalModel/LocalModel.swift
HealthPrivacyTransform.swift
instructions
prompt
available_capabilities
active_experts
persona
memory
credential
source_handle
person_id
device_id
~~~

Completion requires:

- Health contract contains no forbidden Agent/source-authority fields;
- Health generator has no path to Agent tool/discovery helpers;
- Health C ABI is separate from Agent C ABI;
- Agent/Health hosts have independent jobs;
- every direct `LocalModel.swift` compile also compiles the Health contract source;
- remaining `AppleWellbeingReducer.reduce` production use is exactly the pre-CP03 provider path;
- no CP03 provenance/product wiring exists.

Do not add a permanent grep script.

## 12. Completion report

Append after implementation:

1. starting/final HEAD;
2. CP02 commit SHA(s);
3. source-owned contract and ABI shape;
4. Agent/Health independent-job proof;
5. direct compilers/build inputs updated;
6. exact verification commands/results;
7. residual audit;
8. docs updated;
9. clean worktree;
10. explicit statement: Checkpoint 03 has not started.

## 13. Acceptance checklist

- [ ] Exact bounded `HealthPrivacyTransformInput`.
- [ ] Closed `HealthPrivacyTransformOutput` with no free text/authority.
- [ ] `HealthPrivacyTransforming` has no Agent/Inference dependency.
- [ ] Aggregate-to-input and transformed-output-to-View seams exist.
- [ ] FoundationModels Health generation uses fixed instruction and `tools: []`.
- [ ] Health path cannot call Agent/Learner/tool helpers.
- [ ] Health owns a separate start/poll/cancel/release job.
- [ ] Agent and Health jobs coexist.
- [ ] Separate strict C ABI is tested.
- [ ] Every production/validation local-model compile includes Health contract source.
- [ ] Swift tests cover bounds, grammar, failure, cancellation/deadline/release and non-leakage.
- [ ] macOS and iOS simulator builds pass or exact allowed blockers are recorded.
- [ ] Real HealthKit provider still uses pre-CP03 path.
- [ ] No transform provenance/Gateway/routing work has started.
- [ ] CP03 remains not started.
