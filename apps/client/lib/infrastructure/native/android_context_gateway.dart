import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/foundation.dart';

const _channel = MethodChannel('floe/android_context');

final class AndroidContextGateway {
  AndroidContextGateway({required List<String> calendarIds})
    : _calendarIds = List.unmodifiable(calendarIds);

  final List<String> _calendarIds;

  Future<List<Map<String, dynamic>>> connections() async {
    _requireAndroid();
    final values = await _channel.invokeListMethod<Object?>('connections');
    return (values ?? const <Object?>[])
        .map(_strictMap)
        .toList(growable: false);
  }

  Future<bool> requestPermission(AndroidContextSource source) async {
    _requireAndroid();
    final value = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('requestPermission', {
        'source': source.name,
      }),
    );
    if (value.keys.toSet().difference({'granted'}).isNotEmpty ||
        value['granted'] is! bool) {
      throw const FormatException('Invalid Android permission response.');
    }
    return value['granted']! as bool;
  }

  Future<Map<String, dynamic>> readCalendar({
    required DateTime rangeStart,
    required DateTime rangeEnd,
    String cursor = '',
    int limit = 128,
  }) async {
    _requireAndroid();
    final view = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('readCalendar', {
        'range_start_unix_ms': rangeStart.toUtc().millisecondsSinceEpoch,
        'range_end_unix_ms': rangeEnd.toUtc().millisecondsSinceEpoch,
        'calendar_ids': _calendarIds,
        'cursor': cursor,
        'limit': limit,
      }),
    );
    validateAndroidCalendarView(view);
    return view;
  }

  Future<Map<String, dynamic>> readContacts({int limit = 64}) async {
    _requireAndroid();
    final view = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('readContacts', {
        'limit': limit,
      }),
    );
    validateAndroidPeopleView(view);
    return view;
  }

  void _requireAndroid() {
    if (!Platform.isAndroid) {
      throw UnsupportedError('Android context is available only on Android.');
    }
  }
}

enum AndroidContextSource { calendar, contacts }

@visibleForTesting
void validateAndroidCalendarView(Map<String, dynamic> view) {
  const required = {
    'schema_version',
    'view_id',
    'source_handle',
    'observed_at_unix_ms',
    'expires_at_unix_ms',
    'range_start_unix_ms',
    'range_end_unix_ms',
    'coverage_complete',
    'items',
  };
  const optional = {'next_cursor'};
  if (!view.keys.toSet().containsAll(required) ||
      view.keys.toSet().difference({...required, ...optional}).isNotEmpty ||
      view['schema_version'] != 1 ||
      view['view_id'] != 'calendar.timeline' ||
      !_validHandle(view['source_handle']) ||
      view['items'] is! List) {
    throw const FormatException('Invalid Android Calendar View.');
  }
  final observed = _integer(view['observed_at_unix_ms']);
  final expires = _integer(view['expires_at_unix_ms']);
  final start = _integer(view['range_start_unix_ms']);
  final end = _integer(view['range_end_unix_ms']);
  final complete = view['coverage_complete'];
  final next = view['next_cursor'];
  final items = view['items']! as List<Object?>;
  if (observed < 0 ||
      expires <= observed ||
      expires - observed > 300000 ||
      start < 0 ||
      end <= start ||
      end - start > 32 * 86400000 ||
      complete is! bool ||
      complete == (next != null) ||
      next != null && (next is! String || next.isEmpty || next.length > 2048) ||
      items.length > 128) {
    throw const FormatException('Invalid Android Calendar View envelope.');
  }
  final handles = <String>{};
  for (final raw in items) {
    final item = _strictMap(raw);
    const keys = {
      'evidence_handle',
      'untrusted_title',
      'starts_at_unix_ms',
      'ends_at_unix_ms',
      'all_day',
    };
    final itemStart = _integer(item['starts_at_unix_ms']);
    final itemEnd = _integer(item['ends_at_unix_ms']);
    if (item.keys.toSet().difference(keys).isNotEmpty ||
        !item.keys.toSet().containsAll(keys) ||
        !_validHandle(item['evidence_handle']) ||
        !handles.add(item['evidence_handle']! as String) ||
        item['untrusted_title'] is! String ||
        (item['untrusted_title']! as String).length > 1024 ||
        itemStart < 0 ||
        itemEnd <= itemStart ||
        itemStart >= end ||
        itemEnd <= start ||
        item['all_day'] is! bool) {
      throw const FormatException('Invalid Android Calendar item.');
    }
  }
}

@visibleForTesting
void validateAndroidPeopleView(Map<String, dynamic> view) {
  const keys = {
    'schema_version',
    'view_id',
    'source_handle',
    'observed_at_unix_ms',
    'expires_at_unix_ms',
    'coverage_complete',
    'identities',
  };
  if (view.keys.toSet().difference(keys).isNotEmpty ||
      !view.keys.toSet().containsAll(keys) ||
      view['schema_version'] != 1 ||
      view['view_id'] != 'people.identity' ||
      !_validHandle(view['source_handle']) ||
      view['coverage_complete'] is! bool ||
      view['identities'] is! List) {
    throw const FormatException('Invalid Android People View.');
  }
  final observed = _integer(view['observed_at_unix_ms']);
  final expires = _integer(view['expires_at_unix_ms']);
  final identities = view['identities']! as List<Object?>;
  if (observed < 0 ||
      expires <= observed ||
      expires - observed > 300000 ||
      identities.length > 64) {
    throw const FormatException('Invalid Android People View envelope.');
  }
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
    final evidence = identity['evidence_handles'];
    final confidence = _integer(identity['confidence_millis']);
    if (identity.keys.toSet().difference(identityKeys).isNotEmpty ||
        !identity.keys.toSet().containsAll(identityKeys) ||
        !_validHandle(identity['identity_handle']) ||
        !handles.add(identity['identity_handle']! as String) ||
        identity['display_name'] is! String ||
        (identity['display_name']! as String).trim().isEmpty ||
        (identity['display_name']! as String).length > 256 ||
        identity['aliases'] is! List ||
        (identity['aliases']! as List).length > 8 ||
        confidence <= 0 ||
        confidence > 1000 ||
        evidence is! List ||
        evidence.isEmpty ||
        evidence.length > 16 ||
        evidence.any((value) => !_validHandle(value))) {
      throw const FormatException('Invalid Android People identity.');
    }
  }
}

Map<String, dynamic> _strictMap(Object? value) {
  if (value is! Map) throw const FormatException('Expected a map.');
  return value.map((key, item) {
    if (key is! String) throw const FormatException('Expected string keys.');
    return MapEntry(key, item);
  });
}

int _integer(Object? value) {
  if (value is! int) throw const FormatException('Expected an integer.');
  return value;
}

bool _validHandle(Object? value) =>
    value is String && value.trim().isNotEmpty && value.length <= 128;
