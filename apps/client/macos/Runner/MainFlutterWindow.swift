import Cocoa
import FlutterMacOS
import EventKit
import CryptoKit

class MainFlutterWindow: NSWindow {
  private let calendarBridge = CalendarBridge()
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

    super.awakeFromNib()
  }
}

final class CalendarBridge {
  private let store = EKEventStore()
  private let queue = DispatchQueue(label: "floe.calendar.read")

  private var canRead: Bool {
    let status = EKEventStore.authorizationStatus(for: .event)
    if #available(macOS 14.0, *) { return status == .fullAccess }
    return status == .authorized
  }

  func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "settings":
      NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars")!)
      result(nil)
    case "calendars":
      if canRead { list(result); return }
      let completion: (Bool, Error?) -> Void = { granted, _ in
        DispatchQueue.main.async {
          if granted { self.list(result) }
          else { result(self.failure("permission_denied")) }
        }
      }
      if #available(macOS 14.0, *) {
        store.requestFullAccessToEvents(completion: completion)
      } else {
        store.requestAccess(to: .event, completion: completion)
      }
    case "read":
      guard let arguments = call.arguments as? [String: Any],
            let identifier = arguments["calendar_id"] as? String,
            let startText = arguments["starts_at"] as? String,
            let endText = arguments["ends_at"] as? String else {
        result(failure("provider_unavailable")); return
      }
      let formatter = ISO8601DateFormatter()
      formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
      guard let start = formatter.date(from: startText),
            let end = formatter.date(from: endText), end > start,
            end.timeIntervalSince(start) <= 32 * 86400 else {
        result(failure("provider_unavailable")); return
      }
      queue.async {
        guard self.canRead else {
          DispatchQueue.main.async { result(self.failure("permission_denied")) }; return
        }
        self.store.reset()
        guard let calendar = self.store.calendar(withIdentifier: identifier) else {
          DispatchQueue.main.async { result(self.failure("calendar_unavailable")) }; return
        }
        let predicate = self.store.predicateForEvents(withStart: start, end: end, calendars: [calendar])
        let events = self.store.events(matching: predicate)
        guard self.canRead else {
          DispatchQueue.main.async { result(self.failure("permission_denied")) }; return
        }
        let records = events.map { self.record($0) }
        DispatchQueue.main.async { result(records) }
      }
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  private func list(_ result: @escaping FlutterResult) {
    queue.async {
      guard self.canRead else {
        DispatchQueue.main.async { result(self.failure("permission_denied")) }; return
      }
      self.store.reset()
      let calendars = self.store.calendars(for: .event).map {
        ["id": $0.calendarIdentifier, "name": "\($0.source.title) · \($0.title)"]
      }
      DispatchQueue.main.async { result(calendars) }
    }
  }

  private func record(_ event: EKEvent) -> [String: Any] {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let identifier = event.calendarItemIdentifier
    let occurrence = event.occurrenceDate.map { formatter.string(from: $0) } ?? ""
    let dateFormatter = DateFormatter()
    dateFormatter.calendar = Calendar(identifier: .gregorian)
    dateFormatter.locale = Locale(identifier: "en_US_POSIX")
    dateFormatter.timeZone = event.timeZone ?? TimeZone.current
    dateFormatter.dateFormat = "yyyy-MM-dd"
    let schedule: [String: Any] = event.isAllDay ? [
      "kind": "all_day", "start_date": dateFormatter.string(from: event.startDate),
      "end_date_exclusive": dateFormatter.string(from: event.endDate)
    ] : [
      "kind": "timed", "starts_at": formatter.string(from: event.startDate),
      "ends_at": formatter.string(from: event.endDate),
      "timezone": (event.timeZone ?? TimeZone.current).identifier
    ]
    let title = event.title?.trimmingCharacters(in: .whitespacesAndNewlines)
    let normalizedTitle = title?.isEmpty == false ? title! : "(제목 없음)"
    let revisionData = try! JSONSerialization.data(withJSONObject: [
      "title": normalizedTitle, "schedule": schedule,
      "modified": event.lastModifiedDate.map { formatter.string(from: $0) } ?? ""
    ], options: [.sortedKeys])
    let revision = SHA256.hash(data: revisionData).map { String(format: "%02x", $0) }.joined()
    return [
      "external_id": "\(identifier)|\(occurrence)",
      "external_revision": revision,
      "title": normalizedTitle,
      "schedule": schedule
    ]
  }

  private func failure(_ code: String) -> FlutterError {
    FlutterError(code: code, message: "Calendar 연결을 확인하고 다시 시도해 주세요.", details: nil)
  }
}
