import Foundation
import XCTest
@testable import FloeNativeHost

final class LiveMediaHostTests: XCTestCase {
    func testMutedLeaseBindsExactIdentityAndStopsOnceWithoutCapture() throws {
        let peer = FakePeer()
        let host = LiveMediaHost(factory: FakeFactory(peer: peer))
        let binding = LiveMediaBindingWire(
            person_id: UUID().uuidString.lowercased(),
            device_id: "device",
            runtime_epoch: 1,
            voice_session_id: UUID().uuidString.lowercased(),
            media_lease_id: UUID().uuidString.lowercased(),
            plan_id: UUID().uuidString.lowercased()
        )
        let hostID = UUID().uuidString.lowercased()

        XCTAssertEqual(try invoke(host, "prepare_muted", hostID, binding).status, "ok")
        XCTAssertFalse(peer.captureStarted)
        XCTAssertEqual(
            try invoke(host, "offer", hostID, binding, deadline: 1_000).offer_sdp,
            "v=0\r\ns=synthetic-offer\r\n"
        )
        XCTAssertEqual(
            try invoke(
                host, "apply_answer", hostID, binding,
                deadline: 1_000, answer: "v=0\r\ns=synthetic-answer\r\n"
            ).status,
            "ok"
        )
        XCTAssertEqual(try invoke(host, "fence", hostID, binding).status, "ok")
        XCTAssertEqual(
            try invoke(host, "offer", hostID, binding, deadline: 1_000).failure,
            LiveMediaHostFailure.conflict.rawValue
        )
        XCTAssertEqual(try invoke(host, "stop_once", hostID, binding, deadline: 1_000).status, "ok")
        XCTAssertEqual(try invoke(host, "stop_once", hostID, binding, deadline: 1_000).status, "ok")
        let observation = try invoke(host, "observe_stop", hostID, binding)
        XCTAssertEqual(observation.capture_stopped, true)
        XCTAssertEqual(observation.playback_stopped, true)
        XCTAssertFalse(peer.captureStarted)
        XCTAssertEqual(peer.stopCount, 1)
        XCTAssertEqual(try invoke(host, "release", hostID, binding).status, "ok")
    }

    func testForeignHostOrBindingCannotUseLease() throws {
        let peer = FakePeer()
        let host = LiveMediaHost(factory: FakeFactory(peer: peer))
        let binding = LiveMediaBindingWire(
            person_id: UUID().uuidString.lowercased(),
            device_id: "device",
            runtime_epoch: 1,
            voice_session_id: UUID().uuidString.lowercased(),
            media_lease_id: UUID().uuidString.lowercased(),
            plan_id: UUID().uuidString.lowercased()
        )
        let hostID = UUID().uuidString.lowercased()
        XCTAssertEqual(try invoke(host, "prepare_muted", hostID, binding).status, "ok")
        let foreign = LiveMediaBindingWire(
            person_id: binding.person_id,
            device_id: "foreign-device",
            runtime_epoch: 1,
            voice_session_id: binding.voice_session_id,
            media_lease_id: binding.media_lease_id,
            plan_id: binding.plan_id
        )
        XCTAssertEqual(
            try invoke(host, "validate", UUID().uuidString.lowercased(), foreign).failure,
            LiveMediaHostFailure.conflict.rawValue
        )
        XCTAssertFalse(peer.captureStarted)
    }

    private func invoke(
        _ host: LiveMediaHost,
        _ operation: String,
        _ hostID: String,
        _ binding: LiveMediaBindingWire,
        deadline: Int64? = nil,
        answer: String? = nil
    ) throws -> LiveMediaReplyWire {
        let command = LiveMediaCommandWire(
            schema_version: 1,
            operation: operation,
            host_instance_id: hostID,
            binding: binding,
            deadline_ms: deadline,
            answer_sdp: answer
        )
        let data = try JSONEncoder().encode(command)
        return try JSONDecoder().decode(LiveMediaReplyWire.self, from: host.invoke(data))
    }
}

private struct FakeFactory: LiveMediaPeerFactory {
    let peer: FakePeer
    func makeMutedPeer() throws -> any LiveMediaPeer { peer }
}

private final class FakePeer: LiveMediaPeer, @unchecked Sendable {
    private let lock = NSLock()
    private var closed = false
    private(set) var captureStarted = false
    private(set) var stopCount = 0

    func createOffer(deadlineMilliseconds: Int64) throws -> String {
        try lock.withLock {
            guard !closed, deadlineMilliseconds > 0 else { throw LiveMediaHostFailure.conflict }
            return "v=0\r\ns=synthetic-offer\r\n"
        }
    }

    func applyAnswer(_ sdp: String, deadlineMilliseconds: Int64) throws {
        try lock.withLock {
            guard !closed, deadlineMilliseconds > 0, sdp.hasPrefix("v=0\r\n") else {
                throw LiveMediaHostFailure.conflict
            }
        }
    }

    func stop(deadlineMilliseconds: Int64) throws {
        try lock.withLock {
            guard deadlineMilliseconds > 0 else { throw LiveMediaHostFailure.deadlineExceeded }
            guard !closed else { return }
            closed = true
            stopCount += 1
        }
    }

    func observeStop() -> (captureStopped: Bool, playbackStopped: Bool) {
        lock.withLock { (closed && !captureStarted, closed) }
    }
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
