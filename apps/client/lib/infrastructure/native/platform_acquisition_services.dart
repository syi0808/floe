import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';

import 'package:floe_client/infrastructure/diagnostics/app_diagnostics.dart';
import 'package:floe_client/infrastructure/native/android_context_gateway.dart';
import 'package:floe_client/infrastructure/native/apple_context_gateway.dart';
import 'package:floe_client/infrastructure/native/attention_acquisition_broker.dart';
import 'package:floe_client/infrastructure/native/calendar_acquisition_broker.dart';
import 'package:floe_client/infrastructure/native/eventkit_calendar_host.dart';
import 'package:floe_client/infrastructure/native/macos_context_gateway.dart';
import 'package:floe_client/infrastructure/native/native_acquisition_service.dart';
import 'package:floe_client/infrastructure/native/native_context_host_transport.dart';
import 'package:floe_client/infrastructure/native/personal_acquisition_broker.dart';

const _nativeDisposalObservationBound = Duration(seconds: 5);

/// Owns the platform callback services used by one app runtime.
///
/// Production composes the OS-specific services with [forPlatform]. The
/// explicit-services constructor accepts resources assembled at this native
/// boundary without changing the app runtime or product graph.
final class PlatformAcquisitionServices {
  factory PlatformAcquisitionServices(
    Iterable<NativeAcquisitionService> services,
  ) => PlatformAcquisitionServices._(() => services);

  PlatformAcquisitionServices._(this._serviceFactory);

  factory PlatformAcquisitionServices.forPlatform({
    required NativeContextHostTransport transport,
    required String deviceId,
  }) => PlatformAcquisitionServices._(
    () => _platformAcquisitionServices(transport, deviceId),
  );

  final Iterable<NativeAcquisitionService> Function() _serviceFactory;
  final List<NativeAcquisitionService> _ownedServices = [];
  final Map<NativeAcquisitionService, Future<void>> _disposals = {};
  Future<void>? _starting;
  Future<void>? _closing;
  bool _closed = false;

  /// Registers applicable native callback lanes. A single optional lane
  /// failure is diagnosed and isolated from the remaining lanes.
  Future<void> start() {
    if (_closed) {
      return Future<void>.error(
        StateError('Native acquisition services are closed.'),
      );
    }
    return _starting ??= _start();
  }

  Future<void> _start() async {
    final services = _serviceFactory().iterator;
    while (!_closed && services.moveNext()) {
      final service = services.current;
      _ownedServices.add(service);
      try {
        await service.start();
      } on Object catch (error, stackTrace) {
        _recordServiceFailure(
          service,
          'startup',
          error,
          stackTrace,
          retryable: true,
        );
        await _disposeFailedStartup(service);
      }
      if (_closed) return;
    }
  }

  Future<void> close() {
    final closing = _closing;
    if (closing != null) return closing;
    _closed = true;
    return _closing = _close();
  }

  Future<void> _close() async {
    final pending = <Future<void>>[];
    for (final service in _ownedServices) {
      pending.add(_disposeService(service));
    }
    try {
      await Future.wait(pending).timeout(_nativeDisposalObservationBound);
    } on TimeoutException catch (error, stackTrace) {
      AppDiagnostics.error(
        component: 'context',
        operation: 'native_acquisition_disposal_observation_timeout',
        error: error,
        stackTrace: stackTrace,
        failure: 'native_disposal_observation_timeout',
        retryable: true,
      );
      Error.throwWithStackTrace(error, stackTrace);
    }
  }

  Future<void> _disposeFailedStartup(NativeAcquisitionService service) async {
    try {
      await _disposeService(service).timeout(_nativeDisposalObservationBound);
    } on TimeoutException catch (error, stackTrace) {
      AppDiagnostics.error(
        component: 'context',
        operation: '${service.diagnosticOperation}_startup_dispose_timeout',
        error: error,
        stackTrace: stackTrace,
        failure: 'native_disposal_observation_timeout',
        retryable: true,
      );
    } on Object {
      // The failed service is unavailable; its disposal failure was recorded
      // by _disposeService. Other native lanes can still start.
    }
  }

  Future<void> _disposeService(NativeAcquisitionService service) =>
      _disposals.putIfAbsent(service, () {
        try {
          return service.dispose().catchError((Object error, StackTrace stack) {
            _recordServiceFailure(
              service,
              'dispose',
              error,
              stack,
              retryable: true,
            );
            Error.throwWithStackTrace(error, stack);
          });
        } on Object catch (error, stackTrace) {
          _recordServiceFailure(
            service,
            'dispose',
            error,
            stackTrace,
            retryable: true,
          );
          return Future<void>.error(error, stackTrace);
        }
      });

  static void _recordServiceFailure(
    NativeAcquisitionService service,
    String phase,
    Object error,
    StackTrace stackTrace, {
    required bool retryable,
  }) {
    AppDiagnostics.error(
      component: 'context',
      operation: '${service.diagnosticOperation}_$phase',
      error: error,
      stackTrace: stackTrace,
      retryable: retryable,
    );
  }
}

Iterable<NativeAcquisitionService> _platformAcquisitionServices(
  NativeContextHostTransport transport,
  String deviceId,
) sync* {
  final androidNative = Platform.isAndroid ? AndroidContextGateway() : null;
  final calendarHost = EventKitCalendarHost(deviceId: deviceId);
  if (Platform.isMacOS || Platform.isIOS || androidNative != null) {
    final reader = Platform.isMacOS || Platform.isIOS
        ? calendarHost.readAcquisition
        : androidNative!.readAcquisition;
    yield CalendarAcquisitionService(
      broker: CalendarAcquisitionBroker(transport: transport),
      reader: reader,
    );
  }
  final macOSContextGateway = Platform.isMacOS ? MacOSContextGateway() : null;
  if (Platform.isMacOS) {
    final attentionGateway = macOSContextGateway!;
    final broker = AttentionAcquisitionBroker(transport: transport);
    yield AttentionAcquisitionService(
      broker: broker,
      reader: (request) async {
        final mode = request['mode'];
        final requestDeviceId = request['device_id'];
        if (mode is! String ||
            requestDeviceId is! String ||
            requestDeviceId != deviceId) {
          throw PlatformException(code: 'permission_denied');
        }
        final before = await attentionGateway.inspectAttentionSubject(
          requestDeviceId,
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
          requestDeviceId,
        );
        final afterFingerprint = after['subject_fingerprint'];
        if (afterFingerprint != beforeFingerprint) {
          throw PlatformException(code: 'permission_denied');
        }
        return {
          'request_id': request['request_id'],
          'host_epoch': request['host_epoch'],
          'person_id': request['person_id'],
          'device_id': requestDeviceId,
          'mode': mode,
          'native_subject_fingerprint_before': beforeFingerprint,
          'native_subject_fingerprint_after': afterFingerprint,
          'permission_class': before['permission_class'],
          'view': view,
        };
      },
    );
  }
  final appleNativeGateway = Platform.isIOS
      ? AppleContextGateway(deviceId: deviceId)
      : null;
  final personalReader = appleNativeGateway != null
      ? _applePersonalReader(appleNativeGateway, deviceId)
      : androidNative != null
      ? _androidContactsReader(androidNative, deviceId)
      : null;
  if (personalReader != null) {
    final broker = PersonalAcquisitionBroker(transport: transport);
    yield PersonalAcquisitionService(broker: broker, reader: personalReader);
  }
}

PersonalAcquisitionReader _applePersonalReader(
  AppleContextGateway native,
  String deviceId,
) => (request) async {
  if (request['device_id'] != deviceId) {
    throw PlatformException(code: 'permission_denied');
  }
  final mode = request['mode'];
  final domain = request['domain'];
  if (!{'people', 'wellbeing'}.contains(domain)) {
    throw PlatformException(code: 'provider_unavailable');
  }
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
    if (expected != null && before['subject_fingerprint'] != expected) {
      throw PlatformException(code: 'permission_denied');
    }
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
    if (mode != 'read_projection') {
      throw PlatformException(code: 'provider_unavailable');
    }
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
  if (expected != null && before['subject_fingerprint'] != expected) {
    throw PlatformException(code: 'permission_denied');
  }
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
