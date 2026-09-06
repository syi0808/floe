import Cocoa
import FlutterMacOS
import EventKit
import CryptoKit
import Security

class MainFlutterWindow: NSWindow {
  private let calendarBridge = CalendarBridge()
  private let serverBridge = LocalServerBridge()
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
    let serverChannel = FlutterMethodChannel(name: "floe/local-server", binaryMessenger: flutterViewController.engine.binaryMessenger)
    serverChannel.setMethodCallHandler(serverBridge.handle)
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

final class LocalServerBridge {
  private var query: [String: Any] {
    [kSecClass as String: kSecClassGenericPassword,
     kSecAttrService as String: "app.floe.local-server",
     kSecAttrAccount as String: "connection-v1"]
  }

  func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "open":
      guard let source = call.arguments as? String,
            let url = URL(string: source), url.scheme == "http", url.host == "127.0.0.1",
            url.user == nil, url.password == nil, url.query == nil, url.fragment == nil,
            url.path == "/manage/" || url.path == "/manage" else {
        result(failure()); return
      }
      if NSWorkspace.shared.open(url) { result(nil) } else { result(failure()) }
    case "read":
      var request = query
      request[kSecReturnData as String] = true
      request[kSecMatchLimit as String] = kSecMatchLimitOne
      var found: CFTypeRef?
      let status = SecItemCopyMatching(request as CFDictionary, &found)
      if status == errSecItemNotFound { result(nil); return }
      guard status == errSecSuccess, let data = found as? Data,
            let value = String(data: data, encoding: .utf8) else { result(failure()); return }
      result(value)
    case "write":
      guard let value = call.arguments as? String, value.utf8.count <= 4096,
            let data = value.data(using: .utf8) else { result(failure()); return }
      let update = [kSecValueData as String: data]
      var status = SecItemUpdate(query as CFDictionary, update as CFDictionary)
      if status == errSecItemNotFound {
        var item = query
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        status = SecItemAdd(item as CFDictionary, nil)
      }
      result(status == errSecSuccess ? nil : failure())
    case "delete":
      let status = SecItemDelete(query as CFDictionary)
      result(status == errSecSuccess || status == errSecItemNotFound ? nil : failure())
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  private func failure() -> FlutterError {
    FlutterError(code: "credential_store_unavailable", message: "Could not access the local server connection.", details: nil)
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
      if let arguments = call.arguments as? [String: Any], arguments["request_access"] as? Bool == false {
        result(failure("permission_denied")); return
      }
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
    let occurrence = (event.hasRecurrenceRules || event.isDetached)
      ? event.occurrenceDate.map { formatter.string(from: $0) } ?? "" : ""
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
