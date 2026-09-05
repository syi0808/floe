import Foundation
import EventKit

let arguments = Array(CommandLine.arguments.dropFirst())
guard arguments.count == 3 || arguments.count == 4 else { fatalError("inspect CALENDAR_ID EXECUTION_ID ACTION_JSON [--cleanup]") }
guard EKEventStore.authorizationStatus(for: .event) == .fullAccess else { fatalError("permission_denied: no prompt requested") }
let store = EKEventStore()
guard let calendar = store.calendar(withIdentifier: arguments[0]), calendar.title == "Floe Validation",
      calendar.source.title == "iCloud", calendar.allowsContentModifications else { fatalError("wrong calendar") }
let action = try JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: arguments[2]))) as! [String: Any]
let proposal = try Proposal(action)
guard proposal.execution == arguments[1], proposal.calendarID == calendar.calendarIdentifier,
      proposal.title == "Floe S3 — disposable" else { fatalError("wrong disposable proposal") }
let events = store.events(matching: store.predicateForEvents(withStart: proposal.start.addingTimeInterval(-86400), end: proposal.end.addingTimeInterval(86400), calendars: [calendar]))
let found = events.filter { $0.url == proposal.marker }
guard found.count <= 1 else { fatalError("ambiguous match") }
if arguments.count == 4 && arguments[3] == "--verify-absent" {
  guard found.isEmpty else { fatalError("disposable event still exists") }
  print("{\"exact_execution_matches\":0}")
} else if arguments.count == 4 {
  guard arguments[3] == "--cleanup", found.count == 1, proposal.matches(found[0]) else { fatalError("cleanup requires one exact match") }
  try store.remove(found[0], span: .thisEvent, commit: true)
  print("{\"removed_exact_disposable_event\":true}")
} else {
  guard found.count == 1, proposal.matches(found[0]) else { fatalError("exact event not found") }
  let event = found[0]
  let result: [String: Any] = ["matches": 1, "receipt": proposal.receipt(event),
    "record": ["external_id": "\(event.calendarItemIdentifier)|", "external_revision": "s3-live-check",
      "title": proposal.title, "schedule": ["kind": "timed", "starts_at": (action["schedule"] as! [String: Any])["starts_at"]!,
        "ends_at": (action["schedule"] as! [String: Any])["ends_at"]!, "timezone": proposal.timezone]]]
  print(String(decoding: try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]), as: UTF8.self))
}
