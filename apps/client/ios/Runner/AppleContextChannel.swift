import CoreLocation
import FloeAppleContacts
import FloeAppleHealth
import FloeFeasibilityProvider
import FloeScreenTimeGate
import Flutter
import Foundation
import Security

@MainActor
final class AppleContextChannel {
  private static let channelName = "floe/apple_context"
  private static let contactsScope = "CNContactStore.read"
  private static let healthScope = "HKHealthStore.derived.read"
  private static let locationScope = "CLLocationManager.when_in_use"

  private let channel: FlutterMethodChannel
  private let contacts: AppleContactsProvider
  private let health = HealthKitWellbeingProvider.currentHostProvider(
    sourceHandle: "wellbeing:apple-health"
  )
  private let feasibility: AppleFeasibilityProvider
  private var contactsLastView: [String: Any]?
  private var contactsLastSuccess: Int64?
  private var healthLastView: [String: Any]?
  private var healthLastSuccess: Int64?
  private var feasibilityLastView: [String: Any]?
  private var feasibilityLastSuccess: Int64?

  init(messenger: FlutterBinaryMessenger) throws {
    channel = FlutterMethodChannel(name: Self.channelName, binaryMessenger: messenger)
    contacts = try AppleContactsProvider(handleSecret: try Self.contactsHandleSecret())
    feasibility = AppleFeasibilityProvider(
      location: CoreLocationOneShotProvider(),
      directions: MapKitDirectionsProvider(),
      weather: WeatherKitEventWeatherProvider()
    )
    channel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(FlutterError(code: "unavailable", message: "Apple context host is unavailable.", details: nil))
        return
      }
      Task { @MainActor in
        await self.handle(call, result: result)
      }
    }
  }

  private func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) async {
    do {
      switch call.method {
      case "connections":
        result(await connectionSnapshots())
      case "requestPermission":
        result(try await requestPermission(arguments(call)))
      case "readContacts":
        result(try readContacts(arguments(call)))
      case "readFeasibility":
        result(try await readFeasibility(arguments(call)))
      case "readWellbeing":
        result(try await readWellbeing())
      case "screenTimeCapability":
        result(try screenTimeCapability())
      default:
        result(FlutterMethodNotImplemented)
      }
    } catch let failure as ChannelFailure {
      result(FlutterError(code: failure.code, message: failure.message, details: nil))
    } catch let failure as FeasibilityFailure {
      result(FlutterError(code: failure.code.rawValue, message: "Apple feasibility source failed.", details: ["provider": failure.provider]))
    } catch let failure as AppleContactsProviderError {
      result(FlutterError(code: contactsErrorCode(failure), message: "Apple Contacts source failed.", details: nil))
    } catch let failure as HealthKitWellbeingFailure {
      result(FlutterError(code: healthErrorCode(failure), message: "Apple Health source failed.", details: nil))
    } catch {
      result(FlutterError(code: "unavailable", message: "Apple context source is unavailable.", details: nil))
    }
  }

  private func requestPermission(_ arguments: [String: Any]) async throws -> [String: Any] {
    try requireExactKeys(arguments, ["source"])
    guard let source = arguments["source"] as? String else { throw ChannelFailure.invalidInput }
    switch source {
    case "contacts":
      return ["granted": try await contacts.requestAuthorization().canRead]
    case "health":
      let lifecycle = try await health.requestReadAuthorization()
      return ["granted": lifecycle.state != .permissionRequired && lifecycle.state != .unsupported]
    default:
      throw ChannelFailure.invalidInput
    }
  }

  private func readContacts(_ arguments: [String: Any]) throws -> [String: Any] {
    try requireExactKeys(arguments, ["limit"])
    guard let limit = arguments["limit"] as? Int, (1...64).contains(limit) else {
      throw ChannelFailure.invalidInput
    }
    let value = try encodedObject(contacts.readPeopleView(limit: limit))
    contactsLastView = value
    contactsLastSuccess = value["observed_at_unix_ms"] as? Int64
    return value
  }

  private func readFeasibility(_ arguments: [String: Any]) async throws -> [String: Any] {
    let keys: Set<String> = [
      "event_handle", "evidence_handles", "destination_latitude", "destination_longitude",
      "event_start_unix_ms", "event_end_unix_ms", "travel_mode", "source_handle", "timeout_ms",
    ]
    try requireExactKeys(arguments, keys)
    guard let eventHandle = boundedHandle(arguments["event_handle"]),
          let evidenceHandles = arguments["evidence_handles"] as? [String],
          !evidenceHandles.isEmpty, evidenceHandles.count <= 8,
          evidenceHandles.allSatisfy({ boundedHandle($0) != nil }),
          Set(evidenceHandles).count == evidenceHandles.count,
          let latitude = arguments["destination_latitude"] as? Double,
          let longitude = arguments["destination_longitude"] as? Double,
          let startMs = int64(arguments["event_start_unix_ms"]),
          let endMs = int64(arguments["event_end_unix_ms"]),
          let modeName = arguments["travel_mode"] as? String,
          let mode = TravelMode(rawValue: modeName),
          let sourceHandle = boundedHandle(arguments["source_handle"]),
          let timeoutMs = arguments["timeout_ms"] as? Int,
          (1...30_000).contains(timeoutMs)
    else { throw ChannelFailure.invalidInput }
    let now = Date()
    let result = try await feasibility.feasibility(
      for: FeasibilityRequest(
        eventHandle: eventHandle,
        evidenceHandles: evidenceHandles,
        destination: Coordinate(latitude: latitude, longitude: longitude),
        eventStart: date(startMs),
        eventEnd: date(endMs),
        travelMode: mode,
        sourceHandle: sourceHandle,
        deadline: now.addingTimeInterval(Double(timeoutMs) / 1_000)
      )
    )
    let value = try encodedObject(result)
    feasibilityLastView = value["view"] as? [String: Any]
    feasibilityLastSuccess = feasibilityLastView?["observed_at_unix_ms"] as? Int64
    return value
  }

  private func readWellbeing() async throws -> [String: Any] {
    let view = try await health.readDerivedWellbeing()
    let value = try encodedObject(view)
    healthLastView = value
    healthLastSuccess = value["observed_at_unix_ms"] as? Int64
    return value
  }

  private func screenTimeCapability() throws -> [String: Any] {
    if #available(iOS 16.0, *) {
      return try encodedObject(
        AppleScreenTimePublicAPI.currentCapability(
          entitlementProvisioned: false,
          regionAvailability: .unknown
        )
      )
    }
    return try encodedObject(
      ScreenTimeGate.evaluate(
        ScreenTimeGateInputs(
          platformSupported: true,
          apiAvailable: false,
          entitlementProvisioned: false,
          authorization: .notDetermined,
          regionAvailability: .unknown
        ),
        observedAtUnixMs: nowMilliseconds()
      )
    )
  }

  private func connectionSnapshots() async -> [[String: Any]] {
    let observed = nowMilliseconds()
    let contactsSnapshot = contacts.connectionSnapshot()
    let healthLifecycle = await health.lifecycle()
    let locationAuthorization = CLLocationManager.authorizationStatus()
    let screenTime = try? screenTimeCapability()
    return [
      connectionSnapshot(
        connectorID: "contacts.apple", provider: "apple_contacts",
        capabilityID: "contacts.identity.read", requiredScopes: [Self.contactsScope],
        viewID: "people.identity", freshnessMs: 300_000, maxItems: 64, maxBytes: 32_768,
        authorized: contactsSnapshot.canRead, unsupported: false,
        lastView: contactsLastView, lastSuccess: contactsLastSuccess,
        itemKey: "identities", observed: observed
      ),
      connectionSnapshot(
        connectorID: "feasibility.apple", provider: "apple_feasibility",
        capabilityID: "schedule.feasibility.read", requiredScopes: [Self.locationScope],
        viewID: "schedule.feasibility", freshnessMs: 300_000, maxItems: 1, maxBytes: 16_384,
        authorized: locationAuthorization == .authorizedAlways || locationAuthorization == .authorizedWhenInUse,
        unsupported: !CLLocationManager.locationServicesEnabled(),
        lastView: feasibilityLastView, lastSuccess: feasibilityLastSuccess,
        itemKey: "items", observed: observed
      ),
      connectionSnapshot(
        connectorID: "health.apple", provider: "apple_health",
        capabilityID: "health.derived.read", requiredScopes: [Self.healthScope],
        viewID: "wellbeing.derived", freshnessMs: 1_800_000, maxItems: 1, maxBytes: 8_192,
        authorized: healthLifecycle.state != .permissionRequired,
        unsupported: healthLifecycle.state == .unsupported,
        lastView: healthLastView, lastSuccess: healthLastSuccess,
        itemKey: nil, observed: observed
      ),
      connectionSnapshot(
        connectorID: "attention.apple", provider: "apple_screen_time",
        capabilityID: "attention.coarse.read", requiredScopes: ["FamilyControls.authorization"],
        viewID: "attention.coarse", freshnessMs: 120_000, maxItems: 1, maxBytes: 8_192,
        authorized: false, unsupported: true, lastView: nil, lastSuccess: nil,
        itemKey: nil, observed: observed,
        failureKind: (screenTime?["outcome"] as? String) ?? "entitlement_unavailable"
      ),
    ]
  }

  private func connectionSnapshot(
    connectorID: String, provider: String, capabilityID: String,
    requiredScopes: [String], viewID: String, freshnessMs: Int,
    maxItems: Int, maxBytes: Int, authorized: Bool, unsupported: Bool,
    lastView: [String: Any]?, lastSuccess: Int64?, itemKey: String?, observed: Int64,
    failureKind: String? = nil
  ) -> [String: Any] {
    let fresh = authorized && (lastView?["expires_at_unix_ms"] as? Int64 ?? 0) > observed
    let state: String = unsupported ? "unsupported" : !authorized ? "revoked" : fresh ? "ready" : lastSuccess == nil ? "pending" : "unavailable"
    var connection: [String: Any] = [
      "schema_version": 1, "connector_id": connectorID, "state": state,
      "granted_scopes": authorized ? requiredScopes : [], "observed_at_unix_ms": observed,
    ]
    if let lastSuccess { connection["last_success_at_unix_ms"] = lastSuccess }
    if unsupported || !authorized || (lastSuccess != nil && !fresh) {
      connection["last_failure"] = [
        "kind": failureKind ?? (unsupported ? "unsupported" : !authorized ? "permission_denied" : "stale"),
        "observed_at_unix_ms": observed,
      ]
    }
    var views: [[String: Any]] = []
    if fresh, let lastView,
       let source = lastView["source_handle"] as? String,
       let viewObserved = lastView["observed_at_unix_ms"] as? Int64,
       let expires = lastView["expires_at_unix_ms"] as? Int64 {
      let items = itemKey.flatMap { lastView[$0] as? [Any] }
      let evidence = lastView["evidence_handles"] as? [Any]
      views = [[
        "schema_version": 1, "view_id": viewID, "source_handle": source,
        "observed_at_unix_ms": viewObserved, "expires_at_unix_ms": expires,
        "item_count": items?.count ?? (evidence?.isEmpty == false ? 1 : 0),
        "byte_count": (try? JSONSerialization.data(withJSONObject: lastView).count) ?? 0,
        "provenance_count": items?.count ?? evidence?.count ?? 0,
      ]]
    }
    return [
      "descriptor": [
        "schema_version": 1, "id": connectorID, "version": "1.0.0", "provider": provider,
        "execution": ["kind": "device", "device_id": deviceID()],
        "capabilities": [[
          "schema_version": 1, "id": capabilityID, "version": "1.0.0", "authority": "observe",
          "required_scopes": requiredScopes, "output_view_id": viewID,
        ]],
        "views": [[
          "schema_version": 1, "id": viewID, "version": "1.0.0", "data_class": "personal",
          "retention": "ephemeral", "freshness_ttl_ms": freshnessMs,
          "max_items": maxItems, "max_bytes": maxBytes, "provenance_required": true,
        ]],
      ],
      "connection": connection,
      "views": views,
    ]
  }

  private func arguments(_ call: FlutterMethodCall) throws -> [String: Any] {
    guard let arguments = call.arguments as? [String: Any] else { throw ChannelFailure.invalidInput }
    return arguments
  }

  private func requireExactKeys(_ value: [String: Any], _ keys: Set<String>) throws {
    guard Set(value.keys) == keys else { throw ChannelFailure.invalidInput }
  }

  private func encodedObject<T: Encodable>(_ value: T) throws -> [String: Any] {
    let encoder = JSONEncoder()
    encoder.keyEncodingStrategy = .convertToSnakeCase
    let object = try JSONSerialization.jsonObject(with: encoder.encode(value))
    guard let result = object as? [String: Any] else { throw ChannelFailure.unavailable }
    return result
  }

  private func contactsErrorCode(_ failure: AppleContactsProviderError) -> String {
    switch failure {
    case .permissionRequired: "permission_denied"
    case .invalidHandleSecret, .invalidLimit, .invalidSelection: "invalid_input"
    case .authorizationRequestFailed, .storeReadFailed: "unavailable"
    }
  }

  private func healthErrorCode(_ failure: HealthKitWellbeingFailure) -> String {
    switch failure {
    case .unsupported: "unsupported"
    case .permissionRequired: "permission_denied"
    case .noDataOrReadAccessLimited: "no_data"
    case .unavailable: "unavailable"
    }
  }

  private func int64(_ value: Any?) -> Int64? {
    if let value = value as? Int64 { return value }
    if let value = value as? Int { return Int64(value) }
    if let value = value as? NSNumber { return value.int64Value }
    return nil
  }

  private func boundedHandle(_ value: Any?) -> String? {
    guard let value = value as? String, !value.isEmpty, value.utf8.count <= 512,
          !value.contains(where: { $0.isWhitespace }) else { return nil }
    return value
  }

  private func date(_ unixMs: Int64) -> Date {
    Date(timeIntervalSince1970: Double(unixMs) / 1_000)
  }

  private func nowMilliseconds() -> Int64 {
    Int64(Date().timeIntervalSince1970 * 1_000)
  }

  private func deviceID() -> String {
    let key = "floe.apple.device_id"
    if let value = UserDefaults.standard.string(forKey: key) { return value }
    let value = "apple-\(UUID().uuidString.lowercased())"
    UserDefaults.standard.set(value, forKey: key)
    return value
  }

  private static func contactsHandleSecret() throws -> Data {
    let service = "app.floe.contacts-handles"
    let account = "local-device"
    let query: [String: Any] = [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: account,
      kSecReturnData as String: true,
      kSecMatchLimit as String: kSecMatchLimitOne,
    ]
    var result: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &result)
    if status == errSecSuccess, let data = result as? Data, data.count >= 32 { return data }
    guard status == errSecItemNotFound else { throw ChannelFailure.unavailable }
    var bytes = [UInt8](repeating: 0, count: 32)
    guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
      throw ChannelFailure.unavailable
    }
    let data = Data(bytes)
    var insert = query
    insert.removeValue(forKey: kSecReturnData as String)
    insert.removeValue(forKey: kSecMatchLimit as String)
    insert[kSecValueData as String] = data
    insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    guard SecItemAdd(insert as CFDictionary, nil) == errSecSuccess else {
      throw ChannelFailure.unavailable
    }
    return data
  }
}

private struct ChannelFailure: Error {
  let code: String
  let message: String

  static let invalidInput = ChannelFailure(code: "invalid_input", message: "Apple context arguments are invalid.")
  static let unavailable = ChannelFailure(code: "unavailable", message: "Apple context source is unavailable.")
}
