import Foundation
import Testing
@testable import FloeFeasibilityProvider

private struct LocationStub: CurrentLocationProviding {
  let reading: LocationReading
  func currentLocation(deadline: Date) async throws -> LocationReading { reading }
}

private struct DirectionsStub: DirectionsProviding {
  let duration: TimeInterval
  func route(
    from origin: Coordinate,
    to destination: Coordinate,
    mode: TravelMode,
    deadline: Date
  ) async throws -> RouteReading {
    RouteReading(expectedTravelTime: duration)
  }
}

private struct WeatherStub: EventWeatherProviding {
  let reading: WeatherReading
  func weather(
    at destination: Coordinate,
    from start: Date,
    through end: Date,
    deadline: Date
  ) async throws -> WeatherReading { reading }

  func attribution(deadline: Date) async throws -> WeatherAttributionLinks {
    WeatherAttributionLinks(
      legalPageURL: URL(string: "https://weather.example/legal")!,
      combinedMarkLightURL: URL(string: "https://weather.example/light.svg")!,
      combinedMarkDarkURL: URL(string: "https://weather.example/dark.svg")!
    )
  }
}

private struct SlowLocationStub: CurrentLocationProviding {
  func currentLocation(deadline: Date) async throws -> LocationReading {
    try await Task.sleep(for: .seconds(5))
    return LocationReading(
      coordinate: Coordinate(latitude: 37.4, longitude: 127.1),
      observedAt: Date(),
      horizontalAccuracyMeters: 25
    )
  }
}

private let now = Date(timeIntervalSince1970: 2_000_000_000)

private func request(deadline: Date = now.addingTimeInterval(20)) -> FeasibilityRequest {
  FeasibilityRequest(
    eventHandle: "event:next",
    evidenceHandles: ["calendar:event:next", "weather:event:next"],
    destination: Coordinate(latitude: 37.5665, longitude: 126.9780),
    eventStart: now.addingTimeInterval(3_600),
    eventEnd: now.addingTimeInterval(7_200),
    travelMode: .transit,
    sourceHandle: "apple:feasibility:local",
    deadline: deadline
  )
}

@Test func emitsBoundedProviderNeutralViewWithoutCoordinates() async throws {
  let provider = AppleFeasibilityProvider(
    location: LocationStub(
      reading: LocationReading(
        coordinate: Coordinate(latitude: 37.4, longitude: 127.1),
        observedAt: now.addingTimeInterval(-10),
        horizontalAccuracyMeters: 25
      )
    ),
    directions: DirectionsStub(duration: 1_200.2),
    weather: WeatherStub(
      reading: WeatherReading(
        precipitationChance: 0.8,
        windKilometersPerHour: 10,
        severeCondition: false
      )
    ),
    now: { now }
  )

  let result = try await provider.feasibility(for: request())
  #expect(result.view.viewID == "schedule.feasibility")
  #expect(result.view.expiresAtUnixMs - result.view.observedAtUnixMs == 300_000)
  #expect(result.view.items.first?.travelDurationSeconds == 1_201)
  #expect(result.view.items.first?.leaveByUnixMs == now.addingTimeInterval(2_399).unixMilliseconds)
  #expect(result.view.items.first?.weatherImpact == .significant)

  let json = String(decoding: try JSONEncoder().encode(result.view), as: UTF8.self)
  #expect(!json.contains("latitude"))
  #expect(!json.contains("longitude"))
  #expect(!json.contains("location"))
  #expect(json.contains("\"view_id\":\"schedule.feasibility\""))

  let fixtureURL = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appending(path: "../Fixtures/schedule_feasibility.json")
    .standardizedFileURL
  let fixtureObject = try JSONSerialization.jsonObject(with: Data(contentsOf: fixtureURL))
  let encodedObject = try JSONSerialization.jsonObject(with: JSONEncoder().encode(result.view))
  #expect((fixtureObject as? NSDictionary) == (encodedObject as? NSDictionary))
}

@Test func rejectsStaleLocation() async {
  let provider = AppleFeasibilityProvider(
    location: LocationStub(
      reading: LocationReading(
        coordinate: Coordinate(latitude: 37.4, longitude: 127.1),
        observedAt: now.addingTimeInterval(-61),
        horizontalAccuracyMeters: 25
      )
    ),
    directions: DirectionsStub(duration: 900),
    weather: WeatherStub(
      reading: WeatherReading(
        precipitationChance: 0,
        windKilometersPerHour: 0,
        severeCondition: false
      )
    ),
    now: { now }
  )

  await #expect(throws: FeasibilityFailure(code: .staleLocation, provider: "core_location")) {
    try await provider.feasibility(for: request())
  }
}

@Test func rejectsUnboundedAndExpiredRequestsBeforeProvidersRun() async {
  let provider = AppleFeasibilityProvider(
    location: LocationStub(
      reading: LocationReading(
        coordinate: Coordinate(latitude: 37.4, longitude: 127.1),
        observedAt: now,
        horizontalAccuracyMeters: 25
      )
    ),
    directions: DirectionsStub(duration: 900),
    weather: WeatherStub(
      reading: WeatherReading(
        precipitationChance: 0,
        windKilometersPerHour: 0,
        severeCondition: false
      )
    ),
    now: { now }
  )

  await #expect(throws: FeasibilityFailure(code: .invalidInput, provider: "apple_feasibility")) {
    try await provider.feasibility(for: request(deadline: now.addingTimeInterval(31)))
  }
  await #expect(throws: FeasibilityFailure(code: .invalidInput, provider: "apple_feasibility")) {
    try await provider.feasibility(for: request(deadline: now))
  }
}

@Test func enforcesRustHandleByteBoundary() async throws {
  let provider = AppleFeasibilityProvider(
    location: LocationStub(
      reading: LocationReading(
        coordinate: Coordinate(latitude: 37.4, longitude: 127.1),
        observedAt: now,
        horizontalAccuracyMeters: 25
      )
    ),
    directions: DirectionsStub(duration: 900),
    weather: WeatherStub(
      reading: WeatherReading(
        precipitationChance: 0,
        windKilometersPerHour: 0,
        severeCondition: false
      )
    ),
    now: { now }
  )
  let accepted = FeasibilityRequest(
    eventHandle: String(repeating: "a", count: 128),
    evidenceHandles: ["evidence:one"],
    destination: Coordinate(latitude: 37.5665, longitude: 126.9780),
    eventStart: now.addingTimeInterval(3_600),
    eventEnd: now.addingTimeInterval(7_200),
    travelMode: .transit,
    sourceHandle: "apple:feasibility:local",
    deadline: now.addingTimeInterval(20)
  )
  _ = try await provider.feasibility(for: accepted)

  let rejected = FeasibilityRequest(
    eventHandle: String(repeating: "a", count: 129),
    evidenceHandles: ["evidence:one"],
    destination: Coordinate(latitude: 37.5665, longitude: 126.9780),
    eventStart: now.addingTimeInterval(3_600),
    eventEnd: now.addingTimeInterval(7_200),
    travelMode: .transit,
    sourceHandle: "apple:feasibility:local",
    deadline: now.addingTimeInterval(20)
  )
  await #expect(throws: FeasibilityFailure(code: .invalidInput, provider: "apple_feasibility")) {
    try await provider.feasibility(for: rejected)
  }
}

@Test func cancelsCooperativeProviderAtDeadline() async {
  let wallClockNow = Date()
  let provider = AppleFeasibilityProvider(
    location: SlowLocationStub(),
    directions: DirectionsStub(duration: 900),
    weather: WeatherStub(
      reading: WeatherReading(
        precipitationChance: 0,
        windKilometersPerHour: 0,
        severeCondition: false
      )
    ),
    now: Date.init
  )
  let boundedRequest = FeasibilityRequest(
    eventHandle: "event:next",
    evidenceHandles: ["evidence:one"],
    destination: Coordinate(latitude: 37.5665, longitude: 126.9780),
    eventStart: wallClockNow.addingTimeInterval(3_600),
    eventEnd: wallClockNow.addingTimeInterval(7_200),
    travelMode: .transit,
    sourceHandle: "apple:feasibility:local",
    deadline: wallClockNow.addingTimeInterval(0.1)
  )
  let started = ContinuousClock.now
  await #expect(throws: FeasibilityFailure(code: .timeout, provider: "core_location")) {
    try await provider.feasibility(for: boundedRequest)
  }
  #expect(started.duration(to: .now) < .seconds(1))
}

private extension Date {
  var unixMilliseconds: Int64 { Int64((timeIntervalSince1970 * 1_000).rounded()) }
}
