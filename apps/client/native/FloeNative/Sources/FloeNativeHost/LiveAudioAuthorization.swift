import AVFoundation
import Foundation

enum MicrophoneAuthorizationStatus: Sendable, Equatable {
    case authorized
    case notDetermined
    case denied
    case restricted
}

protocol MicrophoneAuthorizationProvider: Sendable {
    func status() -> MicrophoneAuthorizationStatus
    func requestAccess(deadlineMilliseconds: Int64) throws -> Bool
}

struct AppleMicrophoneAuthorizationProvider: MicrophoneAuthorizationProvider {
    func status() -> MicrophoneAuthorizationStatus {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: .authorized
        case .notDetermined: .notDetermined
        case .denied: .denied
        case .restricted: .restricted
        @unknown default: .restricted
        }
    }

    func requestAccess(deadlineMilliseconds: Int64) throws -> Bool {
        guard (1 ... 60_000).contains(deadlineMilliseconds) else {
            throw LiveMediaHostFailure.deadlineExceeded
        }
        let result = PermissionRequestResult()
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            result.complete(granted)
        }
        guard result.wait(milliseconds: deadlineMilliseconds) else {
            throw LiveMediaHostFailure.deadlineExceeded
        }
        return result.granted
    }
}

struct MicrophoneAuthorizationGate: Sendable {
    private let provider: any MicrophoneAuthorizationProvider

    init(provider: any MicrophoneAuthorizationProvider) {
        self.provider = provider
    }

    func requireAccess(deadlineMilliseconds: Int64) throws {
        switch provider.status() {
        case .authorized:
            return
        case .denied, .restricted:
            throw LiveMediaHostFailure.microphonePermissionDenied
        case .notDetermined:
            guard try provider.requestAccess(deadlineMilliseconds: deadlineMilliseconds) else {
                throw LiveMediaHostFailure.microphonePermissionDenied
            }
        }
    }
}

private final class PermissionRequestResult: @unchecked Sendable {
    private let lock = NSLock()
    private let semaphore = DispatchSemaphore(value: 0)
    private var result: Bool?

    var granted: Bool {
        lock.withLock { result == true }
    }

    func complete(_ granted: Bool) {
        lock.withLock {
            guard result == nil else { return }
            result = granted
            semaphore.signal()
        }
    }

    func wait(milliseconds: Int64) -> Bool {
        semaphore.wait(timeout: .now() + .milliseconds(Int(milliseconds))) == .success
    }
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
