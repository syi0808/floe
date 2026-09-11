#if canImport(HealthKit) && canImport(UIKit) && os(iOS)
import Foundation
import HealthKit
import UIKit

public enum HealthKitWellbeingFailure: Error, Equatable {
    case unsupported
    case permissionRequired
    case noDataOrReadAccessLimited
    case unavailable
}

public actor HealthKitWellbeingProvider {
    private let healthStore: HKHealthStore
    private let sourceHandle: String
    private let host: AppleHealthHost
    private let operatingSystemMajorVersion: Int
    private let now: @Sendable () -> Date
    private var authorizationRequestCompleted = false
    private var lastView: AppleWellbeingView?
    private var lastSuccessAtUnixMs: Int64?
    private var lastReadHadNoData = false

    init(
        healthStore: HKHealthStore = HKHealthStore(),
        sourceHandle: String,
        host: AppleHealthHost,
        operatingSystemMajorVersion: Int,
        now: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.healthStore = healthStore
        self.sourceHandle = sourceHandle
        self.host = host
        self.operatingSystemMajorVersion = operatingSystemMajorVersion
        self.now = now
    }

    @MainActor
    public static func currentHostProvider(sourceHandle: String) -> HealthKitWellbeingProvider {
#if targetEnvironment(macCatalyst)
        let host = AppleHealthHost.macCatalyst
#else
        let host: AppleHealthHost = UIDevice.current.userInterfaceIdiom == .pad ? .iPad : .iPhone
#endif
        return HealthKitWellbeingProvider(
            sourceHandle: sourceHandle,
            host: host,
            operatingSystemMajorVersion: ProcessInfo.processInfo.operatingSystemVersion.majorVersion
        )
    }

    static var readTypes: Set<HKObjectType> {
        var types = Set<HKObjectType>()
        if let sleep = HKObjectType.categoryType(forIdentifier: .sleepAnalysis) { types.insert(sleep) }
        if let steps = HKObjectType.quantityType(forIdentifier: .stepCount) { types.insert(steps) }
        if let exercise = HKObjectType.quantityType(forIdentifier: .appleExerciseTime) { types.insert(exercise) }
        return types
    }

    public func requestReadAuthorization() async throws -> AppleHealthLifecycle {
        guard isSupported else { throw HealthKitWellbeingFailure.unsupported }
        do {
            try await requestAuthorization()
            authorizationRequestCompleted = true
            return await lifecycle()
        } catch {
            throw HealthKitWellbeingFailure.unavailable
        }
    }

    public func readDerivedWellbeing() async throws -> AppleWellbeingView {
        guard isSupported else { throw HealthKitWellbeingFailure.unsupported }
        guard try await authorizationRequestStatus() != .shouldRequest else {
            throw HealthKitWellbeingFailure.permissionRequired
        }

        let end = now()
        let start = end.addingTimeInterval(-36 * 60 * 60)
        do {
            async let sleepHours = querySleepHours(start: start, end: end)
            async let steps = queryCumulativeQuantity(.stepCount, unit: .count(), start: start, end: end)
            async let exerciseMinutes = queryCumulativeQuantity(.appleExerciseTime, unit: .minute(), start: start, end: end)
            let aggregate = try await AppleHealthAggregate(
                sleepHours: sleepHours,
                steps: steps,
                exerciseMinutes: exerciseMinutes
            )
            let observedAtUnixMs = Int64(end.timeIntervalSince1970 * 1_000)
            guard let view = AppleWellbeingReducer.reduce(
                aggregate: aggregate,
                sourceHandle: sourceHandle,
                observedAtUnixMs: observedAtUnixMs,
                evidenceHandle: { namespace in "\(namespace):\(UUID().uuidString.lowercased())" }
            ) else {
                lastView = nil
                lastReadHadNoData = true
                throw HealthKitWellbeingFailure.noDataOrReadAccessLimited
            }
            lastView = view
            lastSuccessAtUnixMs = observedAtUnixMs
            lastReadHadNoData = false
            return view
        } catch let failure as HealthKitWellbeingFailure {
            throw failure
        } catch {
            throw HealthKitWellbeingFailure.unavailable
        }
    }

    public func lifecycle() async -> AppleHealthLifecycle {
        let observedAtUnixMs = Int64(now().timeIntervalSince1970 * 1_000)
        guard isSupported else {
            return AppleHealthLifecycle(state: .unsupported, observedAtUnixMs: observedAtUnixMs, lastSuccessAtUnixMs: lastSuccessAtUnixMs, view: nil)
        }
        let requestStatus: HKAuthorizationRequestStatus
        do {
            requestStatus = try await authorizationRequestStatus()
        } catch {
            return AppleHealthLifecycle(state: .unavailable, observedAtUnixMs: observedAtUnixMs, lastSuccessAtUnixMs: lastSuccessAtUnixMs, view: nil)
        }
        if requestStatus == .shouldRequest && !authorizationRequestCompleted {
            return AppleHealthLifecycle(state: .permissionRequired, observedAtUnixMs: observedAtUnixMs, lastSuccessAtUnixMs: lastSuccessAtUnixMs, view: nil)
        }
        if lastReadHadNoData {
            return AppleHealthLifecycle(state: .noDataOrReadAccessLimited, observedAtUnixMs: observedAtUnixMs, lastSuccessAtUnixMs: lastSuccessAtUnixMs, view: nil)
        }
        guard let lastView else {
            return AppleHealthLifecycle(state: .pending, observedAtUnixMs: observedAtUnixMs, lastSuccessAtUnixMs: lastSuccessAtUnixMs, view: nil)
        }
        guard lastView.expiresAtUnixMs > observedAtUnixMs else {
            return AppleHealthLifecycle(state: .stale, observedAtUnixMs: observedAtUnixMs, lastSuccessAtUnixMs: lastSuccessAtUnixMs, view: nil)
        }
        return AppleHealthLifecycle(state: .ready, observedAtUnixMs: observedAtUnixMs, lastSuccessAtUnixMs: lastSuccessAtUnixMs, view: lastView)
    }

    private var isSupported: Bool {
        return AppleHealthAvailability.isSupported(
            host: host,
            operatingSystemMajorVersion: operatingSystemMajorVersion,
            healthDataAvailable: HKHealthStore.isHealthDataAvailable()
        )
    }

    private func requestAuthorization() async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            healthStore.requestAuthorization(toShare: [], read: Self.readTypes) { success, error in
                if let error {
                    continuation.resume(throwing: error)
                } else if success {
                    continuation.resume(returning: ())
                } else {
                    continuation.resume(throwing: HealthKitWellbeingFailure.unavailable)
                }
            }
        }
    }

    private func authorizationRequestStatus() async throws -> HKAuthorizationRequestStatus {
        try await withCheckedThrowingContinuation { continuation in
            healthStore.getRequestStatusForAuthorization(toShare: [], read: Self.readTypes) { status, error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: status)
                }
            }
        }
    }

    private func querySleepHours(start: Date, end: Date) async throws -> Double? {
        guard let type = HKObjectType.categoryType(forIdentifier: .sleepAnalysis) else { return nil }
        let samples: [HKCategorySample] = try await querySamples(type: type, start: start, end: end)
        let asleepIntervals = samples.compactMap { sample -> DateInterval? in
            let asleepValues: Set<Int>
            if #available(iOS 16.0, *) {
                asleepValues = [
                    HKCategoryValueSleepAnalysis.asleepUnspecified.rawValue,
                    HKCategoryValueSleepAnalysis.asleepCore.rawValue,
                    HKCategoryValueSleepAnalysis.asleepDeep.rawValue,
                    HKCategoryValueSleepAnalysis.asleepREM.rawValue,
                ]
            } else {
                asleepValues = [HKCategoryValueSleepAnalysis.asleep.rawValue]
            }
            return asleepValues.contains(sample.value) ? DateInterval(start: sample.startDate, end: sample.endDate) : nil
        }
        guard !asleepIntervals.isEmpty else { return nil }
        let merged = asleepIntervals.sorted { $0.start < $1.start }.reduce(into: [DateInterval]()) { result, interval in
            guard let previous = result.last, interval.start <= previous.end else {
                result.append(interval)
                return
            }
            result[result.count - 1] = DateInterval(start: previous.start, end: max(previous.end, interval.end))
        }
        return merged.reduce(0) { $0 + $1.duration } / 3_600
    }

    private func querySamples<Sample: HKSample>(type: HKSampleType, start: Date, end: Date) async throws -> [Sample] {
        try await withCheckedThrowingContinuation { continuation in
            let predicate = HKQuery.predicateForSamples(withStart: start, end: end, options: .strictEndDate)
            let query = HKSampleQuery(sampleType: type, predicate: predicate, limit: 100, sortDescriptors: nil) { _, samples, error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: (samples as? [Sample]) ?? [])
                }
            }
            healthStore.execute(query)
        }
    }

    private func queryCumulativeQuantity(
        _ identifier: HKQuantityTypeIdentifier,
        unit: HKUnit,
        start: Date,
        end: Date
    ) async throws -> Double? {
        guard let type = HKObjectType.quantityType(forIdentifier: identifier) else { return nil }
        return try await withCheckedThrowingContinuation { continuation in
            let predicate = HKQuery.predicateForSamples(withStart: start, end: end, options: .strictEndDate)
            let query = HKStatisticsQuery(quantityType: type, quantitySamplePredicate: predicate, options: .cumulativeSum) { _, statistics, error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: statistics?.sumQuantity()?.doubleValue(for: unit))
                }
            }
            healthStore.execute(query)
        }
    }
}
#endif
