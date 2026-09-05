import Foundation
import EventKit
import Darwin

struct PoCError: Error, CustomStringConvertible {
  let description: String
  init(_ message: String) { description = message }
}

struct Ledger: Codable {
  let executionID: String
  let calendarID: String
  var title: String
  let startsAt: Date
  let endsAt: Date
  let timezone: String
  let expiresAt: Date
  var state: String
  var externalID: String?

  var marker: URL { URL(string: "floe-poc://execution/\(executionID)")! }
}

let store = EKEventStore()
let arguments = Array(CommandLine.arguments.dropFirst())
let calendarName = "Floe Validation"
let eventTitle = "Floe PoC — disposable"
let encoder = JSONEncoder()
encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
encoder.dateEncodingStrategy = .iso8601
let decoder = JSONDecoder()
decoder.dateDecodingStrategy = .iso8601

func emit(_ value: [String: Any]) throws {
  let data = try JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys])
  print(String(decoding: data, as: UTF8.self))
}

func requireRead() throws {
  guard EKEventStore.authorizationStatus(for: .event) == .fullAccess else {
    throw PoCError("permission_denied: no permission prompt was requested; explicitly authorize the responsible app before retrying")
  }
}

func testCalendar(_ identifier: String) throws -> EKCalendar {
  try requireRead()
  store.reset()
  guard let calendar = store.calendar(withIdentifier: identifier),
        calendar.title == calendarName, calendar.allowsContentModifications,
        !calendar.isSubscribed else {
    throw PoCError("calendar_unavailable: only the exact writable Floe Validation calendar is allowed")
  }
  return calendar
}

func persist(_ ledger: Ledger, at path: String) throws {
  try encoder.encode(ledger).write(to: URL(fileURLWithPath: path), options: .atomic)
  guard chmod(path, S_IRUSR | S_IWUSR) == 0 else { throw PoCError("ledger_permissions_failed") }
  let file = try FileHandle(forWritingTo: URL(fileURLWithPath: path))
  try file.synchronize()
  try file.close()
  let directory = open(URL(fileURLWithPath: path).deletingLastPathComponent().path, O_RDONLY)
  guard directory >= 0 else { throw PoCError("ledger_directory_unavailable") }
  defer { close(directory) }
  guard fsync(directory) == 0 else { throw PoCError("ledger_directory_sync_failed") }
}

func matches(_ event: EKEvent, _ ledger: Ledger) -> Bool {
  event.url == ledger.marker && event.calendar.calendarIdentifier == ledger.calendarID
    && event.title == ledger.title && event.startDate == ledger.startsAt && event.endDate == ledger.endsAt
    && event.timeZone?.identifier == ledger.timezone && !event.isAllDay
    && !event.hasRecurrenceRules && !event.hasAttendees && !event.hasAlarms
}

func lookup(_ ledger: Ledger) throws -> [EKEvent] {
  let calendar = try testCalendar(ledger.calendarID)
  let predicate = store.predicateForEvents(withStart: ledger.startsAt.addingTimeInterval(-86400),
    end: ledger.endsAt.addingTimeInterval(86400), calendars: [calendar])
  let found = store.events(matching: predicate).filter { $0.url == ledger.marker }
  try requireRead()
  return found
}

func run() throws {
  guard let command = arguments.first else {
    throw PoCError("usage: status | calendars | new-calendar SOURCE_ID --approve-test-calendar | prepare CALENDAR_ID LEDGER | create LEDGER --approve=EXECUTION_ID [--lose-response] | recover LEDGER | cleanup LEDGER --approve=EXECUTION_ID")
  }
  if command == "status" {
    try emit(["authorization": EKEventStore.authorizationStatus(for: .event).rawValue,
              "full_access": EKEventStore.authorizationStatus(for: .event) == .fullAccess,
              "writes": false])
    return
  }
  try requireRead()
  if command == "calendars" {
    try emit(["calendars": store.calendars(for: .event).filter { $0.allowsContentModifications }.map {
      ["id": $0.calendarIdentifier, "name": $0.title, "source_id": $0.source.sourceIdentifier,
       "source": $0.source.title, "test_calendar": $0.title == calendarName] as [String: Any]
    }])
    return
  }
  if command == "new-calendar" {
    guard arguments.count == 3, arguments[2] == "--approve-test-calendar",
          let source = store.sources.first(where: { $0.sourceIdentifier == arguments[1] }) else {
      throw PoCError("explicit source and --approve-test-calendar required")
    }
    let existing = store.calendars(for: .event).filter { $0.title == calendarName && $0.source == source }
    guard existing.count <= 1 else { throw PoCError("ambiguous test calendars; inspect manually") }
    if let calendar = existing.first {
      try emit(["calendar_id": calendar.calendarIdentifier, "created": false]); return
    }
    let calendar = EKCalendar(for: .event, eventStore: store)
    calendar.title = calendarName
    calendar.source = source
    try store.saveCalendar(calendar, commit: true)
    try emit(["calendar_id": calendar.calendarIdentifier, "created": true])
    return
  }
  guard arguments.count >= 2 else { throw PoCError("missing ledger path") }
  let path = command == "prepare" && arguments.count == 3 ? arguments[2] : arguments[1]
  let lock = open(path + ".lock", O_CREAT | O_RDWR, S_IRUSR | S_IWUSR)
  guard lock >= 0 else { throw PoCError("cannot open ledger lock; create its private directory first") }
  defer { close(lock) }
  guard flock(lock, LOCK_EX | LOCK_NB) == 0 else { throw PoCError("execution_busy") }
  defer { flock(lock, LOCK_UN) }
  if command == "prepare" {
    guard arguments.count == 3, !FileManager.default.fileExists(atPath: path) else {
      throw PoCError("prepare requires a new ledger; never overwrite an existing execution")
    }
    _ = try testCalendar(arguments[1])
    let formatter = ISO8601DateFormatter()
    let ledger = Ledger(executionID: UUID().uuidString.lowercased(), calendarID: arguments[1], title: eventTitle,
      startsAt: formatter.date(from: "2026-09-06T10:00:00+09:00")!,
      endsAt: formatter.date(from: "2026-09-06T10:15:00+09:00")!,
      timezone: "Asia/Seoul", expiresAt: Date().addingTimeInterval(900), state: "pending", externalID: nil)
    try persist(ledger, at: path)
    print(String(decoding: try encoder.encode(ledger), as: UTF8.self))
    return
  }
  var ledger = try decoder.decode(Ledger.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
  guard [eventTitle, eventTitle + " — edited"].contains(ledger.title), ledger.timezone == "Asia/Seoul",
        UUID(uuidString: ledger.executionID) != nil,
        ledger.endsAt.timeIntervalSince(ledger.startsAt) == 900 else { throw PoCError("invalid test ledger") }
  if command == "create" {
    guard arguments.contains("--approve=\(ledger.executionID)"), ledger.state == "pending",
          Date() < ledger.expiresAt, Date() < ledger.startsAt else {
      throw PoCError("create_blocked: explicit unexpired approval required; executing/unknown/succeeded must never create again")
    }
    let calendar = try testCalendar(ledger.calendarID)
    let predicate = store.predicateForEvents(withStart: ledger.startsAt, end: ledger.endsAt, calendars: [calendar])
    guard store.events(matching: predicate).isEmpty else { throw PoCError("schedule_conflict: no write") }
    ledger.state = "executing"
    try persist(ledger, at: path)
    try requireRead()
    let event = EKEvent(eventStore: store)
    event.calendar = calendar
    event.title = ledger.title
    event.startDate = ledger.startsAt
    event.endDate = ledger.endsAt
    event.timeZone = TimeZone(identifier: ledger.timezone)
    event.url = ledger.marker
    event.alarms = nil
    try store.save(event, span: .thisEvent, commit: true)
    if arguments.contains("--lose-response") { _exit(75) }
    guard matches(event, ledger) else { throw PoCError("unknown: saved result differs; lookup only") }
    ledger.state = "succeeded"
    ledger.externalID = event.calendarItemIdentifier
    try persist(ledger, at: path)
  } else if command == "recover" {
    guard ["executing", "unknown", "succeeded"].contains(ledger.state) else { throw PoCError("not_recoverable") }
    let found = try lookup(ledger)
    if found.count == 1 && matches(found[0], ledger) {
      ledger.state = "succeeded"
      ledger.externalID = found[0].calendarItemIdentifier
    } else {
      ledger.state = "unknown"
    }
    try persist(ledger, at: path)
    try emit(["state": ledger.state, "marker_matches": found.count, "create_retried": false,
              "external_id": ledger.externalID ?? "", "exact_match": found.count == 1 && matches(found[0], ledger)])
    return
  } else if command == "edit-test" {
    guard arguments.contains("--approve=\(ledger.executionID)"), ledger.state == "succeeded", ledger.title == eventTitle else {
      throw PoCError("edit-test requires approval for the unchanged disposable event")
    }
    let found = try lookup(ledger)
    guard found.count == 1, matches(found[0], ledger) else { throw PoCError("edit refused: no unique exact disposable match") }
    found[0].title = eventTitle + " — edited"
    try store.save(found[0], span: .thisEvent, commit: true)
    ledger.title = eventTitle + " — edited"
    try persist(ledger, at: path)
  } else if command == "cleanup" {
    guard arguments.contains("--approve=\(ledger.executionID)"), ["executing", "unknown", "succeeded"].contains(ledger.state) else {
      throw PoCError("cleanup requires explicit approval for this execution")
    }
    let found = try lookup(ledger)
    guard found.count == 1, matches(found[0], ledger) else { throw PoCError("cleanup refused: no unique exact disposable match") }
    try store.remove(found[0], span: .thisEvent, commit: true)
    ledger.state = "cleaned"
    try persist(ledger, at: path)
  } else {
    throw PoCError("unknown command")
  }
  print(String(decoding: try encoder.encode(ledger), as: UTF8.self))
}

do { try run() }
catch {
  FileHandle.standardError.write(Data("\(error)\n".utf8))
  exit(1)
}
