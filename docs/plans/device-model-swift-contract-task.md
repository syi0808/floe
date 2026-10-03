# Implement the pure Swift DeviceModel contract

Implement only these new files in the shared checkout:

- apps/client/native/FloeNative/Sources/FloeModelExecution/JSONValue.swift
- apps/client/native/FloeNative/Sources/FloeModelExecution/ModelSchema.swift
- apps/client/native/FloeNative/Sources/FloeModelExecution/DeviceModel.swift

Read docs/plans/2026-10-03-device-model-contract.md first. Its schema, exact wire tags, fields, bounds, absence/null semantics, numeric semantics and ownership are frozen. Implement those semantics without a parallel abstraction. The coordinator owns Package.swift and build integration; do not create or edit any manifest, existing Swift file, Rust source, test, fixture, checker or lockfile.

The shared values must import Foundation only and be Sendable/Equatable/Codable as appropriate. JSONValue is the closed recursive enum specified in the contract. Reject duplicate object keys before Codable/object decoding can collapse them, for every untrusted JSON command/schema/model result. Reject nonfinite or out-of-safe-range numbers, excess nesting/size and trailing data. Numeric integrality is mathematical, not lexical. FINAL precision rule: preserve JSONValue.number(Double), with no JSONNumber type. Before decoding each numeric token, enforce at most64 bytes, at most32 coefficient digits and exponent magnitude at most32; compare its exact normalized decimal value to the parsed finite Double shortest-roundtrip JSON decimal and reject any difference. 1/1.0/1e0 and 0.1 work; 1.0000000000000000001 rejects before it can round to an integer. Use bounded string significand/exponent normalization, not unbounded arithmetic. This applies to all untrusted model/schema JSON numbers, including numbers nested in input. Do not enable new dependencies. Use Unicode scalar counts for schema string length constraints and UTF-8 bytes for transport bounds. Do not fill absent required properties, clamp numbers, drop unknown keys or silently discard unsupported schema keywords.

Public types and APIs:

- JSONValue with public object/array/string/number/bool/null cases; public strict decoding from Data and encoding to Data. Provide `public static func decode(_ data:Data, maximumBytes:Int) throws -> JSONValue` and `public func encoded() throws -> Data`. A custom parser or duplicate-detecting decoder is necessary; parsing with JSONSerialization and then looking for duplicates is insufficient.
- ModelSchema with `public let json:JSONValue`, `public init(json:JSONValue) throws`, and `public func validate(_ value:JSONValue) throws`. Its Codable representation is the raw schema object. Decode validates the schema immediately.
- ModelOutputFormat with `.text` and `.json(schema:ModelSchema)` and the exact tagged wire representation.
- DeviceModelCapability cases text, structuredOutput, toolProposals, serialized to text/structured_output/tool_proposals.
- DeviceModelRequirements {capabilities:[DeviceModelCapability]}.
- DeviceModelLimits with maxInputBytes,maxInstructionsBytes,maxOutputBytes,maxResponseTokens,maxDeadlineMilliseconds.
- DeviceModelProfile {bindingID:String,capabilities:[DeviceModelCapability],limits:DeviceModelLimits}.
- DeviceModelObservation cases `.available(DeviceModelProfile)` and `.unavailable(DeviceModelUnavailable)` with the exact flattened wire representation from the contract.
- DeviceModelUnavailable cases unsupported,disabled,notReady.
- DeviceTool {name:String,description:String,inputSchema:ModelSchema}.
- DeviceModelRequest with operationID,bindingID,instructions,input,outputFormat,tools,maxResponseTokens,maxOutputBytes,deadlineMilliseconds.
- DeviceModelUsage {tokens:UInt64?,costMicros:UInt64?}; both nullable properties must be present in wire JSON. Omission is invalid. Unknown stays null.
- DeviceModelOutput cases text(String),json(JSONValue),toolProposal(name:String,input:JSONValue),failure(DeviceModelFailure).
- DeviceModelResponse {operationID,bindingID,output,usage}.
- DeviceModelFailure is Error/Codable/Sendable/Equatable, with the exact snake_case wire cases in the contract. Swift case names use conventional camelCase.
- DeviceModel protocol has synchronous bounded `prepare(_ requirements:DeviceModelRequirements) throws -> DeviceModelObservation` and async `generate(_ request:DeviceModelRequest) async throws -> DeviceModelResponse`.

All struct memberwise initializers must be explicitly public. Use Int for bounded byte/token/duration limits in Swift and UInt64 for usage. Make `validate() throws` public on requirements, limits, profile, observation, tool, request and usage. Provide `DeviceModelProfile.validate(request:DeviceModelRequest) throws` to validate exact binding, required capabilities derived from outputFormat/tools, and profile limits; `DeviceModelResponse.validateEnvelope(for request:DeviceModelRequest) throws` validates only exact operation/binding, the fixed65536-byte envelope bound and usage. `DeviceModelResponse.normalized(for request:DeviceModelRequest) throws -> DeviceModelResponse` first validates that envelope, then maps any invalid output form/content/schema/proposal/success-size into failure(.invalidOutput) while preserving the original validated usage. It only throws for untrusted envelope/identity/usage. Failure payloads are exempt from requested maxOutputBytes, since a1-byte allowance must still retain bounded terminal failure+usage. Provide validate(for:) only if a caller needs strict success validation; it must not replace the two-stage accounting seam. Failure output retains acknowledged usage and must remain a valid terminal response. JSON output requires empty tools; text requests may return either nonempty text or one exactly advertised tool proposal. Preparation capabilities must be sorted unique nonempty values; use lexical wire order, not declaration order. Model output schemas have object roots. The supported schema subset excludes null, nullable type arrays and null const; optional fields mean absent, and a present null is invalid. JSONValue.null still exists for required nullable usage/error transport fields and generic input, not structured-schema output. Tool input schemas use the same root contract.

Also provide the neutral transport values in DeviceModel.swift:

- DeviceModelCommand enum cases prepare(DeviceModelRequirements),start(DeviceModelRequest),poll(UUID),cancel(UUID),release(UUID).
- DeviceModelReply enum cases observation(DeviceModelObservation),pending(UUID),done(DeviceModelResponse),error(operationID:UUID?,failure:DeviceModelFailure),released(UUID).
- Custom Codable enforces schema_version=1 and exact flattened keys/tags defined in the contract. Error operation_id is required and explicitly null when no operation was admitted. Other optional fields are not added to unrelated variants.
- DeviceModelCodec.decodeCommand(_ data:Data) throws -> DeviceModelCommand; it must first run strict JSON validation with 131072-byte maximum.
- DeviceModelCodec.encodeReply(_ reply:DeviceModelReply) throws -> Data; maximum 65536 bytes.

Generic JSONValue decoding depth must allow the bounded schema plus its wire envelope (maximum 32), while ModelSchema imposes its separate root0/depth8/node256 bounds. Capabilities and unavailable observations carry no source/domain authority. The contract does not create a runtime, job registry, backend implementation or Domain Transform.

Do not implement tests and do not run compiler, formatter, tests, analyzer, package resolution, build or runtime commands. Static source review is allowed. Do not commit or publish unless the parent explicitly asks. Return the complete three-file artifact and a concise list of any uncertainty; never hide an unsupported contract by accepting broader data. Report useful findings promptly before scratch notes.
