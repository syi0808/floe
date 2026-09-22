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

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
@Generable
private struct GeneratedAnswer {
  @Guide(description: "A complete user-visible response. Never hidden reasoning.")
  var text: String
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
@Generable
enum GeneratedLearnerObservationKind {
  case explicitRemember
  case userCorrection
  case outcomeConflict
  case reusableProcedure
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
@Generable
enum GeneratedLearnerMemoryKind {
  case fact
  case observation
  case inference
  case preference
  case commitment
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
@Generable
enum GeneratedLearnerEpistemicStatus {
  case fact
  case inference
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
@Generable
struct GeneratedLearnerMemoryValue {
  var kind: GeneratedLearnerMemoryKind
  @Guide(description: "Copy the exact named person or entity from the evidence. Use the user only when no other subject is named.")
  var subject: String
  @Guide(description: "Only the claim after the subject, such as 'prefers tea'. Do not omit or rename the named subject from the evidence.")
  var claim: String
  var epistemicStatus: GeneratedLearnerEpistemicStatus
  @Guide(description: "Confidence as an integer from 0 to 1000, for example 950.", .range(0...1000))
  var confidenceMillis: Int
  @Guide(description: "Choose expires when evidence gives only an expiry, starts for only a start date, range for both, and timeless only when neither is stated. Copy explicit dates exactly; never discard a stated expiry.")
  var validity: GeneratedLearnerValidity
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
@Generable
enum GeneratedLearnerValidity {
  case timeless
  case starts(validFrom: String)
  case expires(validUntil: String)
  case range(validFrom: String, validUntil: String)
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
@Generable
struct GeneratedLearnerProposal {
  var observationKind: GeneratedLearnerObservationKind
  var value: GeneratedLearnerMemoryValue
  @Guide(description: "For a new memory, nil. For a revision, the exact UUID of an existing memory in current context. Never a person's name.")
  var targetID: String?
  @Guide(description: "For a new memory, nil. For a revision, the positive revision number of the existing target memory.")
  var baseRevision: Int?
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
@Generable
struct GeneratedLearnerAnswer {
  var proposal: GeneratedLearnerProposal?
}

func foundationModelAvailability() -> String {
  #if os(macOS)
  guard #available(macOS 26.0, *) else { return "unsupported_os" }
  #elseif os(iOS)
  guard #available(iOS 26.0, *) else { return "unsupported_os" }
  #else
  return "unsupported_os"
  #endif
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
  #if os(macOS)
  guard #available(macOS 26.0, *), foundationModelAvailability() == "available" else {
    throw LocalModelFailure("model_unavailable")
  }
  #elseif os(iOS)
  guard #available(iOS 26.0, *), foundationModelAvailability() == "available" else {
    throw LocalModelFailure("model_unavailable")
  }
  #else
  throw LocalModelFailure("model_unavailable")
  #endif
  do {
    let options = GenerationOptions(sampling: .greedy, maximumResponseTokens: input.maxResponseTokens)
    switch learnerPromptClassification(input.prompt) {
    case .learner:
      let session = LanguageModelSession(model: .default, tools: [], instructions: input.instructions)
      let response = try await session.respond(to: input.prompt, generating: GeneratedLearnerAnswer.self,
        options: options)
      let text = try learnerOutputText(response.content)
      return LocalModelStep(kind: "answer", text: text, capabilityID: nil, input: nil)
    case .denied:
      throw LocalModelFailure("policy_denied")
    case .general:
      break
    }
    let actionTools = try nativeActionTools(input.prompt)
    let nativeInstructions = actionTools.isEmpty ? input.instructions : input.instructions + """

      Use a tool only when the current user request strictly requires external evidence or expert
      judgment. Never call a tool for a greeting, general conversation, or when the current user
      explicitly says not to use tools or delegate. Otherwise answer the user directly.
      """
    let session = LanguageModelSession(model: .default, tools: actionTools,
                                       instructions: nativeInstructions)
    guard let currentUserRequest = currentUserRequest(input.prompt) else {
      throw LocalModelFailure("invalid_model_output")
    }
    let nativePrompt = """
      Current user request:
      \(currentUserRequest)

      The canonical request context follows as JSON. Use it to answer the exact current request.

      \(input.prompt)
      """
    if actionTools.isEmpty {
      let response = try await session.respond(to: nativePrompt, generating: GeneratedAnswer.self,
        options: options)
      let text = response.content.text
      guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
        throw LocalModelFailure("invalid_model_output")
      }
      return LocalModelStep(kind: "answer", text: text, capabilityID: nil, input: nil)
    }
    let response = try await session.respond(to: nativePrompt, generating: GeneratedAnswer.self,
      options: options)
    let text = response.content.text
    guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
      throw LocalModelFailure("invalid_model_output")
    }
    return LocalModelStep(kind: "answer", text: text, capabilityID: nil, input: nil)
  } catch let error as LanguageModelSession.ToolCallError {
    guard let action = error.underlyingError as? NativeActionRequest else {
      throw LocalModelFailure("invalid_model_output")
    }
    return LocalModelStep(kind: "call", text: nil, capabilityID: action.capabilityID,
                          input: action.input)
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

enum LearnerPromptClassification: Equatable {
  case learner
  case denied
  case general
}

func learnerPromptClassification(_ prompt: String) -> LearnerPromptClassification {
  guard let data = prompt.data(using: .utf8),
        let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        let scoped = root["scoped_instructions"] as? [String: Any] else { return .general }
  guard scoped["purpose"] as? String == "governed-memory-review" else { return .general }
  guard let capabilities = scoped["available_capabilities"] as? [Any],
        let experts = scoped["active_experts"] as? [Any] else { return .denied }
  return capabilities.isEmpty && experts.isEmpty ? .learner : .denied
}

private func learnerValidityDate(_ timestamp: String) -> Date? {
  let formatter = ISO8601DateFormatter()
  if let date = formatter.date(from: timestamp) { return date }
  formatter.formatOptions.insert(.withFractionalSeconds)
  return formatter.date(from: timestamp)
}

func isLearnerRequest(_ prompt: String) -> Bool {
  learnerPromptClassification(prompt) == .learner
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
func learnerOutputText(_ answer: GeneratedLearnerAnswer) throws -> String {
  var output: [String: Any] = ["schema_version": 1]
  if let proposal = answer.proposal {
    output["proposal"] = try learnerProposalObject(proposal)
  } else {
    output["proposal"] = NSNull()
  }
  guard JSONSerialization.isValidJSONObject(output) else {
    throw LocalModelFailure("invalid_model_output")
  }
  let data = try JSONSerialization.data(withJSONObject: output, options: [.sortedKeys])
  guard let text = String(data: data, encoding: .utf8), !text.isEmpty else {
    throw LocalModelFailure("invalid_model_output")
  }
  return text
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
private func learnerProposalObject(_ proposal: GeneratedLearnerProposal) throws -> [String: Any] {
  let target: Any
  let revision: Any
  switch (proposal.targetID, proposal.baseRevision) {
  case (nil, nil):
    target = NSNull()
    revision = NSNull()
  case let (.some(targetID), .some(baseRevision)):
    guard UUID(uuidString: targetID) != nil, baseRevision > 0 else {
      throw LocalModelFailure("invalid_model_output")
    }
    target = targetID
    revision = baseRevision
  default:
    throw LocalModelFailure("invalid_model_output")
  }
  let value = proposal.value
  let subject = value.subject.trimmingCharacters(in: .whitespacesAndNewlines)
  let claim = value.claim.trimmingCharacters(in: .whitespacesAndNewlines)
  guard !subject.isEmpty, !claim.isEmpty,
        (0...1000).contains(value.confidenceMillis),
        (value.epistemicStatus == .inference) == (value.kind == .inference) else {
    throw LocalModelFailure("invalid_model_output")
  }
  let validFrom: String?
  let validUntil: String?
  switch value.validity {
  case .timeless:
    validFrom = nil
    validUntil = nil
  case let .starts(from):
    guard learnerValidityDate(from) != nil,
          !from.isEmpty else {
      throw LocalModelFailure("invalid_model_output")
    }
    validFrom = from
    validUntil = nil
  case let .expires(until):
    guard learnerValidityDate(until) != nil,
          !until.isEmpty else {
      throw LocalModelFailure("invalid_model_output")
    }
    validUntil = until
    validFrom = nil
  case let .range(from, until):
    guard learnerValidityDate(from) != nil,
          learnerValidityDate(until) != nil,
          let parsedFrom = learnerValidityDate(from),
          let parsedUntil = learnerValidityDate(until), parsedUntil > parsedFrom else {
      throw LocalModelFailure("invalid_model_output")
    }
    validFrom = from
    validUntil = until
  }
  return [
    "observation_kind": learnerObservationKindName(proposal.observationKind),
    "value": [
      "kind": learnerMemoryKindName(value.kind),
      "statement": "\(subject): \(claim)",
      "epistemic_status": learnerEpistemicStatusName(value.epistemicStatus),
      "confidence_millis": value.confidenceMillis,
      "valid_from": validFrom.map { $0 as Any } ?? NSNull(),
      "valid_until": validUntil.map { $0 as Any } ?? NSNull(),
      "observed_at": "1970-01-01T00:00:00Z",
    ],
    "target_id": target,
    "base_revision": revision,
  ]
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
private func learnerObservationKindName(_ value: GeneratedLearnerObservationKind) -> String {
  switch value {
  case .explicitRemember: return "explicit_remember"
  case .userCorrection: return "user_correction"
  case .outcomeConflict: return "outcome_conflict"
  case .reusableProcedure: return "reusable_procedure"
  }
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
private func learnerMemoryKindName(_ value: GeneratedLearnerMemoryKind) -> String {
  switch value {
  case .fact: return "fact"
  case .observation: return "observation"
  case .inference: return "inference"
  case .preference: return "preference"
  case .commitment: return "commitment"
  }
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
private func learnerEpistemicStatusName(_ value: GeneratedLearnerEpistemicStatus) -> String {
  switch value {
  case .fact: return "fact"
  case .inference: return "inference"
  }
}

func currentUserRequest(_ prompt: String) -> String? {
  guard let data = prompt.data(using: .utf8),
        let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        let conversation = root["conversation"] as? [String: Any],
        let currentTurn = conversation["current_turn"] as? [[String: Any]],
        let content = currentTurn.last(where: { $0["role"] as? String == "user" })?["content"]
          as? String, !content.isEmpty else { return nil }
  return content
}

func currentUserActionRestrictions(_ prompt: String) -> (tools: Bool, delegation: Bool) {
  let request = currentUserRequest(prompt)?.lowercased() ?? ""
  let tools = ["do not use tools", "don't use tools", "do not call tools", "don't call tools",
               "without using tools"].contains { request.contains($0) }
  let delegation = ["do not delegate", "don't delegate", "without delegating", "tools or delegate"]
    .contains { request.contains($0) }
  return (tools, delegation)
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
private struct NativeActionRequest: Error, Sendable {
  let capabilityID: String
  let input: String
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
private struct NativeActionTool: Tool {
  let name: String
  let description: String
  let parameters: GenerationSchema
  let capabilityID: String

  func call(arguments: GeneratedContent) async throws -> String {
    throw NativeActionRequest(capabilityID: capabilityID, input: arguments.jsonString)
  }
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
func nativeActionTools(_ prompt: String) throws -> [any Tool] {
  guard let data = prompt.data(using: .utf8),
        let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
        let scoped = root["scoped_instructions"] as? [String: Any] else {
    throw LocalModelFailure("invalid_model_output")
  }
  let capabilities = scoped["available_capabilities"] as? [[String: Any]] ?? []
  let restrictions = currentUserActionRestrictions(prompt)
  var tools: [any Tool] = []
  for (index, capability) in (restrictions.tools ? [] : capabilities).enumerated() {
    guard let capabilityID = capability["id"] as? String, !capabilityID.isEmpty else {
      throw LocalModelFailure("invalid_model_output")
    }
    let inputSchema = capability["input_schema"] as? [String: Any] ?? ["type": "object"]
    let rootSchema = try dynamicSchema(inputSchema, name: "arguments_\(index)")
    tools.append(NativeActionTool(
      name: "floe_capability_\(index)",
      description: "Call the exact Floe capability \(capabilityID) only when the user request requires it.",
      parameters: try GenerationSchema(root: rootSchema, dependencies: []),
      capabilityID: capabilityID
    ))
  }
  let experts = scoped["active_experts"] as? [[String: Any]] ?? []
  let expertIDs = experts.compactMap { $0["id"] as? String }.filter { !$0.isEmpty }.sorted()
  if !restrictions.delegation && !expertIDs.isEmpty {
    let schema: [String: Any] = [
      "type": "object",
      "properties": [
        "agent_id": ["type": "string", "enum": expertIDs],
        "message": ["type": "string"],
        "context_refs": ["type": "array", "items": ["type": "string"]],
      ],
      "required": ["agent_id", "message"],
    ]
    tools.append(NativeActionTool(
      name: "floe_delegate",
      description: "Delegate only when the user request requires one of the available Floe experts.",
      parameters: try GenerationSchema(
        root: dynamicSchema(schema, name: "delegation_arguments"), dependencies: []),
      capabilityID: "floe.a2a.delegate"
    ))
  }
  return tools
}

#if os(macOS)
@available(macOS 26.0, *)
#elseif os(iOS)
@available(iOS 26.0, *)
#endif
private func dynamicSchema(_ schema: [String: Any], name: String) throws -> DynamicGenerationSchema {
  switch schema["type"] as? String {
  case "object", nil:
    let properties = schema["properties"] as? [String: [String: Any]] ?? [:]
    let required = Set(schema["required"] as? [String] ?? [])
    return DynamicGenerationSchema(
      name: name,
      properties: try properties.keys.sorted().map { property in
        DynamicGenerationSchema.Property(
          name: property,
          schema: try dynamicSchema(properties[property]!, name: "\(name)_\(property)"),
          isOptional: !required.contains(property)
        )
      }
    )
  case "array":
    guard let items = schema["items"] as? [String: Any] else {
      throw LocalModelFailure("invalid_model_output")
    }
    return DynamicGenerationSchema(arrayOf: try dynamicSchema(items, name: "\(name)_item"))
  case "string":
    if let choices = schema["enum"] as? [String], !choices.isEmpty {
      return DynamicGenerationSchema(type: String.self, guides: [.anyOf(choices)])
    }
    return DynamicGenerationSchema(type: String.self)
  case "integer":
    return DynamicGenerationSchema(type: Int.self)
  case "number":
    return DynamicGenerationSchema(type: Double.self)
  case "boolean":
    return DynamicGenerationSchema(type: Bool.self)
  default:
    throw LocalModelFailure("invalid_model_output")
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
