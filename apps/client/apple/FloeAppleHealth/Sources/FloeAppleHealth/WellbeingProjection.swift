import Foundation

public enum AppleHealthCapacity: String, Codable, Sendable {
    case reduced
    case typical
    case strong
    case unknown
}

public enum AppleHealthRecovery: String, Codable, Sendable {
    case needsRecovery = "needs_recovery"
    case typical
    case recovered
    case unknown
}

public struct AppleWellbeingView: Codable, Equatable, Sendable {
    public let schemaVersion: Int
    public let viewId: String
    public let sourceHandle: String
    public let observedAtUnixMs: Int64
    public let expiresAtUnixMs: Int64
    public let capacity: AppleHealthCapacity
    public let recovery: AppleHealthRecovery
    public let confidenceMillis: Int
    public let evidenceHandles: [String]

    public func encodedForBoundary() throws -> Data {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(self)
    }
}

public enum AppleHealthLifecycleState: String, Codable, Sendable {
    case permissionRequired = "permission_required"
    case pending
    case ready
    case noDataOrReadAccessLimited = "no_data_or_read_access_limited"
    case stale
    case unavailable
    case unsupported
}

public struct AppleHealthLifecycle: Equatable, Sendable {
    public let state: AppleHealthLifecycleState
    public let observedAtUnixMs: Int64
    public let lastSuccessAtUnixMs: Int64?
    public let view: AppleWellbeingView?
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

struct AppleHealthAggregate: Equatable, Sendable {
    let sleepHours: Double?
    let steps: Double?
    let exerciseMinutes: Double?
}

enum AppleWellbeingReducer {
    static let freshnessMilliseconds: Int64 = 30 * 60 * 1_000

    static func reduce(
        aggregate: AppleHealthAggregate,
        sourceHandle: String,
        observedAtUnixMs: Int64,
        evidenceHandle: (String) -> String
    ) -> AppleWellbeingView? {
        let availableSignals = [aggregate.sleepHours, aggregate.steps, aggregate.exerciseMinutes].compactMap { $0 }
        guard !availableSignals.isEmpty else { return nil }

        let capacity: AppleHealthCapacity
        if let sleepHours = aggregate.sleepHours, sleepHours < 6 {
            capacity = .reduced
        } else if let sleepHours = aggregate.sleepHours,
                  sleepHours >= 8,
                  (aggregate.steps ?? 0) >= 8_000 || (aggregate.exerciseMinutes ?? 0) >= 30 {
            capacity = .strong
        } else {
            capacity = .typical
        }

        let recovery: AppleHealthRecovery
        if let sleepHours = aggregate.sleepHours, sleepHours < 6 {
            recovery = .needsRecovery
        } else if let sleepHours = aggregate.sleepHours, sleepHours >= 8 {
            recovery = .recovered
        } else {
            recovery = .typical
        }

        var evidenceHandles: [String] = []
        if aggregate.sleepHours != nil { evidenceHandles.append(evidenceHandle("health.sleep.window")) }
        if aggregate.steps != nil { evidenceHandles.append(evidenceHandle("health.steps.window")) }
        if aggregate.exerciseMinutes != nil { evidenceHandles.append(evidenceHandle("health.exercise.window")) }

        return AppleWellbeingView(
            schemaVersion: 1,
            viewId: "wellbeing.derived",
            sourceHandle: sourceHandle,
            observedAtUnixMs: observedAtUnixMs,
            expiresAtUnixMs: observedAtUnixMs + freshnessMilliseconds,
            capacity: capacity,
            recovery: recovery,
            confidenceMillis: min(700, 400 + evidenceHandles.count * 100),
            evidenceHandles: evidenceHandles
        )
    }
}
