import Foundation

/// Owns the one-shot transition from a prepared, inaudible lease to active
/// audio. WebRTC callbacks use this same gate so late receivers cannot reopen
/// output after fence, interruption, Stop, or lease replacement.
final class LiveAudioTrackGate: @unchecked Sendable {
    enum State: Equatable {
        case prepared
        case active
        case fenced
        case stopped
    }

    private typealias TrackControl = (Bool) -> Void

    private let lock = NSRecursiveLock()
    private var state = State.prepared
    private var localTrack: TrackControl?
    private var remoteTracks: [String: TrackControl] = [:]

    func installLocalTrack(id: String, setEnabled: @escaping (Bool) -> Void) -> Bool {
        lock.withLock {
            guard state == .prepared, localTrack == nil, !id.isEmpty else {
                setEnabled(false)
                return false
            }
            localTrack = setEnabled
            setEnabled(false)
            return true
        }
    }

    func activate() -> Bool {
        lock.withLock {
            guard state == .prepared, let localTrack else { return false }
            state = .active
            localTrack(true)
            remoteTracks.values.forEach { $0(true) }
            return true
        }
    }

    func receiveRemoteTrack(id: String, setEnabled: @escaping (Bool) -> Void) {
        lock.withLock {
            guard state == .prepared || state == .active, !id.isEmpty else {
                setEnabled(false)
                return
            }
            remoteTracks[id] = setEnabled
            setEnabled(state == .active)
        }
    }

    func removeRemoteTrack(id: String) {
        lock.withLock {
            remoteTracks.removeValue(forKey: id)?(false)
        }
    }

    func fence() {
        lock.withLock {
            guard state == .prepared || state == .active else { return }
            state = .fenced
            localTrack?(false)
            remoteTracks.values.forEach { $0(false) }
        }
    }

    func stop() {
        lock.withLock {
            guard state != .stopped else { return }
            state = .stopped
            localTrack?(false)
            remoteTracks.values.forEach { $0(false) }
            localTrack = nil
            remoteTracks.removeAll()
        }
    }

    var isActive: Bool { lock.withLock { state == .active } }
    var isTerminal: Bool { lock.withLock { state == .fenced || state == .stopped } }
    var hasStoppedAudio: Bool { lock.withLock { state != .active } }
    var currentState: State { lock.withLock { state } }
}

private extension NSRecursiveLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
