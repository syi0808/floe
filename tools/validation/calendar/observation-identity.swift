import Foundation

@main
struct ObservationIdentityChecks {
  static func main() throws {
    let start = Date(timeIntervalSince1970: 1_728_000_000.125)
    let end = start.addingTimeInterval(3600)
    let modified = start.addingTimeInterval(-10)
    let schedule = CalendarObservationSchedule.timed(startsAt: start, endsAt: end, timezone: "Asia/Seoul")
    let expectedSchedule: [String: Any] = ["Timed": [
      "starts_at": calendarObservationTimestamp(start), "ends_at": calendarObservationTimestamp(end),
      "timezone": "Asia/Seoul"
    ]]
    let canonical = try calendarObservationPayload(title: "  실제 일정 🌙  ", schedule: schedule, modifiedAt: modified)
    let expected = try JSONSerialization.data(withJSONObject: [
      "title": "  실제 일정 🌙  ", "schedule": expectedSchedule,
      "modified": calendarObservationTimestamp(modified)
    ], options: [.sortedKeys])
    precondition(canonical == expected, "accepted native effect fingerprint bytes changed")
    precondition(schedule.transport["kind"] as? String == "timed")
    precondition(schedule.transport["Timed"] == nil, "fingerprint shape leaked into transport")
    let decoded = try JSONSerialization.jsonObject(with: canonical) as! [String: Any]
    precondition(decoded["title"] as? String == "  실제 일정 🌙  ")
    precondition(calendarObservationTimestamp(start).hasSuffix(".125Z"))
    let empty = try calendarObservationPayload(title: "", schedule: schedule, modifiedAt: nil)
    let whitespace = try calendarObservationPayload(title: " \n ", schedule: schedule, modifiedAt: nil)
    precondition(empty != whitespace, "native titles were normalized")
    let emptyObject = try JSONSerialization.jsonObject(with: empty) as! [String: Any]
    precondition(emptyObject["modified"] as? String == "")
    let changedTime = try calendarObservationPayload(title: "  실제 일정 🌙  ", schedule: schedule, modifiedAt: start)
    precondition(changedTime != canonical)
    let changedZone = try calendarObservationPayload(title: "  실제 일정 🌙  ",
      schedule: .timed(startsAt: start, endsAt: end, timezone: "UTC"), modifiedAt: modified)
    precondition(changedZone != canonical, "native zone identity must remain in the fingerprint")

    // Reproduce the previous Runner's mismatched flat schedule/title rewriting.
    let oldReader = try JSONSerialization.data(withJSONObject: [
      "title": "실제 일정 🌙", "schedule": schedule.transport,
      "modified": calendarObservationTimestamp(modified)
    ], options: [.sortedKeys])
    precondition(oldReader != canonical)

    let parse = ISO8601DateFormatter()
    let zone = TimeZone(identifier: "America/New_York")!
    let allDay = CalendarObservationSchedule.allDayDates(
      start: parse.date(from: "2026-03-08T05:00:00Z")!,
      end: parse.date(from: "2026-03-09T04:00:00Z")!, timeZone: zone)
    precondition(allDay.transport["start_date"] as? String == "2026-03-08")
    precondition(allDay.transport["end_date_exclusive"] as? String == "2026-03-09",
                 "a DST day uses civil dates, not 24-hour arithmetic")
    let dayBytes = try calendarObservationPayload(title: "", schedule: allDay, modifiedAt: nil)
    let dayObject = try JSONSerialization.jsonObject(with: dayBytes) as! [String: Any]
    precondition((dayObject["schedule"] as! [String: Any])["AllDay"] != nil)
    let partialEnd = CalendarObservationSchedule.allDayDates(
      start: parse.date(from: "2026-03-08T05:00:00Z")!,
      end: parse.date(from: "2026-03-09T05:00:00Z")!, timeZone: zone)
    precondition(partialEnd.transport["end_date_exclusive"] as? String == "2026-03-10")
    let zeroDay = CalendarObservationSchedule.allDayDates(start: start, end: start, timeZone: zone)
    precondition(zeroDay.transport["start_date"] as? String != zeroDay.transport["end_date_exclusive"] as? String)
    print("Calendar observation identity: 15 assertions passed")
  }
}
