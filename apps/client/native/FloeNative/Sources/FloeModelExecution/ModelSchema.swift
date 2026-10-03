import Foundation

public struct ModelSchema: Sendable, Equatable {
    public let json: JSONValue

    public init(json: JSONValue) throws {
        try ModelSchemaValidator.validateSchema(json)
        self.json = json
    }

    public func validate(_ value: JSONValue) throws {
        try validateJSONValue(value)
        try ModelSchemaValidator.validateValue(schema: json, value: value, checkConstant: true)
    }

}

private enum ModelSchemaValidator {
    private static let maximumSchemaBytes = 16 * 1024
    private static let maximumNodes = 256
    private static let maximumDepth = 8
    private static let maximumObjectProperties = 64
    private static let maximumEnumChoices = 64
    private static let maximumTextBound = 32_768

    private enum SchemaType: String, Equatable {
        case object
        case array
        case string
        case integer
        case number
        case boolean
    }

    static func validateSchema(_ schema: JSONValue) throws {
        try validateJSONValue(schema)
        guard jsonEncodedByteCount(schema, maximum: maximumSchemaBytes) <= maximumSchemaBytes,
              case .object = schema else {
            throw DeviceModelContractError.invalidSchema
        }
        var nodeCount = 0
        try validateSchemaNode(schema, depth: 0, nodeCount: &nodeCount, isRoot: true)
    }

    private static func validateSchemaNode(
        _ schema: JSONValue,
        depth: Int,
        nodeCount: inout Int,
        isRoot: Bool = false
    ) throws {
        guard depth <= maximumDepth, nodeCount < maximumNodes, case .object(let fields) = schema else {
            throw DeviceModelContractError.invalidSchema
        }
        nodeCount += 1

        let allowedKeys: Set<JSONUTF8Key> = [
            "type", "properties", "required", "additionalProperties", "items",
            "minItems", "maxItems", "enum", "minLength", "maxLength",
            "minimum", "maximum", "const", "description"
        ]
        guard fields.keySet.isSubset(of: allowedKeys) else { throw DeviceModelContractError.invalidSchema }

        if let description = fields["description"] {
            guard case .string(let text) = description, text.utf8.count <= 2_048 else {
                throw DeviceModelContractError.invalidSchema
            }
        }

        let declaredType: SchemaType?
        if let rawType = fields["type"] {
            guard case .string(let name) = rawType, let schemaType = SchemaType(rawValue: name) else {
                throw DeviceModelContractError.invalidSchema
            }
            declaredType = schemaType
        } else {
            declaredType = nil
        }

        let constant = fields["const"]
        if let constant {
            switch constant {
            case .string, .number, .bool:
                break
            case .object, .array, .null:
                throw DeviceModelContractError.invalidSchema
            }
        }

        if isRoot {
            guard declaredType == .object else { throw DeviceModelContractError.invalidSchema }
        }
        if declaredType == nil {
            guard constant != nil,
                  fields.keySet.isSubset(of: ["const", "description"]) else {
                throw DeviceModelContractError.invalidSchema
            }
        }

        let objectKeys: Set<JSONUTF8Key> = ["properties", "required", "additionalProperties"]
        let arrayKeys: Set<JSONUTF8Key> = ["items", "minItems", "maxItems"]
        let stringKeys: Set<JSONUTF8Key> = ["enum", "minLength", "maxLength"]
        let numericKeys: Set<JSONUTF8Key> = ["minimum", "maximum"]
        switch declaredType {
        case .object:
            guard fields.keySet.intersection(arrayKeys.union(stringKeys).union(numericKeys)).isEmpty,
                  fields["items"] == nil else { throw DeviceModelContractError.invalidSchema }
        case .array:
            guard fields.keySet.intersection(objectKeys.union(stringKeys).union(numericKeys)).isEmpty,
                  let items = fields["items"] else { throw DeviceModelContractError.invalidSchema }
            try validateSchemaNode(items, depth: depth + 1, nodeCount: &nodeCount)
            try validateIntegerRange(fields, minimumKey: "minItems", maximumKey: "maxItems")
        case .string:
            guard fields.keySet.intersection(objectKeys.union(arrayKeys).union(numericKeys)).isEmpty,
                  fields["items"] == nil else { throw DeviceModelContractError.invalidSchema }
            try validateIntegerRange(fields, minimumKey: "minLength", maximumKey: "maxLength")
            try validateEnum(fields["enum"])
        case .integer, .number:
            guard fields.keySet.intersection(objectKeys.union(arrayKeys).union(stringKeys)).isEmpty,
                  fields["items"] == nil, fields["enum"] == nil else {
                throw DeviceModelContractError.invalidSchema
            }
            let lower = try validateNumericBound(fields["minimum"], integerOnly: declaredType == .integer)
            let upper = try validateNumericBound(fields["maximum"], integerOnly: declaredType == .integer)
            if let lower, let upper, lower > upper { throw DeviceModelContractError.invalidSchema }
        case .boolean:
            let typeSpecificKeys = objectKeys.union(arrayKeys).union(stringKeys).union(numericKeys)
            guard fields.keySet.intersection(typeSpecificKeys).isEmpty else {
                throw DeviceModelContractError.invalidSchema
            }
        case nil:
            break
        }

        if declaredType == .object {
            guard case .bool(false)? = fields["additionalProperties"] else {
                throw DeviceModelContractError.invalidSchema
            }
            let properties: JSONObject
            if let rawProperties = fields["properties"] {
                guard case .object(let values) = rawProperties,
                      values.count <= maximumObjectProperties else {
                    throw DeviceModelContractError.invalidSchema
                }
                properties = values
            } else {
                properties = [:]
            }
            for name in properties.keys {
                guard validSchemaName(name) else { throw DeviceModelContractError.invalidSchema }
            }

            var requiredNames = Set<JSONUTF8Key>()
            if let rawRequired = fields["required"] {
                guard case .array(let names) = rawRequired else { throw DeviceModelContractError.invalidSchema }
                for nameValue in names {
                    guard case .string(let name) = nameValue,
                          properties[name] != nil,
                          requiredNames.insert(JSONUTF8Key(name)).inserted else {
                        throw DeviceModelContractError.invalidSchema
                    }
                }
            }
            for property in properties.values {
                try validateSchemaNode(property, depth: depth + 1, nodeCount: &nodeCount)
            }
        }

        if let constant {
            do {
                try validateValue(schema: schema, value: constant, checkConstant: false)
            } catch {
                throw DeviceModelContractError.invalidSchema
            }
        }
    }

    private static func validateIntegerRange(
        _ fields: JSONObject,
        minimumKey: String,
        maximumKey: String
    ) throws {
        let lower = try nonnegativeBound(fields[minimumKey])
        let upper = try nonnegativeBound(fields[maximumKey])
        if let lower, let upper, lower > upper { throw DeviceModelContractError.invalidSchema }
    }

    private static func nonnegativeBound(_ value: JSONValue?) throws -> Int? {
        guard let value else { return nil }
        guard case .number(let number) = value,
              isSafeJSONNumber(number),
              number.rounded(.towardZero) == number,
              number >= 0,
              number <= Double(maximumTextBound) else {
            throw DeviceModelContractError.invalidSchema
        }
        return Int(number)
    }

    private static func validateNumericBound(_ value: JSONValue?, integerOnly: Bool) throws -> Double? {
        guard let value else { return nil }
        guard case .number(let number) = value, isSafeJSONNumber(number) else {
            throw DeviceModelContractError.invalidSchema
        }
        if integerOnly, number.rounded(.towardZero) != number {
            throw DeviceModelContractError.invalidSchema
        }
        return number
    }

    private static func validateEnum(_ value: JSONValue?) throws {
        guard let value else { return }
        guard case .array(let choices) = value,
              !choices.isEmpty,
              choices.count <= maximumEnumChoices else {
            throw DeviceModelContractError.invalidSchema
        }
        var seen = Set<JSONUTF8Key>()
        for choice in choices {
            guard case .string(let text) = choice, seen.insert(JSONUTF8Key(text)).inserted else {
                throw DeviceModelContractError.invalidSchema
            }
        }
    }

    private static func validSchemaName(_ name: String) -> Bool {
        guard !name.isEmpty, name.utf8.count <= 128 else { return false }
        return !name.unicodeScalars.contains { scalar in
            scalar.value <= 0x1F || (0x7F...0x9F).contains(scalar.value)
        }
    }

    static func validateValue(schema: JSONValue, value: JSONValue, checkConstant: Bool) throws {
        guard case .object(let fields) = schema else { throw DeviceModelContractError.invalidSchema }
        if case .null = value { throw DeviceModelContractError.invalidValue }

        if checkConstant, let constant = fields["const"], constant != value {
            throw DeviceModelContractError.invalidValue
        }

        let declaredType: SchemaType?
        if let rawType = fields["type"], case .string(let name) = rawType {
            declaredType = SchemaType(rawValue: name)
        } else {
            declaredType = nil
        }

        switch declaredType {
        case .object:
            guard case .object(let object) = value else { throw DeviceModelContractError.invalidValue }
            var properties: JSONObject = [:]
            if let raw = fields["properties"], case .object(let schemaProperties) = raw {
                properties = schemaProperties
            }
            if let rawRequired = fields["required"], case .array(let required) = rawRequired {
                for nameValue in required {
                    guard case .string(let name) = nameValue, object[name] != nil else {
                        throw DeviceModelContractError.invalidValue
                    }
                }
            }
            for (name, child) in object {
                guard let childSchema = properties[name] else { throw DeviceModelContractError.invalidValue }
                try validateValue(schema: childSchema, value: child, checkConstant: true)
            }
        case .array:
            guard case .array(let values) = value,
                  let rawItems = fields["items"] else {
                throw DeviceModelContractError.invalidValue
            }
            if let minimum = try nonnegativeBound(fields["minItems"]), values.count < minimum {
                throw DeviceModelContractError.invalidValue
            }
            if let maximum = try nonnegativeBound(fields["maxItems"]), values.count > maximum {
                throw DeviceModelContractError.invalidValue
            }
            for child in values { try validateValue(schema: rawItems, value: child, checkConstant: true) }
        case .string:
            guard case .string(let text) = value else { throw DeviceModelContractError.invalidValue }
            let scalarCount = text.unicodeScalars.count
            if let minimum = try nonnegativeBound(fields["minLength"]), scalarCount < minimum {
                throw DeviceModelContractError.invalidValue
            }
            if let maximum = try nonnegativeBound(fields["maxLength"]), scalarCount > maximum {
                throw DeviceModelContractError.invalidValue
            }
            if let rawEnum = fields["enum"], case .array(let choices) = rawEnum,
               !choices.contains(.string(text)) {
                throw DeviceModelContractError.invalidValue
            }
        case .integer:
            guard case .number(let number) = value,
                  isSafeJSONNumber(number),
                  number.rounded(.towardZero) == number else {
                throw DeviceModelContractError.invalidValue
            }
            try validateNumericValue(number, fields: fields)
        case .number:
            guard case .number(let number) = value, isSafeJSONNumber(number) else {
                throw DeviceModelContractError.invalidValue
            }
            try validateNumericValue(number, fields: fields)
        case .boolean:
            guard case .bool = value else { throw DeviceModelContractError.invalidValue }
        case nil:
            guard let constant = fields["const"], constant == value else {
                throw DeviceModelContractError.invalidValue
            }
        }
    }

    private static func validateNumericValue(_ value: Double, fields: JSONObject) throws {
        if let minimum = try validateNumericBound(fields["minimum"], integerOnly: false), value < minimum {
            throw DeviceModelContractError.invalidValue
        }
        if let maximum = try validateNumericBound(fields["maximum"], integerOnly: false), value > maximum {
            throw DeviceModelContractError.invalidValue
        }
    }
}
