import 'dart:async';
import 'dart:io';
import 'dart:ui';

import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:floe_client/app/floe_app.dart';
import 'package:floe_client/app/design_tokens.dart';
import 'package:floe_client/app/floe_primitives.dart';
import 'package:floe_client/app/floe_theme.dart';
import 'package:floe_client/app/runtime/app_runtime.dart';
import 'package:floe_client/features/day/infrastructure/app_wire_day_gateway.dart';
import 'package:floe_client/infrastructure/native/eventkit_calendar_host.dart';
import 'package:floe_client/infrastructure/native/android_context_gateway.dart';
import 'package:floe_client/infrastructure/native/apple_context_gateway.dart';
import 'package:floe_client/infrastructure/native/macos_context_gateway.dart';
import 'package:floe_client/infrastructure/native/attention_acquisition_broker.dart';
import 'package:floe_client/infrastructure/native/calendar_acquisition_broker.dart';
import 'package:floe_client/infrastructure/native/personal_acquisition_broker.dart';
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
  try {
    // App-lifetime objects are created once here; no feature owns them.
    final runtime = await AppRuntime.openDefault();
    final androidNative = Platform.isAndroid ? AndroidContextGateway() : null;
    final calendarHost = EventKitCalendarHost(deviceId: runtime.deviceId);
    final gateway = AppWireDayGateway(runtime);
    CalendarAcquisitionService? calendarAcquisition;
    if (Platform.isMacOS || Platform.isIOS || androidNative != null) {
      final reader = Platform.isMacOS || Platform.isIOS
          ? calendarHost.readAcquisition
          : androidNative!.readAcquisition;
      calendarAcquisition = CalendarAcquisitionService(
        broker: CalendarAcquisitionBroker(
          transport: runtime.nativeHostTransport,
        ),
        reader: reader,
      );
      try {
        await calendarAcquisition.start();
      } on Object catch (error, stackTrace) {
        _recordOptionalContextFailure(error, stackTrace);
        await calendarAcquisition.dispose();
        calendarAcquisition = null;
      }
    }
    final macOSContextGateway = Platform.isMacOS ? MacOSContextGateway() : null;
    AttentionAcquisitionService? attentionAcquisition;
    if (Platform.isMacOS) {
      final attentionGateway = macOSContextGateway!;
      final broker = AttentionAcquisitionBroker(
        transport: runtime.nativeHostTransport,
      );
      attentionAcquisition = AttentionAcquisitionService(
        broker: broker,
        reader: (request) async {
          final mode = request['mode'];
          final deviceId = request['device_id'];
          if (mode is! String || deviceId is! String || deviceId != runtime.deviceId) {
            throw PlatformException(code: 'permission_denied');
          }
          final before = await attentionGateway.inspectAttentionSubject(
            deviceId,
          );
          final beforeFingerprint = before['subject_fingerprint'];
          final expected = request['expected_native_subject_fingerprint'];
          if (beforeFingerprint is! String ||
              (mode == 'read_projection' && beforeFingerprint != expected)) {
            throw PlatformException(code: 'permission_denied');
          }
          Map<String, dynamic>? view;
          if (mode == 'read_projection') {
            view = await attentionGateway.readAttention();
          } else if (mode != 'inspect_subject') {
            throw PlatformException(code: 'provider_unavailable');
          }
          final after = await attentionGateway.inspectAttentionSubject(
            deviceId,
          );
          final afterFingerprint = after['subject_fingerprint'];
          if (afterFingerprint != beforeFingerprint) {
            throw PlatformException(code: 'permission_denied');
          }
          return {
            'request_id': request['request_id'],
            'host_epoch': request['host_epoch'],
            'person_id': request['person_id'],
            'device_id': deviceId,
            'mode': mode,
            'native_subject_fingerprint_before': beforeFingerprint,
            'native_subject_fingerprint_after': afterFingerprint,
            'permission_class': before['permission_class'],
            'view': view,
          };
        },
      );
      try {
        await attentionAcquisition.start();
      } on Object catch (error, stackTrace) {
        _recordOptionalContextFailure(error, stackTrace);
        await attentionAcquisition.dispose();
        attentionAcquisition = null;
      }
    }
    final appleNativeGateway = Platform.isIOS
        ? AppleContextGateway(deviceId: runtime.deviceId)
        : null;
    PersonalAcquisitionService? personalAcquisition;
    final personalReader = appleNativeGateway != null
        ? _applePersonalReader(appleNativeGateway, runtime.deviceId)
        : androidNative != null
        ? _androidContactsReader(androidNative, runtime.deviceId)
        : null;
    if (personalReader != null) {
      final broker = PersonalAcquisitionBroker(
        transport: runtime.nativeHostTransport,
      );
      personalAcquisition = PersonalAcquisitionService(
        broker: broker,
        reader: personalReader,
      );
      try {
        await personalAcquisition.start();
      } on Object catch (error, stackTrace) {
        _recordOptionalContextFailure(error, stackTrace);
        await personalAcquisition.dispose();
        personalAcquisition = null;
      }
    }
    runApp(
      FloeApp(
        personId: runtime.personId,
        gateway: gateway,
        calendarActions: runtime.actions,
        agentGateway: runtime.conversation,
        ownerGateways: runtime.owners,
        connectionsGateway: runtime.connections,
        onDisposeGateway: () async {
          try {
            await Future.wait([
              if (calendarAcquisition != null) calendarAcquisition.dispose(),
              if (attentionAcquisition != null) attentionAcquisition.dispose(),
              if (personalAcquisition != null) personalAcquisition.dispose(),
            ]).timeout(const Duration(seconds: 5));
          } finally {
            await runtime.close();
          }
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

PersonalAcquisitionReader _applePersonalReader(
  AppleContextGateway native,
  String deviceId,
) => (request) async {
  if (request['device_id'] != deviceId)
    throw PlatformException(code: 'permission_denied');
  final mode = request['mode'];
  final domain = request['domain'];
  if (!{'people', 'wellbeing'}.contains(domain))
    throw PlatformException(code: 'provider_unavailable');
  if (mode == 'request_permission') {
    final completion = await native.requestPermissionAcquisition(request);
    return _personalPeopleResult(
      request,
      {
        'subject_fingerprint': completion['native_subject_fingerprint_before'],
        'permission_class': completion['permission_class'],
      },
      {
        'subject_fingerprint': completion['native_subject_fingerprint_after'],
        'permission_class': completion['permission_class'],
      },
      null,
      domain == 'people' ? 'apple_contacts' : 'apple_health',
    );
  }
  if (mode == 'inspect_catalog') {
    final before = domain == 'people'
        ? await native.inspectContactsCatalog()
        : await native.inspectWellbeingCatalog();
    final after = domain == 'people'
        ? await native.inspectContactsCatalog()
        : await native.inspectWellbeingCatalog();
    if (before['native_subject_fingerprint'] !=
        after['native_subject_fingerprint']) {
      throw PlatformException(code: 'permission_denied');
    }
    final subject = {
      'subject_fingerprint': after['native_subject_fingerprint'],
      'permission_class': after['permission_class'],
    };
    return _personalPeopleResult(
      request,
      subject,
      subject,
      null,
      domain == 'people' ? 'apple_contacts' : 'apple_health',
      resources: (after['resources'] as List).cast<Map>(),
      catalogComplete: after['catalog_complete'] as bool,
    );
  }
  if (domain == 'wellbeing') {
    final before = await native.inspectWellbeingSubject();
    final expected = request['expected_native_subject_fingerprint'];
    if (expected != null && before['subject_fingerprint'] != expected)
      throw PlatformException(code: 'permission_denied');
    if (mode == 'inspect_subject') {
      final after = await native.inspectWellbeingSubject();
      return _personalPeopleResult(
        request,
        before,
        after,
        null,
        'apple_health',
      );
    }
    if (mode != 'read_projection')
      throw PlatformException(code: 'provider_unavailable');
    final result = await native.readWellbeingAcquisition({
      'request_id': request['request_id'],
      'host_epoch': request['host_epoch'],
      'person_id': request['person_id'],
      'device_id': request['device_id'],
      'native_subject_fingerprint': before['subject_fingerprint'],
    });
    final after = await native.inspectWellbeingSubject();
    return {
      ..._personalPeopleResult(
        request,
        before,
        after,
        Map<String, dynamic>.from(result['view'] as Map),
        'apple_health',
      ),
      'transform_operation_id':
          (result['privacy_transform'] as Map)['operation_id'],
    };
  }
  final selected = request['selected_handles'];
  if (domain != 'people' ||
      selected is! List ||
      selected.isEmpty ||
      selected.any((value) => value is! String)) {
    throw PlatformException(code: 'provider_unavailable');
  }
  final handles = selected.cast<String>();
  final before = await native.inspectContactsSubject(handles);
  final expected = request['expected_native_subject_fingerprint'];
  if (expected != null && before['subject_fingerprint'] != expected)
    throw PlatformException(code: 'permission_denied');
  Map<String, dynamic>? view;
  if (mode == 'read_projection') {
    view = await native.readContacts(
      limit: handles.length,
      selectedHandles: handles,
    );
  } else if (mode != 'inspect_subject') {
    throw PlatformException(code: 'provider_unavailable');
  }
  final after = await native.inspectContactsSubject(handles);
  return _personalPeopleResult(request, before, after, view, 'apple_contacts');
};

PersonalAcquisitionReader _androidContactsReader(
  AndroidContextGateway native,
  String deviceId,
) => (request) async {
  final selected = request['selected_handles'];
  if (request['mode'] != 'read_projection' ||
      request['domain'] != 'people' ||
      request['device_id'] != deviceId ||
      selected is! List ||
      selected.isEmpty ||
      selected.any((value) => value is! String)) {
    throw PlatformException(code: 'provider_unavailable');
  }
  final handles = selected.cast<String>();
  final before = await native.inspectContactsSubject(handles);
  final expected = request['expected_native_subject_fingerprint'];
  if (expected != null && before['subject_fingerprint'] != expected) {
    throw PlatformException(code: 'permission_denied');
  }
  final view = await native.readContacts(
    limit: handles.length,
    selectedHandles: handles,
  );
  final after = await native.inspectContactsSubject(handles);
  if (after['subject_fingerprint'] != before['subject_fingerprint']) {
    throw PlatformException(code: 'permission_denied');
  }
  return _personalPeopleResult(
    request,
    before,
    after,
    view,
    'android_contacts',
  );
};

Map<String, dynamic> _personalPeopleResult(
  Map<String, dynamic> request,
  Map<String, dynamic> before,
  Map<String, dynamic> after,
  Map<String, dynamic>? view,
  String provider, {
  List<Map> resources = const [],
  bool catalogComplete = false,
}) => {
  'request_id': request['request_id'],
  'host_epoch': request['host_epoch'],
  'person_id': request['person_id'],
  'device_id': request['device_id'],
  'domain': request['domain'],
  'mode': request['mode'],
  'native_subject_fingerprint_before': before['subject_fingerprint'],
  'native_subject_fingerprint_after': after['subject_fingerprint'],
  'permission_class': before['permission_class'],
  'provider': provider,
  'view': view,
  'transform_operation_id': null,
  'resources': resources,
  'catalog_complete': catalogComplete,
};

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
