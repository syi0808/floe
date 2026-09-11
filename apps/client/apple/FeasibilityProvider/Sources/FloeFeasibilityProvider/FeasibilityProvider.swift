import CoreLocation
import Foundation
import MapKit
import WeatherKit

public enum TravelMode: String, Codable, Sendable {
  case automobile
  case transit
  case walking
}

public enum WeatherImpact: String, Codable, Sendable {
  case none
  case minor
  case significant
  case unknown
}

public enum FeasibilityFailureCode: String, Codable, Sendable {
  case invalidInput = "invalid_input"
  case permissionDenied = "permission_denied"
  case locationUnavailable = "location_unavailable"
  case staleLocation = "stale_location"
  case unsupportedRegion = "unsupported_region"
  case providerUnavailable = "provider_unavailable"
  case providerFailure = "provider_failure"
  case timeout
}

public struct FeasibilityFailure: Error, Codable, Equatable, Sendable {
  public let code: FeasibilityFailureCode
  public let provider: String

  public init(code: FeasibilityFailureCode, provider: String) {
    self.code = code
    self.provider = provider
  }
}

public struct Coordinate: Codable, Equatable, Sendable {
  public let latitude: Double
  public let longitude: Double

  public init(latitude: Double, longitude: Double) {
    self.latitude = latitude
    self.longitude = longitude
  }

  var isValid: Bool {
    CLLocationCoordinate2DIsValid(
      CLLocationCoordinate2D(latitude: latitude, longitude: longitude)
    )
  }
}

public struct FeasibilityRequest: Sendable {
  public let eventHandle: String
  public let evidenceHandles: [String]
  public let destination: Coordinate
  public let eventStart: Date
  public let eventEnd: Date
  public let travelMode: TravelMode
  public let sourceHandle: String
  public let deadline: Date

  public init(
    eventHandle: String,
    evidenceHandles: [String],
    destination: Coordinate,
    eventStart: Date,
    eventEnd: Date,
    travelMode: TravelMode,
    sourceHandle: String,
    deadline: Date
  ) {
    self.eventHandle = eventHandle
    self.evidenceHandles = evidenceHandles
    self.destination = destination
    self.eventStart = eventStart
    self.eventEnd = eventEnd
    self.travelMode = travelMode
    self.sourceHandle = sourceHandle
    self.deadline = deadline
  }
}

public struct FeasibilityItem: Codable, Equatable, Sendable {
  public let eventHandle: String
  public let evidenceHandles: [String]
  public let travelDurationSeconds: UInt32
  public let leaveByUnixMs: Int64
  public let weatherImpact: WeatherImpact
  public let confidenceMillis: UInt16

  enum CodingKeys: String, CodingKey {
    case eventHandle = "event_handle"
    case evidenceHandles = "evidence_handles"
    case travelDurationSeconds = "travel_duration_seconds"
    case leaveByUnixMs = "leave_by_unix_ms"
    case weatherImpact = "weather_impact"
    case confidenceMillis = "confidence_millis"
  }
}

public struct FeasibilityView: Codable, Equatable, Sendable {
  public let schemaVersion: UInt32
  public let viewID: String
  public let sourceHandle: String
  public let observedAtUnixMs: Int64
  public let expiresAtUnixMs: Int64
  public let items: [FeasibilityItem]

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case viewID = "view_id"
    case sourceHandle = "source_handle"
    case observedAtUnixMs = "observed_at_unix_ms"
    case expiresAtUnixMs = "expires_at_unix_ms"
    case items
  }
}

public struct WeatherAttributionLinks: Codable, Equatable, Sendable {
  public let legalPageURL: URL
  public let combinedMarkLightURL: URL
  public let combinedMarkDarkURL: URL

  enum CodingKeys: String, CodingKey {
    case legalPageURL = "legal_page_url"
    case combinedMarkLightURL = "combined_mark_light_url"
    case combinedMarkDarkURL = "combined_mark_dark_url"
  }
}

public struct FeasibilityResult: Codable, Equatable, Sendable {
  public let view: FeasibilityView
  public let weatherAttribution: WeatherAttributionLinks

  enum CodingKeys: String, CodingKey {
    case view
    case weatherAttribution = "weather_attribution"
  }
}

public struct LocationReading: Equatable, Sendable {
  public let coordinate: Coordinate
  public let observedAt: Date
  public let horizontalAccuracyMeters: Double

  public init(coordinate: Coordinate, observedAt: Date, horizontalAccuracyMeters: Double) {
    self.coordinate = coordinate
    self.observedAt = observedAt
    self.horizontalAccuracyMeters = horizontalAccuracyMeters
  }
}

public struct RouteReading: Equatable, Sendable {
  public let expectedTravelTime: TimeInterval

  public init(expectedTravelTime: TimeInterval) {
    self.expectedTravelTime = expectedTravelTime
  }
}

public struct WeatherReading: Equatable, Sendable {
  public let precipitationChance: Double
  public let windKilometersPerHour: Double
  public let severeCondition: Bool

  public init(
    precipitationChance: Double,
    windKilometersPerHour: Double,
    severeCondition: Bool
  ) {
    self.precipitationChance = precipitationChance
    self.windKilometersPerHour = windKilometersPerHour
    self.severeCondition = severeCondition
  }
}

public protocol CurrentLocationProviding: Sendable {
  func currentLocation(deadline: Date) async throws -> LocationReading
}

public protocol DirectionsProviding: Sendable {
  func route(
    from origin: Coordinate,
    to destination: Coordinate,
    mode: TravelMode,
    deadline: Date
  ) async throws -> RouteReading
}

public protocol EventWeatherProviding: Sendable {
  func weather(
    at destination: Coordinate,
    from start: Date,
    through end: Date,
    deadline: Date
  ) async throws -> WeatherReading
  func attribution(deadline: Date) async throws -> WeatherAttributionLinks
}

public struct FeasibilityPolicy: Equatable, Sendable {
  public let locationMaximumAge: TimeInterval
  public let maximumLocationAccuracyMeters: Double
  public let viewTTL: TimeInterval
  public let maximumRequestDuration: TimeInterval
  public let maximumEventWindow: TimeInterval

  public init(
    locationMaximumAge: TimeInterval = 60,
    maximumLocationAccuracyMeters: Double = 5_000,
    viewTTL: TimeInterval = 300,
    maximumRequestDuration: TimeInterval = 30,
    maximumEventWindow: TimeInterval = 24 * 60 * 60
  ) {
    self.locationMaximumAge = locationMaximumAge
    self.maximumLocationAccuracyMeters = maximumLocationAccuracyMeters
    self.viewTTL = viewTTL
    self.maximumRequestDuration = maximumRequestDuration
    self.maximumEventWindow = maximumEventWindow
  }
}

public struct AppleFeasibilityProvider: Sendable {
  private let location: any CurrentLocationProviding
  private let directions: any DirectionsProviding
  private let weather: any EventWeatherProviding
  private let policy: FeasibilityPolicy
  private let now: @Sendable () -> Date

  public init(
    location: any CurrentLocationProviding,
    directions: any DirectionsProviding,
    weather: any EventWeatherProviding,
    policy: FeasibilityPolicy = FeasibilityPolicy(),
    now: @escaping @Sendable () -> Date = Date.init
  ) {
    self.location = location
    self.directions = directions
    self.weather = weather
    self.policy = policy
    self.now = now
  }

  public func feasibility(for request: FeasibilityRequest) async throws -> FeasibilityResult {
    let queryStartedAt = now()
    try validate(request, at: queryStartedAt)

    let origin = try await withDeadline(request.deadline, provider: "core_location") {
      try await location.currentLocation(deadline: request.deadline)
    }
    let locationAge = now().timeIntervalSince(origin.observedAt)
    guard locationAge >= 0, locationAge <= policy.locationMaximumAge else {
      throw FeasibilityFailure(code: .staleLocation, provider: "core_location")
    }
    guard origin.coordinate.isValid,
          origin.horizontalAccuracyMeters >= 0,
          origin.horizontalAccuracyMeters <= policy.maximumLocationAccuracyMeters else {
      throw FeasibilityFailure(code: .locationUnavailable, provider: "core_location")
    }

    async let route = withDeadline(request.deadline, provider: "map_kit") {
      try await directions.route(
        from: origin.coordinate,
        to: request.destination,
        mode: request.travelMode,
        deadline: request.deadline
      )
    }
    async let eventWeather = withDeadline(request.deadline, provider: "weather_kit") {
      try await weather.weather(
        at: request.destination,
        from: request.eventStart,
        through: request.eventEnd,
        deadline: request.deadline
      )
    }
    async let attribution = withDeadline(request.deadline, provider: "weather_kit") {
      try await weather.attribution(deadline: request.deadline)
    }
    let (routeReading, weatherReading, attributionLinks) = try await (
      route, eventWeather, attribution
    )

    let observedAt = now()
    guard observedAt < request.deadline else {
      throw FeasibilityFailure(code: .timeout, provider: "apple_feasibility")
    }
    guard routeReading.expectedTravelTime >= 0,
          routeReading.expectedTravelTime <= 86_400 else {
      throw FeasibilityFailure(code: .providerFailure, provider: "map_kit")
    }

    let travelSeconds = UInt32(routeReading.expectedTravelTime.rounded(.up))
    let leaveBy = request.eventStart.addingTimeInterval(-TimeInterval(travelSeconds))
    let expiresAt = observedAt.addingTimeInterval(min(policy.viewTTL, 300))

    return FeasibilityResult(
      view: FeasibilityView(
        schemaVersion: 1,
        viewID: "schedule.feasibility",
        sourceHandle: request.sourceHandle,
        observedAtUnixMs: observedAt.unixMilliseconds,
        expiresAtUnixMs: expiresAt.unixMilliseconds,
        items: [
          FeasibilityItem(
            eventHandle: request.eventHandle,
            evidenceHandles: request.evidenceHandles,
            travelDurationSeconds: travelSeconds,
            leaveByUnixMs: leaveBy.unixMilliseconds,
            weatherImpact: weatherReading.impact,
            confidenceMillis: confidence(for: origin)
          )
        ]
      ),
      weatherAttribution: attributionLinks
    )
  }

  private func validate(_ request: FeasibilityRequest, at now: Date) throws {
    let handles = [request.eventHandle, request.sourceHandle] + request.evidenceHandles
    guard policy.locationMaximumAge > 0,
          policy.locationMaximumAge <= 300,
          policy.maximumLocationAccuracyMeters > 0,
          policy.maximumLocationAccuracyMeters <= 50_000,
          policy.viewTTL > 0,
          policy.viewTTL <= 300,
          policy.maximumRequestDuration > 0,
          policy.maximumRequestDuration <= 30,
          policy.maximumEventWindow > 0,
          policy.maximumEventWindow <= 86_400,
          handles.allSatisfy(Self.validHandle),
          !request.evidenceHandles.isEmpty,
          request.evidenceHandles.count <= 8,
          Set(request.evidenceHandles).count == request.evidenceHandles.count,
          request.destination.isValid,
          request.eventEnd > request.eventStart,
          request.eventEnd.timeIntervalSince(request.eventStart) <= policy.maximumEventWindow,
          request.deadline > now,
          request.deadline.timeIntervalSince(now) <= policy.maximumRequestDuration else {
      throw FeasibilityFailure(code: .invalidInput, provider: "apple_feasibility")
    }
  }

  private static func validHandle(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 128 && !value.contains(where: { $0.isWhitespace })
  }

  private func confidence(for reading: LocationReading) -> UInt16 {
    let accuracyRatio = min(1, reading.horizontalAccuracyMeters / policy.maximumLocationAccuracyMeters)
    return UInt16(max(500, (1_000 - accuracyRatio * 400).rounded()))
  }
}

private func withDeadline<Value: Sendable>(
  _ deadline: Date,
  provider: String,
  operation: @escaping @Sendable () async throws -> Value
) async throws -> Value {
  let remaining = deadline.timeIntervalSinceNow
  guard remaining > 0 else {
    throw FeasibilityFailure(code: .timeout, provider: provider)
  }

  return try await withThrowingTaskGroup(of: Value.self) { group in
    group.addTask { try await operation() }
    group.addTask {
      try await Task.sleep(for: .seconds(remaining))
      try Task.checkCancellation()
      throw FeasibilityFailure(code: .timeout, provider: provider)
    }
    guard let first = try await group.next() else {
      throw FeasibilityFailure(code: .providerFailure, provider: provider)
    }
    group.cancelAll()
    return first
  }
}

extension Date {
  fileprivate var unixMilliseconds: Int64 {
    Int64((timeIntervalSince1970 * 1_000).rounded())
  }
}

extension WeatherReading {
  fileprivate var impact: WeatherImpact {
    if severeCondition || precipitationChance >= 0.7 || windKilometersPerHour >= 60 {
      return .significant
    }
    if precipitationChance >= 0.3 || windKilometersPerHour >= 35 {
      return .minor
    }
    return .none
  }
}

public actor MapKitDirectionsProvider: DirectionsProviding {
  public init() {}

  public func route(
    from origin: Coordinate,
    to destination: Coordinate,
    mode: TravelMode,
    deadline: Date
  ) async throws -> RouteReading {
    guard Date() < deadline else {
      throw FeasibilityFailure(code: .timeout, provider: "map_kit")
    }
    let request = MKDirections.Request()
    request.source = MKMapItem(
      placemark: MKPlacemark(
        coordinate: CLLocationCoordinate2D(latitude: origin.latitude, longitude: origin.longitude)
      )
    )
    request.destination = MKMapItem(
      placemark: MKPlacemark(
        coordinate: CLLocationCoordinate2D(
          latitude: destination.latitude,
          longitude: destination.longitude
        )
      )
    )
    request.transportType = mode.mapKitTransportType

    let directions = CancellableDirections(request: request)
    do {
      let eta = try await withTaskCancellationHandler {
        try await directions.value.calculateETA()
      } onCancel: {
        directions.value.cancel()
      }
      guard Date() < deadline else {
        throw FeasibilityFailure(code: .timeout, provider: "map_kit")
      }
      return RouteReading(expectedTravelTime: eta.expectedTravelTime)
    } catch let failure as FeasibilityFailure {
      throw failure
    } catch let error as MKError where error.code == .directionsNotFound {
      throw FeasibilityFailure(code: .unsupportedRegion, provider: "map_kit")
    } catch {
      throw FeasibilityFailure(code: .providerFailure, provider: "map_kit")
    }
  }
}

private final class CancellableDirections: @unchecked Sendable {
  let value: MKDirections

  init(request: MKDirections.Request) {
    value = MKDirections(request: request)
  }
}

extension TravelMode {
  fileprivate var mapKitTransportType: MKDirectionsTransportType {
    switch self {
    case .automobile: .automobile
    case .transit: .transit
    case .walking: .walking
    }
  }
}

public actor WeatherKitEventWeatherProvider: EventWeatherProviding {
  public init() {}

  public func weather(
    at destination: Coordinate,
    from start: Date,
    through end: Date,
    deadline: Date
  ) async throws -> WeatherReading {
    guard Date() < deadline else {
      throw FeasibilityFailure(code: .timeout, provider: "weather_kit")
    }
    do {
      let forecast = try await WeatherService.shared.weather(
        for: CLLocation(latitude: destination.latitude, longitude: destination.longitude),
        including: .hourly
      )
      let hours = forecast.forecast.filter { $0.date >= start && $0.date <= end }
      guard !hours.isEmpty else {
        throw FeasibilityFailure(code: .providerUnavailable, provider: "weather_kit")
      }
      guard Date() < deadline else {
        throw FeasibilityFailure(code: .timeout, provider: "weather_kit")
      }
      return WeatherReading(
        precipitationChance: hours.map(\.precipitationChance).max() ?? 0,
        windKilometersPerHour: hours.map {
          $0.wind.speed.converted(to: .kilometersPerHour).value
        }.max() ?? 0,
        severeCondition: hours.contains { Self.isSevere($0.condition) }
      )
    } catch let failure as FeasibilityFailure {
      throw failure
    } catch {
      throw FeasibilityFailure(code: .providerFailure, provider: "weather_kit")
    }
  }

  public func attribution(deadline: Date) async throws -> WeatherAttributionLinks {
    guard Date() < deadline else {
      throw FeasibilityFailure(code: .timeout, provider: "weather_kit")
    }
    do {
      let attribution = try await WeatherService.shared.attribution
      return WeatherAttributionLinks(
        legalPageURL: attribution.legalPageURL,
        combinedMarkLightURL: attribution.combinedMarkLightURL,
        combinedMarkDarkURL: attribution.combinedMarkDarkURL
      )
    } catch {
      throw FeasibilityFailure(code: .providerFailure, provider: "weather_kit")
    }
  }

  private static func isSevere(_ condition: WeatherCondition) -> Bool {
    switch condition {
    case .blizzard, .blowingDust, .blowingSnow, .freezingDrizzle, .freezingRain,
         .heavyRain, .heavySnow, .hurricane, .isolatedThunderstorms, .scatteredThunderstorms,
         .sleet, .strongStorms, .thunderstorms, .tropicalStorm:
      true
    default:
      false
    }
  }
}

@MainActor
public final class CoreLocationOneShotProvider: NSObject, CurrentLocationProviding,
  @preconcurrency CLLocationManagerDelegate, @unchecked Sendable
{
  private let manager: CLLocationManager
  private var continuation: CheckedContinuation<LocationReading, Error>?
  private var timeoutTask: Task<Void, Never>?

  public override init() {
    manager = CLLocationManager()
    super.init()
    manager.delegate = self
    manager.desiredAccuracy = kCLLocationAccuracyHundredMeters
  }

  public func currentLocation(deadline: Date) async throws -> LocationReading {
    guard continuation == nil else {
      throw FeasibilityFailure(code: .providerUnavailable, provider: "core_location")
    }
    guard Date() < deadline else {
      throw FeasibilityFailure(code: .timeout, provider: "core_location")
    }
    guard CLLocationManager.locationServicesEnabled() else {
      throw FeasibilityFailure(code: .locationUnavailable, provider: "core_location")
    }
    let needsAuthorization: Bool
    switch manager.authorizationStatus {
    case .authorizedAlways, .authorizedWhenInUse:
      needsAuthorization = false
    case .notDetermined:
      needsAuthorization = true
    case .denied, .restricted:
      throw FeasibilityFailure(code: .permissionDenied, provider: "core_location")
    @unknown default:
      throw FeasibilityFailure(code: .providerUnavailable, provider: "core_location")
    }

    return try await withCheckedThrowingContinuation { continuation in
      self.continuation = continuation
      if needsAuthorization {
        manager.requestWhenInUseAuthorization()
      } else {
        manager.requestLocation()
      }
      timeoutTask = Task { [weak self] in
        let duration = max(0, deadline.timeIntervalSinceNow)
        try? await Task.sleep(for: .seconds(duration))
        guard !Task.isCancelled else { return }
        await MainActor.run {
          self?.finish(
            throwing: FeasibilityFailure(code: .timeout, provider: "core_location")
          )
        }
      }
    }
  }

  public func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
    guard let location = locations.last else {
      finish(
        throwing: FeasibilityFailure(code: .locationUnavailable, provider: "core_location")
      )
      return
    }
    finish(
      returning: LocationReading(
        coordinate: Coordinate(
          latitude: location.coordinate.latitude,
          longitude: location.coordinate.longitude
        ),
        observedAt: location.timestamp,
        horizontalAccuracyMeters: location.horizontalAccuracy
      )
    )
  }

  public func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
    let code: FeasibilityFailureCode
    if let locationError = error as? CLError, locationError.code == .denied {
      code = .permissionDenied
    } else {
      code = .locationUnavailable
    }
    finish(throwing: FeasibilityFailure(code: code, provider: "core_location"))
  }

  public func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
    guard continuation != nil else { return }
    switch manager.authorizationStatus {
    case .authorizedAlways, .authorizedWhenInUse:
      manager.requestLocation()
    case .denied, .restricted:
      finish(throwing: FeasibilityFailure(code: .permissionDenied, provider: "core_location"))
    default:
      break
    }
  }

  private func finish(returning reading: LocationReading) {
    guard let continuation else { return }
    self.continuation = nil
    timeoutTask?.cancel()
    timeoutTask = nil
    manager.stopUpdatingLocation()
    continuation.resume(returning: reading)
  }

  private func finish(throwing error: Error) {
    guard let continuation else { return }
    self.continuation = nil
    timeoutTask?.cancel()
    timeoutTask = nil
    manager.stopUpdatingLocation()
    continuation.resume(throwing: error)
  }
}
