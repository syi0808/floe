import Foundation
import XCTest
@testable import FloeAppleHealth

final class AppleWellbeingProjectionTests: XCTestCase {
    func testIPadRequiresVersion17AndAnAvailableHealthStore() {
        XCTAssertFalse(AppleHealthAvailability.isSupported(host: .iPad, operatingSystemMajorVersion: 16, healthDataAvailable: true))
        XCTAssertTrue(AppleHealthAvailability.isSupported(host: .iPad, operatingSystemMajorVersion: 17, healthDataAvailable: true))
        XCTAssertFalse(AppleHealthAvailability.isSupported(host: .iPad, operatingSystemMajorVersion: 17, healthDataAvailable: false))
        XCTAssertFalse(AppleHealthAvailability.isSupported(host: .macCatalyst, operatingSystemMajorVersion: 17, healthDataAvailable: true))
    }

    func testReducerEmitsOnlyCoarseDerivedStateAndOpaqueEvidence() throws {
        let view = try XCTUnwrap(AppleWellbeingReducer.reduce(
            aggregate: AppleHealthAggregate(sleepHours: 8.25, steps: 9_100, exerciseMinutes: 35),
            sourceHandle: "wellbeing:opaque-device",
            observedAtUnixMs: 1_789_128_000_000,
            evidenceHandle: { "\($0):opaque" }
        ))

        XCTAssertEqual(view.capacity, .strong)
        XCTAssertEqual(view.recovery, .recovered)
        XCTAssertEqual(view.confidenceMillis, 700)
        XCTAssertEqual(view.expiresAtUnixMs, 1_789_129_800_000)

        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: view.encodedForBoundary()) as? [String: Any])
        XCTAssertEqual(Set(object.keys), [
            "schema_version", "view_id", "source_handle", "observed_at_unix_ms",
            "expires_at_unix_ms", "capacity", "recovery", "confidence_millis", "evidence_handles",
        ])
        for forbidden in ["sleep_hours", "steps", "exercise_minutes", "samples", "provider_id", "metadata"] {
            XCTAssertNil(object[forbidden])
        }
    }

    func testSerializedFixtureMatchesTheSwiftBoundary() throws {
        let view = try XCTUnwrap(AppleWellbeingReducer.reduce(
            aggregate: AppleHealthAggregate(sleepHours: 8.25, steps: 9_100, exerciseMinutes: 35),
            sourceHandle: "wellbeing:apple-fixture",
            observedAtUnixMs: 1_789_128_000_000,
            evidenceHandle: { "\($0):apple-fixture" }
        ))
        let fixtureURL = try XCTUnwrap(Bundle.module.url(forResource: "wellbeing_view", withExtension: "json"))
        let fixtureObject = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: fixtureURL)) as? NSDictionary)
        let encodedObject = try XCTUnwrap(JSONSerialization.jsonObject(with: view.encodedForBoundary()) as? NSDictionary)

        XCTAssertEqual(fixtureObject, encodedObject)
    }

    func testShortSleepReducesCapacityAndRequestsRecovery() throws {
        let view = try XCTUnwrap(AppleWellbeingReducer.reduce(
            aggregate: AppleHealthAggregate(sleepHours: 5.5, steps: 1_000, exerciseMinutes: nil),
            sourceHandle: "wellbeing:opaque-device",
            observedAtUnixMs: 100,
            evidenceHandle: { "\($0):opaque" }
        ))
        XCTAssertEqual(view.capacity, .reduced)
        XCTAssertEqual(view.recovery, .needsRecovery)
        XCTAssertEqual(view.confidenceMillis, 600)
    }

    func testSleepOnlyLeavesCapacityUnknown() throws {
        let view = try XCTUnwrap(AppleWellbeingReducer.reduce(
            aggregate: AppleHealthAggregate(sleepHours: 8.25, steps: nil, exerciseMinutes: nil),
            sourceHandle: "wellbeing:opaque-device",
            observedAtUnixMs: 100,
            evidenceHandle: { "\($0):opaque" }
        ))
        XCTAssertEqual(view.capacity, .unknown)
        XCTAssertEqual(view.recovery, .recovered)
        XCTAssertEqual(view.confidenceMillis, 500)
        XCTAssertEqual(view.evidenceHandles, ["health.sleep.window:opaque"])
    }

    func testStepsOnlyLeavesBothDimensionsUnknown() throws {
        let view = try XCTUnwrap(AppleWellbeingReducer.reduce(
            aggregate: AppleHealthAggregate(sleepHours: nil, steps: 9_100, exerciseMinutes: nil),
            sourceHandle: "wellbeing:opaque-device",
            observedAtUnixMs: 100,
            evidenceHandle: { "\($0):opaque" }
        ))
        XCTAssertEqual(view.capacity, .unknown)
        XCTAssertEqual(view.recovery, .unknown)
        XCTAssertEqual(view.confidenceMillis, 0)
        XCTAssertTrue(view.evidenceHandles.isEmpty)
    }

    func testExerciseOnlyLeavesBothDimensionsUnknown() throws {
        let view = try XCTUnwrap(AppleWellbeingReducer.reduce(
            aggregate: AppleHealthAggregate(sleepHours: nil, steps: nil, exerciseMinutes: 45),
            sourceHandle: "wellbeing:opaque-device",
            observedAtUnixMs: 100,
            evidenceHandle: { "\($0):opaque" }
        ))
        XCTAssertEqual(view.capacity, .unknown)
        XCTAssertEqual(view.recovery, .unknown)
        XCTAssertEqual(view.confidenceMillis, 0)
        XCTAssertTrue(view.evidenceHandles.isEmpty)
    }

    func testNoSignalsProducesNoView() {
        XCTAssertNil(AppleWellbeingReducer.reduce(
            aggregate: AppleHealthAggregate(sleepHours: nil, steps: nil, exerciseMinutes: nil),
            sourceHandle: "wellbeing:opaque-device",
            observedAtUnixMs: 100,
            evidenceHandle: { "\($0):opaque" }
        ))
    }
}
