import Foundation
import Testing
@testable import FloeScreenTimeGate

struct ScreenTimeGateTests {
  private let now: Int64 = 1_789_056_000_000

  @Test func gateModelsEveryAvailabilityBoundary() {
    let cases: [(ScreenTimeGateInputs, ScreenTimeCapabilityOutcome)] = [
      (.init(platformSupported: false, apiAvailable: true, entitlementProvisioned: true, authorization: .approved, regionAvailability: .available), .unsupportedPlatform),
      (.init(platformSupported: true, apiAvailable: false, entitlementProvisioned: true, authorization: .approved, regionAvailability: .available), .apiUnavailable),
      (.init(platformSupported: true, apiAvailable: true, entitlementProvisioned: false, authorization: .approved, regionAvailability: .available), .entitlementUnavailable),
      (.init(platformSupported: true, apiAvailable: true, entitlementProvisioned: true, authorization: .approved, regionAvailability: .unavailable), .regionUnavailable),
      (.init(platformSupported: true, apiAvailable: true, entitlementProvisioned: true, authorization: .approved, regionAvailability: .unknown), .regionUnknown),
      (.init(platformSupported: true, apiAvailable: true, entitlementProvisioned: true, authorization: .notDetermined, regionAvailability: .notRequired), .authorizationRequired),
      (.init(platformSupported: true, apiAvailable: true, entitlementProvisioned: true, authorization: .denied, regionAvailability: .notRequired), .authorizationDenied),
      (.init(platformSupported: true, apiAvailable: true, entitlementProvisioned: true, authorization: .approved, regionAvailability: .notRequired), .supported),
    ]

    for (inputs, expected) in cases {
      #expect(ScreenTimeGate.evaluate(inputs, observedAtUnixMs: now).outcome == expected)
    }
  }

  @Test func reducerExportsOnlyBoundedCoarseAttention() throws {
    let capability = supportedCapability()
    let view = try ScreenTimeAttentionReducer.reduce(
      CoarseActivityAggregate(
        observedAtUnixMs: now,
        intervalSeconds: 900,
        activeSeconds: 720,
        interruptionCount: 1
      ),
      capability: capability
    )

    #expect(view.state == .focused)
    #expect(view.expiresAtUnixMs == now + 120_000)
    #expect(view.evidenceHandles == ["attention:device-activity:aggregate"])

    let data = try JSONEncoder.floeScreenTimeEncoder().encode(
      ScreenTimeExport(capability: capability, attention: view)
    )
    let json = String(decoding: data, as: UTF8.self)
    for forbidden in ["application", "bundle", "domain", "notification", "pickup", "shield", "active_seconds", "interruption_count"] {
      #expect(!json.localizedCaseInsensitiveContains(forbidden))
    }
  }

  @Test func reducerRejectsUnavailableCapabilityAndInvalidInput() {
    let unavailable = ScreenTimeCapability(
      outcome: .entitlementUnavailable,
      authorization: .notDetermined,
      regionAvailability: .unknown,
      observedAtUnixMs: now
    )
    let aggregate = CoarseActivityAggregate(
      observedAtUnixMs: now,
      intervalSeconds: 900,
      activeSeconds: 1_000,
      interruptionCount: 0
    )

    #expect(throws: AttentionReductionError.capabilityUnavailable) {
      try ScreenTimeAttentionReducer.reduce(aggregate, capability: unavailable)
    }
    #expect(throws: AttentionReductionError.invalidAggregate) {
      try ScreenTimeAttentionReducer.reduce(aggregate, capability: supportedCapability())
    }
  }

  @Test func fixturesContainOnlyCapabilityOrCoarseView() throws {
    for name in ["supported_attention", "unsupported_capability"] {
      let url = try #require(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
      let data = try Data(contentsOf: url)
      _ = try JSONDecoder.floeScreenTimeDecoder().decode(ScreenTimeExport.self, from: data)
      let json = String(decoding: data, as: UTF8.self)
      for forbidden in ["bundle_id", "application_name", "domain", "url", "notification", "pickup", "shield"] {
        #expect(!json.localizedCaseInsensitiveContains(forbidden))
      }
    }
  }

  private func supportedCapability() -> ScreenTimeCapability {
    ScreenTimeCapability(
      outcome: .supported,
      authorization: .approved,
      regionAvailability: .notRequired,
      observedAtUnixMs: now
    )
  }
}
