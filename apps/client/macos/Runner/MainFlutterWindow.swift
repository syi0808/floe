import Cocoa
import FlutterMacOS
import EventKit
import CryptoKit

class MainFlutterWindow: NSWindow {
  private let calendarBridge = CalendarBridge()
  private let attentionBridge = MacOSAttentionBridge()
  private var designFeedbackChannel: FlutterMethodChannel?

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    styleMask.insert(.fullSizeContentView)
    titleVisibility = .hidden
    titlebarAppearsTransparent = true
    isMovableByWindowBackground = true
    if #available(macOS 11.0, *) {
      titlebarSeparatorStyle = .none
    }

    RegisterGeneratedPlugins(registry: flutterViewController)
    let channel = FlutterMethodChannel(name: "floe/calendar", binaryMessenger: flutterViewController.engine.binaryMessenger)
    channel.setMethodCallHandler(calendarBridge.handle)
    let contextChannel = FlutterMethodChannel(name: "floe/macos_context", binaryMessenger: flutterViewController.engine.binaryMessenger)
    contextChannel.setMethodCallHandler(attentionBridge.handle)
    designFeedbackChannel = FlutterMethodChannel(name: "floe/design-feedback", binaryMessenger: flutterViewController.engine.binaryMessenger)
#if DEBUG
    installDesignFeedbackMenuItem()
#endif

    super.awakeFromNib()
  }

  private func installDesignFeedbackMenuItem() {
    guard let menu = NSApp.mainMenu?.item(withTitle: "View")?.submenu else { return }
    let item = NSMenuItem(title: "Toggle Design Feedback", action: #selector(toggleDesignFeedback), keyEquivalent: "f")
    item.keyEquivalentModifierMask = [.command, .shift]
    item.target = self
    menu.addItem(NSMenuItem.separator())
    menu.addItem(item)
  }

  @objc private func toggleDesignFeedback() {
    designFeedbackChannel?.invokeMethod("toggle", arguments: nil)
  }
}

final class CalendarBridge {
  private struct AcquisitionFailure: Error { let code: String }
  private static let provider = "event_kit"
  private var boundDeviceID: String?
  private let store = EKEventStore()
  private let queue = DispatchQueue(label: "floe.calendar.read")

  private var canRead: Bool {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(macOS 14.0, *) { return status == .fullAccess }
    return status == .authorized
  }

  func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    let noArguments = call.arguments == nil || call.arguments is NSNull
    switch call.method {
    case "inspectSystemAccess":
      guard noArguments else {
        result(FlutterError(code: "invalid_input", message: "No arguments expected.", details: nil)); return
      }
      // Authorization metadata only: no calendar enumeration and no permission prompt.
      let status = EKEventStore.authorizationStatus(for: .event)
      if #available(macOS 14.0, *) {
        if status == .fullAccess { result("allowed"); return }
        if status == .writeOnly { result("write_only"); return }
      } else if status == .authorized { result("allowed"); return }
      switch status {
      case .notDetermined: result("not_requested")
      case .denied: result("denied")
      case .restricted: result("restricted")
      default: result("unavailable")
      }
    case "openSystemAccessSettings":
      guard noArguments,
            let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars") else {
        result(FlutterError(code: "invalid_input", message: "Invalid settings navigation.", details: nil)); return
      }
      // Opening settings never changes authorization or grants Floe source access.
      result(NSWorkspace.shared.open(url))
    case "readAcquisition":
      guard let arguments = call.arguments as? [String: Any] else {
        result(failure("invalid_input")); return
      }
      if arguments["mode"] as? String == "request_permission" {
        Task {
          do { result(try await self.requestPermissionAcquisition(arguments)) }
          catch let error as AcquisitionFailure { result(self.failure(error.code)) }
          catch { result(self.failure("provider_unavailable")) }
        }
        return
      }
      queue.async {
        do {
          let response = try self.readAcquisition(arguments)
          DispatchQueue.main.async { result(response) }
        } catch let error as AcquisitionFailure {
          DispatchQueue.main.async { result(self.failure(error.code)) }
        } catch {
          DispatchQueue.main.async { result(self.failure("provider_unavailable")) }
        }
      }
    default:
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
      if #available(macOS 14.0, *) {
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
      let calendarSource: EKSource? = calendar.source
      guard let calendarSource else { throw AcquisitionFailure(code: "provider_unavailable") }
      let label = calendar.title ?? ""
      let groupHandle = calendarSource.sourceIdentifier ?? ""
      let groupLabel = calendarSource.title ?? ""
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
      let calendarSource: EKSource? = calendar.source
      guard let calendarSource else { throw AcquisitionFailure(code: "provider_unavailable") }
      return [calendar.calendarIdentifier, calendarSource.sourceIdentifier, String(calendarSource.sourceType.rawValue)]
    }
    let payload: [String: Any] = ["permission_class": permissionClass, "subjects": subjects]
    let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
    let fingerprint = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    return (fingerprint, calendars.map(\.calendarIdentifier), resources, permissionClass)
  }

  private static func sourcePermissionClass() -> String {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(macOS 14.0, *) {
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

  private static func integer(_ value: Any?) -> Int64? {
    if let value = value as? Int { return Int64(value) }
    if let value = value as? Int64 { return value }
    if let value = value as? NSNumber { return value.int64Value }
    return nil
  }

  private static func unixMilliseconds() -> Int64 {
    Int64(Date().timeIntervalSince1970 * 1000)
  }
  private func record(_ event: EKEvent) -> [String: Any] {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let identifier = event.calendarItemIdentifier
    let occurrence = (event.hasRecurrenceRules || event.isDetached)
      ? event.occurrenceDate.map { formatter.string(from: $0) } ?? "" : ""
    let dateFormatter = DateFormatter()
    dateFormatter.calendar = Calendar(identifier: .gregorian)
    dateFormatter.locale = Locale(identifier: "en_US_POSIX")
    dateFormatter.timeZone = event.timeZone ?? TimeZone.current
    dateFormatter.dateFormat = "yyyy-MM-dd"
    let allDayEnd = normalizedAllDayEndExclusive(
      start: event.startDate,
      end: event.endDate,
      calendar: dateFormatter.calendar
    )
    let schedule: [String: Any] = event.isAllDay ? [
      "kind": "all_day", "start_date": dateFormatter.string(from: event.startDate),
      "end_date_exclusive": dateFormatter.string(from: allDayEnd)
    ] : [
      "kind": "timed", "starts_at": formatter.string(from: event.startDate),
      "ends_at": formatter.string(from: event.endDate),
      "timezone": (event.timeZone ?? TimeZone.current).identifier
    ]
    let title = event.title?.trimmingCharacters(in: .whitespacesAndNewlines)
    let normalizedTitle = title?.isEmpty == false ? title! : "(Untitled)"
    let revisionData = try! JSONSerialization.data(withJSONObject: [
      "title": normalizedTitle, "schedule": schedule,
      "modified": event.lastModifiedDate.map { formatter.string(from: $0) } ?? ""
    ], options: [.sortedKeys])
    let revision = SHA256.hash(data: revisionData).map { String(format: "%02x", $0) }.joined()
    return [
      "external_id": "\(identifier)|\(occurrence)",
      "can_modify": event.calendar.allowsContentModifications && !event.calendar.isSubscribed &&
        !event.isAllDay && !event.hasRecurrenceRules && !event.isDetached && !event.hasAttendees &&
        event.endDate > event.startDate && event.endDate.timeIntervalSince(event.startDate) <= 86400,
      "external_revision": revision,
      "title": normalizedTitle,
      "schedule": schedule
    ]
  }

  private func failure(_ code: String) -> FlutterError {
    FlutterError(code: code, message: "Check your Calendar connection and try again.", details: nil)
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
