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

    func testShortSleepReducesCapacityAndRequestsRecovery() throws {
        let view = try XCTUnwrap(AppleWellbeingReducer.reduce(
            aggregate: AppleHealthAggregate(sleepHours: 5.5, steps: nil, exerciseMinutes: nil),
            sourceHandle: "wellbeing:opaque-device",
            observedAtUnixMs: 100,
            evidenceHandle: { "\($0):opaque" }
        ))
        XCTAssertEqual(view.capacity, .reduced)
        XCTAssertEqual(view.recovery, .needsRecovery)
        XCTAssertEqual(view.confidenceMillis, 500)
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
