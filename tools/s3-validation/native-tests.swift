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
var fixedOffset = raw
var fixedSchedule = raw["schedule"] as! [String: Any]
fixedSchedule["timezone"] = "UTC+09:00"
fixedOffset["schedule"] = fixedSchedule
let fixedProposal = try Proposal(fixedOffset)
check(TimeZone(identifier: fixedProposal.timezone) != nil, "local fixed-offset metadata rejected")
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
check((capabilities["data"] as? [String: Any])?["writes_enabled"] as? Bool == true, "writes unavailable in default build")
var original = timed
original["id"] = "original-event"
original["source"] = ["Calendar": ["external_id": "external-event|"]]
var updateRaw = raw
updateRaw["mutation"] = ["original": original, "delete": false]
let update = try Proposal(updateRaw)
check(update.externalID == "external-event|", "update lost target identity")
let selfConflict = try localConflict([original], update)
check(!selfConflict, "moving event conflicts with itself")
var other = timed
other["id"] = "another-event"
let otherConflict = try localConflict([original, other], update)
check(otherConflict, "update ignores other overlaps")
updateRaw["mutation"] = ["original": original, "delete": true]
let deletion = try Proposal(updateRaw)
check(deletion.deleting, "delete operation lost")
let formatter = ISO8601DateFormatter()
let accessRequest: [String: Any] = ["schema_version": 1, "person_id": raw["person_id"]!,
  "provider": "event_kit", "calendar_ids": ["second", "first"],
  "deadline": formatter.string(from: Date().addingTimeInterval(10))]
var permissions = 0
let stamp = try calendarViewAccess(accessRequest, permission: { permissions += 1 },
  contains: { ["first", "second"].contains($0) }, generation: { "stable" })
check(permissions == 2, "access must be checked on both sides of inventory lookup")
check(stamp["calendar_ids"] as? [String] == ["first", "second"], "scope was changed")
check(stamp["generation"] as? String == "stable", "generation was lost")
for change: [String: Any] in [
  ["schema_version": 2], ["person_id": UUID().uuidString], ["calendar_ids": [String]()],
  ["calendar_ids": ["first", "first"]], ["calendar_ids": ["1", "2", "3", "4", "5"]],
  ["deadline": formatter.string(from: Date().addingTimeInterval(-1))]
] {
  var invalid = accessRequest
  invalid.merge(change) { _, updated in updated }
  var touched = false
  do {
    _ = try calendarViewAccess(invalid, permission: { touched = true }, contains: { _ in true }, generation: { "stable" })
    fatalError("invalid access request was allowed")
  } catch { check(!touched, "invalid input reached permission boundary") }
}
var generations = 0
do {
  _ = try calendarViewAccess(accessRequest, permission: {}, contains: { _ in true }, generation: {
    generations += 1; return String(generations)
  })
  fatalError("changed access generation was allowed")
} catch { check(generations == 2, "generation must be checked after lookup") }
var queried = false
do {
  _ = try calendarViewAccess(accessRequest, permission: { throw NSError(domain: "fixture", code: 1) },
    contains: { _ in queried = true; return true }, generation: { "stable" })
  fatalError("denied permission was ignored")
} catch { check(!queried, "denied permission reached Calendar lookup") }
var laterPermissions = 0
do {
  _ = try calendarViewAccess(accessRequest, permission: {
    laterPermissions += 1
    if laterPermissions == 2 { throw NSError(domain: "fixture", code: 2) }
  }, contains: { _ in true }, generation: { "stable" })
  fatalError("permission withdrawal during inventory lookup was ignored")
} catch { check(laterPermissions == 2, "permission was not rechecked") }
print("25 native validation assertions passed; no OS permission or event access invoked")
