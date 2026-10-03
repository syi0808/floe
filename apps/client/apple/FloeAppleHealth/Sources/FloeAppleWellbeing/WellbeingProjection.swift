import Foundation
import FloeHealthTransform
import FloeHealthTransformBridge

public struct AppleWellbeingView: Codable, Equatable, Sendable {
    public let schemaVersion: Int
    public let viewId: String
    public let sourceHandle: String
    public let observedAtUnixMs: Int64
    public let expiresAtUnixMs: Int64
    public let capacity: HealthTransformOutput.Capacity
    public let recovery: HealthTransformOutput.Recovery
    public let confidenceMillis: Int
    public let evidenceHandles: [String]

    public init(
        schemaVersion: Int,
        viewId: String,
        sourceHandle: String,
        observedAtUnixMs: Int64,
        expiresAtUnixMs: Int64,
        capacity: HealthTransformOutput.Capacity,
        recovery: HealthTransformOutput.Recovery,
        confidenceMillis: Int,
        evidenceHandles: [String]
    ) {
        self.schemaVersion = schemaVersion
        self.viewId = viewId
        self.sourceHandle = sourceHandle
        self.observedAtUnixMs = observedAtUnixMs
        self.expiresAtUnixMs = expiresAtUnixMs
        self.capacity = capacity
        self.recovery = recovery
        self.confidenceMillis = confidenceMillis
        self.evidenceHandles = evidenceHandles
    }

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

    public init(
        state: AppleHealthLifecycleState,
        observedAtUnixMs: Int64,
        lastSuccessAtUnixMs: Int64?,
        view: AppleWellbeingView?
    ) {
        self.state = state
        self.observedAtUnixMs = observedAtUnixMs
        self.lastSuccessAtUnixMs = lastSuccessAtUnixMs
        self.view = view
    }
}

public struct AppleWellbeingObservation: Sendable {
    public let view: AppleWellbeingView
    public let privacyTransform: HealthTransformProof

    public init(view: AppleWellbeingView, privacyTransform: HealthTransformProof) {
        self.view = view
        self.privacyTransform = privacyTransform
    }
}

enum AppleWellbeingProjection {
    static let freshnessMilliseconds: Int64 = 30 * 60 * 1_000

    static func make(
        output: HealthTransformOutput,
        proof: HealthTransformProof,
        sourceHandle: String,
        observedAtUnixMs: Int64
    ) -> AppleWellbeingView {
        let known = output.capacity != .unknown || output.recovery != .unknown
        let evidenceHandles = known
            ? ["health.transform:\(proof.operationID.uuidString.lowercased())"]
            : []
        return AppleWellbeingView(
            schemaVersion: 1,
            viewId: "wellbeing.derived",
            sourceHandle: sourceHandle,
            observedAtUnixMs: observedAtUnixMs,
            expiresAtUnixMs: observedAtUnixMs + freshnessMilliseconds,
            capacity: output.capacity,
            recovery: output.recovery,
            confidenceMillis: known ? 600 : 0,
            evidenceHandles: evidenceHandles
        )
    }
}
