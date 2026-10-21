import Foundation
import XCTest
@testable import FloeNativeHost

final class LiveMediaHostTests: XCTestCase {
    func testMutedLeaseRequestsPermissionOnlyAtStartAndStopsOnce() throws {
        let permission = FakePermissionProvider(status: .notDetermined, grant: true)
        let peer = FakePeer(authorization: MicrophoneAuthorizationGate(provider: permission))
        let host = LiveMediaHost(factory: FakeFactory(peer: peer))
        let binding = makeBinding()
        let hostID = UUID().uuidString.lowercased()

        XCTAssertEqual(try invoke(host, "prepare_muted", hostID, binding).status, "ok")
        XCTAssertFalse(peer.captureStarted)
        XCTAssertEqual(permission.requestCount, 0)
        XCTAssertEqual(
            try invoke(host, "offer", hostID, binding, deadline: 1_000).failure,
            LiveMediaHostFailure.conflict.rawValue
        )

        XCTAssertEqual(
            try invoke(host, "start_audio", hostID, binding, deadline: 1_000).status,
            "ok"
        )
        XCTAssertEqual(permission.requestCount, 1)
        XCTAssertTrue(peer.captureStarted)
        peer.receiveRemoteAudio(id: "receiver-1")
        XCTAssertTrue(peer.remotePlaybackStarted)

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
        XCTAssertFalse(peer.captureStarted)
        XCTAssertFalse(peer.remotePlaybackStarted)
        peer.receiveRemoteAudio(id: "late-receiver")
        XCTAssertFalse(peer.remotePlaybackStarted)

        XCTAssertEqual(try invoke(host, "stop_once", hostID, binding, deadline: 1_000).status, "ok")
        XCTAssertEqual(try invoke(host, "stop_once", hostID, binding, deadline: 1_000).status, "ok")
        let observation = try invoke(host, "observe_stop", hostID, binding)
        XCTAssertEqual(observation.capture_stopped, true)
        XCTAssertEqual(observation.playback_stopped, true)
        XCTAssertEqual(peer.stopCount, 1)
        XCTAssertEqual(try invoke(host, "release", hostID, binding).status, "ok")
    }

    func testDeniedPermissionDoesNotAttachTrackOrCreateOffer() throws {
        let permission = FakePermissionProvider(status: .denied, grant: false)
        let peer = FakePeer(authorization: MicrophoneAuthorizationGate(provider: permission))
        let host = LiveMediaHost(factory: FakeFactory(peer: peer))
        let binding = makeBinding()
        let hostID = UUID().uuidString.lowercased()

        XCTAssertEqual(try invoke(host, "prepare_muted", hostID, binding).status, "ok")
        let start = try invoke(host, "start_audio", hostID, binding, deadline: 1_000)
        XCTAssertEqual(start.failure, LiveMediaHostFailure.microphonePermissionDenied.rawValue)
        XCTAssertFalse(peer.captureStarted)
        XCTAssertEqual(permission.requestCount, 0)
        XCTAssertEqual(
            try invoke(host, "offer", hostID, binding, deadline: 1_000).failure,
            LiveMediaHostFailure.conflict.rawValue
        )
    }

    func testStopFencesPendingPermissionAndDelayedGrantCannotReviveLease() throws {
        let permission = FakePermissionProvider(status: .notDetermined, grant: true, blocksRequest: true)
        let peer = FakePeer(authorization: MicrophoneAuthorizationGate(provider: permission))
        let host = LiveMediaHost(factory: FakeFactory(peer: peer))
        let binding = makeBinding()
        let hostID = UUID().uuidString.lowercased()
        XCTAssertEqual(try invoke(host, "prepare_muted", hostID, binding).status, "ok")

        let request = try commandData("start_audio", hostID, binding, deadline: 5_000)
        let result = AsyncDataResult()
        DispatchQueue.global().async {
            result.set(host.invoke(request))
        }
        XCTAssertEqual(permission.waitForRequest(), .success)

        XCTAssertEqual(try invoke(host, "fence", hostID, binding).status, "ok")
        XCTAssertEqual(try invoke(host, "stop_once", hostID, binding, deadline: 1_000).status, "ok")
        permission.completeRequest(granted: true)

        XCTAssertTrue(result.wait())
        let lateStart = try JSONDecoder().decode(LiveMediaReplyWire.self, from: result.value)
        XCTAssertEqual(lateStart.failure, LiveMediaHostFailure.conflict.rawValue)
        XCTAssertFalse(peer.captureStarted)
        XCTAssertFalse(peer.remotePlaybackStarted)
        peer.receiveRemoteAudio(id: "late-after-stop")
        XCTAssertFalse(peer.remotePlaybackStarted)
        XCTAssertEqual(peer.stopCount, 1)
    }

    func testDeniedAndRestrictedAuthorizationFailClosed() {
        for status in [MicrophoneAuthorizationStatus.denied, .restricted] {
            let permission = FakePermissionProvider(status: status, grant: true)
            let gate = MicrophoneAuthorizationGate(provider: permission)
            XCTAssertThrowsError(try gate.requireAccess(deadlineMilliseconds: 1_000)) { error in
                XCTAssertEqual(error as? LiveMediaHostFailure, .microphonePermissionDenied)
            }
            XCTAssertEqual(permission.requestCount, 0)
        }

        let undecided = FakePermissionProvider(status: .notDetermined, grant: false)
        XCTAssertThrowsError(
            try MicrophoneAuthorizationGate(provider: undecided).requireAccess(deadlineMilliseconds: 1_000)
        ) { error in
            XCTAssertEqual(error as? LiveMediaHostFailure, .microphonePermissionDenied)
        }
        XCTAssertEqual(undecided.requestCount, 1)
    }

    func testInterruptionAndOldLeaseCallbacksCannotReopenAudioAfterRestart() {
        let oldLocal = TrackState()
        let oldRemote = TrackState()
        let oldLease = LiveAudioTrackGate()
        XCTAssertTrue(oldLease.installLocalTrack(id: "old-local") { oldLocal.enabled = $0 })
        XCTAssertTrue(oldLease.activate())
        oldLease.receiveRemoteTrack(id: "old-receiver") { oldRemote.enabled = $0 }
        XCTAssertTrue(oldRemote.enabled)

        oldLease.fence()
        XCTAssertFalse(oldLocal.enabled)
        XCTAssertFalse(oldRemote.enabled)
        XCTAssertFalse(oldLease.activate())

        let lateTrack = TrackState()
        oldLease.receiveRemoteTrack(id: "delayed-old-receiver") { lateTrack.enabled = $0 }
        XCTAssertFalse(lateTrack.enabled)

        let newLocal = TrackState()
        let newLease = LiveAudioTrackGate()
        XCTAssertTrue(newLease.installLocalTrack(id: "new-local") { newLocal.enabled = $0 })
        XCTAssertTrue(newLease.activate())
        XCTAssertTrue(newLocal.enabled)
        XCTAssertFalse(oldLocal.enabled)
        XCTAssertFalse(oldRemote.enabled)
        XCTAssertFalse(lateTrack.enabled)
        oldLease.stop()
        XCTAssertFalse(newLease.isTerminal)
    }

    func testForeignHostOrBindingCannotUseLease() throws {
        let peer = FakePeer(authorization: MicrophoneAuthorizationGate(provider: FakePermissionProvider(status: .authorized)))
        let host = LiveMediaHost(factory: FakeFactory(peer: peer))
        let binding = makeBinding()
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

    private func makeBinding() -> LiveMediaBindingWire {
        LiveMediaBindingWire(
            person_id: UUID().uuidString.lowercased(),
            device_id: "device",
            runtime_epoch: 1,
            voice_session_id: UUID().uuidString.lowercased(),
            media_lease_id: UUID().uuidString.lowercased(),
            plan_id: UUID().uuidString.lowercased()
        )
    }

    private func invoke(
        _ host: LiveMediaHost,
        _ operation: String,
        _ hostID: String,
        _ binding: LiveMediaBindingWire,
        deadline: Int64? = nil,
        answer: String? = nil
    ) throws -> LiveMediaReplyWire {
        try JSONDecoder().decode(
            LiveMediaReplyWire.self,
            from: host.invoke(commandData(operation, hostID, binding, deadline: deadline, answer: answer))
        )
    }

    private func commandData(
        _ operation: String,
        _ hostID: String,
        _ binding: LiveMediaBindingWire,
        deadline: Int64? = nil,
        answer: String? = nil
    ) throws -> Data {
        let command = LiveMediaCommandWire(
            schema_version: 1,
            operation: operation,
            host_instance_id: hostID,
            binding: binding,
            deadline_ms: deadline,
            answer_sdp: answer
        )
        return try JSONEncoder().encode(command)
    }
}

private struct FakeFactory: LiveMediaPeerFactory {
    let peer: FakePeer
    func makeMutedPeer() throws -> any LiveMediaPeer { peer }
}

private final class FakePermissionProvider: MicrophoneAuthorizationProvider, @unchecked Sendable {
    private let lock = NSLock()
    private let initialStatus: MicrophoneAuthorizationStatus
    private let initialGrant: Bool
    private let blocksRequest: Bool
    private let requestStarted = DispatchSemaphore(value: 0)
    private let response = DispatchSemaphore(value: 0)
    private var resolvedGrant = false
    private var requests = 0

    init(
        status: MicrophoneAuthorizationStatus,
        grant: Bool = false,
        blocksRequest: Bool = false
    ) {
        initialStatus = status
        initialGrant = grant
        self.blocksRequest = blocksRequest
    }

    func status() -> MicrophoneAuthorizationStatus { initialStatus }

    var requestCount: Int { lock.withLock { requests } }

    func requestAccess(deadlineMilliseconds: Int64) throws -> Bool {
        lock.withLock { requests += 1 }
        requestStarted.signal()
        if blocksRequest {
            guard response.wait(timeout: .now() + .milliseconds(Int(deadlineMilliseconds))) == .success else {
                throw LiveMediaHostFailure.deadlineExceeded
            }
            return lock.withLock { resolvedGrant }
        }
        return initialGrant
    }

    func waitForRequest() -> DispatchTimeoutResult {
        requestStarted.wait(timeout: .now() + .seconds(2))
    }

    func completeRequest(granted: Bool) {
        lock.withLock { resolvedGrant = granted }
        response.signal()
    }
}

private final class FakePeer: LiveMediaPeer, @unchecked Sendable {
    private let lock = NSLock()
    private let authorization: MicrophoneAuthorizationGate
    private let audioGate = LiveAudioTrackGate()
    private var closed = false
    private let localTrack = TrackState()
    private let remoteTrack = TrackState()
    private(set) var stopCount = 0

    var captureStarted: Bool { localTrack.enabled }
    var remotePlaybackStarted: Bool { remoteTrack.enabled }

    init(authorization: MicrophoneAuthorizationGate) {
        self.authorization = authorization
    }

    func startAudio(deadlineMilliseconds: Int64) throws {
        try authorization.requireAccess(deadlineMilliseconds: deadlineMilliseconds)
        let localTrackInstalled = audioGate.installLocalTrack(
            id: UUID().uuidString,
            setEnabled: { [localTrack] enabled in localTrack.enabled = enabled }
        )
        guard localTrackInstalled, audioGate.activate() else {
            throw LiveMediaHostFailure.conflict
        }
    }

    func createOffer(deadlineMilliseconds: Int64) throws -> String {
        try lock.withLock {
            guard !closed, audioGate.isActive, deadlineMilliseconds > 0 else {
                throw LiveMediaHostFailure.conflict
            }
            return "v=0\r\ns=synthetic-offer\r\n"
        }
    }

    func applyAnswer(_ sdp: String, deadlineMilliseconds: Int64) throws {
        try lock.withLock {
            guard !closed, audioGate.isActive, deadlineMilliseconds > 0, sdp.hasPrefix("v=0\r\n") else {
                throw LiveMediaHostFailure.conflict
            }
        }
    }

    func receiveRemoteAudio(id: String) {
        audioGate.receiveRemoteTrack(id: id) { [remoteTrack] enabled in
            remoteTrack.enabled = enabled
        }
    }

    func fence() {
        audioGate.fence()
    }

    func stop(deadlineMilliseconds: Int64) throws {
        guard deadlineMilliseconds > 0 else { throw LiveMediaHostFailure.deadlineExceeded }
        audioGate.stop()
        lock.withLock {
            guard !closed else { return }
            closed = true
            stopCount += 1
        }
    }

    func observeStop() -> (captureStopped: Bool, playbackStopped: Bool) {
        lock.withLock {
            (closed && !captureStarted, closed && !remotePlaybackStarted)
        }
    }
}

private final class TrackState {
    private let lock = NSLock()
    private var value = false

    var enabled: Bool {
        get { lock.withLock { value } }
        set { lock.withLock { value = newValue } }
    }
}

private final class AsyncDataResult: @unchecked Sendable {
    private let lock = NSLock()
    private let semaphore = DispatchSemaphore(value: 0)
    private var result: Data?

    func set(_ value: Data) {
        lock.withLock { result = value }
        semaphore.signal()
    }

    func wait() -> Bool { semaphore.wait(timeout: .now() + .seconds(2)) == .success }

    var value: Data {
        lock.withLock { result ?? Data() }
    }
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
