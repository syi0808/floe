import Foundation
import LiveKitWebRTC
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

    func testTypedIceConfigurationIsValidatedAndScopedToLeaseCreation() throws {
        let permission = FakePermissionProvider(status: .authorized)
        let peer = FakePeer(authorization: MicrophoneAuthorizationGate(provider: permission))
        let factory = FakeFactory(peer: peer)
        let host = LiveMediaHost(factory: factory)
        let binding = makeBinding()
        let hostID = UUID().uuidString.lowercased()
        let iceServer = LiveMediaIceServerWire(
            urls: ["turn:turn.example.test:3478?transport=udp"],
            username: "fixture-turn-user",
            credential: "fixture-ephemeral-secret"
        )

        XCTAssertEqual(
            try invoke(host, "prepare_muted", hostID, binding, ice_servers: [iceServer]).status,
            "ok"
        )
        XCTAssertEqual(factory.receivedIceServers, [iceServer])
        XCTAssertFalse(peer.captureStarted)
        XCTAssertEqual(permission.requestCount, 0)
        let response = String(decoding: host.invoke(try commandData("validate", hostID, binding)), as: UTF8.self)
        XCTAssertFalse(response.contains("fixture-ephemeral-secret"))

        let rejectedFactory = FakeFactory(peer: FakePeer(
            authorization: MicrophoneAuthorizationGate(provider: FakePermissionProvider(status: .authorized))
        ))
        let rejectedHost = LiveMediaHost(factory: rejectedFactory)
        let rejected = try invoke(
            rejectedHost,
            "prepare_muted",
            UUID().uuidString.lowercased(),
            makeBinding(),
            ice_servers: [LiveMediaIceServerWire(
                urls: ["https://ice.example.test"],
                username: "fixture-turn-user",
                credential: "fixture-ephemeral-secret"
            )]
        )
        XCTAssertEqual(rejected.failure, LiveMediaHostFailure.invalidInput.rawValue)
        XCTAssertEqual(rejectedFactory.makeCount, 0)
        XCTAssertFalse(String(decoding: try JSONEncoder().encode(rejected), as: UTF8.self).contains("fixture-ephemeral-secret"))
    }

    func testWebRTCConfigurationReceivesOnlyExplicitICEServers() {
        let empty = WebRTCMediaPeerFactory.makeRTCConfiguration(iceServers: [])
        XCTAssertTrue(empty.iceServers.isEmpty)

        let input = LiveMediaIceServerWire(
            urls: ["turns:turn.example.test:5349?transport=tcp"],
            username: "fixture-user",
            credential: "fixture-secret"
        )
        XCTAssertFalse(String(reflecting: input).contains("fixture-user"))
        XCTAssertFalse(String(reflecting: input).contains("fixture-secret"))
        let configuration = WebRTCMediaPeerFactory.makeRTCConfiguration(iceServers: [input])
        XCTAssertEqual(configuration.iceServers.count, 1)
        XCTAssertEqual(configuration.iceServers[0].urlStrings, input.urls)
        XCTAssertEqual(configuration.iceServers[0].username, input.username)
        XCTAssertEqual(configuration.iceServers[0].credential, input.credential)
    }

    func testLateGatheredOfferIsDiscardedAfterLeaseFence() throws {
        let peer = BlockingOfferPeer()
        let host = LiveMediaHost(factory: FakeFactory(peer: peer))
        let binding = makeBinding()
        let hostID = UUID().uuidString.lowercased()
        XCTAssertEqual(try invoke(host, "prepare_muted", hostID, binding).status, "ok")
        XCTAssertEqual(try invoke(host, "start_audio", hostID, binding, deadline: 1_000).status, "ok")

        let pendingOffer = try commandData("offer", hostID, binding, deadline: 5_000)
        let result = AsyncDataResult()
        DispatchQueue.global().async { result.set(host.invoke(pendingOffer)) }
        XCTAssertEqual(peer.waitForOffer(), .success)
        XCTAssertEqual(try invoke(host, "fence", hostID, binding).status, "ok")
        XCTAssertEqual(try invoke(host, "stop_once", hostID, binding, deadline: 1_000).status, "ok")
        peer.releaseOffer()

        XCTAssertTrue(result.wait())
        let lateOffer = try JSONDecoder().decode(LiveMediaReplyWire.self, from: result.value)
        XCTAssertEqual(lateOffer.failure, LiveMediaHostFailure.conflict.rawValue)
        XCTAssertNil(lateOffer.offer_sdp)
        XCTAssertEqual(
            try invoke(host, "offer", hostID, binding, deadline: 1_000).failure,
            LiveMediaHostFailure.conflict.rawValue
        )
        XCTAssertEqual(try invoke(host, "observe_stop", hostID, binding).capture_stopped, true)
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
        answer: String? = nil,
        ice_servers: [LiveMediaIceServerWire]? = nil
    ) throws -> LiveMediaReplyWire {
        try JSONDecoder().decode(
            LiveMediaReplyWire.self,
            from: host.invoke(commandData(
                operation, hostID, binding, deadline: deadline, answer: answer, ice_servers: ice_servers
            ))
        )
    }

    private func commandData(
        _ operation: String,
        _ hostID: String,
        _ binding: LiveMediaBindingWire,
        deadline: Int64? = nil,
        answer: String? = nil,
        ice_servers: [LiveMediaIceServerWire]? = nil
    ) throws -> Data {
        let command = LiveMediaCommandWire(
            schema_version: 1,
            operation: operation,
            host_instance_id: hostID,
            binding: binding,
            deadline_ms: deadline,
            answer_sdp: answer,
            ice_servers: ice_servers ?? (operation == "prepare_muted" ? [] : nil)
        )
        return try JSONEncoder().encode(command)
    }
}

private final class FakeFactory: LiveMediaPeerFactory, @unchecked Sendable {
    let peer: any LiveMediaPeer
    private let lock = NSLock()
    private var _receivedIceServers: [LiveMediaIceServerWire] = []
    private var _makeCount = 0

    init(peer: any LiveMediaPeer) {
        self.peer = peer
    }

    var receivedIceServers: [LiveMediaIceServerWire] { lock.withLock { _receivedIceServers } }
    var makeCount: Int { lock.withLock { _makeCount } }

    func makeMutedPeer(iceServers: [LiveMediaIceServerWire]) throws -> any LiveMediaPeer {
        lock.withLock {
            _makeCount += 1
            _receivedIceServers = iceServers
        }
        return peer
    }
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

private final class BlockingOfferPeer: LiveMediaPeer, @unchecked Sendable {
    private let lock = NSLock()
    private let offerEntered = DispatchSemaphore(value: 0)
    private let releaseOfferGate = DispatchSemaphore(value: 0)
    private var active = false
    private var terminal = false
    private var stopped = false

    func startAudio(deadlineMilliseconds: Int64) throws {
        try lock.withLock {
            guard deadlineMilliseconds > 0, !terminal else { throw LiveMediaHostFailure.conflict }
            active = true
        }
    }

    func createOffer(deadlineMilliseconds: Int64) throws -> String {
        guard lock.withLock({ active && !terminal }) else { throw LiveMediaHostFailure.conflict }
        offerEntered.signal()
        guard releaseOfferGate.wait(timeout: .now() + .milliseconds(Int(deadlineMilliseconds))) == .success else {
            throw LiveMediaHostFailure.deadlineExceeded
        }
        return "v=0\r\ns=late-synthetic-offer\r\na=end-of-candidates\r\n"
    }

    func applyAnswer(_ sdp: String, deadlineMilliseconds: Int64) throws {
        guard lock.withLock({ active && !terminal && deadlineMilliseconds > 0 }),
              sdp.hasPrefix("v=0\r\n") else { throw LiveMediaHostFailure.conflict }
    }

    func fence() { lock.withLock { terminal = true; active = false } }

    func stop(deadlineMilliseconds: Int64) throws {
        guard deadlineMilliseconds > 0 else { throw LiveMediaHostFailure.deadlineExceeded }
        fence()
        lock.withLock { stopped = true }
    }

    func observeStop() -> (captureStopped: Bool, playbackStopped: Bool) {
        lock.withLock { (stopped && !active, stopped && !active) }
    }

    func waitForOffer() -> DispatchTimeoutResult {
        offerEntered.wait(timeout: .now() + .seconds(2))
    }

    func releaseOffer() { releaseOfferGate.signal() }
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
