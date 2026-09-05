import Foundation

func check(_ condition: @autoclosure () -> Bool, _ message: String) {
  if !condition() { fatalError(message) }
}

let raw: [String: Any] = [
  "person_id": "00000000-0000-4000-8000-000000000001", "provider": "event_kit",
  "calendar_id": "calendar", "title": "Focus", "execution_id": UUID().uuidString,
  "expires_at": "2026-03-08T12:00:00Z",
  "schedule": ["starts_at": "2026-03-08T07:00:00Z", "ends_at": "2026-03-08T08:00:00Z", "timezone": "America/New_York"]
]
let proposal = try Proposal(raw)
check(proposal.end.timeIntervalSince(proposal.start) == 3600, "UTC interval changed across DST")
check(proposal.marker.absoluteString.contains(proposal.execution), "missing execution marker")
let timed: [String: Any] = ["deleted_at": NSNull(), "schedule": ["Timed": ["starts_at": "2026-03-08T07:30:00Z", "ends_at": "2026-03-08T08:30:00Z"]]]
let overlapping = try localConflict([timed], proposal)
check(overlapping, "local overlap missed")
let adjacent: [String: Any] = ["deleted_at": NSNull(), "schedule": ["Timed": ["starts_at": "2026-03-08T08:00:00Z", "ends_at": "2026-03-08T09:00:00Z"]]]
let adjacentConflict = try localConflict([adjacent], proposal)
check(!adjacentConflict, "adjacent interval blocked")
var deleted = timed
deleted["deleted_at"] = "2026-03-07T00:00:00Z"
let deletedConflict = try localConflict([deleted], proposal)
check(!deletedConflict, "deleted event blocked")
let allDay: [String: Any] = ["deleted_at": NSNull(), "schedule": ["AllDay": ["start_date": "2026-03-08", "end_date_exclusive": "2026-03-09"]]]
let allDayConflict = try localConflict([allDay], proposal)
check(allDayConflict, "floating all-day event missed across DST")
var invalid = raw
invalid["person_id"] = UUID().uuidString
do { _ = try Proposal(invalid); fatalError("foreign Person accepted") } catch {}
let response = "{\"operation\":\"capabilities\"}".withCString { floeEventKitAction($0) }!
let capabilities = try JSONSerialization.jsonObject(with: Data(String(cString: response).utf8)) as! [String: Any]
floeEventKitFree(response)
check((capabilities["data"] as? [String: Any])?["writes_enabled"] as? Bool == false, "writes enabled in default build")
print("8 native validation assertions passed; no OS permission or event access invoked")
