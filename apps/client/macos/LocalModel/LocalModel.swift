import Foundation
import FoundationModels

struct LocalModelInput: Codable, Equatable, Sendable {
  let instructions: String
  let prompt: String
  let maxResponseTokens: Int
  let maxOutputBytes: Int
  let deadlineMilliseconds: Int

  var valid: Bool {
    !instructions.isEmpty && instructions.utf8.count <= 4096 &&
      !prompt.isEmpty && prompt.utf8.count <= 12288 &&
      (1...1024).contains(maxResponseTokens) &&
      (1...16384).contains(maxOutputBytes) &&
      (1...30000).contains(deadlineMilliseconds)
  }
}

struct LocalModelCommand: Codable {
  let schemaVersion: Int
  let operation: String
  let requestID: UUID?
  let input: LocalModelInput?
}

struct LocalModelStep: Codable, Sendable {
  let kind: String
  let text: String?
  let capabilityID: String?
  let input: String?
}

struct LocalModelReply: Encodable, Sendable {
  let schemaVersion = 1
  var status: String
  var requestID: UUID?
  var availability: String?
  var step: LocalModelStep?
  var failure: String?
}

struct LocalModelFailure: Error {
  let reason: String
  init(_ reason: String) { self.reason = reason }
}

final class LocalModelHost: @unchecked Sendable {
  typealias Generator = @Sendable (LocalModelInput) async throws -> LocalModelStep
  private struct Job {
    let requestID: UUID
    let input: LocalModelInput
    let deadline: UInt64
    var task: Task<Void, Never>?
    var timer: Task<Void, Never>?
    var reply: LocalModelReply?
    var cancelled: String?
    var abandoned = false
  }

  private let lock = NSLock()
  private var job: Job?
  private let availability: @Sendable () -> String
  private let generate: Generator

  init(availability: @escaping @Sendable () -> String, generate: @escaping Generator) {
    self.availability = availability
    self.generate = generate
  }

  func invoke(_ command: LocalModelCommand) -> LocalModelReply {
    lock.lock()
    defer { lock.unlock() }
    guard command.schemaVersion == 1 else { return failure("unsupported_version") }
    if command.operation == "availability" {
      guard command.requestID == nil && command.input == nil else { return failure("invalid_input") }
      return LocalModelReply(status: "availability", availability: availability())
    }
    guard let requestID = command.requestID else { return failure("invalid_input") }
    if command.operation == "start" {
      guard let input = command.input, input.valid else { return failure("invalid_input", requestID) }
      if let current = job {
        guard current.requestID == requestID && current.input == input && !current.abandoned else {
          return failure("conflict", requestID)
        }
        return snapshot(current)
      }
      let available = availability()
      guard available == "available" else {
        return LocalModelReply(status: "error", requestID: requestID,
                               availability: available, failure: "model_unavailable")
      }
      job = Job(requestID: requestID, input: input,
                deadline: DispatchTime.now().uptimeNanoseconds + UInt64(input.deadlineMilliseconds) * 1_000_000)
      job?.task = Task.detached { [self] in
        let result: Result<LocalModelStep, LocalModelFailure>
        do {
          try Task.checkCancellation()
          let step = try await generate(input)
          try Task.checkCancellation()
          let size = try JSONEncoder().encode(step).count
          guard size <= input.maxOutputBytes else { throw LocalModelFailure("budget_exceeded") }
          result = .success(step)
        } catch is CancellationError {
          result = .failure(LocalModelFailure("cancelled"))
        } catch let error as LocalModelFailure {
          result = .failure(error)
        } catch {
          result = .failure(LocalModelFailure("model_unavailable"))
        }
        finish(requestID, result)
      }
      job?.timer = Task.detached { [self] in
        do {
          try await Task.sleep(nanoseconds: UInt64(input.deadlineMilliseconds) * 1_000_000)
          cancel(requestID, reason: "deadline_exceeded")
        } catch {}
      }
      return LocalModelReply(status: "pending", requestID: requestID)
    }
    guard command.input == nil else { return failure("invalid_input", requestID) }
    guard let current = job, current.requestID == requestID, !current.abandoned else {
      return failure("not_found", requestID)
    }
    switch command.operation {
    case "poll": return snapshot(current)
    case "cancel":
      if current.reply == nil {
        job?.cancelled = current.cancelled ?? "cancelled"
        current.task?.cancel()
      }
      return snapshot(job!)
    case "release":
      if current.reply != nil {
        job = nil
      } else {
        job?.abandoned = true
        job?.cancelled = current.cancelled ?? "cancelled"
        current.task?.cancel()
      }
      return LocalModelReply(status: "released", requestID: requestID)
    default: return failure("invalid_input", requestID)
    }
  }

  private func cancel(_ requestID: UUID, reason: String) {
    lock.lock()
    defer { lock.unlock() }
    guard let current = job, current.requestID == requestID, current.reply == nil else { return }
    job?.cancelled = current.cancelled ?? reason
    current.task?.cancel()
  }

  private func finish(_ requestID: UUID, _ result: Result<LocalModelStep, LocalModelFailure>) {
    lock.lock()
    defer { lock.unlock() }
    guard let current = job, current.requestID == requestID else { return }
    current.timer?.cancel()
    if current.abandoned { job = nil; return }
    if let reason = current.cancelled {
      job?.reply = failure(reason, requestID)
    } else if DispatchTime.now().uptimeNanoseconds >= current.deadline {
      job?.reply = failure("deadline_exceeded", requestID)
    } else {
      switch result {
      case .success(let step): job?.reply = LocalModelReply(status: "done", requestID: requestID, step: step)
      case .failure(let error): job?.reply = failure(error.reason, requestID)
      }
    }
    job?.task = nil
    job?.timer = nil
  }

  private func snapshot(_ current: Job) -> LocalModelReply {
    current.reply ?? LocalModelReply(status: "pending", requestID: current.requestID)
  }

  private func failure(_ reason: String, _ requestID: UUID? = nil) -> LocalModelReply {
    LocalModelReply(status: "error", requestID: requestID, failure: reason)
  }
}

@available(macOS 26.0, *)
@Generable
private struct GeneratedStep {
  @Guide(description: "Choose answer to respond, or call to request exactly one advertised read capability.", .anyOf(["answer", "call"]))
  var kind: String
  @Guide(description: "User-visible response for answer; empty for call. Never hidden reasoning.")
  var text: String
  @Guide(description: "An advertised capability ID for call; empty for answer.")
  var capabilityID: String
  @Guide(description: "A bounded input string for call; empty for answer.")
  var input: String
}

func foundationModelAvailability() -> String {
  guard #available(macOS 26.0, *) else { return "unsupported_os" }
  guard ProcessInfo.processInfo.operatingSystemVersion.majorVersion == 26 else {
    return "unsupported_profile"
  }
  switch SystemLanguageModel.default.availability {
  case .available: return "available"
  case .unavailable(.deviceNotEligible): return "device_not_eligible"
  case .unavailable(.appleIntelligenceNotEnabled): return "apple_intelligence_not_enabled"
  case .unavailable(.modelNotReady): return "model_not_ready"
  @unknown default: return "model_unavailable"
  }
}

private func foundationGenerate(_ input: LocalModelInput) async throws -> LocalModelStep {
  guard #available(macOS 26.0, *), foundationModelAvailability() == "available" else {
    throw LocalModelFailure("model_unavailable")
  }
  let session = LanguageModelSession(model: .default, tools: [], instructions: input.instructions)
  do {
    let response = try await session.respond(to: input.prompt, generating: GeneratedStep.self,
      options: GenerationOptions(sampling: .greedy, maximumResponseTokens: input.maxResponseTokens))
    let content = response.content
    switch content.kind {
    case "answer" where !content.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
      content.capabilityID.isEmpty && content.input.isEmpty:
      return LocalModelStep(kind: "answer", text: content.text, capabilityID: nil, input: nil)
    case "call" where content.text.isEmpty && !content.capabilityID.isEmpty:
      return LocalModelStep(kind: "call", text: nil, capabilityID: content.capabilityID, input: content.input)
    default: throw LocalModelFailure("invalid_model_output")
    }
  } catch let error as LanguageModelSession.GenerationError {
    switch error {
    case .exceededContextWindowSize: throw LocalModelFailure("budget_exceeded")
    case .guardrailViolation, .refusal: throw LocalModelFailure("policy_denied")
    case .decodingFailure, .unsupportedGuide: throw LocalModelFailure("invalid_model_output")
    case .rateLimited: throw LocalModelFailure("quota_exceeded")
    default: throw LocalModelFailure("model_unavailable")
    }
  }
}

private let foundationHost = LocalModelHost(availability: foundationModelAvailability,
                                          generate: foundationGenerate)

@_cdecl("floe_local_model")
public func floeLocalModel(_ bytes: UnsafePointer<UInt8>?, _ length: Int) -> UnsafeMutablePointer<CChar>? {
  let reply: LocalModelReply
  if let bytes, (1...32768).contains(length),
     let command = decodeLocalCommand(Data(bytes: bytes, count: length)) {
    reply = foundationHost.invoke(command)
  } else {
    reply = LocalModelReply(status: "error", failure: "invalid_input")
  }
  guard let data = try? JSONEncoder().encode(reply), let text = String(data: data, encoding: .utf8) else { return nil }
  return strdup(text)
}

func decodeLocalCommand(_ data: Data) -> LocalModelCommand? {
  guard let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
        Set(object.keys).isSubset(of: ["schemaVersion", "operation", "requestID", "input"]) else { return nil }
  if let input = object["input"] as? [String: Any],
     Set(input.keys) != ["instructions", "prompt", "maxResponseTokens", "maxOutputBytes", "deadlineMilliseconds"] {
    return nil
  }
  return try? JSONDecoder().decode(LocalModelCommand.self, from: data)
}

@_cdecl("floe_local_model_free")
public func floeLocalModelFree(_ output: UnsafeMutablePointer<CChar>?) {
  free(output)
}
