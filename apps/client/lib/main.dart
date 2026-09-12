import 'dart:async';
import 'dart:io';
import 'dart:ui';

import 'package:floe_client/l10n/app_localizations.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

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
import 'infrastructure/native/macos_context_gateway.dart';
import 'infrastructure/native/attention_acquisition_broker.dart';
import 'infrastructure/native/calendar_acquisition_broker.dart';
import 'infrastructure/native/local_context_publication.dart';
import 'infrastructure/native/personal_acquisition_broker.dart';
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
    final calendarAdapter = androidNative == null
        ? EventKitCalendarAdapter(deviceId: device.id)
        : AndroidCalendarAdapter(androidNative);
    final gateway = await FfiDayGateway.openDefault(
      serverClient: serverClient,
      deviceId: device.id,
      calendarAdapter: calendarAdapter,
    );
    CalendarAcquisitionService? calendarAcquisition;
    if (Platform.isIOS || androidNative != null) {
      final reader = Platform.isIOS
          ? (calendarAdapter as EventKitCalendarAdapter).readAcquisition
          : androidNative!.readAcquisition;
      calendarAcquisition = CalendarAcquisitionService(
        broker: CalendarAcquisitionBroker(
          transport: gateway.localContextTransport,
          personId: localPersonId,
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
        transport: gateway.localContextTransport,
        personId: localPersonId,
      );
      attentionAcquisition = AttentionAcquisitionService(
        broker: broker,
        reader: (request) async {
          final mode = request['mode'];
          final deviceId = request['device_id'];
          if (mode is! String || deviceId is! String || deviceId != device.id) {
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
    Timer? macOSContextRefresh;
    final appleNativeGateway = Platform.isIOS
        ? AppleContextGateway(deviceId: device.id)
        : null;
    final appleContext = appleNativeGateway == null
        ? null
        : PublishingAppleContextGateway(
            gateway: appleNativeGateway,
            transport: gateway.localContextTransport,
            personId: localPersonId,
            deviceId: device.id,
          );
    final androidContext = androidNative == null
        ? null
        : PublishingAndroidContextGateway(
            gateway: androidNative,
            transport: gateway.localContextTransport,
            personId: localPersonId,
            deviceId: device.id,
          );
    PersonalAcquisitionService? personalAcquisition;
    final personalReader = appleNativeGateway != null
        ? _applePersonalReader(appleNativeGateway, device.id)
        : androidNative != null
        ? _androidContactsReader(androidNative, device.id)
        : null;
    if (personalReader != null) {
      final broker = PersonalAcquisitionBroker(
        transport: gateway.localContextTransport,
        personId: localPersonId,
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
    if (Platform.isMacOS) {
      final macOSContext = PublishingMacOSContextGateway(
        gateway: macOSContextGateway!,
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
        macOSContext: macOSContextGateway,
        onDisposeGateway: () async {
          macOSContextRefresh?.cancel();
          await calendarAcquisition?.dispose();
          await attentionAcquisition?.dispose();
          await personalAcquisition?.dispose();
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

PersonalAcquisitionReader _applePersonalReader(
  AppleContextGateway native,
  String deviceId,
) => (request) async {
  if (request['domain'] == 'feasibility' && request['device_id'] == deviceId) {
    final before = await native.inspectFeasibilitySubject();
    final expected = request['expected_native_subject_fingerprint'];
    if (expected != null && before['subject_fingerprint'] != expected) {
      throw PlatformException(code: 'permission_denied');
    }
    final remaining =
        (request['deadline_unix_ms'] as int) -
        DateTime.now().toUtc().millisecondsSinceEpoch;
    if (remaining <= 0) throw PlatformException(code: 'cancelled');
    final result = await native.readFeasibility(
      AppleFeasibilityQuery(
        eventHandle: request['event_handle'] as String,
        evidenceHandles: (request['evidence_handles'] as List).cast<String>(),
        latitude: (request['destination_latitude'] as num).toDouble(),
        longitude: (request['destination_longitude'] as num).toDouble(),
        eventStart: DateTime.fromMillisecondsSinceEpoch(
          request['event_start_unix_ms'] as int,
          isUtc: true,
        ),
        eventEnd: DateTime.fromMillisecondsSinceEpoch(
          request['event_end_unix_ms'] as int,
          isUtc: true,
        ),
        travelMode: AppleTravelMode.values.byName(
          request['travel_mode'] as String,
        ),
        timeout: Duration(milliseconds: remaining.clamp(1, 20000)),
      ),
      governed: true,
    );
    final after = await native.inspectFeasibilitySubject();
    if (after['subject_fingerprint'] != before['subject_fingerprint']) {
      throw PlatformException(code: 'permission_denied');
    }
    return _personalPeopleResult(
      request,
      before,
      after,
      Map<String, dynamic>.from(result['view'] as Map),
      'apple_feasibility',
    );
  }
  if (request['domain'] == 'wellbeing' && request['device_id'] == deviceId) {
    final before = await native.inspectWellbeingSubject();
    final expected = request['expected_native_subject_fingerprint'];
    if (expected != null && before['subject_fingerprint'] != expected) {
      throw PlatformException(code: 'permission_denied');
    }
    final view = await native.readWellbeing();
    final after = await native.inspectWellbeingSubject();
    if (after['subject_fingerprint'] != before['subject_fingerprint']) {
      throw PlatformException(code: 'permission_denied');
    }
    return _personalPeopleResult(request, before, after, view, 'apple_health');
  }
  final selected = request['selected_handles'];
  if (request['domain'] != 'people' ||
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
  return _personalPeopleResult(request, before, after, view, 'apple_contacts');
};

PersonalAcquisitionReader _androidContactsReader(
  AndroidContextGateway native,
  String deviceId,
) => (request) async {
  final selected = request['selected_handles'];
  if (request['domain'] != 'people' ||
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
  Map<String, dynamic> view,
  String provider,
) => {
  'request_id': request['request_id'],
  'host_epoch': request['host_epoch'],
  'person_id': request['person_id'],
  'device_id': request['device_id'],
  'domain': request['domain'],
  'native_subject_fingerprint_before': before['subject_fingerprint'],
  'native_subject_fingerprint_after': after['subject_fingerprint'],
  'permission_class': before['permission_class'],
  'provider': provider,
  'view': view,
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
