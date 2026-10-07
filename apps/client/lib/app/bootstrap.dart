import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/day/application/day_gateway.dart';
import 'package:floe_client/features/day/infrastructure/app_wire_day_gateway.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:floe_client/infrastructure/native/platform_acquisition_services.dart';

/// Owns the app-lifetime Flutter runtime and its native callback services.
final class ClientAppBootstrap {
  ClientAppBootstrap._(this.runtime, this._platformServices)
    : dayGateway = AppWireDayGateway(runtime);

  final AppRuntime runtime;
  final DayGateway dayGateway;
  final PlatformAcquisitionServices _platformServices;
  Future<void>? _closing;

  /// Opens the production runtime, registers native callbacks, then prepares
  /// Vault through that same runtime. Native registration failures remain
  /// individually diagnosed and do not prevent Day from opening.
  static Future<ClientAppBootstrap> openDefault() async {
    AppRuntime? openedRuntime;
    ClientAppBootstrap? bootstrap;
    try {
      final runtime = await AppRuntime.openDefault();
      openedRuntime = runtime;
      final openedBootstrap = ClientAppBootstrap._(
        runtime,
        PlatformAcquisitionServices.forRuntime(runtime),
      );
      bootstrap = openedBootstrap;
      await openedBootstrap._platformServices.startBefore(runtime.startVault);
      return openedBootstrap;
    } on Object catch (error, stackTrace) {
      if (bootstrap != null) {
        await bootstrap.close();
      } else if (openedRuntime != null) {
        await _closeOpenedRuntime(openedRuntime);
      }
      Error.throwWithStackTrace(error, stackTrace);
    }
  }

  /// Repeated calls join the original shutdown and never begin a second drain.
  Future<void> close() => _closing ??= _close();

  Future<void> _close() async {
    try {
      runtime.closeAdmission();
    } on Object catch (error, stackTrace) {
      _recordCleanupFailure(
        'runtime_close_admission',
        error,
        stackTrace,
      );
    }
    try {
      await _platformServices.close();
    } on Object catch (error, stackTrace) {
      _recordCleanupFailure(
        'native_acquisition_disposal',
        error,
        stackTrace,
      );
    } finally {
      await _closeRuntime(runtime);
    }
  }

  static Future<void> _closeOpenedRuntime(AppRuntime runtime) async {
    try {
      runtime.closeAdmission();
    } on Object catch (error, stackTrace) {
      _recordCleanupFailure('runtime_close_admission', error, stackTrace);
    }
    await _closeRuntime(runtime);
  }

  static Future<void> _closeRuntime(AppRuntime runtime) async {
    try {
      await runtime.close();
    } on Object catch (error, stackTrace) {
      _recordCleanupFailure('runtime_close', error, stackTrace);
    }
  }

  static void _recordCleanupFailure(
    String operation,
    Object error,
    StackTrace stackTrace,
  ) {
    AppDiagnostics.error(
      component: 'app',
      operation: operation,
      error: error,
      stackTrace: stackTrace,
      retryable: true,
    );
  }
}
