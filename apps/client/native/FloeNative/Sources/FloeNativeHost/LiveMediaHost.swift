import Darwin
import Foundation

struct LiveMediaBindingWire: Codable, Equatable, Sendable {
    let person_id: String
    let device_id: String
    let runtime_epoch: UInt64
    let voice_session_id: String
    let media_lease_id: String
    let plan_id: String
}

struct LiveMediaIceServerWire: Codable, Equatable, Sendable, CustomDebugStringConvertible {
    let urls: [String]
    let username: String?
    let credential: String?

    var debugDescription: String {
        "LiveMediaIceServerWire(url_count: \(urls.count), credentials: [redacted])"
    }
}

struct LiveMediaCommandWire: Codable, Sendable {
    let schema_version: Int
    let operation: String
    let host_instance_id: String
    let binding: LiveMediaBindingWire
    let deadline_ms: Int64?
    let answer_sdp: String?
    let ice_servers: [LiveMediaIceServerWire]?
}

struct LiveMediaReplyWire: Codable, Sendable {
    let schema_version: Int
    let status: String
    let host_instance_id: String
    let binding: LiveMediaBindingWire
    let offer_sdp: String?
    let capture_stopped: Bool?
    let playback_stopped: Bool?
    let failure: String?
}

enum LiveMediaHostFailure: String, Error {
    case invalidInput = "invalid_input"
    case conflict
    case notFound = "not_found"
    case deadlineExceeded = "deadline_exceeded"
    case capabilityUnavailable = "capability_unavailable"
    case microphonePermissionDenied = "microphone_permission_denied"
}

protocol LiveMediaPeer: AnyObject, Sendable {
    func startAudio(deadlineMilliseconds: Int64) throws
    func createOffer(deadlineMilliseconds: Int64) throws -> String
    func applyAnswer(_ sdp: String, deadlineMilliseconds: Int64) throws
    func fence()
    func stop(deadlineMilliseconds: Int64) throws
    func observeStop() -> (captureStopped: Bool, playbackStopped: Bool)
}

protocol LiveMediaPeerFactory: Sendable {
    func makeMutedPeer(iceServers: [LiveMediaIceServerWire]) throws -> any LiveMediaPeer
}

/// Process-local native owner for exact Voice bindings. The initial peer has
/// an audio send/receive transceiver but no local audio track, so preparation,
/// offer creation and answer application do not start microphone capture.
final class LiveMediaHost: @unchecked Sendable {
    private final class Lease: @unchecked Sendable {
        let binding: LiveMediaBindingWire
        let hostInstanceID: String
        let peer: any LiveMediaPeer
        let lock = NSLock()
        var fenced = false
        var stopIssued = false
        var audioState = AudioState.prepared
        var offerCreated = false

        enum AudioState: Equatable {
            case prepared
            case starting
            case active
            case fenced
        }

        init(binding: LiveMediaBindingWire, hostInstanceID: String, peer: any LiveMediaPeer) {
            self.binding = binding
            self.hostInstanceID = hostInstanceID
            self.peer = peer
        }
    }

    private let factory: any LiveMediaPeerFactory
    private let lock = NSLock()
    private var leases: [String: Lease] = [:]

    init(factory: any LiveMediaPeerFactory) {
        self.factory = factory
    }

    func invoke(_ request: Data) -> Data {
        guard request.count <= 131_072,
              let command = try? JSONDecoder().decode(LiveMediaCommandWire.self, from: request),
              command.schema_version == 1,
              Self.valid(command) else {
            return encodeFailure(nil, .invalidInput)
        }
        do {
            return try JSONEncoder().encode(execute(command))
        } catch let failure as LiveMediaHostFailure {
            return encodeFailure(command, failure)
        } catch {
            return encodeFailure(command, .capabilityUnavailable)
        }
    }

    private func execute(_ command: LiveMediaCommandWire) throws -> LiveMediaReplyWire {
        switch command.operation {
        case "prepare_muted":
            let peer = try factory.makeMutedPeer(iceServers: command.ice_servers ?? [])
            let lease = Lease(binding: command.binding, hostInstanceID: command.host_instance_id, peer: peer)
            lock.lock()
            defer { lock.unlock() }
            if leases[command.binding.media_lease_id] != nil || !leases.isEmpty {
                throw LiveMediaHostFailure.conflict
            }
            leases[command.binding.media_lease_id] = lease
            return reply(command)
        case "validate":
            let lease = try exactLease(command)
            try lease.lock.withLock {
                guard !lease.fenced, !lease.stopIssued else { throw LiveMediaHostFailure.conflict }
            }
            return reply(command)
        case "start_audio":
            let lease = try exactLease(command)
            let shouldStart = try lease.lock.withLock { () throws -> Bool in
                guard !lease.fenced, !lease.stopIssued else { throw LiveMediaHostFailure.conflict }
                switch lease.audioState {
                case .prepared:
                    lease.audioState = .starting
                    return true
                case .active:
                    return false
                case .starting, .fenced:
                    throw LiveMediaHostFailure.conflict
                }
            }
            guard shouldStart else { return reply(command) }
            do {
                try lease.peer.startAudio(deadlineMilliseconds: deadline(command))
                let committed = lease.lock.withLock { () -> Bool in
                    guard !lease.fenced, !lease.stopIssued, lease.audioState == .starting else {
                        return false
                    }
                    lease.audioState = .active
                    return true
                }
                guard committed else {
                    lease.peer.fence()
                    throw LiveMediaHostFailure.conflict
                }
                return reply(command)
            } catch {
                lease.lock.withLock {
                    if lease.audioState == .starting { lease.audioState = .prepared }
                }
                throw error
            }
        case "offer":
            let lease = try exactLease(command)
            let deadline = try deadline(command)
            try lease.lock.withLock {
                guard !lease.fenced, !lease.stopIssued, lease.audioState == .active else {
                    throw LiveMediaHostFailure.conflict
                }
            }
            let sdp = try lease.peer.createOffer(deadlineMilliseconds: deadline)
            guard sdp.utf8.count <= 65_536, sdp.hasPrefix("v=0\r\n") else {
                throw LiveMediaHostFailure.invalidInput
            }
            try lease.lock.withLock {
                guard !lease.fenced, !lease.stopIssued, lease.audioState == .active else {
                    throw LiveMediaHostFailure.conflict
                }
                lease.offerCreated = true
            }
            return reply(command, offerSDP: sdp)
        case "apply_answer":
            let lease = try exactLease(command)
            let deadline = try deadline(command)
            guard let answer = command.answer_sdp,
                  answer.utf8.count <= 65_536,
                  answer.hasPrefix("v=0\r\n"),
                  !answer.contains("\0") else { throw LiveMediaHostFailure.invalidInput }
            try lease.lock.withLock {
                guard !lease.fenced, !lease.stopIssued, lease.audioState == .active,
                      lease.offerCreated else { throw LiveMediaHostFailure.conflict }
            }
            try lease.peer.applyAnswer(answer, deadlineMilliseconds: deadline)
            try lease.lock.withLock {
                guard !lease.fenced, !lease.stopIssued, lease.audioState == .active else {
                    throw LiveMediaHostFailure.conflict
                }
            }
            return reply(command)
        case "fence":
            let lease = try exactLease(command)
            lease.lock.withLock {
                lease.fenced = true
                lease.audioState = .fenced
            }
            lease.peer.fence()
            return reply(command)
        case "stop_once":
            let lease = try exactLease(command)
            let shouldStop = lease.lock.withLock { () -> Bool in
                guard !lease.stopIssued else { return false }
                lease.fenced = true
                lease.audioState = .fenced
                lease.stopIssued = true
                return true
            }
            if shouldStop {
                lease.peer.fence()
                try lease.peer.stop(deadlineMilliseconds: try deadline(command))
            }
            return reply(command)
        case "observe_stop":
            let lease = try exactLease(command)
            guard lease.lock.withLock({ lease.stopIssued }) else { throw LiveMediaHostFailure.conflict }
            let observation = lease.peer.observeStop()
            return reply(
                command,
                captureStopped: observation.captureStopped,
                playbackStopped: observation.playbackStopped
            )
        case "release":
            let lease = try exactLease(command)
            guard lease.lock.withLock({ lease.stopIssued }) else { throw LiveMediaHostFailure.conflict }
            let observation = lease.peer.observeStop()
            guard observation.captureStopped, observation.playbackStopped else {
                throw LiveMediaHostFailure.conflict
            }
            lock.lock()
            defer { lock.unlock() }
            guard leases[command.binding.media_lease_id] === lease else {
                throw LiveMediaHostFailure.conflict
            }
            leases.removeValue(forKey: command.binding.media_lease_id)
            return reply(command)
        default:
            throw LiveMediaHostFailure.invalidInput
        }
    }

    private func exactLease(_ command: LiveMediaCommandWire) throws -> Lease {
        lock.lock()
        defer { lock.unlock() }
        guard let lease = leases[command.binding.media_lease_id] else {
            throw LiveMediaHostFailure.notFound
        }
        guard lease.binding == command.binding,
              lease.hostInstanceID == command.host_instance_id else {
            throw LiveMediaHostFailure.conflict
        }
        return lease
    }

    private func deadline(_ command: LiveMediaCommandWire) throws -> Int64 {
        guard let value = command.deadline_ms, (1...60_000).contains(value) else {
            throw LiveMediaHostFailure.invalidInput
        }
        return value
    }

    private func reply(
        _ command: LiveMediaCommandWire,
        offerSDP: String? = nil,
        captureStopped: Bool? = nil,
        playbackStopped: Bool? = nil
    ) -> LiveMediaReplyWire {
        LiveMediaReplyWire(
            schema_version: 1,
            status: "ok",
            host_instance_id: command.host_instance_id,
            binding: command.binding,
            offer_sdp: offerSDP,
            capture_stopped: captureStopped,
            playback_stopped: playbackStopped,
            failure: nil
        )
    }

    private func encodeFailure(_ command: LiveMediaCommandWire?, _ failure: LiveMediaHostFailure) -> Data {
        let fallback = LiveMediaCommandWire(
            schema_version: 1,
            operation: "",
            host_instance_id: command?.host_instance_id ?? "",
            binding: command?.binding ?? LiveMediaBindingWire(
                person_id: "", device_id: "", runtime_epoch: 0,
                voice_session_id: "", media_lease_id: "", plan_id: ""
            ),
            deadline_ms: nil,
            answer_sdp: nil,
            ice_servers: nil
        )
        let reply = LiveMediaReplyWire(
            schema_version: 1,
            status: "error",
            host_instance_id: fallback.host_instance_id,
            binding: fallback.binding,
            offer_sdp: nil,
            capture_stopped: nil,
            playback_stopped: nil,
            failure: failure.rawValue
        )
        return (try? JSONEncoder().encode(reply)) ?? Data()
    }

    private static func valid(_ command: LiveMediaCommandWire) -> Bool {
        guard UUID(uuidString: command.host_instance_id) != nil,
              UUID(uuidString: command.binding.person_id) != nil,
              UUID(uuidString: command.binding.voice_session_id) != nil,
              UUID(uuidString: command.binding.media_lease_id) != nil,
              UUID(uuidString: command.binding.plan_id) != nil,
              !command.binding.device_id.isEmpty,
              command.binding.device_id.utf8.count <= 128,
              command.binding.runtime_epoch > 0,
              validIceConfiguration(command) else { return false }
        return true
    }

    private static func validIceConfiguration(_ command: LiveMediaCommandWire) -> Bool {
        guard command.operation == "prepare_muted" else {
            return command.ice_servers == nil
        }
        guard let servers = command.ice_servers, servers.count <= 8 else { return false }
        var seenURLs = Set<String>()
        for server in servers {
            guard !server.urls.isEmpty, server.urls.count <= 4 else { return false }
            let schemes = server.urls.compactMap(validIceURL)
            guard schemes.count == server.urls.count else { return false }
            guard server.urls.allSatisfy({ seenURLs.insert($0).inserted }) else { return false }
            guard (server.username == nil) == (server.credential == nil) else { return false }
            if let username = server.username, let credential = server.credential {
                guard validIceSecret(username, maximumBytes: 512),
                      validIceSecret(credential, maximumBytes: 4_096),
                      schemes.allSatisfy({ $0 == "turn" || $0 == "turns" }) else {
                    return false
                }
            }
        }
        return true
    }

    private static func validIceSecret(_ value: String, maximumBytes: Int) -> Bool {
        !value.isEmpty
            && value.utf8.count <= maximumBytes
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    private static func validIceURL(_ value: String) -> String? {
        guard !value.isEmpty, value.utf8.count <= 2_048,
              value.utf8.allSatisfy({ (33...126).contains($0) }) else { return nil }
        let parts = value.split(separator: ":", maxSplits: 1, omittingEmptySubsequences: false)
        guard parts.count == 2 else { return nil }
        let scheme = String(parts[0])
        guard ["stun", "stuns", "turn", "turns"].contains(scheme) else { return nil }
        let remainder = String(parts[1])
        let queryParts = remainder.split(separator: "?", maxSplits: 1, omittingEmptySubsequences: false)
        let authority = String(queryParts[0])
        guard validIceAuthority(authority) else { return nil }
        if queryParts.count == 2 {
            let query = String(queryParts[1])
            guard !query.contains("?"),
                  scheme == "turn" || scheme == "turns",
                  query == "transport=udp" || query == "transport=tcp",
                  !(scheme == "turns" && query == "transport=udp") else {
                return nil
            }
        }
        return scheme
    }

    private static func validIceAuthority(_ authority: String) -> Bool {
        guard !authority.isEmpty,
              !authority.hasPrefix("//"),
              !authority.contains(where: { "/\\\\@#%?".contains($0) }) else { return false }
        if authority.hasPrefix("[") {
            guard let close = authority.firstIndex(of: "]") else { return false }
            let address = String(authority[authority.index(after: authority.startIndex)..<close])
            var parsed = in6_addr()
            guard address.withCString({ inet_pton(AF_INET6, $0, &parsed) }) == 1 else { return false }
            let suffix = String(authority[authority.index(after: close)...])
            if suffix.isEmpty { return true }
            return suffix.first == ":" && validIcePort(String(suffix.dropFirst()))
        }

        let parts = authority.split(separator: ":", maxSplits: 1, omittingEmptySubsequences: false)
        guard parts.count <= 2 else { return false }
        let host = String(parts[0])
        guard !host.isEmpty, host.utf8.count <= 253 else { return false }
        let labels = host.split(separator: ".", omittingEmptySubsequences: false)
        guard labels.allSatisfy({ label in
            guard !label.isEmpty, label.utf8.count <= 63,
                  let first = label.first, let last = label.last,
                  isASCIIAlphaNumeric(first), isASCIIAlphaNumeric(last) else { return false }
            return label.allSatisfy { isASCIIAlphaNumeric($0) || $0 == "-" }
        }) else { return false }
        return parts.count == 1 || validIcePort(String(parts[1]))
    }

    private static func validIcePort(_ value: String) -> Bool {
        !value.isEmpty
            && value.utf8.allSatisfy({ (48...57).contains($0) })
            && UInt16(value).map { $0 > 0 } == true
    }

    private static func isASCIIAlphaNumeric(_ character: Character) -> Bool {
        guard let byte = character.asciiValue else { return false }
        return (48...57).contains(byte) || (65...90).contains(byte) || (97...122).contains(byte)
    }
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
