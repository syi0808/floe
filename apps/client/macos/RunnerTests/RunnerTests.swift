import Cocoa
import FlutterMacOS
import XCTest
@testable import floe_client

class RunnerTests: XCTestCase {

  func testAttentionReducerUsesOnlyCoarseActivationCounts() {
    let start = Date(timeIntervalSince1970: 1_000)
    let reducer = MacOSAttentionReducer(startedAt: start)
    for offset in [10, 20, 30, 40, 50, 60] {
      reducer.recordActivation(at: start.addingTimeInterval(TimeInterval(offset)))
    }

    let projection = reducer.projection(at: start.addingTimeInterval(90), idleSeconds: 2)

    XCTAssertEqual(projection.state, "high_interruption_pressure")
    XCTAssertEqual(projection.confidenceMillis, 800)
    XCTAssertEqual(projection.evidenceHandles, [
      "attention.macos:switch_pressure",
      "attention.macos:recent_input",
    ])
  }

  func testAttentionReducerDiscardsActivityOlderThanFifteenMinutes() {
    let start = Date(timeIntervalSince1970: 1_000)
    let reducer = MacOSAttentionReducer(startedAt: start)
    reducer.recordActivation(at: start)

    _ = reducer.projection(at: start.addingTimeInterval(901), idleSeconds: 10)

    XCTAssertEqual(reducer.retainedActivationCount, 0)
  }

  func testAttentionReducerReturnsUnknownUntilObservationWindowExists() {
    let start = Date(timeIntervalSince1970: 1_000)
    let reducer = MacOSAttentionReducer(startedAt: start)

    let projection = reducer.projection(at: start.addingTimeInterval(30), idleSeconds: 2)

    XCTAssertEqual(projection.state, "unknown")
    XCTAssertEqual(projection.confidenceMillis, 0)
    XCTAssertTrue(projection.evidenceHandles.isEmpty)
  }

  func testAttentionReducerDoesNotTreatInactiveSessionAsInterruptible() {
    let start = Date(timeIntervalSince1970: 1_000)
    let reducer = MacOSAttentionReducer(startedAt: start)
    reducer.recordSessionActive(false, at: start.addingTimeInterval(60))

    let projection = reducer.projection(at: start.addingTimeInterval(90), idleSeconds: 2)

    XCTAssertEqual(projection.state, "unknown")
    XCTAssertEqual(projection.confidenceMillis, 0)
    XCTAssertTrue(projection.evidenceHandles.isEmpty)
  }

  func testAttentionReducerRequiresRecentPresence() {
    let start = Date(timeIntervalSince1970: 1_000)
    let reducer = MacOSAttentionReducer(startedAt: start)

    let projection = reducer.projection(at: start.addingTimeInterval(360), idleSeconds: 300)

    XCTAssertEqual(projection.state, "unknown")
    XCTAssertEqual(projection.confidenceMillis, 0)
    XCTAssertTrue(projection.evidenceHandles.isEmpty)
  }

}
