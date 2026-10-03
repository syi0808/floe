import Foundation
import EventKit
import CryptoKit
import CoreFoundation

private let eventStoreLock = NSLock()
private let maxCalendarReadItems = 128
private let maxCalendarReadBytes = 65_536
private let maxNativeActionEntries = 128
private let nativeReceiptRetention: TimeInterval = 15 * 60
private let nativeHostEpoch = UUID().uuidString.lowercased()

private final class CalendarViewGeneration: @unchecked Sendable {
  private let lock = NSLock()
  private var current = UUID().uuidString
  private var observer: NSObjectProtocol?

  init() {
    observer = NotificationCenter.default.addObserver(forName: .EKEventStoreChanged, object: nil, queue: nil) { [weak self] _ in
      self?.invalidate()
    }
  }

  private func invalidate() {
    lock.lock()
    current = UUID().uuidString
    lock.unlock()
  }

  func value() -> String {
    lock.lock()
    defer { lock.unlock() }
    return current
  }

  deinit { if let observer = observer { NotificationCenter.default.removeObserver(observer) } }
}

private let calendarViewGeneration = CalendarViewGeneration()

private struct NativeFailure: Error {
  let reason: String
  init(_ reason: String) { self.reason = reason }
}

private func exactKeys(_ value: [String: Any], _ keys: Set<String>) -> Bool {
  return Set(value.keys) == keys
}

private func actionText(_ value: Any?, maximum: Int, allowEmpty: Bool = false) -> String? {
  guard let text = value as? String,
        (allowEmpty || !text.isEmpty),
        text.utf8.count <= maximum,
        text.trimmingCharacters(in: .whitespacesAndNewlines) == text,
        !text.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
    return nil
  }
  return text
}

private func rawEventTitle(_ value: Any?, maximum: Int) -> String? {
  guard let text = value as? String, text.utf8.count <= maximum else { return nil }
  return text
}

private func jsonUInt(_ value: Any?) -> UInt64? {
  guard let number = value as? NSNumber,
        CFGetTypeID(number) != CFBooleanGetTypeID(),
        let value = UInt64(number.stringValue),
        String(value) == number.stringValue else { return nil }
  return value
}

private func jsonBool(_ value: Any?) -> Bool? {
  guard let number = value as? NSNumber,
        CFGetTypeID(number) == CFBooleanGetTypeID() else { return nil }
  return number.boolValue
}

private func digestBytes(_ value: Any?) -> [Int]? {
  guard let bytes = value as? [Any], bytes.count == 32 else { return nil }
  let result = bytes.compactMap { item -> Int? in
    guard let byte = jsonUInt(item), byte <= 255 else { return nil }
    return Int(byte)
  }
  return result.count == 32 ? result : nil
}

private func canonicalJSON(_ value: Any) -> Data? {
  guard JSONSerialization.isValidJSONObject(value) else { return nil }
  return try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
}

private func sameJSON(_ left: Any, _ right: Any) -> Bool {
  guard let leftBytes = canonicalJSON(left), let rightBytes = canonicalJSON(right) else { return false }
  return leftBytes == rightBytes
}

private func validUUID(_ value: Any?, nonNil: Bool = true) -> String? {
  guard let text = value as? String, let id = UUID(uuidString: text),
        id.uuidString.lowercased() == text,
        !nonNil || id != UUID(uuidString: "00000000-0000-0000-0000-000000000000")! else {
    return nil
  }
  return text
}

private func actionDate(_ value: Any?) -> Date? {
  guard let text = value as? String,
        text.hasSuffix("Z") || text.hasSuffix("+00:00") else { return nil }
  let formatter = ISO8601DateFormatter()
  formatter.timeZone = TimeZone(secondsFromGMT: 0)
  formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  if let value = formatter.date(from: text) { return value }
  formatter.formatOptions = [.withInternetDateTime]
  return formatter.date(from: text)
}

private func actionTimestamp(_ date: Date) -> String {
  let formatter = ISO8601DateFormatter()
  formatter.timeZone = TimeZone(secondsFromGMT: 0)
  formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  return formatter.string(from: date)
}

private func wholeSecond(_ value: Date) -> Bool {
  return abs(value.timeIntervalSince1970 - value.timeIntervalSince1970.rounded()) < 0.000001
}

private func sha256Hex(_ data: Data) -> String {
  return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

private struct NativeTimedSchedule {
  let start: Date
  let end: Date
  let timezone: String
}

private struct NativeDestination {
  let connectionID: String
  let connectionRevision: UInt64
  let calendarID: String
}

private struct NativeCalendarTarget {
  let raw: [String: Any]
  let eventID: String
  let title: String
  let schedule: NativeTimedSchedule
  let externalID: String
  let externalRevision: String
}

private struct NativeCalendarEffect {
  let raw: [String: Any]
  let kind: String
  let destination: NativeDestination
  let title: String?
  let schedule: NativeTimedSchedule?
  let target: NativeCalendarTarget?
}

private struct NativeSourceFence {
  let raw: [String: Any]
  let connectionID: String
  let revision: UInt64
  let resources: [String]
  let subjectFingerprint: String
}

private struct NativeEffectIdentity {
  let raw: [String: Any]
  let executionID: String
  let effectDigest: [Int]
  let personID: String
  let deviceID: String
  let executorGeneration: UInt64
  let connectionID: String
  let calendarID: String
}

private struct NativeExecutionIntent {
  let raw: [String: Any]
  let identity: NativeEffectIdentity
  let effect: NativeCalendarEffect
  let source: NativeSourceFence
  let authorizationExpiresAtRaw: String
  let authorizationExpiresAt: Date
}

private struct NativePreparation {
  let executionID: String
  let identity: [String: Any]
  let effect: [String: Any]
  let source: [String: Any]
  let localEvents: [[String: Any]]
  let authorizationExpiresAtRaw: String
  let authorizationExpiresAt: Date
  let preparationID: String
  let hostEpoch: String
  let nativeSubjectFingerprint: String
  let expiresAt: Date
}

private struct NativeInvocation {
  let executionID: String
  let identity: [String: Any]
  let intent: [String: Any]
  let preparationID: String?
  let hostEpoch: String?
  var outcome: [String: Any]?
  var completedAt: Date?
}

private func parseTimedSchedule(_ value: Any?, tagged: Bool) throws -> NativeTimedSchedule {
  guard let raw = value as? [String: Any] else { throw NativeFailure("executor_unavailable") }
  let expected: Set<String> = tagged
    ? ["kind", "starts_at", "ends_at", "timezone"]
    : ["starts_at", "ends_at", "timezone"]
  guard exactKeys(raw, expected),
        !tagged || raw["kind"] as? String == "timed",
        let startText = actionText(raw["starts_at"], maximum: 64),
        let endText = actionText(raw["ends_at"], maximum: 64),
        let timezone = actionText(raw["timezone"], maximum: 128),
        let start = actionDate(startText), let end = actionDate(endText),
        end > start, TimeZone(identifier: timezone) != nil else {
    throw NativeFailure("executor_unavailable")
  }
  return NativeTimedSchedule(start: start, end: end, timezone: timezone)
}

private func externalObservationFingerprint(_ value: Any?) -> String? {
  guard let revision = value as? [String: Any],
        exactKeys(revision, ["kind", "sha256"]),
        revision["kind"] as? String == "observation_fingerprint",
        let digest = revision["sha256"] as? String,
        digest.utf8.count == 64,
        digest.utf8.allSatisfy({ ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102) }),
        digest != String(repeating: "0", count: 64) else { return nil }
  return digest
}

private func parseDestination(_ value: Any?) throws -> NativeDestination {
  guard let raw = value as? [String: Any],
        exactKeys(raw, ["provider", "connection_id", "connection_revision", "calendar_id", "calendar_name"]),
        raw["provider"] as? String == "event_kit",
        let connectionID = actionText(raw["connection_id"], maximum: 256),
        let revision = jsonUInt(raw["connection_revision"]), revision > 0,
        let calendarID = actionText(raw["calendar_id"], maximum: 512),
        actionText(raw["calendar_name"], maximum: 512) != nil else {
    throw NativeFailure("policy_denied")
  }
  return NativeDestination(connectionID: connectionID, connectionRevision: revision,
                           calendarID: calendarID)
}

private func parseTarget(_ value: Any?, personID: String,
                         destination: NativeDestination) throws -> NativeCalendarTarget {
  guard let raw = value as? [String: Any], exactKeys(raw, ["original"]),
        let original = raw["original"] as? [String: Any],
        exactKeys(original, ["id", "person_id", "title", "schedule", "source", "created_at", "updated_at", "revision", "deleted_at"]),
        let eventID = validUUID(original["id"]),
        let originalPerson = validUUID(original["person_id"]), originalPerson == personID,
        let title = rawEventTitle(original["title"], maximum: 4096),
        let scheduleRaw = original["schedule"] as? [String: Any],
        scheduleRaw["kind"] as? String == "timed",
        let schedule = try? parseTimedSchedule(scheduleRaw, tagged: true),
        let created = actionDate(original["created_at"]),
        let updated = actionDate(original["updated_at"]), updated >= created,
        let revision = jsonUInt(original["revision"]), revision > 0,
        original["deleted_at"] is NSNull,
        let source = original["source"] as? [String: Any], exactKeys(source, ["Calendar"]),
        let calendar = source["Calendar"] as? [String: Any],
        exactKeys(calendar, ["can_modify", "connection_id", "provider", "calendar_id", "calendar_name", "external_id", "external_revision"]),
        jsonBool(calendar["can_modify"]) == true,
        calendar["provider"] as? String == "event_kit",
        let connectionID = actionText(calendar["connection_id"], maximum: 256), connectionID == destination.connectionID,
        let calendarID = actionText(calendar["calendar_id"], maximum: 512), calendarID == destination.calendarID,
        let calendarName = actionText(calendar["calendar_name"], maximum: 512),
        let externalID = actionText(calendar["external_id"], maximum: 1024), externalID.hasSuffix("|"),
        !String(externalID.dropLast()).isEmpty,
        let externalRevision = externalObservationFingerprint(calendar["external_revision"]) else {
    throw NativeFailure("source_changed")
  }
  _ = calendarName
  return NativeCalendarTarget(raw: raw, eventID: eventID, title: title,
                              schedule: schedule, externalID: externalID,
                              externalRevision: externalRevision)
}

private func parseEffect(_ value: Any?, personID: String) throws -> NativeCalendarEffect {
  guard let raw = value as? [String: Any], let kind = raw["kind"] as? String else {
    throw NativeFailure("policy_denied")
  }
  let destination = try parseDestination(raw["destination"])
  switch kind {
  case "create":
    guard exactKeys(raw, ["kind", "destination", "title", "schedule"]),
          let title = actionText(raw["title"], maximum: 1024),
          let schedule = try? parseTimedSchedule(raw["schedule"], tagged: false),
          wholeSecond(schedule.start), wholeSecond(schedule.end),
          schedule.end.timeIntervalSince(schedule.start) <= 86_400 else {
      throw NativeFailure("executor_unavailable")
    }
    return NativeCalendarEffect(raw: raw, kind: kind, destination: destination,
                                title: title, schedule: schedule, target: nil)
  case "update":
    guard exactKeys(raw, ["kind", "destination", "target", "title", "schedule"]),
          let title = actionText(raw["title"], maximum: 1024),
          let schedule = try? parseTimedSchedule(raw["schedule"], tagged: false),
          wholeSecond(schedule.start), wholeSecond(schedule.end),
          schedule.end.timeIntervalSince(schedule.start) <= 86_400 else {
      throw NativeFailure("executor_unavailable")
    }
    let target = try parseTarget(raw["target"], personID: personID, destination: destination)
    return NativeCalendarEffect(raw: raw, kind: kind, destination: destination,
                                title: title, schedule: schedule, target: target)
  case "delete":
    guard exactKeys(raw, ["kind", "destination", "target"]) else {
      throw NativeFailure("policy_denied")
    }
    let target = try parseTarget(raw["target"], personID: personID, destination: destination)
    return NativeCalendarEffect(raw: raw, kind: kind, destination: destination,
                                title: nil, schedule: nil, target: target)
  default:
    throw NativeFailure("policy_denied")
  }
}

private func parseIdentity(_ value: Any?) throws -> NativeEffectIdentity {
  guard let raw = value as? [String: Any],
        exactKeys(raw, ["execution_id", "effect_digest", "person_id", "device_id", "executor_generation", "connection_id", "calendar_id"]),
        let executionID = validUUID(raw["execution_id"]),
        let digestValues = digestBytes(raw["effect_digest"]) else {
    throw NativeFailure("policy_denied")
  }
  guard let personID = validUUID(raw["person_id"]),
        let deviceID = actionText(raw["device_id"], maximum: 256),
        let generation = jsonUInt(raw["executor_generation"]), generation > 0,
        let connectionID = actionText(raw["connection_id"], maximum: 256),
        let calendarID = actionText(raw["calendar_id"], maximum: 512) else {
    throw NativeFailure("policy_denied")
  }
  return NativeEffectIdentity(raw: raw, executionID: executionID, effectDigest: digestValues,
                              personID: personID, deviceID: deviceID,
                              executorGeneration: generation, connectionID: connectionID,
                              calendarID: calendarID)
}

private func parseSourceFence(_ value: Any?, deviceID: String) throws -> NativeSourceFence {
  guard let raw = value as? [String: Any],
        exactKeys(raw, ["connection_id", "revision", "authority", "execution_owner", "resources", "native_subject_fingerprint"]),
        let connectionID = actionText(raw["connection_id"], maximum: 256),
        let revision = jsonUInt(raw["revision"]), revision > 0,
        let authority = raw["authority"] as? [String: Any],
        exactKeys(authority, ["incarnation", "epoch"]), validUUID(authority["incarnation"]) != nil,
        let authorityEpoch = jsonUInt(authority["epoch"]), authorityEpoch > 0,
        let executionOwner = actionText(raw["execution_owner"], maximum: 256),
        let resources = raw["resources"] as? [String], !resources.isEmpty, resources.count <= 4096,
        resources.allSatisfy({ actionText($0, maximum: 512) != nil }),
        resources == resources.sorted(), Set(resources).count == resources.count,
        let fingerprint = actionText(raw["native_subject_fingerprint"], maximum: 512),
        fingerprint.utf8.count == 64,
        fingerprint.utf8.allSatisfy({ ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102) }) else {
    throw NativeFailure("source_changed")
  }
  let ownerMatches = executionOwner == "apple:\(deviceID)"
  guard ownerMatches else {
    throw NativeFailure("policy_denied")
  }
  return NativeSourceFence(raw: raw, connectionID: connectionID, revision: revision,
                           resources: resources,
                           subjectFingerprint: fingerprint)
}

private func parseSourceFence(_ value: Any?, effect: NativeCalendarEffect,
                              identity: NativeEffectIdentity) throws -> NativeSourceFence {
  let source = try parseSourceFence(value, deviceID: identity.deviceID)
  guard source.connectionID == effect.destination.connectionID,
        source.revision == effect.destination.connectionRevision,
        source.connectionID == identity.connectionID,
        effect.destination.calendarID == identity.calendarID,
        source.resources.contains(effect.destination.calendarID) else {
    throw NativeFailure("policy_denied")
  }
  return source
}

private func parseAuthorization(_ value: Any?, actionID: String, personID: String,
                                deviceID: String, effectDigest: [Int],
                                effect: NativeCalendarEffect, preparedAt: Date) throws
  -> (expiresAtRaw: String, expiresAt: Date) {
  guard let raw = value as? [String: Any], let kind = raw["kind"] as? String else {
    throw NativeFailure("policy_denied")
  }
  let expiresAtRaw: String
  switch kind {
  case "direct_instruction":
    guard exactKeys(raw, ["kind", "command_id", "person_id", "device_id", "effect_digest", "authority_revision", "expires_at"]),
          validUUID(raw["command_id"]) != nil,
          validUUID(raw["person_id"]) == personID,
          raw["device_id"] as? String == deviceID,
          digestBytes(raw["effect_digest"]) == effectDigest,
          let authorityRevision = jsonUInt(raw["authority_revision"]), authorityRevision > 0,
          let expiry = actionText(raw["expires_at"], maximum: 64) else {
      throw NativeFailure("policy_denied")
    }
    expiresAtRaw = expiry
  case "reviewed_decision":
    guard exactKeys(raw, ["kind", "command_id", "person_id", "device_id", "review", "decided_at"]),
          validUUID(raw["command_id"]) != nil,
          validUUID(raw["person_id"]) == personID,
          raw["device_id"] as? String == deviceID,
          let decidedAt = actionDate(raw["decided_at"]), decidedAt <= preparedAt,
          let review = raw["review"] as? [String: Any],
          exactKeys(review, ["id", "action_id", "effect_digest", "source_digest", "authority_revision", "expires_at"]),
          validUUID(review["id"]) != nil,
          validUUID(review["action_id"]) == actionID,
          digestBytes(review["effect_digest"]) == effectDigest,
          digestBytes(review["source_digest"]) != nil,
          let authorityRevision = jsonUInt(review["authority_revision"]), authorityRevision > 0,
          let expiry = actionText(review["expires_at"], maximum: 64) else {
      throw NativeFailure("policy_denied")
    }
    expiresAtRaw = expiry
  case "standing_policy":
    guard exactKeys(raw, ["kind", "person_id", "effect_digest", "authority_revision", "expires_at"]),
          validUUID(raw["person_id"]) == personID,
          digestBytes(raw["effect_digest"]) == effectDigest,
          let authorityRevision = jsonUInt(raw["authority_revision"]), authorityRevision > 0,
          let expiry = actionText(raw["expires_at"], maximum: 64), effect.kind == "create" else {
      throw NativeFailure("policy_denied")
    }
    expiresAtRaw = expiry
  default:
    throw NativeFailure("policy_denied")
  }
  guard let expiresAt = actionDate(expiresAtRaw), preparedAt < expiresAt else {
    throw NativeFailure("policy_denied")
  }
  return (expiresAtRaw, expiresAt)
}

private func parseExecutionIntent(_ value: Any?) throws -> NativeExecutionIntent {
  guard let raw = value as? [String: Any],
        exactKeys(raw, ["action_id", "person_id", "device_id", "execution_id", "effect_digest", "effect", "source", "authorization", "executor_generation", "prepared_at"]),
        let actionID = validUUID(raw["action_id"]),
        let personID = validUUID(raw["person_id"]),
        let deviceID = actionText(raw["device_id"], maximum: 256),
        let executionID = validUUID(raw["execution_id"]), actionID != executionID,
        let digest = digestBytes(raw["effect_digest"]),
        let generation = jsonUInt(raw["executor_generation"]), generation > 0,
        let preparedAt = actionDate(raw["prepared_at"]) else {
    throw NativeFailure("policy_denied")
  }
  let effect = try parseEffect(raw["effect"], personID: personID)
  let identityRaw: [String: Any] = [
    "execution_id": executionID,
    "effect_digest": raw["effect_digest"]!,
    "person_id": personID,
    "device_id": deviceID,
    "executor_generation": raw["executor_generation"]!,
    "connection_id": effect.destination.connectionID,
    "calendar_id": effect.destination.calendarID
  ]
  let identity = try parseIdentity(identityRaw)
  guard identity.effectDigest == digest, identity.personID == personID,
        identity.deviceID == deviceID, identity.executorGeneration == generation else {
    throw NativeFailure("policy_denied")
  }
  let source = try parseSourceFence(raw["source"], effect: effect, identity: identity)
  let authorization = try parseAuthorization(raw["authorization"], actionID: actionID,
    personID: personID, deviceID: deviceID, effectDigest: digest, effect: effect,
    preparedAt: preparedAt)
  return NativeExecutionIntent(raw: raw, identity: identity, effect: effect, source: source,
    authorizationExpiresAtRaw: authorization.expiresAtRaw,
    authorizationExpiresAt: authorization.expiresAt)
}

private func unknownOutcome(_ identity: [String: Any], _ reason: String) -> [String: Any] {
  return ["status": "unknown", "identity": identity, "reason": reason]
}

private func notAppliedOutcome(_ identity: [String: Any], hostEpoch: String,
                               invocationID: String, reason: String,
                               rejectedAt: Date) -> [String: Any] {
  return ["status": "not_applied", "proof": [
    "identity": identity,
    "host_epoch": hostEpoch,
    "invocation_id": invocationID,
    "reason": reason,
    "rejected_at": actionTimestamp(rejectedAt)
  ]]
}

private enum NativeDispatchDecision {
  case accepted(NativePreparation)
  case outcome([String: Any])
}

private enum NativePreparationDecision {
  case ready(NativePreparation)
  case blocked(String)
}

private final class NativeActionCache: @unchecked Sendable {
  private let lock = NSLock()
  private var preparations: [String: NativePreparation] = [:]
  private var invocations: [String: NativeInvocation] = [:]

  private func prune(_ now: Date) {
    preparations = preparations.filter { $0.value.expiresAt > now }
    invocations = invocations.filter { _, value in
      guard let completedAt = value.completedAt else { return true }
      return now.timeIntervalSince(completedAt) < nativeReceiptRetention
    }
  }

  private func makeCapacity(_ now: Date) -> Bool {
    prune(now)
    while preparations.count + invocations.count >= maxNativeActionEntries {
      guard let oldest = invocations
        .filter({ $0.value.completedAt != nil })
        .min(by: { $0.value.completedAt! < $1.value.completedAt! }) else { return false }
      invocations.removeValue(forKey: oldest.key)
    }
    return true
  }

  func reserve(_ candidate: NativePreparation) -> NativePreparationDecision {
    lock.lock()
    defer { lock.unlock() }
    let now = Date()
    prune(now)
    if invocations[candidate.executionID] != nil {
      return .blocked("executor_unavailable")
    }
    if let existing = preparations[candidate.executionID] {
      let same = sameJSON(existing.identity, candidate.identity) &&
        sameJSON(existing.effect, candidate.effect) &&
        sameJSON(existing.source, candidate.source) &&
        sameJSON(existing.localEvents, candidate.localEvents) &&
        existing.authorizationExpiresAtRaw == candidate.authorizationExpiresAtRaw
      return same ? .ready(existing) : .blocked("source_changed")
    }
    guard makeCapacity(now) else { return .blocked("executor_unavailable") }
    preparations[candidate.executionID] = candidate
    return .ready(candidate)
  }

  func beginDispatch(_ intent: NativeExecutionIntent, preparationID: String,
                     hostEpoch: String) -> NativeDispatchDecision {
    lock.lock()
    defer { lock.unlock() }
    let now = Date()
    prune(now)
    if let existing = invocations[intent.identity.executionID] {
      let same = existing.preparationID == preparationID && existing.hostEpoch == hostEpoch &&
        sameJSON(existing.identity, intent.identity.raw) && sameJSON(existing.intent, intent.raw)
      if !same { return .outcome(unknownOutcome(intent.identity.raw, "invalid_receipt")) }
      return .outcome(existing.outcome ?? unknownOutcome(intent.identity.raw, "native_operation_pending"))
    }
    if invocations.values.contains(where: { $0.preparationID == preparationID }) ||
       preparations.values.contains(where: { $0.preparationID == preparationID && $0.executionID != intent.identity.executionID }) {
      return .outcome(unknownOutcome(intent.identity.raw, "invalid_receipt"))
    }
    guard let preparation = preparations[intent.identity.executionID] else {
      return .outcome(unknownOutcome(intent.identity.raw, "native_receipt_unavailable"))
    }
    guard preparation.preparationID == preparationID && preparation.hostEpoch == hostEpoch &&
          hostEpoch == nativeHostEpoch && sameJSON(preparation.identity, intent.identity.raw) &&
          sameJSON(preparation.effect, intent.effect.raw) && sameJSON(preparation.source, intent.source.raw) &&
          preparation.authorizationExpiresAtRaw == intent.authorizationExpiresAtRaw else {
      return .outcome(unknownOutcome(intent.identity.raw, "invalid_receipt"))
    }
    preparations.removeValue(forKey: intent.identity.executionID)
    invocations[intent.identity.executionID] = NativeInvocation(
      executionID: intent.identity.executionID,
      identity: intent.identity.raw,
      intent: intent.raw,
      preparationID: preparationID,
      hostEpoch: hostEpoch,
      outcome: nil,
      completedAt: nil
    )
    return .accepted(preparation)
  }

  func finish(_ intent: NativeExecutionIntent, outcome: [String: Any]) {
    lock.lock()
    defer { lock.unlock() }
    guard var invocation = invocations[intent.identity.executionID],
          sameJSON(invocation.identity, intent.identity.raw),
          sameJSON(invocation.intent, intent.raw),
          invocation.outcome == nil else { return }
    invocation.outcome = outcome
    invocation.completedAt = Date()
    invocations[intent.identity.executionID] = invocation
  }

  func readback(_ intent: NativeExecutionIntent) -> [String: Any] {
    lock.lock()
    defer { lock.unlock() }
    prune(Date())
    guard let invocation = invocations[intent.identity.executionID] else {
      return unknownOutcome(intent.identity.raw, "native_receipt_unavailable")
    }
    guard sameJSON(invocation.identity, intent.identity.raw) && sameJSON(invocation.intent, intent.raw) else {
      return unknownOutcome(intent.identity.raw, "invalid_receipt")
    }
    return invocation.outcome ?? unknownOutcome(intent.identity.raw, "native_operation_pending")
  }

  func recordLookup(_ intent: NativeExecutionIntent, outcome: [String: Any]) {
    lock.lock()
    defer { lock.unlock() }
    let now = Date()
    prune(now)
    if let existing = invocations[intent.identity.executionID] {
      if sameJSON(existing.identity, intent.identity.raw) && sameJSON(existing.intent, intent.raw) { return }
      return
    }
    preparations.removeValue(forKey: intent.identity.executionID)
    guard makeCapacity(now) else { return }
    invocations[intent.identity.executionID] = NativeInvocation(
      executionID: intent.identity.executionID,
      identity: intent.identity.raw,
      intent: intent.raw,
      preparationID: nil,
      hostEpoch: nil,
      outcome: outcome,
      completedAt: now
    )
  }
}

private let nativeActionCache = NativeActionCache()

private func timestamp(_ value: Any?) throws -> Date {
  guard let text = value as? String else { throw NativeFailure("uncertain_result") }
  let formatter = ISO8601DateFormatter()
  formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  if let date = formatter.date(from: text) { return date }
  formatter.formatOptions = [.withInternetDateTime]
  guard let date = formatter.date(from: text) else { throw NativeFailure("uncertain_result") }
  return date
}

private func requirePermission() throws {
  let status = EKEventStore.authorizationStatus(for: .event)
  if #available(macOS 14.0, *) {
    guard status == .fullAccess else { throw NativeFailure("permission_denied") }
  } else {
    guard status == .authorized else { throw NativeFailure("permission_denied") }
  }
}

private func localConflict(_ records: [[String: Any]], schedule: NativeTimedSchedule,
                           excludingEventID: String?) throws -> Bool {
  for record in records {
    guard let deleted = record["deleted_at"] else { throw NativeFailure("uncertain_result") }
    if !(deleted is NSNull) {
      guard actionDate(deleted) != nil else { throw NativeFailure("uncertain_result") }
      continue
    }
    if let excludingEventID, record["id"] as? String == excludingEventID { continue }
    guard let recordSchedule = record["schedule"] as? [String: Any] else { throw NativeFailure("uncertain_result") }
    if recordSchedule["kind"] as? String == "timed" {
      guard let start = actionDate(recordSchedule["starts_at"]),
            let end = actionDate(recordSchedule["ends_at"]), end > start else {
        throw NativeFailure("uncertain_result")
      }
      if start < schedule.end && end > schedule.start { return true }
    } else if recordSchedule["kind"] as? String == "all_day",
              let startText = recordSchedule["start_date"] as? String,
              let endText = recordSchedule["end_date_exclusive"] as? String {
      let start = try timestamp(startText + "T00:00:00Z").addingTimeInterval(-14 * 3600)
      let end = try timestamp(endText + "T00:00:00Z").addingTimeInterval(14 * 3600)
      if start < schedule.end && end > schedule.start { return true }
    } else { throw NativeFailure("uncertain_result") }
  }
  return false
}

private func nativeSubjectFingerprint(_ store: EKEventStore, _ identifiers: [String]) throws -> String {
  let permissionClass: String
  if #available(macOS 14.0, *) {
    switch EKEventStore.authorizationStatus(for: .event) {
    case .fullAccess:
      permissionClass = "full"
    case .writeOnly:
      permissionClass = "write_only"
    case .authorized:
      permissionClass = "authorized"
    case .denied:
      permissionClass = "denied"
    case .restricted:
      permissionClass = "restricted"
    case .notDetermined:
      permissionClass = "not_determined"
    @unknown default:
      permissionClass = "unknown"
    }
  } else {
    switch EKEventStore.authorizationStatus(for: .event) {
    case .authorized:
      permissionClass = "authorized"
    case .denied:
      permissionClass = "denied"
    case .restricted:
      permissionClass = "restricted"
    case .notDetermined:
      permissionClass = "not_determined"
    case .fullAccess:
      permissionClass = "authorized"
    case .writeOnly:
      permissionClass = "authorized"
    @unknown default:
      permissionClass = "unknown"
    }
  }
  let tuples = try identifiers.sorted().map { identifier -> [String] in
    guard let calendar = store.calendar(withIdentifier: identifier) else {
      throw NativeFailure("provider_unavailable")
    }
    return [calendar.calendarIdentifier, calendar.source.sourceIdentifier,
            String(calendar.source.sourceType.rawValue)]
  }
  let canonical: [String: Any] = ["permission_class": permissionClass, "subjects": tuples]
  let bytes = try JSONSerialization.data(withJSONObject: canonical, options: [.sortedKeys])
  return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
}

private func nativeEventRevision(_ event: EKEvent) throws -> String {
  let formatter = ISO8601DateFormatter()
  formatter.timeZone = TimeZone(secondsFromGMT: 0)
  formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  // Fingerprints retain the native externally-tagged schedule shape; the
  // returned native record uses the separate flat kind-tagged DTO.
  let schedule: [String: Any] = ["Timed": [
    "starts_at": formatter.string(from: event.startDate),
    "ends_at": formatter.string(from: event.endDate),
    "timezone": (event.timeZone ?? TimeZone.current).identifier
  ]]
  let data = try JSONSerialization.data(withJSONObject: [
    "title": event.title ?? "",
    "schedule": schedule,
    "modified": event.lastModifiedDate.map { formatter.string(from: $0) } ?? ""
  ], options: [.sortedKeys])
  return sha256Hex(data)
}

private func currentNativeSchedule(_ event: EKEvent) -> [String: Any] {
  let timezone = event.timeZone ?? TimeZone.current
  return [
    "starts_at": actionTimestamp(event.startDate),
    "ends_at": actionTimestamp(event.endDate),
    "timezone": timezone.identifier
  ]
}

private func verifyNativeSubject(_ store: EKEventStore,
                                _ source: NativeSourceFence) throws -> String {
  try requirePermission()
  let fingerprint = try nativeSubjectFingerprint(store, source.resources)
  guard fingerprint == source.subjectFingerprint else { throw NativeFailure("source_changed") }
  return fingerprint
}

private func nativeCalendar(_ store: EKEventStore,
                            _ destination: NativeDestination,
                            requireWrite: Bool = true) throws -> EKCalendar {
  guard let calendar = store.calendar(withIdentifier: destination.calendarID),
        calendar.calendarIdentifier == destination.calendarID else {
    throw NativeFailure("source_changed")
  }
  guard !requireWrite || (calendar.allowsContentModifications && !calendar.isSubscribed) else {
    throw NativeFailure("provider_unavailable")
  }
  return calendar
}

private func nativeTargetEvent(_ store: EKEventStore,
                               _ effect: NativeCalendarEffect) throws -> EKEvent? {
  guard let target = effect.target else { return nil }
  let itemID = String(target.externalID.dropLast())
  guard let event = store.calendarItem(withIdentifier: itemID) as? EKEvent,
        event.calendar.calendarIdentifier == effect.destination.calendarID,
        "\(event.calendarItemIdentifier)|" == target.externalID,
        !event.isAllDay, !event.hasRecurrenceRules, !event.isDetached, !event.hasAttendees,
        (event.title ?? "") == target.title,
        (event.timeZone ?? TimeZone.current).identifier == target.schedule.timezone,
        abs(event.startDate.timeIntervalSince(target.schedule.start)) < 0.001,
        abs(event.endDate.timeIntervalSince(target.schedule.end)) < 0.001,
        try nativeEventRevision(event) == target.externalRevision else {
    throw NativeFailure("source_changed")
  }
  return event
}

private func nativeScheduleConflict(_ store: EKEventStore, _ resources: [String],
                                   _ schedule: NativeTimedSchedule,
                                   excludingItemID: String?) throws -> Bool {
  let calendars = resources.compactMap { store.calendar(withIdentifier: $0) }
  guard calendars.count == resources.count else { throw NativeFailure("source_changed") }
  let predicate = store.predicateForEvents(withStart: schedule.start, end: schedule.end,
                                            calendars: calendars)
  return store.events(matching: predicate).contains { event in
    if let excludingItemID, event.calendarItemIdentifier == excludingItemID { return false }
    return event.startDate < schedule.end && event.endDate > schedule.start
  }
}

private func nativeWriteResult(_ event: EKEvent) throws -> [String: Any] {
  guard !event.calendarItemIdentifier.isEmpty,
        !event.isAllDay, !event.hasRecurrenceRules, !event.isDetached, !event.hasAttendees,
        event.endDate > event.startDate,
        event.endDate.timeIntervalSince(event.startDate) <= 86_400 else {
    throw NativeFailure("invalid_receipt")
  }
  let revision = try nativeEventRevision(event)
  return [
    "external_id": "\(event.calendarItemIdentifier)|",
    "external_revision": ["kind": "observation_fingerprint", "sha256": revision],
    "title": event.title ?? "",
    "schedule": currentNativeSchedule(event),
    "can_modify": event.calendar.allowsContentModifications && !event.calendar.isSubscribed
  ]
}

private func nativeWriteMatches(_ event: EKEvent, effect: NativeCalendarEffect,
                                marker: URL?) -> Bool {
  guard let title = effect.title, let schedule = effect.schedule else { return false }
  return event.calendar.calendarIdentifier == effect.destination.calendarID &&
    (event.title ?? "") == title &&
    abs(event.startDate.timeIntervalSince(schedule.start)) < 0.001 &&
    abs(event.endDate.timeIntervalSince(schedule.end)) < 0.001 &&
    wholeSecond(event.startDate) && wholeSecond(event.endDate) &&
    (event.timeZone ?? TimeZone.current).identifier == schedule.timezone &&
    !event.isAllDay && !event.hasRecurrenceRules && !event.isDetached && !event.hasAttendees &&
    (marker == nil || event.url == marker)
}

/// Shape validation only; the Rust owner admits Person authority before this boundary.
private func admittedPerson(_ value: Any?) -> String? {
  guard let text = value as? String, let id = UUID(uuidString: text),
        id.uuidString.lowercased() == text,
        text != "00000000-0000-0000-0000-000000000000" else { return nil }
  return text
}

func calendarViewAccess(_ request: [String: Any], permission: () throws -> Void,
                        contains: (String) -> Bool, generation: () -> String,
                        subjectFingerprint: () throws -> String) throws -> [String: Any] {
  guard request["schema_version"] as? Int == 1,
        let personID = admittedPerson(request["person_id"]),
        let deviceID = request["device_id"] as? String,
        !deviceID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
        deviceID.utf8.count <= 128,
        let connectionID = request["connection_id"] as? String,
        !connectionID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
        connectionID.utf8.count <= 128,
        let connectionRevision = request["connection_revision"] as? Int,
        connectionRevision > 0,
        request["provider"] as? String == "event_kit",
        let identifiers = request["calendar_ids"] as? [String],
        !identifiers.isEmpty,
        Set(identifiers).count == identifiers.count,
        identifiers.allSatisfy({ !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.utf8.count <= 512 }) else {
    throw NativeFailure("permission_denied")
  }
  _ = (deviceID, connectionID, connectionRevision)
  let deadline = try timestamp(request["deadline"])
  guard Date() < deadline, deadline.timeIntervalSinceNow <= 30 else { throw NativeFailure("timeout") }
  try permission()
  let beforeSubject = try subjectFingerprint()
  let before = generation()
  guard identifiers.allSatisfy(contains) else { throw NativeFailure("provider_unavailable") }
  try permission()
  guard before == generation(), beforeSubject == (try subjectFingerprint()), Date() < deadline else { throw NativeFailure("timeout") }
  return ["schema_version": 1, "person_id": personID, "device_id": deviceID,
          "provider": "event_kit", "calendar_ids": identifiers.sorted(),
          "native_subject_fingerprint": beforeSubject, "generation": before]
}

private func observationRecord(_ event: EKEvent, calendarID: String) -> [String: Any] {
  let timestamp = ISO8601DateFormatter()
  timestamp.timeZone = TimeZone(secondsFromGMT: 0)
  timestamp.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  let identifier = event.calendarItemIdentifier
  let occurrence = (event.hasRecurrenceRules || event.isDetached)
    ? event.occurrenceDate.map { timestamp.string(from: $0) } ?? "" : ""
  let date = DateFormatter()
  date.calendar = Calendar(identifier: .gregorian)
  date.locale = Locale(identifier: "en_US_POSIX")
  date.timeZone = event.timeZone ?? TimeZone.current
  date.dateFormat = "yyyy-MM-dd"
  let allDayEnd = normalizedObservationAllDayEnd(
    start: event.startDate,
    end: event.endDate,
    calendar: date.calendar
  )
  let schedule: [String: Any]
  let revisionSchedule: [String: Any]
  if event.isAllDay {
    let startDate = date.string(from: event.startDate)
    let endDateExclusive = date.string(from: allDayEnd)
    schedule = ["kind": "all_day", "start_date": startDate,
                "end_date_exclusive": endDateExclusive]
    revisionSchedule = ["AllDay": ["start_date": startDate,
                                   "end_date_exclusive": endDateExclusive]]
  } else {
    let startsAt = timestamp.string(from: event.startDate)
    let endsAt = timestamp.string(from: event.endDate)
    let timezone = (event.timeZone ?? TimeZone.current).identifier
    schedule = ["kind": "timed", "starts_at": startsAt,
                "ends_at": endsAt, "timezone": timezone]
    revisionSchedule = ["Timed": ["starts_at": startsAt, "ends_at": endsAt,
                                  "timezone": timezone]]
  }
  let revisionData = try! JSONSerialization.data(withJSONObject: [
    "title": event.title ?? "", "schedule": revisionSchedule,
    "modified": event.lastModifiedDate.map { timestamp.string(from: $0) } ?? ""
  ], options: [.sortedKeys])
  let revision = SHA256.hash(data: revisionData).map { String(format: "%02x", $0) }.joined()
  return [
    "can_modify": event.calendar.allowsContentModifications && !event.calendar.isSubscribed &&
      !event.isAllDay && !event.hasRecurrenceRules && !event.isDetached && !event.hasAttendees &&
      event.endDate > event.startDate && event.endDate.timeIntervalSince(event.startDate) <= 86400,
    "calendar_id": calendarID,
    "external_id": "\(identifier)|\(occurrence)",
    "external_revision": revision,
    "title": event.title ?? "",
    "schedule": schedule
  ]
}

private func normalizedObservationAllDayEnd(start: Date, end: Date, calendar: Calendar) -> Date {
  let startDay = calendar.startOfDay(for: start)
  let endDay = calendar.startOfDay(for: end)
  let candidate = end > endDay
    ? calendar.date(byAdding: .day, value: 1, to: endDay)!
    : endDay
  return candidate > startDay
    ? candidate
    : calendar.date(byAdding: .day, value: 1, to: startDay)!
}

private func calendarObservation(_ request: [String: Any]) throws -> [String: Any] {
  guard request["schema_version"] as? Int == 1,
        let personID = admittedPerson(request["person_id"]),
        let deviceID = request["device_id"] as? String,
        !deviceID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
        deviceID.utf8.count <= 128,
        let connectionID = request["connection_id"] as? String,
        !connectionID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
        connectionID.utf8.count <= 128,
        let connectionRevision = request["connection_revision"] as? Int,
        connectionRevision > 0,
        request["provider"] as? String == "event_kit",
        let identifiers = request["calendar_ids"] as? [String],
        !identifiers.isEmpty,
        Set(identifiers).count == identifiers.count,
        identifiers.allSatisfy({ !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && $0.utf8.count <= 512 }),
        let startText = request["starts_at"] as? String,
        let endText = request["ends_at"] as? String,
        let itemLimit = request["item_limit"] as? Int,
        itemLimit > 0, itemLimit <= maxCalendarReadItems,
        let byteLimit = request["byte_limit"] as? Int,
        byteLimit > 0, byteLimit <= maxCalendarReadBytes else {
    throw NativeFailure("permission_denied")
  }
  _ = (deviceID, connectionID, connectionRevision)
  let start = try timestamp(startText)
  let end = try timestamp(endText)
  let deadline = try timestamp(request["deadline"])
  guard start < end, end.timeIntervalSince(start) <= 32 * 86400,
        Date() < deadline, deadline.timeIntervalSinceNow <= 30 else {
    throw NativeFailure("timeout")
  }
  try requirePermission()
  let before = calendarViewGeneration.value()
  let store = EKEventStore()
  let beforeSubject = try nativeSubjectFingerprint(store, identifiers)
  if let expectedSubject = request["expected_native_subject_fingerprint"] as? String,
     expectedSubject != beforeSubject {
    throw NativeFailure("permission_denied")
  }
  let calendars = identifiers.compactMap { store.calendar(withIdentifier: $0) }
  guard calendars.count == identifiers.count else { throw NativeFailure("provider_unavailable") }
  var totalItems = 0
  let batches = try calendars.map { calendar -> [String: Any] in
    let predicate = store.predicateForEvents(withStart: start, end: end, calendars: [calendar])
    let events = store.events(matching: predicate)
    totalItems += events.count
    if totalItems > itemLimit { throw NativeFailure("budget_exceeded") }
    return [
      "calendar_id": calendar.calendarIdentifier,
      "records": events.map {
        observationRecord($0, calendarID: calendar.calendarIdentifier)
      }
    ]
  }
  try requirePermission()
  guard before == calendarViewGeneration.value(), beforeSubject == (try nativeSubjectFingerprint(store, identifiers)), Date() < deadline else {
    throw NativeFailure("timeout")
  }
  let formatter = ISO8601DateFormatter()
  formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  let response: [String: Any] = [
    "stamp": ["schema_version": 1, "person_id": personID, "device_id": deviceID,
      "provider": "event_kit",
      "calendar_ids": identifiers.sorted(), "native_subject_fingerprint": beforeSubject,
      "generation": before],
    "observed_at": formatter.string(from: Date()),
    "batches": batches
  ]
  guard let encoded = try? JSONSerialization.data(withJSONObject: response, options: [.sortedKeys]),
        encoded.count <= byteLimit else { throw NativeFailure("budget_exceeded") }
  return response
}

/// Returns metadata only for the exact Calendar resources admitted by Rust.
/// This path neither requests permission nor reads event payloads or writes.
private func actionDestinations(_ request: [String: Any]) throws -> [String: Any] {
  let keys: Set<String> = ["schema_version", "operation", "person_id", "device_id",
                           "executor_generation", "source", "deadline"]
  guard exactActionRequest(request, operation: "action_destinations", keys: keys),
        let personID = validUUID(request["person_id"]),
        let deviceID = actionText(request["device_id"], maximum: 256),
        let executorGeneration = jsonUInt(request["executor_generation"]), executorGeneration > 0,
        let deadline = actionDate(request["deadline"]),
        let encodedRequest = canonicalJSON(request) else {
    throw NativeFailure("uncertain_result")
  }
  guard encodedRequest.count <= maxCalendarReadBytes else { throw NativeFailure("budget_exceeded") }
  guard Date() < deadline, deadline.timeIntervalSinceNow <= 30 else {
    throw NativeFailure("timeout")
  }
  let source = try parseSourceFence(request["source"], deviceID: deviceID)

  eventStoreLock.lock()
  defer { eventStoreLock.unlock() }
  let store = EKEventStore()
  let generation = calendarViewGeneration.value()
  try requirePermission()
  let beforeFingerprint = try nativeSubjectFingerprint(store, source.resources)
  guard beforeFingerprint == source.subjectFingerprint else { throw NativeFailure("source_changed") }

  let resources = try source.resources.map { identifier -> [String: Any] in
    guard let calendar = store.calendar(withIdentifier: identifier),
          calendar.calendarIdentifier == identifier else {
      throw NativeFailure("provider_unavailable")
    }
    return [
      "calendar_id": calendar.calendarIdentifier,
      "calendar_name": "\(calendar.source.title) · \(calendar.title)",
      "can_modify": calendar.allowsContentModifications && !calendar.isSubscribed
    ]
  }
  guard resources.count == source.resources.count,
        Set(resources.compactMap { $0["calendar_id"] as? String }).count == source.resources.count else {
    throw NativeFailure("provider_unavailable")
  }

  try requirePermission()
  let afterFingerprint = try nativeSubjectFingerprint(store, source.resources)
  guard afterFingerprint == source.subjectFingerprint,
        generation == calendarViewGeneration.value() else {
    throw NativeFailure("source_changed")
  }
  guard Date() < deadline else { throw NativeFailure("timeout") }
  let result: [String: Any] = [
    "schema_version": 1,
    "person_id": personID,
    "device_id": deviceID,
    "executor_generation": executorGeneration,
    "source": source.raw,
    "resources": resources
  ]
  guard let encodedReply = canonicalJSON(["data": result]),
        encodedReply.count <= maxCalendarReadBytes else {
    throw NativeFailure("budget_exceeded")
  }
  guard Date() < deadline else { throw NativeFailure("timeout") }
  return result
}

private func blockedPreparation(_ identity: [String: Any], _ reason: String) -> [String: Any] {
  let allowed = Set(["permission_denied", "policy_denied", "source_changed", "executor_unavailable", "schedule_conflict"])
  return ["schema_version": 1, "status": "blocked", "identity": identity,
          "reason": allowed.contains(reason) ? reason : "executor_unavailable"]
}

private func preparationReason(_ error: NativeFailure) -> String {
  switch error.reason {
  case "permission_denied", "policy_denied", "source_changed", "schedule_conflict": return error.reason
  default: return "executor_unavailable"
  }
}

private func exactActionRequest(_ request: [String: Any], operation: String,
                                keys: Set<String>) -> Bool {
  return exactKeys(request, keys) && jsonUInt(request["schema_version"]) == 1 &&
    request["operation"] as? String == operation
}

private func markerValue(personID: String, executionID: String) -> (String, URL)? {
  let text = "floe://calendar-action/\(personID)/\(executionID)"
  guard let url = URL(string: text) else { return nil }
  return (text, url)
}

private func markerExists(_ store: EKEventStore, calendar: EKCalendar,
                          schedule: NativeTimedSchedule, marker: String) -> Bool {
  let predicate = store.predicateForEvents(
    withStart: schedule.start.addingTimeInterval(-86_400),
    end: schedule.end.addingTimeInterval(86_400), calendars: [calendar])
  return store.events(matching: predicate).contains { $0.url?.absoluteString == marker }
}

private func actionPreflight(_ request: [String: Any]) throws -> [String: Any] {
  let keys: Set<String> = ["schema_version", "operation", "identity", "effect", "source",
                           "authorization_expires_at", "local_events", "deadline"]
  guard exactActionRequest(request, operation: "action_preflight", keys: keys) else {
    throw NativeFailure("uncertain_result")
  }
  let identity = try parseIdentity(request["identity"])
  do {
    let effect = try parseEffect(request["effect"], personID: identity.personID)
    let source = try parseSourceFence(request["source"], effect: effect, identity: identity)
    guard let expiryText = actionText(request["authorization_expires_at"], maximum: 64),
          let authorizationExpiry = actionDate(expiryText), Date() < authorizationExpiry,
          let deadline = actionDate(request["deadline"]), Date() < deadline,
          deadline.timeIntervalSinceNow <= 30,
          let localEvents = request["local_events"] as? [[String: Any]],
          localEvents.count <= 4096,
          let encodedEvents = canonicalJSON(localEvents), encodedEvents.count <= maxCalendarReadBytes else {
      throw NativeFailure("policy_denied")
    }
    eventStoreLock.lock()
    defer { eventStoreLock.unlock() }
    try requirePermission()
    let store = EKEventStore()
    let generation = calendarViewGeneration.value()
    let fingerprint = try verifyNativeSubject(store, source)
    let calendars = source.resources.compactMap { store.calendar(withIdentifier: $0) }
    guard calendars.count == source.resources.count else { throw NativeFailure("source_changed") }
    let calendar = try nativeCalendar(store, effect.destination)
    let existing = try nativeTargetEvent(store, effect)
    if effect.kind == "create", let schedule = effect.schedule {
      if try localConflict(localEvents, schedule: schedule, excludingEventID: effect.target?.eventID) {
        throw NativeFailure("schedule_conflict")
      }
      if try nativeScheduleConflict(store, source.resources, schedule,
                                    excludingItemID: existing?.calendarItemIdentifier) {
        throw NativeFailure("schedule_conflict")
      }
      guard let marker = markerValue(personID: identity.personID,
                                     executionID: identity.executionID)?.0 else {
        throw NativeFailure("policy_denied")
      }
      if markerExists(store, calendar: calendar, schedule: schedule, marker: marker) {
        throw NativeFailure("schedule_conflict")
      }
    }
    try requirePermission()
    guard try verifyNativeSubject(store, source) == fingerprint,
          generation == calendarViewGeneration.value(), Date() < deadline else {
      throw NativeFailure("source_changed")
    }
    if effect.target != nil { _ = try nativeTargetEvent(store, effect) }
    let createdAt = Date()
    let expiresAt = min(createdAt.addingTimeInterval(30), authorizationExpiry).addingTimeInterval(-0.001)
    guard expiresAt > createdAt else { throw NativeFailure("policy_denied") }
    let candidate = NativePreparation(
      executionID: identity.executionID,
      identity: identity.raw,
      effect: effect.raw,
      source: source.raw,
      localEvents: localEvents,
      authorizationExpiresAtRaw: expiryText,
      authorizationExpiresAt: authorizationExpiry,
      preparationID: UUID().uuidString.lowercased(),
      hostEpoch: nativeHostEpoch,
      nativeSubjectFingerprint: fingerprint,
      expiresAt: expiresAt
    )
    switch nativeActionCache.reserve(candidate) {
    case .blocked(let reason):
      return blockedPreparation(identity.raw, reason)
    case .ready(let prepared):
      // ISO8601DateFormatter emits millisecond precision; the stored expiry
      // already leaves a margin so rounding cannot exceed authorization.
      guard prepared.expiresAt > Date() else { return blockedPreparation(identity.raw, "policy_denied") }
      return ["schema_version": 1, "status": "ready", "identity": prepared.identity,
        "host_epoch": prepared.hostEpoch, "preparation_id": prepared.preparationID,
        "native_subject_fingerprint": prepared.nativeSubjectFingerprint,
        "expires_at": actionTimestamp(prepared.expiresAt)]
    }
  } catch let error as NativeFailure {
    return blockedPreparation(identity.raw, preparationReason(error))
  } catch {
    return blockedPreparation(identity.raw, "executor_unavailable")
  }
}

private func notAppliedReason(_ error: NativeFailure) -> String {
  switch error.reason {
  case "permission_denied": return "permission_denied"
  case "source_changed": return "source_changed"
  case "provider_unavailable": return "provider_unavailable"
  default: return "provider_rejected"
  }
}

private func postDispatchUnknownReason(_ error: NativeFailure) -> String {
  switch error.reason {
  case "timeout": return "timeout"
  case "invalid_receipt": return "invalid_receipt"
  default: return "response_lost"
  }
}

private func committedOutcome(_ identity: [String: Any], effect: [String: Any],
                              evidence: [String: Any], committedAt: Date) -> [String: Any] {
  return ["status": "committed", "receipt": [
    "identity": identity,
    "effect": effect,
    "evidence": evidence,
    "committed_at": actionTimestamp(committedAt)
  ]]
}

private func actionDispatch(_ request: [String: Any]) throws -> [String: Any] {
  let keys: Set<String> = ["schema_version", "operation", "admission", "preparation_id", "host_epoch", "deadline"]
  guard exactActionRequest(request, operation: "action_dispatch", keys: keys),
        let preparationID = validUUID(request["preparation_id"]),
        let hostEpoch = validUUID(request["host_epoch"]),
        let deadline = actionDate(request["deadline"]) else {
    throw NativeFailure("uncertain_result")
  }
  let intent = try parseExecutionIntent(request["admission"])
  switch nativeActionCache.beginDispatch(intent, preparationID: preparationID, hostEpoch: hostEpoch) {
  case .outcome(let outcome):
    return outcome
  case .accepted(let preparation):
    eventStoreLock.lock()
    defer { eventStoreLock.unlock() }
    var crossedBoundary = false
    let outcome: [String: Any]
    do {
      guard Date() < deadline, Date() < preparation.expiresAt,
            Date() < preparation.authorizationExpiresAt,
            intent.authorizationExpiresAtRaw == preparation.authorizationExpiresAtRaw else {
        throw NativeFailure("provider_rejected")
      }
      try requirePermission()
      let store = EKEventStore()
      let generation = calendarViewGeneration.value()
      let fingerprint = try verifyNativeSubject(store, intent.source)
      let calendar = try nativeCalendar(store, intent.effect.destination)
      let existing = try nativeTargetEvent(store, intent.effect)
      if intent.effect.kind == "create", let schedule = intent.effect.schedule {
        if try localConflict(preparation.localEvents, schedule: schedule,
                             excludingEventID: intent.effect.target?.eventID) {
          throw NativeFailure("provider_rejected")
        }
        if try nativeScheduleConflict(store, intent.source.resources, schedule,
                                      excludingItemID: existing?.calendarItemIdentifier) {
          throw NativeFailure("provider_rejected")
        }
      }
      let marker = intent.effect.kind == "create"
        ? markerValue(personID: intent.identity.personID, executionID: intent.identity.executionID)
        : nil
      if intent.effect.kind == "create" {
        guard let schedule = intent.effect.schedule, let marker,
              !markerExists(store, calendar: calendar, schedule: schedule, marker: marker.0) else {
          throw NativeFailure("provider_rejected")
        }
      }
      try requirePermission()
      guard try verifyNativeSubject(store, intent.source) == fingerprint,
            generation == calendarViewGeneration.value(), Date() < deadline,
            Date() < preparation.expiresAt, Date() < preparation.authorizationExpiresAt else {
        throw NativeFailure("provider_rejected")
      }
      let finalCalendar = try nativeCalendar(store, intent.effect.destination)
      let finalTarget = try nativeTargetEvent(store, intent.effect)
      if intent.effect.kind == "create", let schedule = intent.effect.schedule {
        if try nativeScheduleConflict(store, intent.source.resources, schedule,
                                      excludingItemID: finalTarget?.calendarItemIdentifier) {
          throw NativeFailure("provider_rejected")
        }
      }
      try requirePermission()
      guard try verifyNativeSubject(store, intent.source) == fingerprint,
            Date() < deadline, Date() < preparation.expiresAt,
            Date() < preparation.authorizationExpiresAt,
            finalCalendar.allowsContentModifications, !finalCalendar.isSubscribed else {
        throw NativeFailure("provider_rejected")
      }
      // EventKit exposes no atomic compare-and-write; the final target/source checks still have a check-to-write race.
      if intent.effect.kind == "delete" {
        guard let finalTarget, let target = intent.effect.target else { throw NativeFailure("source_changed") }
        crossedBoundary = true
        try store.remove(finalTarget, span: .thisEvent, commit: true)
        let physical: [String: Any] = ["kind": "deleted", "target": target.raw]
        let evidence: [String: Any] = ["kind": "native_acknowledgement", "host_epoch": nativeHostEpoch,
                                       "receipt_id": UUID().uuidString.lowercased()]
        outcome = committedOutcome(intent.identity.raw, effect: physical, evidence: evidence, committedAt: Date())
      } else {
        guard let schedule = intent.effect.schedule, let title = intent.effect.title else {
          throw NativeFailure("provider_rejected")
        }
        let event = finalTarget ?? EKEvent(eventStore: store)
        event.calendar = finalCalendar
        event.title = title
        event.startDate = schedule.start
        event.endDate = schedule.end
        event.timeZone = TimeZone(identifier: schedule.timezone)
        if intent.effect.kind == "create" {
          guard let marker else { throw NativeFailure("provider_rejected") }
          event.url = marker.1
          event.alarms = nil
        }
        try requirePermission()
        guard try verifyNativeSubject(store, intent.source) == fingerprint,
              Date() < deadline, Date() < preparation.expiresAt,
              Date() < preparation.authorizationExpiresAt,
              finalCalendar.allowsContentModifications, !finalCalendar.isSubscribed else {
          throw NativeFailure("provider_rejected")
        }
        crossedBoundary = true
        try store.save(event, span: .thisEvent, commit: true)
        guard nativeWriteMatches(event, effect: intent.effect, marker: marker?.1),
              intent.effect.kind != "create" || !event.hasAlarms else {
          throw NativeFailure("invalid_receipt")
        }
        let eventResult = try nativeWriteResult(event)
        let physical: [String: Any]
        if intent.effect.kind == "create" {
          physical = ["kind": "created", "event": eventResult]
        } else {
          guard let target = intent.effect.target else { throw NativeFailure("invalid_receipt") }
          physical = ["kind": "updated", "target": target.raw, "event": eventResult]
        }
        let evidence: [String: Any] = ["kind": "native_acknowledgement", "host_epoch": nativeHostEpoch,
                                       "receipt_id": UUID().uuidString.lowercased()]
        outcome = committedOutcome(intent.identity.raw, effect: physical, evidence: evidence, committedAt: Date())
      }
    } catch let error as NativeFailure {
      outcome = crossedBoundary
        ? unknownOutcome(intent.identity.raw, postDispatchUnknownReason(error))
        : notAppliedOutcome(intent.identity.raw, hostEpoch: nativeHostEpoch,
            invocationID: preparation.preparationID, reason: notAppliedReason(error), rejectedAt: Date())
    } catch {
      outcome = crossedBoundary
        ? unknownOutcome(intent.identity.raw, "response_lost")
        : notAppliedOutcome(intent.identity.raw, hostEpoch: nativeHostEpoch,
            invocationID: preparation.preparationID, reason: "provider_unavailable", rejectedAt: Date())
    }
    nativeActionCache.finish(intent, outcome: outcome)
    return outcome
  }
}

private func actionReadback(_ request: [String: Any]) throws -> [String: Any] {
  let keys: Set<String> = ["schema_version", "operation", "admission", "deadline"]
  guard exactActionRequest(request, operation: "action_readback", keys: keys),
        let deadline = actionDate(request["deadline"]) else {
    throw NativeFailure("uncertain_result")
  }
  let intent = try parseExecutionIntent(request["admission"])
  guard Date() < deadline, deadline.timeIntervalSinceNow <= 30 else {
    return unknownOutcome(intent.identity.raw, "timeout")
  }
  let outcome = nativeActionCache.readback(intent)
  guard Date() < deadline else { return unknownOutcome(intent.identity.raw, "timeout") }
  return outcome
}

private func actionLookup(_ request: [String: Any]) throws -> [String: Any] {
  let keys: Set<String> = ["schema_version", "operation", "admission", "deadline"]
  guard exactActionRequest(request, operation: "action_lookup", keys: keys),
        let deadline = actionDate(request["deadline"]) else { throw NativeFailure("uncertain_result") }
  let intent = try parseExecutionIntent(request["admission"])
  guard intent.effect.kind == "create" else {
    return unknownOutcome(intent.identity.raw, "inconclusive_lookup")
  }
  guard Date() < deadline, deadline.timeIntervalSinceNow <= 30 else {
    return unknownOutcome(intent.identity.raw, "timeout")
  }
  eventStoreLock.lock()
  defer { eventStoreLock.unlock() }
  do {
    try requirePermission()
    let store = EKEventStore()
    let generation = calendarViewGeneration.value()
    let fingerprint = try verifyNativeSubject(store, intent.source)
    let calendar = try nativeCalendar(store, intent.effect.destination, requireWrite: false)
    guard let schedule = intent.effect.schedule, let marker = markerValue(
      personID: intent.identity.personID, executionID: intent.identity.executionID) else {
      return unknownOutcome(intent.identity.raw, "inconclusive_lookup")
    }
    let predicate = store.predicateForEvents(
      withStart: schedule.start.addingTimeInterval(-86_400),
      end: schedule.end.addingTimeInterval(86_400), calendars: [calendar])
    let markerEvents = store.events(matching: predicate).filter {
      $0.url?.absoluteString == marker.0
    }
    try requirePermission()
    guard try verifyNativeSubject(store, intent.source) == fingerprint,
          generation == calendarViewGeneration.value(), Date() < deadline,
          markerEvents.count == 1,
          nativeWriteMatches(markerEvents[0], effect: intent.effect, marker: marker.1),
          !markerEvents[0].hasAlarms else {
      return unknownOutcome(intent.identity.raw, "inconclusive_lookup")
    }
    let observedAt = Date()
    let eventResult = try nativeWriteResult(markerEvents[0])
    let physical: [String: Any] = ["kind": "created", "event": eventResult]
    let evidence: [String: Any] = ["kind": "unique_create_marker", "observed_at": actionTimestamp(observedAt),
                                   "marker": marker.0]
    let outcome = committedOutcome(intent.identity.raw, effect: physical, evidence: evidence,
                                    committedAt: observedAt)
    nativeActionCache.recordLookup(intent, outcome: outcome)
    return outcome
  } catch {
    return unknownOutcome(intent.identity.raw, Date() >= deadline ? "timeout" : "inconclusive_lookup")
  }
}

private func runAction(_ request: [String: Any]) throws -> Any {
  guard let operation = request["operation"] as? String else { throw NativeFailure("uncertain_result") }
  if operation == "capabilities" { return ["writes_enabled": true] }
  if operation == "view_access" {
    eventStoreLock.lock()
    defer { eventStoreLock.unlock() }
    let store = EKEventStore()
    return try calendarViewAccess(request, permission: requirePermission,
      contains: { store.calendar(withIdentifier: $0) != nil }, generation: calendarViewGeneration.value,
      subjectFingerprint: { try nativeSubjectFingerprint(store, request["calendar_ids"] as! [String]) })
  }
  if operation == "observe" {
    eventStoreLock.lock()
    defer { eventStoreLock.unlock() }
    return try calendarObservation(request)
  }
  switch operation {
  case "action_destinations": return try actionDestinations(request)
  case "action_preflight": return try actionPreflight(request)
  case "action_dispatch": return try actionDispatch(request)
  case "action_readback": return try actionReadback(request)
  case "action_lookup": return try actionLookup(request)
  default: throw NativeFailure("uncertain_result")
  }
}

@_cdecl("floe_eventkit_action")
public func floeEventKitAction(_ input: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
  let response: [String: Any]
  do {
    guard let input = input, let bytes = String(cString: input).data(using: .utf8),
          bytes.count <= 65_536,
          let request = try JSONSerialization.jsonObject(with: bytes) as? [String: Any] else {
      throw NativeFailure("uncertain_result")
    }
    response = ["data": try runAction(request)]
  } catch let error as NativeFailure {
    response = ["error": error.reason]
  } catch {
    response = ["error": "uncertain_result"]
  }
  guard let data = try? JSONSerialization.data(withJSONObject: response, options: [.sortedKeys]),
        let text = String(data: data, encoding: .utf8) else { return nil }
  return strdup(text)
}

@_cdecl("floe_eventkit_free")
public func floeEventKitFree(_ value: UnsafeMutablePointer<CChar>?) { free(value) }
