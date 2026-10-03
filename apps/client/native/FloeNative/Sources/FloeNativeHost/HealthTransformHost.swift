import Dispatch
import Foundation
import FloeHealthTransform
import FloeHealthTransformBridge
import FloeModelExecution

/// Owns Health operation admission and consume-once receipts for one native image.
/// It is independent from the general DeviceModel ABI job host.
final class HealthTransformHost: @unchecked Sendable {
    private static let operationDeadlineNanoseconds: UInt64 = 10_000_000_000
    private static let retentionNanoseconds: UInt64 = 30 * 60 * 1_000_000_000
    private static let retentionMilliseconds: Int64 = 30 * 60 * 1_000
    private static let maximumReceipts = 64
    private static let maximumIdentities = 128
    private static let noOperationID = UUID(uuid: (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))

    private struct Job {
        let requestID: UUID
        let incarnation: UUID
        let input: HealthTransformInput
        let binding: HealthTransformBinding
        let deadlineUptimeNanoseconds: UInt64
        var task: Task<Result<HealthTransformOutput, HealthTransformFailure>, Never>?
        var workerObserver: Task<Void, Never>?
        var timer: Task<Void, Never>?
        var reply: HealthTransformReply?
        var released = false
        var workerFinished = false
    }

    private enum OperationIdentity {
        case inFlight
        case retained(completedAtUptimeNanoseconds: UInt64, receiptExpiresAtUnixMs: Int64?)
    }

    private enum Availability {
        case available
        case unavailable(HealthTransformFailure)
        case failed(HealthTransformFailure)
    }

    private let model: any DeviceModel
    private let lock = NSLock()
    private var job: Job?
    private var receipts: [UUID: HealthTransformReply] = [:]
    private var operationIdentities: [UUID: OperationIdentity] = [:]

    init(model: any DeviceModel) {
        self.model = model
    }

    func invoke(_ command: HealthTransformCommand) -> HealthTransformReply {
        guard command.schemaVersion == 1 else { return failure(.invalidInput) }

        switch command.operation {
        case "availability":
            guard command.requestID == nil, command.input == nil,
                  command.binding == nil, command.outputSHA256 == nil else {
                return failure(.invalidInput)
            }
            switch observeAvailability() {
            case .available:
                return HealthTransformReply(status: "availability", availability: "available")
            case .unavailable(let reason):
                return HealthTransformReply(status: "availability", availability: reason.rawValue)
            case .failed(let reason):
                return failure(reason)
            }

        case "start":
            guard let requestID = command.requestID, requestID != Self.noOperationID else {
                return failure(.invalidInput)
            }
            guard let input = command.input, let binding = command.binding,
                  command.outputSHA256 == nil else {
                return failure(.invalidInput, command.requestID)
            }
            return start(requestID: requestID, input: input, binding: binding)

        case "poll", "cancel", "release":
            guard let requestID = command.requestID, requestID != Self.noOperationID else {
                return failure(.invalidInput)
            }
            guard command.input == nil, command.binding == nil,
                  command.outputSHA256 == nil else {
                return failure(.invalidInput, command.requestID)
            }
            return operateOnJob(command.operation, requestID: requestID)

        case "consume_receipt":
            guard let requestID = command.requestID, requestID != Self.noOperationID else {
                return failure(.invalidInput)
            }
            guard command.input == nil, let binding = command.binding,
                  let digest = command.outputSHA256 else {
                return failure(.invalidInput, command.requestID)
            }
            return consumeReceipt(requestID: requestID, binding: binding, digest: digest)

        default:
            return failure(.invalidInput, command.requestID)
        }
    }

    private func start(
        requestID: UUID,
        input: HealthTransformInput,
        binding: HealthTransformBinding
    ) -> HealthTransformReply {
        do {
            try input.validate()
            try binding.validate()
        } catch {
            return failure(.invalidInput, requestID)
        }

        lock.lock()
        pruneRetainedStateLocked(uptimeNanoseconds: DispatchTime.now().uptimeNanoseconds, unixMs: nowUnixMs())
        if let reply = admissionReplyLocked(requestID: requestID, input: input, binding: binding) {
            lock.unlock()
            return reply
        }
        lock.unlock()

        // This is a bounded local observation only. It must never hold the host lock.
        switch observeAvailability() {
        case .available:
            break
        case .unavailable(let reason):
            return HealthTransformReply(
                status: "error",
                requestID: requestID,
                availability: reason.rawValue,
                failure: reason
            )
        case .failed(let reason):
            return failure(reason, requestID)
        }

        lock.lock()
        let now = DispatchTime.now().uptimeNanoseconds
        pruneRetainedStateLocked(uptimeNanoseconds: now, unixMs: nowUnixMs())
        if let reply = admissionReplyLocked(requestID: requestID, input: input, binding: binding) {
            lock.unlock()
            return reply
        }

        let deadline = adding(now, Self.operationDeadlineNanoseconds)
        let incarnation = UUID()
        job = Job(
            requestID: requestID,
            incarnation: incarnation,
            input: input,
            binding: binding,
            deadlineUptimeNanoseconds: deadline
        )
        operationIdentities[requestID] = .inFlight
        lock.unlock()

        return launch(requestID: requestID, incarnation: incarnation, input: input, deadline: deadline)
    }

    /// Returns a terminal/replay response when admission cannot proceed.
    /// Must be called with `lock` held.
    private func admissionReplyLocked(
        requestID: UUID,
        input: HealthTransformInput,
        binding: HealthTransformBinding
    ) -> HealthTransformReply? {
        if let current = job, current.requestID == requestID {
            guard current.input == input, current.binding == binding, !current.released else {
                return failure(.conflict, requestID)
            }
            return snapshotLocked(current)
        }

        if operationIdentities[requestID] != nil {
            return failure(.conflict, requestID)
        }

        if job != nil || receipts.count >= Self.maximumReceipts
            || operationIdentities.count >= Self.maximumIdentities {
            return failure(.busy, requestID)
        }
        return nil
    }

    private func observeAvailability() -> Availability {
        do {
            let requirements = DeviceModelRequirements(capabilities: [.structuredOutput])
            try requirements.validate()
            let observation = try model.prepare(requirements)
            try observation.validate()
            switch observation {
            case .available(let profile):
                guard profile.capabilities.contains(.structuredOutput) else {
                    return .failed(.invalidOutput)
                }
                return .available
            case .unavailable(let reason):
                switch reason {
                case .unsupported: return .unavailable(.unsupported)
                case .disabled: return .unavailable(.disabled)
                case .notReady: return .unavailable(.notReady)
                }
            }
        } catch {
            return .failed(mapError(error))
        }
    }

    private func launch(
        requestID: UUID,
        incarnation: UUID,
        input: HealthTransformInput,
        deadline: UInt64
    ) -> HealthTransformReply {
        lock.lock()
        guard var current = job, current.requestID == requestID,
              current.incarnation == incarnation, current.deadlineUptimeNanoseconds == deadline,
              current.task == nil, !current.workerFinished else {
            lock.unlock()
            return failure(.conflict, requestID)
        }

        if current.reply == nil && DispatchTime.now().uptimeNanoseconds >= deadline {
            current.reply = failure(.deadlineExceeded, requestID)
        }

        // The task first reacquires the lock in beginWorker. Since this lock is
        // held through installation, no model work can begin under the lock.
        // Even an already-cancelled reservation gets this short worker. Only
        // the observer awaiting its physical return may complete the job.
        let worker: Task<Result<HealthTransformOutput, HealthTransformFailure>, Never> = Task.detached { [self, input] in
            guard !Task.isCancelled, beginWorker(requestID, incarnation: incarnation, deadline: deadline) else {
                return .failure(.cancelled)
            }

            do {
                try Task.checkCancellation()
                let transform = try HealthTransform(
                    model: model,
                    operationID: requestID,
                    deadlineUptimeNanoseconds: deadline
                )
                let output = try await transform.transform(input)
                try Task.checkCancellation()
                return .success(output)
            } catch {
                return .failure(mapError(error))
            }
        }
        current.task = worker
        current.workerObserver = Task.detached { [self, worker] in
            let result = await worker.value
            finish(requestID, incarnation: incarnation, deadline: deadline, result)
        }
        let timerStart = DispatchTime.now().uptimeNanoseconds
        let remaining = deadline > timerStart ? deadline - timerStart : 0
        if current.reply == nil && !current.released {
            current.timer = Task.detached { [self] in
                do {
                    try await Task.sleep(nanoseconds: remaining)
                    expire(requestID, incarnation: incarnation, deadline: deadline)
                } catch {}
            }
        }
        let immediateReply = current.reply ?? HealthTransformReply(status: "pending", requestID: requestID)
        job = current
        lock.unlock()
        return immediateReply
    }

    /// Prevents a released/cancelled reservation from starting model work.
    private func beginWorker(_ requestID: UUID, incarnation: UUID, deadline: UInt64) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard let current = job, current.requestID == requestID,
              current.incarnation == incarnation, current.deadlineUptimeNanoseconds == deadline,
              !current.released, current.reply == nil else {
            return false
        }
        guard DispatchTime.now().uptimeNanoseconds < current.deadlineUptimeNanoseconds else {
            job?.reply = failure(.deadlineExceeded, requestID)
            current.task?.cancel()
            current.timer?.cancel()
            return false
        }
        return true
    }

    private func operateOnJob(_ operation: String, requestID: UUID) -> HealthTransformReply {
        lock.lock()
        defer { lock.unlock() }
        let nowUptime = DispatchTime.now().uptimeNanoseconds
        pruneRetainedStateLocked(uptimeNanoseconds: nowUptime, unixMs: nowUnixMs())

        guard var current = job, current.requestID == requestID, !current.released else {
            return failure(.notFound, requestID)
        }
        switch operation {
        case "poll":
            return snapshotLocked(current)
        case "cancel":
            if current.reply == nil {
                current.reply = failure(.cancelled, requestID)
                current.task?.cancel()
                current.timer?.cancel()
                job = current
            }
            return snapshotLocked(current)
        case "release":
            if current.workerFinished {
                job = nil
            } else {
                current.released = true
                current.reply = current.reply ?? failure(.cancelled, requestID)
                current.task?.cancel()
                current.timer?.cancel()
                job = current
            }
            return HealthTransformReply(status: "released", requestID: requestID)
        default:
            return failure(.invalidInput, requestID)
        }
    }

    private func expire(_ requestID: UUID, incarnation: UUID, deadline: UInt64) {
        lock.lock()
        defer { lock.unlock() }
        // A cancelled timer may already have left sleep. It cannot affect a
        // later reuse of this UUID after the original identity horizon ends.
        guard var current = job, current.requestID == requestID,
              current.incarnation == incarnation,
              current.deadlineUptimeNanoseconds == deadline, current.reply == nil else { return }
        current.reply = failure(.deadlineExceeded, requestID)
        current.task?.cancel()
        current.timer?.cancel()
        job = current
    }

    private func finish(
        _ requestID: UUID,
        incarnation: UUID,
        deadline: UInt64,
        _ result: Result<HealthTransformOutput, HealthTransformFailure>
    ) {
        lock.lock()
        defer { lock.unlock() }
        guard var current = job, current.requestID == requestID,
              current.incarnation == incarnation, current.deadlineUptimeNanoseconds == deadline,
              !current.workerFinished else { return }

        current.timer?.cancel()
        current.task = nil
        current.workerObserver = nil
        current.timer = nil
        current.workerFinished = true
        let completedAtUptime = DispatchTime.now().uptimeNanoseconds

        if current.released {
            retainIdentityLocked(requestID, receiptExpiryUnixMs: nil, completedAtUptime: completedAtUptime)
            job = nil
            return
        }

        guard current.reply == nil else {
            retainIdentityLocked(requestID, receiptExpiryUnixMs: nil, completedAtUptime: completedAtUptime)
            job = current
            return
        }

        guard completedAtUptime < current.deadlineUptimeNanoseconds else {
            current.reply = failure(.deadlineExceeded, requestID)
            retainIdentityLocked(requestID, receiptExpiryUnixMs: nil, completedAtUptime: completedAtUptime)
            job = current
            return
        }

        switch result {
        case .success(let output):
            guard receipts.count < Self.maximumReceipts else {
                current.reply = failure(.busy, requestID)
                retainIdentityLocked(requestID, receiptExpiryUnixMs: nil, completedAtUptime: completedAtUptime)
                job = current
                return
            }
            let transformedAt = nowUnixMs()
            let expiresAt = adding(transformedAt, Self.retentionMilliseconds)
            let digest = HealthTransformDigest.hex(output)
            current.reply = HealthTransformReply(
                status: "done",
                requestID: requestID,
                output: output,
                binding: current.binding,
                outputSHA256: digest,
                transformedAtUnixMs: transformedAt,
                expiresAtUnixMs: expiresAt
            )
            receipts[requestID] = HealthTransformReply(
                status: "receipt",
                requestID: requestID,
                output: output,
                binding: current.binding,
                outputSHA256: digest,
                transformedAtUnixMs: transformedAt,
                expiresAtUnixMs: expiresAt
            )
            retainIdentityLocked(
                requestID,
                receiptExpiryUnixMs: expiresAt,
                completedAtUptime: completedAtUptime
            )

        case .failure(let reason):
            current.reply = failure(reason, requestID)
            retainIdentityLocked(requestID, receiptExpiryUnixMs: nil, completedAtUptime: completedAtUptime)
        }
        job = current
    }

    private func snapshotLocked(_ current: Job) -> HealthTransformReply {
        if let reply = current.reply { return reply }
        guard DispatchTime.now().uptimeNanoseconds < current.deadlineUptimeNanoseconds else {
            job?.reply = failure(.deadlineExceeded, current.requestID)
            current.task?.cancel()
            current.timer?.cancel()
            return job?.reply ?? failure(.deadlineExceeded, current.requestID)
        }
        return HealthTransformReply(status: "pending", requestID: current.requestID)
    }

    private func consumeReceipt(
        requestID: UUID,
        binding: HealthTransformBinding,
        digest: String
    ) -> HealthTransformReply {
        lock.lock()
        defer { lock.unlock() }
        pruneRetainedStateLocked(uptimeNanoseconds: DispatchTime.now().uptimeNanoseconds, unixMs: nowUnixMs())
        guard (try? binding.validate()) != nil,
              let saved = receipts[requestID], saved.requestID == requestID,
              saved.binding == binding, saved.outputSHA256 == digest,
              let expiry = saved.expiresAtUnixMs, expiry > nowUnixMs() else {
            return failure(.notFound, requestID)
        }
        receipts.removeValue(forKey: requestID)
        // Consuming the proof removes the receipt, never the operation tombstone.
        return saved
    }

    private func pruneRetainedStateLocked(uptimeNanoseconds: UInt64, unixMs: Int64) {
        receipts = receipts.filter { ($0.value.expiresAtUnixMs ?? 0) > unixMs }
        operationIdentities = operationIdentities.filter { requestID, identity in
            if job?.requestID == requestID || receipts[requestID] != nil { return true }
            guard case let .retained(completedAt, receiptExpiry) = identity else { return true }
            let monotonicHorizonPassed = uptimeNanoseconds >= adding(completedAt, Self.retentionNanoseconds)
            let receiptHorizonPassed = receiptExpiry.map { $0 <= unixMs } ?? true
            return !(monotonicHorizonPassed && receiptHorizonPassed)
        }
    }

    private func retainIdentityLocked(
        _ requestID: UUID,
        receiptExpiryUnixMs: Int64?,
        completedAtUptime: UInt64 = DispatchTime.now().uptimeNanoseconds
    ) {
        operationIdentities[requestID] = .retained(
            completedAtUptimeNanoseconds: completedAtUptime,
            receiptExpiresAtUnixMs: receiptExpiryUnixMs
        )
    }

    private func mapError(_ error: any Error) -> HealthTransformFailure {
        if error is CancellationError { return .cancelled }
        if let validation = error as? HealthTransformValidationError {
            switch validation {
            case .invalidInput: return .invalidInput
            case .invalidOutput: return .invalidOutput
            }
        }
        if let failure = error as? DeviceModelFailure {
            switch failure {
            case .unsupported: return .unsupported
            case .disabled: return .disabled
            case .notReady: return .notReady
            case .unavailable: return .modelUnavailable
            case .invalidInput: return .invalidInput
            case .invalidOutput: return .invalidOutput
            case .deadlineExceeded: return .deadlineExceeded
            case .cancelled: return .cancelled
            case .policyDenied: return .policyDenied
            case .quotaExceeded, .budgetExceeded: return .modelUnavailable
            case .busy: return .busy
            case .conflict: return .conflict
            case .notFound: return .notFound
            }
        }
        return .modelUnavailable
    }

    private func failure(_ reason: HealthTransformFailure, _ requestID: UUID? = nil) -> HealthTransformReply {
        HealthTransformReply(status: "error", requestID: requestID, failure: reason)
    }

    private func nowUnixMs() -> Int64 {
        Int64(Date().timeIntervalSince1970 * 1_000)
    }

    private func adding(_ lhs: UInt64, _ rhs: UInt64) -> UInt64 {
        let (sum, overflow) = lhs.addingReportingOverflow(rhs)
        return overflow ? UInt64.max : sum
    }

    private func adding(_ lhs: Int64, _ rhs: Int64) -> Int64 {
        let (sum, overflow) = lhs.addingReportingOverflow(rhs)
        return overflow ? Int64.max : sum
    }
}
