import Flutter
import UIKit
import Security
import LocalAuthentication

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  private var appleContextChannel: AppleContextChannel?
  private var unavailableAppleContextChannel: FlutterMethodChannel?
  private var calendarChannel: CalendarChannel?
  private let localServerBridge = IOSLocalServerBridge()
  private var localServerChannel: FlutterMethodChannel?

  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    if let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "FloeAppleContext") {
      let channel = FlutterMethodChannel(name: "floe/local-server", binaryMessenger: registrar.messenger())
      channel.setMethodCallHandler(localServerBridge.handle)
      localServerChannel = channel
      calendarChannel = CalendarChannel(messenger: registrar.messenger())
      do {
        appleContextChannel = try AppleContextChannel(messenger: registrar.messenger())
      } catch {
        let channel = FlutterMethodChannel(
          name: "floe/apple_context",
          binaryMessenger: registrar.messenger()
        )
        channel.setMethodCallHandler { _, result in
          result(
            FlutterError(
              code: "unavailable",
              message: "Apple context host could not initialize.",
              details: nil
            )
          )
        }
        unavailableAppleContextChannel = channel
      }
    }
  }
}

final class IOSLocalServerBridge {
  private let credentialQueue = DispatchQueue(label: "floe.local-server.credentials")
  private var query: [String: Any] {
    [kSecClass as String: kSecClassGenericPassword,
     kSecAttrService as String: "app.floe.local-server",
     kSecAttrAccount as String: "connection-v1"]
  }

  func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "open":
      guard let source = call.arguments as? String,
            let url = URL(string: source), url.scheme == "http", url.host == "127.0.0.1",
            url.user == nil, url.password == nil, url.query == nil, url.fragment == nil,
            url.path == "/manage/" || url.path == "/manage" else {
        result(failure()); return
      }
      UIApplication.shared.open(url) { opened in
        result(opened ? nil : self.failure())
      }
    default:
      credentialQueue.async {
        self.handleCredential(call) { value in
          DispatchQueue.main.async { result(value) }
        }
      }
    }
  }

  private func handleCredential(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
    switch call.method {
    case "read":
      var request = query
      request[kSecReturnData as String] = true
      request[kSecMatchLimit as String] = kSecMatchLimitOne
      let authentication = LAContext()
      authentication.interactionNotAllowed = true
      request[kSecUseAuthenticationContext as String] = authentication
      var found: CFTypeRef?
      let status = SecItemCopyMatching(request as CFDictionary, &found)
      if status == errSecItemNotFound { result(nil); return }
      guard status == errSecSuccess, let data = found as? Data,
            let value = String(data: data, encoding: .utf8) else { result(failure()); return }
      result(value)
    case "write":
      guard let value = call.arguments as? String, value.utf8.count <= 4096,
            let data = value.data(using: .utf8) else { result(failure()); return }
      let update = [kSecValueData as String: data]
      var status = SecItemUpdate(query as CFDictionary, update as CFDictionary)
      if status == errSecItemNotFound {
        var item = query
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        status = SecItemAdd(item as CFDictionary, nil)
      }
      result(status == errSecSuccess ? nil : failure())
    case "delete":
      let status = SecItemDelete(query as CFDictionary)
      result(status == errSecSuccess || status == errSecItemNotFound ? nil : failure())
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  private func failure() -> FlutterError {
    FlutterError(code: "credential_store_unavailable", message: "Could not access the local server connection.", details: nil)
  }
}
