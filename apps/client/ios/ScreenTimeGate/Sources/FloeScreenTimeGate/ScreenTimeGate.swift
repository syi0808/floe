import Foundation

public enum ScreenTimeAuthorization: String, Codable, Sendable {
  case approved
  case denied
  case notDetermined = "not_determined"
}

public enum ScreenTimeRegionAvailability: String, Codable, Sendable {
  case available
  case unavailable
  case notRequired = "not_required"
  case unknown
}

public enum ScreenTimeCapabilityOutcome: String, Codable, Sendable {
  case supported
  case authorizationRequired = "authorization_required"
  case authorizationDenied = "authorization_denied"
  case entitlementUnavailable = "entitlement_unavailable"
  case regionUnavailable = "region_unavailable"
  case regionUnknown = "region_unknown"
  case unsupportedPlatform = "unsupported_platform"
  case apiUnavailable = "api_unavailable"
  case providerError = "provider_error"
}

public struct ScreenTimeCapability: Codable, Equatable, Sendable {
  public let schemaVersion: Int
  public let sourceHandle: String
  public let outcome: ScreenTimeCapabilityOutcome
  public let authorization: ScreenTimeAuthorization
  public let regionAvailability: ScreenTimeRegionAvailability
  public let observedAtUnixMs: Int64
  public let detailCode: String?

  public init(
    outcome: ScreenTimeCapabilityOutcome,
    authorization: ScreenTimeAuthorization,
    regionAvailability: ScreenTimeRegionAvailability,
    observedAtUnixMs: Int64,
    detailCode: String? = nil
  ) {
    schemaVersion = 1
    sourceHandle = "attention:apple-device-activity"
    self.outcome = outcome
    self.authorization = authorization
    self.regionAvailability = regionAvailability
    self.observedAtUnixMs = observedAtUnixMs
    self.detailCode = detailCode
  }
}

public struct ScreenTimeGateInputs: Equatable, Sendable {
  public let platformSupported: Bool
  public let apiAvailable: Bool
  public let entitlementProvisioned: Bool
  public let authorization: ScreenTimeAuthorization
  public let regionAvailability: ScreenTimeRegionAvailability

  public init(
    platformSupported: Bool,
    apiAvailable: Bool,
    entitlementProvisioned: Bool,
    authorization: ScreenTimeAuthorization,
    regionAvailability: ScreenTimeRegionAvailability
  ) {
    self.platformSupported = platformSupported
    self.apiAvailable = apiAvailable
    self.entitlementProvisioned = entitlementProvisioned
    self.authorization = authorization
    self.regionAvailability = regionAvailability
  }
}

public enum ScreenTimeGate {
  public static func evaluate(
    _ inputs: ScreenTimeGateInputs,
    observedAtUnixMs: Int64
  ) -> ScreenTimeCapability {
    let outcome: ScreenTimeCapabilityOutcome
    if !inputs.platformSupported {
      outcome = .unsupportedPlatform
    } else if !inputs.apiAvailable {
      outcome = .apiUnavailable
    } else if !inputs.entitlementProvisioned {
      outcome = .entitlementUnavailable
    } else {
      switch inputs.regionAvailability {
      case .unavailable:
        outcome = .regionUnavailable
      case .unknown:
        outcome = .regionUnknown
      case .available, .notRequired:
        switch inputs.authorization {
        case .approved:
          outcome = .supported
        case .denied:
          outcome = .authorizationDenied
        case .notDetermined:
          outcome = .authorizationRequired
        }
      }
    }

    return ScreenTimeCapability(
      outcome: outcome,
      authorization: inputs.authorization,
      regionAvailability: inputs.regionAvailability,
      observedAtUnixMs: observedAtUnixMs
    )
  }
}

public enum AttentionState: String, Codable, Sendable {
  case available
  case focused
  case highInterruptionPressure = "high_interruption_pressure"
  case unknown
}

public struct CoarseActivityAggregate: Equatable, Sendable {
  public let observedAtUnixMs: Int64
  public let intervalSeconds: Int
  public let activeSeconds: Int
  public let interruptionCount: Int

  public init(
    observedAtUnixMs: Int64,
    intervalSeconds: Int,
    activeSeconds: Int,
    interruptionCount: Int
  ) {
    self.observedAtUnixMs = observedAtUnixMs
    self.intervalSeconds = intervalSeconds
    self.activeSeconds = activeSeconds
    self.interruptionCount = interruptionCount
  }
}

public struct AttentionView: Codable, Equatable, Sendable {
  public let schemaVersion: Int
  public let viewId: String
  public let sourceHandle: String
  public let observedAtUnixMs: Int64
  public let expiresAtUnixMs: Int64
  public let state: AttentionState
  public let confidenceMillis: Int
  public let evidenceHandles: [String]

  public init(
    observedAtUnixMs: Int64,
    expiresAtUnixMs: Int64,
    state: AttentionState,
    confidenceMillis: Int
  ) {
    schemaVersion = 1
    viewId = "attention.coarse"
    sourceHandle = "attention:apple-device-activity"
    self.observedAtUnixMs = observedAtUnixMs
    self.expiresAtUnixMs = expiresAtUnixMs
    self.state = state
    self.confidenceMillis = confidenceMillis
    evidenceHandles = ["attention:device-activity:aggregate"]
  }
}

public enum AttentionReductionError: Error, Equatable {
  case capabilityUnavailable
  case invalidAggregate
}

public enum ScreenTimeAttentionReducer {
  public static func reduce(
    _ aggregate: CoarseActivityAggregate,
    capability: ScreenTimeCapability
  ) throws -> AttentionView {
    guard capability.outcome == .supported else {
      throw AttentionReductionError.capabilityUnavailable
    }
    guard aggregate.intervalSeconds > 0,
      aggregate.activeSeconds >= 0,
      aggregate.activeSeconds <= aggregate.intervalSeconds,
      aggregate.interruptionCount >= 0
    else {
      throw AttentionReductionError.invalidAggregate
    }

    let state: AttentionState
    let confidenceMillis: Int
    if aggregate.interruptionCount >= 8 {
      state = .highInterruptionPressure
      confidenceMillis = 800
    } else if aggregate.activeSeconds >= 600 && aggregate.interruptionCount <= 2 {
      state = .focused
      confidenceMillis = 750
    } else if aggregate.activeSeconds <= 120 {
      state = .available
      confidenceMillis = 650
    } else {
      state = .unknown
      confidenceMillis = 500
    }

    return AttentionView(
      observedAtUnixMs: aggregate.observedAtUnixMs,
      expiresAtUnixMs: aggregate.observedAtUnixMs + 120_000,
      state: state,
      confidenceMillis: confidenceMillis
    )
  }
}

public struct ScreenTimeExport: Codable, Equatable, Sendable {
  public let capability: ScreenTimeCapability
  public let attention: AttentionView?

  public init(capability: ScreenTimeCapability, attention: AttentionView? = nil) {
    precondition(
      attention == nil || capability.outcome == .supported,
      "Attention can only accompany a supported Screen Time capability"
    )
    self.capability = capability
    self.attention = attention
  }
}

extension JSONEncoder {
  public static func floeScreenTimeEncoder() -> JSONEncoder {
    let encoder = JSONEncoder()
    encoder.keyEncodingStrategy = .convertToSnakeCase
    encoder.outputFormatting = [.sortedKeys]
    return encoder
  }
}

extension JSONDecoder {
  public static func floeScreenTimeDecoder() -> JSONDecoder {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    return decoder
  }
}
