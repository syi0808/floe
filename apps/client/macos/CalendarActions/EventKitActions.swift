import Foundation
import EventKit

private let actionLock = NSLock()
private let localPerson = "00000000-0000-4000-8000-000000000001"

private struct NativeFailure: Error {
  let reason: String
  init(_ reason: String) { self.reason = reason }
}

private func timestamp(_ value: Any?) throws -> Date {
  guard let text = value as? String else { throw NativeFailure("uncertain_result") }
  let formatter = ISO8601DateFormatter()
  formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  if let date = formatter.date(from: text) { return date }
  formatter.formatOptions = [.withInternetDateTime]
  guard let date = formatter.date(from: text) else { throw NativeFailure("uncertain_result") }
  return date
}

private var writesEnabled: Bool {
  #if FLOE_CALENDAR_WRITES
  return true
  #else
  return false
  #endif
}

private func requirePermission() throws {
  let status = EKEventStore.authorizationStatus(for: .event)
  if #available(macOS 14.0, *) {
    guard status == .fullAccess else { throw NativeFailure("permission_denied") }
  } else {
    guard status == .authorized else { throw NativeFailure("permission_denied") }
  }
}

struct Proposal {
  let raw: [String: Any]
  let person: String
  let calendarID: String
  let title: String
  let start: Date
  let end: Date
  let timezone: String
  let execution: String
  let expiry: Date
  let marker: URL

  init(_ raw: [String: Any]) throws {
    guard let person = raw["person_id"] as? String, person == localPerson,
          raw["provider"] as? String == "event_kit",
          let calendarID = raw["calendar_id"] as? String, !calendarID.isEmpty,
          let title = raw["title"] as? String, !title.isEmpty,
          let schedule = raw["schedule"] as? [String: Any],
          let timezone = schedule["timezone"] as? String,
          let execution = raw["execution_id"] as? String, UUID(uuidString: execution) != nil,
          let marker = URL(string: "floe://calendar-action/\(person)/\(execution)") else {
      throw NativeFailure("uncertain_result")
    }
    self.raw = raw
    self.person = person
    self.calendarID = calendarID
    self.title = title
    self.start = try timestamp(schedule["starts_at"])
    self.end = try timestamp(schedule["ends_at"])
    self.expiry = try timestamp(raw["expires_at"])
    self.timezone = timezone
    self.execution = execution
    self.marker = marker
    guard end > start, end.timeIntervalSince(start) <= 86400 else { throw NativeFailure("uncertain_result") }
  }

  func matches(_ event: EKEvent) -> Bool {
    event.url == marker && event.calendar.calendarIdentifier == calendarID && event.title == title &&
      abs(event.startDate.timeIntervalSince(start)) < 0.001 && abs(event.endDate.timeIntervalSince(end)) < 0.001 &&
      event.timeZone?.identifier == timezone && !event.isAllDay && !event.hasRecurrenceRules &&
      !event.hasAttendees && !event.hasAlarms
  }

  func receipt(_ event: EKEvent) -> [String: Any] {
    ["execution_id": execution, "person_id": person, "provider": "event_kit",
     "calendar_id": calendarID, "external_id": "\(event.calendarItemIdentifier)|",
     "title": title, "schedule": raw["schedule"]!]
  }
}

func localConflict(_ records: [[String: Any]], _ proposal: Proposal) throws -> Bool {
  for record in records where record["deleted_at"] is NSNull {
    guard let schedule = record["schedule"] as? [String: Any] else { throw NativeFailure("uncertain_result") }
    if let timed = schedule["Timed"] as? [String: Any] {
      if try timestamp(timed["starts_at"]) < proposal.end && timestamp(timed["ends_at"]) > proposal.start { return true }
    } else if let allDay = schedule["AllDay"] as? [String: Any],
              let startText = allDay["start_date"] as? String, let endText = allDay["end_date_exclusive"] as? String {
      let start = try timestamp(startText + "T00:00:00Z").addingTimeInterval(-14 * 3600)
      let end = try timestamp(endText + "T00:00:00Z").addingTimeInterval(14 * 3600)
      if start < proposal.end && end > proposal.start { return true }
    } else { throw NativeFailure("uncertain_result") }
  }
  return false
}

private func runAction(_ request: [String: Any]) throws -> Any {
  guard let operation = request["operation"] as? String else { throw NativeFailure("uncertain_result") }
  if operation == "capabilities" { return ["writes_enabled": writesEnabled] }
  guard let raw = request["action"] as? [String: Any] else { throw NativeFailure("uncertain_result") }
  let proposal = try Proposal(raw)
  let deadline = try timestamp(request["deadline"])
  guard Date() < deadline else { throw NativeFailure("timeout") }
  try requirePermission()
  let store = EKEventStore()
  guard let target = store.calendar(withIdentifier: proposal.calendarID) else { throw NativeFailure("provider_unavailable") }
  let predicate = store.predicateForEvents(withStart: proposal.start.addingTimeInterval(-86400),
    end: proposal.end.addingTimeInterval(86400), calendars: [target])
  if operation == "lookup" {
    let matches = store.events(matching: predicate).filter { $0.url == proposal.marker }
    try requirePermission()
    guard Date() < deadline else { throw NativeFailure("timeout") }
    guard matches.count == 1, proposal.matches(matches[0]) else { throw NativeFailure("uncertain_result") }
    return matches.map { proposal.receipt($0) }
  }
  guard operation == "preflight" || operation == "create", writesEnabled,
        let identifiers = request["calendar_ids"] as? [String], identifiers.contains(proposal.calendarID),
        let records = request["local_events"] as? [[String: Any]],
        let state = raw["state"] as? [String: Any], state["status"] as? String == "executing",
        let approved = raw["approved_at"], !(approved is NSNull) else { throw NativeFailure("permission_denied") }
  let approvedAt = try timestamp(approved)
  guard Date() >= approvedAt, Date() < proposal.expiry else { throw NativeFailure("timeout") }
  let calendars = identifiers.compactMap { store.calendar(withIdentifier: $0) }
  guard calendars.count == identifiers.count,
        raw["calendar_name"] as? String == "\(target.source.title) · \(target.title)" else { throw NativeFailure("provider_unavailable") }
  let canCreate = target.allowsContentModifications && !target.isSubscribed
  let supportedZone = proposal.timezone == "Etc/UTC" || TimeZone.knownTimeZoneIdentifiers.contains(proposal.timezone)
  let zone = supportedZone ? TimeZone(identifier: proposal.timezone) : nil
  let events = store.events(matching: store.predicateForEvents(withStart: proposal.start, end: proposal.end, calendars: calendars))
  let hasLocalConflict = try localConflict(records, proposal)
  let conflict = events.contains { $0.startDate < proposal.end && $0.endDate > proposal.start } || hasLocalConflict
  try requirePermission()
  if operation == "preflight" {
    return ["person_id": proposal.person, "provider": "event_kit", "calendar_id": proposal.calendarID,
      "permission_granted": true, "can_create": canCreate, "timezone_valid": zone != nil, "has_conflict": conflict]
  }
  guard canCreate, let zone = zone, !conflict else { throw NativeFailure("uncertain_result") }
  guard store.events(matching: predicate).allSatisfy({ $0.url != proposal.marker }) else { throw NativeFailure("uncertain_result") }
  let event = EKEvent(eventStore: store)
  event.calendar = target
  event.title = proposal.title
  event.startDate = proposal.start
  event.endDate = proposal.end
  event.timeZone = zone
  event.url = proposal.marker
  event.alarms = nil
  try requirePermission()
  guard Date() < deadline, Date() < proposal.expiry, target.allowsContentModifications else { throw NativeFailure("timeout") }
  try store.save(event, span: .thisEvent, commit: true)
  guard proposal.matches(event), !event.calendarItemIdentifier.isEmpty else { throw NativeFailure("uncertain_result") }
  return proposal.receipt(event)
}

@_cdecl("floe_eventkit_action")
public func floeEventKitAction(_ input: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
  actionLock.lock()
  defer { actionLock.unlock() }
  let response: [String: Any]
  do {
    guard let input = input, let bytes = String(cString: input).data(using: .utf8),
          let request = try JSONSerialization.jsonObject(with: bytes) as? [String: Any] else { throw NativeFailure("uncertain_result") }
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
