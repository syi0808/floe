# Isolated Health implementation packages

These are bounded implementation handoffs for the user-approved 2026-10-03 correction. The coordinator chooses the saved cloud environment and requested Luna Max Fast configuration. Do not launch until the coordinator publishes one exact base containing both [Health contract](2026-10-03-health-transform-contract.md) and [DeviceModel contract](2026-10-03-device-model-contract.md), plus the installed-SDK inventory supplied with the task. The inspected source base `2423f4428373856f470252158943f6758cb92da8` is evidence only, not the final launch base. Every task prompt must name the exact published SHA; never silently fetch a moving branch.

All workers read AGENTS.md, the two contract documents and the architecture-change guidance. Implement only the listed files. Source reads are allowed; do not run compilers, builds, formatters, tests, analyzers, schema checkers or native/model/provider operations. Do not modify credentials, user data, signing configuration or external accounts. No push or deployment. Do not introduce compatibility aliases, fallback classifiers, Android/Email implementations, a transform registry, or role-specific model routing. Return the exact changed files, patch and concise source-level findings. Stop at the requested artifact; the coordinator owns integration and later qualification.

Every actual task handoff includes: “Report useful results right away, before scratch notes. If work remains, use collaboration.send_message; otherwise return the result. Keep your status report short; complete the research and deliverable at the depth the task requires. If authorization is missing, or the reviewer denies an action for that reason, pause and report the exact action, target, and blocker to me. If I send <transcript_evidence> that authorizes that action and target, proceed; if already denied, retry the same tool call once. Don’t add the evidence to tool arguments. If the evidence is insufficient or the retry is denied, stop and report back. If you delegate that action, forward the block unchanged.”

## A. Implement shared Transform and concrete Health domain

Exclusive new files:

```text
apps/client/native/FloeNative/Sources/FloeTransforms/Transform.swift
apps/client/native/FloeNative/Sources/FloeHealthTransform/HealthTransformInput.swift
apps/client/native/FloeNative/Sources/FloeHealthTransform/HealthTransformOutput.swift
apps/client/native/FloeNative/Sources/FloeHealthTransform/HealthTransform.swift
```

Implement the exact shared `Transform<Input, Output>` protocol and concrete Health types in the Health contract. HealthTransform's initializer receives the common DeviceModel, operation UUID and absolute monotonic deadline. Consume the neutral model types from the common contract; do not create a model protocol, JSONValue, schema type or decoder of your own.

HealthTransform validates finite bounded optional numbers with at least one signal, calls bounded synchronous `prepare` for structuredOutput on its retained model, then generates using the same binding, explicit enum-object schema, no tools, 64 tokens, 1,024 output bytes and remaining deadline. Validate returned identity, binding, output kind, exact keys and enums. Preserve both-unknown as valid output. Generic execution errors propagate; do not mint a proof or source metadata. Keep prompt/domain validation here; no FoundationModels, HealthKit, Agent, Persona, Memory or source authority imports.

Copy the existing Health semantic prompt's meaning from the old HealthPrivacyTransform.swift. Do not copy its SDK Generable declarations, availability branch, LanguageModelSession, receipt state or ABI. Input/output wire spellings and domain bounds remain unchanged. Export every initializer/member needed by the frozen cross-module callers. No Package.swift edit. Report any contract inconsistency before inventing an alternate signature.

Delivery: four source files and a patch against the supplied exact base, with source-level notes identifying imports, typed output/schema ownership, cancellation/deadline checks and absence of backend/source policy.

## B. Extract stateless Health wire and bundled client

Exclusive new files:

```text
apps/client/native/FloeNative/Sources/FloeHealthTransformBridge/HealthTransformWire.swift
apps/client/native/FloeNative/Sources/FloeHealthTransformBridge/BundledHealthTransformClient.swift
```

Extract the existing Health command/reply codec, exact binding/proof/success values, closed wire failures, digest and bundled ABI client into the real transport module described by the Health contract. New public domain names are HealthTransformInput and HealthTransformOutput from FloeHealthTransform. Bridge names are HealthTransformBinding, HealthTransformProof, HealthTransformSuccess, HealthTransformFailure, HealthTransformCommand, HealthTransformReply, HealthTransformWireCodec, HealthTransformDigest, HealthTransformOperationClient and BundledHealthTransformClient. Preserve the existing JSON bytes/keys/schema 1 and the two native symbol names.

Use strict 8,192-byte commands and 4,096-byte replies, exact operation field matrices, duplicate/unknown-field rejection and bounded output scanning before decode. Preserve the digest's exact UTF-8/NUL sequence and lower-case SHA256 hex. The bundled client generates one UUID, starts/polls, forwards cancellation and always releases; returned output must match exact binding, digest and completion/expiry before success. No source/HealthKit/Agent/FoundationModels imports, no global host, receipt store, model invocation, policy widening or cached success authority.

FloeHealthTransformBridge explicitly depends on FloeModelExecution. Mandatory first step for every incoming command/reply is its bounded `JSONValue.decode` strict decoder, rejecting duplicate keys and malformed JSON before ordinary Codable decoding. Then validate the exact Health operation/field matrix and domain values. Do not move/copy HealthJSONFraming, write a competing generic JSON parser or rely on JSONSerialization/JSONDecoder after duplicate keys have already collapsed. Use the shared decoder API exactly as frozen by the common contract. No edits to the old monolith yet; the coordinator removes it after all imports change. No manifest edits.

Delivery: two new files and patch against the supplied base, with a precise list of preserved wire tags/limits and any intentionally stricter malformed-input rejection.

## C. Extract the live Health receipt host

Exclusive new files:

```text
apps/client/native/FloeNative/Sources/FloeNativeHost/HealthTransformHost.swift
apps/client/native/FloeNative/Sources/FloeNativeHost/HealthTransformABI.swift
```

Move the existing independent Health job/receipt/tombstone state machine from HealthPrivacyTransform.swift into an internal `HealthTransformHost(model: any DeviceModel)`. Use the new stateless bridge values/codec. Keep `invoke(_ command: HealthTransformCommand) -> HealthTransformReply` synchronous. Availability calls the common bounded synchronous prepare method; do not import or inspect FoundationModels directly. Reserve a Health job under the existing lock, then construct one HealthTransform with the exact operation UUID and host deadline and execute outside that lock. Re-prepare and generation binding checks remain inside the domain/common executor path.

Preserve the ten-second immutable monotonic deadline, 30-minute receipt expiry, 64 receipts/128 tombstones, same-ID exact replay/conflict, consume-once semantics and late-worker fences. Cancellation/release cannot free the slot until the worker physically returns. Only successful validated domain output completed before the live deadline can mint a receipt. The model receives no person/device/source binding. The host alone attaches exact binding, digest and timestamps. Preserve failure mapping specified by the Health contract, including fail-closed common model errors.

The C entry points keep existing symbol names and byte limits. The ABI calls `NativeComposition.healthTransformHost.invoke(...)`. Do not define NativeComposition, a production backend/global singleton, Package.swift or build scripts; the coordinator owns composition and links this target only into the dylib. Do not alter the generic model host or merge its jobs with Health. Do not remove the old monolith yet.

Delivery: the two source files and patch, with source-level review of lock/I/O separation, replay capacity, abandon/finish behavior and exact one-use receipt consumption. No tests or native model calls.

## D. Separate HealthKit acquisition and Apple source assembly

Exclusive files:

```text
apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthKitWellbeingProvider.swift
apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthKitAcquisition.swift
apps/client/apple/FloeAppleHealth/Sources/FloeAppleWellbeing/AppleHealthWellbeingMapper.swift
apps/client/apple/FloeAppleHealth/Sources/FloeAppleWellbeing/AppleWellbeingSource.swift
apps/client/apple/FloeAppleHealth/Sources/FloeAppleWellbeing/WellbeingProjection.swift
```

Replace the provider's transformer/View-publication responsibility with the exact acquisition-only API in the Health contract. Keep existing HealthKit support checks, prompt-completion fact, 36-hour window, sleep sample limit/merging, cumulative quantity queries and no-data/read-limited ambiguity. Do not add a permission prompt to read or lifecycle. The connector imports no Transform/model/bridge module, stores no View/proof/sourceHandle and has no transform failure case.

Implement AppleHealthWellbeingMapper.map as explicit numeric unit/field mapping plus Health input validation. Implement AppleWellbeingSource with the concrete acquisition actor, HealthTransformOperationClient, source handle and clock. It performs acquisition → mapping → bundled transform → post-success View construction, owns the existing sanitized cache and lifecycle state, and maps acquisition/bridge errors into AppleWellbeingFailure. Preserve the exact lifecycle precedence and lastReadHadNoData behavior documented in the contract. A failed transform clears the current View, cannot extend expiry and cannot publish aggregates.

Move the old WellbeingProjection value/envelope code into the new target, using new domain output/proof names. Preserve wire fields, freshness, confidence/evidence rules and no-data/permission semantics. Add explicit public initializers required by cross-target construction. Keep non-iOS value types separate from iOS-only HealthKit actors so the shared package/dylib can build for macOS without importing unavailable HealthKit/UIKit definitions.

Do not edit Package.swift, AppleContextChannel, build scripts or Xcode references. Do not delete the old WellbeingProjection.swift or HealthPrivacyTransform.swift yet: the coordinator removes the old target files while switching all callers in one integration batch. Do not add aliases or forwarders in those old files. Report the required imports/catch-case replacements for AppleContextChannel using the exact new public API.

Delivery: those five files and patch, with source-level comparison of acquisition bounds, cache/lifecycle/error behavior and which metadata is excluded from model input.

## Native integration reserved to coordinator

The coordinator exclusively owns:

```text
apps/client/native/FloeNative/Package.swift
apps/client/native/FloeNative/Sources/FloeNativeHost/NativeComposition.swift
apps/client/apple/FloeAppleHealth/Package.swift
apps/client/apple/native_build.sh
apps/client/macos/build_native.sh
apps/client/ios/build_native.sh
apps/client/macos/Runner.xcodeproj/project.pbxproj
apps/client/ios/Runner.xcodeproj/project.pbxproj
apps/client/ios/Runner/AppleContextChannel.swift
```

NativeComposition owns one backend instance and the two independent hosts (`deviceModel`, `deviceModelHost`, `healthTransformHost`). The coordinator integrates the separate common-model cloud packages and these Health artifacts, deletes the old monolith/old projection file after all callers have moved, updates exact package products and cache inputs, and preserves one physical receipt image. Foundation backend implementation stays with the model/native owner. The synchronized portable schema subset excludes null/nullable unions; Knowledge owns its proposals-array contract. Do not reopen nullable-schema investigation or add an undocumented SDK construction. Required qualification starts only at the later agreed full-structure gate; none of the above workers runs it.
