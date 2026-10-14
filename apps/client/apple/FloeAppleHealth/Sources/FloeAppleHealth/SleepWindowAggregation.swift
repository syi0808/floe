import Foundation

struct SleepWindowSample {
    let start: Date
    let end: Date
    let asleep: Bool
}

/// A bounded complete query is required before these local aggregates may enter
/// the existing privacy transform. An overflow is not a partial success.
enum SleepWindowAggregation {
    static let maximumSampleCount = 4_096
    static let queryLimit = maximumSampleCount + 1

    static func hours(samples: [SleepWindowSample], start: Date, end: Date) throws -> Double? {
        guard samples.count <= maximumSampleCount,
              start.timeIntervalSince1970.isFinite,
              end.timeIntervalSince1970.isFinite, start < end else {
            throw HealthKitWellbeingFailure.unavailable
        }
        var intervals: [DateInterval] = []
        for sample in samples {
            guard sample.start.timeIntervalSince1970.isFinite,
                  sample.end.timeIntervalSince1970.isFinite,
                  sample.start <= sample.end else {
                throw HealthKitWellbeingFailure.unavailable
            }
            guard sample.asleep else { continue }
            let clippedStart = max(start, sample.start)
            let clippedEnd = min(end, sample.end)
            guard clippedStart < clippedEnd else { continue }
            intervals.append(DateInterval(start: clippedStart, end: clippedEnd))
        }
        guard !intervals.isEmpty else { return nil }
        intervals.sort { $0.start < $1.start }
        var merged: [DateInterval] = []
        for interval in intervals {
            guard let previous = merged.last, interval.start <= previous.end else {
                merged.append(interval)
                continue
            }
            merged[merged.count - 1] = DateInterval(start: previous.start, end: max(previous.end, interval.end))
        }
        return merged.reduce(0) { $0 + $1.duration } / 3_600
    }
}
