import Foundation
import FloeModelExecution

/// Owns only the generic transport lifetime for an injected device model.
public final class DeviceModelHost: @unchecked Sendable {
    private enum Outcome: Sendable {
        case pending
        case response(DeviceModelResponse)
        case failure(DeviceModelFailure)
    }

    private struct Operation {
        // Keep this request unchanged for exact duplicate-start identity.
        let request: DeviceModelRequest
        let incarnation: UUID
        let deadlineUptimeNanoseconds: UInt64
        var outcome: Outcome = .pending
        var workerReturned = false
        var physicalReturnUptimeNanoseconds: UInt64?
        var released = false
        var worker: Task<Outcome?, Never>?
        var workerObserver: Task<Void, Never>?
        var timer: Task<Void, Never>?
    }

    private let model: any DeviceModel
    private let lock = NSLock()
    private var operations: [UUID: Operation] = [:]
    private var tombstones: [UUID: UInt64] = [:]
    private var reservedTombstones = 0

    private let maximumOperations = 8
    private let maximumTombstones = 256
    private let tombstoneLifetimeNanoseconds: UInt64 = 30_000_000_000

    public init(model: any DeviceModel) {
        self.model = model
    }

    public func invoke(_ command: DeviceModelCommand) -> DeviceModelReply {
        switch command {
        case .prepare(let requirements):
            return prepare(requirements)
        case .start(let request):
            return start(request)
        case .poll(let operationID):
            return poll(operationID)
        case .cancel(let operationID):
            return cancel(operationID)
        case .release(let operationID):
            return release(operationID)
        }
    }

    private func prepare(_ requirements: DeviceModelRequirements) -> DeviceModelReply {
        do {
            try requirements.validate()
        } catch {
            return .error(operationID: nil, failure: failure(from: error, fallback: .invalidInput))
        }

        let observation: DeviceModelObservation
        do {
            observation = try model.prepare(requirements)
        } catch {
            return .error(operationID: nil, failure: failure(from: error, fallback: .unavailable))
        }

        do {
            try observation.validate()
        } catch {
            return .error(operationID: nil, failure: .invalidOutput)
        }
        if case .available(let profile) = observation,
           !requirements.capabilities.allSatisfy({ profile.capabilities.contains($0) }) {
            return .error(operationID: nil, failure: .invalidOutput)
        }
        return .observation(observation)
    }

    private func start(_ request: DeviceModelRequest) -> DeviceModelReply {
        do {
            try request.validate()
        } catch {
            return .error(operationID: request.operationID,
                          failure: failure(from: error, fallback: .invalidInput))
        }

        let incarnation = UUID()
        let now = Self.monotonicNow()
        let duration = UInt64(request.deadlineMilliseconds) * 1_000_000
        let (deadline, overflow) = now.addingReportingOverflow(duration)
        guard !overflow else {
            return .error(operationID: request.operationID, failure: .invalidInput)
        }

        var workerToCancel: Task<Outcome?, Never>?
        var timerToCancel: Task<Void, Never>?
        let admission: StartAdmission = withLock {
            pruneExpiredTombstonesLocked(at: now)

            if let current = operations[request.operationID] {
                guard !current.released else {
                    return .rejected(.error(operationID: request.operationID, failure: .conflict))
                }
                guard Self.sameRequest(current.request, request) else {
                    return .rejected(.error(operationID: request.operationID, failure: .conflict))
                }
                let reply = snapshotLocked(request.operationID,
                                          workerToCancel: &workerToCancel,
                                          timerToCancel: &timerToCancel)
                return .existing(reply)
            }

            guard tombstones[request.operationID] == nil else {
                return .rejected(.error(operationID: request.operationID, failure: .conflict))
            }
            guard operations.count < maximumOperations,
                  tombstones.count + reservedTombstones < maximumTombstones else {
                return .rejected(.error(operationID: request.operationID, failure: .busy))
            }

            operations[request.operationID] = Operation(
                request: request,
                incarnation: incarnation,
                deadlineUptimeNanoseconds: deadline
            )
            // Every admitted operation reserves its eventual tombstone slot.
            reservedTombstones += 1
            return .admitted
        }
        workerToCancel?.cancel()
        timerToCancel?.cancel()

        switch admission {
        case .existing(let reply), .rejected(let reply):
            return reply
        case .admitted:
            break
        }

        workerToCancel = nil
        timerToCancel = nil
        let reply = withLock { () -> DeviceModelReply in
            guard var operation = operations[request.operationID], operation.incarnation == incarnation else {
                return .error(operationID: request.operationID, failure: .notFound)
            }
            // Workers first reacquire this lock in isActive. Install every
            // cancellation handle before any backend preparation can begin.
            let model = self.model
            let worker: Task<Outcome?, Never> = Task.detached { [self, model, request] in
                await execute(request, incarnation: incarnation, using: model)
            }
            let observer: Task<Void, Never> = Task.detached { [weak self, worker, request] in
                let result = await worker.value
                self?.finishWorker(request.operationID, incarnation: incarnation, result: result)
            }
            operation.worker = worker
            operation.workerObserver = observer
            if isPending(operation.outcome) && !operation.released {
                let timer: Task<Void, Never> = Task.detached { [weak self] in
                    let now = Self.monotonicNow()
                    let remaining = deadline > now ? deadline - now : 0
                    if remaining > 0 {
                        do {
                            try await Task.sleep(nanoseconds: remaining)
                        } catch {
                            return
                        }
                    }
                    self?.expire(request.operationID, incarnation: incarnation)
                }
                operation.timer = timer
            } else {
                workerToCancel = worker
            }
            operations[request.operationID] = operation
            return snapshotLocked(request.operationID,
                                  workerToCancel: &workerToCancel,
                                  timerToCancel: &timerToCancel)
        }
        workerToCancel?.cancel()
        timerToCancel?.cancel()
        return reply
    }

    private enum StartAdmission {
        case admitted
        case existing(DeviceModelReply)
        case rejected(DeviceModelReply)
    }

    private func execute(_ request: DeviceModelRequest, incarnation: UUID, using model: any DeviceModel) async -> Outcome? {
        do {
            guard isActive(request.operationID, incarnation: incarnation) else {
                return nil
            }

            let requirements = DeviceModelRequirements(capabilities: requiredCapabilities(for: request))
            try Task.checkCancellation()
            try requirements.validate()
            let observation = try model.prepare(requirements)
            do {
                try observation.validate()
            } catch {
                throw DeviceModelFailure.invalidOutput
            }

            let profile: DeviceModelProfile
            switch observation {
            case .available(let available):
                profile = available
            case .unavailable(let unavailable):
                throw failure(for: unavailable)
            }
            try profile.validate(request: request)

            guard let remainingMilliseconds = remainingMilliseconds(for: request.operationID, incarnation: incarnation),
                  remainingMilliseconds > 0 else {
                return nil
            }
            try Task.checkCancellation()

            let generationRequest = Self.request(request, withDeadlineMilliseconds: remainingMilliseconds)
            let response = try await model.generate(generationRequest)
            try Task.checkCancellation()

            do {
                try response.validateEnvelope(for: generationRequest)
                return .response(try response.normalized(for: generationRequest))
            } catch {
                // Envelope identity or usage is untrusted and must not carry accounting.
                return .failure(.invalidOutput)
            }
        } catch let failure as DeviceModelFailure {
            return .failure(failure)
        } catch is CancellationError {
            return .failure(.cancelled)
        } catch {
            return .failure(.unavailable)
        }
    }

    private func poll(_ operationID: UUID) -> DeviceModelReply {
        var workerToCancel: Task<Outcome?, Never>?
        var timerToCancel: Task<Void, Never>?
        let reply = withLock {
            snapshotLocked(operationID,
                          workerToCancel: &workerToCancel,
                          timerToCancel: &timerToCancel)
        }
        workerToCancel?.cancel()
        timerToCancel?.cancel()
        return reply
    }

    private func cancel(_ operationID: UUID) -> DeviceModelReply {
        var workerToCancel: Task<Outcome?, Never>?
        var timerToCancel: Task<Void, Never>?
        let reply = withLock { () -> DeviceModelReply in
            guard var operation = operations[operationID], !operation.released else {
                return .error(operationID: operationID, failure: .notFound)
            }
            if isPending(operation.outcome) {
                if Self.monotonicNow() >= operation.deadlineUptimeNanoseconds {
                    operation.outcome = .failure(.deadlineExceeded)
                } else {
                    operation.outcome = .failure(.cancelled)
                }
                workerToCancel = operation.worker
                timerToCancel = operation.timer
                operation.timer = nil
                operations[operationID] = operation
            }
            return snapshotLocked(operationID,
                                  workerToCancel: &workerToCancel,
                                  timerToCancel: &timerToCancel)
        }
        workerToCancel?.cancel()
        timerToCancel?.cancel()
        return reply
    }

    private func release(_ operationID: UUID) -> DeviceModelReply {
        var workerToCancel: Task<Outcome?, Never>?
        var timerToCancel: Task<Void, Never>?
        let reply = withLock { () -> DeviceModelReply in
            pruneExpiredTombstonesLocked(at: Self.monotonicNow())
            guard var operation = operations[operationID] else {
                return tombstones[operationID] == nil
                    ? .error(operationID: operationID, failure: .notFound)
                    : .released(operationID)
            }

            if !operation.released {
                operation.released = true
                // A released result is no longer observable; do not retain its payload.
                operation.outcome = .failure(.cancelled)
                workerToCancel = operation.worker
                timerToCancel = operation.timer
                operation.timer = nil
                operations[operationID] = operation
            }

            if operation.workerReturned {
                finalizeReleaseLocked(operationID, at: Self.monotonicNow())
            }
            return .released(operationID)
        }
        workerToCancel?.cancel()
        timerToCancel?.cancel()
        return reply
    }

    private func expire(_ operationID: UUID, incarnation: UUID) {
        var workerToCancel: Task<Outcome?, Never>?
        var timerToCancel: Task<Void, Never>?
        withLock {
            guard var operation = operations[operationID],
                  operation.incarnation == incarnation,
                  !operation.released,
                  isPending(operation.outcome),
                  Self.monotonicNow() >= operation.deadlineUptimeNanoseconds else { return }
            operation.outcome = .failure(.deadlineExceeded)
            workerToCancel = operation.worker
            timerToCancel = operation.timer
            operation.timer = nil
            operations[operationID] = operation
        }
        workerToCancel?.cancel()
        timerToCancel?.cancel()
    }

    private func isActive(_ operationID: UUID, incarnation: UUID) -> Bool {
        var timerToCancel: Task<Void, Never>?
        let active = withLock { () -> Bool in
            guard var operation = operations[operationID],
                  operation.incarnation == incarnation,
                  !operation.released,
                  isPending(operation.outcome) else { return false }
            if Self.monotonicNow() >= operation.deadlineUptimeNanoseconds {
                operation.outcome = .failure(.deadlineExceeded)
                timerToCancel = operation.timer
                operation.timer = nil
                operations[operationID] = operation
                return false
            }
            return true
        }
        timerToCancel?.cancel()
        return active
    }

    private func remainingMilliseconds(for operationID: UUID, incarnation: UUID) -> Int? {
        var timerToCancel: Task<Void, Never>?
        let remaining = withLock { () -> Int? in
            guard var operation = operations[operationID],
                  operation.incarnation == incarnation,
                  !operation.released,
                  isPending(operation.outcome) else { return nil }

            let now = Self.monotonicNow()
            guard now < operation.deadlineUptimeNanoseconds else {
                operation.outcome = .failure(.deadlineExceeded)
                timerToCancel = operation.timer
                operation.timer = nil
                operations[operationID] = operation
                return nil
            }

            let wholeMilliseconds = (operation.deadlineUptimeNanoseconds - now) / 1_000_000
            guard wholeMilliseconds > 0,
                  wholeMilliseconds <= UInt64(Int.max) else {
                operation.outcome = .failure(.deadlineExceeded)
                timerToCancel = operation.timer
                operation.timer = nil
                operations[operationID] = operation
                return nil
            }
            return Int(wholeMilliseconds)
        }
        timerToCancel?.cancel()
        return remaining
    }

    private func finishWorker(_ operationID: UUID, incarnation: UUID, result: Outcome?) {
        var timerToCancel: Task<Void, Never>?
        withLock {
            guard var operation = operations[operationID], operation.incarnation == incarnation else { return }
            operation.workerReturned = true
            let physicalReturn = Self.monotonicNow()
            operation.physicalReturnUptimeNanoseconds = physicalReturn
            operation.worker = nil
            operation.workerObserver = nil
            timerToCancel = operation.timer
            operation.timer = nil

            if isPending(operation.outcome) {
                if operation.released {
                    operation.outcome = .failure(.cancelled)
                } else if physicalReturn >= operation.deadlineUptimeNanoseconds {
                    operation.outcome = .failure(.deadlineExceeded)
                } else {
                    operation.outcome = result ?? .failure(.unavailable)
                }
            }

            operations[operationID] = operation
            if operation.released {
                finalizeReleaseLocked(operationID, at: Self.monotonicNow())
            }
        }
        timerToCancel?.cancel()
    }

    private func snapshotLocked(
        _ operationID: UUID,
        workerToCancel: inout Task<Outcome?, Never>?,
        timerToCancel: inout Task<Void, Never>?
    ) -> DeviceModelReply {
        guard var operation = operations[operationID], !operation.released else {
            return .error(operationID: operationID, failure: .notFound)
        }

        if isPending(operation.outcome),
           Self.monotonicNow() >= operation.deadlineUptimeNanoseconds {
            operation.outcome = .failure(.deadlineExceeded)
            workerToCancel = operation.worker
            timerToCancel = operation.timer
            operation.timer = nil
            operations[operationID] = operation
        }

        switch operation.outcome {
        case .pending:
            return .pending(operationID)
        case .response(let response):
            return .done(response)
        case .failure(let failure):
            return .error(operationID: operationID, failure: failure)
        }
    }

    private func pruneExpiredTombstonesLocked(at now: UInt64) {
        tombstones = tombstones.filter { $0.value > now }
    }

    private func finalizeReleaseLocked(_ operationID: UUID, at now: UInt64) {
        guard let operation = operations[operationID],
              operation.released,
              operation.workerReturned else { return }
        operations.removeValue(forKey: operationID)

        reservedTombstones -= 1

        guard let returnedAt = operation.physicalReturnUptimeNanoseconds else { return }
        let (expiry, overflow) = returnedAt.addingReportingOverflow(tombstoneLifetimeNanoseconds)
        guard !overflow, expiry > now else { return }
        tombstones[operationID] = expiry
    }

    private func withLock<Value>(_ body: () -> Value) -> Value {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }

    private func isPending(_ outcome: Outcome) -> Bool {
        if case .pending = outcome {
            return true
        }
        return false
    }

    private static func monotonicNow() -> UInt64 {
        DispatchTime.now().uptimeNanoseconds
    }

    private static func sameRequest(_ lhs: DeviceModelRequest, _ rhs: DeviceModelRequest) -> Bool {
        let sameFormat: Bool
        switch (lhs.outputFormat, rhs.outputFormat) {
        case (.text, .text):
            sameFormat = true
        case (.json(let left), .json(let right)):
            sameFormat = left.json == right.json
        default:
            sameFormat = false
        }
        let sameTools = lhs.tools.count == rhs.tools.count &&
            zip(lhs.tools, rhs.tools).allSatisfy { pair in
                pair.0.name == pair.1.name &&
                    pair.0.description.utf8.elementsEqual(pair.1.description.utf8) &&
                    pair.0.inputSchema.json == pair.1.inputSchema.json
            }
        return lhs.operationID == rhs.operationID &&
            lhs.bindingID == rhs.bindingID &&
            lhs.instructions.utf8.elementsEqual(rhs.instructions.utf8) &&
            lhs.input == rhs.input &&
            sameFormat &&
            sameTools &&
            lhs.maxResponseTokens == rhs.maxResponseTokens &&
            lhs.maxOutputBytes == rhs.maxOutputBytes &&
            lhs.deadlineMilliseconds == rhs.deadlineMilliseconds
    }

    private static func request(
        _ request: DeviceModelRequest,
        withDeadlineMilliseconds deadlineMilliseconds: Int
    ) -> DeviceModelRequest {
        DeviceModelRequest(
            operationID: request.operationID,
            bindingID: request.bindingID,
            instructions: request.instructions,
            input: request.input,
            outputFormat: request.outputFormat,
            tools: request.tools,
            maxResponseTokens: request.maxResponseTokens,
            maxOutputBytes: request.maxOutputBytes,
            deadlineMilliseconds: deadlineMilliseconds
        )
    }

    private func requiredCapabilities(for request: DeviceModelRequest) -> [DeviceModelCapability] {
        var required: [DeviceModelCapability] = []
        switch request.outputFormat {
        case .text:
            required.append(.text)
        case .json(_):
            required.append(.structuredOutput)
        }
        if !request.tools.isEmpty {
            required.append(.toolProposals)
        }
        return required.sorted { Self.wireName($0) < Self.wireName($1) }
    }

    private static func wireName(_ capability: DeviceModelCapability) -> String {
        switch capability {
        case .text: return "text"
        case .structuredOutput: return "structured_output"
        case .toolProposals: return "tool_proposals"
        }
    }

    private func failure(for unavailable: DeviceModelUnavailable) -> DeviceModelFailure {
        switch unavailable {
        case .unsupported: return .unsupported
        case .disabled: return .disabled
        case .notReady: return .notReady
        }
    }

    private func failure(from error: Error, fallback: DeviceModelFailure) -> DeviceModelFailure {
        if let failure = error as? DeviceModelFailure {
            return failure
        }
        if error is CancellationError {
            return .cancelled
        }
        return fallback
    }
}

@_cdecl("floe_device_model")
public func floeDeviceModel(
    _ bytes: UnsafePointer<UInt8>?,
    _ length: Int
) -> UnsafeMutablePointer<CChar>? {
    let reply: DeviceModelReply
    guard let bytes, (1...131_072).contains(length) else {
        reply = .error(operationID: nil, failure: .invalidInput)
        return encodeDeviceModelReply(reply)
    }

    do {
        let command = try DeviceModelCodec.decodeCommand(Data(bytes: bytes, count: length))
        reply = NativeComposition.deviceModelHost.invoke(command)
    } catch let failure as DeviceModelFailure {
        reply = .error(operationID: nil, failure: failure)
    } catch {
        reply = .error(operationID: nil, failure: .invalidInput)
    }
    return encodeDeviceModelReply(reply)
}

@_cdecl("floe_device_model_free")
public func floeDeviceModelFree(_ output: UnsafeMutablePointer<CChar>?) {
    free(output)
}

private func encodeDeviceModelReply(_ reply: DeviceModelReply) -> UnsafeMutablePointer<CChar>? {
    guard let data = try? DeviceModelCodec.encodeReply(reply),
          data.count <= 65_536,
          let text = String(data: data, encoding: .utf8) else { return nil }
    return text.withCString { strdup($0) }
}
