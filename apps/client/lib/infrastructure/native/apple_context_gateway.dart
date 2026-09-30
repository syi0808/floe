import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

const _channel = MethodChannel('floe/apple_context');

abstract interface class AppleContextApi {
  Future<List<Map<String, dynamic>>> connections();
  Future<bool> requestPermission(AppleContextSource source);
  Future<Map<String, dynamic>> readContacts({
    int limit = 64,
    List<String>? selectedHandles,
  });
  Future<Map<String, dynamic>> readWellbeing();
  Future<Map<String, dynamic>> screenTimeCapability();
}

abstract interface class AppleContextSubjectApi {
  Future<Map<String, dynamic>> inspectContactsSubject(
    List<String> selectedHandles,
  );
}

abstract interface class AppleHealthSubjectApi {
  Future<Map<String, dynamic>> inspectWellbeingSubject();
  Future<bool> requestWellbeingPermission();
}

enum AppleContextSource { contacts, health }

final class AppleContextGateway
    implements AppleContextApi, AppleContextSubjectApi, AppleHealthSubjectApi {
  AppleContextGateway({required String deviceId}) : _deviceId = deviceId {
    validateAppleDeviceId(deviceId);
  }

  final String _deviceId;

  @override
  Future<List<Map<String, dynamic>>> connections() async {
    _requireAppleMobile();
    final values = await _channel.invokeListMethod<Object?>(
      'connections',
      appleNativeArguments(_deviceId),
    );
    final connections = (values ?? const <Object?>[])
        .map(_strictMap)
        .toList(growable: false);
    validateAppleConnectionInventory(connections);
    return connections;
  }

  @override
  Future<bool> requestPermission(AppleContextSource source) async {
    _requireAppleMobile();
    final value = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('requestPermission', {
        'device_id': _deviceId,
        'source': source.name,
      }),
    );
    if (value.keys.toSet().difference({'granted'}).isNotEmpty ||
        value['granted'] is! bool) {
      throw const FormatException('Invalid Apple permission response.');
    }
    return value['granted']! as bool;
  }

  @override
  Future<Map<String, dynamic>> readContacts({
    int limit = 64,
    List<String>? selectedHandles,
  }) async {
    _requireAppleMobile();
    final view = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('readContacts', {
        'device_id': _deviceId,
        'limit': limit,
        'selected_handles': ?selectedHandles,
      }),
    );
    validateApplePeopleView(view);
    return view;
  }

  @override
  Future<Map<String, dynamic>> inspectContactsSubject(
    List<String> selectedHandles,
  ) async {
    _requireAppleMobile();
    if (selectedHandles.isEmpty || selectedHandles.length > 64) {
      throw const FormatException('Contact selection is empty.');
    }
    final value = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>(
        'inspectContactsSubject',
        appleNativeArguments(_deviceId, {'selected_handles': selectedHandles}),
      ),
    );
    const fields = {
      'schema_version',
      'subject_fingerprint',
      'permission_class',
      'resolved_handles',
    };
    if (value.keys.toSet().difference(fields).isNotEmpty ||
        !value.keys.toSet().containsAll(fields) ||
        value['schema_version'] != 1 ||
        value['subject_fingerprint'] is! String ||
        (value['subject_fingerprint']! as String).length != 64 ||
        value['permission_class'] is! String ||
        value['resolved_handles'] is! List ||
        (value['resolved_handles']! as List).length != selectedHandles.length ||
        !(value['resolved_handles']! as List).every(selectedHandles.contains)) {
      throw const FormatException('Invalid Apple Contacts subject.');
    }
    return value;
  }

  @override
  Future<Map<String, dynamic>> inspectWellbeingSubject() async {
    _requireAppleMobile();
    final value = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>(
        'inspectWellbeingSubject',
        appleNativeArguments(_deviceId, const {}),
      ),
    );
    const fields = {
      'schema_version',
      'subject_fingerprint',
      'permission_class',
    };
    if (value.keys.toSet().difference(fields).isNotEmpty ||
        !value.keys.toSet().containsAll(fields) ||
        value['schema_version'] != 1 ||
        value['subject_fingerprint'] is! String ||
        !RegExp(r'^[0-9a-f]{64}$')
            .hasMatch(value['subject_fingerprint'] as String) ||
        value['permission_class'] is! String) {
      throw const FormatException('Invalid Apple Health subject.');
    }
    return value;
  }

  @override
  Future<bool> requestWellbeingPermission() async {
    _requireAppleMobile();
    final value = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('requestPermission', {
        'device_id': _deviceId,
        'source': AppleContextSource.health.name,
      }),
    );
    if (value.keys.toSet().difference({'granted'}).isNotEmpty ||
        value['granted'] is! bool) {
      throw const FormatException('Invalid Apple Health permission response.');
    }
    return value['granted']! as bool;
  }

  @override
  Future<Map<String, dynamic>> readWellbeing() async {
    _requireAppleMobile();
    final view = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>(
        'readWellbeing',
        appleNativeArguments(_deviceId),
      ),
    );
    validateAppleWellbeingView(view);
    return view;
  }

  @override
  Future<Map<String, dynamic>> screenTimeCapability() async {
    _requireAppleMobile();
    final value = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>(
        'screenTimeCapability',
        appleNativeArguments(_deviceId),
      ),
    );
    validateAppleScreenTimeCapability(value);
    return value;
  }

  void _requireAppleMobile() {
    if (!Platform.isIOS) {
      throw UnsupportedError(
        'Apple mobile context is available only on iOS and iPadOS.',
      );
    }
  }
}

@visibleForTesting
void validateAppleConnectionInventory(List<Map<String, dynamic>> connections) {
  if (connections.length != 3) {
    throw const FormatException('Invalid Apple connection inventory.');
  }
}

@visibleForTesting
void validateAppleDeviceId(String value) {
  if (value.isEmpty || value.length > 128 || value.contains(RegExp(r'\s'))) {
    throw ArgumentError.value(value, 'deviceId', 'Invalid local device ID.');
  }
}

@visibleForTesting
Map<String, Object?> appleNativeArguments(
  String deviceId, [
  Map<String, Object?> values = const {},
]) {
  validateAppleDeviceId(deviceId);
  return {'device_id': deviceId, ...values};
}

void validateApplePeopleView(Map<String, dynamic> view) {
  const keys = {
    'schema_version',
    'view_id',
    'source_handle',
    'observed_at_unix_ms',
    'expires_at_unix_ms',
    'coverage_complete',
    'identities',
  };
  _requireExactKeys(view, keys, 'Apple People View');
  final identities = view['identities'];
  if (view['schema_version'] != 1 ||
      view['view_id'] != 'people.identity' ||
      !_validHandle(view['source_handle']) ||
      view['coverage_complete'] is! bool ||
      identities is! List ||
      identities.length > 64) {
    throw const FormatException('Invalid Apple People View.');
  }
  _validateTimes(view, maximumTtlMs: 300000);
  final handles = <String>{};
  for (final raw in identities) {
    final identity = _strictMap(raw);
    const identityKeys = {
      'identity_handle',
      'display_name',
      'aliases',
      'confidence_millis',
      'evidence_handles',
    };
    _requireExactKeys(identity, identityKeys, 'Apple identity');
    final aliases = identity['aliases'];
    final evidence = identity['evidence_handles'];
    if (!_validHandle(identity['identity_handle']) ||
        !handles.add(identity['identity_handle']! as String) ||
        identity['display_name'] is! String ||
        (identity['display_name']! as String).isEmpty ||
        (identity['display_name']! as String).length > 256 ||
        aliases is! List ||
        aliases.length > 8 ||
        aliases.any((value) => value is! String || value.length > 256) ||
        _integer(identity['confidence_millis']) < 0 ||
        _integer(identity['confidence_millis']) > 1000 ||
        evidence is! List ||
        evidence.isEmpty ||
        evidence.length > 8 ||
        evidence.any((value) => !_validHandle(value))) {
      throw const FormatException('Invalid Apple identity.');
    }
  }
}

void validateAppleWellbeingView(Map<String, dynamic> view) {
  const keys = {
    'schema_version',
    'view_id',
    'source_handle',
    'observed_at_unix_ms',
    'expires_at_unix_ms',
    'capacity',
    'recovery',
    'confidence_millis',
    'evidence_handles',
  };
  _requireExactKeys(view, keys, 'Apple Wellbeing View');
  final evidence = view['evidence_handles'];
  if (view['schema_version'] != 1 ||
      view['view_id'] != 'wellbeing.derived' ||
      !_validHandle(view['source_handle']) ||
      !{'reduced', 'typical', 'strong', 'unknown'}.contains(view['capacity']) ||
      !{
        'needs_recovery',
        'typical',
        'recovered',
        'unknown',
      }.contains(view['recovery']) ||
      _integer(view['confidence_millis']) < 0 ||
      _integer(view['confidence_millis']) > 1000 ||
      evidence is! List ||
      evidence.isEmpty ||
      evidence.length > 3 ||
      evidence.any((value) => !_validHandle(value))) {
    throw const FormatException('Invalid Apple Wellbeing View.');
  }
  _validateTimes(view, maximumTtlMs: 1800000);
}

void validateAppleScreenTimeCapability(Map<String, dynamic> value) {
  const required = {
    'schema_version',
    'source_handle',
    'outcome',
    'authorization',
    'region_availability',
    'observed_at_unix_ms',
  };
  const optional = {'detail_code'};
  if (!value.keys.toSet().containsAll(required) ||
      value.keys.toSet().difference({...required, ...optional}).isNotEmpty ||
      value['schema_version'] != 1 ||
      value['source_handle'] != 'attention:apple-device-activity' ||
      !{
        'supported',
        'authorization_required',
        'authorization_denied',
        'entitlement_unavailable',
        'region_unavailable',
        'region_unknown',
        'unsupported_platform',
        'api_unavailable',
        'provider_error',
      }.contains(value['outcome']) ||
      !{
        'approved',
        'denied',
        'not_determined',
      }.contains(value['authorization']) ||
      !{
        'available',
        'unavailable',
        'not_required',
        'unknown',
      }.contains(value['region_availability']) ||
      _integer(value['observed_at_unix_ms']) < 0 ||
      value['outcome'] == 'supported' &&
          (value['authorization'] != 'approved' ||
              !{
                'available',
                'not_required',
              }.contains(value['region_availability'])) ||
      value['detail_code'] != null && !_validHandle(value['detail_code'])) {
    throw const FormatException('Invalid Apple Screen Time capability.');
  }
}

void _validateTimes(Map<String, dynamic> view, {required int maximumTtlMs}) {
  final observed = _integer(view['observed_at_unix_ms']);
  final expires = _integer(view['expires_at_unix_ms']);
  if (observed < 0 ||
      expires <= observed ||
      expires - observed > maximumTtlMs) {
    throw const FormatException('Invalid Apple View freshness envelope.');
  }
}

void _requireExactKeys(
  Map<String, dynamic> value,
  Set<String> keys,
  String name,
) {
  if (!value.keys.toSet().containsAll(keys) ||
      value.keys.toSet().difference(keys).isNotEmpty) {
    throw FormatException('Invalid $name keys.');
  }
}

Map<String, dynamic> _strictMap(Object? value) {
  if (value is! Map) {
    throw const FormatException('Expected an Apple provider map.');
  }
  return value.map((key, value) {
    if (key is! String) {
      throw const FormatException('Expected string map keys.');
    }
    return MapEntry(key, value);
  });
}

int _integer(Object? value) {
  if (value is! int) throw const FormatException('Expected an integer.');
  return value;
}

bool _validHandle(Object? value, {int maximum = 128}) =>
    value is String && value.trim().isNotEmpty && value.length <= maximum;
