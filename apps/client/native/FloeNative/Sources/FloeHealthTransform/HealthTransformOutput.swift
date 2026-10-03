import Foundation

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

    private enum CodingKeys: String, CodingKey {
        case capacity
        case recovery
    }

    public init(capacity: Capacity, recovery: Recovery) {
        self.capacity = capacity
        self.recovery = recovery
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: HealthTransformOutputCodingKey.self)
        let keys = Set(container.allKeys.map(\.stringValue))
        guard keys == Set(["capacity", "recovery"]) else {
            throw DecodingError.dataCorrupted(.init(
                codingPath: decoder.codingPath,
                debugDescription: "Health transform output must contain exactly capacity and recovery."
            ))
        }

        capacity = try container.decode(
            Capacity.self,
            forKey: HealthTransformOutputCodingKey(stringValue: "capacity")!
        )
        recovery = try container.decode(
            Recovery.self,
            forKey: HealthTransformOutputCodingKey(stringValue: "recovery")!
        )
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(capacity, forKey: .capacity)
        try container.encode(recovery, forKey: .recovery)
    }
}

private struct HealthTransformOutputCodingKey: CodingKey {
    let stringValue: String
    let intValue: Int?

    init?(stringValue: String) {
        self.stringValue = stringValue
        self.intValue = nil
    }

    init?(intValue: Int) {
        self.stringValue = String(intValue)
        self.intValue = intValue
    }
}
