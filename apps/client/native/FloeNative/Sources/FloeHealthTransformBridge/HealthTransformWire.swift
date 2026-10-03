import CryptoKit
import Foundation
import FloeHealthTransform
import FloeModelExecution

public enum HealthTransformFailure: String, Error, Codable, Equatable, Sendable {
    case unsupported
    case disabled
    case notReady = "not_ready"
    case modelUnavailable = "model_unavailable"
    case invalidInput = "invalid_input"
    case invalidOutput = "invalid_output"
    case deadlineExceeded = "deadline_exceeded"
    case cancelled
    case policyDenied = "policy_denied"
    case busy
    case conflict
    case notFound = "not_found"
}

public struct HealthTransformBinding: Codable, Equatable, Sendable {
    public let requestID: UUID
    public let hostEpoch: String
    public let personID: UUID
    public let deviceID: String
    public let nativeSubjectFingerprint: String

    private enum CodingKeys: String, CodingKey {
        case requestID = "request_id"
        case hostEpoch = "host_epoch"
        case personID = "person_id"
        case deviceID = "device_id"
        case nativeSubjectFingerprint = "native_subject_fingerprint"
    }

    public init(
        requestID: UUID,
        hostEpoch: String,
        personID: UUID,
        deviceID: String,
        nativeSubjectFingerprint: String
    ) {
        self.requestID = requestID
        self.hostEpoch = hostEpoch
        self.personID = personID
        self.deviceID = deviceID
        self.nativeSubjectFingerprint = nativeSubjectFingerprint
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.requestID == rhs.requestID && lhs.personID == rhs.personID
            && lhs.hostEpoch.utf8.elementsEqual(rhs.hostEpoch.utf8)
            && lhs.deviceID.utf8.elementsEqual(rhs.deviceID.utf8)
            && lhs.nativeSubjectFingerprint == rhs.nativeSubjectFingerprint
    }

    public func validate() throws {
        guard requestID != Self.zeroUUID,
              personID != Self.zeroUUID,
              !hostEpoch.isEmpty,
              hostEpoch.utf8.count <= 128,
              !hostEpoch.contains(where: { $0.isWhitespace }),
              !hostEpoch.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              !deviceID.isEmpty,
              deviceID.utf8.count <= 128,
              !deviceID.contains(where: { $0.isWhitespace }),
              !deviceID.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              HealthTransformWireValidation.isLowerHexDigest(nativeSubjectFingerprint) else {
            throw HealthTransformFailure.invalidInput
        }
    }

    public static func decode(_ data: Data) throws -> HealthTransformBinding {
        let value: JSONValue
        do {
            value = try JSONValue.decode(data, maximumBytes: HealthTransformWireLimits.commandBytes)
        } catch {
            throw HealthTransformFailure.invalidInput
        }
        guard case .object(let fields) = value,
              Set(fields.keys) == HealthTransformWireValidation.bindingFields else {
            throw HealthTransformFailure.invalidInput
        }
        do {
            let binding = try JSONDecoder().decode(Self.self, from: data)
            try binding.validate()
            return binding
        } catch let failure as HealthTransformFailure {
            throw failure
        } catch {
            throw HealthTransformFailure.invalidInput
        }
    }

    private static let zeroUUID = UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
}

public struct HealthTransformProof: Codable, Equatable, Sendable {
    public let operationID: UUID
    public let outputSHA256: String

    private enum CodingKeys: String, CodingKey {
        case operationID = "operation_id"
        case outputSHA256 = "output_sha256"
    }

    public init(operationID: UUID, outputSHA256: String) {
        self.operationID = operationID
        self.outputSHA256 = outputSHA256
    }
}

public struct HealthTransformSuccess: Equatable, Sendable {
    public let output: HealthTransformOutput
    public let proof: HealthTransformProof
    public let transformedAtUnixMs: Int64
    public let expiresAtUnixMs: Int64

    public init(
        output: HealthTransformOutput,
        proof: HealthTransformProof,
        transformedAtUnixMs: Int64,
        expiresAtUnixMs: Int64
    ) {
        self.output = output
        self.proof = proof
        self.transformedAtUnixMs = transformedAtUnixMs
        self.expiresAtUnixMs = expiresAtUnixMs
    }
}

public struct HealthTransformCommand: Codable, Equatable, Sendable {
    public let schemaVersion: Int
    public let operation: String
    public let requestID: UUID?
    public let input: HealthTransformInput?
    public let binding: HealthTransformBinding?
    public let outputSHA256: String?

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case operation
        case requestID = "request_id"
        case input
        case binding
        case outputSHA256 = "output_sha256"
    }

    public init(
        operation: String,
        requestID: UUID? = nil,
        input: HealthTransformInput? = nil,
        binding: HealthTransformBinding? = nil,
        outputSHA256: String? = nil
    ) {
        schemaVersion = 1
        self.operation = operation
        self.requestID = requestID
        self.input = input
        self.binding = binding
        self.outputSHA256 = outputSHA256
    }

    public func validate() throws {
        guard schemaVersion == 1 else { throw HealthTransformFailure.invalidInput }
        switch operation {
        case "availability":
            guard requestID == nil, input == nil, binding == nil, outputSHA256 == nil else {
                throw HealthTransformFailure.invalidInput
            }
        case "start":
            guard let requestID, requestID != Self.zeroUUID,
                  let input, let binding, outputSHA256 == nil else {
                throw HealthTransformFailure.invalidInput
            }
            do {
                try input.validate()
                try binding.validate()
            } catch {
                throw HealthTransformFailure.invalidInput
            }
        case "consume_receipt":
            guard let requestID, requestID != Self.zeroUUID,
                  input == nil,
                  let binding,
                  let outputSHA256,
                  HealthTransformWireValidation.isLowerHexDigest(outputSHA256) else {
                throw HealthTransformFailure.invalidInput
            }
            try binding.validate()
        case "poll", "cancel", "release":
            guard let requestID, requestID != Self.zeroUUID,
                  input == nil, binding == nil, outputSHA256 == nil else {
                throw HealthTransformFailure.invalidInput
            }
        default:
            throw HealthTransformFailure.invalidInput
        }
    }

    private static let zeroUUID = UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
}

public struct HealthTransformReply: Codable, Equatable, Sendable {
    public let schemaVersion: Int
    public let status: String
    public let requestID: UUID?
    public let availability: String?
    public let output: HealthTransformOutput?
    public let failure: HealthTransformFailure?
    public let binding: HealthTransformBinding?
    public let outputSHA256: String?
    public let transformedAtUnixMs: Int64?
    public let expiresAtUnixMs: Int64?

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case status
        case requestID = "request_id"
        case availability
        case output
        case failure
        case binding
        case outputSHA256 = "output_sha256"
        case transformedAtUnixMs = "transformed_at_unix_ms"
        case expiresAtUnixMs = "expires_at_unix_ms"
    }

    public init(
        status: String,
        requestID: UUID? = nil,
        availability: String? = nil,
        output: HealthTransformOutput? = nil,
        failure: HealthTransformFailure? = nil,
        binding: HealthTransformBinding? = nil,
        outputSHA256: String? = nil,
        transformedAtUnixMs: Int64? = nil,
        expiresAtUnixMs: Int64? = nil
    ) {
        schemaVersion = 1
        self.status = status
        self.requestID = requestID
        self.availability = availability
        self.output = output
        self.failure = failure
        self.binding = binding
        self.outputSHA256 = outputSHA256
        self.transformedAtUnixMs = transformedAtUnixMs
        self.expiresAtUnixMs = expiresAtUnixMs
    }

    public func validate() throws {
        guard schemaVersion == 1 else { throw HealthTransformFailure.invalidOutput }
        switch status {
        case "availability":
            guard requestID == nil,
                  let availability,
                  HealthTransformWireValidation.availabilityValues.contains(availability),
                  output == nil, failure == nil, binding == nil, outputSHA256 == nil,
                  transformedAtUnixMs == nil, expiresAtUnixMs == nil else {
                throw HealthTransformFailure.invalidOutput
            }
        case "pending", "released":
            guard let requestID, requestID != Self.zeroUUID,
                  availability == nil, output == nil, failure == nil, binding == nil,
                  outputSHA256 == nil, transformedAtUnixMs == nil, expiresAtUnixMs == nil else {
                throw HealthTransformFailure.invalidOutput
            }
        case "error":
            guard let failure,
                  requestID.map({ $0 != Self.zeroUUID }) ?? true,
                  output == nil, binding == nil, outputSHA256 == nil,
                  transformedAtUnixMs == nil, expiresAtUnixMs == nil else {
                throw HealthTransformFailure.invalidOutput
            }
            if let availability {
                guard requestID != nil,
                      HealthTransformWireValidation.unavailableValues.contains(availability),
                      failure.rawValue == availability else {
                    throw HealthTransformFailure.invalidOutput
                }
            }
        case "done", "receipt":
            guard let requestID, requestID != Self.zeroUUID,
                  availability == nil, let output, failure == nil,
                  let binding, let outputSHA256,
                  HealthTransformWireValidation.isLowerHexDigest(outputSHA256),
                  let transformedAtUnixMs, transformedAtUnixMs > 0,
                  let expiresAtUnixMs else {
                throw HealthTransformFailure.invalidOutput
            }
            try binding.validateAsOutput()
            let (expectedExpiry, overflow) = transformedAtUnixMs.addingReportingOverflow(HealthTransformWireLimits.receiptLifetimeMilliseconds)
            guard !overflow, expiresAtUnixMs == expectedExpiry else {
                throw HealthTransformFailure.invalidOutput
            }
            _ = output
        default:
            throw HealthTransformFailure.invalidOutput
        }
    }

    private static let zeroUUID = UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
}

public enum HealthTransformWireCodec {
    public static func decodeCommand(_ data: Data) throws -> HealthTransformCommand {
        let value: JSONValue
        do {
            value = try JSONValue.decode(data, maximumBytes: HealthTransformWireLimits.commandBytes)
        } catch {
            throw HealthTransformFailure.invalidInput
        }
        let fields = try HealthTransformWireValidation.object(value, failure: .invalidInput)
        guard case .number(let version)? = fields["schema_version"], version == 1,
              case .string(let operation)? = fields["operation"],
              let expectedKeys = HealthTransformWireValidation.commandFields[operation],
              fields.count == expectedKeys.count,
              Set(fields.keys) == expectedKeys else {
            throw HealthTransformFailure.invalidInput
        }
        if operation == "start" {
            guard case .object(let inputFields)? = fields["input"],
                  inputFields.count == Set(inputFields.keys).count,
                  Set(inputFields.keys).isSubset(of: HealthTransformWireValidation.inputFields) else {
                throw HealthTransformFailure.invalidInput
            }
            try HealthTransformWireValidation.requireObjectFields(
                fields["binding"],
                exact: HealthTransformWireValidation.bindingFields,
                failure: .invalidInput
            )
        } else if operation == "consume_receipt" {
            try HealthTransformWireValidation.requireObjectFields(
                fields["binding"],
                exact: HealthTransformWireValidation.bindingFields,
                failure: .invalidInput
            )
        }
        do {
            let command = try JSONDecoder().decode(HealthTransformCommand.self, from: data)
            try command.validate()
            return command
        } catch let failure as HealthTransformFailure {
            throw failure
        } catch {
            throw HealthTransformFailure.invalidInput
        }
    }

    public static func decodeReply(_ data: Data) throws -> HealthTransformReply {
        let value: JSONValue
        do {
            value = try JSONValue.decode(data, maximumBytes: HealthTransformWireLimits.replyBytes)
        } catch {
            throw HealthTransformFailure.invalidOutput
        }
        let fields = try HealthTransformWireValidation.object(value, failure: .invalidOutput)
        guard case .number(let version)? = fields["schema_version"], version == 1,
              case .string(let status)? = fields["status"],
              let expectedKeys = HealthTransformWireValidation.replyFields[status] else {
            throw HealthTransformFailure.invalidOutput
        }
        let actualKeys = Set(fields.keys)
        let validKeys = status == "error"
            ? HealthTransformWireValidation.replyKeySet(fields: fields, status: status)
            : actualKeys == expectedKeys
        guard fields.count == actualKeys.count, validKeys else {
            throw HealthTransformFailure.invalidOutput
        }
        if status == "error" {
            if let requestIDValue = fields["request_id"], case .null = requestIDValue {
                throw HealthTransformFailure.invalidOutput
            }
            if let availabilityValue = fields["availability"], case .null = availabilityValue {
                throw HealthTransformFailure.invalidOutput
            }
        }
        if status == "done" || status == "receipt" {
            try HealthTransformWireValidation.requireObjectFields(
                fields["output"],
                exact: HealthTransformWireValidation.outputFields,
                failure: .invalidOutput
            )
            try HealthTransformWireValidation.requireObjectFields(
                fields["binding"],
                exact: HealthTransformWireValidation.bindingFields,
                failure: .invalidOutput
            )
        }
        do {
            let reply = try JSONDecoder().decode(HealthTransformReply.self, from: data)
            try reply.validate()
            return reply
        } catch let failure as HealthTransformFailure {
            throw failure
        } catch {
            throw HealthTransformFailure.invalidOutput
        }
    }

    public static func encodeCommand(_ command: HealthTransformCommand) throws -> Data {
        do {
            try command.validate()
            let data = try JSONEncoder().encode(command)
            guard data.count <= HealthTransformWireLimits.commandBytes else {
                throw HealthTransformFailure.invalidInput
            }
            return data
        } catch let failure as HealthTransformFailure {
            throw failure
        } catch {
            throw HealthTransformFailure.invalidInput
        }
    }

    public static func encodeReply(_ reply: HealthTransformReply) throws -> Data {
        do {
            try reply.validate()
            let data = try JSONEncoder().encode(reply)
            guard data.count <= HealthTransformWireLimits.replyBytes else {
                throw HealthTransformFailure.invalidOutput
            }
            return data
        } catch let failure as HealthTransformFailure {
            throw failure
        } catch {
            throw HealthTransformFailure.invalidOutput
        }
    }
}

public enum HealthTransformDigest {
    public static func hex(_ output: HealthTransformOutput) -> String {
        let bytes = Data("floe.health.transform.v1\0\(output.capacity.rawValue)\0\(output.recovery.rawValue)".utf8)
        return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
    }
}

enum HealthTransformWireLimits {
    static let commandBytes = 8_192
    static let replyBytes = 4_096
    static let receiptLifetimeMilliseconds: Int64 = 30 * 60 * 1_000
}

private enum HealthTransformWireValidation {
    static let inputFields: Set<String> = ["sleep_hours", "steps", "exercise_minutes"]
    static let bindingFields: Set<String> = ["request_id", "host_epoch", "person_id", "device_id", "native_subject_fingerprint"]
    static let outputFields: Set<String> = ["capacity", "recovery"]
    static let availabilityValues: Set<String> = ["available", "unsupported", "disabled", "not_ready"]
    static let unavailableValues: Set<String> = ["unsupported", "disabled", "not_ready"]

    static let commandFields: [String: Set<String>] = [
        "availability": ["schema_version", "operation"],
        "start": ["schema_version", "operation", "request_id", "input", "binding"],
        "consume_receipt": ["schema_version", "operation", "request_id", "output_sha256", "binding"],
        "poll": ["schema_version", "operation", "request_id"],
        "cancel": ["schema_version", "operation", "request_id"],
        "release": ["schema_version", "operation", "request_id"]
    ]

    static let replyFields: [String: Set<String>] = [
        "availability": ["schema_version", "status", "availability"],
        "pending": ["schema_version", "status", "request_id"],
        "released": ["schema_version", "status", "request_id"],
        "error": ["schema_version", "status", "failure"],
        "done": ["schema_version", "status", "request_id", "output", "binding", "output_sha256", "transformed_at_unix_ms", "expires_at_unix_ms"],
        "receipt": ["schema_version", "status", "request_id", "output", "binding", "output_sha256", "transformed_at_unix_ms", "expires_at_unix_ms"]
    ]

    static func object(_ value: JSONValue, failure: HealthTransformFailure) throws -> JSONObject {
        guard case .object(let fields) = value else { throw failure }
        return fields
    }

    static func requireObjectFields(
        _ value: JSONValue?,
        exact expected: Set<String>,
        failure: HealthTransformFailure
    ) throws {
        guard let value, case .object(let fields) = value,
              fields.count == expected.count, Set(fields.keys) == expected else {
            throw failure
        }
    }

    static func isLowerHexDigest(_ value: String) -> Bool {
        value.utf8.count == 64 && value.utf8.allSatisfy {
            (48...57).contains($0) || (97...102).contains($0)
        }
    }

    static func replyKeySet(fields: JSONObject, status: String) -> Bool {
        guard status == "error" else { return true }
        let base: Set<String> = ["schema_version", "status", "failure"]
        let withRequest: Set<String> = base.union(["request_id"])
        let withAvailability: Set<String> = withRequest.union(["availability"])
        let keys = Set(fields.keys)
        return fields.count == keys.count &&
            (keys == base || keys == withRequest || keys == withAvailability)
    }
}

private extension HealthTransformBinding {
    func validateAsOutput() throws {
        do {
            try validate()
        } catch {
            throw HealthTransformFailure.invalidOutput
        }
    }
}
