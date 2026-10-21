import Darwin
import Foundation
import LiveKitWebRTC
import XCTest

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
}

/// This test-only device owns no AudioUnit or AVAudioSession. Its lifecycle
/// methods only record state; the test never calls the module's start methods.
private final class MemoryOnlyRTCAudioDevice: NSObject, LKRTCAudioDevice, @unchecked Sendable {
    private let lock = NSLock()
    private var audioDelegate: LKRTCAudioDeviceDelegate?
    private var initialized = false
    private var playoutInitialized = false
    private var recordingInitialized = false
    private var playing = false
    private var recording = false

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
