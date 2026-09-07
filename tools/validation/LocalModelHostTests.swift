import Foundation

private actor Gate {
  private var continuation: CheckedContinuation<Void, Never>?
  private var open = false
  private(set) var started = false

  func wait() async {
    if open { return }
    started = true
    await withCheckedContinuation { continuation = $0 }
  }

  func finish() {
    open = true
    continuation?.resume()
    continuation = nil
  }
}

@main
struct LocalModelHostTests {
  static func input(deadline: Int = 1000, output: Int = 16384) -> LocalModelInput {
    LocalModelInput(instructions: "Synthetic test", prompt: "Synthetic question",
                    maxResponseTokens: 64, maxOutputBytes: output, deadlineMilliseconds: deadline)
  }

  static func command(_ operation: String, _ requestID: UUID, _ input: LocalModelInput? = nil) -> LocalModelCommand {
    LocalModelCommand(schemaVersion: 1, operation: operation, requestID: requestID, input: input)
  }

  static func answer() -> LocalModelStep {
    LocalModelStep(kind: "answer", text: "Synthetic answer", capabilityID: nil, input: nil)
  }

  static func terminal(_ host: LocalModelHost, _ requestID: UUID) async throws -> LocalModelReply {
    for _ in 0..<200 {
      let reply = host.invoke(command("poll", requestID))
      if reply.status != "pending" { return reply }
      try await Task.sleep(nanoseconds: 5_000_000)
    }
    fatalError("fixture did not finish")
  }

  private static func waitForStart(_ gate: Gate) async throws {
    for _ in 0..<200 {
      if await gate.started { return }
      try await Task.sleep(nanoseconds: 5_000_000)
    }
    fatalError("fixture did not start")
  }

  static func main() async throws {
    let unavailable = LocalModelHost(availability: { "apple_intelligence_not_enabled" }, generate: { _ in
      fatalError("unavailable model must not generate")
    })
    let denied = unavailable.invoke(command("start", UUID(), input()))
    precondition(denied.failure == "model_unavailable" && denied.availability == "apple_intelligence_not_enabled")

    let host = LocalModelHost(availability: { "available" }, generate: { _ in answer() })
    let requestID = UUID()
    precondition(host.invoke(command("start", requestID, input())).status == "pending")
    let completed = try await terminal(host, requestID)
    precondition(completed.status == "done" && completed.step?.text == "Synthetic answer")
    precondition(host.invoke(command("start", requestID, input())).status == "done")
    precondition(host.invoke(command("start", UUID(), input())).failure == "conflict")
    precondition(host.invoke(command("poll", UUID())).failure == "not_found")
    precondition(host.invoke(command("release", requestID)).status == "released")

    let oversizedID = UUID()
    precondition(host.invoke(command("start", oversizedID, input(output: 1))).status == "pending")
    let oversized = try await terminal(host, oversizedID)
    precondition(oversized.failure == "budget_exceeded")
    _ = host.invoke(command("release", oversizedID))

    let cancelGate = Gate()
    let cancelHost = LocalModelHost(availability: { "available" }, generate: { _ in
      await cancelGate.wait()
      return answer()
    })
    let cancelledID = UUID()
    _ = cancelHost.invoke(command("start", cancelledID, input()))
    try await waitForStart(cancelGate)
    precondition(cancelHost.invoke(command("cancel", cancelledID)).status == "pending")
    await cancelGate.finish()
    let cancelled = try await terminal(cancelHost, cancelledID)
    precondition(cancelled.failure == "cancelled" && cancelled.step == nil)
    _ = cancelHost.invoke(command("release", cancelledID))

    let gate = Gate()
    let blocked = LocalModelHost(availability: { "available" }, generate: { _ in
      await gate.wait()
      return answer()
    })
    let blockedID = UUID()
    precondition(blocked.invoke(command("start", blockedID, input())).status == "pending")
    try await waitForStart(gate)
    precondition(blocked.invoke(command("cancel", blockedID)).status == "pending")
    precondition(blocked.invoke(command("release", blockedID)).status == "released")
    precondition(blocked.invoke(command("start", UUID(), input())).failure == "conflict")
    await gate.finish()
    let nextID = UUID()
    var next: LocalModelReply?
    for _ in 0..<200 {
      next = blocked.invoke(command("start", nextID, input()))
      if next?.status == "pending" { break }
      try await Task.sleep(nanoseconds: 5_000_000)
    }
    precondition(next?.status == "pending")
    _ = blocked.invoke(command("release", nextID))
    await gate.finish()

    let deadlineGate = Gate()
    let deadlineHost = LocalModelHost(availability: { "available" }, generate: { _ in
      await deadlineGate.wait()
      return answer()
    })
    let deadlineID = UUID()
    _ = deadlineHost.invoke(command("start", deadlineID, input(deadline: 50)))
    try await waitForStart(deadlineGate)
    try await Task.sleep(nanoseconds: 80_000_000)
    precondition(deadlineHost.invoke(command("poll", deadlineID)).status == "pending")
    precondition(deadlineHost.invoke(command("start", UUID(), input())).failure == "conflict")
    await deadlineGate.finish()
    let timedOut = try await terminal(deadlineHost, deadlineID)
    precondition(timedOut.failure == "deadline_exceeded")
    _ = deadlineHost.invoke(command("release", deadlineID))

    precondition(decodeLocalCommand(Data(#"{"schemaVersion":1,"operation":"availability","secret":"must not echo"}"#.utf8)) == nil)
    precondition(decodeLocalCommand(Data(#"{"schemaVersion":1,"operation":"availability"}"#.utf8)) != nil)
    let malformed = floeLocalModel(nil, 0)!
    precondition(String(cString: malformed).contains("invalid_input"))
    floeLocalModelFree(malformed)
    let invalid = Array(#"{"schemaVersion":1,"operation":"availability","secret":"do-not-echo"}"#.utf8)
    invalid.withUnsafeBufferPointer { buffer in
      let output = floeLocalModel(buffer.baseAddress, buffer.count)!
      let text = String(cString: output)
      precondition(text.contains("invalid_input") && !text.contains("do-not-echo"))
      floeLocalModelFree(output)
    }
    print("LocalModelHost: availability, structured answer, replay, ownership, output, cancellation, deadline, strict decoding passed")
  }
}
