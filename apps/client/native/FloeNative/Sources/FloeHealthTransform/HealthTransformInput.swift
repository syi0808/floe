import Foundation

public struct HealthTransformInput: Codable, Equatable, Sendable {
    public let sleepHours: Double?
    public let steps: Double?
    public let exerciseMinutes: Double?

    private enum CodingKeys: String, CodingKey {
        case sleepHours = "sleep_hours"
        case steps
        case exerciseMinutes = "exercise_minutes"
    }

    public init(sleepHours: Double?, steps: Double?, exerciseMinutes: Double?) throws {
        self.sleepHours = sleepHours
        self.steps = steps
        self.exerciseMinutes = exerciseMinutes
        try validate()
    }

    public func validate() throws {
        guard sleepHours != nil || steps != nil || exerciseMinutes != nil else {
            throw HealthTransformValidationError.invalidInput
        }

        for (value, maximum) in [
            (sleepHours, 36.0),
            (steps, 1_000_000.0),
            (exerciseMinutes, 2_160.0),
        ] {
            guard let value else { continue }
            guard value.isFinite, value >= 0, value <= maximum else {
                throw HealthTransformValidationError.invalidInput
            }
        }
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: HealthTransformInputCodingKey.self)
        let allowedKeys: Set<String> = ["sleep_hours", "steps", "exercise_minutes"]
        guard container.allKeys.allSatisfy({ allowedKeys.contains($0.stringValue) }) else {
            throw DecodingError.dataCorrupted(.init(
                codingPath: decoder.codingPath,
                debugDescription: "Health transform input contains an unsupported field."
            ))
        }

        sleepHours = try container.decodeIfPresent(
            Double.self,
            forKey: HealthTransformInputCodingKey(stringValue: "sleep_hours")!
        )
        steps = try container.decodeIfPresent(
            Double.self,
            forKey: HealthTransformInputCodingKey(stringValue: "steps")!
        )
        exerciseMinutes = try container.decodeIfPresent(
            Double.self,
            forKey: HealthTransformInputCodingKey(stringValue: "exercise_minutes")!
        )
        try validate()
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encodeIfPresent(sleepHours, forKey: .sleepHours)
        try container.encodeIfPresent(steps, forKey: .steps)
        try container.encodeIfPresent(exerciseMinutes, forKey: .exerciseMinutes)
    }
}

public enum HealthTransformValidationError: Error, Equatable, Sendable {
    case invalidInput
    case invalidOutput
}

private struct HealthTransformInputCodingKey: CodingKey {
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
