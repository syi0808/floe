import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

const _channel = MethodChannel('floe/apple_context');

abstract interface class AppleContextApi {
  Future<List<Map<String, dynamic>>> connections();
  Future<bool> requestPermission(AppleContextSource source);
  Future<Map<String, dynamic>> readContacts({int limit = 64});
  Future<Map<String, dynamic>> readFeasibility(AppleFeasibilityQuery query);
  Future<Map<String, dynamic>> readWellbeing();
  Future<Map<String, dynamic>> screenTimeCapability();
}

enum AppleContextSource { contacts, health }

final class AppleFeasibilityQuery {
  const AppleFeasibilityQuery({
    required this.eventHandle,
    required this.evidenceHandles,
    required this.latitude,
    required this.longitude,
    required this.eventStart,
    required this.eventEnd,
    required this.travelMode,
    this.sourceHandle = 'feasibility:apple',
    this.timeout = const Duration(seconds: 20),
  });

  final String eventHandle;
  final List<String> evidenceHandles;
  final double latitude;
  final double longitude;
  final DateTime eventStart;
  final DateTime eventEnd;
  final AppleTravelMode travelMode;
  final String sourceHandle;
  final Duration timeout;
}

enum AppleTravelMode { automobile, transit, walking }

final class AppleContextGateway implements AppleContextApi {
  @override
  Future<List<Map<String, dynamic>>> connections() async {
    _requireAppleMobile();
    final values = await _channel.invokeListMethod<Object?>('connections');
    final connections = (values ?? const <Object?>[])
        .map(_strictMap)
        .toList(growable: false);
    if (connections.length != 4) {
      throw const FormatException('Invalid Apple connection inventory.');
    }
    return connections;
  }

  @override
  Future<bool> requestPermission(AppleContextSource source) async {
    _requireAppleMobile();
    final value = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('requestPermission', {
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
  Future<Map<String, dynamic>> readContacts({int limit = 64}) async {
    _requireAppleMobile();
    final view = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('readContacts', {
        'limit': limit,
      }),
    );
    validateApplePeopleView(view);
    return view;
  }

  @override
  Future<Map<String, dynamic>> readFeasibility(
    AppleFeasibilityQuery query,
  ) async {
    _requireAppleMobile();
    final view = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('readFeasibility', {
        'event_handle': query.eventHandle,
        'evidence_handles': query.evidenceHandles,
        'destination_latitude': query.latitude,
        'destination_longitude': query.longitude,
        'event_start_unix_ms': query.eventStart.toUtc().millisecondsSinceEpoch,
        'event_end_unix_ms': query.eventEnd.toUtc().millisecondsSinceEpoch,
        'travel_mode': query.travelMode.name,
        'source_handle': query.sourceHandle,
        'timeout_ms': query.timeout.inMilliseconds,
      }),
    );
    validateAppleFeasibilityResult(view);
    return view;
  }

  @override
  Future<Map<String, dynamic>> readWellbeing() async {
    _requireAppleMobile();
    final view = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('readWellbeing'),
    );
    validateAppleWellbeingView(view);
    return view;
  }

  @override
  Future<Map<String, dynamic>> screenTimeCapability() async {
    _requireAppleMobile();
    final value = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('screenTimeCapability'),
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

@visibleForTesting
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

@visibleForTesting
void validateAppleFeasibilityResult(Map<String, dynamic> result) {
  _requireExactKeys(result, {
    'view',
    'weather_attribution',
  }, 'Apple feasibility result');
  final view = _strictMap(result['view']);
  const keys = {
    'schema_version',
    'view_id',
    'source_handle',
    'observed_at_unix_ms',
    'expires_at_unix_ms',
    'items',
  };
  _requireExactKeys(view, keys, 'Apple Feasibility View');
  final items = view['items'];
  if (view['schema_version'] != 1 ||
      view['view_id'] != 'schedule.feasibility' ||
      !_validHandle(view['source_handle'], maximum: 512) ||
      items is! List ||
      items.length != 1) {
    throw const FormatException('Invalid Apple Feasibility View.');
  }
  _validateTimes(view, maximumTtlMs: 300000);
  final item = _strictMap(items.single);
  const itemKeys = {
    'event_handle',
    'evidence_handles',
    'travel_duration_seconds',
    'leave_by_unix_ms',
    'weather_impact',
    'confidence_millis',
  };
  _requireExactKeys(item, itemKeys, 'Apple feasibility item');
  final evidence = item['evidence_handles'];
  if (!_validHandle(item['event_handle'], maximum: 512) ||
      evidence is! List ||
      evidence.isEmpty ||
      evidence.length > 8 ||
      evidence.any((value) => !_validHandle(value, maximum: 512)) ||
      _integer(item['travel_duration_seconds']) < 0 ||
      _integer(item['travel_duration_seconds']) > 86400 ||
      _integer(item['leave_by_unix_ms']) < 0 ||
      !{
        'none',
        'minor',
        'significant',
        'unknown',
      }.contains(item['weather_impact']) ||
      _integer(item['confidence_millis']) < 0 ||
      _integer(item['confidence_millis']) > 1000) {
    throw const FormatException('Invalid Apple feasibility item.');
  }
  final attribution = _strictMap(result['weather_attribution']);
  const attributionKeys = {
    'legal_page_url',
    'combined_mark_light_url',
    'combined_mark_dark_url',
  };
  _requireExactKeys(attribution, attributionKeys, 'Weather attribution');
  if (attribution.values.any(
    (value) => value is! String || Uri.tryParse(value)?.hasScheme != true,
  )) {
    throw const FormatException('Invalid Weather attribution.');
  }
}

@visibleForTesting
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
              !{'available', 'not_required'}.contains(
                value['region_availability'],
              )) ||
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
