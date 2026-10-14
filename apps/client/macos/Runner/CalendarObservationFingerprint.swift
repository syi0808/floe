import Foundation

/// The native observation has one semantic representation. Transport and
/// fingerprint encodings intentionally differ; neither may normalize the title.
enum CalendarObservationSchedule {
  case timed(startsAt: Date, endsAt: Date, timezone: String)
  case allDay(startDate: String, endDateExclusive: String)

  private var fields: [String: Any] {
    switch self {
    case let .timed(start, end, timezone):
      return ["starts_at": calendarObservationTimestamp(start),
              "ends_at": calendarObservationTimestamp(end), "timezone": timezone]
    case let .allDay(start, end):
      return ["start_date": start, "end_date_exclusive": end]
    }
  }

  var transport: [String: Any] {
    var result = fields
    switch self {
    case .timed: result["kind"] = "timed"
    case .allDay: result["kind"] = "all_day"
    }
    return result
  }

  var fingerprint: [String: Any] {
    switch self {
    case .timed: return ["Timed": fields]
    case .allDay: return ["AllDay": fields]
    }
  }

  static func allDayDates(start: Date, end: Date, timeZone: TimeZone) -> Self {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = timeZone
    let startDay = calendar.startOfDay(for: start)
    let endDay = calendar.startOfDay(for: end)
    let candidate = end > endDay ? calendar.date(byAdding: .day, value: 1, to: endDay)! : endDay
    let exclusiveEnd = candidate > startDay ? candidate : calendar.date(byAdding: .day, value: 1, to: startDay)!
    let formatter = DateFormatter()
    formatter.calendar = calendar
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = timeZone
    formatter.dateFormat = "yyyy-MM-dd"
    return .allDay(startDate: formatter.string(from: startDay),
                   endDateExclusive: formatter.string(from: exclusiveEnd))
  }
}

func calendarObservationTimestamp(_ date: Date) -> String {
  let formatter = ISO8601DateFormatter()
  formatter.timeZone = TimeZone(secondsFromGMT: 0)
  formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  return formatter.string(from: date)
}

/// SHA256 is applied by the platform adapter to these exact sorted-key bytes.
/// Empty, whitespace-only and Unicode titles remain native observation data.
func calendarObservationPayload(title: String, schedule: CalendarObservationSchedule,
                                modifiedAt: Date?) throws -> Data {
  try JSONSerialization.data(withJSONObject: [
    "title": title, "schedule": schedule.fingerprint,
    "modified": modifiedAt.map(calendarObservationTimestamp) ?? ""
  ], options: [.sortedKeys])
}
