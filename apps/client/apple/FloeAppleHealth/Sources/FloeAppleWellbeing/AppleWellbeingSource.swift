#if canImport(HealthKit) && canImport(UIKit) && os(iOS)
import Foundation
import FloeAppleHealth
import FloeHealthTransform
import FloeHealthTransformBridge

public enum AppleWellbeingFailure: Error, Equatable, Sendable {
    case unsupported
    case permissionRequired
    case noDataOrReadAccessLimited
    case unavailable
    case privacyTransform(HealthTransformFailure)
}

public actor AppleWellbeingSource {
    private let acquisition: HealthKitWellbeingProvider
    private let transform: any HealthTransformOperationClient
    private let sourceHandle: String
    private let now: @Sendable () -> Date
    private var lastView: AppleWellbeingView?
    private var lastSuccessAtUnixMs: Int64?
    private var lastReadHadNoData = false

    public init(
        acquisition: HealthKitWellbeingProvider,
        transform: any HealthTransformOperationClient,
        sourceHandle: String,
        now: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.acquisition = acquisition
        self.transform = transform
        self.sourceHandle = sourceHandle
        self.now = now
    }

    public func requestReadAuthorization() async throws -> AppleHealthLifecycle {
        do {
            try await acquisition.requestReadAuthorization()
            return await lifecycle()
        } catch let failure as HealthKitWellbeingFailure {
            throw Self.map(failure)
        } catch {
            throw AppleWellbeingFailure.unavailable
        }
    }

    public func readDerivedWellbeing(binding: HealthTransformBinding) async throws -> AppleWellbeingObservation {
        do {
            let values = try await acquisition.readAggregates()
            let input = try AppleHealthWellbeingMapper.map(values)
            let success = try await transform.perform(input, binding: binding)
            try Task.checkCancellation()

            let view = AppleWellbeingProjection.make(
                output: success.output,
                proof: success.proof,
                sourceHandle: sourceHandle,
                observedAtUnixMs: success.transformedAtUnixMs
            )
            lastView = view
            lastSuccessAtUnixMs = success.transformedAtUnixMs
            lastReadHadNoData = false
            return AppleWellbeingObservation(view: view, privacyTransform: success.proof)
        } catch let failure as HealthKitWellbeingFailure {
            lastView = nil
            if case .noDataOrReadAccessLimited = failure {
                lastReadHadNoData = true
            }
            throw Self.map(failure)
        } catch let failure as HealthTransformValidationError {
            lastView = nil
            throw AppleWellbeingFailure.privacyTransform(Self.map(failure))
        } catch let failure as HealthTransformFailure {
            lastView = nil
            throw AppleWellbeingFailure.privacyTransform(failure)
        } catch is CancellationError {
            lastView = nil
            throw AppleWellbeingFailure.privacyTransform(.cancelled)
        } catch {
            lastView = nil
            throw AppleWellbeingFailure.privacyTransform(.modelUnavailable)
        }
    }

    public func lifecycle() async -> AppleHealthLifecycle {
        let observedAtUnixMs = Int64(now().timeIntervalSince1970 * 1_000)
        let status = await acquisition.readStatus()
        switch status {
        case .unsupported:
            return AppleHealthLifecycle(
                state: .unsupported,
                observedAtUnixMs: observedAtUnixMs,
                lastSuccessAtUnixMs: lastSuccessAtUnixMs,
                view: nil
            )
        case .unavailable:
            return AppleHealthLifecycle(
                state: .unavailable,
                observedAtUnixMs: observedAtUnixMs,
                lastSuccessAtUnixMs: lastSuccessAtUnixMs,
                view: nil
            )
        case .requestRequired(let requestCompleted) where !requestCompleted:
            return AppleHealthLifecycle(
                state: .permissionRequired,
                observedAtUnixMs: observedAtUnixMs,
                lastSuccessAtUnixMs: lastSuccessAtUnixMs,
                view: nil
            )
        case .requestRequired(_), .queryable:
            break
        }

        if lastReadHadNoData {
            return AppleHealthLifecycle(
                state: .noDataOrReadAccessLimited,
                observedAtUnixMs: observedAtUnixMs,
                lastSuccessAtUnixMs: lastSuccessAtUnixMs,
                view: nil
            )
        }
        guard let lastView else {
            return AppleHealthLifecycle(
                state: .pending,
                observedAtUnixMs: observedAtUnixMs,
                lastSuccessAtUnixMs: lastSuccessAtUnixMs,
                view: nil
            )
        }
        guard lastView.expiresAtUnixMs > observedAtUnixMs else {
            return AppleHealthLifecycle(
                state: .stale,
                observedAtUnixMs: observedAtUnixMs,
                lastSuccessAtUnixMs: lastSuccessAtUnixMs,
                view: nil
            )
        }
        return AppleHealthLifecycle(
            state: .ready,
            observedAtUnixMs: observedAtUnixMs,
            lastSuccessAtUnixMs: lastSuccessAtUnixMs,
            view: lastView
        )
    }

    private static func map(_ failure: HealthKitWellbeingFailure) -> AppleWellbeingFailure {
        switch failure {
        case .unsupported:
            return .unsupported
        case .permissionRequired:
            return .permissionRequired
        case .noDataOrReadAccessLimited:
            return .noDataOrReadAccessLimited
        case .unavailable:
            return .unavailable
        }
    }

    private static func map(_ failure: HealthTransformValidationError) -> HealthTransformFailure {
        switch failure {
        case .invalidInput:
            return .invalidInput
        case .invalidOutput:
            return .invalidOutput
        }
    }
}
#endif
