import CryptoKit
import EventKit
import Flutter
import UIKit

@MainActor
final class CalendarChannel {
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
    guard ["settings", "calendars", "read"].contains(call.method) else {
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
    case "settings":
      guard await UIApplication.shared.open(URL(string: UIApplication.openSettingsURLString)!) else {
        result(failure("provider_unavailable"))
        return
      }
      result(nil)
    case "calendars":
      let requestAccess = arguments["request_access"] as? Bool ?? true
      guard await ensureReadAccess(requestIfNeeded: requestAccess) else {
        result(failure("permission_denied"))
        return
      }
      result(
        store.calendars(for: .event).map {
          [
            "id": $0.calendarIdentifier,
            "name": "\($0.source.title) · \($0.title)",
            "provider": Self.provider,
            "device_id": deviceID,
          ]
        }
      )
    case "read":
      guard canRead else {
        result(failure("permission_denied"))
        return
      }
      guard let identifier = arguments["calendar_id"] as? String,
            let startText = arguments["starts_at"] as? String,
            let endText = arguments["ends_at"] as? String,
            let start = Self.date(startText),
            let end = Self.date(endText),
            end > start,
            end.timeIntervalSince(start) <= 32 * 86_400
      else {
        result(failure("provider_unavailable"))
        return
      }
      store.reset()
      guard canRead else {
        result(failure("permission_denied"))
        return
      }
      guard let calendar = store.calendar(withIdentifier: identifier) else {
        result(failure("calendar_unavailable"))
        return
      }
      let predicate = store.predicateForEvents(withStart: start, end: end, calendars: [calendar])
      let records = store.events(matching: predicate).map(record)
      guard canRead else {
        result(failure("permission_denied"))
        return
      }
      result(records)
    default:
      assertionFailure("Known calendar method was not handled.")
      result(FlutterMethodNotImplemented)
    }
  }

  private var canRead: Bool {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(iOS 17.0, *) { return status == .fullAccess }
    return status == .authorized
  }

  private func ensureReadAccess(requestIfNeeded: Bool) async -> Bool {
    if canRead { return true }
    guard requestIfNeeded, EKEventStore.authorizationStatus(for: .event) == .notDetermined else {
      return false
    }
    do {
      if #available(iOS 17.0, *) {
        return try await store.requestFullAccessToEvents()
      }
      return try await withCheckedThrowingContinuation { continuation in
        store.requestAccess(to: .event) { granted, error in
          if let error { continuation.resume(throwing: error) }
          else { continuation.resume(returning: granted) }
        }
      }
    } catch {
      return false
    }
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
    let schedule: [String: Any] = event.isAllDay ? [
      "kind": "all_day",
      "start_date": dayFormatter.string(from: event.startDate),
      "end_date_exclusive": dayFormatter.string(from: event.endDate),
    ] : [
      "kind": "timed",
      "starts_at": formatter.string(from: event.startDate),
      "ends_at": formatter.string(from: event.endDate),
      "timezone": (event.timeZone ?? TimeZone.current).identifier,
    ]
    let title = event.title?.trimmingCharacters(in: .whitespacesAndNewlines)
    let normalizedTitle = title?.isEmpty == false ? title! : "(Untitled)"
    let revisionData = try! JSONSerialization.data(
      withJSONObject: [
        "title": normalizedTitle,
        "schedule": schedule,
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
      "title": normalizedTitle,
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
}
