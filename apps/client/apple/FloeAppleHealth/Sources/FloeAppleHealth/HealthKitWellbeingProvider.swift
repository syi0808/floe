#if canImport(HealthKit) && canImport(UIKit) && os(iOS)
import Foundation
import HealthKit
import UIKit

public actor HealthKitWellbeingProvider {
    private let healthStore: HKHealthStore
    private let host: AppleHealthHost
    private let operatingSystemMajorVersion: Int
    private let now: @Sendable () -> Date
    private var authorizationRequestCompleted = false

    init(
        healthStore: HKHealthStore = HKHealthStore(),
        host: AppleHealthHost,
        operatingSystemMajorVersion: Int,
        now: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.healthStore = healthStore
        self.host = host
        self.operatingSystemMajorVersion = operatingSystemMajorVersion
        self.now = now
    }

    @MainActor
    public static func currentHostProvider() -> HealthKitWellbeingProvider {
#if targetEnvironment(macCatalyst)
        let host = AppleHealthHost.macCatalyst
#else
        let host: AppleHealthHost = UIDevice.current.userInterfaceIdiom == .pad ? .iPad : .iPhone
#endif
        return HealthKitWellbeingProvider(
            host: host,
            operatingSystemMajorVersion: ProcessInfo.processInfo.operatingSystemVersion.majorVersion
        )
    }

    private static var readTypes: Set<HKObjectType> {
        var types = Set<HKObjectType>()
        if let sleep = HKObjectType.categoryType(forIdentifier: .sleepAnalysis) { types.insert(sleep) }
        if let steps = HKObjectType.quantityType(forIdentifier: .stepCount) { types.insert(steps) }
        if let exercise = HKObjectType.quantityType(forIdentifier: .appleExerciseTime) { types.insert(exercise) }
        return types
    }

    public func requestReadAuthorization() async throws {
        guard isSupported else { throw HealthKitWellbeingFailure.unsupported }
        do {
            try await requestAuthorization()
            authorizationRequestCompleted = true
        } catch let failure as HealthKitWellbeingFailure {
            throw failure
        } catch {
            throw HealthKitWellbeingFailure.unavailable
        }
    }

    public func readAggregates() async throws -> HealthKitWellbeingAcquisition {
        guard isSupported else { throw HealthKitWellbeingFailure.unsupported }
        do {
            guard try await authorizationRequestStatus() != .shouldRequest else {
                throw HealthKitWellbeingFailure.permissionRequired
            }

            let end = now()
            let start = end.addingTimeInterval(-36 * 60 * 60)
            async let sleepHours = querySleepHours(start: start, end: end)
            async let steps = queryCumulativeQuantity(.stepCount, unit: .count(), start: start, end: end)
            async let exerciseMinutes = queryCumulativeQuantity(.appleExerciseTime, unit: .minute(), start: start, end: end)
            let values = try await (sleepHours, steps, exerciseMinutes)
            guard values.0 != nil || values.1 != nil || values.2 != nil else {
                throw HealthKitWellbeingFailure.noDataOrReadAccessLimited
            }
            return HealthKitWellbeingAcquisition(
                sleepHours: values.0,
                steps: values.1,
                exerciseMinutes: values.2
            )
        } catch let failure as HealthKitWellbeingFailure {
            throw failure
        } catch {
            throw HealthKitWellbeingFailure.unavailable
        }
    }

    public func readStatus() async -> HealthKitReadStatus {
        guard isSupported else { return .unsupported }
        do {
            let status = try await authorizationRequestStatus()
            if status == .shouldRequest {
                return .requestRequired(requestCompleted: authorizationRequestCompleted)
            }
            return .queryable
        } catch {
            return .unavailable
        }
    }

    private var isSupported: Bool {
        AppleHealthAvailability.isSupported(
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
