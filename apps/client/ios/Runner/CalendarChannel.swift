import CryptoKit
import EventKit
import Flutter
import UIKit

@MainActor
final class CalendarChannel {
  private struct AcquisitionFailure: Error {
    let code: String
  }

  private static let channelName = "floe/calendar"
  private static let provider = "event_kit"

  private let channel: FlutterMethodChannel
  private let store: EKEventStore
  private var boundDeviceID: String?

  init(messenger: FlutterBinaryMessenger, store: EKEventStore = EKEventStore()) {
    channel = FlutterMethodChannel(name: Self.channelName, binaryMessenger: messenger)
    self.store = store
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(Self.failure("provider_unavailable", status: Self.authorizationStatus()))
        return
      }
      Task { @MainActor in
        await self.handle(call, result: result)
      }
    }
  }

  private func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) async {
    guard call.method == "readAcquisition" else {
      result(FlutterMethodNotImplemented)
      return
    }
    guard let arguments = call.arguments as? [String: Any],
          let deviceID = bindDeviceID(arguments)
    else {
      result(Self.failure("invalid_input", status: Self.authorizationStatus()))
      return
    }
    switch call.method {
    case "readAcquisition":
      do {
        if arguments["mode"] as? String == "request_permission" {
          result(try await requestPermissionAcquisition(arguments))
        } else {
          result(try readAcquisition(arguments))
        }
      } catch let error as AcquisitionFailure {
        result(failure(error.code))
      } catch {
        result(failure("provider_unavailable"))
      }
    default:
      assertionFailure("Known calendar method was not handled.")
      result(FlutterMethodNotImplemented)
    }
  }

  private func requestPermissionAcquisition(_ arguments: [String: Any]) async throws -> [String: Any] {
    guard arguments["mode"] as? String == "request_permission",
          let requestID = arguments["request_id"] as? String, UUID(uuidString: requestID) != nil,
          let personID = arguments["person_id"] as? String, UUID(uuidString: personID) != nil,
          let epoch = arguments["host_epoch"] as? String, !epoch.isEmpty, epoch.utf8.count <= 128,
          let connectionID = arguments["connection_id"] as? String, !connectionID.isEmpty,
          let revision = Self.integer(arguments["connection_revision"]), revision > 0,
          arguments["provider"] as? String == Self.provider,
          let calendarIDs = arguments["calendar_ids"] as? [String], calendarIDs.isEmpty,
          let start = Self.integer(arguments["range_start_unix_ms"]),
          let end = Self.integer(arguments["range_end_unix_ms"]), end > start,
          let deadline = Self.integer(arguments["deadline_unix_ms"]), deadline > Self.unixMilliseconds(),
          let deviceID = bindDeviceID(arguments)
    else { throw AcquisitionFailure(code: "invalid_input") }
    func fingerprint() throws -> String {
      let bytes = try JSONSerialization.data(withJSONObject: ["permission_class": Self.sourcePermissionClass(), "subjects": []] as [String: Any], options: [.sortedKeys])
      return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
    }
    let before = try fingerprint()
    let granted: Bool
    if canRead {
      granted = true
    } else if EKEventStore.authorizationStatus(for: .event) == .notDetermined {
      if #available(iOS 17.0, *) {
        granted = try await store.requestFullAccessToEvents()
      } else {
        granted = try await withCheckedThrowingContinuation { continuation in
          store.requestAccess(to: .event) { allowed, error in
            if let error { continuation.resume(throwing: error) }
            else { continuation.resume(returning: allowed) }
          }
        }
      }
    } else {
      granted = false
    }
    return ["request_id": requestID, "host_epoch": epoch, "person_id": personID,
            "device_id": deviceID, "connection_id": connectionID, "connection_revision": revision,
            "provider": Self.provider, "mode": "request_permission", "calendar_ids": calendarIDs,
            "range_start_unix_ms": start, "range_end_unix_ms": end,
            "native_subject_fingerprint_before": before, "native_subject_fingerprint_after": try fingerprint(),
            "available_calendar_ids": [], "available_calendars": [],
            "permission_class": granted ? "request_completed" : "denied", "batches": []]
  }

  private func readAcquisition(_ arguments: [String: Any]) throws -> [String: Any] {
    guard let requestID = arguments["request_id"] as? String,
          let hostEpoch = arguments["host_epoch"] as? String,
          let personID = arguments["person_id"] as? String,
          let connectionID = arguments["connection_id"] as? String,
          let deviceID = arguments["device_id"] as? String,
          let connectionRevision = Self.integer(arguments["connection_revision"]),
          let calendarIDs = arguments["calendar_ids"] as? [String],
          let start = Self.integer(arguments["range_start_unix_ms"]),
          let end = Self.integer(arguments["range_end_unix_ms"]),
          let deadline = Self.integer(arguments["deadline_unix_ms"]),
          let mode = arguments["mode"] as? String,
          let provider = arguments["provider"] as? String,
          provider == Self.provider,
          ["inspect_subject", "inspect_catalog", "read_events"].contains(mode),
          !requestID.isEmpty, !hostEpoch.isEmpty, !personID.isEmpty, !connectionID.isEmpty,
          connectionRevision > 0,
          (mode == "inspect_catalog" ? calendarIDs.isEmpty : !calendarIDs.isEmpty),
          calendarIDs == calendarIDs.sorted(), Set(calendarIDs).count == calendarIDs.count,
          calendarIDs.allSatisfy({ !$0.isEmpty && $0.utf8.count <= 512 }),
          start >= 0, end > start, end - start <= 32 * 86_400_000,
          deadline > Self.unixMilliseconds(), deadline - Self.unixMilliseconds() <= 30_000
    else { throw AcquisitionFailure(code: "invalid_input") }
    guard bindDeviceID(arguments) == deviceID else { throw AcquisitionFailure(code: "stale_context") }

    guard canRead else { throw AcquisitionFailure(code: "permission_denied") }
    store.reset()
    let evidenceBefore = try subjectEvidence(calendarIDs: calendarIDs)
    guard calendarIDs.allSatisfy(evidenceBefore.availableCalendarIDs.contains) else {
      throw AcquisitionFailure(code: "calendar_unavailable")
    }
    if mode == "read_events" {
      guard canRead,
            let expected = arguments["expected_native_subject_fingerprint"] as? String,
            expected == evidenceBefore.fingerprint
      else { throw AcquisitionFailure(code: "permission_denied") }
    } else if arguments["expected_native_subject_fingerprint"] != nil {
      throw AcquisitionFailure(code: "invalid_input")
    }
    guard deadline > Self.unixMilliseconds() else { throw AcquisitionFailure(code: "deadline_exceeded") }

    var batches: [[String: Any]] = []
    if mode == "read_events" {
      let startDate = Date(timeIntervalSince1970: TimeInterval(start) / 1000)
      let endDate = Date(timeIntervalSince1970: TimeInterval(end) / 1000)
      var total = 0
      for calendarID in calendarIDs {
        guard deadline > Self.unixMilliseconds() else { throw AcquisitionFailure(code: "deadline_exceeded") }
        guard let calendar = store.calendar(withIdentifier: calendarID) else {
          throw AcquisitionFailure(code: "calendar_unavailable")
        }
        let predicate = store.predicateForEvents(withStart: startDate, end: endDate, calendars: [calendar])
        let events = store.events(matching: predicate)
        guard events.count <= 128 - total else { throw AcquisitionFailure(code: "provider_unavailable") }
        var records: [[String: Any]] = []
        for event in events {
          total += 1
          records.append(try acquisitionRecord(event))
          let encoded = try JSONSerialization.data(withJSONObject: batches + [[
            "calendar_id": calendarID,
            "records": records,
            "failure": NSNull(),
          ]], options: [.sortedKeys])
          guard encoded.count <= 65_536 else { throw AcquisitionFailure(code: "provider_unavailable") }
        }
        batches.append([
          "calendar_id": calendarID,
          "records": records,
          "failure": NSNull(),
        ])
      }
    }
    let evidenceAfter = try subjectEvidence(calendarIDs: calendarIDs)
    guard canRead, evidenceAfter.fingerprint == evidenceBefore.fingerprint,
          evidenceAfter.availableCalendars == evidenceBefore.availableCalendars,
          deadline > Self.unixMilliseconds()
    else { throw AcquisitionFailure(code: "stale_context") }
    let response: [String: Any] = [
      "request_id": requestID,
      "host_epoch": hostEpoch,
      "person_id": personID,
      "device_id": deviceID,
      "connection_id": connectionID,
      "connection_revision": Int(connectionRevision),
      "provider": provider,
      "mode": mode,
      "calendar_ids": calendarIDs,
      "range_start_unix_ms": start,
      "range_end_unix_ms": end,
      "native_subject_fingerprint_before": evidenceBefore.fingerprint,
      "native_subject_fingerprint_after": evidenceAfter.fingerprint,
      "available_calendar_ids": evidenceBefore.availableCalendarIDs,
      "available_calendars": evidenceBefore.availableCalendars,
      "permission_class": evidenceBefore.permissionClass,
      "batches": batches,
    ]
    guard JSONSerialization.isValidJSONObject(response),
          (try JSONSerialization.data(withJSONObject: response)).count <= 65_536
    else { throw AcquisitionFailure(code: "provider_unavailable") }
    return response
  }

  private func subjectEvidence(calendarIDs: [String]) throws -> (fingerprint: String, availableCalendarIDs: [String], availableCalendars: [[String: String]], permissionClass: String) {
    let permissionClass = Self.sourcePermissionClass()
    let calendars = store.calendars(for: .event).sorted { $0.calendarIdentifier < $1.calendarIdentifier }
    guard calendars.count <= 256, Set(calendars.map(\.calendarIdentifier)).count == calendars.count else {
      throw AcquisitionFailure(code: "provider_unavailable")
    }
    let resources = try calendars.map { calendar -> [String: String] in
      let label = calendar.title ?? ""
      let groupHandle = calendar.source.sourceIdentifier ?? ""
      let groupLabel = calendar.source.title ?? ""
      guard !calendar.calendarIdentifier.isEmpty, calendar.calendarIdentifier.utf8.count <= 512,
            !label.isEmpty, label.utf8.count <= 256,
            !groupHandle.isEmpty, groupHandle.utf8.count <= 512,
            !groupLabel.isEmpty, groupLabel.utf8.count <= 256,
            ![label, groupHandle, groupLabel].contains(where: { $0.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) })
      else { throw AcquisitionFailure(code: "provider_unavailable") }
      return ["handle": calendar.calendarIdentifier, "label": label, "group_handle": groupHandle, "group_label": groupLabel]
    }
    let subjects = try calendarIDs.map { identifier -> [String] in
      guard let calendar = calendars.first(where: { $0.calendarIdentifier == identifier }) else {
        throw AcquisitionFailure(code: "calendar_unavailable")
      }
      return [calendar.calendarIdentifier, calendar.source.sourceIdentifier, String(calendar.source.sourceType.rawValue)]
    }
    let payload: [String: Any] = ["permission_class": permissionClass, "subjects": subjects]
    let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
    let fingerprint = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    return (fingerprint, calendars.map(\.calendarIdentifier), resources, permissionClass)
  }

  private static func sourcePermissionClass() -> String {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(iOS 17.0, *) {
      switch status {
      case .fullAccess: return "full"
      case .writeOnly: return "write_only"
      case .authorized: return "authorized"
      case .denied: return "denied"
      case .restricted: return "restricted"
      case .notDetermined: return "not_determined"
      @unknown default: return "unknown"
      }
    }
    switch status {
    case .authorized: return "authorized"
    case .denied: return "denied"
    case .restricted: return "restricted"
    case .notDetermined: return "not_determined"
    default: return "unknown"
    }
  }

  private func acquisitionRecord(_ event: EKEvent) throws -> [String: Any] {
    let value = record(event)
    guard let externalID = value["external_id"] as? String,
          let externalRevision = value["external_revision"] as? String,
          let title = value["title"] as? String,
          externalID.utf8.count <= 512,
          externalRevision.utf8.count <= 512,
          title.utf8.count <= 4_096
    else { throw AcquisitionFailure(code: "provider_unavailable") }
    return [
      "calendar_id": event.calendar.calendarIdentifier,
      "external_id": externalID,
      "external_revision": externalRevision,
      "title": title,
      "schedule": value["schedule"]!,
      "can_modify": value["can_modify"]!,
    ]
  }

  private var canRead: Bool {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(iOS 17.0, *) { return status == .fullAccess }
    return status == .authorized
  }

  private func bindDeviceID(_ arguments: [String: Any]) -> String? {
    guard let value = arguments["device_id"] as? String,
          !value.isEmpty,
          value.utf8.count <= 128,
          !value.contains(where: { $0.isWhitespace })
    else { return nil }
    if let boundDeviceID, boundDeviceID != value { return nil }
    boundDeviceID = value
    return value
  }

  private func record(_ event: EKEvent) -> [String: Any] {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let identifier = event.calendarItemIdentifier
    let occurrence = (event.hasRecurrenceRules || event.isDetached)
      ? event.occurrenceDate.map { formatter.string(from: $0) } ?? "" : ""
    let dayFormatter = DateFormatter()
    dayFormatter.calendar = Calendar(identifier: .gregorian)
    dayFormatter.locale = Locale(identifier: "en_US_POSIX")
    dayFormatter.timeZone = event.timeZone ?? TimeZone.current
    dayFormatter.dateFormat = "yyyy-MM-dd"
    let allDayEnd = normalizedAllDayEndExclusive(
      start: event.startDate,
      end: event.endDate,
      calendar: dayFormatter.calendar
    )
    let schedule: [String: Any] = event.isAllDay ? [
      "kind": "all_day",
      "start_date": dayFormatter.string(from: event.startDate),
      "end_date_exclusive": dayFormatter.string(from: allDayEnd),
    ] : [
      "kind": "timed",
      "starts_at": formatter.string(from: event.startDate),
      "ends_at": formatter.string(from: event.endDate),
      "timezone": (event.timeZone ?? TimeZone.current).identifier,
    ]
    // Fingerprint input is a private native comparison contract, separate
    // from the flat transport schedule. Keep it identical to EventKitActions.
    let fingerprintSchedule: [String: Any] = event.isAllDay ? ["AllDay": [
      "start_date": dayFormatter.string(from: event.startDate),
      "end_date_exclusive": dayFormatter.string(from: allDayEnd),
    ]] : ["Timed": [
      "starts_at": formatter.string(from: event.startDate),
      "ends_at": formatter.string(from: event.endDate),
      "timezone": (event.timeZone ?? TimeZone.current).identifier,
    ]]
    let rawTitle = event.title ?? ""
    let revisionData = try! JSONSerialization.data(
      withJSONObject: [
        "title": rawTitle,
        "schedule": fingerprintSchedule,
        "modified": event.lastModifiedDate.map { formatter.string(from: $0) } ?? "",
      ],
      options: [.sortedKeys]
    )
    let revision = SHA256.hash(data: revisionData).map { String(format: "%02x", $0) }.joined()
    return [
      "external_id": "\(identifier)|\(occurrence)",
      "can_modify": event.calendar.allowsContentModifications && !event.calendar.isSubscribed &&
        !event.isAllDay && !event.hasRecurrenceRules && !event.isDetached && !event.hasAttendees &&
        event.endDate > event.startDate && event.endDate.timeIntervalSince(event.startDate) <= 86_400,
      "external_revision": revision,
      "title": rawTitle,
      "schedule": schedule,
      "provider": Self.provider,
      "device_id": boundDeviceID!,
    ]
  }

  private func failure(_ code: String) -> FlutterError {
    Self.failure(code, status: Self.authorizationStatus(), deviceID: boundDeviceID)
  }

  private static func failure(
    _ code: String,
    status: String,
    deviceID: String? = nil
  ) -> FlutterError {
    var details: [String: Any] = [
      "provider": provider,
      "authorization_status": status,
    ]
    if let deviceID { details["device_id"] = deviceID }
    return FlutterError(
      code: code,
      message: "Check your Calendar connection and try again.",
      details: details
    )
  }

  private static func authorizationStatus() -> String {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(iOS 17.0, *) {
      switch status {
      case .notDetermined: return "not_determined"
      case .restricted: return "restricted"
      case .denied: return "denied"
      case .fullAccess, .authorized: return "full_access"
      case .writeOnly: return "write_only"
      @unknown default: return "unknown"
      }
    } else {
      switch status {
      case .notDetermined: return "not_determined"
      case .restricted: return "restricted"
      case .denied: return "denied"
      case .authorized: return "full_access"
      default: return "unknown"
      }
    }
  }

  private static func date(_ value: String) -> Date? {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter.date(from: value)
  }

  private static func integer(_ value: Any?) -> Int64? {
    if let value = value as? Int { return Int64(value) }
    if let value = value as? Int64 { return value }
    if let value = value as? NSNumber { return value.int64Value }
    return nil
  }

  private static func unixMilliseconds() -> Int64 {
    Int64(Date().timeIntervalSince1970 * 1000)
  }
}

func normalizedAllDayEndExclusive(start: Date, end: Date, calendar: Calendar) -> Date {
  let startDay = calendar.startOfDay(for: start)
  let endDay = calendar.startOfDay(for: end)
  let candidate = end > endDay
    ? calendar.date(byAdding: .day, value: 1, to: endDay)!
    : endDay
  return candidate > startDay
    ? candidate
    : calendar.date(byAdding: .day, value: 1, to: startDay)!
}
