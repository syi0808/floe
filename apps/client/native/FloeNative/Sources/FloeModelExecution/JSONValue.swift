import Foundation

// Swift String equality normalizes canonically equivalent Unicode. JSON compares
// decoded scalar sequences exactly, so keys and string values use their UTF-8 bytes.
struct JSONUTF8Key: Sendable, Hashable, ExpressibleByStringLiteral {
    let string: String
    private let bytes: [UInt8]

    init(_ string: String) {
        self.string = string
        self.bytes = Array(string.utf8)
    }

    init(stringLiteral value: String) { self.init(value) }

    static func == (lhs: Self, rhs: Self) -> Bool { lhs.bytes == rhs.bytes }
    func hash(into hasher: inout Hasher) { hasher.combine(bytes) }
}

func utf8Equal(_ lhs: String, _ rhs: String) -> Bool {
    lhs.utf8.elementsEqual(rhs.utf8)
}

func utf8Precedes(_ lhs: String, _ rhs: String) -> Bool {
    lhs.utf8.lexicographicallyPrecedes(rhs.utf8)
}

/// A JSON object whose property identity is exact decoded UTF-8, without Unicode
/// normalization. Never pass untrusted properties through a String-keyed Dictionary.
public struct JSONObject: Sendable, Equatable, ExpressibleByDictionaryLiteral, Sequence {
    public typealias Element = (key: String, value: JSONValue)
    private var storage: [JSONUTF8Key: JSONValue]

    public init() { storage = [:] }

    public init(dictionaryLiteral elements: (String, JSONValue)...) {
        storage = [:]
        for (name, value) in elements {
            precondition(storage.updateValue(value, forKey: JSONUTF8Key(name)) == nil,
                         "Duplicate JSON object literal key")
        }
    }

    public subscript(_ name: String) -> JSONValue? {
        get { storage[JSONUTF8Key(name)] }
        set { storage[JSONUTF8Key(name)] = newValue }
    }

    public var keys: [String] { storage.keys.map(\.string) }
    public var values: [JSONValue] { Array(storage.values) }
    public var count: Int { storage.count }
    public var isEmpty: Bool { storage.isEmpty }
    var keySet: Set<JSONUTF8Key> { Set(storage.keys) }

    public func makeIterator() -> AnyIterator<Element> {
        var iterator = storage.makeIterator()
        return AnyIterator {
            guard let pair = iterator.next() else { return nil }
            return (key: pair.key.string, value: pair.value)
        }
    }
}

public indirect enum JSONValue: Sendable, Equatable {
    case object(JSONObject)
    case array([JSONValue])
    case string(String)
    case number(Double)
    case bool(Bool)
    case null

    public static func == (lhs: JSONValue, rhs: JSONValue) -> Bool {
        switch (lhs, rhs) {
        case (.object(let lhs), .object(let rhs)): return lhs == rhs
        case (.array(let lhs), .array(let rhs)): return lhs == rhs
        case (.string(let lhs), .string(let rhs)): return utf8Equal(lhs, rhs)
        case (.number(let lhs), .number(let rhs)): return lhs == rhs
        case (.bool(let lhs), .bool(let rhs)): return lhs == rhs
        case (.null, .null): return true
        default: return false
        }
    }

    /// Strictly parses untrusted JSON before any Codable decoder can collapse duplicate object keys.
    public static func decode(_ data: Data, maximumBytes: Int) throws -> JSONValue {
        guard maximumBytes >= 0, data.count <= maximumBytes else {
            throw DeviceModelContractError.byteLimitExceeded
        }
        var parser = StrictJSONParser(bytes: Array(data), maximumDepth: 32)
        return try parser.parse()
    }

    public func encoded() throws -> Data {
        var output: [UInt8] = []
        try appendStrictJSON(self, depth: 0, into: &output)
        return Data(output)
    }

}

enum DeviceModelContractError: Error, Sendable {
    case invalidJSON
    case duplicateObjectKey
    case invalidNumber
    case byteLimitExceeded
    case depthLimitExceeded
    case invalidSchema
    case invalidValue
}

let deviceModelMaximumSafeInteger = 9_007_199_254_740_991.0

func isSafeJSONNumber(_ value: Double) -> Bool {
    value.isFinite && Swift.abs(value) <= deviceModelMaximumSafeInteger
}

func validateJSONValue(_ value: JSONValue, depth: Int = 0) throws {
    guard depth <= 32 else { throw DeviceModelContractError.depthLimitExceeded }
    switch value {
    case .number(let number):
        guard isSafeJSONNumber(number), boundedJSONNumberToken(number) != nil else {
            throw DeviceModelContractError.invalidNumber
        }
    case .array(let values):
        for child in values { try validateJSONValue(child, depth: depth + 1) }
    case .object(let values):
        for child in values.values { try validateJSONValue(child, depth: depth + 1) }
    case .string, .bool, .null:
        break
    }
}

func jsonEncodedByteCount(_ value: JSONValue, maximum: Int, tolerateInvalidNumbers: Bool = false) -> Int {
    let limit = maximum == Int.max ? Int.max : max(0, maximum) + 1

    func add(_ left: Int, _ right: Int) -> Int {
        guard left < limit, right < limit, left <= limit - right else { return limit }
        return left + right
    }

    func stringCount(_ value: String) -> Int {
        var count = 2
        for scalar in value.unicodeScalars {
            switch scalar.value {
            case 0x08, 0x09, 0x0A, 0x0C, 0x0D, 0x22, 0x5C:
                count = add(count, 2)
            case 0x00...0x1F:
                count = add(count, 6)
            default:
                count = add(count, String(scalar).utf8.count)
            }
            if count >= limit { return limit }
        }
        return count
    }

    func count(_ value: JSONValue, depth: Int) -> Int {
        guard depth <= (tolerateInvalidNumbers ? 1_024 : 32) else { return limit }
        switch value {
        case .null:
            return 4
        case .bool(let flag):
            return flag ? 4 : 5
        case .string(let string):
            return stringCount(string)
        case .number(let number):
            if let token = boundedJSONNumberToken(number) { return token.utf8.count }
            return tolerateInvalidNumbers ? String(number).utf8.count : limit
        case .array(let values):
            var result = 2
            for (index, child) in values.enumerated() {
                if index > 0 { result = add(result, 1) }
                result = add(result, count(child, depth: depth + 1))
                if result >= limit { return limit }
            }
            return result
        case .object(let values):
            var result = 2
            for (index, pair) in values.enumerated() {
                if index > 0 { result = add(result, 1) }
                result = add(result, stringCount(pair.key))
                result = add(result, 1)
                result = add(result, count(pair.value, depth: depth + 1))
                if result >= limit { return limit }
            }
            return result
        }
    }

    return count(value, depth: 0)
}

private func appendStrictJSON(_ value: JSONValue, depth: Int, into output: inout [UInt8]) throws {
    guard depth <= 32 else { throw DeviceModelContractError.depthLimitExceeded }
    switch value {
    case .null:
        output.append(contentsOf: [110, 117, 108, 108])
    case .bool(let flag):
        output.append(contentsOf: flag ? [116, 114, 117, 101] : [102, 97, 108, 115, 101])
    case .string(let string):
        appendJSONString(string, into: &output)
    case .number(let number):
        guard isSafeJSONNumber(number), let token = boundedJSONNumberToken(number) else {
            throw DeviceModelContractError.invalidNumber
        }
        output.append(contentsOf: token.utf8)
    case .array(let values):
        output.append(0x5B)
        for index in values.indices {
            if index > 0 { output.append(0x2C) }
            try appendStrictJSON(values[index], depth: depth + 1, into: &output)
        }
        output.append(0x5D)
    case .object(let values):
        output.append(0x7B)
        for (index, name) in values.keys.sorted(by: utf8Precedes).enumerated() {
            if index > 0 { output.append(0x2C) }
            appendJSONString(name, into: &output)
            output.append(0x3A)
            guard let child = values[name] else { throw DeviceModelContractError.invalidJSON }
            try appendStrictJSON(child, depth: depth + 1, into: &output)
        }
        output.append(0x7D)
    }
}

private func appendJSONString(_ value: String, into output: inout [UInt8]) {
    output.append(0x22)
    let hex = Array("0123456789abcdef".utf8)
    for scalar in value.unicodeScalars {
        switch scalar.value {
        case 0x08: output.append(contentsOf: [0x5C, 0x62])
        case 0x09: output.append(contentsOf: [0x5C, 0x74])
        case 0x0A: output.append(contentsOf: [0x5C, 0x6E])
        case 0x0C: output.append(contentsOf: [0x5C, 0x66])
        case 0x0D: output.append(contentsOf: [0x5C, 0x72])
        case 0x22: output.append(contentsOf: [0x5C, 0x22])
        case 0x5C: output.append(contentsOf: [0x5C, 0x5C])
        case 0x00...0x1F:
            output.append(contentsOf: [0x5C, 0x75, 0x30, 0x30, hex[Int((scalar.value >> 4) & 0xF)], hex[Int(scalar.value & 0xF)]])
        default:
            output.append(contentsOf: String(scalar).utf8)
        }
    }
    output.append(0x22)
}

private struct NormalizedDecimal: Equatable {
    let negative: Bool
    let digits: String
    let exponent: Int
}

// Bounded string normalization compares exact decimal values without arbitrary-precision arithmetic.
private func normalizeJSONDecimal(_ token: String) -> NormalizedDecimal? {
    let bytes = Array(token.utf8)
    if bytes.count > 64 { return nil }
    guard !bytes.isEmpty else { return nil }

    var index = 0
    var negative = false
    if bytes[index] == 0x2D {
        negative = true
        index += 1
        if index >= bytes.count { return nil }
    }

    var integerDigits: [UInt8] = []
    if bytes[index] == 0x30 {
        integerDigits.append(bytes[index])
        index += 1
        if index < bytes.count, isASCIIDigit(bytes[index]) { return nil }
    } else {
        guard bytes[index] >= 0x31, bytes[index] <= 0x39 else { return nil }
        while index < bytes.count, isASCIIDigit(bytes[index]) {
            integerDigits.append(bytes[index])
            index += 1
        }
    }

    var fractionDigits: [UInt8] = []
    if index < bytes.count, bytes[index] == 0x2E {
        index += 1
        let start = index
        while index < bytes.count, isASCIIDigit(bytes[index]) {
            fractionDigits.append(bytes[index])
            index += 1
        }
        if index == start { return nil }
    }

    var explicitExponent = 0
    if index < bytes.count, (bytes[index] == 0x65 || bytes[index] == 0x45) {
        index += 1
        var exponentNegative = false
        if index < bytes.count, (bytes[index] == 0x2B || bytes[index] == 0x2D) {
            exponentNegative = bytes[index] == 0x2D
            index += 1
        }
        let start = index
        var magnitude = 0
        while index < bytes.count, isASCIIDigit(bytes[index]) {
            let digit = Int(bytes[index] - 0x30)
            if magnitude > 1_000 { return nil }
            magnitude = magnitude * 10 + digit
            index += 1
        }
        if index == start { return nil }
        if magnitude > 32 { return nil }
        explicitExponent = exponentNegative ? -magnitude : magnitude
    }
    guard index == bytes.count else { return nil }

    let coefficient = integerDigits + fractionDigits
    if coefficient.count > 32 { return nil }
    var firstSignificant = coefficient.startIndex
    while firstSignificant < coefficient.endIndex, coefficient[firstSignificant] == 0x30 {
        firstSignificant += 1
    }
    if firstSignificant == coefficient.endIndex {
        return NormalizedDecimal(negative: false, digits: "0", exponent: 0)
    }

    var significant = Array(coefficient[firstSignificant...])
    var exponent = explicitExponent - fractionDigits.count
    while significant.count > 1, significant.last == 0x30 {
        significant.removeLast()
        exponent += 1
    }
    guard let digits = String(bytes: significant, encoding: .ascii) else { return nil }
    return NormalizedDecimal(negative: negative, digits: digits, exponent: exponent)
}

private func isASCIIDigit(_ byte: UInt8) -> Bool {
    byte >= 0x30 && byte <= 0x39
}

private func boundedJSONNumberToken(_ number: Double) -> String? {
    guard isSafeJSONNumber(number) else { return nil }
    let shortest = String(number)
    guard normalizeJSONDecimal(shortest) != nil else { return nil }
    if number.rounded(.towardZero) == number {
        return String(Int64(number))
    }
    return shortest
}

private struct StrictJSONParser {
    private let bytes: [UInt8]
    private let maximumDepth: Int
    private var index = 0

    init(bytes: [UInt8], maximumDepth: Int) {
        self.bytes = bytes
        self.maximumDepth = maximumDepth
    }

    mutating func parse() throws -> JSONValue {
        skipWhitespace()
        let value = try parseValue(depth: 0)
        skipWhitespace()
        guard index == bytes.count else { throw DeviceModelContractError.invalidJSON }
        return value
    }

    private mutating func parseValue(depth: Int) throws -> JSONValue {
        guard depth <= maximumDepth, index < bytes.count else {
            throw depth > maximumDepth ? DeviceModelContractError.depthLimitExceeded : DeviceModelContractError.invalidJSON
        }
        switch bytes[index] {
        case 0x7B: return try parseObject(depth: depth)
        case 0x5B: return try parseArray(depth: depth)
        case 0x22: return .string(try parseString())
        case 0x74:
            try consumeLiteral([0x74, 0x72, 0x75, 0x65])
            return .bool(true)
        case 0x66:
            try consumeLiteral([0x66, 0x61, 0x6C, 0x73, 0x65])
            return .bool(false)
        case 0x6E:
            try consumeLiteral([0x6E, 0x75, 0x6C, 0x6C])
            return .null
        case 0x2D, 0x30...0x39:
            return .number(try parseNumber())
        default:
            throw DeviceModelContractError.invalidJSON
        }
    }

    private mutating func parseObject(depth: Int) throws -> JSONValue {
        index += 1
        skipWhitespace()
        var values: JSONObject = [:]
        var names = Set<JSONUTF8Key>()
        if consumeIf(0x7D) { return .object(values) }
        while true {
            guard index < bytes.count, bytes[index] == 0x22 else { throw DeviceModelContractError.invalidJSON }
            let name = try parseString()
            guard names.insert(JSONUTF8Key(name)).inserted else { throw DeviceModelContractError.duplicateObjectKey }
            skipWhitespace()
            guard consumeIf(0x3A) else { throw DeviceModelContractError.invalidJSON }
            skipWhitespace()
            values[name] = try parseValue(depth: depth + 1)
            skipWhitespace()
            if consumeIf(0x7D) { return .object(values) }
            guard consumeIf(0x2C) else { throw DeviceModelContractError.invalidJSON }
            skipWhitespace()
        }
    }

    private mutating func parseArray(depth: Int) throws -> JSONValue {
        index += 1
        skipWhitespace()
        var values: [JSONValue] = []
        if consumeIf(0x5D) { return .array(values) }
        while true {
            values.append(try parseValue(depth: depth + 1))
            skipWhitespace()
            if consumeIf(0x5D) { return .array(values) }
            guard consumeIf(0x2C) else { throw DeviceModelContractError.invalidJSON }
            skipWhitespace()
        }
    }

    private mutating func parseString() throws -> String {
        guard consumeIf(0x22) else { throw DeviceModelContractError.invalidJSON }
        var scalars = String.UnicodeScalarView()
        var rawBytes: [UInt8] = []
        while index < bytes.count {
            let byte = bytes[index]
            if byte == 0x22 {
                try appendRawUTF8(rawBytes, to: &scalars)
                index += 1
                return String(scalars)
            }
            if byte == 0x5C {
                try appendRawUTF8(rawBytes, to: &scalars)
                rawBytes.removeAll(keepingCapacity: true)
                index += 1
                guard index < bytes.count else { throw DeviceModelContractError.invalidJSON }
                let escaped = bytes[index]
                index += 1
                switch escaped {
                case 0x22: scalars.append(UnicodeScalar(0x22)!)
                case 0x5C: scalars.append(UnicodeScalar(0x5C)!)
                case 0x2F: scalars.append(UnicodeScalar(0x2F)!)
                case 0x62: scalars.append(UnicodeScalar(0x08)!)
                case 0x66: scalars.append(UnicodeScalar(0x0C)!)
                case 0x6E: scalars.append(UnicodeScalar(0x0A)!)
                case 0x72: scalars.append(UnicodeScalar(0x0D)!)
                case 0x74: scalars.append(UnicodeScalar(0x09)!)
                case 0x75:
                    let first = try parseHexQuad()
                    let scalarValue: UInt32
                    if (0xD800...0xDBFF).contains(first) {
                        guard index + 1 < bytes.count, bytes[index] == 0x5C, bytes[index + 1] == 0x75 else {
                            throw DeviceModelContractError.invalidJSON
                        }
                        index += 2
                        let second = try parseHexQuad()
                        guard (0xDC00...0xDFFF).contains(second) else { throw DeviceModelContractError.invalidJSON }
                        scalarValue = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                    } else {
                        guard !(0xDC00...0xDFFF).contains(first) else { throw DeviceModelContractError.invalidJSON }
                        scalarValue = first
                    }
                    guard let scalar = UnicodeScalar(scalarValue) else { throw DeviceModelContractError.invalidJSON }
                    scalars.append(scalar)
                default:
                    throw DeviceModelContractError.invalidJSON
                }
            } else {
                guard byte >= 0x20 else { throw DeviceModelContractError.invalidJSON }
                rawBytes.append(byte)
                index += 1
            }
        }
        throw DeviceModelContractError.invalidJSON
    }

    private func appendRawUTF8(_ bytes: [UInt8], to scalars: inout String.UnicodeScalarView) throws {
        guard let string = String(bytes: bytes, encoding: .utf8) else { throw DeviceModelContractError.invalidJSON }
        scalars.append(contentsOf: string.unicodeScalars)
    }

    private mutating func parseHexQuad() throws -> UInt32 {
        guard index + 4 <= bytes.count else { throw DeviceModelContractError.invalidJSON }
        var value: UInt32 = 0
        for _ in 0..<4 {
            let byte = bytes[index]
            index += 1
            let digit: UInt32
            switch byte {
            case 0x30...0x39: digit = UInt32(byte - 0x30)
            case 0x41...0x46: digit = UInt32(byte - 0x41 + 10)
            case 0x61...0x66: digit = UInt32(byte - 0x61 + 10)
            default: throw DeviceModelContractError.invalidJSON
            }
            value = value * 16 + digit
        }
        return value
    }

    private mutating func parseNumber() throws -> Double {
        let start = index
        if consumeIf(0x2D), index >= bytes.count { throw DeviceModelContractError.invalidJSON }
        if consumeIf(0x30) {
            if index < bytes.count, isASCIIDigit(bytes[index]) { throw DeviceModelContractError.invalidJSON }
        } else {
            guard index < bytes.count, bytes[index] >= 0x31, bytes[index] <= 0x39 else {
                throw DeviceModelContractError.invalidJSON
            }
            while index < bytes.count, isASCIIDigit(bytes[index]) { index += 1 }
        }
        if consumeIf(0x2E) {
            let fractionStart = index
            while index < bytes.count, isASCIIDigit(bytes[index]) { index += 1 }
            guard index > fractionStart else { throw DeviceModelContractError.invalidJSON }
        }
        if index < bytes.count, (bytes[index] == 0x65 || bytes[index] == 0x45) {
            index += 1
            if index < bytes.count, (bytes[index] == 0x2B || bytes[index] == 0x2D) { index += 1 }
            let exponentStart = index
            while index < bytes.count, isASCIIDigit(bytes[index]) { index += 1 }
            guard index > exponentStart else { throw DeviceModelContractError.invalidJSON }
        }
        let tokenBytes = Array(bytes[start..<index])
        guard tokenBytes.count <= 64, let token = String(bytes: tokenBytes, encoding: .ascii),
              let original = normalizeJSONDecimal(token),
              let value = Double(token), isSafeJSONNumber(value),
              let roundTrip = normalizeJSONDecimal(String(value)),
              original == roundTrip else {
            throw DeviceModelContractError.invalidNumber
        }
        return value
    }

    private mutating func consumeLiteral(_ literal: [UInt8]) throws {
        guard index + literal.count <= bytes.count,
              Array(bytes[index..<(index + literal.count)]) == literal else {
            throw DeviceModelContractError.invalidJSON
        }
        index += literal.count
    }

    private mutating func skipWhitespace() {
        while index < bytes.count {
            switch bytes[index] {
            case 0x20, 0x09, 0x0A, 0x0D: index += 1
            default: return
            }
        }
    }

    private mutating func consumeIf(_ byte: UInt8) -> Bool {
        guard index < bytes.count, bytes[index] == byte else { return false }
        index += 1
        return true
    }
}
