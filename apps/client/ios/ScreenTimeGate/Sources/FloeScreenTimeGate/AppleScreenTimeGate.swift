#if os(iOS) && canImport(DeviceActivity) && canImport(FamilyControls)
import DeviceActivity
import FamilyControls
import Foundation

@available(iOS 16.0, *)
public enum AppleScreenTimePublicAPI {
  public static func aggregateReportFilter(interval: DateInterval) -> DeviceActivityFilter {
    DeviceActivityFilter(segment: .hourly(during: interval), devices: .all)
  }

  public static func currentCapability(
    entitlementProvisioned: Bool,
    regionAvailability: ScreenTimeRegionAvailability = .notRequired,
    now: Date = Date()
  ) -> ScreenTimeCapability {
    ScreenTimeGate.evaluate(
      ScreenTimeGateInputs(
        platformSupported: true,
        apiAvailable: true,
        entitlementProvisioned: entitlementProvisioned,
        authorization: authorizationStatus(),
        regionAvailability: regionAvailability
      ),
      observedAtUnixMs: Int64(now.timeIntervalSince1970 * 1_000)
    )
  }

  @MainActor
  public static func requestIndividualAuthorization(
    entitlementProvisioned: Bool,
    regionAvailability: ScreenTimeRegionAvailability = .notRequired,
    now: Date = Date()
  ) async -> ScreenTimeCapability {
    let before = currentCapability(
      entitlementProvisioned: entitlementProvisioned,
      regionAvailability: regionAvailability,
      now: now
    )
    guard before.outcome == .authorizationRequired else {
      return before
    }

    do {
      try await AuthorizationCenter.shared.requestAuthorization(for: .individual)
      return currentCapability(
        entitlementProvisioned: entitlementProvisioned,
        regionAvailability: regionAvailability,
        now: Date()
      )
    } catch {
      return ScreenTimeCapability(
        outcome: .providerError,
        authorization: authorizationStatus(),
        regionAvailability: regionAvailability,
        observedAtUnixMs: Int64(Date().timeIntervalSince1970 * 1_000),
        detailCode: "authorization_request_failed"
      )
    }
  }

  private static func authorizationStatus() -> ScreenTimeAuthorization {
    switch AuthorizationCenter.shared.authorizationStatus {
    case .approved:
      return .approved
    case .denied:
      return .denied
    case .notDetermined:
      return .notDetermined
    @unknown default:
      return .notDetermined
    }
  }

}
#endif
