# Health Transform implementation contract

Status: the user approved implementation at 10:52 on 2026-10-03. This is the Health contract appendix to the active architecture work, not a separate execution plan. Health ownership and interfaces below use the package layout and synchronous-prepare/asynchronous-generate seam agreed with the common DeviceModel owner. The final shared model document must be published with this appendix before implementation workers start. The [common DeviceModel contract](2026-10-03-device-model-contract.md) owns model values, capability names and error cases; this document must not create duplicate model definitions.

Source baseline inspected: `2423f4428373856f470252158943f6758cb92da8`. The coordinator supplies the final published contract checkpoint to cloud workers after synchronization. No implementation or qualification is claimed here. No builds, formatters, compilers, tests or native/provider/data operations were run.

## Accepted behavior

Transform is a shared typed interface. HealthTransform is its concrete Health/Wellbeing implementation. FoundationModels is the first peer DeviceModel backend; no second backend, Email transform, Android implementation, registry or transform framework is included. Manager, Experts and Learner retain common server Primary/device Fallback selection with the existing valid-absence/failure distinction. Learner uses `deep_work`. Health transformation itself remains mandatory, device-local and fail-closed.

The pipeline is bounded SDK acquisition and deterministic minimization, Apple input mapping, hidden local semantic filtering, validated typed Wellbeing content, then a source-owned envelope and independently consumed transform receipt. The transform receives no Agent context, tools, source identity or authority. Transform success grants no source permission and does not change HighlySensitive classification.

## Swift modules and one physical image

The synchronized layout is one new package at `apps/client/native/FloeNative/Package.swift`:

| Target | Sources | Dependencies |
| --- | --- | --- |
| `FloeModelExecution` | `Sources/FloeModelExecution/` | Standard value/runtime libraries only; common model owner defines the contract |
| `FloeTransforms` | `Sources/FloeTransforms/Transform.swift` | Standard Swift only |
| `FloeHealthTransform` | `Sources/FloeHealthTransform/` | `FloeTransforms`, `FloeModelExecution` |
| `FloeHealthTransformBridge` | `Sources/FloeHealthTransformBridge/` | `FloeHealthTransform`, `FloeModelExecution`, Foundation, CryptoKit; Darwin for bundled ABI client |
| `FloeFoundationModels` | `Sources/FloeFoundationModels/` | `FloeModelExecution`, Apple FoundationModels SDK |
| `FloeNativeHost` | `Sources/FloeNativeHost/` | Model contract/backend, Health domain/bridge; contains the separate model and Health C ABI hosts |

The manifest exports the dynamic library product `floe_local_model`, backed by `FloeNativeHost`, and stateless products required by the Apple source package. The final artifact remains `libfloe_local_model.dylib`. All live model/Health operation state belongs to `FloeNativeHost`. Runner must never link that target or instantiate the receipt host. Sharing stateless Codable/domain types between Runner and the native image is safe; copying live receipt authority is not.

The existing `apps/client/apple/FloeAppleHealth` package has two targets/products after cutover:

- `FloeAppleHealth`: HealthKit acquisition and platform permission facts only.
- `FloeAppleWellbeing`: Apple input mapping, concrete source pipeline, sanitized View/lifecycle values and cache. It depends on `FloeAppleHealth` and the stateless Health domain/bridge products from `../../native/FloeNative`.

The coordinator owns both manifests, native scripts/helper and Xcode product/input references. Build the dynamic SwiftPM product once per actual target/SDK, then reuse the existing embed/lipo/install-name/signing steps. Do not concatenate imported modules without building them. Do not add a second dylib. Build caching must account for all package manifests, source files, compiler/SDK/target and linking options, including transitive target sources; the current helper only hashes explicit file arguments and cannot automatically infer a package directory's contents. No build command is to be run during contract implementation.

## Shared Transform interface

`Sources/FloeTransforms/Transform.swift` exports exactly this small abstraction:

```swift
public protocol Transform<Input, Output>: Sendable {
    associatedtype Input: Sendable
    associatedtype Output: Sendable

    func transform(_ input: Input) async throws -> Output
}
```

It owns no registry, source binding, execution placement, model field, receipt, cache or persistence. Concrete model-backed transforms receive DeviceModel through construction. Future domain execution policy remains separately owned; the shared interface does not universally require every future transform to be local-only.

## Concrete Health domain

`Sources/FloeHealthTransform/HealthTransformInput.swift`:

```swift
public struct HealthTransformInput: Codable, Equatable, Sendable {
    public let sleepHours: Double?
    public let steps: Double?
    public let exerciseMinutes: Double?
    public init(sleepHours: Double?, steps: Double?, exerciseMinutes: Double?) throws
    public func validate() throws
}

public enum HealthTransformValidationError: Error, Equatable, Sendable {
    case invalidInput
    case invalidOutput
}
```

Wire keys remain `sleep_hours`, `steps`, `exercise_minutes`. At least one field is non-nil; every present value is finite, nonnegative and at most 36, 1,000,000 or 2,160 respectively. Decoding calls the same validator. There is no source/platform/subject metadata in this value. No rounding, clamping, diagnosis, deterministic category fallback or fabricated missing numeric value is added.

`Sources/FloeHealthTransform/HealthTransformOutput.swift`:

```swift
public struct HealthTransformOutput: Codable, Equatable, Sendable {
    public enum Capacity: String, Codable, Sendable {
        case reduced, typical, strong, unknown
    }
    public enum Recovery: String, Codable, Sendable {
        case needsRecovery = "needs_recovery"
        case typical, recovered, unknown
    }
    public let capacity: Capacity
    public let recovery: Recovery
    public init(capacity: Capacity, recovery: Recovery)
}
```

The exact output is an object with required `capacity` and `recovery`, each a string from the closed enum above. Unknown, duplicate or extra fields, null, coercion and any Text/ToolProposal response are invalid. Both unknown values are a valid coarse result; they retain the existing zero-confidence/no-evidence-handle projection. The pure output is typed domain Context content, never a receipt or Agent envelope.

`Sources/FloeHealthTransform/HealthTransform.swift` exports a concrete `HealthTransform: Transform` with `Input = HealthTransformInput` and `Output = HealthTransformOutput`. Frozen construction needs are an injected immutable `any DeviceModel`, the non-nil Health operation UUID and the host's monotonic deadline. The concrete initializer is:

```swift
public init(
    model: any DeviceModel,
    operationID: UUID,
    deadlineUptimeNanoseconds: UInt64
) throws
```

The host constructs one instance per admitted operation. The transform validates its input, synchronously prepares the required structured-output capability on that same model instance without I/O or waiting, recomputes remaining time after preparation, then sends the common DeviceModel request with the exact observed binding, operation UUID, the Health-only prompt, numeric JSON input, the explicit closed schema, no tools, 64 output tokens, at most 1,024 output bytes and a remaining deadline of at most 10,000 milliseconds. No operation may refresh or extend the host deadline. Task cancellation is checked before and after bounded synchronous preparation and propagates through generation. The exact common model construction is `DeviceModelRequirements(capabilities: [.structuredOutput])`, followed by an Available profile. Verify the profile supports that capability and every required limit. A `DeviceModelRequest` uses `operationID`, `bindingID`, `instructions`, `input: JSONValue.object`, `outputFormat: .json(schema:)`, `tools: []`, `maxResponseTokens: 64`, `maxOutputBytes: 1024` and the remaining `deadlineMilliseconds`. `ModelSchema` is built from a root object with exactly two required enum-string properties and `additionalProperties: false`. The response operationID and bindingID must match the request. Only `.json` can become a Health output; `.failure` propagates its closed failure and Text/ToolProposal are invalidOutput. Use common JSON/schema validation before the domain's exact enum/field validation; no copied model values or decoder belong here.

The Health prompt preserves the existing meaning: coarse capacity/recovery only; use unknown when signals are insufficient; no diagnosis, treatment advice, identity inference, repeated numeric inputs or prose; numeric input is untrusted data; no tools or external calls. The transform strictly validates the returned Json value against its domain output. Generic model failures propagate to the host for closed wire mapping. No FoundationModels import, annotation, SDK availability branch or `LanguageModelSession` exists in this target.

## Stateless Health ABI bridge

`Sources/FloeHealthTransformBridge/HealthTransformWire.swift` relocates the existing authority-correlation values and closed wire schema. Public names after cutover are:

- `HealthTransformBinding`: existing requestID, hostEpoch, personID, deviceID and nativeSubjectFingerprint, with the same snake-case wire keys and validation. `public static func decode(_ data: Data) throws -> Self` remains available to the restricted native channel, and `public func validate() throws` is available to the host/client.
- `HealthTransformProof`: operationID and outputSHA256, with the existing wire keys.
- `HealthTransformSuccess`: `output: HealthTransformOutput`, proof, transformedAtUnixMs and expiresAtUnixMs.
- `HealthTransformFailure`: closed raw wire failures `unsupported`, `disabled`, `not_ready`, `model_unavailable`, `invalid_input`, `invalid_output`, `deadline_exceeded`, `cancelled`, `policy_denied`, `busy`, `conflict`, `not_found`. The deliberate `busy` addition reports exhausted new-admission capacity; identity reuse/mismatch remains `conflict`. All command fields, successful receipt fields, schema version and digest bytes remain unchanged.
- `HealthTransformCommand` and `HealthTransformReply`: the existing field names, schema version 1 and optional-field matrix. Explicit public initializers/getters permit the host and bundled client to use these values across module boundaries. They grant no authority.
- `HealthTransformWireCodec`: public static `decodeCommand(_ data: Data) throws -> HealthTransformCommand`, `decodeReply(_ data: Data) throws -> HealthTransformReply`, `encodeCommand(_ command: HealthTransformCommand) throws -> Data`, and `encodeReply(_ reply: HealthTransformReply) throws -> Data`, with current 8,192-byte command/4,096-byte reply ceilings and exact per-operation keys. Every incoming command/reply first uses the shared bounded `JSONValue.decode(_ data: Data, maximumBytes: Int)` strict decoder from FloeModelExecution to reject duplicate keys and malformed JSON before any ordinary Codable decode, then applies exact Health field validation. Do not retain or copy the old HealthJSONFraming parser or add another generic decoder. Domain Decodable validation remains mandatory after strict transport decoding; unknown-field and malformed-number rejection cannot be relaxed.
- `HealthTransformDigest.hex(_ output: HealthTransformOutput) -> String`: SHA256 of UTF-8 `floe.health.transform.v1`, NUL, capacity raw value, NUL, recovery raw value, lower-case hex. This exact algorithm and enum spelling are unchanged.

`Sources/FloeHealthTransformBridge/BundledHealthTransformClient.swift` exports:

```swift
public protocol HealthTransformOperationClient: Sendable {
    func perform(
        _ input: HealthTransformInput,
        binding: HealthTransformBinding
    ) async throws -> HealthTransformSuccess
}

public final class BundledHealthTransformClient: HealthTransformOperationClient, @unchecked Sendable {
    public init() throws
    public func perform(_ input: HealthTransformInput, binding: HealthTransformBinding) async throws -> HealthTransformSuccess
}
```

This is a real C ABI transport boundary, not another domain Transform or model backend. It loads the current bundle path and symbols, generates a fresh operation UUID, starts/polls/cancels/releases with the existing semantics and compares the exact returned binding/digest/timestamps. It never mints a receipt or accepts a caller success flag. The old `HealthPrivacyTransforming` protocol and `BundledHealthPrivacyTransformer` names are removed, without aliases.

## Live Health host and unchanged C ABI

`Sources/FloeNativeHost/HealthTransformHost.swift` owns the single Health job, successful receipt map and seen-operation tombstones. Its internal construction is `HealthTransformHost(model: any DeviceModel)`. `invoke(_ command: HealthTransformCommand) -> HealthTransformReply` remains synchronous; bounded synchronous preparation and asynchronous generation occur in the admitted job outside the state lock.

The synchronous Health ABI availability call invokes common `DeviceModel.prepare` with the Health structured-output requirements. That method performs bounded local observation only: no I/O, model loading, download or wait. Model initialization/loading belongs to native composition; an unready backend reports not_ready. No separate Health availability branch calls FoundationModels. An earlier Available result is never execution authority: the admitted job independently calls prepare outside the state lock and generate revalidates the same backend binding before work. The prepare result and failure names are defined by the common model contract.

`Sources/FloeNativeHost/HealthTransformABI.swift` owns the unchanged exported symbols:

```text
floe_health_privacy_transform(bytes, length) -> allocated UTF-8 JSON
floe_health_privacy_transform_free(pointer)
```

The exact commands stay `availability`, `start`, `poll`, `cancel`, `release`, `consume_receipt`. Coordinator-owned `NativeComposition` defines `static let deviceModel: any DeviceModel`, `static let deviceModelHost: DeviceModelHost` and `static let healthTransformHost: HealthTransformHost`, supplying one backend instance to the independent model and Health hosts. The Health C entry point calls `NativeComposition.healthTransformHost.invoke(...)`. The Health host is not constructed in Runner or in a stateless package initializer.

Preserve the host safety semantics below, including the required correction to the baseline replay-retention bug:

- Separate Health job slot, independent from ordinary model ABI job state.
- Non-nil exact operation identity; same-ID start rejoins only equal input/binding and a non-released current job before considering capacity for new work. Altered/reused operation identity conflicts. A different new operation encountering the occupied Health slot or other exhausted admission capacity returns busy.
- Ten-second monotonic deadline; no success receipt after timeout, cancellation or release.
- Releasing an unfinished worker marks it abandoned/cancels it; the slot is not reusable until the worker has finished.
- At most 64 retained receipts and 128 retained operation identities, including active jobs and completion tombstones. Capacity exhaustion rejects a new admission as busy; no unexpired identity/receipt is evicted to admit it.
- The baseline `seenOperations = start + 30 minutes` rule is incorrect because the receipt expires at completion + 30 minutes. Retain the operation identity while any matching job or receipt exists, regardless of any nominal timestamp. Only after physical worker completion may a tombstone receive an expiry, no earlier than physical completion + 30 minutes and no earlier than any associated receipt expiry. Keep the completion horizon with a monotonic clock so a wall-clock jump cannot shorten it; receipt expiry still uses the existing public wall-clock timestamp. Cleanup requires all retention conditions, not just one expired map entry.
- A noncooperative worker retains the Health slot and its identity indefinitely until it physically returns, even after cancellation, release, timeout or 30 minutes. Physical completion starts the tombstone horizon even when the result was abandoned and no receipt can be minted. A completed job still held by its caller retains identity until that job is released as well.
- Receipt timestamps come from the host clock only. Expiry is exactly completion + 30 minutes.
- Consume requires exact operation, binding and digest, unexpired receipt, then removes that receipt atomically. It never clears or shortens the operation tombstone. Duplicate consumption fails and the operation cannot be reused while the retained identity remains.
- The model output cannot assert source handles, identity, timestamps, confidence, evidence or success.

Domain validation errors map to matching invalid_input/invalid_output. Common model cancellation/deadline/refusal/unavailable errors map to the existing closed Health failures. Unknown errors fail closed as model_unavailable; no domain-specific SDK error switching remains in this host. The exact `DeviceModelFailure` mapping is: unsupported → unsupported; disabled → disabled; notReady → notReady; unavailable → modelUnavailable; invalidInput → invalidInput; invalidOutput → invalidOutput; deadlineExceeded → deadlineExceeded; cancelled → cancelled; policyDenied → policyDenied; quotaExceeded → modelUnavailable; busy → busy; conflict → conflict; notFound → notFound. These are Swift case names with snake-case Health wire raw values, including the deliberate new busy case. `DeviceModelObservation.unavailable` maps unsupported/disabled/notReady to the corresponding Health availability strings. A malformed profile/response is invalidOutput and no error selects another backend.

## Apple acquisition, mapper and source host

`FloeAppleHealth/Sources/FloeAppleHealth/HealthKitWellbeingProvider.swift` becomes acquisition-only. Its exported interface is:

```swift
public struct HealthKitWellbeingAcquisition: Equatable, Sendable {
    public let sleepHours: Double?
    public let steps: Double?
    public let exerciseMinutes: Double?
}

public enum HealthKitReadStatus: Equatable, Sendable {
    case unsupported
    case unavailable
    case requestRequired(requestCompleted: Bool)
    case queryable
}

public enum HealthKitWellbeingFailure: Error, Equatable, Sendable {
    case unsupported
    case permissionRequired
    case noDataOrReadAccessLimited
    case unavailable
}

public actor HealthKitWellbeingProvider {
    @MainActor public static func currentHostProvider() -> HealthKitWellbeingProvider
    public func requestReadAuthorization() async throws
    public func readAggregates() async throws -> HealthKitWellbeingAcquisition
    public func readStatus() async -> HealthKitReadStatus
}
```

The actor retains HealthKit/host support, authorizationRequestCompleted, its acquisition clock and the existing bounded query algorithms. It retains the 36-hour window, 100-sample sleep-query bound, sleep interval merging, cumulative steps/exercise queries and ambiguous no-data/read-limit failure. A completed OS prompt is not an assertion of read permission. `requestRequired(requestCompleted:)` preserves the distinction used by the current lifecycle: metadata can know a prompt completed while a new read still refuses a current shouldRequest result. No model, View cache, transform failure/proof, source handle or source publication remains in this target. `AppleHealthHost` and platform support helpers move to an acquisition-only file if needed.

`Sources/FloeAppleWellbeing/AppleHealthWellbeingMapper.swift` exports:

```swift
public enum AppleHealthWellbeingMapper {
    public static func map(_ acquisition: HealthKitWellbeingAcquisition) throws -> HealthTransformInput
}
```

It performs explicit units/field mapping and domain input validation. It does not invoke a model, create evidence, manufacture missing values or perform a deterministic capacity/recovery classification. No Android mapper is added.

`Sources/FloeAppleWellbeing/AppleWellbeingSource.swift` exports:

```swift
public enum AppleWellbeingFailure: Error, Equatable, Sendable {
    case unsupported
    case permissionRequired
    case noDataOrReadAccessLimited
    case unavailable
    case privacyTransform(HealthTransformFailure)
}

public actor AppleWellbeingSource {
    public init(
        acquisition: HealthKitWellbeingProvider,
        transform: any HealthTransformOperationClient,
        sourceHandle: String,
        now: @escaping @Sendable () -> Date = { Date() }
    )
    public func requestReadAuthorization() async throws -> AppleHealthLifecycle
    public func readDerivedWellbeing(binding: HealthTransformBinding) async throws -> AppleWellbeingObservation
    public func lifecycle() async -> AppleHealthLifecycle
}
```

This concrete source host owns `lastView`, `lastSuccessAtUnixMs` and `lastReadHadNoData`. It sequences acquisition → mapper → independently hosted transform → post-success envelope. It checks cancellation before publication and clears the current View on failed acquisition/transform exactly as before. Acquisition failures map to the matching AppleWellbeingFailure; mapper validation failures map to privacyTransform invalidInput/invalidOutput, and bridge HealthTransformFailure values are retained in privacyTransform. No SDK model error or raw diagnostic text crosses this source boundary. A no-data read sets lastReadHadNoData; only a fully successful transformed publication clears it, preserving the current lifecycle behavior. Other failures preserve lastSuccessAtUnixMs without extending expiry.

`Sources/FloeAppleWellbeing/WellbeingProjection.swift` holds the existing AppleWellbeingView, AppleWellbeingObservation, AppleHealthLifecycleState, AppleHealthLifecycle and AppleWellbeingProjection with the new Health output/proof imports. Preserve existing View encoding, source handle, `wellbeing.derived`, 30-minute freshness, confidence 600 for any known category/0 otherwise, and evidence handle `health.transform:<lowercase operation UUID>` only for a known category. No payload fields change.

Lifecycle precedence remains: unsupported → unavailable permission inspection → requestRequired with no completed prompt → no-data/read-limited → no cached View/pending → expired/stale → fresh/ready. A requestRequired status after a completed prompt follows the existing cache/lifecycle branch but does not authorize a read; readAggregates independently rejects shouldRequest. Read-only lifecycle inspection never invokes the model or prompts permission.

The coordinator's integration changes AppleContextChannel to hold AppleWellbeingSource, construct the acquisition actor plus bundled operation client, and catch AppleWellbeingFailure. Existing before/after subject fingerprint checks, device binding, restricted callback fields, permission-completion semantics and Flutter error strings remain exact. Any duplicated channel cache remains sanitized display metadata only; no raw aggregate enters Dart.

## Deletion and preserved callers

Delete the old monolithic `FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift` after all new module callers exist. Move WellbeingProjection out of the acquisition target; do not retain a compatibility export. Remove `HealthPrivacyTransforming`, old bundled transformer and old provider transformer constructor. Remove direct FoundationModels/session/schema declarations from Health code. The common model owner separately removes domain prompt classification from the backend.

Rust `platform/native/health_privacy.rs`, provider personal_native receipt consumption, Context lineage and Access Health checks retain their exact semantic contracts. The new same-snapshot Swift arrangement must still satisfy their existing ABI bytes. Any discovered mismatch is corrected at the real boundary, never with a synthetic success flag or relaxed proof check.

## SDK evidence and qualification limit

The coordinator supplied a read-only inventory of installed Xcode 26.2 / Swift 6.2.3 FoundationModels interfaces at `/workspace/scratch/42a4c1725521/device-sdk-inventory/FoundationModels-26.2-API-Inventory.md`. The inventory confirms `DynamicGenerationSchema` plus throwing `GenerationSchema(root:dependencies:)`, `LanguageModelSession.respond(to:schema:includeSchemaInPrompt:options:)`, and `GeneratedContent.jsonString`/`isComplete`. These declarations support the proposed explicit nonnullable enum-object Health schema without Health-specific `@Generable` types in the backend. This is declaration evidence, not compiled or runtime qualification. Swift's generic Decodable is not Generable; the backend translates the portable schema and strictly validates generated JSON.

The installed SDK has no documented dynamic null-schema generation route. The synchronized first portable schema subset therefore excludes null and nullable unions; Knowledge owns its deliberate nonnullable proposals-array output contract. Generic wire JSON can still represent null where the common transport explicitly requires it, such as unknown usage, but that does not add nullable schema support. Health's closed enum schema already satisfies this subset. Task cancellation has no separate session.cancel API in the inspected interface; keep the host's late-completion and receipt fences regardless of how promptly the SDK responds to cancellation.

No source operation, credential change, data reset, permission prompt or model invocation is authorized merely to implement this refactor. Qualification follows the coordinator's later gate after the complete structure is in place.
