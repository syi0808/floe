import Darwin
import Foundation
import FloeHealthTransform

public protocol HealthTransformOperationClient: Sendable {
    func perform(
        _ input: HealthTransformInput,
        binding: HealthTransformBinding
    ) async throws -> HealthTransformSuccess
}

/// Transports Health operations to the receipt host in the bundled native image.
/// The client owns no job, receipt, source or model state.
public final class BundledHealthTransformClient: HealthTransformOperationClient, @unchecked Sendable {
    private typealias Call = @convention(c) (UnsafePointer<UInt8>?, Int) -> UnsafeMutablePointer<CChar>?
    private typealias Release = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void

    // A released operation may still have a noncooperative native worker or an
    // unconsumed receipt. Client destruction must never unload its host image.
    // This cache owns only the immutable loader handle and C entry points.
    private final class ResolvedLibrary: @unchecked Sendable {
        let handle: UnsafeMutableRawPointer
        let call: Call
        let release: Release

        init() throws {
            guard let frameworks = Bundle.main.privateFrameworksURL,
                  let handle = dlopen(
                    frameworks.appendingPathComponent("libfloe_local_model.dylib").path,
                    RTLD_NOW | RTLD_LOCAL
                  ) else {
                throw HealthTransformFailure.modelUnavailable
            }
            guard let entry = dlsym(handle, "floe_health_privacy_transform"),
                  let freeEntry = dlsym(handle, "floe_health_privacy_transform_free") else {
                dlclose(handle)
                throw HealthTransformFailure.modelUnavailable
            }
            self.handle = handle
            call = unsafeBitCast(entry, to: Call.self)
            release = unsafeBitCast(freeEntry, to: Release.self)
        }
    }

    private static let resolution: Result<ResolvedLibrary, HealthTransformFailure> = {
        do { return .success(try ResolvedLibrary()) }
        catch { return .failure(.modelUnavailable) }
    }()
    private let library: ResolvedLibrary

    public init() throws {
        library = try Self.resolution.get()
    }

    public func perform(
        _ input: HealthTransformInput,
        binding: HealthTransformBinding
    ) async throws -> HealthTransformSuccess {
        do {
            try input.validate()
            try binding.validate()
        } catch {
            throw HealthTransformFailure.invalidInput
        }
        try Task.checkCancellation()

        let operationID = UUID()
        let start = HealthTransformCommand(
            operation: "start",
            requestID: operationID,
            input: input,
            binding: binding
        )
        let initialReply = invoke(start)
        defer {
            _ = invoke(HealthTransformCommand(operation: "release", requestID: operationID))
        }

        return try await withTaskCancellationHandler {
            var reply = initialReply
            while true {
                try Task.checkCancellation()
                guard reply.requestID == operationID else {
                    throw HealthTransformFailure.invalidOutput
                }
                switch reply.status {
                case "done":
                    guard let output = reply.output,
                          reply.binding == binding,
                          let digest = reply.outputSHA256,
                          digest == HealthTransformDigest.hex(output),
                          let transformedAtUnixMs = reply.transformedAtUnixMs,
                          let expiresAtUnixMs = reply.expiresAtUnixMs else {
                        throw HealthTransformFailure.invalidOutput
                    }
                    let (expectedExpiry, overflow) = transformedAtUnixMs.addingReportingOverflow(
                        30 * 60 * 1_000
                    )
                    let now = Self.nowUnixMilliseconds()
                    guard !overflow,
                          transformedAtUnixMs > 0,
                          expiresAtUnixMs == expectedExpiry,
                          transformedAtUnixMs <= now,
                          expiresAtUnixMs > now else {
                        throw HealthTransformFailure.invalidOutput
                    }
                    try Task.checkCancellation()
                    return HealthTransformSuccess(
                        output: output,
                        proof: HealthTransformProof(operationID: operationID, outputSHA256: digest),
                        transformedAtUnixMs: transformedAtUnixMs,
                        expiresAtUnixMs: expiresAtUnixMs
                    )
                case "error":
                    throw reply.failure ?? HealthTransformFailure.invalidOutput
                case "pending":
                    try await Task.sleep(nanoseconds: 20_000_000)
                    reply = invoke(HealthTransformCommand(operation: "poll", requestID: operationID))
                default:
                    throw HealthTransformFailure.invalidOutput
                }
            }
        } onCancel: {
            _ = self.invoke(HealthTransformCommand(operation: "cancel", requestID: operationID))
        }
    }

    private func invoke(_ command: HealthTransformCommand) -> HealthTransformReply {
        let invalid = HealthTransformReply(
            status: "error",
            requestID: command.requestID,
            failure: .invalidOutput
        )
        guard let data = try? HealthTransformWireCodec.encodeCommand(command),
              data.count <= HealthTransformWireLimits.commandBytes else {
            return invalid
        }
        let output = data.withUnsafeBytes { bytes in
            library.call(bytes.bindMemory(to: UInt8.self).baseAddress, data.count)
        }
        guard let output else { return invalid }
        defer { library.release(output) }

        var count = 0
        while count <= HealthTransformWireLimits.replyBytes && output[count] != 0 {
            count += 1
        }
        guard count <= HealthTransformWireLimits.replyBytes else { return invalid }
        let bytes = Data(bytes: output, count: count)
        return (try? HealthTransformWireCodec.decodeReply(bytes)) ?? invalid
    }

    private static func nowUnixMilliseconds() -> Int64 {
        Int64(Date().timeIntervalSince1970 * 1_000)
    }
}
