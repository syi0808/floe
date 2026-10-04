import CryptoKit
import FloeAppleContacts
import FloeAppleHealth
import FloeAppleWellbeing
import FloeHealthTransformBridge
import FloeScreenTimeGate
import Flutter
import Foundation
import Security
import Darwin

@MainActor
final class AppleContextChannel {
  private static let channelName = "floe/apple_context"
  private static let contactsScope = "CNContactStore.read"
  private static let healthScope = "HKHealthStore.derived.read"

  private let channel: FlutterMethodChannel
  private let contacts: AppleContactsProvider
  private let nativeSubjectKey: SymmetricKey
  private let health: AppleWellbeingSource
  private var contactsLastView: [String: Any]?
  private var contactsLastSuccess: Int64?
  private var healthLastView: [String: Any]?
  private var healthLastSuccess: Int64?
  private var boundDeviceID: String?

  init(messenger: FlutterBinaryMessenger) throws {
    channel = FlutterMethodChannel(name: Self.channelName, binaryMessenger: messenger)
    let handleSecret = try Self.contactsHandleSecret()
    nativeSubjectKey = SymmetricKey(data: handleSecret)
    contacts = try AppleContactsProvider(handleSecret: handleSecret)
    health = AppleWellbeingSource(
      acquisition: HealthKitWellbeingProvider.currentHostProvider(),
      transform: try BundledHealthTransformClient(),
      sourceHandle: "wellbeing:apple-health"
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
      let arguments = try arguments(call)
      let deviceID = try bindDeviceID(arguments)
      switch call.method {
      case "connections":
        try requireExactKeys(arguments, ["device_id"])
        result(await connectionSnapshots(deviceID: deviceID))
      case "requestPermissionAcquisition":
        result(try await requestPermissionAcquisition(arguments, deviceID: deviceID))
      case "readContacts":
        result(try readContacts(arguments))
      case "inspectContactsSubject":
        result(try inspectContactsSubject(arguments))
      case "inspectContactsCatalog":
        try requireExactKeys(arguments, ["device_id"])
        result(try encodedObject(contacts.inspectCatalog()))
      case "inspectWellbeingSubject":
        try requireExactKeys(arguments, ["device_id"])
        result(try await inspectWellbeingSubject(deviceID: deviceID))
      case "inspectWellbeingCatalog":
        try requireExactKeys(arguments, ["device_id"])
        let subject = try await inspectWellbeingSubject(deviceID: deviceID)
        result([
          "resources": [["handle": "wellbeing.derived", "label": "Wellbeing summary"]],
          "native_subject_fingerprint": subject["subject_fingerprint"]!,
          "permission_class": subject["permission_class"]!,
          "catalog_complete": true,
        ])
      case "readWellbeing":
        try requireExactKeys(arguments, ["device_id", "transform_binding"])
        guard let value = arguments["transform_binding"] as? [String: Any] else { throw ChannelFailure.invalidInput }
        let binding = try HealthTransformBinding.decode(JSONSerialization.data(withJSONObject: value))
        guard binding.deviceID == deviceID else { throw ChannelFailure.invalidInput }
        result(try await readWellbeing(binding: binding))
      case "screenTimeCapability":
        try requireExactKeys(arguments, ["device_id"])
        result(try screenTimeCapability())
      default:
        result(FlutterMethodNotImplemented)
      }
    } catch let failure as ChannelFailure {
      result(FlutterError(code: failure.code, message: failure.message, details: nil))
    } catch let failure as AppleContactsProviderError {
      result(FlutterError(code: contactsErrorCode(failure), message: "Apple Contacts source failed.", details: nil))
    } catch let failure as AppleWellbeingFailure {
      result(FlutterError(code: healthErrorCode(failure), message: "Apple Health source failed.", details: nil))
    } catch {
      result(FlutterError(code: "unavailable", message: "Apple context source is unavailable.", details: nil))
    }
  }

  private func requestPermissionAcquisition(_ arguments: [String: Any], deviceID: String) async throws -> [String: Any] {
    guard arguments["mode"] as? String == "request_permission",
          let requestID = arguments["request_id"] as? String, UUID(uuidString: requestID) != nil,
          let personID = arguments["person_id"] as? String, UUID(uuidString: personID) != nil,
          let epoch = arguments["host_epoch"] as? String, !epoch.isEmpty, epoch.utf8.count <= 128,
          let handles = arguments["selected_handles"] as? [String], handles.isEmpty,
          let deadline = arguments["deadline_unix_ms"] as? NSNumber, deadline.int64Value > nowMilliseconds(),
          let domain = arguments["domain"] as? String, ["people", "wellbeing"].contains(domain)
    else { throw ChannelFailure.invalidInput }
    func fingerprint(_ status: String) -> String {
      let bytes = Data("floe.permission.subject.v1\0\(deviceID)\0\(domain)\0\(status)".utf8)
      return HMAC<SHA256>.authenticationCode(for: bytes, using: nativeSubjectKey).map { String(format: "%02x", $0) }.joined()
    }
    let beforeStatus = domain == "people" ? contacts.connectionSnapshot().authorization.rawValue : (await health.lifecycle()).state.rawValue
    let outcome: String
    if domain == "people" {
      outcome = try await contacts.requestAuthorization().canRead ? "request_completed" : "denied"
    } else {
      do {
        _ = try await health.requestReadAuthorization()
        outcome = "request_completed"
      } catch AppleWellbeingFailure.unsupported {
        outcome = "unavailable"
      } catch {
        outcome = "unavailable"
      }
    }
    let afterStatus = domain == "people" ? contacts.connectionSnapshot().authorization.rawValue : (await health.lifecycle()).state.rawValue
    return ["native_subject_fingerprint_before": fingerprint(beforeStatus),
            "native_subject_fingerprint_after": fingerprint(afterStatus), "permission_class": outcome]
  }

  private func readContacts(_ arguments: [String: Any]) throws -> [String: Any] {
    guard Set(arguments.keys).isSubset(of: ["device_id", "limit", "selected_handles"]) else {
      throw ChannelFailure.invalidInput
    }
    guard let limit = arguments["limit"] as? Int, (1...64).contains(limit) else {
      throw ChannelFailure.invalidInput
    }
    let selection: AppleContactsSelection
    if let selected = arguments["selected_handles"] {
      guard let handles = selected as? [String], !handles.isEmpty, handles.count <= 64,
            Set(handles).count == handles.count,
            handles.allSatisfy({ boundedHandle($0) != nil }) else {
        throw ChannelFailure.invalidInput
      }
      selection = .identityHandles(Set(handles))
    } else {
      selection = .allAuthorized
    }
    let value = try encodedObject(contacts.readPeopleView(selection: selection, limit: limit))
    contactsLastView = value
    contactsLastSuccess = value["observed_at_unix_ms"] as? Int64
    return value
  }

  private func inspectContactsSubject(_ arguments: [String: Any]) throws -> [String: Any] {
    try requireExactKeys(arguments, ["device_id", "selected_handles"])
    guard let selected = arguments["selected_handles"] as? [String],
          !selected.isEmpty, selected.count <= 64,
          Set(selected).count == selected.count,
          selected.allSatisfy({ boundedHandle($0) != nil }) else {
      throw ChannelFailure.invalidInput
    }
    let subject = try contacts.inspectSelectedSubject(selected)
    return [
      "schema_version": 1,
      "subject_fingerprint": subject.fingerprint,
      "permission_class": subject.permissionClass,
      "resolved_handles": subject.resolvedHandles,
    ]
  }

  private func readWellbeing(binding: HealthTransformBinding) async throws -> [String: Any] {
    let before = try await inspectWellbeingSubject(deviceID: binding.deviceID)
    guard before["subject_fingerprint"] as? String == binding.nativeSubjectFingerprint else { throw ChannelFailure.invalidInput }
    let observation = try await health.readDerivedWellbeing(binding: binding)
    let after = try await inspectWellbeingSubject(deviceID: binding.deviceID)
    guard before["subject_fingerprint"] as? String == after["subject_fingerprint"] as? String else { throw ChannelFailure.invalidInput }
    let value = try encodedObject(observation.view)
    healthLastView = value
    healthLastSuccess = value["observed_at_unix_ms"] as? Int64
    return ["view": value, "privacy_transform": try encodedObject(observation.privacyTransform)]
  }

  private func inspectWellbeingSubject(deviceID: String) async throws -> [String: Any] {
    let lifecycle = await health.lifecycle()
    guard lifecycle.state != .unsupported else { throw ChannelFailure.unsupported }
    let identity = [
      "floe.wellbeing.subject.v1",
      deviceID,
      "sleep_analysis",
      "step_count",
      "apple_exercise_time",
      "window:36h",
      "derived:v1",
    ]
    let data = try JSONSerialization.data(withJSONObject: identity)
    let fingerprint = HMAC<SHA256>.authenticationCode(for: data, using: nativeSubjectKey)
      .map { String(format: "%02x", $0) }.joined()
    return [
      "schema_version": 1,
      "subject_fingerprint": fingerprint,
      "permission_class": lifecycle.state.rawValue,
    ]
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

  private func connectionSnapshots(deviceID: String) async -> [[String: Any]] {
    let observed = nowMilliseconds()
    let contactsSnapshot = contacts.connectionSnapshot()
    let healthLifecycle = await health.lifecycle()
    let screenTime = try? screenTimeCapability()
    return [
      connectionSnapshot(
        connectorID: "contacts.apple", provider: "apple_contacts",
        capabilityID: "contacts.identity.read", requiredScopes: [Self.contactsScope],
        viewID: "people.identity", dataClass: "personal", freshnessMs: 300_000, maxItems: 64, maxBytes: 32_768,
        authorized: contactsSnapshot.canRead, unsupported: false,
        lastView: contactsLastView, lastSuccess: contactsLastSuccess,
        itemKey: "identities", observed: observed, deviceID: deviceID
      ),
      connectionSnapshot(
        connectorID: "health.apple", provider: "apple_health",
        capabilityID: "health.derived.read", requiredScopes: [Self.healthScope],
        viewID: "wellbeing.derived", dataClass: "highly_sensitive", freshnessMs: 1_800_000, maxItems: 1, maxBytes: 8_192,
        authorized: healthLifecycle.state == .ready,
        unsupported: healthLifecycle.state == .unsupported,
        lastView: healthLastView, lastSuccess: healthLastSuccess,
        itemKey: nil, observed: observed, deviceID: deviceID
      ),
      connectionSnapshot(
        connectorID: "attention.apple", provider: "apple_screen_time",
        capabilityID: "attention.coarse.read", requiredScopes: ["FamilyControls.authorization"],
        viewID: "attention.coarse", dataClass: "personal", freshnessMs: 120_000, maxItems: 1, maxBytes: 8_192,
        authorized: false, unsupported: true, lastView: nil, lastSuccess: nil,
        itemKey: nil, observed: observed, deviceID: deviceID,
        failureKind: (screenTime?["outcome"] as? String) ?? "entitlement_unavailable"
      ),
    ]
  }

  private func connectionSnapshot(
    connectorID: String, provider: String, capabilityID: String,
    requiredScopes: [String], viewID: String, dataClass: String, freshnessMs: Int,
    maxItems: Int, maxBytes: Int, authorized: Bool, unsupported: Bool,
    lastView: [String: Any]?, lastSuccess: Int64?, itemKey: String?, observed: Int64,
    deviceID: String,
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
        "execution": ["kind": "device", "device_id": deviceID],
        "capabilities": [[
          "schema_version": 1, "id": capabilityID, "version": "1.0.0", "authority": "observe",
          "required_scopes": requiredScopes, "output_view_id": viewID,
        ]],
        "views": [[
          "schema_version": 1, "id": viewID, "version": "1.0.0", "data_class": dataClass,
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

  private func bindDeviceID(_ arguments: [String: Any]) throws -> String {
    guard let value = arguments["device_id"] as? String,
          !value.isEmpty, value.utf8.count <= 128,
          !value.contains(where: { $0.isWhitespace }) else {
      throw ChannelFailure.invalidInput
    }
    if let boundDeviceID {
      guard boundDeviceID == value else { throw ChannelFailure.invalidInput }
    } else {
      boundDeviceID = value
    }
    return value
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
    case .selectionUnresolved: "unavailable"
    case .authorizationRequestFailed, .storeReadFailed: "unavailable"
    }
  }

  private func healthErrorCode(_ failure: AppleWellbeingFailure) -> String {
    switch failure {
    case .unsupported: "unsupported"
    case .permissionRequired: "permission_denied"
    case .noDataOrReadAccessLimited: "no_data"
    case .unavailable: "unavailable"
    case .privacyTransform(let failure): failure.rawValue
    }
  }

  private func boundedHandle(_ value: Any?) -> String? {
    guard let value = value as? String, !value.isEmpty, value.utf8.count <= 512,
          !value.contains(where: { $0.isWhitespace }) else { return nil }
    return value
  }

  private func nowMilliseconds() -> Int64 {
    Int64(Date().timeIntervalSince1970 * 1_000)
  }

  private static func contactsHandleSecret() throws -> Data {
    #if FLOE_DEVELOPMENT_STORAGE
    return try developmentContactsHandleSecret()
    #else
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
    #endif
  }

  #if FLOE_DEVELOPMENT_STORAGE
  /// Debug-only custody for stable synthetic Contacts handles. This namespace
  /// never reads, copies or replaces the production Keychain item.
  private static func developmentContactsHandleSecret() throws -> Data {
    let manager = FileManager.default
    let support = try manager.url(for: .applicationSupportDirectory, in: .userDomainMask,
                                  appropriateFor: nil, create: true)
    let directory = support.appendingPathComponent("FloeDevelopmentNative", isDirectory: true)
    if !manager.fileExists(atPath: directory.path) {
      try manager.createDirectory(at: directory, withIntermediateDirectories: false,
                                  attributes: [.posixPermissions: 0o700])
    }
    var directoryInfo = stat()
    guard directory.path.withCString({ lstat($0, &directoryInfo) }) == 0,
          directoryInfo.st_mode & mode_t(S_IFMT) == mode_t(S_IFDIR),
          directoryInfo.st_mode & 0o077 == 0, directoryInfo.st_uid == geteuid()
    else { throw ChannelFailure.unavailable }
    let path = directory.appendingPathComponent("contacts-handle-key").path
    var descriptor = path.withCString { Darwin.open($0, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC) }
    if descriptor < 0 {
      guard errno == ENOENT else { throw ChannelFailure.unavailable }
      var bytes = [UInt8](repeating: 0, count: 32)
      guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess
      else { throw ChannelFailure.unavailable }
      descriptor = path.withCString { Darwin.open($0, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600) }
      guard descriptor >= 0 else { throw ChannelFailure.unavailable }
      defer { Darwin.close(descriptor) }
      let written = bytes.withUnsafeBytes { Darwin.write(descriptor, $0.baseAddress!, $0.count) }
      guard written == bytes.count, fsync(descriptor) == 0 else { throw ChannelFailure.unavailable }
      let parent = directory.path.withCString { Darwin.open($0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC) }
      guard parent >= 0 else { throw ChannelFailure.unavailable }
      defer { Darwin.close(parent) }
      guard fsync(parent) == 0 else { throw ChannelFailure.unavailable }
      return Data(bytes)
    }
    defer { Darwin.close(descriptor) }
    var info = stat()
    guard fstat(descriptor, &info) == 0,
          info.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG), info.st_mode & 0o077 == 0,
          info.st_uid == geteuid(), info.st_nlink == 1, info.st_size == 32
    else { throw ChannelFailure.unavailable }
    let data = try FileHandle(fileDescriptor: descriptor, closeOnDealloc: false).read(upToCount: 33)
    guard let data, data.count == 32 else { throw ChannelFailure.unavailable }
    return data
  }
  #endif
}

private struct ChannelFailure: Error {
  let code: String
  let message: String

  static let invalidInput = ChannelFailure(code: "invalid_input", message: "Apple context arguments are invalid.")
  static let unsupported = ChannelFailure(code: "unsupported", message: "Apple Health is unsupported on this device.")
  static let unavailable = ChannelFailure(code: "unavailable", message: "Apple context source is unavailable.")
}
