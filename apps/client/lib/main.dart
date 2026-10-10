import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import 'package:floe_client/app/bootstrap.dart';
import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/startup_view.dart';
import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:floe_client/preview/design_feedback_overlay.dart';

void main() {
  runZonedGuarded(
    () async {
      WidgetsFlutterBinding.ensureInitialized();
      await AppDiagnostics.initialize();
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
  runApp(const FloeStartupApp.waiting());
  ClientAppBootstrap? bootstrap;
  try {
    final openedBootstrap = await ClientAppBootstrap.openDefault();
    bootstrap = openedBootstrap;
    final runtime = openedBootstrap.runtime;
    runApp(
      FloeApp(
        personId: runtime.personId,
        gateway: openedBootstrap.dayGateway,
        agentGateway: runtime.conversation,
        ownerGateways: runtime.owners,
        connectionsController: runtime.connectionsController,
        onDisposeGateway: openedBootstrap.close,
        builder: kDebugMode
            ? (context, child) => Banner(
                message: 'DEV DATA',
                location: BannerLocation.topEnd,
                child: DesignFeedbackOverlay(child: child!),
              )
            : null,
      ),
    );
    bootstrap = null;
  } on Object catch (error, stackTrace) {
    // runApp can fail after bootstrap succeeds but before its root widget owns
    // the app lifetime. close() joins the same future if disposal also ran.
    if (bootstrap != null) {
      try {
        await bootstrap.close();
      } on Object catch (cleanupError, cleanupStackTrace) {
        AppDiagnostics.error(
          component: 'app',
          operation: 'startup_cleanup',
          error: cleanupError,
          stackTrace: cleanupStackTrace,
          retryable: true,
        );
      }
    }
    final errorId = AppDiagnostics.error(
      component: 'app',
      operation: 'startup',
      error: error,
      stackTrace: stackTrace,
    );
    runApp(FloeStartupApp.failed('${error.toString()}\nError ID: $errorId'));
  }
}
