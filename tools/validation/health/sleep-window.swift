import Foundation

@main
struct SleepWindowChecks {
    static func main() throws {
        func date(_ hour: Double) -> Date { Date(timeIntervalSince1970: hour * 3_600) }
        func sample(_ start: Double, _ end: Double, _ asleep: Bool = true) -> SleepWindowSample {
            SleepWindowSample(start: date(start), end: date(end), asleep: asleep)
        }
        func hours(_ samples: [SleepWindowSample]) throws -> Double? {
            try SleepWindowAggregation.hours(samples: samples, start: date(0), end: date(36))
        }
        func expect(_ value: Double?, _ expected: Double?) {
            if let expected { precondition(value != nil && abs(value! - expected) < 0.000001) }
            else { precondition(value == nil) }
        }
        func unavailable(_ operation: () throws -> Double?) {
            do { _ = try operation(); preconditionFailure("incomplete input was accepted") }
            catch { precondition(error as? HealthKitWellbeingFailure == .unavailable) }
        }
        expect(try hours([]), nil)
        expect(try hours([sample(0, 8, false)]), nil)
        expect(try hours([sample(-5, 5)]), 5)
        expect(try hours([sample(30, 40)]), 6)
        expect(try hours([sample(-5, 40)]), 36)
        expect(try hours([sample(-10, 0), sample(36, 40), sample(5, 5)]), nil)
        expect(try hours([sample(8, 12), sample(0, 8), sample(4, 10), sample(0, 8)]), 12)
        expect(try hours([sample(0, 2), sample(4, 6), sample(6, 8)]), 6)
        expect(try hours([sample(0, 8), sample(0, 36, false)]), 8)
        let segmented = (0..<200).map { sample(Double($0) / 10, Double($0 + 1) / 10) }
        expect(try hours(segmented), 20)
        let exactBudget = Array(repeating: sample(0, 8), count: SleepWindowAggregation.maximumSampleCount)
        expect(try hours(exactBudget), 8)
        unavailable { try hours(exactBudget + [sample(10, 11)]) }
        unavailable { try hours([sample(2, 1)]) }
        unavailable { try hours([SleepWindowSample(start: Date(timeIntervalSince1970: .nan), end: date(8), asleep: true)]) }
        unavailable { try SleepWindowAggregation.hours(samples: [], start: date(1), end: date(1)) }
        unavailable { try SleepWindowAggregation.hours(samples: [], start: date(2), end: date(1)) }
        precondition(SleepWindowAggregation.queryLimit == SleepWindowAggregation.maximumSampleCount + 1)
        print("Sleep window aggregation: 17 checks passed")
    }
}
