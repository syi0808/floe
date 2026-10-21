import Foundation
import LiveKitWebRTC
#if os(iOS)
import AVFAudio
#endif

struct WebRTCMediaPeerFactory: LiveMediaPeerFactory, @unchecked Sendable {
    private let microphoneAuthorization: MicrophoneAuthorizationGate

    init(
        microphoneAuthorization: MicrophoneAuthorizationGate = MicrophoneAuthorizationGate(
            provider: AppleMicrophoneAuthorizationProvider()
        )
    ) {
        self.microphoneAuthorization = microphoneAuthorization
    }

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

        // Preparation creates only a negotiation slot. It does not create a
        // source, attach a sender track, request permission, or start audio.
        let initOptions = LKRTCRtpTransceiverInit()
        initOptions.direction = .sendRecv
        guard let transceiver = connection.addTransceiver(of: .audio, init: initOptions) else {
            connection.close()
            throw LiveMediaHostFailure.capabilityUnavailable
        }

        let peer = WebRTCMediaPeer(
            factory: factory,
            connection: connection,
            audioTransceiver: transceiver,
            microphoneAuthorization: microphoneAuthorization
        )
        connection.delegate = peer
        return peer
    }
}

private final class WebRTCMediaPeer: NSObject, LiveMediaPeer, LKRTCPeerConnectionDelegate, @unchecked Sendable {
    private let factory: LKRTCPeerConnectionFactory
    private let connection: LKRTCPeerConnection
    private let audioTransceiver: LKRTCRtpTransceiver
    private let microphoneAuthorization: MicrophoneAuthorizationGate
    private let audioGate = LiveAudioTrackGate()
    private let lock = NSLock()
    private var localAudioSource: LKRTCAudioSource?
    private var localAudioTrack: LKRTCAudioTrack?

    #if os(iOS)
    private let interruptionLock = NSLock()
    private var interruptionObserver: NSObjectProtocol?
    #endif

    init(
        factory: LKRTCPeerConnectionFactory,
        connection: LKRTCPeerConnection,
        audioTransceiver: LKRTCRtpTransceiver,
        microphoneAuthorization: MicrophoneAuthorizationGate
    ) {
        self.factory = factory
        self.connection = connection
        self.audioTransceiver = audioTransceiver
        self.microphoneAuthorization = microphoneAuthorization
        super.init()
    }

    func startAudio(deadlineMilliseconds: Int64) throws {
        // This is reached only from the fresh, explicit Voice Start path. The
        // permission decision precedes source creation and sender attachment.
        try microphoneAuthorization.requireAccess(deadlineMilliseconds: deadlineMilliseconds)
        guard !audioGate.isTerminal else { throw LiveMediaHostFailure.conflict }

        let source = factory.audioSource(with: nil)
        let trackID = "floe-audio-\(UUID().uuidString.lowercased())"
        let track = factory.audioTrack(with: source, trackId: trackID)
        track.isEnabled = false

        try lock.withLock {
            guard !audioGate.isTerminal else { throw LiveMediaHostFailure.conflict }
            guard audioGate.installLocalTrack(id: trackID, setEnabled: { [weak track] enabled in
                track?.isEnabled = enabled
            }) else {
                throw LiveMediaHostFailure.conflict
            }

            audioTransceiver.sender.track = track
            localAudioSource = source
            localAudioTrack = track
            #if os(iOS)
            installInterruptionObserver()
            #endif

            guard audioGate.activate() else {
                audioTransceiver.sender.track = nil
                track.isEnabled = false
                localAudioTrack = nil
                localAudioSource = nil
                #if os(iOS)
                removeInterruptionObserver()
                #endif
                throw LiveMediaHostFailure.conflict
            }
        }
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

    func fence() {
        // The gate is fenced before touching WebRTC objects, so an in-flight
        // Start or a late receiver callback cannot re-enable either direction.
        audioGate.fence()
        stopAudioDevices()
        #if os(iOS)
        removeInterruptionObserver()
        #endif
        lock.withLock {
            audioTransceiver.sender.track = nil
            localAudioTrack?.isEnabled = false
            localAudioTrack = nil
            localAudioSource = nil
        }
    }

    func stop(deadlineMilliseconds: Int64) throws {
        fence()
        try lock.withLock {
            guard deadlineMilliseconds > 0 else { throw LiveMediaHostFailure.deadlineExceeded }
            connection.close()
            connection.delegate = nil
            stopAudioDevices()
            audioGate.stop()
            guard connection.signalingState == .closed,
                  !factory.audioDeviceModule.isRecording,
                  !factory.audioDeviceModule.isPlaying,
                  audioGate.hasStoppedAudio else {
                throw LiveMediaHostFailure.capabilityUnavailable
            }
        }
    }

    func observeStop() -> (captureStopped: Bool, playbackStopped: Bool) {
        lock.withLock {
            let audio = factory.audioDeviceModule
            let closed = connection.signalingState == .closed
            return (
                captureStopped: closed && !audio.isRecording && audioGate.hasStoppedAudio,
                playbackStopped: closed && !audio.isPlaying && audioGate.hasStoppedAudio
            )
        }
    }

    private func stopAudioDevices() {
        let audio = factory.audioDeviceModule
        if audio.isRecording { _ = audio.stopRecording() }
        if audio.isPlaying { _ = audio.stopPlayout() }
    }

    #if os(iOS)
    private func installInterruptionObserver() {
        interruptionLock.withLock {
            guard interruptionObserver == nil else { return }
            interruptionObserver = NotificationCenter.default.addObserver(
                forName: AVAudioSession.interruptionNotification,
                object: AVAudioSession.sharedInstance(),
                queue: nil
            ) { [weak self] notification in
                guard let value = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? NSNumber,
                      AVAudioSession.InterruptionType(rawValue: value.uintValue) == .began else {
                    return
                }
                // Interruption is a one-way privacy fence. A later system
                // interruption-ended event never resumes a stale Voice lease.
                self?.fence()
            }
        }
    }

    private func removeInterruptionObserver() {
        interruptionLock.withLock {
            guard let interruptionObserver else { return }
            NotificationCenter.default.removeObserver(interruptionObserver)
            self.interruptionObserver = nil
        }
    }
    #endif

    // WebRTC's native audio device module renders enabled remote audio tracks.
    // The delegate controls only the lease-bound enable gate; no raw samples
    // cross this native owner or enter the Flutter event journal.
    func peerConnection(
        _ peerConnection: LKRTCPeerConnection,
        didAdd rtpReceiver: LKRTCRtpReceiver,
        streams mediaStreams: [LKRTCMediaStream]
    ) {
        guard let track = rtpReceiver.track as? LKRTCAudioTrack else { return }
        audioGate.receiveRemoteTrack(id: rtpReceiver.receiverId) { [weak track] enabled in
            track?.isEnabled = enabled
        }
    }

    func peerConnection(_ peerConnection: LKRTCPeerConnection, didRemove rtpReceiver: LKRTCRtpReceiver) {
        audioGate.removeRemoteTrack(id: rtpReceiver.receiverId)
    }

    func peerConnection(_ peerConnection: LKRTCPeerConnection, didChange stateChanged: LKRTCSignalingState) {}
    func peerConnection(_ peerConnection: LKRTCPeerConnection, didAdd stream: LKRTCMediaStream) {}
    func peerConnection(_ peerConnection: LKRTCPeerConnection, didRemove stream: LKRTCMediaStream) {}
    func peerConnectionShouldNegotiate(_ peerConnection: LKRTCPeerConnection) {}
    func peerConnection(_ peerConnection: LKRTCPeerConnection, didChange newState: LKRTCIceConnectionState) {}
    func peerConnection(_ peerConnection: LKRTCPeerConnection, didChange newState: LKRTCIceGatheringState) {}
    func peerConnection(_ peerConnection: LKRTCPeerConnection, didGenerate candidate: LKRTCIceCandidate) {}
    func peerConnection(_ peerConnection: LKRTCPeerConnection, didRemove candidates: [LKRTCIceCandidate]) {}
    func peerConnection(_ peerConnection: LKRTCPeerConnection, didOpen dataChannel: LKRTCDataChannel) {}

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
