import Foundation

public enum DeviceModelCapability: String, Sendable, Equatable, Hashable, Codable {
    case text
    case structuredOutput = "structured_output"
    case toolProposals = "tool_proposals"
}

public struct DeviceModelRequirements: Sendable, Equatable {
    public let capabilities: [DeviceModelCapability]

    public init(capabilities: [DeviceModelCapability]) {
        self.capabilities = capabilities
    }

    public func validate() throws {
        try DeviceModelWire.validateCapabilities(capabilities)
    }

    var wireValue: JSONValue {
        .object(["capabilities": DeviceModelWire.capabilitiesValue(capabilities)])
    }
}

public struct DeviceModelLimits: Sendable, Equatable {
    public let maxInputBytes: Int
    public let maxInstructionsBytes: Int
    public let maxOutputBytes: Int
    public let maxResponseTokens: Int
    public let maxDeadlineMilliseconds: Int

    public init(
        maxInputBytes: Int,
        maxInstructionsBytes: Int,
        maxOutputBytes: Int,
        maxResponseTokens: Int,
        maxDeadlineMilliseconds: Int
    ) {
        self.maxInputBytes = maxInputBytes
        self.maxInstructionsBytes = maxInstructionsBytes
        self.maxOutputBytes = maxOutputBytes
        self.maxResponseTokens = maxResponseTokens
        self.maxDeadlineMilliseconds = maxDeadlineMilliseconds
    }

    public func validate() throws {
        guard (1...DeviceModelBounds.maxInputBytes).contains(maxInputBytes),
              (1...DeviceModelBounds.maxInstructionsBytes).contains(maxInstructionsBytes),
              (1...DeviceModelBounds.maxOutputBytes).contains(maxOutputBytes),
              (1...DeviceModelBounds.maxResponseTokens).contains(maxResponseTokens),
              (1...DeviceModelBounds.maxDeadlineMilliseconds).contains(maxDeadlineMilliseconds) else {
            throw DeviceModelContractError.invalidValue
        }
    }

    var wireValue: JSONValue {
        .object([
            "max_input_bytes": .number(Double(maxInputBytes)),
            "max_instructions_bytes": .number(Double(maxInstructionsBytes)),
            "max_output_bytes": .number(Double(maxOutputBytes)),
            "max_response_tokens": .number(Double(maxResponseTokens)),
            "max_deadline_milliseconds": .number(Double(maxDeadlineMilliseconds))
        ])
    }
}

public struct DeviceModelProfile: Sendable, Equatable {
    public let bindingID: String
    public let capabilities: [DeviceModelCapability]
    public let limits: DeviceModelLimits

    public init(bindingID: String, capabilities: [DeviceModelCapability], limits: DeviceModelLimits) {
        self.bindingID = bindingID
        self.capabilities = capabilities
        self.limits = limits
    }

    public func validate() throws {
        guard DeviceModelWire.isHex64(bindingID) else { throw DeviceModelContractError.invalidValue }
        try DeviceModelWire.validateCapabilities(capabilities)
        try limits.validate()
    }

    public func validate(request: DeviceModelRequest) throws {
        try validate()
        try request.validate()
        guard bindingID == request.bindingID else { throw DeviceModelContractError.invalidValue }

        var required: Set<DeviceModelCapability> = []
        switch request.outputFormat {
        case .text:
            required.insert(.text)
        case .json:
            required.insert(.structuredOutput)
        }
        if !request.tools.isEmpty { required.insert(.toolProposals) }
        let available = Set(capabilities)
        guard required.isSubset(of: available) else { throw DeviceModelContractError.invalidValue }

        let inputBytes = jsonEncodedByteCount(request.input, maximum: limits.maxInputBytes)
        guard inputBytes <= limits.maxInputBytes,
              request.instructions.utf8.count <= limits.maxInstructionsBytes,
              request.maxOutputBytes <= limits.maxOutputBytes,
              request.maxResponseTokens <= limits.maxResponseTokens,
              request.deadlineMilliseconds <= limits.maxDeadlineMilliseconds else {
            throw DeviceModelContractError.invalidValue
        }
    }

    var wireValue: JSONValue {
        .object([
            "binding_id": .string(bindingID),
            "capabilities": DeviceModelWire.capabilitiesValue(capabilities),
            "limits": limits.wireValue
        ])
    }
}

public enum DeviceModelUnavailable: String, Sendable, Equatable, Codable {
    case unsupported
    case disabled
    case notReady = "not_ready"
}

public enum DeviceModelObservation: Sendable, Equatable {
    case available(DeviceModelProfile)
    case unavailable(DeviceModelUnavailable)

    public func validate() throws {
        if case .available(let profile) = self { try profile.validate() }
    }

    static func fromWireValue(_ value: JSONValue) throws -> DeviceModelObservation {
        let fields = try DeviceModelWire.object(value)
        switch try DeviceModelWire.string(fields["status"]) {
        case "available":
            let exact = try DeviceModelWire.exactObject(value, keys: ["status", "binding_id", "capabilities", "limits"])
            let profile = DeviceModelProfile(
                bindingID: try DeviceModelWire.string(exact["binding_id"]),
                capabilities: try DeviceModelWire.capabilities(exact["capabilities"]),
                limits: try DeviceModelLimits.fromWireValue(exact["limits"])
            )
            try profile.validate()
            return .available(profile)
        case "unavailable":
            let exact = try DeviceModelWire.exactObject(value, keys: ["status", "reason"])
            guard let reason = DeviceModelUnavailable(rawValue: try DeviceModelWire.string(exact["reason"])) else {
                throw DeviceModelContractError.invalidValue
            }
            return .unavailable(reason)
        default:
            throw DeviceModelContractError.invalidValue
        }
    }

    var wireValue: JSONValue {
        switch self {
        case .available(let profile):
            let fields: JSONObject = [
                "status": .string("available"),
                "binding_id": .string(profile.bindingID),
                "capabilities": DeviceModelWire.capabilitiesValue(profile.capabilities),
                "limits": profile.limits.wireValue
            ]
            return .object(fields)
        case .unavailable(let reason):
            return .object(["status": .string("unavailable"), "reason": .string(reason.rawValue)])
        }
    }
}

public enum ModelOutputFormat: Sendable, Equatable {
    case text
    case json(schema: ModelSchema)

    static func fromWireValue(_ value: JSONValue) throws -> ModelOutputFormat {
        let fields = try DeviceModelWire.object(value)
        switch try DeviceModelWire.string(fields["kind"]) {
        case "text":
            _ = try DeviceModelWire.exactObject(value, keys: ["kind"])
            return .text
        case "json":
            let exact = try DeviceModelWire.exactObject(value, keys: ["kind", "schema"])
            guard let schemaJSON = exact["schema"] else { throw DeviceModelContractError.invalidValue }
            return .json(schema: try ModelSchema(json: schemaJSON))
        default:
            throw DeviceModelContractError.invalidValue
        }
    }

    var wireValue: JSONValue {
        switch self {
        case .text:
            return .object(["kind": .string("text")])
        case .json(let schema):
            return .object(["kind": .string("json"), "schema": schema.json])
        }
    }
}

public struct DeviceTool: Sendable, Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool {
        utf8Equal(lhs.name, rhs.name) && utf8Equal(lhs.description, rhs.description)
            && lhs.inputSchema == rhs.inputSchema
    }

    public let name: String
    public let description: String
    public let inputSchema: ModelSchema

    public init(name: String, description: String, inputSchema: ModelSchema) {
        self.name = name
        self.description = description
        self.inputSchema = inputSchema
    }

    public func validate() throws {
        guard DeviceModelWire.validToolName(name),
              (1...2_048).contains(description.utf8.count) else {
            throw DeviceModelContractError.invalidValue
        }
        _ = try ModelSchema(json: inputSchema.json)
    }

    var wireValue: JSONValue {
        .object([
            "name": .string(name),
            "description": .string(description),
            "input_schema": inputSchema.json
        ])
    }
}

public struct DeviceModelRequest: Sendable, Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.operationID == rhs.operationID && utf8Equal(lhs.bindingID, rhs.bindingID)
            && utf8Equal(lhs.instructions, rhs.instructions) && lhs.input == rhs.input
            && lhs.outputFormat == rhs.outputFormat && lhs.tools == rhs.tools
            && lhs.maxResponseTokens == rhs.maxResponseTokens
            && lhs.maxOutputBytes == rhs.maxOutputBytes
            && lhs.deadlineMilliseconds == rhs.deadlineMilliseconds
    }

    public let operationID: UUID
    public let bindingID: String
    public let instructions: String
    public let input: JSONValue
    public let outputFormat: ModelOutputFormat
    public let tools: [DeviceTool]
    public let maxResponseTokens: Int
    public let maxOutputBytes: Int
    public let deadlineMilliseconds: Int

    public init(
        operationID: UUID,
        bindingID: String,
        instructions: String,
        input: JSONValue,
        outputFormat: ModelOutputFormat,
        tools: [DeviceTool],
        maxResponseTokens: Int,
        maxOutputBytes: Int,
        deadlineMilliseconds: Int
    ) {
        self.operationID = operationID
        self.bindingID = bindingID
        self.instructions = instructions
        self.input = input
        self.outputFormat = outputFormat
        self.tools = tools
        self.maxResponseTokens = maxResponseTokens
        self.maxOutputBytes = maxOutputBytes
        self.deadlineMilliseconds = deadlineMilliseconds
    }

    public func validate() throws {
        guard operationID != UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)),
              DeviceModelWire.isHex64(bindingID),
              !instructions.unicodeScalars.allSatisfy(\.properties.isWhitespace),
              instructions.utf8.count <= DeviceModelBounds.maxInstructionsBytes,
              (1...DeviceModelBounds.maxResponseTokens).contains(maxResponseTokens),
              (1...DeviceModelBounds.maxOutputBytes).contains(maxOutputBytes),
              (1...DeviceModelBounds.maxDeadlineMilliseconds).contains(deadlineMilliseconds),
              case .object = input else {
            throw DeviceModelContractError.invalidValue
        }
        try validateJSONValue(input)
        guard jsonEncodedByteCount(input, maximum: DeviceModelBounds.maxInputBytes) <= DeviceModelBounds.maxInputBytes else {
            throw DeviceModelContractError.byteLimitExceeded
        }
        if case .json(let schema) = outputFormat {
            _ = try ModelSchema(json: schema.json)
            guard tools.isEmpty else { throw DeviceModelContractError.invalidValue }
        }
        guard tools.count <= 64 else { throw DeviceModelContractError.invalidValue }
        var toolNames = Set<String>()
        for tool in tools {
            try tool.validate()
            guard toolNames.insert(tool.name).inserted else { throw DeviceModelContractError.invalidValue }
        }
        guard jsonEncodedByteCount(wireValue, maximum: DeviceModelBounds.maxRequestBytes) <= DeviceModelBounds.maxRequestBytes else {
            throw DeviceModelContractError.byteLimitExceeded
        }
    }

    static func fromWireValue(_ value: JSONValue) throws -> DeviceModelRequest {
        let fields = try DeviceModelWire.exactObject(value, keys: [
            "operation_id", "binding_id", "instructions", "input", "output_format",
            "tools", "max_response_tokens", "max_output_bytes", "deadline_milliseconds"
        ])
        let request = DeviceModelRequest(
            operationID: try DeviceModelWire.uuid(fields["operation_id"]),
            bindingID: try DeviceModelWire.string(fields["binding_id"]),
            instructions: try DeviceModelWire.string(fields["instructions"]),
            input: try DeviceModelWire.required(fields["input"]),
            outputFormat: try ModelOutputFormat.fromWireValue(DeviceModelWire.required(fields["output_format"])),
            tools: try DeviceModelWire.tools(fields["tools"]),
            maxResponseTokens: try DeviceModelWire.integer(fields["max_response_tokens"]),
            maxOutputBytes: try DeviceModelWire.integer(fields["max_output_bytes"]),
            deadlineMilliseconds: try DeviceModelWire.integer(fields["deadline_milliseconds"])
        )
        try request.validate()
        return request
    }

    var wireValue: JSONValue {
        .object([
            "operation_id": DeviceModelWire.uuidValue(operationID),
            "binding_id": .string(bindingID),
            "instructions": .string(instructions),
            "input": input,
            "output_format": outputFormat.wireValue,
            "tools": .array(tools.map(\.wireValue)),
            "max_response_tokens": .number(Double(maxResponseTokens)),
            "max_output_bytes": .number(Double(maxOutputBytes)),
            "deadline_milliseconds": .number(Double(deadlineMilliseconds))
        ])
    }
}

public struct DeviceModelUsage: Sendable, Equatable {
    public let tokens: UInt64?
    public let costMicros: UInt64?

    public init(tokens: UInt64?, costMicros: UInt64?) {
        self.tokens = tokens
        self.costMicros = costMicros
    }

    public func validate() throws {
        guard tokens.map({ $0 <= UInt64(deviceModelMaximumSafeInteger) }) ?? true,
              costMicros.map({ $0 <= UInt64(deviceModelMaximumSafeInteger) }) ?? true else {
            throw DeviceModelContractError.invalidValue
        }
    }

    var wireValue: JSONValue {
        .object([
            "tokens": tokens.map { .number(Double($0)) } ?? .null,
            "cost_micros": costMicros.map { .number(Double($0)) } ?? .null
        ])
    }
}

public enum DeviceModelFailure: String, Error, Sendable, Equatable, Codable {
    case unsupported
    case disabled
    case notReady = "not_ready"
    case unavailable
    case invalidInput = "invalid_input"
    case invalidOutput = "invalid_output"
    case deadlineExceeded = "deadline_exceeded"
    case cancelled
    case policyDenied = "policy_denied"
    case quotaExceeded = "quota_exceeded"
    case budgetExceeded = "budget_exceeded"
    case busy
    case conflict
    case notFound = "not_found"
}

public enum DeviceModelOutput: Sendable, Equatable {
    public static func == (lhs: Self, rhs: Self) -> Bool {
        switch (lhs, rhs) {
        case (.text(let lhs), .text(let rhs)): return utf8Equal(lhs, rhs)
        case (.json(let lhs), .json(let rhs)): return lhs == rhs
        case (.toolProposal(let ln, let li), .toolProposal(let rn, let ri)):
            return utf8Equal(ln, rn) && li == ri
        case (.failure(let lhs), .failure(let rhs)): return lhs == rhs
        default: return false
        }
    }

    case text(String)
    case json(JSONValue)
    case toolProposal(name: String, input: JSONValue)
    case failure(DeviceModelFailure)

    static func fromWireValue(_ value: JSONValue) throws -> DeviceModelOutput {
        let fields = try DeviceModelWire.object(value)
        switch try DeviceModelWire.string(fields["kind"]) {
        case "text":
            let exact = try DeviceModelWire.exactObject(value, keys: ["kind", "text"])
            return .text(try DeviceModelWire.string(exact["text"]))
        case "json":
            let exact = try DeviceModelWire.exactObject(value, keys: ["kind", "value"])
            return .json(try DeviceModelWire.required(exact["value"]))
        case "tool_proposal":
            let exact = try DeviceModelWire.exactObject(value, keys: ["kind", "name", "input"])
            return .toolProposal(
                name: try DeviceModelWire.string(exact["name"]),
                input: try DeviceModelWire.required(exact["input"])
            )
        case "failure":
            let exact = try DeviceModelWire.exactObject(value, keys: ["kind", "failure"])
            guard let failure = DeviceModelFailure(rawValue: try DeviceModelWire.string(exact["failure"])) else {
                throw DeviceModelContractError.invalidValue
            }
            return .failure(failure)
        default:
            throw DeviceModelContractError.invalidValue
        }
    }

    var wireValue: JSONValue {
        switch self {
        case .text(let text):
            return .object(["kind": .string("text"), "text": .string(text)])
        case .json(let value):
            return .object(["kind": .string("json"), "value": value])
        case .toolProposal(let name, let input):
            return .object(["kind": .string("tool_proposal"), "name": .string(name), "input": input])
        case .failure(let failure):
            return .object(["kind": .string("failure"), "failure": .string(failure.rawValue)])
        }
    }
}

public struct DeviceModelResponse: Sendable, Equatable {
    public let operationID: UUID
    public let bindingID: String
    public let output: DeviceModelOutput
    public let usage: DeviceModelUsage

    public init(operationID: UUID, bindingID: String, output: DeviceModelOutput, usage: DeviceModelUsage) {
        self.operationID = operationID
        self.bindingID = bindingID
        self.output = output
        self.usage = usage
    }

    public func validateEnvelope(for request: DeviceModelRequest) throws {
        guard operationID == request.operationID, bindingID == request.bindingID else {
            throw DeviceModelContractError.invalidValue
        }
        try usage.validate()
        guard jsonEncodedByteCount(wireValue, maximum: DeviceModelBounds.maxResponseEnvelopeBytes, tolerateInvalidNumbers: true)
                <= DeviceModelBounds.maxResponseEnvelopeBytes else {
            throw DeviceModelContractError.byteLimitExceeded
        }
    }

    public func normalized(for request: DeviceModelRequest) throws -> DeviceModelResponse {
        // Validate identity, envelope size and trusted usage before content can be converted to invalid_output.
        try validateEnvelope(for: request)
        do {
            try validateSuccessfulOutput(for: request)
            return self
        } catch {
            return DeviceModelResponse(
                operationID: operationID,
                bindingID: bindingID,
                output: .failure(.invalidOutput),
                usage: usage
            )
        }
    }

    var wireValue: JSONValue {
        .object([
            "operation_id": DeviceModelWire.uuidValue(operationID),
            "binding_id": .string(bindingID),
            "output": output.wireValue,
            "usage": usage.wireValue
        ])
    }

    private func validateSuccessfulOutput(for request: DeviceModelRequest) throws {
        if case .failure = output { return }
        guard jsonEncodedByteCount(output.wireValue, maximum: request.maxOutputBytes)
                <= request.maxOutputBytes else {
            throw DeviceModelContractError.byteLimitExceeded
        }
        switch output {
        case .failure:
            return
        case .text(let text):
            guard case .text = request.outputFormat,
                  !text.unicodeScalars.allSatisfy(\.properties.isWhitespace) else {
                throw DeviceModelContractError.invalidValue
            }
        case .json(let value):
            guard case .json(let schema) = request.outputFormat else {
                throw DeviceModelContractError.invalidValue
            }
            try schema.validate(value)
        case .toolProposal(let name, let input):
            guard case .text = request.outputFormat,
                  DeviceModelWire.validToolName(name),
                  case .object = input,
                  let advertisedTool = request.tools.first(where: { $0.name == name }) else {
                throw DeviceModelContractError.invalidValue
            }
            try advertisedTool.inputSchema.validate(input)
        }
    }
}

public protocol DeviceModel: Sendable {
    func prepare(_ requirements: DeviceModelRequirements) throws -> DeviceModelObservation
    func generate(_ request: DeviceModelRequest) async throws -> DeviceModelResponse
}

public enum DeviceModelCommand: Sendable, Equatable {
    case prepare(DeviceModelRequirements)
    case start(DeviceModelRequest)
    case poll(UUID)
    case cancel(UUID)
    case release(UUID)

    static func fromWireValue(_ value: JSONValue) throws -> DeviceModelCommand {
        let fields = try DeviceModelWire.object(value)
        guard try DeviceModelWire.integer(fields["schema_version"]) == 1 else {
            throw DeviceModelContractError.invalidValue
        }
        switch try DeviceModelWire.string(fields["operation"]) {
        case "prepare":
            let exact = try DeviceModelWire.exactObject(value, keys: ["schema_version", "operation", "requirements"])
            let requirements = try DeviceModelRequirements.fromWireValue(exact["requirements"])
            try requirements.validate()
            return .prepare(requirements)
        case "start":
            let exact = try DeviceModelWire.exactObject(value, keys: ["schema_version", "operation", "request"])
            return .start(try DeviceModelRequest.fromWireValue(DeviceModelWire.required(exact["request"])))
        case "poll", "cancel", "release":
            let exact = try DeviceModelWire.exactObject(value, keys: ["schema_version", "operation", "operation_id"])
            let operationID = try DeviceModelWire.uuid(exact["operation_id"])
            switch try DeviceModelWire.string(fields["operation"]) {
            case "poll": return .poll(operationID)
            case "cancel": return .cancel(operationID)
            default: return .release(operationID)
            }
        default:
            throw DeviceModelContractError.invalidValue
        }
    }

    var wireValue: JSONValue {
        switch self {
        case .prepare(let requirements):
            return .object([
                "schema_version": .number(1),
                "operation": .string("prepare"),
                "requirements": requirements.wireValue
            ])
        case .start(let request):
            return .object([
                "schema_version": .number(1),
                "operation": .string("start"),
                "request": request.wireValue
            ])
        case .poll(let operationID):
            return operationWireValue("poll", operationID: operationID)
        case .cancel(let operationID):
            return operationWireValue("cancel", operationID: operationID)
        case .release(let operationID):
            return operationWireValue("release", operationID: operationID)
        }
    }
}

public enum DeviceModelReply: Sendable, Equatable {
    case observation(DeviceModelObservation)
    case pending(UUID)
    case done(DeviceModelResponse)
    case error(operationID: UUID?, failure: DeviceModelFailure)
    case released(UUID)

    static func fromWireValue(_ value: JSONValue) throws -> DeviceModelReply {
        let fields = try DeviceModelWire.object(value)
        guard try DeviceModelWire.integer(fields["schema_version"]) == 1 else {
            throw DeviceModelContractError.invalidValue
        }
        switch try DeviceModelWire.string(fields["status"]) {
        case "observation":
            let exact = try DeviceModelWire.exactObject(value, keys: ["schema_version", "status", "observation"])
            return .observation(try DeviceModelObservation.fromWireValue(DeviceModelWire.required(exact["observation"])))
        case "pending":
            let exact = try DeviceModelWire.exactObject(value, keys: ["schema_version", "status", "operation_id"])
            return .pending(try DeviceModelWire.uuid(exact["operation_id"]))
        case "done":
            let exact = try DeviceModelWire.exactObject(value, keys: ["schema_version", "status", "response"])
            let response = try DeviceModelResponse.fromWireValue(DeviceModelWire.required(exact["response"]))
            return .done(response)
        case "error":
            let exact = try DeviceModelWire.exactObject(value, keys: ["schema_version", "status", "operation_id", "failure"])
            let operationID: UUID?
            if case .null? = exact["operation_id"] {
                operationID = nil
            } else {
                operationID = try DeviceModelWire.uuid(exact["operation_id"])
            }
            guard let failure = DeviceModelFailure(rawValue: try DeviceModelWire.string(exact["failure"])) else {
                throw DeviceModelContractError.invalidValue
            }
            return .error(operationID: operationID, failure: failure)
        case "released":
            let exact = try DeviceModelWire.exactObject(value, keys: ["schema_version", "status", "operation_id"])
            return .released(try DeviceModelWire.uuid(exact["operation_id"]))
        default:
            throw DeviceModelContractError.invalidValue
        }
    }

    var wireValue: JSONValue {
        switch self {
        case .observation(let observation):
            var object: JSONObject = ["schema_version": JSONValue.number(1), "status": .string("observation")]
            object["observation"] = observation.wireValue
            return .object(object)
        case .pending(let operationID):
            return statusOperationWireValue("pending", operationID: operationID)
        case .done(let response):
            return .object([
                "schema_version": .number(1),
                "status": .string("done"),
                "response": response.wireValue
            ])
        case .error(let operationID, let failure):
            return .object([
                "schema_version": .number(1),
                "status": .string("error"),
                "operation_id": operationID.map(DeviceModelWire.uuidValue) ?? .null,
                "failure": .string(failure.rawValue)
            ])
        case .released(let operationID):
            return statusOperationWireValue("released", operationID: operationID)
        }
    }
}

public enum DeviceModelCodec {
    public static func decodeCommand(_ data: Data) throws -> DeviceModelCommand {
        let value = try JSONValue.decode(data, maximumBytes: DeviceModelBounds.maxRequestBytes)
        return try DeviceModelCommand.fromWireValue(value)
    }

    public static func encodeReply(_ reply: DeviceModelReply) throws -> Data {
        switch reply {
        case .observation(let observation):
            try observation.validate()
        case .done(let response):
            guard DeviceModelWire.isHex64(response.bindingID) else { throw DeviceModelContractError.invalidValue }
            try response.usage.validate()
        case .pending, .error, .released:
            break
        }
        let value = reply.wireValue
        guard jsonEncodedByteCount(value, maximum: DeviceModelBounds.maxResponseEnvelopeBytes)
                <= DeviceModelBounds.maxResponseEnvelopeBytes else {
            throw DeviceModelContractError.byteLimitExceeded
        }
        let data = try value.encoded()
        guard data.count <= DeviceModelBounds.maxResponseEnvelopeBytes else {
            throw DeviceModelContractError.byteLimitExceeded
        }
        return data
    }
}

private enum DeviceModelBounds {
    static let maxInputBytes = 16_384
    static let maxInstructionsBytes = 9_216
    static let maxOutputBytes = 32_768
    static let maxResponseTokens = 4_096
    static let maxDeadlineMilliseconds = 30_000
    static let maxRequestBytes = 131_072
    static let maxResponseEnvelopeBytes = 65_536
}

private enum DeviceModelWire {
    static func exactObject(_ value: JSONValue, keys: [String]) throws -> JSONObject {
        guard case .object(let fields) = value, fields.keySet == Set(keys.map(JSONUTF8Key.init)) else {
            throw DeviceModelContractError.invalidValue
        }
        return fields
    }

    static func object(_ value: JSONValue) throws -> JSONObject {
        guard case .object(let fields) = value else { throw DeviceModelContractError.invalidValue }
        return fields
    }

    static func required(_ value: JSONValue?) throws -> JSONValue {
        guard let value else { throw DeviceModelContractError.invalidValue }
        return value
    }

    static func string(_ value: JSONValue?) throws -> String {
        guard case .string(let text)? = value else { throw DeviceModelContractError.invalidValue }
        return text
    }

    static func integer(_ value: JSONValue?) throws -> Int {
        guard case .number(let number)? = value,
              isSafeJSONNumber(number),
              number.rounded(.towardZero) == number,
              number >= Double(Int.min), number <= Double(Int.max) else {
            throw DeviceModelContractError.invalidValue
        }
        return Int(number)
    }

    static func optionalUInt64(_ value: JSONValue?) throws -> UInt64? {
        guard let value else { throw DeviceModelContractError.invalidValue }
        if case .null = value { return nil }
        guard case .number(let number) = value,
              isSafeJSONNumber(number),
              number.rounded(.towardZero) == number,
              number >= 0 else {
            throw DeviceModelContractError.invalidValue
        }
        return UInt64(number)
    }

    static func uuid(_ value: JSONValue?) throws -> UUID {
        let text = try string(value)
        let bytes = Array(text.utf8)
        guard bytes.count == 36,
              bytes[8] == 0x2D, bytes[13] == 0x2D, bytes[18] == 0x2D, bytes[23] == 0x2D,
              bytes.enumerated().allSatisfy({ pair in
                  [8, 13, 18, 23].contains(pair.offset) || isHexByte(pair.element)
              }),
              let id = UUID(uuidString: text) else {
            throw DeviceModelContractError.invalidValue
        }
        return id
    }

    static func uuidValue(_ value: UUID) -> JSONValue {
        .string(value.uuidString.lowercased())
    }

    static func isHex64(_ value: String) -> Bool {
        let bytes = Array(value.utf8)
        return bytes.count == 64 && bytes.allSatisfy { ($0 >= 0x30 && $0 <= 0x39) || ($0 >= 0x61 && $0 <= 0x66) }
    }

    static func validToolName(_ value: String) -> Bool {
        let bytes = Array(value.utf8)
        guard (1...64).contains(bytes.count) else { return false }
        return bytes.allSatisfy { byte in
            (byte >= 0x41 && byte <= 0x5A) ||
            (byte >= 0x61 && byte <= 0x7A) ||
            (byte >= 0x30 && byte <= 0x39) || byte == 0x5F || byte == 0x2D
        }
    }

    static func capabilities(_ value: JSONValue?) throws -> [DeviceModelCapability] {
        guard case .array(let values)? = value else { throw DeviceModelContractError.invalidValue }
        return try values.map { value in
            guard case .string(let raw) = value, let capability = DeviceModelCapability(rawValue: raw) else {
                throw DeviceModelContractError.invalidValue
            }
            return capability
        }
    }

    static func capabilitiesValue(_ values: [DeviceModelCapability]) -> JSONValue {
        .array(values.map { .string($0.rawValue) })
    }

    static func validateCapabilities(_ values: [DeviceModelCapability]) throws {
        let names = values.map(\.rawValue)
        guard !names.isEmpty,
              names == names.sorted(),
              Set(names).count == names.count else {
            throw DeviceModelContractError.invalidValue
        }
    }

    static func tools(_ value: JSONValue?) throws -> [DeviceTool] {
        guard case .array(let values)? = value else { throw DeviceModelContractError.invalidValue }
        return try values.map(DeviceTool.fromWireValue)
    }

    private static func isHexByte(_ byte: UInt8) -> Bool {
        (byte >= 0x30 && byte <= 0x39) ||
        (byte >= 0x41 && byte <= 0x46) ||
        (byte >= 0x61 && byte <= 0x66)
    }
}

extension DeviceModelLimits {
    static func fromWireValue(_ value: JSONValue?) throws -> DeviceModelLimits {
        let fields = try DeviceModelWire.exactObject(DeviceModelWire.required(value), keys: [
            "max_input_bytes", "max_instructions_bytes", "max_output_bytes",
            "max_response_tokens", "max_deadline_milliseconds"
        ])
        let limits = DeviceModelLimits(
            maxInputBytes: try DeviceModelWire.integer(fields["max_input_bytes"]),
            maxInstructionsBytes: try DeviceModelWire.integer(fields["max_instructions_bytes"]),
            maxOutputBytes: try DeviceModelWire.integer(fields["max_output_bytes"]),
            maxResponseTokens: try DeviceModelWire.integer(fields["max_response_tokens"]),
            maxDeadlineMilliseconds: try DeviceModelWire.integer(fields["max_deadline_milliseconds"])
        )
        try limits.validate()
        return limits
    }
}

extension DeviceTool {
    static func fromWireValue(_ value: JSONValue) throws -> DeviceTool {
        let fields = try DeviceModelWire.exactObject(value, keys: ["name", "description", "input_schema"])
        let tool = DeviceTool(
            name: try DeviceModelWire.string(fields["name"]),
            description: try DeviceModelWire.string(fields["description"]),
            inputSchema: try ModelSchema(json: DeviceModelWire.required(fields["input_schema"]))
        )
        try tool.validate()
        return tool
    }
}

extension DeviceModelRequirements {
    static func fromWireValue(_ value: JSONValue?) throws -> DeviceModelRequirements {
        let fields = try DeviceModelWire.exactObject(DeviceModelWire.required(value), keys: ["capabilities"])
        let requirements = DeviceModelRequirements(capabilities: try DeviceModelWire.capabilities(fields["capabilities"]))
        try requirements.validate()
        return requirements
    }
}

extension DeviceModelUsage {
    static func fromWireValue(_ value: JSONValue?) throws -> DeviceModelUsage {
        let fields = try DeviceModelWire.exactObject(DeviceModelWire.required(value), keys: ["tokens", "cost_micros"])
        let usage = DeviceModelUsage(
            tokens: try DeviceModelWire.optionalUInt64(fields["tokens"]),
            costMicros: try DeviceModelWire.optionalUInt64(fields["cost_micros"])
        )
        try usage.validate()
        return usage
    }
}

extension DeviceModelResponse {
    static func fromWireValue(_ value: JSONValue) throws -> DeviceModelResponse {
        let fields = try DeviceModelWire.exactObject(value, keys: ["operation_id", "binding_id", "output", "usage"])
        let response = DeviceModelResponse(
            operationID: try DeviceModelWire.uuid(fields["operation_id"]),
            bindingID: try DeviceModelWire.string(fields["binding_id"]),
            output: try DeviceModelOutput.fromWireValue(DeviceModelWire.required(fields["output"])),
            usage: try DeviceModelUsage.fromWireValue(fields["usage"])
        )
        guard DeviceModelWire.isHex64(response.bindingID) else { throw DeviceModelContractError.invalidValue }
        try response.usage.validate()
        guard jsonEncodedByteCount(response.wireValue, maximum: DeviceModelBounds.maxResponseEnvelopeBytes, tolerateInvalidNumbers: true)
                <= DeviceModelBounds.maxResponseEnvelopeBytes else {
            throw DeviceModelContractError.byteLimitExceeded
        }
        return response
    }
}

private func operationWireValue(_ operation: String, operationID: UUID) -> JSONValue {
    .object([
        "schema_version": .number(1),
        "operation": .string(operation),
        "operation_id": DeviceModelWire.uuidValue(operationID)
    ])
}

private func statusOperationWireValue(_ status: String, operationID: UUID) -> JSONValue {
    .object([
        "schema_version": .number(1),
        "status": .string(status),
        "operation_id": DeviceModelWire.uuidValue(operationID)
    ])
}
