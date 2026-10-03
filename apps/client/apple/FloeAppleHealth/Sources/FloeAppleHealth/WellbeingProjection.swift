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

/// Constructs source metadata only after the mandatory local transform succeeds.
enum AppleWellbeingProjection {
    static let freshnessMilliseconds: Int64 = 30 * 60 * 1_000

    static func make(
        output: HealthPrivacyTransformOutput,
        sourceHandle: String,
        observedAtUnixMs: Int64,
        evidenceHandle: String
    ) -> AppleWellbeingView {
        let known = output.capacity != .unknown || output.recovery != .unknown
        return AppleWellbeingView(
            schemaVersion: 1,
            viewId: "wellbeing.derived",
            sourceHandle: sourceHandle,
            observedAtUnixMs: observedAtUnixMs,
            expiresAtUnixMs: observedAtUnixMs + freshnessMilliseconds,
            capacity: AppleHealthCapacity(rawValue: output.capacity.rawValue)!,
            recovery: AppleHealthRecovery(rawValue: output.recovery.rawValue)!,
            confidenceMillis: known ? 600 : 0,
            evidenceHandles: known ? [evidenceHandle] : []
        )
    }
}
