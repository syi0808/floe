import Foundation
import LiveKitWebRTC

struct WebRTCMediaPeerFactory: LiveMediaPeerFactory, @unchecked Sendable {
    func makeMutedPeer() throws -> any LiveMediaPeer {
        guard LKRTCInitializeSSL() else { throw LiveMediaHostFailure.capabilityUnavailable }
        let factory = LKRTCPeerConnectionFactory()
        let configuration = LKRTCConfiguration()
        configuration.sdpSemantics = .unifiedPlan
        let constraints = LKRTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        guard let connection = factory.peerConnection(
            with: configuration,
            constraints: constraints,
            delegate: nil
        ) else { throw LiveMediaHostFailure.capabilityUnavailable }

        // A send/receive m-line lets the existing Gateway offer/answer path
        // negotiate audio, while the sender has no local track. No capture
        // source, permission request or microphone start occurs in this lease.
        let transceiver = LKRTCRtpTransceiverInit()
        transceiver.direction = .sendRecv
        guard connection.addTransceiver(of: .audio, init: transceiver) != nil else {
            connection.close()
            throw LiveMediaHostFailure.capabilityUnavailable
        }
        return WebRTCMediaPeer(factory: factory, connection: connection)
    }
}

private final class WebRTCMediaPeer: LiveMediaPeer, @unchecked Sendable {
    private let factory: LKRTCPeerConnectionFactory
    private let connection: LKRTCPeerConnection
    private let lock = NSLock()

    init(factory: LKRTCPeerConnectionFactory, connection: LKRTCPeerConnection) {
        self.factory = factory
        self.connection = connection
    }

    func createOffer(deadlineMilliseconds: Int64) throws -> String {
        try lock.withLock {
            let deadline = Date().addingTimeInterval(TimeInterval(deadlineMilliseconds) / 1_000)
            let constraints = LKRTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
            let offer: LKRTCSessionDescription = try waitForValue(deadline: deadline) { completion in
                connection.offer(for: constraints, completionHandler: completion)
            }
            try waitForSuccess(deadline: deadline) { completion in
                connection.setLocalDescription(offer, completionHandler: completion)
            }
            while connection.iceGatheringState != .complete {
                guard Date() < deadline else { throw LiveMediaHostFailure.deadlineExceeded }
                Thread.sleep(forTimeInterval: 0.01)
            }
            guard let local = connection.localDescription else {
                throw LiveMediaHostFailure.capabilityUnavailable
            }
            return local.sdp
        }
    }

    func applyAnswer(_ sdp: String, deadlineMilliseconds: Int64) throws {
        try lock.withLock {
            let deadline = Date().addingTimeInterval(TimeInterval(deadlineMilliseconds) / 1_000)
            let answer = LKRTCSessionDescription(type: .answer, sdp: sdp)
            try waitForSuccess(deadline: deadline) { completion in
                connection.setRemoteDescription(answer, completionHandler: completion)
            }
        }
    }

    func stop(deadlineMilliseconds: Int64) throws {
        try lock.withLock {
            guard deadlineMilliseconds > 0 else { throw LiveMediaHostFailure.deadlineExceeded }
            connection.close()
            let audio = factory.audioDeviceModule
            if audio.isRecording, audio.stopRecording() != 0 {
                throw LiveMediaHostFailure.capabilityUnavailable
            }
            if audio.isPlaying, audio.stopPlayout() != 0 {
                throw LiveMediaHostFailure.capabilityUnavailable
            }
            guard connection.signalingState == .closed,
                  !audio.isRecording,
                  !audio.isPlaying else {
                throw LiveMediaHostFailure.capabilityUnavailable
            }
        }
    }

    func observeStop() -> (captureStopped: Bool, playbackStopped: Bool) {
        lock.withLock {
            let audio = factory.audioDeviceModule
            let closed = connection.signalingState == .closed
            return (captureStopped: closed && !audio.isRecording,
                    playbackStopped: closed && !audio.isPlaying)
        }
    }

    private func waitForValue<T>(
        deadline: Date,
        start: (@escaping @Sendable (T?, Error?) -> Void) -> Void
    ) throws -> T {
        let semaphore = DispatchSemaphore(value: 0)
        let state = CallbackState<T>()
        start { value, error in
            state.resolve(value: value, error: error)
            semaphore.signal()
        }
        let remaining = max(0, deadline.timeIntervalSinceNow)
        guard semaphore.wait(timeout: .now() + remaining) == .success else {
            throw LiveMediaHostFailure.deadlineExceeded
        }
        guard let result = state.snapshot() else {
            throw LiveMediaHostFailure.capabilityUnavailable
        }
        return try result.get()
    }

    private func waitForSuccess(
        deadline: Date,
        start: (@escaping @Sendable (Error?) -> Void) -> Void
    ) throws {
        let semaphore = DispatchSemaphore(value: 0)
        let state = CallbackState<Void>()
        start { error in
            state.resolve(value: error == nil ? () : nil, error: error)
            semaphore.signal()
        }
        let remaining = max(0, deadline.timeIntervalSinceNow)
        guard semaphore.wait(timeout: .now() + remaining) == .success else {
            throw LiveMediaHostFailure.deadlineExceeded
        }
        guard let result = state.snapshot() else {
            throw LiveMediaHostFailure.capabilityUnavailable
        }
        _ = try result.get()
    }
}

private final class CallbackState<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var result: Result<Value, Error>?

    func resolve(value: Value?, error: Error?) {
        lock.withLock {
            if let error {
                result = .failure(error)
            } else if let value {
                result = .success(value)
            } else {
                result = .failure(LiveMediaHostFailure.capabilityUnavailable)
            }
        }
    }

    func snapshot() -> Result<Value, Error>? {
        lock.withLock { result }
    }
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
