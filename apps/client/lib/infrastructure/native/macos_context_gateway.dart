import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

const _channel = MethodChannel('floe/macos_context');

final class MacOSContextGateway {
  Future<Map<String, dynamic>> readAttention() async {
    if (!Platform.isMacOS) {
      throw UnsupportedError('macOS context is available only on macOS.');
    }
    final view = _strictMap(
      await _channel.invokeMapMethod<Object?, Object?>('readAttention'),
    );
    validateMacOSAttentionView(view);
    return view;
  }
}

@visibleForTesting
void validateMacOSAttentionView(Map<String, dynamic> view) {
  const keys = {
    'schema_version',
    'view_id',
    'source_handle',
    'observed_at_unix_ms',
    'expires_at_unix_ms',
    'state',
    'confidence_millis',
    'evidence_handles',
  };
  if (!view.keys.toSet().containsAll(keys) ||
      view.keys.toSet().difference(keys).isNotEmpty ||
      view['schema_version'] != 1 ||
      view['view_id'] != 'attention.coarse' ||
      view['source_handle'] != 'attention:macos_local' ||
      view['evidence_handles'] is! List) {
    throw const FormatException('Invalid macOS Attention View.');
  }
  final observed = _integer(view['observed_at_unix_ms']);
  final expires = _integer(view['expires_at_unix_ms']);
  final confidence = _integer(view['confidence_millis']);
  final state = view['state'];
  final evidence = view['evidence_handles']! as List<Object?>;
  const states = {
    'available',
    'focused',
    'high_interruption_pressure',
    'unknown',
  };
  if (observed < 0 ||
      expires <= observed ||
      expires - observed > 300000 ||
      !states.contains(state) ||
      confidence < 0 ||
      confidence > 1000 ||
      (state == 'unknown') != (confidence == 0) ||
      evidence.length > 16 ||
      (state != 'unknown' && evidence.isEmpty) ||
      evidence.any((value) => !_validHandle(value))) {
    throw const FormatException('Invalid macOS Attention View envelope.');
  }
}

Map<String, dynamic> _strictMap(Object? value) {
  if (value is! Map) {
    throw const FormatException('Expected a map from the macOS provider.');
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

bool _validHandle(Object? value) =>
    value is String && value.trim().isNotEmpty && value.length <= 128;
