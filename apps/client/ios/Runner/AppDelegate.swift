import Flutter
import UIKit

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  private var appleContextChannel: AppleContextChannel?
  private var unavailableAppleContextChannel: FlutterMethodChannel?

  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    if let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "FloeAppleContext") {
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
