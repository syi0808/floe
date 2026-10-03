public struct HealthKitWellbeingAcquisition: Equatable, Sendable {
    public let sleepHours: Double?
    public let steps: Double?
    public let exerciseMinutes: Double?

    public init(sleepHours: Double?, steps: Double?, exerciseMinutes: Double?) {
        self.sleepHours = sleepHours
        self.steps = steps
        self.exerciseMinutes = exerciseMinutes
    }
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

enum AppleHealthHost: Equatable, Sendable {
    case iPhone
    case iPad
    case macCatalyst
    case unsupported
}

enum AppleHealthAvailability {
    static func isSupported(host: AppleHealthHost, operatingSystemMajorVersion: Int, healthDataAvailable: Bool) -> Bool {
        guard healthDataAvailable else { return false }
        switch host {
        case .iPhone:
            return true
        case .iPad:
            return operatingSystemMajorVersion >= 17
        case .macCatalyst, .unsupported:
            return false
        }
    }
}
