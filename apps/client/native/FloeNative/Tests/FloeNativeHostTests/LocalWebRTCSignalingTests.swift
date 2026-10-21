import AudioToolbox
import Darwin
import Foundation
import LiveKitWebRTC
import XCTest

@testable import FloeNativeHost

/// Verifies local ICE negotiation using an injected memory-only device. Both
/// peers receive only private/link-local host candidates assigned to this
/// machine. No PCM is driven, and no provider, STUN/TURN service, microphone,
/// or physical speaker is used.
final class LocalWebRTCSignalingTests: XCTestCase {
    func testLocalHostCandidatesNegotiateWithMemoryOnlyAudioDevices() throws {
        guard LKRTCInitializeSSL() else { throw LocalWebRTCFailure.stackUnavailable }

        let leftAudio = MemoryOnlyRTCAudioDevice()
        let rightAudio = MemoryOnlyRTCAudioDevice()
        let leftFactory = LKRTCPeerConnectionFactory(
            encoderFactory: nil,
            decoderFactory: nil,
            audioDevice: leftAudio
        )
        let rightFactory = LKRTCPeerConnectionFactory(
            encoderFactory: nil,
            decoderFactory: nil,
            audioDevice: rightAudio
        )
        let constraints = LKRTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        guard
            let left = leftFactory.peerConnection(
                with: localOnlyConfiguration(),
                constraints: constraints,
                delegate: nil
            ),
            let right = rightFactory.peerConnection(
                with: localOnlyConfiguration(),
                constraints: constraints,
                delegate: nil
            )
        else {
            throw LocalWebRTCFailure.peerUnavailable
        }
        defer {
            left.close()
            right.close()
            left.delegate = nil
            right.delegate = nil
        }

        let leftTransceiver = LKRTCRtpTransceiverInit()
        leftTransceiver.direction = .sendRecv
        let rightTransceiver = LKRTCRtpTransceiverInit()
        rightTransceiver.direction = .sendRecv
        guard left.addTransceiver(of: .audio, init: leftTransceiver) != nil,
            right.addTransceiver(of: .audio, init: rightTransceiver) != nil
        else {
            throw LocalWebRTCFailure.transceiverUnavailable
        }

        let offer = try makeLocalDescription(on: left, type: .offer, constraints: constraints)
        try setRemoteDescription(offer, on: right)
        let answer = try makeLocalDescription(on: right, type: .answer, constraints: constraints)
        try setRemoteDescription(answer, on: left)

        XCTAssertTrue(
            waitUntil(timeout: 10) {
                isConnected(left.iceConnectionState) && isConnected(right.iceConnectionState)
            })
    }

    func testSyntheticPCMTraversesLocalPeerAndMemoryDeviceStops() throws {
        guard LKRTCInitializeSSL() else { throw LocalWebRTCFailure.stackUnavailable }

        let leftAudio = MemoryOnlyRTCAudioDevice()
        let rightAudio = MemoryOnlyRTCAudioDevice()
        let leftFactory = LKRTCPeerConnectionFactory(
            encoderFactory: nil,
            decoderFactory: nil,
            audioDevice: leftAudio
        )
        let rightFactory = LKRTCPeerConnectionFactory(
            encoderFactory: nil,
            decoderFactory: nil,
            audioDevice: rightAudio
        )
        let constraints = LKRTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        guard
            let left = leftFactory.peerConnection(
                with: localOnlyConfiguration(),
                constraints: constraints,
                delegate: nil
            ),
            let right = rightFactory.peerConnection(
                with: localOnlyConfiguration(),
                constraints: constraints,
                delegate: nil
            )
        else {
            throw LocalWebRTCFailure.peerUnavailable
        }
        defer {
            left.close()
            right.close()
            left.delegate = nil
            right.delegate = nil
            _ = leftFactory.audioDeviceModule.stopRecording()
            _ = leftFactory.audioDeviceModule.stopPlayout()
            _ = rightFactory.audioDeviceModule.stopRecording()
            _ = rightFactory.audioDeviceModule.stopPlayout()
        }

        let leftInit = LKRTCRtpTransceiverInit()
        leftInit.direction = .sendOnly
        let rightInit = LKRTCRtpTransceiverInit()
        rightInit.direction = .recvOnly
        guard
            let leftTransceiver = left.addTransceiver(of: .audio, init: leftInit),
            right.addTransceiver(of: .audio, init: rightInit) != nil
        else {
            throw LocalWebRTCFailure.transceiverUnavailable
        }

        let source = leftFactory.audioSource(with: nil)
        let track = leftFactory.audioTrack(with: source, trackId: "synthetic-local-pcm")
        track.isEnabled = true
        leftTransceiver.sender.track = track

        let offer = try makeLocalDescription(on: left, type: .offer, constraints: constraints)
        try setRemoteDescription(offer, on: right)
        let answer = try makeLocalDescription(on: right, type: .answer, constraints: constraints)
        try setRemoteDescription(answer, on: left)
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                isConnected(left.iceConnectionState) && isConnected(right.iceConnectionState)
            })

        let leftAudioModule = leftFactory.audioDeviceModule
        let rightAudioModule = rightFactory.audioDeviceModule
        if !leftAudioModule.isRecording {
            let result =
                leftAudioModule.isRecordingInitialized
                ? leftAudioModule.startRecording()
                : leftAudioModule.initAndStartRecording(audioProcessingOptions: nil)
            XCTAssertEqual(result, 0, "the injected memory-only capture device must start")
        }
        if !rightAudioModule.isPlayoutInitialized {
            XCTAssertEqual(rightAudioModule.initPlayout(), 0)
        }
        if !rightAudioModule.isPlaying {
            XCTAssertEqual(rightAudioModule.startPlayout(), 0)
        }
        XCTAssertTrue(leftAudio.isRecording)
        XCTAssertTrue(rightAudio.isPlaying)

        for _ in 0..<200 {
            XCTAssertTrue(leftAudio.deliverSyntheticPCM(frameCount: 480))
            _ = rightAudio.capturePlayoutPCM(frameCount: 480)
            if rightAudio.nonzeroPlayoutSampleCount > 64 { break }
            Thread.sleep(forTimeInterval: 0.01)
        }
        XCTAssertGreaterThan(leftAudio.syntheticFrameCount, 0)
        XCTAssertGreaterThan(rightAudio.playoutCallbackCount, 0)
        XCTAssertGreaterThan(
            rightAudio.nonzeroPlayoutSampleCount,
            64,
            "decoded remote PCM should reach the memory-only playout sink"
        )

        track.isEnabled = false
        leftTransceiver.sender.track = nil
        left.close()
        right.close()
        XCTAssertEqual(leftAudioModule.stopRecording(), 0)
        XCTAssertEqual(rightAudioModule.stopPlayout(), 0)
        XCTAssertEqual(left.signalingState, .closed)
        XCTAssertEqual(right.signalingState, .closed)
        XCTAssertFalse(leftAudio.isRecording)
        XCTAssertFalse(rightAudio.isPlaying)
        XCTAssertFalse(leftAudio.deliverSyntheticPCM(frameCount: 480), "Stop must reject later PCM")
        XCTAssertFalse(
            rightAudio.capturePlayoutPCM(frameCount: 480), "Stop must reject later playout")
    }

    func testStopFenceWinsAgainstBlockedSyntheticAudioCallback() throws {
        guard LKRTCInitializeSSL() else { throw LocalWebRTCFailure.stackUnavailable }

        let leftAudio = MemoryOnlyRTCAudioDevice()
        let rightAudio = MemoryOnlyRTCAudioDevice()
        let leftFactory = LKRTCPeerConnectionFactory(
            encoderFactory: nil,
            decoderFactory: nil,
            audioDevice: leftAudio
        )
        let rightFactory = LKRTCPeerConnectionFactory(
            encoderFactory: nil,
            decoderFactory: nil,
            audioDevice: rightAudio
        )
        let constraints = LKRTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        guard
            let left = leftFactory.peerConnection(
                with: localOnlyConfiguration(),
                constraints: constraints,
                delegate: nil
            ),
            let right = rightFactory.peerConnection(
                with: localOnlyConfiguration(),
                constraints: constraints,
                delegate: nil
            )
        else {
            throw LocalWebRTCFailure.peerUnavailable
        }
        defer {
            left.close()
            right.close()
            left.delegate = nil
            right.delegate = nil
            _ = leftFactory.audioDeviceModule.stopRecording()
            _ = leftFactory.audioDeviceModule.stopPlayout()
            _ = rightFactory.audioDeviceModule.stopRecording()
            _ = rightFactory.audioDeviceModule.stopPlayout()
        }

        let leftInit = LKRTCRtpTransceiverInit()
        leftInit.direction = .sendOnly
        let rightInit = LKRTCRtpTransceiverInit()
        rightInit.direction = .recvOnly
        guard
            let leftTransceiver = left.addTransceiver(of: .audio, init: leftInit),
            right.addTransceiver(of: .audio, init: rightInit) != nil
        else {
            throw LocalWebRTCFailure.transceiverUnavailable
        }

        let source = leftFactory.audioSource(with: nil)
        let track = leftFactory.audioTrack(with: source, trackId: "stop-race-local-pcm")
        track.isEnabled = false
        let leaseGate = LiveAudioTrackGate()
        XCTAssertTrue(
            leaseGate.installLocalTrack(id: "stop-race-local-pcm") { [weak track] enabled in
                track?.isEnabled = enabled
            })
        leftTransceiver.sender.track = track
        XCTAssertTrue(leaseGate.activate())

        let offer = try makeLocalDescription(on: left, type: .offer, constraints: constraints)
        try setRemoteDescription(offer, on: right)
        let answer = try makeLocalDescription(on: right, type: .answer, constraints: constraints)
        try setRemoteDescription(answer, on: left)
        XCTAssertTrue(
            waitUntil(timeout: 10) {
                isConnected(left.iceConnectionState) && isConnected(right.iceConnectionState)
            })

        let leftAudioModule = leftFactory.audioDeviceModule
        let rightAudioModule = rightFactory.audioDeviceModule
        if !leftAudioModule.isRecording {
            let result =
                leftAudioModule.isRecordingInitialized
                ? leftAudioModule.startRecording()
                : leftAudioModule.initAndStartRecording(audioProcessingOptions: nil)
            XCTAssertEqual(result, 0)
        }
        if !rightAudioModule.isPlayoutInitialized {
            XCTAssertEqual(rightAudioModule.initPlayout(), 0)
        }
        if !rightAudioModule.isPlaying {
            XCTAssertEqual(rightAudioModule.startPlayout(), 0)
        }
        XCTAssertTrue(leftAudio.isRecording)
        XCTAssertTrue(rightAudio.isPlaying)

        let renderBarrier = RenderCallbackBarrier()
        defer { renderBarrier.release() }
        let deliveryResult = AsyncResult<Bool>()
        DispatchQueue.global().async {
            deliveryResult.resolve(
                .success(
                    leftAudio.deliverSyntheticPCM(frameCount: 480, renderBarrier: renderBarrier))
            )
        }
        XCTAssertTrue(
            renderBarrier.waitUntilEntered(timeout: 5),
            "PCM must be held inside WebRTC's render callback")

        leaseGate.fence()
        XCTAssertFalse(track.isEnabled)
        leftTransceiver.sender.track = nil

        let stopInvocationStarted = DispatchSemaphore(value: 0)
        let stopResult = AsyncResult<Int>()
        let audioModuleStopper = AudioModuleStopper(module: leftAudioModule)
        DispatchQueue.global().async {
            stopInvocationStarted.signal()
            stopResult.resolve(.success(audioModuleStopper.stopRecording()))
        }
        XCTAssertEqual(stopInvocationStarted.wait(timeout: .now() + .seconds(5)), .success)
        XCTAssertTrue(leftAudio.stopRecording(), "the memory device must fence input during Stop")

        renderBarrier.release()
        let accepted = try deliveryResult.wait(timeout: 5)
        XCTAssertFalse(accepted, "PCM held before delivery must be rejected after the Stop fence")
        XCTAssertEqual(try stopResult.wait(timeout: 5), 0)
        XCTAssertEqual(leftAudio.syntheticFrameCount, 0)
        XCTAssertFalse(leftAudio.isRecording)
        XCTAssertFalse(
            leftAudio.deliverSyntheticPCM(frameCount: 480), "new PCM is rejected after Stop")
        XCTAssertNil(leftTransceiver.sender.track)
        XCTAssertTrue(leaseGate.isTerminal)
        XCTAssertFalse(leaseGate.activate(), "a fenced lease cannot be revived")

        var lateTrackEnabled = true
        leaseGate.receiveRemoteTrack(id: "late-stop-race-receiver") { lateTrackEnabled = $0 }
        XCTAssertFalse(lateTrackEnabled, "late receiver callbacks stay disabled on the old lease")
        XCTAssertTrue(rightAudio.capturePlayoutPCM(frameCount: 480))
        XCTAssertEqual(rightAudio.nonzeroPlayoutSampleCount, 0)
    }
}

/// Test-only device: all input samples are synthesized in memory and all output
/// samples end in a memory buffer. It owns no AudioUnit or AVAudioSession.
private final class MemoryOnlyRTCAudioDevice: NSObject, LKRTCAudioDevice, @unchecked Sendable {
    private let lock = NSLock()
    private var audioDelegate: LKRTCAudioDeviceDelegate?
    private var initialized = false
    private var playoutInitialized = false
    private var recordingInitialized = false
    private var playing = false
    private var recording = false
    private var captureSampleCursor = 0
    private var playoutSampleCursor = 0
    private var recordedFrameTotal = 0
    private var playoutCallbackTotal = 0
    private var nonzeroPlayoutSampleTotal = 0

    var deviceInputSampleRate: Double { 48_000 }
    var inputIOBufferDuration: TimeInterval { 0.01 }
    var inputNumberOfChannels: Int { 2 }
    var inputLatency: TimeInterval { 0 }
    var deviceOutputSampleRate: Double { 48_000 }
    var outputIOBufferDuration: TimeInterval { 0.01 }
    var outputNumberOfChannels: Int { 2 }
    var outputLatency: TimeInterval { 0 }
    var isInitialized: Bool { lock.withLock { initialized } }
    var isPlayoutInitialized: Bool { lock.withLock { playoutInitialized } }
    var isRecordingInitialized: Bool { lock.withLock { recordingInitialized } }
    var isPlaying: Bool { lock.withLock { playing } }
    var isRecording: Bool { lock.withLock { recording } }
    var syntheticFrameCount: Int { lock.withLock { recordedFrameTotal } }
    var playoutCallbackCount: Int { lock.withLock { playoutCallbackTotal } }
    var nonzeroPlayoutSampleCount: Int { lock.withLock { nonzeroPlayoutSampleTotal } }

    func initialize(with delegate: LKRTCAudioDeviceDelegate) -> Bool {
        lock.withLock {
            audioDelegate = delegate
            initialized = true
            return true
        }
    }

    func terminateDevice() -> Bool {
        lock.withLock {
            audioDelegate = nil
            initialized = false
            recordingInitialized = false
            playoutInitialized = false
            recording = false
            playing = false
            return true
        }
    }

    func initializePlayout() -> Bool {
        lock.withLock {
            playoutInitialized = true
            return true
        }
    }
    func startPlayout() -> Bool {
        lock.withLock {
            playing = true
            return true
        }
    }
    func stopPlayout() -> Bool {
        lock.withLock {
            playing = false
            return true
        }
    }
    func initializeRecording() -> Bool {
        lock.withLock {
            recordingInitialized = true
            return true
        }
    }
    func startRecording() -> Bool {
        lock.withLock {
            recording = true
            return true
        }
    }
    func stopRecording() -> Bool {
        lock.withLock {
            recording = false
            return true
        }
    }

    func deliverSyntheticPCM(frameCount: Int, renderBarrier: RenderCallbackBarrier? = nil) -> Bool {
        guard frameCount > 0 else { return false }
        guard
            let (delegate, sampleStart) = lock.withLock({ () -> (LKRTCAudioDeviceDelegate, Int)? in
                guard recording, let audioDelegate else { return nil }
                let start = captureSampleCursor
                captureSampleCursor += frameCount
                return (audioDelegate, start)
            })
        else {
            return false
        }

        var samples = [Int16](repeating: 0, count: frameCount * inputNumberOfChannels)
        for frame in 0..<frameCount {
            let phase =
                2.0 * Double.pi * 440.0 * Double(sampleStart + frame) / deviceInputSampleRate
            let sample = Int16(sin(phase) * 8_000)
            let offset = frame * inputNumberOfChannels
            samples[offset] = sample
            samples[offset + 1] = sample
        }
        let pcmData = samples.withUnsafeBytes { Data($0) }

        return samples.withUnsafeMutableBufferPointer { sampleBuffer in
            guard let baseAddress = sampleBuffer.baseAddress else { return false }
            let audioBuffer = AudioBuffer(
                mNumberChannels: UInt32(inputNumberOfChannels),
                mDataByteSize: UInt32(sampleBuffer.count * MemoryLayout<Int16>.size),
                mData: UnsafeMutableRawPointer(baseAddress)
            )
            var audioBufferList = AudioBufferList(mNumberBuffers: 1, mBuffers: audioBuffer)
            var actionFlags: AudioUnitRenderActionFlags = []
            var timestamp = AudioTimeStamp()
            timestamp.mSampleTime = Double(sampleStart)
            timestamp.mFlags = .sampleTimeValid
            let status: OSStatus
            if let renderBarrier {
                status = delegate.deliverRecordedData(
                    &actionFlags,
                    &timestamp,
                    0,
                    UInt32(frameCount),
                    nil,
                    nil
                ) { _, _, _, renderedFrameCount, inputData, _ in
                    guard renderBarrier.holdUntilReleased(timeout: 5), self.isRecording,
                        let destination = inputData.pointee.mBuffers.mData
                    else {
                        return -1
                    }
                    let byteCount = min(
                        Int(inputData.pointee.mBuffers.mDataByteSize),
                        Int(renderedFrameCount) * self.inputNumberOfChannels
                            * MemoryLayout<Int16>.size,
                        pcmData.count
                    )
                    pcmData.withUnsafeBytes { source in
                        guard let sourceAddress = source.baseAddress else { return }
                        memcpy(destination, sourceAddress, byteCount)
                    }
                    return noErr
                }
            } else {
                status = withUnsafePointer(to: &audioBufferList) { bufferList in
                    delegate.deliverRecordedData(
                        &actionFlags,
                        &timestamp,
                        0,
                        UInt32(frameCount),
                        bufferList,
                        nil,
                        nil
                    )
                }
            }
            if status == noErr {
                return lock.withLock {
                    guard recording else { return false }
                    recordedFrameTotal += frameCount
                    return true
                }
            }
            return false
        }
    }

    func capturePlayoutPCM(frameCount: Int) -> Bool {
        guard frameCount > 0 else { return false }
        guard let delegate = lock.withLock({ playing ? audioDelegate : nil }) else { return false }

        var samples = [Int16](repeating: 0, count: frameCount * outputNumberOfChannels)
        let status = samples.withUnsafeMutableBufferPointer { sampleBuffer -> OSStatus in
            guard let baseAddress = sampleBuffer.baseAddress else { return -1 }
            let audioBuffer = AudioBuffer(
                mNumberChannels: UInt32(outputNumberOfChannels),
                mDataByteSize: UInt32(sampleBuffer.count * MemoryLayout<Int16>.size),
                mData: UnsafeMutableRawPointer(baseAddress)
            )
            var audioBufferList = AudioBufferList(mNumberBuffers: 1, mBuffers: audioBuffer)
            var actionFlags: AudioUnitRenderActionFlags = []
            var timestamp = AudioTimeStamp()
            timestamp.mSampleTime = Double(playoutSampleCursor)
            timestamp.mFlags = .sampleTimeValid
            return withUnsafeMutablePointer(to: &audioBufferList) { bufferList in
                delegate.getPlayoutData(
                    &actionFlags,
                    &timestamp,
                    0,
                    UInt32(frameCount),
                    bufferList
                )
            }
        }
        guard status == noErr else { return false }

        let nonzero = samples.reduce(into: 0) { count, sample in
            if sample != 0 { count += 1 }
        }
        lock.withLock {
            guard playing else { return }
            playoutSampleCursor += frameCount
            playoutCallbackTotal += 1
            nonzeroPlayoutSampleTotal += nonzero
        }
        return true
    }
}

private final class RenderCallbackBarrier: @unchecked Sendable {
    private let entered = DispatchSemaphore(value: 0)
    private let releaseSignal = DispatchSemaphore(value: 0)

    func waitUntilEntered(timeout: TimeInterval) -> Bool {
        entered.wait(timeout: .now() + timeout) == .success
    }

    func holdUntilReleased(timeout: TimeInterval) -> Bool {
        entered.signal()
        return releaseSignal.wait(timeout: .now() + timeout) == .success
    }

    func release() {
        releaseSignal.signal()
    }
}

private final class AudioModuleStopper: @unchecked Sendable {
    private let module: LKRTCAudioDeviceModule

    init(module: LKRTCAudioDeviceModule) {
        self.module = module
    }

    func stopRecording() -> Int {
        module.stopRecording()
    }
}

private enum LocalWebRTCFailure: Error {
    case stackUnavailable
    case peerUnavailable
    case transceiverUnavailable
    case descriptionUnavailable
    case noPrivateHostCandidate
    case timeout
}

private func localOnlyConfiguration() -> LKRTCConfiguration {
    let configuration = LKRTCConfiguration()
    configuration.sdpSemantics = .unifiedPlan
    configuration.iceServers = []
    configuration.iceTransportPolicy = .all
    configuration.tcpCandidatePolicy = .disabled
    return configuration
}

private func makeLocalDescription(
    on connection: LKRTCPeerConnection,
    type: LKRTCSdpType,
    constraints: LKRTCMediaConstraints
) throws -> LKRTCSessionDescription {
    let description: LKRTCSessionDescription
    if type == .offer {
        description = try awaitDescription { completion in
            connection.offer(for: constraints, completionHandler: completion)
        }
    } else {
        description = try awaitDescription { completion in
            connection.answer(for: constraints, completionHandler: completion)
        }
    }
    try awaitCompletion { completion in
        connection.setLocalDescription(description, completionHandler: completion)
    }
    guard waitUntil(timeout: 10, condition: { connection.iceGatheringState == .complete }),
        let local = connection.localDescription
    else {
        throw LocalWebRTCFailure.timeout
    }
    return try privateHostCandidatesOnly(local)
}

private func setRemoteDescription(
    _ description: LKRTCSessionDescription, on connection: LKRTCPeerConnection
) throws {
    try awaitCompletion { completion in
        connection.setRemoteDescription(description, completionHandler: completion)
    }
}

private func privateHostCandidatesOnly(_ description: LKRTCSessionDescription) throws
    -> LKRTCSessionDescription
{
    let lines = description.sdp.components(separatedBy: .newlines)
    let candidates = lines.filter { $0.hasPrefix("a=candidate:") }
    let localAddresses = localPrivateInterfaceAddresses()
    let allowed = candidates.filter { candidate in
        let fields = candidate.split(whereSeparator: \Character.isWhitespace)
        guard fields.count > 7, fields[6] == "typ", fields[7] == "host" else { return false }
        return localAddresses.contains(
            fields[4].trimmingCharacters(in: CharacterSet(charactersIn: "[]")))
    }
    guard !allowed.isEmpty else { throw LocalWebRTCFailure.noPrivateHostCandidate }
    if allowed.count == candidates.count { return description }

    let allowedSet = Set(allowed)
    let sdp = lines.filter { !$0.hasPrefix("a=candidate:") || allowedSet.contains($0) }
        .joined(separator: "\r\n")
    return LKRTCSessionDescription(type: description.type, sdp: sdp)
}

private func localPrivateInterfaceAddresses() -> Set<String> {
    var head: UnsafeMutablePointer<ifaddrs>?
    guard getifaddrs(&head) == 0, let first = head else { return [] }
    defer { freeifaddrs(head) }

    var result: Set<String> = []
    var current: UnsafeMutablePointer<ifaddrs>? = first
    while let entry = current?.pointee {
        defer { current = entry.ifa_next }
        guard let socketAddress = entry.ifa_addr else { continue }
        var buffer = [CChar](repeating: 0, count: Int(INET6_ADDRSTRLEN))
        let family = Int32(socketAddress.pointee.sa_family)
        let rendered: String?
        if family == AF_INET {
            var address = UnsafeRawPointer(socketAddress).assumingMemoryBound(to: sockaddr_in.self)
                .pointee.sin_addr
            rendered = withUnsafePointer(to: &address) { pointer in
                inet_ntop(AF_INET, pointer, &buffer, socklen_t(INET_ADDRSTRLEN)).map { _ in
                    string(from: buffer)
                }
            }
        } else if family == AF_INET6 {
            var address = UnsafeRawPointer(socketAddress).assumingMemoryBound(to: sockaddr_in6.self)
                .pointee.sin6_addr
            rendered = withUnsafePointer(to: &address) { pointer in
                inet_ntop(AF_INET6, pointer, &buffer, socklen_t(INET6_ADDRSTRLEN)).map { _ in
                    string(from: buffer)
                }
            }
        } else {
            rendered = nil
        }
        if let rendered, isPrivateOrLoopback(rendered) { result.insert(rendered) }
    }
    return result
}

private func string(from buffer: [CChar]) -> String {
    String(decoding: buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
}

private func isPrivateOrLoopback(_ address: String) -> Bool {
    let octets = address.split(separator: ".").compactMap { UInt8($0) }
    if octets.count == 4 {
        return octets[0] == 10
            || (octets[0] == 172 && (16...31).contains(octets[1]))
            || (octets[0] == 192 && octets[1] == 168)
            || (octets[0] == 169 && octets[1] == 254)
            || octets[0] == 127
    }

    var binary = in6_addr()
    guard address.withCString({ inet_pton(AF_INET6, $0, &binary) }) == 1 else { return false }
    let bytes = withUnsafeBytes(of: &binary) { Array($0) }
    return (bytes.dropLast().allSatisfy { $0 == 0 } && bytes.last == 1)
        || (bytes[0] & 0xfe) == 0xfc
        || (bytes[0] == 0xfe && (bytes[1] & 0xc0) == 0x80)
}

private func isConnected(_ state: LKRTCIceConnectionState) -> Bool {
    state == .connected || state == .completed
}

private func awaitDescription<Value>(
    _ begin: (@escaping @Sendable (Value?, Error?) -> Void) -> Void
) throws -> Value {
    let result = AsyncResult<Value>()
    begin { value, error in
        if let error {
            result.resolve(.failure(error))
        } else if let value {
            result.resolve(.success(value))
        } else {
            result.resolve(.failure(LocalWebRTCFailure.descriptionUnavailable))
        }
    }
    return try result.wait()
}

private func awaitCompletion(_ begin: (@escaping @Sendable (Error?) -> Void) -> Void) throws {
    let result = AsyncResult<Void>()
    begin { error in
        if let error { result.resolve(.failure(error)) } else { result.resolve(.success(())) }
    }
    try result.wait()
}

private final class AsyncResult<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private let semaphore = DispatchSemaphore(value: 0)
    private var result: Result<Value, Error>?

    func resolve(_ result: Result<Value, Error>) {
        lock.withLock { self.result = result }
        semaphore.signal()
    }

    func wait(timeout: TimeInterval = 10) throws -> Value {
        guard semaphore.wait(timeout: .now() + timeout) == .success else {
            throw LocalWebRTCFailure.timeout
        }
        return try lock.withLock { try result!.get() }
    }
}

private func waitUntil(timeout: TimeInterval, condition: () -> Bool) -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
        if condition() { return true }
        Thread.sleep(forTimeInterval: 0.01)
    }
    return condition()
}

extension NSLock {
    fileprivate func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
