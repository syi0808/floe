import 'dart:async';
import 'dart:io';
import 'dart:ui';

import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import 'app/floe_app.dart';
import 'app/local_identity.dart';
import 'app/design_tokens.dart';
import 'app/floe_primitives.dart';
import 'app/floe_theme.dart';
import 'features/day_canvas/application/ffi_day_gateway.dart';
import 'features/day_canvas/application/calendar_gateway.dart';
import 'features/server/local_server_client.dart';
import 'infrastructure/native/android_context_gateway.dart';
import 'infrastructure/native/apple_context_gateway.dart';
import 'infrastructure/native/local_context_publication.dart';
import 'infrastructure/native/macos_context_gateway.dart';
import 'infrastructure/diagnostics/app_diagnostics.dart';
import 'preview/design_feedback_overlay.dart';

void main() {
  runZonedGuarded(
    () async {
      WidgetsFlutterBinding.ensureInitialized();
      FlutterError.onError = (details) {
        FlutterError.presentError(details);
        AppDiagnostics.error(
          component: 'flutter',
          operation: 'framework_error',
          error: details.exception,
          stackTrace: details.stack,
        );
      };
      PlatformDispatcher.instance.onError = (error, stackTrace) {
        AppDiagnostics.error(
          component: 'flutter',
          operation: 'platform_error',
          error: error,
          stackTrace: stackTrace,
        );
        return true;
      };
      await _start();
    },
    (error, stackTrace) {
      AppDiagnostics.error(
        component: 'flutter',
        operation: 'uncaught_async_error',
        error: error,
        stackTrace: stackTrace,
      );
    },
  );
}

Future<void> _start() async {
  try {
    final androidNative = Platform.isAndroid ? AndroidContextGateway() : null;
    final device = await LocalDeviceIdentity.openDefault();
    final serverClient = LocalServerClient(
      personId: defaultLocalPersonId,
      deviceId: device.id,
    );
    final gateway = await FfiDayGateway.openDefault(
      serverClient: serverClient,
      deviceId: device.id,
      calendarAdapter: androidNative == null
          ? EventKitCalendarAdapter(deviceId: device.id)
          : AndroidCalendarAdapter(androidNative),
    );
    Timer? macOSContextRefresh;
    final appleContext = Platform.isIOS
        ? PublishingAppleContextGateway(
            gateway: AppleContextGateway(deviceId: device.id),
            transport: gateway.localContextTransport,
            personId: localPersonId,
            deviceId: device.id,
          )
        : null;
    final androidContext = androidNative == null
        ? null
        : PublishingAndroidContextGateway(
            gateway: androidNative,
            transport: gateway.localContextTransport,
            personId: localPersonId,
            deviceId: device.id,
          );
    if (Platform.isMacOS) {
      final macOSContext = PublishingMacOSContextGateway(
        gateway: MacOSContextGateway(),
        transport: gateway.localContextTransport,
        personId: localPersonId,
        deviceId: device.id,
      );
      try {
        await macOSContext.readAttention();
      } on Object catch (error, stackTrace) {
        _recordOptionalContextFailure(error, stackTrace);
      }
      macOSContextRefresh = Timer.periodic(const Duration(seconds: 45), (_) {
        unawaited(
          macOSContext.readAttention().catchError((
            Object error,
            StackTrace stackTrace,
          ) {
            _recordOptionalContextFailure(error, stackTrace);
            return <String, dynamic>{};
          }),
        );
      });
    }
    runApp(
      FloeApp(
        gateway: gateway,
        agentGateway: gateway.secureAgent,
        serverClient: gateway.serverClient,
        androidContext: androidContext,
        appleContext: appleContext,
        onDisposeGateway: () async {
          macOSContextRefresh?.cancel();
          await gateway.close();
        },
        builder: kDebugMode
            ? (context, child) => DesignFeedbackOverlay(child: child!)
            : null,
      ),
    );
  } on Object catch (error, stackTrace) {
    final errorId = AppDiagnostics.error(
      component: 'app',
      operation: 'startup',
      error: error,
      stackTrace: stackTrace,
    );
    runApp(
      _StartupErrorApp(message: '${error.toString()}\nError ID: $errorId'),
    );
  }
}

void _recordOptionalContextFailure(Object error, StackTrace stackTrace) {
  AppDiagnostics.error(
    component: 'context',
    operation: 'macos_attention_refresh',
    error: error,
    stackTrace: stackTrace,
    retryable: true,
  );
}

class _StartupErrorApp extends StatelessWidget {
  const _StartupErrorApp({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: FloeTheme.light,
    locale: const Locale('en'),
    supportedLocales: AppLocalizations.supportedLocales,
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: Builder(
      builder: (context) => FloeScaffold(
        body: Center(
          child: ConstrainedBox(
            constraints: BoxConstraints(maxWidth: 480),
            child: Padding(
              padding: EdgeInsets.all(32),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(Icons.error_outline, size: 32),
                  SizedBox(height: 16),
                  Text(
                    AppLocalizations.of(context).couldNotStartFloeCore,
                    style: FloeType.title,
                  ),
                  SizedBox(height: 8),
                  SelectableText(
                    message,
                    textAlign: TextAlign.center,
                    style: FloeType.body,
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    ),
  );
}
