import CryptoKit
import Foundation
import FoundationModels
import FloeModelExecution

/// The immutable FoundationModels implementation of the neutral device-model contract.
public struct FoundationModelsDeviceModel: DeviceModel, Sendable {
    private static let limits = DeviceModelLimits(
        maxInputBytes: 16_384,
        maxInstructionsBytes: 9_216,
        maxOutputBytes: 32_768,
        maxResponseTokens: 1_024,
        maxDeadlineMilliseconds: 30_000
    )

    // These are sorted by their serialized capability names.
    private static let capabilities: [DeviceModelCapability] = [
        .structuredOutput,
        .text,
        .toolProposals
    ]

    @available(iOS 26.0, macOS 26.0, visionOS 26.0, *)
    private static let configuredModel = SystemLanguageModel.default

    private let bindingID: String

    public init() {
        let nonce = UUID().uuidString.lowercased()
        let version = ProcessInfo.processInfo.operatingSystemVersion
        let operatingSystem = "\(version.majorVersion).\(version.minorVersion).\(version.patchVersion)"
        let contract = [
            "implementation=FoundationModelsDeviceModel/v1",
            "model=SystemLanguageModel.default",
            "capabilities=structured_output,text,tool_proposals",
            "limits=input:16384,instructions:9216,output:32768,response_tokens:1024,deadline_ms:30000",
            "contract=device-model/v1",
            "os=\(operatingSystem)",
            "instance=\(nonce)"
        ].joined(separator: "\n")
        let digest = SHA256.hash(data: Data(contract.utf8))
        self.bindingID = digest.map { String(format: "%02x", $0) }.joined()
    }

    public func prepare(
        _ requirements: DeviceModelRequirements
    ) throws -> DeviceModelObservation {
        do {
            try requirements.validate()
        } catch let failure as DeviceModelFailure {
            throw failure
        } catch {
            throw DeviceModelFailure.invalidInput
        }

        guard requirements.capabilities.allSatisfy({
            Self.capabilities.contains($0)
        }) else {
            return .unavailable(.unsupported)
        }

        guard #available(iOS 26.0, macOS 26.0, visionOS 26.0, *) else {
            return .unavailable(.unsupported)
        }

        return try currentObservation()
    }

    public func generate(
        _ request: DeviceModelRequest
    ) async throws -> DeviceModelResponse {
        if Task.isCancelled {
            throw DeviceModelFailure.cancelled
        }
        guard (1...Self.limits.maxDeadlineMilliseconds).contains(request.deadlineMilliseconds) else {
            throw DeviceModelFailure.invalidInput
        }

        let deadline = ProcessInfo.processInfo.systemUptime
            + Double(request.deadlineMilliseconds) / 1_000.0

        do {
            try Self.checkFences(deadline: deadline)
            try request.validate()
            try Self.checkFences(deadline: deadline)

            let requirements = try Self.requirements(for: request)
            let observation = try prepare(requirements)
            try Self.checkFences(deadline: deadline)

            let profile: DeviceModelProfile
            switch observation {
            case .available(let availableProfile):
                profile = availableProfile
            case .unavailable(let reason):
                throw Self.failure(for: reason)
            }

            guard profile.bindingID == request.bindingID else {
                throw DeviceModelFailure.conflict
            }
            do {
                try profile.validate(request: request)
            } catch let failure as DeviceModelFailure {
                throw failure
            } catch {
                throw DeviceModelFailure.invalidInput
            }
            try Self.checkFences(deadline: deadline)

            guard #available(iOS 26.0, macOS 26.0, visionOS 26.0, *) else {
                throw DeviceModelFailure.unsupported
            }

            return try await generateAvailable(
                request,
                deadline: deadline
            )
        } catch is CancellationError {
            throw DeviceModelFailure.cancelled
        } catch let failure as DeviceModelFailure {
            throw failure
        } catch {
            throw DeviceModelFailure.unavailable
        }
    }

    private func currentObservation() throws -> DeviceModelObservation {
        guard #available(iOS 26.0, macOS 26.0, visionOS 26.0, *) else {
            return .unavailable(.unsupported)
        }

        switch Self.configuredModel.availability {
        case .available:
            return .available(
                DeviceModelProfile(
                    bindingID: bindingID,
                    capabilities: Self.capabilities,
                    limits: Self.limits
                )
            )
        case .unavailable(let reason):
            switch reason {
            case .deviceNotEligible:
                return .unavailable(.unsupported)
            case .appleIntelligenceNotEnabled:
                return .unavailable(.disabled)
            case .modelNotReady:
                return .unavailable(.notReady)
            @unknown default:
                throw DeviceModelFailure.unavailable
            }
        @unknown default:
            throw DeviceModelFailure.unavailable
        }
    }

    @available(iOS 26.0, macOS 26.0, visionOS 26.0, *)
    private func generateAvailable(
        _ request: DeviceModelRequest,
        deadline: TimeInterval
    ) async throws -> DeviceModelResponse {
        try Self.checkFences(deadline: deadline)

        let inputData: Data
        do {
            inputData = try request.input.encoded()
        } catch {
            throw DeviceModelFailure.invalidInput
        }
        guard inputData.count <= Self.limits.maxInputBytes else {
            throw DeviceModelFailure.invalidInput
        }

        let prompt = """
        Input JSON data:
        \(String(decoding: inputData, as: UTF8.self))
        """

        let generationSchema: GenerationSchema?
        switch request.outputFormat {
        case .text:
            generationSchema = nil
        case .json(let modelSchema):
            do {
                generationSchema = try Self.generationSchema(for: modelSchema)
            } catch let failure as DeviceModelFailure {
                throw failure
            } catch {
                throw DeviceModelFailure.unsupported
            }
        }

        let toolAdapters: [any Tool]
        do {
            toolAdapters = try request.tools.map { try InertProposalTool(deviceTool: $0) }
        } catch let failure as DeviceModelFailure {
            throw failure
        } catch {
            throw DeviceModelFailure.unsupported
        }

        let session = LanguageModelSession(
            model: Self.configuredModel,
            tools: toolAdapters,
            instructions: request.instructions
        )
        let options = GenerationOptions(
            sampling: .greedy,
            maximumResponseTokens: request.maxResponseTokens
        )

        let generated: GeneratedValue
        do {
            generated = try await Self.firstValue(
                deadline: deadline
            ) {
                try Task.checkCancellation()

                switch request.outputFormat {
                case .text:
                    let response = try await session.respond(
                        to: prompt,
                        options: options
                    )
                    return .text(response.content)

                case .json:
                    guard let generationSchema else {
                        throw DeviceModelFailure.invalidInput
                    }
                    let response = try await session.respond(
                        to: prompt,
                        schema: generationSchema,
                        includeSchemaInPrompt: true,
                        options: options
                    )
                    return .structured(response.content)
                }
            }
        } catch let toolError as LanguageModelSession.ToolCallError {
            guard let proposal = toolError.underlyingError as? ToolProposalSignal else {
                return try normalizedResponse(request: request, output: .failure(.invalidOutput), deadline: deadline)
            }
            return try normalizedResponse(
                request: request,
                output: Self.validatedProposal(
                    proposal,
                    tools: request.tools,
                    maximumBytes: request.maxOutputBytes
                ),
                deadline: deadline
            )
        } catch let error as LanguageModelSession.GenerationError {
            let failure: DeviceModelFailure
            switch error {
            case .guardrailViolation, .refusal:
                failure = .policyDenied
            case .decodingFailure, .unsupportedGuide:
                failure = .invalidOutput
            case .rateLimited:
                failure = .quotaExceeded
            case .assetsUnavailable:
                failure = .notReady
            case .unsupportedLanguageOrLocale:
                failure = .unsupported
            case .concurrentRequests:
                failure = .busy
            case .exceededContextWindowSize:
                failure = .budgetExceeded
            default:
                failure = .unavailable
            }
            return try normalizedResponse(request: request, output: .failure(failure), deadline: deadline)
        } catch {
            throw error
        }

        let output: DeviceModelOutput
        switch (request.outputFormat, generated) {
        case (.text, .text(let text)):
            output = .text(text)

        case (.json(let schema), .structured(let content)):
            output = Self.validatedJSON(content, schema: schema, maximumBytes: request.maxOutputBytes)

        default:
            output = .failure(.invalidOutput)
        }

        return try normalizedResponse(
            request: request,
            output: output,
            deadline: deadline
        )
    }

    private func normalizedResponse(
        request: DeviceModelRequest,
        output: DeviceModelOutput,
        deadline: TimeInterval
    ) throws -> DeviceModelResponse {
        let fencedOutput: DeviceModelOutput
        if Task.isCancelled {
            fencedOutput = .failure(.cancelled)
        } else if ProcessInfo.processInfo.systemUptime >= deadline {
            fencedOutput = .failure(.deadlineExceeded)
        } else {
            fencedOutput = output
        }
        let response = DeviceModelResponse(
            operationID: request.operationID,
            bindingID: request.bindingID,
            output: fencedOutput,
            usage: DeviceModelUsage(tokens: nil, costMicros: 0)
        )
        let boundedResponse: DeviceModelResponse
        do {
            try response.validateEnvelope(for: request)
            boundedResponse = response
        } catch {
            // The identity and usage above come from trusted request state. An oversized
            // generated payload is therefore reduced to the bounded terminal failure.
            boundedResponse = DeviceModelResponse(
                operationID: request.operationID,
                bindingID: request.bindingID,
                output: .failure(.invalidOutput),
                usage: response.usage
            )
        }

        let normalized: DeviceModelResponse
        do {
            normalized = try boundedResponse.normalized(for: request)
        } catch {
            throw DeviceModelFailure.invalidInput
        }

        if Task.isCancelled || ProcessInfo.processInfo.systemUptime >= deadline {
            return DeviceModelResponse(operationID: request.operationID, bindingID: request.bindingID,
                output: .failure(Task.isCancelled ? .cancelled : .deadlineExceeded), usage: normalized.usage)
        }
        return normalized
    }

    @available(iOS 26.0, macOS 26.0, visionOS 26.0, *)
    private static func validatedJSON(
        _ content: GeneratedContent,
        schema: ModelSchema,
        maximumBytes: Int
    ) -> DeviceModelOutput {
        guard content.isComplete else {
            return .failure(.invalidOutput)
        }

        do {
            let value = try JSONValue.decode(
                Data(content.jsonString.utf8),
                maximumBytes: maximumBytes
            )
            guard case .object(_) = value else {
                return .failure(.invalidOutput)
            }
            try schema.validate(value)
            return .json(value)
        } catch {
            return .failure(.invalidOutput)
        }
    }

    private static func validatedProposal(
        _ proposal: ToolProposalSignal,
        tools: [DeviceTool],
        maximumBytes: Int
    ) -> DeviceModelOutput {
        guard let tool = tools.first(where: { $0.name == proposal.name }),
              proposal.json.utf8.count <= maximumBytes else {
            return .failure(.invalidOutput)
        }

        let input: JSONValue
        do {
            input = try JSONValue.decode(
                Data(proposal.json.utf8),
                maximumBytes: maximumBytes
            )
        } catch {
            return .failure(.invalidOutput)
        }
        guard case .object(_) = input else {
            return .failure(.invalidOutput)
        }

        do {
            try tool.inputSchema.validate(input)
            return .toolProposal(name: proposal.name, input: input)
        } catch {
            return .failure(.invalidOutput)
        }
    }

    private static func requirements(
        for request: DeviceModelRequest
    ) throws -> DeviceModelRequirements {
        var required: [DeviceModelCapability] = []
        switch request.outputFormat {
        case .text:
            required.append(.text)
        case .json:
            required.append(.structuredOutput)
        }
        if !request.tools.isEmpty {
            required.append(.toolProposals)
        }

        let sorted = required.sorted {
            capabilityWireName($0) < capabilityWireName($1)
        }
        let requirements = DeviceModelRequirements(capabilities: sorted)
        do {
            try requirements.validate()
        } catch let failure as DeviceModelFailure {
            throw failure
        } catch {
            throw DeviceModelFailure.invalidInput
        }
        return requirements
    }

    private static func capabilityWireName(
        _ capability: DeviceModelCapability
    ) -> String {
        switch capability {
        case .text:
            return "text"
        case .structuredOutput:
            return "structured_output"
        case .toolProposals:
            return "tool_proposals"
        }
    }

    private static func failure(
        for unavailable: DeviceModelUnavailable
    ) -> DeviceModelFailure {
        switch unavailable {
        case .unsupported:
            return .unsupported
        case .disabled:
            return .disabled
        case .notReady:
            return .notReady
        }
    }

    private static func checkFences(deadline: TimeInterval) throws {
        if Task.isCancelled {
            throw DeviceModelFailure.cancelled
        }
        guard ProcessInfo.processInfo.systemUptime < deadline else {
            throw DeviceModelFailure.deadlineExceeded
        }
    }

    @available(iOS 26.0, macOS 26.0, visionOS 26.0, *)
    private static func firstValue(
        deadline: TimeInterval,
        operation: @escaping @Sendable () async throws -> GeneratedValue
    ) async throws -> GeneratedValue {
        try checkFences(deadline: deadline)

        return try await withThrowingTaskGroup(of: GeneratedValue.self) { group in
            group.addTask {
                try await operation()
            }
            group.addTask {
                let remaining = deadline - ProcessInfo.processInfo.systemUptime
                guard remaining > 0 else {
                    throw DeviceModelFailure.deadlineExceeded
                }
                let nanoseconds = UInt64(remaining * 1_000_000_000)
                try await Task.sleep(nanoseconds: nanoseconds)
                throw DeviceModelFailure.deadlineExceeded
            }
            defer { group.cancelAll() }

            guard let value = try await group.next() else {
                throw DeviceModelFailure.unavailable
            }
            return value
        }
    }

    @available(iOS 26.0, macOS 26.0, visionOS 26.0, *)
    private static func generationSchema(
        for schema: ModelSchema
    ) throws -> GenerationSchema {
        guard case .object(_) = schema.json else {
            throw DeviceModelFailure.unsupported
        }
        var nextName = 0
        let root = try dynamicSchema(
            from: schema.json,
            prefix: "FloeNode",
            nextName: &nextName
        )

        do {
            return try GenerationSchema(root: root, dependencies: [])
        } catch {
            throw DeviceModelFailure.unsupported
        }
    }

    @available(iOS 26.0, macOS 26.0, visionOS 26.0, *)
    fileprivate static func dynamicSchema(
        from value: JSONValue,
        prefix: String,
        nextName: inout Int
    ) throws -> DynamicGenerationSchema {
        guard case .object(let fields) = value else {
            throw DeviceModelFailure.unsupported
        }

        nextName += 1
        let name = "\(prefix)\(nextName)"
        let description = try optionalString(fields["description"])
        let declaredType = try optionalString(fields["type"])
        let constant = fields["const"]

        if declaredType == nil, constant == nil {
            throw DeviceModelFailure.unsupported
        }

        let type = declaredType ?? inferredType(for: constant)
        guard let type else {
            throw DeviceModelFailure.unsupported
        }
        if fields["enum"] != nil, type != "string" {
            throw DeviceModelFailure.unsupported
        }
        if let constant {
            let matchesType: Bool
            switch (type, constant) {
            case ("string", .string(_)):
                matchesType = true
            case ("integer", .number(let value)):
                matchesType = value.isFinite && value.rounded(.towardZero) == value
            case ("number", .number(let value)):
                matchesType = value.isFinite
            case ("boolean", .bool(_)):
                matchesType = true
            default:
                matchesType = false
            }
            guard matchesType else {
                throw DeviceModelFailure.unsupported
            }
        }

        switch type {
        case "object":
            guard case .object(let propertySchemas)? = fields["properties"] else {
                throw DeviceModelFailure.unsupported
            }
            let required = try stringSet(fields["required"])
            var properties: [DynamicGenerationSchema.Property] = []
            for (propertyName, propertyValue) in propertySchemas.sorted(by: { $0.key.utf8.lexicographicallyPrecedes($1.key.utf8) }) {
                guard case .object(let propertyFields) = propertyValue else {
                    throw DeviceModelFailure.unsupported
                }
                let propertyDescription = try optionalString(propertyFields["description"])
                let propertySchema = try dynamicSchema(
                    from: propertyValue,
                    prefix: "FloeNode",
                    nextName: &nextName
                )
                properties.append(
                    DynamicGenerationSchema.Property(
                        name: propertyName,
                        description: propertyDescription,
                        schema: propertySchema,
                        isOptional: !required.contains(where: { $0.utf8.elementsEqual(propertyName.utf8) })
                    )
                )
            }
            return DynamicGenerationSchema(
                name: name,
                description: description,
                properties: properties
            )

        case "array":
            guard let items = fields["items"] else {
                throw DeviceModelFailure.unsupported
            }
            let itemSchema = try dynamicSchema(
                from: items,
                prefix: "FloeNode",
                nextName: &nextName
            )
            return DynamicGenerationSchema(
                arrayOf: itemSchema,
                minimumElements: try integer(fields["minItems"]),
                maximumElements: try integer(fields["maxItems"])
            )

        case "string":
            // Validate length keywords even though no SDK guide has exact scalar-count semantics.
            _ = try integer(fields["minLength"])
            _ = try integer(fields["maxLength"])
            if let enumValue = fields["enum"] {
                guard let choices = stringChoices(enumValue) else {
                    throw DeviceModelFailure.unsupported
                }
                return DynamicGenerationSchema(
                    name: name,
                    description: description,
                    anyOf: choices
                )
            }
            if case .string(let value)? = constant {
                return DynamicGenerationSchema(
                    name: name,
                    description: description,
                    anyOf: [value]
                )
            }
            // The SDK has regex guides, but no guide with the schema's exact Unicode-scalar
            // counting semantics. The shared validator enforces minLength and maxLength.
            return DynamicGenerationSchema(type: String.self)

        case "integer":
            var guides: [GenerationGuide<Int>] = []
            if let constant {
                guard case .number(let value) = constant,
                      let exact = Int(exactly: value) else {
                    throw DeviceModelFailure.unsupported
                }
                guides.append(.minimum(exact))
                guides.append(.maximum(exact))
            } else {
                if let minimum = try integer(fields["minimum"]) {
                    guides.append(.minimum(minimum))
                }
                if let maximum = try integer(fields["maximum"]) {
                    guides.append(.maximum(maximum))
                }
            }
            return DynamicGenerationSchema(type: Int.self, guides: guides)

        case "number":
            var guides: [GenerationGuide<Double>] = []
            if let constant {
                guard case .number(let value) = constant else {
                    throw DeviceModelFailure.unsupported
                }
                guides.append(.minimum(value))
                guides.append(.maximum(value))
            } else {
                if let minimum = try number(fields["minimum"]) {
                    guides.append(.minimum(minimum))
                }
                if let maximum = try number(fields["maximum"]) {
                    guides.append(.maximum(maximum))
                }
            }
            return DynamicGenerationSchema(type: Double.self, guides: guides)

        case "boolean":
            // The SDK has no Boolean const guide; the shared validator enforces it.
            return DynamicGenerationSchema(type: Bool.self)

        default:
            throw DeviceModelFailure.unsupported
        }
    }

    private static func inferredType(for constant: JSONValue?) -> String? {
        guard let constant else { return nil }
        switch constant {
        case .string(_):
            return "string"
        case .number(let value):
            return value.rounded(.towardZero) == value ? "integer" : "number"
        case .bool(_):
            return "boolean"
        case .object(_), .array(_), .null:
            return nil
        }
    }

    private static func optionalString(
        _ value: JSONValue?
    ) throws -> String? {
        guard let value else { return nil }
        guard case .string(let text) = value else {
            throw DeviceModelFailure.unsupported
        }
        return text
    }

    private static func stringSet(
        _ value: JSONValue?
    ) throws -> [String] {
        guard let value else { return [] }
        guard case .array(let values) = value else {
            throw DeviceModelFailure.unsupported
        }
        let strings: [String] = values.compactMap { item -> String? in
            guard case .string(let text) = item else { return nil }
            return text
        }
        guard strings.count == values.count else {
            throw DeviceModelFailure.unsupported
        }
        return strings
    }

    private static func stringChoices(
        _ value: JSONValue?
    ) -> [String]? {
        guard case .array(let choices)? = value else { return nil }
        let strings: [String] = choices.compactMap { item -> String? in
            guard case .string(let value) = item else { return nil }
            return value
        }
        guard strings.count == choices.count, !strings.isEmpty else { return nil }
        return strings
    }

    private static func integer(
        _ value: JSONValue?
    ) throws -> Int? {
        guard let value else { return nil }
        guard case .number(let number) = value,
              number.isFinite,
              number.rounded(.towardZero) == number,
              let exact = Int(exactly: number) else {
            throw DeviceModelFailure.unsupported
        }
        return exact
    }

    private static func number(
        _ value: JSONValue?
    ) throws -> Double? {
        guard let value else { return nil }
        guard case .number(let number) = value, number.isFinite else {
            throw DeviceModelFailure.unsupported
        }
        return number
    }
}

@available(iOS 26.0, macOS 26.0, visionOS 26.0, *)
private enum GeneratedValue: Sendable {
    case text(String)
    case structured(GeneratedContent)
}

private struct ToolProposalSignal: Error, Sendable {
    let name: String
    let json: String
}

@available(iOS 26.0, macOS 26.0, visionOS 26.0, *)
private struct InertProposalTool: Tool {
    typealias Arguments = GeneratedContent
    typealias Output = String

    let deviceTool: DeviceTool
    let parameters: GenerationSchema

    var name: String { deviceTool.name }
    var description: String { deviceTool.description }

    init(deviceTool: DeviceTool) throws {
        self.deviceTool = deviceTool
        var nextName = 0
        let dynamicSchema = try FoundationModelsDeviceModel.dynamicSchema(
            from: deviceTool.inputSchema.json,
            prefix: "FloeToolNode",
            nextName: &nextName
        )
        do {
            self.parameters = try GenerationSchema(
                root: dynamicSchema,
                dependencies: []
            )
        } catch {
            throw DeviceModelFailure.unsupported
        }
    }

    func call(arguments: GeneratedContent) async throws -> String {
        // Preserve the exact generated tool object until the owner validates it.
        throw ToolProposalSignal(
            name: deviceTool.name,
            json: arguments.jsonString
        )
    }
}
