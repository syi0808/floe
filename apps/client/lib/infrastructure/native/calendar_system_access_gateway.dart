import 'package:flutter/services.dart';
import 'package:floe_client/features/connections/application/calendar_system_access_gateway.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';

final class EventKitSystemAccessGateway implements CalendarSystemAccessGateway {
  const EventKitSystemAccessGateway();
  static const _channel = MethodChannel('floe/calendar');
  static const _timeout = Duration(seconds: 3);

  @override
  Future<CalendarSystemAccess> inspect() async {
    try {
      final value = await _channel
          .invokeMethod<String>('inspectSystemAccess')
          .timeout(_timeout);
      return switch (value) {
        'allowed' => CalendarSystemAccess.allowed,
        'not_requested' => CalendarSystemAccess.notRequested,
        'denied' => CalendarSystemAccess.denied,
        'restricted' => CalendarSystemAccess.restricted,
        'write_only' => CalendarSystemAccess.writeOnly,
        'unavailable' => CalendarSystemAccess.unavailable,
        _ => throw const FormatException('Invalid Calendar access status.'),
      };
    } catch (error, stack) {
      AppDiagnostics.error(
        component: 'connections',
        operation: 'calendar_system_access',
        error: error,
        stackTrace: stack,
        retryable: true,
      );
      return CalendarSystemAccess.unavailable;
    }
  }

  @override
  Future<void> openSettings() async {
    try {
      final opened = await _channel
          .invokeMethod<bool>('openSystemAccessSettings')
          .timeout(_timeout);
      if (opened != true) throw StateError('System Settings did not open.');
    } catch (error, stack) {
      AppDiagnostics.error(
        component: 'connections',
        operation: 'calendar_access_settings',
        error: error,
        stackTrace: stack,
        retryable: true,
      );
      rethrow;
    }
  }
}
